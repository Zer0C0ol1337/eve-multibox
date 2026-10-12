//! Engine-Thread unter Linux: besitzt die X11-Verbindung, Hotkeys, Vorschaufenster und
//! das Tray-Symbol und führt die Befehle der Oberfläche aus – mit derselben API wie die
//! Windows-Engine (`EngineHandle`, `Command`, `Event`, `Snapshot`).
//!
//! Ablauf: Eine Schleife wartet per `poll` gleichzeitig auf X-Ereignisse und auf einen
//! Weckruf (eventfd) von Oberfläche, Tray oder zweiter Instanz. Alle 500 ms werden
//! Clients gesucht und gemessen, die Vorschauen etwa 30-mal pro Sekunde aufgefrischt.
//!
//! Wichtig (EVE-EULA): Die Engine sendet niemals Tastatur- oder Mauseingaben an
//! EVE-Clients. Sie verschiebt nur Fenster, holt eines in den Vordergrund und ändert
//! Prozess-Einstellungen – ein Tastendruck löst genau eine Aktion aus.

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use evemb::config::{Config, Priority, Rect, cores_to_mask, mask_to_cores};
use evemb::i18n::{t, tf};
use evemb::{hotkey, layout};
use x11rb::connection::Connection;
use x11rb::protocol::Event as XEvent;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _, EventMask, Window};

pub use crate::engine_api::{ClientInfo, Command, Event, Snapshot};
use crate::engine_api::{Hwnd, MemInfo};

use super::keys::{GrabError, KeyGrabber};
use super::preview::{PreviewEvent, PreviewItem, PreviewManager};
use super::procstat::{self, CpuMeter, Proc, SystemCpu};
use super::signal::{self, Signal};
use super::tray::Tray;
use super::x11::X;

/// Aktualisierungsintervall für Clients und Messwerte.
const TICK: Duration = Duration::from_millis(500);
/// Bildrate der Vorschauen.
const PAINT: Duration = Duration::from_millis(33);
/// Bytes pro Megabyte.
const MB: u64 = 1024 * 1024;

/// Was ein Hotkey auslöst.
#[derive(Clone, Debug)]
enum HotkeyAction {
    Next,
    Prev,
    Swap,
    Layout,
    TogglePreviews,
    Character(String),
}

/// Ein verwalteter EVE-Client.
struct Client {
    hwnd: Hwnd,
    pid: u32,
    character: Option<String>,
    proc_: Option<Proc>,
    meter: CpuMeter,
    cpu: f32,
    mem: MemInfo,
    system_mask: u64,
    original_priority: Option<Priority>,
    original_mask: Option<u64>,
    applied_priority: Option<Priority>,
    applied_mask: Option<u64>,
    throttled: bool,
    limited: bool,
}

impl Client {
    /// Übernimmt ein gefundenes Fenster und merkt sich die Original-Einstellungen.
    fn new(hwnd: Hwnd, pid: u32, character: Option<String>) -> Self {
        let proc_ = (pid > 0).then(|| Proc::open(pid)).flatten();
        let original_priority = proc_.as_ref().and_then(|p| p.priority());
        let (original_mask, system_mask) = match proc_.as_ref().and_then(|p| p.affinity()) {
            Some((pm, sm)) => (Some(pm), sm),
            None => (None, u64::MAX),
        };
        Self {
            hwnd,
            pid,
            character,
            proc_,
            meter: CpuMeter::default(),
            cpu: 0.0,
            mem: MemInfo::default(),
            system_mask,
            original_priority,
            original_mask,
            applied_priority: original_priority,
            applied_mask: original_mask,
            throttled: false,
            limited: false,
        }
    }

    /// Anzeigename bzw. Schlüssel für Vorschau-Positionen.
    fn display_name(&self) -> String {
        self.character
            .clone()
            .unwrap_or_else(|| format!("{} ({})", t("status.login"), self.pid))
    }

    /// Setzt Priorität und Kerne auf die Originalwerte zurück.
    fn restore(&mut self, can_prio: bool) {
        let Some(ph) = &self.proc_ else { return };
        if can_prio && let Some(p) = self.original_priority.filter(|&p| Some(p) != self.applied_priority) {
            ph.set_priority(p);
            self.applied_priority = Some(p);
        }
        if let Some(m) = self.original_mask.filter(|&m| Some(m) != self.applied_mask) {
            ph.set_affinity(m);
            self.applied_mask = Some(m);
        }
    }
}

/// Wandelt eine X-Fenster-ID in die gemeinsame Kennung um.
fn h(w: Window) -> Hwnd {
    w as Hwnd
}

/// Wandelt die gemeinsame Kennung zurück in eine X-Fenster-ID.
fn xw(h: Hwnd) -> Window {
    h as Window
}

/// Position und Größe eines Vorschaufensters: ((x, y), (Breite, Höhe)).
type PreviewPlacement = ((i32, i32), (i32, i32));

