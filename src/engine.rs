//! Engine-Thread: besitzt alle Win32-Ressourcen (Hotkeys, Vorschaufenster, Timer,
//! Vordergrund-Hook) und führt die Befehle der Oberfläche aus.
//!
//! Kommunikation:
//! * Oberfläche → Engine: `Command` über einen Kanal + Weck-Nachricht ans Engine-Fenster.
//! * Engine → Oberfläche: `Snapshot` (geteilter Zustand) und `Event` (Kanal).
//!
//! Wichtig (EVE-EULA): Die Engine sendet niemals Tastatur- oder Mauseingaben an
//! EVE-Clients. Sie verschiebt nur Fenster, holt eines in den Vordergrund und
//! ändert Prozess-Einstellungen – ein Tastendruck löst genau eine Windows-Aktion aus.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use evemb::config::{Config, Priority, Rect, cores_to_mask, mask_to_cores};
use evemb::i18n::{t, tf};
use evemb::{hotkey, layout};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::preview::{self, PreviewEvent, PreviewItem, PreviewManager};
use crate::procstat::{self, CpuMeter, MemInfo, ProcHandle, SystemCpu};
use crate::tray::{Tray, TrayCommand};
use crate::win::{self, Hwnd, wide};

/// Nachricht: „Es liegen Befehle im Kanal.“
const WM_ENGINE_WAKE: u32 = WM_APP + 1;
/// Nachricht: „Das Vordergrundfenster hat gewechselt.“
const WM_ENGINE_FOREGROUND: u32 = WM_APP + 3;
/// Nachricht vom Tray-Symbol; `wParam` enthält den `TrayCommand`.
const WM_ENGINE_TRAY: u32 = WM_APP + 5;
/// Timer-ID für die regelmäßige Aktualisierung.
const TIMER_TICK: usize = 1;
/// Aktualisierungsintervall in Millisekunden.
const TICK_MS: u32 = 500;
/// Bytes pro Megabyte.
const MB: u64 = 1024 * 1024;

/// Engine-Fenster für den Vordergrund-Hook (der Callback hat keinen eigenen Kontext).
static ENGINE_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    /// Die Engine-Instanz des Engine-Threads.
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

