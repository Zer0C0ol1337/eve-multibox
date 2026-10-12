//! Oberfläche (egui): Hauptfenster mit Tabs, gemeinsame Bausteine, Tray-Verhalten
//! (Ausblenden statt Beenden), Sprachumschaltung, Hotkey-Aufnahme, Wasserzeichen und
//! das automatische Speichern/Übertragen der Konfiguration an die Engine.

mod dashboard;
mod hotkeys;
mod layouts;
mod performance;
mod preview_tab;
mod settings;
mod theme;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Margin, RichText, Ui, ViewportCommand, vec2};
use evemb::config::{self, ClientRule, Config, Priority, Rect};
use evemb::hotkey;
use evemb::i18n::{self, Lang, t, tf};

use crate::engine::{Command, EngineHandle, Event, Snapshot};
use crate::platform::{self, Hwnd, MonitorInfo};

/// Wie lange nach der letzten Änderung gewartet wird, bevor gespeichert wird.
const SAVE_DELAY: Duration = Duration::from_millis(400);
/// Anzahl gespeicherter CPU-Messwerte je Client (bei 0,5 s = 60 s Verlauf).
const HISTORY_LEN: usize = 120;
/// Maximale Anzahl Protokollzeilen.
const LOG_LEN: usize = 100;
/// Copyright-Hinweis (Statusleiste, Wasserzeichen, „Über“).
pub const COPYRIGHT: &str = "© 2026 LeggaSchnegge";

/// Warnfarbe (Bernstein).
const WARN: Color32 = Color32::from_rgb(255, 190, 60);
/// Fehlerfarbe.
const ERROR: Color32 = Color32::from_rgb(255, 110, 100);
/// Akzentfarbe für Hauptfenster/aktiven Client.
const ACCENT: Color32 = theme::ACCENT;

/// Die Tabs des Hauptfensters.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Layouts,
    Preview,
    Performance,
    Hotkeys,
    Settings,
}

/// Zustand der Schrift für Chinesisch (wird erst bei Bedarf aus Windows geladen).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CjkFont {
    NotLoaded,
    Loaded,
    Missing,
}

/// Laufende Hotkey-Aufnahme: welches Feld gerade auf einen Tastendruck wartet.
#[derive(Default)]
pub struct Recorder {
    active: Option<egui::Id>,
}

/// Anwendungszustand der Oberfläche.
pub struct App {
    /// Bearbeitete Konfiguration.
    cfg: Config,
    /// Zuletzt gespeicherte und an die Engine übertragene Konfiguration.
    committed: Config,
    /// Seit wann es ungespeicherte Änderungen gibt.
    dirty_since: Option<Instant>,
    engine: EngineHandle,
    snap: Snapshot,
    last_seq: u64,
    /// CPU-Verlauf je Client für die Mini-Diagramme.
    history: HashMap<Hwnd, VecDeque<f32>>,
    tab: Tab,
    log: VecDeque<String>,
    save_error: Option<String>,
    monitors: Vec<MonitorInfo>,
    layout_ui: layouts::LayoutUiState,
    recorder: Recorder,
    /// Zuletzt an die Engine gemeldeter Pause-Zustand der Hotkeys.
    hotkeys_paused: bool,
    /// Autostart-Status (Quelle: Registry) und letzter Fehler.
    autostart: bool,
    autostart_error: Option<String>,
    /// Beenden wurde bestätigt (Tray „Beenden“) – Schließen nicht mehr abfangen.
    quitting: bool,
    cjk_font: CjkFont,
    /// Frames, in denen das Dashboard noch ausgeblendet werden muss (Start im Infobereich).
    /// eframe macht das Fenster nach dem ersten Bild sichtbar – danach wird es wieder versteckt.
    hide_frames: u8,
    /// App-Icon für die eigene Titelleiste.
    icon: egui::TextureHandle,
}