/// Zustand der Engine (lebt ausschließlich im Engine-Thread).
struct Engine {
    x: X,
    cfg: Config,
    rx: Receiver<Command>,
    events: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    repaint: Box<dyn Fn() + Send>,
    clients: Vec<Client>,
    main_hwnd: Option<Hwnd>,
    prev_main: Option<Hwnd>,
    last_fg_eve: Option<Hwnd>,
    previews: PreviewManager,
    previews_on: bool,
    /// Vorschau-Position und -Größe von Login-Fenstern – nur bis Programmende gemerkt,
    /// weil sich deren Kennung (Prozess-ID) bei jedem Start ändert.
    login_previews: HashMap<String, PreviewPlacement>,
    keys: KeyGrabber,
    hotkeys: HashMap<i32, HotkeyAction>,
    hotkeys_active: bool,
    hotkeys_suspended: bool,
    hotkey_signature: Vec<String>,
    hotkey_errors: Vec<String>,
    sys_cpu: SystemCpu,
    sys_cpu_pct: f32,
    cores: u32,
    last_trim: Instant,
    seq: u64,
    tray: Option<Tray>,
    /// Darf die Priorität geändert und wieder zurückgesetzt werden?
    can_prio: bool,
    /// Darf Speicher ausgelagert werden (CAP_SYS_NICE)?
    can_trim: bool,
    last_fg: Hwnd,
    quit: bool,
}

impl Engine {
    /// Schickt eine Statusmeldung an die Oberfläche.
    fn message(&self, text: impl Into<String>) {
        let _ = self.events.send(Event::Message(text.into()));
        (self.repaint)();
    }

    /// Aktuell aktives Fenster als gemeinsame Kennung.
    fn foreground(&self) -> Hwnd {
        h(self.x.active_window())
    }

    /// Index eines Clients anhand seines Fensters.
    fn index_of(&self, w: Hwnd) -> Option<usize> {
        self.clients.iter().position(|c| c.hwnd == w)
    }

    /// Charakternamen aller Clients (für die Layout-Logik).
    fn chars(&self) -> Vec<Option<&str>> {
        self.clients.iter().map(|c| c.character.as_deref()).collect()
    }

    /// Wechsel-Reihenfolge der Clients (Indizes).
    fn order(&self) -> Vec<usize> {
        layout::cycle_order(&self.chars(), self.cfg.active_profile())
    }

    // ---------------------------------------------------------------- Ablauf

    /// Regelmäßige Aktualisierung: Clients suchen, messen, Regeln anwenden, veröffentlichen.
    fn tick(&mut self) {
        self.x.refresh_wm_info();
        let new_client = self.refresh_clients();
        self.sample_stats();
        if new_client && self.cfg.active_profile().auto_apply {
            self.apply_layout();
        }
        self.foreground_policy();
        self.auto_trim();
        self.publish();
    }

    /// Gleicht die Client-Liste mit den offenen EVE-Fenstern ab.
    /// Liefert `true`, wenn ein Client neu ist oder sich ein Charakter eingeloggt hat.
    fn refresh_clients(&mut self) -> bool {
        let mut old: HashMap<Hwnd, Client> = self.clients.drain(..).map(|c| (c.hwnd, c)).collect();
        let mut changed = false;
        for w in self.x.client_windows() {
            if self.previews.owns(w) {
                continue;
            }
            let title = self.x.title(w);
            if !self.x.is_eve_window(w, &title) {
                continue;
            }
            let character = evemb::layout::character_from_title(&title);
            match old.remove(&h(w)) {
                Some(mut c) => {
                    if character.is_some() && c.character != character {
                        changed = true;
                    }
                    c.character = character;
                    self.clients.push(c);
                }
                None => {
                    changed = true;
                    let pid = self.x.pid(w).unwrap_or(0);
                    // Ereignisse des Clients mitbekommen (Titeländerung beim Login).
                    let aux = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
                    let _ = self.x.conn.change_window_attributes(w, &aux);
                    self.clients.push(Client::new(h(w), pid, character));
                }
            }
        }
        let alive = |v: &Option<Hwnd>, list: &[Client]| v.filter(|x| list.iter().any(|c| c.hwnd == *x));
        self.main_hwnd = alive(&self.main_hwnd, &self.clients);
        self.prev_main = alive(&self.prev_main, &self.clients);
        self.last_fg_eve = alive(&self.last_fg_eve, &self.clients);
        changed
    }

    /// Misst CPU und Speicher aller Clients sowie die System-CPU.
    fn sample_stats(&mut self) {
        self.sys_cpu_pct = self.sys_cpu.sample();
        let cores = self.cores;
        for c in &mut self.clients {
            let Some(ph) = &c.proc_ else { continue };
            if let Some(t) = ph.cpu_time() {
                c.cpu = c.meter.sample(t, cores);
            }
            if let Some(m) = ph.memory() {
                c.mem = m;
            }
        }
    }