pub use crate::engine_api::{ClientInfo, Command, Event, Snapshot};

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
    proc_: Option<ProcHandle>,
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
    fn new(w: win::EveWindow) -> Self {
        let proc_ = ProcHandle::open(w.pid);
        let original_priority = proc_.as_ref().and_then(|p| p.priority());
        let (original_mask, system_mask) = match proc_.as_ref().and_then(|p| p.affinity()) {
            Some((pm, sm)) => (Some(pm), sm),
            None => (None, u64::MAX),
        };
        Self {
            hwnd: w.hwnd,
            pid: w.pid,
            character: w.character,
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
    fn restore(&mut self) {
        let Some(ph) = &self.proc_ else { return };
        if let Some(p) = self.original_priority.filter(|&p| Some(p) != self.applied_priority) {
            ph.set_priority(p);
            self.applied_priority = Some(p);
        }
        if let Some(m) = self.original_mask.filter(|&m| Some(m) != self.applied_mask) {
            ph.set_affinity(m);
            self.applied_mask = Some(m);
        }
    }
}

/// Position und Größe eines Vorschaufensters: ((x, y), (Breite, Höhe)).
type PreviewPlacement = ((i32, i32), (i32, i32));

/// Zustand der Engine (lebt ausschließlich im Engine-Thread).
struct Engine {
    hwnd: Hwnd,
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
}

impl Engine {
    /// Neue Engine für das gegebene (Nachrichten-)Fenster.
    fn new(
        hwnd: Hwnd,
        cfg: Config,
        rx: Receiver<Command>,
        events: Sender<Event>,
        snapshot: Arc<Mutex<Snapshot>>,
        repaint: Box<dyn Fn() + Send>,
    ) -> Self {
        Self {
            hwnd,
            cfg,
            rx,
            events,
            snapshot,
            repaint,
            clients: Vec::new(),
            main_hwnd: None,
            prev_main: None,
            last_fg_eve: None,
            previews: PreviewManager::default(),
            previews_on: true,
            login_previews: HashMap::new(),
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
            tray: None,
        }
    }

    /// Schickt eine Statusmeldung an die Oberfläche.
    fn message(&self, text: impl Into<String>) {
        let _ = self.events.send(Event::Message(text.into()));
    }

    /// Index eines Clients anhand seines Fensters.
    fn index_of(&self, h: Hwnd) -> Option<usize> {
        self.clients.iter().position(|c| c.hwnd == h)
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
        for w in win::find_eve_windows() {
            match old.remove(&w.hwnd) {
                Some(mut c) => {
                    if w.character.is_some() && c.character != w.character {
                        changed = true;
                    }
                    c.character = w.character;
                    self.clients.push(c);
                }
                None => {
                    changed = true;
                    self.clients.push(Client::new(w));
                }
            }
        }
        // Verbleibende Einträge in `old` sind geschlossen; ihre Handles werden beim Drop freigegeben.
        let alive = |h: &Option<Hwnd>, list: &[Client]| h.filter(|x| list.iter().any(|c| c.hwnd == *x));
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

    /// Reaktion auf (mögliche) Vordergrundwechsel: Drosseln, Minimieren, Vorschauen.
    fn foreground_policy(&mut self) {
        let fg = win::foreground();
        self.sync_hotkeys(fg);
        self.apply_performance(fg);
        if self.index_of(fg).is_some() {
            if self.cfg.performance.minimize_inactive
                && let Some(prev) = self.last_fg_eve.filter(|&p| p != fg && self.index_of(p).is_some())
            {
                win::minimize(prev);
            }
            self.last_fg_eve = Some(fg);
        }
        self.update_previews(fg);
    }

    /// Setzt Priorität und Kern-Affinität aller Clients gemäß Regeln und Drosselung.
    fn apply_performance(&mut self, fg: Hwnd) {
        let perf = &self.cfg.performance;
        let profile = self.cfg.active_profile();
        for c in &mut self.clients {
            let Some(ph) = &c.proc_ else { continue };
            let rule = c.character.as_deref().and_then(|n| profile.rule(n));
            let throttle = perf.throttle_inactive && c.hwnd != fg;

            let want_prio = if throttle {
                Some(perf.inactive_priority)
            } else if let Some(p) = rule.and_then(|r| r.priority) {
                Some(p)
            } else if perf.throttle_inactive {
                Some(perf.active_priority)
            } else {
                None
            };
            if let Some(t) = want_prio.or(c.original_priority)
                && c.applied_priority != Some(t)
            {
                if !ph.set_priority(t) {
                    c.limited = true;
                }
                c.applied_priority = Some(t);
            }

            let rule_mask = rule.map(|r| cores_to_mask(&r.cores)).filter(|&m| m != 0);
            let bg_mask = Some(cores_to_mask(&perf.background_cores)).filter(|&m| throttle && m != 0);
            let want_mask = bg_mask.or(rule_mask).map(|m| m & c.system_mask).filter(|&m| m != 0);
            if let Some(t) = want_mask.or(c.original_mask)
                && c.applied_mask != Some(t)
            {
                if !ph.set_affinity(t) {
                    c.limited = true;
                }
                c.applied_mask = Some(t);
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
                let mut label = c.display_name();
                if Some(c.hwnd) == self.main_hwnd {
                    label.push_str("  ★");
                }
                if win::is_minimized(c.hwnd) {
                    label.push_str("  ");
                    label.push_str(t("preview.minimized"));
                }
                PreviewItem {
                    source: c.hwnd,
                    key: c.display_name(),
                    label,
                    active,
                    visible: show && !(s.hide_active && active),
                }
            })
            .collect();
        self.previews.sync(&items, s);
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
        let fg = win::foreground();
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
                if c.proc_.is_none() || c.limited {
                    warnings.push(tf("eng.warn_rights", &[&c.display_name()]));
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
                    minimized: win::is_minimized(c.hwnd),
                    throttled: c.throttled,
                    limited: c.proc_.is_none() || c.limited,
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

        if let Some(tray) = &mut self.tray {
            tray.set_tooltip(&tf("tray.tooltip", &[&self.clients.len()]));
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
        if let Some(i) = self.main_hwnd.and_then(|h| self.index_of(h)) {
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
            .or_else(|| self.index_of(win::foreground()))
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
        for (h, r) in targets {
            win::place_window(h, r, borderless);
        }
    }

    /// Macht einen Client zum Haupt-Client und wendet das Layout an.
    fn make_main(&mut self, h: Hwnd) {
        if self.index_of(h).is_none() {
            return;
        }
        self.resolve_main();
        if self.main_hwnd != Some(h) {
            self.prev_main = self.main_hwnd;
            self.main_hwnd = Some(h);
        }
        self.apply_layout();
    }

    /// Holt einen Client in den Vordergrund (optional mit Tausch ins Hauptfenster).
    fn activate_client(&mut self, h: Hwnd) {
        if self.index_of(h).is_none() {
            return;
        }
        if self.cfg.active_profile().swap_on_activate {
            self.make_main(h);
        }
        if !win::activate(h) {
            self.message(t("eng.focus_denied"));
        }
        self.foreground_policy();
        self.publish();
    }

    /// Hotkey „Tauschen“: Vordergrund-Client ins Hauptfenster; ist er es schon,
    /// kommt der vorherige Haupt-Client zurück.
    fn swap_hotkey(&mut self) {
        let fg = win::foreground();
        if self.index_of(fg).is_none() {
            return;
        }
        if Some(fg) == self.main_hwnd {
            if let Some(prev) = self.prev_main.filter(|&p| self.index_of(p).is_some()) {
                self.make_main(prev);
                win::activate(prev);
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
        let current = self.index_of(win::foreground());
        if let Some(next) = layout::cycle_step(&order, current, forward) {
            self.activate_client(self.clients[next].hwnd);
        }
    }

    /// Gibt den Arbeitsspeicher der ausgewählten Clients frei und meldet das Ergebnis.
    fn trim_where(&mut self, filter: impl Fn(&Client) -> bool) {
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

    /// Speicher aller Clients außer dem Vordergrund-Client freigeben.
    fn trim_background(&mut self) {
        let fg = win::foreground();
        self.trim_where(|c| c.hwnd != fg);
    }

    /// Erfasst die aktuellen Fensterpositionen (Haupt zuerst) und schickt sie an die Oberfläche.
    fn capture_layout(&mut self) {
        let Some(main) = self.resolve_main() else {
            self.message(t("eng.no_clients_capture"));
            return;
        };
        let Some(main_rect) = win::visible_rect(self.clients[main].hwnd) else {
            return;
        };
        let mut secondaries = Vec::new();
        let mut characters = vec![self.clients[main].character.clone()];
        for i in self.order().into_iter().filter(|&i| i != main) {
            if let Some(r) = win::visible_rect(self.clients[i].hwnd) {
                secondaries.push(r);
                characters.push(self.clients[i].character.clone());
            }
        }
        let _ = self.events.send(Event::LayoutCaptured {
            main: main_rect,
            secondaries,
            characters,
        });
    }

    // ---------------------------------------------------------------- Hotkeys

    /// Alle Hotkey-Texte, deren Änderung eine Neuregistrierung erfordert.
    fn current_hotkey_signature(&self) -> Vec<String> {
        let h = &self.cfg.hotkeys;
        let mut sig = vec![
            h.cycle_next.clone(),
            h.cycle_prev.clone(),
            h.swap_main.clone(),
            h.apply_layout.clone(),
            h.toggle_previews.clone(),
        ];
        for r in &self.cfg.active_profile().clients {
            sig.push(format!("{}={}", r.character, r.hotkey));
        }
        sig
    }

    /// Sollen die Hotkeys gerade aktiv sein? (Nur-in-EVE-Option beachten.)
    fn hotkeys_wanted(&self, fg: Hwnd) -> bool {
        !self.hotkeys_suspended
            && (!self.cfg.hotkeys.only_when_eve_focused || self.index_of(fg).is_some() || win::is_own_window(fg))
    }

    /// Schaltet die Hotkeys passend zum Vordergrund an/aus und registriert sie
    /// nach Konfigurationsänderungen neu.
    fn sync_hotkeys(&mut self, fg: Hwnd) {
        // Während eines Fensterwechsels ist kurz gar kein Fenster vorne – nicht reagieren.
        if fg == 0 {
            return;
        }
        let want = self.hotkeys_wanted(fg);
        if want && (!self.hotkeys_active || self.current_hotkey_signature() != self.hotkey_signature) {
            self.register_hotkeys();
        } else if !want && self.hotkeys_active {
            self.unregister_hotkeys();
        }
    }

    /// Gibt alle registrierten Hotkeys wieder frei (andere Programme bekommen die Tasten zurück).
    fn unregister_hotkeys(&mut self) {
        let w = win::hwnd(self.hwnd);
        for id in self.hotkeys.keys() {
            unsafe { UnregisterHotKey(w, *id) };
        }
        self.hotkeys.clear();
        self.hotkeys_active = false;
    }

    /// Registriert alle globalen und Charakter-Hotkeys neu und sammelt Fehler.
    fn register_hotkeys(&mut self) {
        let w = win::hwnd(self.hwnd);
        self.unregister_hotkeys();
        self.hotkeys_active = true;
        self.hotkey_errors.clear();
        self.hotkey_signature = self.current_hotkey_signature();

        let h = &self.cfg.hotkeys;
        let mut list: Vec<(String, String, HotkeyAction)> = vec![
            (t("hk.next").into(), h.cycle_next.clone(), HotkeyAction::Next),
            (t("hk.prev").into(), h.cycle_prev.clone(), HotkeyAction::Prev),
            (t("hk.swap").into(), h.swap_main.clone(), HotkeyAction::Swap),
            (
                t("top.apply_layout").into(),
                h.apply_layout.clone(),
                HotkeyAction::Layout,
            ),
            (
                t("hk.toggle").into(),
                h.toggle_previews.clone(),
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
                Ok(Some(hk)) => {
                    if let Some(other) = used.get(&hk) {
                        self.hotkey_errors.push(tf("eng.hk_dup", &[&label, &hk, other]));
                        continue;
                    }
                    let ok = unsafe { RegisterHotKey(w, next_id, hk.mods | MOD_NOREPEAT, hk.vk) } != 0;
                    if ok {
                        self.hotkeys.insert(next_id, action);
                        used.insert(hk, label);
                        next_id += 1;
                    } else {
                        self.hotkey_errors.push(tf("eng.hk_taken", &[&label, &hk]));
                    }
                }
            }
        }
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
                self.update_previews(win::foreground());
                self.publish();
            }
            HotkeyAction::Character(name) => {
                let target = self
                    .clients
                    .iter()
                    .find(|c| c.character.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name)));
                if let Some(h) = target.map(|c| c.hwnd) {
                    self.activate_client(h);
                }
            }
        }
    }

    // ---------------------------------------------------------------- Nachrichten

    /// Arbeitet alle Befehle der Oberfläche ab.
    fn drain_commands(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                Command::UpdateConfig(cfg) => {
                    let focus_rule_changed =
                        cfg.hotkeys.only_when_eve_focused != self.cfg.hotkeys.only_when_eve_focused;
                    self.cfg = *cfg;
                    if focus_rule_changed {
                        self.unregister_hotkeys();
                    }
                    // Registriert bei Bedarf neu (Signatur geändert oder gerade inaktiv).
                    self.foreground_policy();
                }
                Command::Activate(h) => self.activate_client(h),
                Command::MakeMain(h) => self.make_main(h),
                Command::ApplyLayout => self.apply_layout(),
                Command::CaptureLayout => self.capture_layout(),
                Command::TrimClient(h) => self.trim_where(|c| c.hwnd == h),
                Command::TrimBackground => self.trim_background(),
                Command::TogglePreviews => {
                    self.previews_on = !self.previews_on;
                    self.update_previews(win::foreground());
                }
                Command::SuspendHotkeys(on) => {
                    self.hotkeys_suspended = on;
                    if on {
                        self.unregister_hotkeys();
                    }
                    self.foreground_policy();
                }
                Command::Quit => {
                    unsafe { PostQuitMessage(0) };
                    return;
                }
            }
        }
        self.publish();
    }

    /// Führt einen Befehl aus dem Tray-Menü aus.
    fn on_tray(&mut self, raw: usize) {
        match TrayCommand::from_raw(raw) {
            Some(TrayCommand::Show) => {
                let _ = self.events.send(Event::ShowDashboard);
            }
            Some(TrayCommand::Quit) => {
                let _ = self.events.send(Event::QuitRequested);
            }
            Some(TrayCommand::TogglePreviews) => {
                self.previews_on = !self.previews_on;
                self.update_previews(win::foreground());
            }
            Some(TrayCommand::ApplyLayout) => self.apply_layout(),
            None => return,
        }
        self.publish();
    }

    /// Verarbeitet Klicks und Verschiebungen in den Vorschaufenstern.
    fn drain_preview_events(&mut self) {
        for ev in preview::take_events() {
            match ev {
                PreviewEvent::Click(src) => self.activate_client(src),
                PreviewEvent::MakeMain(src) => {
                    self.make_main(src);
                    win::activate(src);
                    self.foreground_policy();
                    self.publish();
                }
                PreviewEvent::Moved { source, x, y, w, h } => {
                    let Some(i) = self.index_of(source) else { continue };
                    let key = self.clients[i].display_name();
                    if self.clients[i].character.is_none() {
                        // Login-Fenster: nur für diese Sitzung merken, nicht speichern.
                        self.login_previews.insert(key, ((x, y), (w, h)));
                        continue;
                    }
                    let p = &mut self.cfg.preview;
                    // Nur abweichende Größen speichern, damit der Standard-Regler weiter wirkt.
                    let size = ((w, h) != (p.width, p.height)).then_some((w, h));
                    p.positions.insert(key.clone(), (x, y));
                    match size {
                        Some(sz) => p.sizes.insert(key.clone(), sz),
                        None => p.sizes.remove(&key),
                    };
                    let _ = self.events.send(Event::PreviewMoved { key, x, y, size });
                }
            }
        }
    }

    /// Aufräumen beim Beenden: Hotkeys lösen, Vorschauen schließen, Clients zurücksetzen.
    fn shutdown(&mut self) {
        self.unregister_hotkeys();
        self.previews.clear();
        self.tray = None;
        for c in &mut self.clients {
            c.restore();
        }
    }
}

/// Führt `f` mit der Engine aus, falls sie gerade nicht anderweitig ausgeliehen ist.
fn with_engine(f: impl FnOnce(&mut Engine)) {
    ENGINE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut()
            && let Some(engine) = guard.as_mut()
        {
            f(engine);
        }
    });
}