impl App {
    /// Startet die Engine und richtet die Oberfläche ein.
    /// `start_hidden`: Dashboard gleich in den Infobereich ausblenden (Autostart).
    pub fn new(cc: &eframe::CreationContext<'_>, cfg: Config, start_hidden: bool) -> Self {
        theme::apply(&cc.egui_ctx);
        let icon = cc.egui_ctx.load_texture(
            "app_icon",
            egui::ColorImage::from_rgba_unmultiplied([64, 64], &evemb::icon::icon_rgba(64)),
            egui::TextureOptions::LINEAR,
        );
        let ctx = cc.egui_ctx.clone();
        let engine = EngineHandle::start(cfg.clone(), Box::new(move || ctx.request_repaint()));
        let mut log = VecDeque::new();
        log.push_back(tf("log.config", &[&config::config_path().display()]));
        let mut app = Self {
            committed: cfg.clone(),
            cfg,
            dirty_since: None,
            engine,
            snap: Snapshot::default(),
            last_seq: 0,
            history: HashMap::new(),
            tab: Tab::Dashboard,
            log,
            save_error: None,
            monitors: platform::monitors(),
            layout_ui: layouts::LayoutUiState::default(),
            recorder: Recorder::default(),
            hotkeys_paused: false,
            autostart: platform::autostart::is_enabled(),
            autostart_error: None,
            quitting: false,
            cjk_font: CjkFont::NotLoaded,
            hide_frames: if start_hidden { 3 } else { 0 },
            icon,
        };
        app.apply_language(&cc.egui_ctx);
        app
    }

    /// Fügt eine Zeile ins Protokoll ein.
    fn log(&mut self, text: impl Into<String>) {
        self.log.push_back(text.into());
        while self.log.len() > LOG_LEN {
            self.log.pop_front();
        }
    }

    /// Setzt die Sprache aus der Konfiguration und lädt für Chinesisch die Windows-Schrift.
    fn apply_language(&mut self, ctx: &egui::Context) {
        let lang = platform::resolve_language(&self.cfg);
        i18n::set_lang(lang);
        if lang == Lang::Zh && self.cjk_font == CjkFont::NotLoaded {
            self.cjk_font = if load_cjk_font(ctx) {
                CjkFont::Loaded
            } else {
                CjkFont::Missing
            };
        }
    }