    /// Reaktion auf (mögliche) Vordergrundwechsel: Hotkeys, Drosseln, Minimieren, Vorschauen.
    fn foreground_policy(&mut self) {
        let fg = self.foreground();
        self.last_fg = fg;
        self.sync_hotkeys(fg);
        self.apply_performance(fg);
        if self.index_of(fg).is_some() {
            if self.cfg.performance.minimize_inactive
                && let Some(prev) = self.last_fg_eve.filter(|&p| p != fg && self.index_of(p).is_some())
            {
                self.x.minimize(xw(prev));
            }
            self.last_fg_eve = Some(fg);
        }
        self.update_previews(fg);
    }

    /// Setzt Priorität und Kern-Affinität aller Clients gemäß Regeln und Drosselung.
    /// Ohne CAP_SYS_NICE wird die Priorität nicht angefasst (sie ließe sich nicht zurücksetzen).
    fn apply_performance(&mut self, fg: Hwnd) {
        let perf = &self.cfg.performance;
        let profile = self.cfg.active_profile();
        for c in &mut self.clients {
            let Some(ph) = &c.proc_ else { continue };
            let rule = c.character.as_deref().and_then(|n| profile.rule(n));
            let throttle = perf.throttle_inactive && c.hwnd != fg;

            if self.can_prio {
                let want_prio = if throttle {
                    Some(perf.inactive_priority)
                } else if let Some(p) = rule.and_then(|r| r.priority) {
                    Some(p)
                } else if perf.throttle_inactive {
                    Some(perf.active_priority)
                } else {
                    None
                };
                if let Some(target) = want_prio.or(c.original_priority)
                    && c.applied_priority != Some(target)
                {
                    if !ph.set_priority(target) {
                        c.limited = true;
                    }
                    c.applied_priority = Some(target);
                }
            }

            let rule_mask = rule.map(|r| cores_to_mask(&r.cores)).filter(|&m| m != 0);
            let bg_mask = Some(cores_to_mask(&perf.background_cores)).filter(|&m| throttle && m != 0);
            let want_mask = bg_mask.or(rule_mask).map(|m| m & c.system_mask).filter(|&m| m != 0);
            if let Some(target) = want_mask.or(c.original_mask)
                && c.applied_mask != Some(target)
            {
                if !ph.set_affinity(target) {
                    c.limited = true;
                }
                c.applied_mask = Some(target);
            }
            c.throttled = throttle;
        }
    }

    /// Aktualisiert alle Vorschaufenster.
    fn update_previews(&mut self, fg: Hwnd) {
        // Login-Fenster, die in dieser Sitzung verschoben wurden, behalten ihre Position.
        let merged;
        let s = if self.login_previews.is_empty() {
            &self.cfg.preview
        } else {
            let mut p = self.cfg.preview.clone();
            for (key, (pos, size)) in &self.login_previews {
                p.positions.insert(key.clone(), *pos);
                p.sizes.insert(key.clone(), *size);
            }
            merged = p;
            &merged
        };
        let eve_focused = self.index_of(fg).is_some();
        let show = s.enabled && self.previews_on && (!s.only_when_eve_focused || eve_focused);
        let items: Vec<PreviewItem> = self
            .order()
            .into_iter()
            .map(|i| {
                let c = &self.clients[i];
                let active = c.hwnd == fg;
                PreviewItem {
                    source: c.hwnd,
                    key: c.display_name(),
                    label: c.display_name(),
                    active,
                    main: Some(c.hwnd) == self.main_hwnd,
                    visible: show && !(s.hide_active && active),
                    minimized: self.x.is_minimized(xw(c.hwnd)),
                }
            })
            .collect();
        self.previews.sync(&self.x, &items, s);
        let _ = self.x.conn.flush();
    }

    /// Gibt regelmäßig den Speicher der Hintergrund-Clients frei (falls aktiviert).
    fn auto_trim(&mut self) {
        let perf = &self.cfg.performance;
        let interval = Duration::from_secs(perf.auto_trim_minutes as u64 * 60);
        if perf.auto_trim && self.last_trim.elapsed() >= interval {
            self.last_trim = Instant::now();
            self.trim_background();
        }
    }