/// Fensterprozedur des unsichtbaren Engine-Fensters: Timer, Befehle, Hotkeys, Ereignisse.
unsafe extern "system" fn engine_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wp == TIMER_TICK => with_engine(Engine::tick),
        WM_ENGINE_WAKE => with_engine(Engine::drain_commands),
        WM_HOTKEY => with_engine(|e| e.on_hotkey(wp as i32)),
        preview::WM_PREVIEW_EVENT => with_engine(Engine::drain_preview_events),
        WM_ENGINE_TRAY => with_engine(|e| e.on_tray(wp)),
        WM_ENGINE_FOREGROUND => with_engine(|e| {
            e.foreground_policy();
            e.publish();
        }),
        _ => return unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
    0
}

/// Callback des Vordergrund-Hooks: leitet den Wechsel als Nachricht an die Engine weiter.
unsafe extern "system" fn on_foreground_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    let target = ENGINE_HWND.load(Ordering::SeqCst);
    if target != 0 {
        unsafe { PostMessageW(win::hwnd(target), WM_ENGINE_FOREGROUND, 0, 0) };
    }
}

/// Hauptfunktion des Engine-Threads: Fenster anlegen, Nachrichtenschleife, Aufräumen.
fn thread_main(
    cfg: Config,
    rx: Receiver<Command>,
    events: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    repaint: Box<dyn Fn() + Send>,
    ready: Sender<Hwnd>,
) {
    let class = wide("EveMultiboxEngine");
    let title = wide("EVE Multibox Engine");
    let h = unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(engine_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    };
    let handle = win::handle(h);
    if h.is_null() {
        crate::diag::log("Engine-Fenster konnte nicht erstellt werden");
    }
    ENGINE_HWND.store(handle, Ordering::SeqCst);
    preview::set_notify_target(handle);
    preview::register_class();

    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(on_foreground_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };

    let mut engine = Engine::new(handle, cfg, rx, events, snapshot, repaint);
    engine.tray = Tray::create(handle, WM_ENGINE_TRAY);
    if engine.tray.is_none() {
        crate::diag::log("Tray-Symbol konnte nicht angelegt werden");
    }
    ENGINE.with(|e| *e.borrow_mut() = Some(engine));
    // Erster Durchlauf: Clients suchen, Hotkeys je nach Vordergrund registrieren, veröffentlichen.
    with_engine(Engine::tick);
    unsafe { SetTimer(h, TIMER_TICK, TICK_MS, None) };
    crate::diag::log(&format!("Engine bereit (Fenster {handle:#x})"));
    let _ = ready.send(handle);

    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        KillTimer(h, TIMER_TICK);
        if !hook.is_null() {
            UnhookWinEvent(hook);
        }
    }
    if let Some(mut engine) = ENGINE.with(|e| e.borrow_mut().take()) {
        engine.shutdown();
    }
    unsafe { DestroyWindow(h) };
    ENGINE_HWND.store(0, Ordering::SeqCst);
}