    /// Verarbeitet Ereignisse der Engine (Vorschauen, erfasste Layouts, Meldungen, Tray).
    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.engine.events.try_recv() {
            match ev {
                Event::PreviewMoved { key, x, y, size } => {
                    let p = &mut self.cfg.preview;
                    p.positions.insert(key.clone(), (x, y));
                    match size {
                        Some(sz) => p.sizes.insert(key, sz),
                        None => p.sizes.remove(&key),
                    };
                }
                Event::LayoutCaptured {
                    main,
                    secondaries,
                    characters,
                } => {
                    self.apply_captured_layout(main, secondaries, characters);
                }
                Event::Message(text) => self.log(text),
                Event::ShowDashboard => {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                Event::QuitRequested => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
    }

    /// Start im Infobereich: Dashboard nach den ersten Frames wieder ausblenden
    /// (ohne Tray-Symbol bliebe es sonst unerreichbar – dann bleibt es sichtbar).
    fn hide_on_start(&mut self, ctx: &egui::Context) {
        if self.hide_frames == 0 {
            return;
        }
        self.hide_frames -= 1;
        if platform::tray_available() {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
        ctx.request_repaint();
    }

    /// Fängt das Schließen (X) ab und blendet das Dashboard stattdessen ins Tray aus.
    fn handle_close(&mut self, ctx: &egui::Context) {
        let close = ctx.input(|i| i.viewport().close_requested());
        if close && !self.quitting && self.cfg.general.close_to_tray && platform::tray_available() {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }

    /// Übernimmt erfasste Fensterpositionen ins aktive Profil und ordnet die Regeln passend.
    fn apply_captured_layout(&mut self, main: Rect, secondaries: Vec<Rect>, characters: Vec<Option<String>>) {
        let count = secondaries.len();
        let profile = self.cfg.active_profile_mut();
        profile.main_slot = main;
        profile.secondary_slots = secondaries;
        if let Some(Some(first)) = characters.first() {
            profile.default_main = first.clone();
        }
        // Regeln in der erfassten Reihenfolge anlegen bzw. sortieren, übrige hinten anhängen.
        let mut ordered: Vec<ClientRule> = Vec::new();
        for name in characters.iter().flatten() {
            let pos = profile
                .clients
                .iter()
                .position(|r| r.character.eq_ignore_ascii_case(name));
            ordered.push(match pos {
                Some(i) => profile.clients.remove(i),
                None => ClientRule {
                    character: name.clone(),
                    ..Default::default()
                },
            });
        }
        ordered.append(&mut profile.clients);
        profile.clients = ordered;
        self.log(tf("log.layout_captured", &[&count]));
    }

    /// Holt den aktuellen Zustand der Engine und pflegt den CPU-Verlauf.
    fn refresh_snapshot(&mut self) {
        self.snap = self.engine.snapshot();
        if self.snap.seq == self.last_seq {
            return;
        }
        self.last_seq = self.snap.seq;
        for c in &self.snap.clients {
            let h = self.history.entry(c.hwnd).or_default();
            h.push_back(c.cpu);
            while h.len() > HISTORY_LEN {
                h.pop_front();
            }
        }
        let alive: Vec<Hwnd> = self.snap.clients.iter().map(|c| c.hwnd).collect();
        self.history.retain(|h, _| alive.contains(h));
    }

    /// Meldet der Engine, ob Hotkeys gerade pausiert werden sollen (während einer Aufnahme).
    fn sync_hotkey_pause(&mut self) {
        let want = self.recorder.active.is_some();
        if want != self.hotkeys_paused {
            self.engine.send(Command::SuspendHotkeys(want));
            self.hotkeys_paused = want;
        }
    }

    /// Speichert Änderungen nach kurzer Ruhezeit und überträgt sie an die Engine.
    fn commit_config(&mut self, ctx: &egui::Context) {
        if self.cfg == self.committed {
            self.dirty_since = None;
            return;
        }
        let now = Instant::now();
        let since = *self.dirty_since.get_or_insert(now);
        let waited = now.duration_since(since);
        if waited < SAVE_DELAY {
            ctx.request_repaint_after(SAVE_DELAY - waited);
            return;
        }
        self.cfg.normalize();
        self.save_error = config::save(&self.cfg).err().map(|e| tf("status.save_failed", &[&e]));
        self.engine.send(Command::UpdateConfig(Box::new(self.cfg.clone())));
        self.committed = self.cfg.clone();
        self.dirty_since = None;
    }

    /// Tableiste unter der Titelleiste: Tabs links, Profil-Schnellwahl und häufige Aktionen rechts.
    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (tab, key) in [
                (Tab::Dashboard, "tab.dashboard"),
                (Tab::Layouts, "tab.layouts"),
                (Tab::Preview, "tab.preview"),
                (Tab::Performance, "tab.performance"),
                (Tab::Hotkeys, "tab.hotkeys"),
                (Tab::Settings, "tab.settings"),
            ] {
                if theme::tab(ui, self.tab == tab, t(key)).clicked() {
                    self.tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if theme::accent_button(ui, t("top.apply_layout")).clicked() {
                    self.engine.send(Command::ApplyLayout);
                }
                let previews = if self.snap.previews_on {
                    t("top.previews_on")
                } else {
                    t("top.previews_off")
                };
                if ui.button(previews).clicked() {
                    self.engine.send(Command::TogglePreviews);
                }
                let names: Vec<String> = self.cfg.profiles.iter().map(|p| p.name.clone()).collect();
                egui::ComboBox::from_id_salt("quick_profile")
                    .selected_text(self.cfg.active_profile.clone())
                    .show_ui(ui, |ui| {
                        for n in names {
                            ui.selectable_value(&mut self.cfg.active_profile, n.clone(), n);
                        }
                    });
                ui.label(RichText::new(t("top.profile")).color(theme::TEXT_WEAK));
            });
        });
    }

    /// Fußzeile: letzte Meldung bzw. Speicherfehler, Speicherstatus und Copyright.
    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(err) = &self.save_error {
                ui.colored_label(ERROR, RichText::new(err).small());
            } else if let Some(last) = self.log.back() {
                ui.label(RichText::new(last).small().color(theme::TEXT_WEAK));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(COPYRIGHT).small().color(ACCENT.gamma_multiply(0.8)));
                ui.label(RichText::new("·").small().color(theme::TEXT_WEAK));
                let state = if self.dirty_since.is_some() {
                    t("status.saving")
                } else {
                    t("status.saved")
                };
                ui.label(RichText::new(state).small().color(theme::TEXT_WEAK))
                    .on_hover_text(config::config_path().display().to_string());
            });
        });
    }
}