    /// Schreibt den aktuellen Zustand in den geteilten Snapshot und weckt die Oberfläche.
    fn publish(&mut self) {
        let fg = self.last_fg;
        let perf = &self.cfg.performance;
        let (ram_total, ram_avail) = procstat::system_memory();
        let mut warnings = Vec::new();

        let clients: Vec<ClientInfo> = self
            .order()
            .into_iter()
            .map(|i| {
                let c = &self.clients[i];
                let ws_mb = c.mem.working_set / MB;
                if perf.ram_warn_mb > 0 && ws_mb > perf.ram_warn_mb {
                    warnings.push(tf(
                        "eng.warn_ram",
                        &[
                            &c.display_name(),
                            &format!("{:.1}", ws_mb as f64 / 1024.0),
                            &format!("{:.1}", perf.ram_warn_mb as f64 / 1024.0),
                        ],
                    ));
                }
                if c.limited {
                    warnings.push(tf("eng.warn_rights_linux", &[&c.display_name()]));
                }
                let mask = c.applied_mask.unwrap_or(c.system_mask);
                ClientInfo {
                    hwnd: c.hwnd,
                    pid: c.pid,
                    name: c.display_name(),
                    logged_in: c.character.is_some(),
                    cpu: c.cpu,
                    mem: c.mem,
                    priority: c.applied_priority,
                    cores: if mask & c.system_mask == c.system_mask {
                        Vec::new()
                    } else {
                        mask_to_cores(mask)
                    },
                    foreground: c.hwnd == fg,
                    main: Some(c.hwnd) == self.main_hwnd,
                    minimized: self.x.is_minimized(xw(c.hwnd)),
                    throttled: c.throttled,
                    limited: c.limited,
                }
            })
            .collect();

        if ram_total > 0 && ram_avail * 100 / ram_total < perf.system_ram_warn_percent as u64 {
            warnings.push(tf(
                "eng.warn_sysram",
                &[
                    &format!("{:.1}", ram_avail as f64 / (1024.0 * MB as f64)),
                    &format!("{:.1}", ram_total as f64 / (1024.0 * MB as f64)),
                ],
            ));
        }
        if self.cfg.preview.enabled && !self.previews.supported {
            warnings.push(t("eng.no_composite").to_string());
        }

        if let Some(tray) = &mut self.tray {
            tray.set_clients(self.clients.len());
        }
        self.seq += 1;
        let snap = Snapshot {
            seq: self.seq,
            clients,
            sys_cpu: self.sys_cpu_pct,
            ram_total,
            ram_avail,
            cores: self.cores,
            warnings,
            hotkey_errors: self.hotkey_errors.clone(),
            previews_on: self.previews_on,
        };
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = snap;
        (self.repaint)();
    }

    // ---------------------------------------------------------------- Aktionen