/// Zugriff der Oberfläche auf die Engine.
pub struct EngineHandle {
    tx: Sender<Command>,
    hwnd: Hwnd,
    thread: Option<JoinHandle<()>>,
    /// Zuletzt veröffentlichter Zustand.
    pub snapshot: Arc<Mutex<Snapshot>>,
    /// Ereignisse der Engine (Positionen, erfasste Layouts, Meldungen).
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
        let thread = std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || thread_main(cfg, rx, ev_tx, snap, repaint, ready_tx))
            .expect("Engine-Thread konnte nicht gestartet werden");
        // Nie unbegrenzt warten: Hängt die Engine, startet wenigstens die Oberfläche.
        let hwnd = ready_rx.recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| {
            crate::diag::log("Engine-Thread hat sich nicht innerhalb von 10 s gemeldet");
            0
        });
        Self {
            tx,
            hwnd,
            thread: Some(thread),
            snapshot,
            events: ev_rx,
        }
    }

    /// Schickt einen Befehl und weckt die Engine.
    pub fn send(&self, cmd: Command) {
        if self.tx.send(cmd).is_ok() && self.hwnd != 0 {
            unsafe { PostMessageW(win::hwnd(self.hwnd), WM_ENGINE_WAKE, 0, 0) };
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
        }
    }
}

impl Drop for EngineHandle {
    /// Sorgt dafür, dass die Engine auch bei unerwartetem Ende sauber aufräumt.
    fn drop(&mut self) {
        self.shutdown();
    }
}