impl eframe::App for App {
    /// Läuft vor jedem Frame – auch wenn das Dashboard im Tray versteckt ist:
    /// Engine-Ereignisse, Tray-Befehle, Schließen abfangen, Speichern.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_language(ctx);
        self.process_events(ctx);
        self.hide_on_start(ctx);
        self.handle_close(ctx);
        self.refresh_snapshot();
        self.sync_hotkey_pause();
        self.commit_config(ctx);
    }

    /// Zeichnet einen Frame: eigene Titelleiste, Tableiste, Tab-Inhalt, Statusleiste,
    /// Wasserzeichen sowie Fensterrand zum Größe-Ziehen.
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let bar_frame = egui::Frame::new().fill(theme::BG0);
        let title = egui::Panel::top("title_bar")
            .exact_size(theme::title_bar_height())
            .show_separator_line(false)
            .frame(bar_frame)
            .show(ui, |ui| theme::title_bar(ui, &self.icon))
            .inner;
        if title.close {
            // Wie das X des Systemrahmens – `handle_close` blendet ggf. ins Tray aus.
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        egui::Panel::top("top_bar")
            .show_separator_line(false)
            .frame(bar_frame.inner_margin(Margin {
                left: 10,
                right: 12,
                top: 0,
                bottom: 6,
            }))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .show_separator_line(false)
            .frame(bar_frame.inner_margin(Margin::symmetric(12, 5)))
            .show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BG1).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                let area = ui.max_rect();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Dashboard => dashboard::show(self, ui),
                        Tab::Layouts => layouts::show(self, ui),
                        Tab::Preview => preview_tab::show(self, ui),
                        Tab::Performance => performance::show(self, ui),
                        Tab::Hotkeys => hotkeys::show(self, ui),
                        Tab::Settings => settings::show(self, ui),
                    });
                // Dezentes Wasserzeichen unten rechts.
                ui.painter().text(
                    area.right_bottom() + vec2(4.0, 10.0),
                    Align2::RIGHT_BOTTOM,
                    t("watermark"),
                    FontId::proportional(13.0),
                    Color32::from_white_alpha(22),
                );
            });
        theme::window_edges(ui.ctx());
    }
}

impl Drop for App {
    /// Engine sauber beenden (setzt Priorität/Affinität der Clients zurück, entfernt das Tray-Symbol).
    fn drop(&mut self) {
        self.engine.shutdown();
    }
}

// -------------------------------------------------------------------- Bausteine

/// Lädt eine chinesische Systemschrift als Rückfallschrift. Liefert `false`, wenn keine gefunden wurde.
fn load_cjk_font(ctx: &egui::Context) -> bool {
    let Some(bytes) = platform::cjk_font_candidates()
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    else {
        return false;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);
    true
}

/// Formatiert Bytes als „512 MB“ bzw. „1.25 GB“.
fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Überschrift eines Abschnitts (mit Akzentstrich).
fn section(ui: &mut Ui, title: &str) {
    theme::section(ui, title);
}

/// Kleiner, gedimmter Erklärtext.
fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).small().color(theme::TEXT_WEAK));
}

/// Auswahl einer Prioritätsstufe.
fn priority_combo(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut Priority) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for p in Priority::ALL {
                ui.selectable_value(value, p, p.label());
            }
        });
}

/// Auswahl einer optionalen Prioritätsstufe („Standard“ = keine eigene Vorgabe).
fn opt_priority_combo(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut Option<Priority>) {
    let text = value.map_or(t("prio.default"), Priority::label);
    egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        ui.selectable_value(value, None, t("prio.default"));
        for p in Priority::ALL {
            ui.selectable_value(value, Some(p), p.label());
        }
    });
}