    /// Bestimmt den Haupt-Client: gemerkt → Profil-Vorgabe → Vordergrund → Erster.
    fn resolve_main(&mut self) -> Option<usize> {
        if let Some(i) = self.main_hwnd.and_then(|w| self.index_of(w)) {
            return Some(i);
        }
        let default_main = self.cfg.active_profile().default_main.clone();
        let by_name = (!default_main.is_empty())
            .then(|| {
                self.clients.iter().position(|c| {
                    c.character
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(&default_main))
                })
            })
            .flatten();
        let idx = by_name
            .or_else(|| self.index_of(self.foreground()))
            .or_else(|| self.order().first().copied())?;
        self.main_hwnd = Some(self.clients[idx].hwnd);
        Some(idx)
    }

    /// Ordnet alle Clients gemäß aktivem Profil an.
    fn apply_layout(&mut self) {
        let main = self.resolve_main();
        let profile = self.cfg.active_profile();
        let plan = layout::plan_layout(&self.chars(), profile, main);
        let borderless = profile.borderless;
        let targets: Vec<(Hwnd, Rect)> = plan.iter().map(|p| (self.clients[p.index].hwnd, p.rect)).collect();
        for (w, r) in targets {
            self.x.place_window(xw(w), r, borderless);
        }
    }

    /// Macht einen Client zum Haupt-Client und wendet das Layout an.
    fn make_main(&mut self, w: Hwnd) {
        if self.index_of(w).is_none() {
            return;
        }
        self.resolve_main();
        if self.main_hwnd != Some(w) {
            self.prev_main = self.main_hwnd;
            self.main_hwnd = Some(w);
        }
        self.apply_layout();
    }

    /// Holt einen Client in den Vordergrund (optional mit Tausch ins Hauptfenster).
    fn activate_client(&mut self, w: Hwnd) {
        if self.index_of(w).is_none() {
            return;
        }
        if self.cfg.active_profile().swap_on_activate {
            self.make_main(w);
        }
        self.x.activate(xw(w));
        self.foreground_policy();
        self.publish();
    }

    /// Hotkey „Tauschen“: Vordergrund-Client ins Hauptfenster; ist er es schon,
    /// kommt der vorherige Haupt-Client zurück.
    fn swap_hotkey(&mut self) {
        let fg = self.foreground();
        if self.index_of(fg).is_none() {
            return;
        }
        if Some(fg) == self.main_hwnd {
            if let Some(prev) = self.prev_main.filter(|&p| self.index_of(p).is_some()) {
                self.make_main(prev);
                self.x.activate(xw(prev));
            }
        } else {
            self.make_main(fg);
        }
        self.foreground_policy();
        self.publish();
    }

    /// Schaltet zum nächsten/vorherigen Client.
    fn cycle(&mut self, forward: bool) {
        let order = self.order();
        let current = self.index_of(self.foreground());
        if let Some(next) = layout::cycle_step(&order, current, forward) {
            self.activate_client(self.clients[next].hwnd);
        }
    }

    /// Lagert Speicher der ausgewählten Clients aus und meldet das Ergebnis.
    fn trim_where(&mut self, filter: impl Fn(&Client) -> bool) {
        if !self.can_trim {
            self.message(t("eng.trim_unavailable"));
            return;
        }
        let mut count = 0;
        let mut freed = 0u64;
        for c in self.clients.iter_mut().filter(|c| filter(c)) {
            let Some(ph) = &c.proc_ else { continue };
            let before = ph.memory().map_or(0, |m| m.working_set);
            if ph.trim() {
                count += 1;
                let after = ph.memory().map_or(before, |m| m.working_set);
                freed += before.saturating_sub(after);
                c.mem.working_set = after;
            }
        }
        if count > 0 {
            self.message(tf("eng.trimmed", &[&count, &(freed / MB)]));
        }
    }

    /// Speicher aller Clients außer dem Vordergrund-Client auslagern.
    fn trim_background(&mut self) {
        let fg = self.foreground();
        self.trim_where(|c| c.hwnd != fg);
    }

    /// Erfasst die aktuellen Fensterpositionen (Haupt zuerst) und schickt sie an die Oberfläche.
    fn capture_layout(&mut self) {
        let Some(main) = self.resolve_main() else {
            self.message(t("eng.no_clients_capture"));
            return;
        };
        let Some(main_rect) = self.x.visible_rect(xw(self.clients[main].hwnd)) else {
            return;
        };
        let mut secondaries = Vec::new();
        let mut characters = vec![self.clients[main].character.clone()];
        for i in self.order().into_iter().filter(|&i| i != main) {
            if let Some(r) = self.x.visible_rect(xw(self.clients[i].hwnd)) {
                secondaries.push(r);
                characters.push(self.clients[i].character.clone());
            }
        }
        let _ = self.events.send(Event::LayoutCaptured {
            main: main_rect,
            secondaries,
            characters,
        });
        (self.repaint)();
    }

    // ---------------------------------------------------------------- Hotkeys

    /// Alle Hotkey-Texte, deren Änderung eine Neuregistrierung erfordert.
    fn current_hotkey_signature(&self) -> Vec<String> {
        let hk = &self.cfg.hotkeys;
        let mut sig = vec![
            hk.cycle_next.clone(),
            hk.cycle_prev.clone(),
            hk.swap_main.clone(),
            hk.apply_layout.clone(),
            hk.toggle_previews.clone(),
        ];
        for r in &self.cfg.active_profile().clients {
            sig.push(format!("{}={}", r.character, r.hotkey));
        }
        sig
    }

    /// Sollen die Hotkeys gerade aktiv sein? (Nur-in-EVE-Option beachten.)
    fn hotkeys_wanted(&self, fg: Hwnd) -> bool {
        !self.hotkeys_suspended
            && (!self.cfg.hotkeys.only_when_eve_focused || self.index_of(fg).is_some() || self.x.is_own_window(xw(fg)))
    }

    /// Schaltet die Hotkeys passend zum Vordergrund an/aus und registriert sie
    /// nach Konfigurationsänderungen neu.
    fn sync_hotkeys(&mut self, fg: Hwnd) {
        let want = self.hotkeys_wanted(fg);
        if want && (!self.hotkeys_active || self.current_hotkey_signature() != self.hotkey_signature) {
            self.register_hotkeys();
        } else if !want && self.hotkeys_active {
            self.unregister_hotkeys();
        }
    }

    /// Gibt alle gegriffenen Tasten wieder frei (andere Programme bekommen die Tasten zurück).
    fn unregister_hotkeys(&mut self) {
        self.keys.ungrab_all(&self.x.conn, self.x.root);
        self.hotkeys.clear();
        self.hotkeys_active = false;
        let _ = self.x.conn.flush();
    }

    /// Registriert alle globalen und Charakter-Hotkeys neu und sammelt Fehler.
    fn register_hotkeys(&mut self) {
        self.unregister_hotkeys();
        self.hotkeys_active = true;
        self.hotkey_errors.clear();
        self.hotkey_signature = self.current_hotkey_signature();

        let hk = &self.cfg.hotkeys;
        let mut list: Vec<(String, String, HotkeyAction)> = vec![
            (t("hk.next").into(), hk.cycle_next.clone(), HotkeyAction::Next),
            (t("hk.prev").into(), hk.cycle_prev.clone(), HotkeyAction::Prev),
            (t("hk.swap").into(), hk.swap_main.clone(), HotkeyAction::Swap),
            (
                t("top.apply_layout").into(),
                hk.apply_layout.clone(),
                HotkeyAction::Layout,
            ),
            (
                t("hk.toggle").into(),
                hk.toggle_previews.clone(),
                HotkeyAction::TogglePreviews,
            ),
        ];
        for r in &self.cfg.active_profile().clients {
            if !r.character.trim().is_empty() {
                list.push((
                    r.character.clone(),
                    r.hotkey.clone(),
                    HotkeyAction::Character(r.character.clone()),
                ));
            }
        }

        let mut used: HashMap<hotkey::Hotkey, String> = HashMap::new();
        let mut next_id = 1;
        for (label, text, action) in list {
            match hotkey::parse(&text) {
                Ok(None) => {}
                Err(e) => self.hotkey_errors.push(format!("{label}: {e}")),
                Ok(Some(key)) => {
                    if let Some(other) = used.get(&key) {
                        self.hotkey_errors.push(tf("eng.hk_dup", &[&label, &key, other]));
                        continue;
                    }
                    match self.keys.grab(&self.x.conn, self.x.root, key, next_id) {
                        Ok(()) => {
                            self.hotkeys.insert(next_id, action);
                            used.insert(key, label);
                            next_id += 1;
                        }
                        Err(GrabError::Taken) => self.hotkey_errors.push(tf("eng.hk_taken", &[&label, &key])),
                        Err(GrabError::NoKey) => self.hotkey_errors.push(tf("hkp.unknown", &[&key])),
                    }
                }
            }
        }
        let _ = self.x.conn.flush();
    }

    /// Führt die Aktion eines gedrückten Hotkeys aus.
    fn on_hotkey(&mut self, id: i32) {
        let Some(action) = self.hotkeys.get(&id).cloned() else {
            return;
        };
        match action {
            HotkeyAction::Next => self.cycle(true),
            HotkeyAction::Prev => self.cycle(false),
            HotkeyAction::Swap => self.swap_hotkey(),
            HotkeyAction::Layout => self.apply_layout(),
            HotkeyAction::TogglePreviews => {
                self.previews_on = !self.previews_on;
                self.update_previews(self.foreground());
                self.publish();
            }
            HotkeyAction::Character(name) => {
                let target = self
                    .clients
                    .iter()
                    .find(|c| c.character.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name)));
                if let Some(w) = target.map(|c| c.hwnd) {
                    self.activate_client(w);
                }
            }
        }
    }

    // ---------------------------------------------------------------- Eingänge

    /// Arbeitet alle Befehle der Oberfläche ab.
    fn drain_commands(&mut self) {
        let mut any = false;
        while let Ok(cmd) = self.rx.try_recv() {
            any = true;
            match cmd {
                Command::UpdateConfig(cfg) => {
                    let focus_rule_changed =
                        cfg.hotkeys.only_when_eve_focused != self.cfg.hotkeys.only_when_eve_focused;
                    self.cfg = *cfg;
                    if focus_rule_changed {
                        self.unregister_hotkeys();
                    }
                    self.foreground_policy();
                }
                Command::Activate(w) => self.activate_client(w),
                Command::MakeMain(w) => self.make_main(w),
                Command::ApplyLayout => self.apply_layout(),
                Command::CaptureLayout => self.capture_layout(),
                Command::TrimClient(w) => self.trim_where(|c| c.hwnd == w),
                Command::TrimBackground => self.trim_background(),
                Command::TogglePreviews => {
                    self.previews_on = !self.previews_on;
                    self.update_previews(self.foreground());
                }
                Command::SuspendHotkeys(on) => {
                    self.hotkeys_suspended = on;
                    if on {
                        self.unregister_hotkeys();
                    }
                    self.foreground_policy();
                }
                Command::Quit => self.quit = true,
            }
        }
        if any {
            self.publish();
        }
    }

    /// Verarbeitet Signale von Tray und zweiter Instanz.
    fn drain_signals(&mut self) {
        for sig in signal::take() {
            match sig {
                Signal::Show => {
                    let _ = self.events.send(Event::ShowDashboard);
                }
                Signal::Quit => {
                    let _ = self.events.send(Event::QuitRequested);
                }
                Signal::TogglePreviews => {
                    self.previews_on = !self.previews_on;
                    self.update_previews(self.foreground());
                }
                Signal::ApplyLayout => self.apply_layout(),
            }
            self.publish();
        }
    }

    /// Verarbeitet Klicks und Verschiebungen in den Vorschaufenstern.
    fn drain_preview_events(&mut self) {
        for ev in self.previews.take_events() {
            match ev {
                PreviewEvent::Click(src) => self.activate_client(src),
                PreviewEvent::MakeMain(src) => {
                    self.make_main(src);
                    self.x.activate(xw(src));
                    self.foreground_policy();
                    self.publish();
                }
                PreviewEvent::Moved { source, x, y, w, h: hh } => {
                    let Some(i) = self.index_of(source) else { continue };
                    let key = self.clients[i].display_name();
                    if self.clients[i].character.is_none() {
                        // Login-Fenster: nur für diese Sitzung merken, nicht speichern.
                        self.login_previews.insert(key, ((x, y), (w, hh)));
                        continue;
                    }
                    let p = &mut self.cfg.preview;
                    // Nur abweichende Größen speichern, damit der Standard-Regler weiter wirkt.
                    let size = ((w, hh) != (p.width, p.height)).then_some((w, hh));
                    p.positions.insert(key.clone(), (x, y));
                    match size {
                        Some(sz) => p.sizes.insert(key.clone(), sz),
                        None => p.sizes.remove(&key),
                    };
                    let _ = self.events.send(Event::PreviewMoved { key, x, y, size });
                    (self.repaint)();
                }
            }
        }
    }

    /// Verarbeitet ein X-Ereignis (Vorschau, Hotkey, Fokuswechsel, Tastaturbelegung).
    fn on_x_event(&mut self, ev: XEvent) {
        if self.previews.handle_event(&self.x, &ev) {
            self.drain_preview_events();
            return;
        }
        match ev {
            XEvent::KeyPress(e) => {
                if let Some(id) = self.keys.on_press(&e) {
                    self.on_hotkey(id);
                }
            }
            XEvent::KeyRelease(e) => self.keys.on_release(&e),
            XEvent::PropertyNotify(e) if e.window == self.x.root && e.atom == self.x.atoms._NET_ACTIVE_WINDOW => {
                if self.foreground() != self.last_fg {
                    self.foreground_policy();
                    self.publish();
                }
            }
            XEvent::PropertyNotify(e)
                if e.atom == self.x.atoms._NET_WM_NAME && self.index_of(h(e.window)).is_some() =>
            {
                // Charakter hat sich eingeloggt/ausgeloggt – sofort neu einlesen.
                self.tick();
            }
            XEvent::MappingNotify(_) => {
                self.keys.reload_mapping(&self.x.conn);
                if self.hotkeys_active {
                    self.register_hotkeys();
                }
            }
            _ => {}
        }
    }

    /// Aufräumen beim Beenden: Hotkeys lösen, Vorschauen schließen, Clients zurücksetzen.
    fn shutdown(&mut self) {
        self.unregister_hotkeys();
        self.previews.clear(&self.x);
        let can_prio = self.can_prio;
        for c in &mut self.clients {
            c.restore(can_prio);
        }
        let _ = self.x.conn.flush();
        self.tray = None;
    }
}

/// Wartet bis zu `timeout` auf die X-Verbindung oder den Weckruf.
fn wait(fds: &[i32], timeout: Duration) {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .filter(|&&fd| fd >= 0)
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    let ms = timeout.as_millis().min(1000) as i32;
    unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, ms) };
}

/// Hauptschleife mit X11-Verbindung.
fn run(mut e: Engine, wake_fd: i32) {
    // Fokuswechsel und Tastaturbelegung mitbekommen.
    let aux = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    let _ = e.x.conn.change_window_attributes(e.x.root, &aux);
    e.previews.bounds = e.x.monitors().into_iter().map(|m| m.work).collect();
    e.tick();
    crate::diag::log("Engine bereit (X11)");

    let x_fd = e.x.conn.stream().as_raw_fd();
    let mut next_tick = Instant::now() + TICK;
    let mut next_paint = Instant::now();
    while !e.quit {
        loop {
            match e.x.conn.poll_for_event() {
                Ok(Some(ev)) => e.on_x_event(ev),
                Ok(None) => break,
                Err(err) => {
                    crate::diag::log(&format!("X-Verbindung verloren: {err}"));
                    e.quit = true;
                    break;
                }
            }
        }
        signal::drain_wake(wake_fd);
        e.drain_commands();
        e.drain_signals();

        let now = Instant::now();
        if now >= next_tick {
            e.previews.bounds = e.x.monitors().into_iter().map(|m| m.work).collect();
            e.tick();
            next_tick = now + TICK;
        }
        if now >= next_paint {
            e.previews.paint_all(&e.x);
            next_paint = now + PAINT;
        }
        let _ = e.x.conn.flush();
        if e.quit {
            break;
        }
        let due = next_tick.min(next_paint);
        wait(&[x_fd, wake_fd], due.saturating_duration_since(Instant::now()));
    }
    e.shutdown();
}