/// Kern-Auswahl als Raster von Checkboxen (leere Liste = alle Kerne).
fn cores_editor(ui: &mut Ui, cores: &mut Vec<u32>, count: u32) {
    let count = count.max(1);
    ui.horizontal(|ui| {
        if ui.small_button(t("cores.all")).clicked() {
            cores.clear();
        }
        if ui
            .small_button(t("cores.even"))
            .on_hover_text(t("cores.even_hint"))
            .clicked()
        {
            *cores = (0..count).step_by(2).collect();
        }
        if ui.small_button(t("cores.first_half")).clicked() {
            *cores = (0..count.div_ceil(2)).collect();
        }
        if ui.small_button(t("cores.second_half")).clicked() {
            *cores = (count / 2..count).collect();
        }
    });
    egui::Grid::new(ui.next_auto_id()).spacing([6.0, 2.0]).show(ui, |ui| {
        for c in 0..count {
            let mut on = cores.is_empty() || cores.contains(&c);
            if ui.checkbox(&mut on, format!("{c}")).changed() {
                if cores.is_empty() {
                    *cores = (0..count).collect();
                }
                if on {
                    cores.push(c);
                } else {
                    cores.retain(|&x| x != c);
                }
                cores.sort_unstable();
                cores.dedup();
                // Alle ausgewählt → als „alle“ speichern; keiner → ebenfalls „alle“ (ungültig).
                if cores.len() as u32 >= count || cores.is_empty() {
                    cores.clear();
                }
            }
            if (c + 1) % 8 == 0 {
                ui.end_row();
            }
        }
    });
}

/// Eingabe eines Rechtecks (X, Y, Breite, Höhe).
fn rect_editor(ui: &mut Ui, r: &mut Rect) {
    ui.label("X");
    ui.add(egui::DragValue::new(&mut r.x).speed(2));
    ui.label("Y");
    ui.add(egui::DragValue::new(&mut r.y).speed(2));
    ui.label(t("rect.w"));
    ui.add(egui::DragValue::new(&mut r.w).speed(2).range(0..=16384));
    ui.label(t("rect.h"));
    ui.add(egui::DragValue::new(&mut r.h).speed(2).range(0..=16384));
}

/// Macht aus einem egui-Tastendruck einen Hotkey-Text wie „Ctrl+Shift+F1“ (None = nicht belegbar).
fn combo_text(key: egui::Key, mods: egui::Modifiers) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    let name = key.name();
    parts.push(name);
    let text = parts.join("+");
    // Nur übernehmen, was der Parser (und damit Windows) auch belegen kann.
    hotkey::parse(&text).ok().flatten().map(|hk| hk.to_string())
}

/// Hotkey-Feld: Textfeld mit sofortiger Prüfung plus Knopf „Aufnehmen“.
/// Während der Aufnahme wird der nächste Tastendruck übernommen
/// (Esc = abbrechen, Entf/Backspace = Hotkey löschen).
fn hotkey_field(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    text: &mut String,
    width: f32,
    rec: &mut Recorder,
) {
    let id = ui.make_persistent_id(id_salt);
    ui.horizontal(|ui| {
        if rec.active == Some(id) {
            let button = egui::Button::new(RichText::new(t("hk.recording")).color(ACCENT)).min_size(vec2(width, 0.0));
            let resp = ui.add(button);
            let events = ui.input(|i| i.events.clone());
            for ev in events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                {
                    match key {
                        egui::Key::Escape => rec.active = None,
                        egui::Key::Delete | egui::Key::Backspace => {
                            text.clear();
                            rec.active = None;
                        }
                        _ => {
                            if let Some(combo) = combo_text(key, modifiers) {
                                *text = combo;
                                rec.active = None;
                            }
                        }
                    }
                }
            }
            if resp.clicked() || resp.clicked_elsewhere() {
                rec.active = None;
            }
            ui.ctx().request_repaint();
            return;
        }

        let parsed = hotkey::parse(text);
        let mut edit = egui::TextEdit::singleline(text)
            .desired_width(width)
            .hint_text(t("hk.field_hint"));
        if parsed.is_err() {
            edit = edit.text_color(ERROR);
        }
        let resp = ui.add(edit);
        match parsed {
            Err(e) => {
                resp.on_hover_text(e);
            }
            Ok(Some(hk)) => {
                resp.on_hover_text(tf("hk.recognized", &[&hk]));
            }
            Ok(None) => {
                resp.on_hover_text(t("hk.empty"));
            }
        }
        if ui
            .small_button(t("hk.record"))
            .on_hover_text(t("hk.record_hint"))
            .clicked()
        {
            rec.active = Some(id);
        }
    });
}