/// Ersatzschleife ohne X11 (reines Wayland ohne XWayland): nur Systemwerte, Tray und Befehle.
fn run_headless(
    rx: Receiver<Command>,
    events: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    repaint: Box<dyn Fn() + Send>,
    wake_fd: i32,
) {
    let _tray = Tray::create();
    let mut sys = SystemCpu::default();
    let mut seq = 0;
    loop {
        signal::drain_wake(wake_fd);
        while let Ok(cmd) = rx.try_recv() {
            if matches!(cmd, Command::Quit) {
                return;
            }
        }
        for sig in signal::take() {
            let ev = match sig {
                Signal::Show => Event::ShowDashboard,
                Signal::Quit => Event::QuitRequested,
                _ => continue,
            };
            let _ = events.send(ev);
        }
        let (ram_total, ram_avail) = procstat::system_memory();
        seq += 1;
        *snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Snapshot {
            seq,
            sys_cpu: sys.sample(),
            ram_total,
            ram_avail,
            cores: procstat::logical_cores(),
            warnings: vec![t("eng.no_x11").to_string()],
            previews_on: false,
            ..Default::default()
        };
        repaint();
        wait(&[wake_fd], TICK);
    }
}

/// Hauptfunktion des Engine-Threads: verbinden und Schleife starten.
fn thread_main(
    cfg: Config,
    rx: Receiver<Command>,
    events: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    repaint: Box<dyn Fn() + Send>,
    ready: Sender<()>,
    wake_fd: i32,
) {
    let x = match X::connect() {
        Ok(x) => x,
        Err(err) => {
            crate::diag::log(&format!("Keine X11-Verbindung: {err}"));
            let _ = ready.send(());
            run_headless(rx, events, snapshot, repaint, wake_fd);
            return;
        }
    };
    let previews = PreviewManager::new(&x);
    if !previews.supported {
        crate::diag::log("Composite/Render nicht verfügbar – keine Live-Vorschau");
    }
    let keys = KeyGrabber::new(&x.conn);
    let can_prio = procstat::can_restore_priority();
    let can_trim = procstat::has_cap_sys_nice();
    crate::diag::log(&format!(
        "X11: Fenstermanager {}, Priorität {}, Speicher auslagern {}",
        if x.wm_present { "ja" } else { "nein" },
        if can_prio { "ja" } else { "nein (CAP_SYS_NICE fehlt)" },
        if can_trim { "ja" } else { "nein (CAP_SYS_NICE fehlt)" }
    ));
    let engine = Engine {
        x,
        cfg,
        rx,
        events,
        snapshot,
        repaint,
        clients: Vec::new(),
        main_hwnd: None,
        prev_main: None,
        last_fg_eve: None,
        previews,
        previews_on: true,
        login_previews: HashMap::new(),
        keys,
        hotkeys: HashMap::new(),
        hotkeys_active: false,
        hotkeys_suspended: false,
        hotkey_signature: Vec::new(),
        hotkey_errors: Vec::new(),
        sys_cpu: SystemCpu::default(),
        sys_cpu_pct: 0.0,
        cores: procstat::logical_cores(),
        last_trim: Instant::now(),
        seq: 0,
        tray: Tray::create(),
        can_prio,
        can_trim,
        last_fg: 0,
        quit: false,
    };
    let _ = ready.send(());
    run(engine, wake_fd);
}

/// Zugriff der Oberfläche auf die Engine.
pub struct EngineHandle {
    tx: Sender<Command>,
    wake_fd: i32,
    thread: Option<JoinHandle<()>>,
    /// Zuletzt veröffentlichter Zustand.
    pub snapshot: Arc<Mutex<Snapshot>>,
    /// Ereignisse der Engine (Positionen, erfasste Layouts, Meldungen, Tray).
    pub events: Receiver<Event>,
}

impl EngineHandle {
    /// Startet den Engine-Thread; `repaint` wird nach jeder Aktualisierung aufgerufen.
    pub fn start(cfg: Config, repaint: Box<dyn Fn() + Send>) -> Self {
        let (tx, rx) = mpsc::channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let snap = Arc::clone(&snapshot);
        let wake_fd = signal::create_wake_fd();
        let thread = std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || thread_main(cfg, rx, ev_tx, snap, repaint, ready_tx, wake_fd))
            .expect("Engine-Thread konnte nicht gestartet werden");
        // Nie unbegrenzt warten: Hängt die Engine, startet wenigstens die Oberfläche.
        if ready_rx.recv_timeout(Duration::from_secs(10)).is_err() {
            crate::diag::log("Engine-Thread hat sich nicht innerhalb von 10 s gemeldet");
        }
        Self {
            tx,
            wake_fd,
            thread: Some(thread),
            snapshot,
            events: ev_rx,
        }
    }

    /// Schickt einen Befehl und weckt die Engine.
    pub fn send(&self, cmd: Command) {
        if self.tx.send(cmd).is_ok() {
            signal::wake();
        }
    }

    /// Kopie des aktuellen Zustands.
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Beendet die Engine und wartet, bis alle Clients zurückgesetzt sind.
    pub fn shutdown(&mut self) {
        if let Some(t) = self.thread.take() {
            self.send(Command::Quit);
            let _ = t.join();
            signal::close_wake_fd(self.wake_fd);
        }
    }
}

impl Drop for EngineHandle {
    /// Sorgt dafür, dass die Engine auch bei unerwartetem Ende sauber aufräumt.
    fn drop(&mut self) {
        self.shutdown();
    }
}
