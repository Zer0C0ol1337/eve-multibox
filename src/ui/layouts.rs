//! Layouts-Tab: Profile verwalten, Haupt-/Nebenfenster-Plätze bearbeiten,
//! Monitor-Vorlagen erzeugen, aktuelle Positionen erfassen und Charakter-Regeln pflegen.

use eframe::egui::{self, Align2, Color32, FontId, Rect as ERect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use evemb::config::{ClientRule, Profile, Rect, cores_label};
use evemb::i18n::{t, tf};
use evemb::layout;

use super::{ACCENT, App, cores_editor, hint, hotkey_field, opt_priority_combo, rect_editor, section};
use crate::engine::Command;
use crate::platform::{self, MonitorInfo};

/// Art der Monitor-Vorlage.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetKind {
    /// Alle Clients in voller Monitorgröße übereinander, Umschalten per Vorschau/Hotkey.
    #[default]
    Stacked,
    /// Hauptfenster auf einem Monitor, Nebenfenster als Raster (gleicher oder anderer Monitor).
    MainPlusGrid,
    /// Hauptfenster und Seitenleiste mit Nebenfenstern auf demselben Monitor.
    Sidebar,
}

/// Eingaben der Vorlagen-Werkzeuge (werden nicht gespeichert).
pub struct LayoutUiState {
    kind: PresetKind,
    main_monitor: usize,
    grid_monitor: usize,
    cols: u32,
    rows: u32,
    side_count: u32,
    side_percent: u32,
    side_left: bool,
    stack_clients: u32,
    use_work_area: bool,
}

impl Default for LayoutUiState {
    /// Vorgaben: alle Clients gestapelt im Vollbild auf Monitor 1.
    fn default() -> Self {
        Self {
            kind: PresetKind::Stacked,
            main_monitor: 0,
            grid_monitor: 1,
            cols: 2,
            rows: 2,
            side_count: 3,
            side_percent: 25,
            side_left: false,
            stack_clients: 8,
            use_work_area: false,
        }
    }
}

/// Zeichnet den Layouts-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    profile_bar(app, ui);

    section(ui, t("lay.section_options"));
    let running: Vec<String> = app
        .snap
        .clients
        .iter()
        .filter(|c| c.logged_in)
        .map(|c| c.name.clone())
        .collect();
    {
        let p = app.cfg.active_profile_mut();
        ui.checkbox(&mut p.borderless, t("lay.borderless"))
            .on_hover_text(t("lay.borderless_hint"));
        ui.checkbox(&mut p.auto_apply, t("lay.auto_apply"));
        ui.checkbox(&mut p.swap_on_activate, t("lay.swap_on_activate"));
        ui.horizontal(|ui| {
            ui.label(t("lay.default_main"));
            ui.add(
                egui::TextEdit::singleline(&mut p.default_main)
                    .desired_width(180.0)
                    .hint_text(t("lay.character_name")),
            );
            ui.menu_button("⏷", |ui| {
                if ui.button(t("lay.fg_or_first")).clicked() {
                    p.default_main.clear();
                    ui.close();
                }
                for name in &running {
                    if ui.button(name).clicked() {
                        p.default_main = name.clone();
                        ui.close();
                    }
                }
            });
        });
    }

    section(ui, t("lay.section_overview"));
    layout_canvas(ui, &app.monitors, app.cfg.active_profile());
    ui.horizontal(|ui| {
        if ui
            .button(t("lay.capture"))
            .on_hover_text(t("lay.capture_hint"))
            .clicked()
        {
            app.engine.send(Command::CaptureLayout);
        }
        if ui.button(t("lay.apply_now")).clicked() {
            app.engine.send(Command::ApplyLayout);
        }
    });

    section(ui, t("lay.section_slots"));
    slots_editor(ui, app.cfg.active_profile_mut());

    section(ui, t("lay.section_presets"));
    presets(app, ui);

    section(ui, t("lay.section_rules"));
    hint(ui, t("lay.rules_hint"));
    rules_editor(app, ui, &running);
}

/// Profilauswahl, Anlegen, Duplizieren, Umbenennen und Löschen.
fn profile_bar(app: &mut App, ui: &mut Ui) {
    let cfg = &mut app.cfg;
    ui.horizontal(|ui| {
        ui.label(RichText::new(t("prof.profile")).strong());
        let names: Vec<String> = cfg.profiles.iter().map(|p| p.name.clone()).collect();
        egui::ComboBox::from_id_salt("profile_select")
            .selected_text(cfg.active_profile.clone())
            .show_ui(ui, |ui| {
                for n in &names {
                    ui.selectable_value(&mut cfg.active_profile, n.clone(), n);
                }
            });

        let mut name = cfg.active_profile.clone();
        let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(160.0));
        let trimmed = name.trim().to_string();
        if resp.changed() && !trimmed.is_empty() && !names.contains(&trimmed) {
            cfg.active_profile_mut().name = trimmed.clone();
            cfg.active_profile = trimmed;
        }
        resp.on_hover_text(t("prof.rename_hint"));

        if ui.button(t("prof.new")).clicked() {
            let name = unique_name(&names, t("prof.profile"));
            cfg.profiles.push(Profile {
                name: name.clone(),
                ..Default::default()
            });
            cfg.active_profile = name;
        }
        if ui.button(t("prof.duplicate")).clicked() {
            let mut copy = cfg.active_profile().clone();
            copy.name = unique_name(&names, &format!("{} {}", copy.name, t("prof.copy_suffix")));
            cfg.active_profile = copy.name.clone();
            cfg.profiles.push(copy);
        }
        let can_delete = cfg.profiles.len() > 1;
        if ui
            .add_enabled(can_delete, egui::Button::new(t("prof.delete")))
            .clicked()
        {
            let active = cfg.active_profile.clone();
            cfg.profiles.retain(|p| p.name != active);
            cfg.active_profile = cfg.profiles[0].name.clone();
        }
    });
}

/// Erzeugt einen noch nicht vergebenen Namen („Profil 2“, „Profil 3“ …).
fn unique_name(existing: &[String], base: &str) -> String {
    if !existing.iter().any(|n| n == base) {
        return base.to_string();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !existing.contains(n))
        .expect("unendlicher Iterator")
}

/// Rechteck-Editor für Haupt- und Nebenplätze.
fn slots_editor(ui: &mut Ui, p: &mut Profile) {
    egui::Grid::new("slots").spacing([8.0, 4.0]).show(ui, |ui| {
        ui.label(RichText::new(t("dash.main_marker")).color(ACCENT));
        ui.horizontal(|ui| rect_editor(ui, &mut p.main_slot));
        ui.end_row();
        let mut remove = None;
        for (i, slot) in p.secondary_slots.iter_mut().enumerate() {
            ui.label(tf("lay.secondary_n", &[&(i + 1)]));
            ui.horizontal(|ui| {
                rect_editor(ui, slot);
                if ui.small_button("×").clicked() {
                    remove = Some(i);
                }
            });
            ui.end_row();
        }
        if let Some(i) = remove {
            p.secondary_slots.remove(i);
        }
    });
    if ui.button(t("lay.add_slot")).clicked() {
        let next = p
            .secondary_slots
            .last()
            .map_or(Rect::new(0, 0, 960, 540), |r| Rect::new(r.x + 40, r.y + 40, r.w, r.h));
        p.secondary_slots.push(next);
    }
}

/// Vorlagen-Generator auf Basis der erkannten Monitore.
fn presets(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(tf("lay.monitors_found", &[&app.monitors.len()]));
        if ui.small_button(t("lay.rescan")).clicked() {
            app.monitors = platform::monitors();
        }
    });
    if app.monitors.is_empty() {
        return;
    }
    let s = &mut app.layout_ui;
    let labels: Vec<String> = app
        .monitors
        .iter()
        .enumerate()
        .map(|(i, m)| monitor_label(i, m))
        .collect();
    s.main_monitor = s.main_monitor.min(labels.len() - 1);
    s.grid_monitor = s.grid_monitor.min(labels.len() - 1);

    ui.horizontal_wrapped(|ui| {
        ui.radio_value(&mut s.kind, PresetKind::Stacked, t("lay.preset_stacked"));
        ui.radio_value(&mut s.kind, PresetKind::MainPlusGrid, t("lay.preset_grid"));
        ui.radio_value(&mut s.kind, PresetKind::Sidebar, t("lay.preset_sidebar"));
        ui.checkbox(&mut s.use_work_area, t("lay.exclude_taskbar"));
    });
    ui.horizontal(|ui| {
        ui.label(t("lay.main_on"));
        monitor_combo(ui, "preset_main_mon", &labels, &mut s.main_monitor);
        match s.kind {
            PresetKind::Stacked => {
                ui.label(t("lay.clients_max"));
                ui.add(egui::DragValue::new(&mut s.stack_clients).range(1..=20))
                    .on_hover_text(t("lay.clients_max_hint"));
            }
            PresetKind::MainPlusGrid => {
                ui.label(t("lay.grid_on"));
                monitor_combo(ui, "preset_grid_mon", &labels, &mut s.grid_monitor);
                ui.label(t("lay.cols"));
                ui.add(egui::DragValue::new(&mut s.cols).range(1..=6));
                ui.label(t("lay.rows"));
                ui.add(egui::DragValue::new(&mut s.rows).range(1..=6));
            }
            PresetKind::Sidebar => {
                ui.label(t("lay.secondaries"));
                ui.add(egui::DragValue::new(&mut s.side_count).range(1..=8));
                ui.label(t("lay.width_pct"));
                ui.add(egui::DragValue::new(&mut s.side_percent).range(10..=50));
                ui.checkbox(&mut s.side_left, t("lay.sidebar_left"));
            }
        }
    });
    if s.kind == PresetKind::Stacked {
        hint(ui, t("lay.stacked_hint"));
    }
    if s.kind == PresetKind::MainPlusGrid && s.main_monitor == s.grid_monitor {
        hint(ui, t("lay.same_monitor_hint"));
    }
    if ui.button(t("lay.apply_preset")).clicked() {
        let area = |i: usize| {
            let m = &app.monitors[i];
            if s.use_work_area { m.work } else { m.area }
        };
        let (main, slots) = match s.kind {
            PresetKind::Stacked => layout::preset_stacked(area(s.main_monitor), s.stack_clients),
            PresetKind::MainPlusGrid => {
                layout::preset_main_plus_grid(area(s.main_monitor), area(s.grid_monitor), s.cols, s.rows)
            }
            PresetKind::Sidebar => {
                layout::preset_main_with_sidebar(area(s.main_monitor), s.side_count, s.side_percent, s.side_left)
            }
        };
        let p = app.cfg.active_profile_mut();
        p.main_slot = main;
        p.secondary_slots = slots;
        if s.kind == PresetKind::Stacked {
            p.borderless = true;
        }
    }
}

/// Beschriftung eines Monitors, z. B. „1: 2560×1440 (Haupt)“.
fn monitor_label(i: usize, m: &MonitorInfo) -> String {
    let primary = if m.primary {
        format!(" {}", t("lay.primary"))
    } else {
        String::new()
    };
    format!("{}: {}×{}{}", i + 1, m.area.w, m.area.h, primary)
}

/// Auswahlliste der Monitore.
fn monitor_combo(ui: &mut Ui, id: &str, labels: &[String], value: &mut usize) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(labels[*value].clone())
        .show_ui(ui, |ui| {
            for (i, l) in labels.iter().enumerate() {
                ui.selectable_value(value, i, l);
            }
        });
}

/// Maßstäbliche Skizze: Monitore grau, Hauptplatz orange, Nebenplätze blau.
fn layout_canvas(ui: &mut Ui, monitors: &[MonitorInfo], p: &Profile) {
    let mut rects: Vec<Rect> = monitors.iter().map(|m| m.area).collect();
    rects.push(p.main_slot);
    rects.extend(p.secondary_slots.iter().copied());
    let rects: Vec<Rect> = rects.into_iter().filter(Rect::is_valid).collect();
    if rects.is_empty() {
        return;
    }
    let min_x = rects.iter().map(|r| r.x).min().unwrap_or(0) as f32;
    let min_y = rects.iter().map(|r| r.y).min().unwrap_or(0) as f32;
    let max_x = rects.iter().map(|r| r.right()).max().unwrap_or(1) as f32;
    let max_y = rects.iter().map(|r| r.bottom()).max().unwrap_or(1) as f32;
    let (bw, bh) = ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0));

    let width = ui.available_width().min(760.0);
    let scale = (width / bw).min(260.0 / bh);
    let (resp, painter) = ui.allocate_painter(vec2(bw * scale, bh * scale), Sense::hover());
    let origin = resp.rect.min;
    let map = |r: &Rect| {
        ERect::from_min_size(
            pos2(
                origin.x + (r.x as f32 - min_x) * scale,
                origin.y + (r.y as f32 - min_y) * scale,
            ),
            vec2(r.w as f32 * scale, r.h as f32 * scale),
        )
    };
    let small = FontId::proportional(11.0);

    for (i, m) in monitors.iter().enumerate() {
        let r = map(&m.area);
        painter.rect_filled(r, 2.0, Color32::from_gray(38));
        painter.rect_stroke(r, 2.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Inside);
        painter.text(
            r.left_bottom() + vec2(4.0, -3.0),
            Align2::LEFT_BOTTOM,
            monitor_label(i, m),
            small.clone(),
            Color32::from_gray(140),
        );
    }
    let blue = Color32::from_rgb(90, 150, 255);
    for (i, s) in p.secondary_slots.iter().enumerate().filter(|(_, s)| s.is_valid()) {
        let r = map(s).shrink(1.0);
        painter.rect_filled(r, 2.0, blue.gamma_multiply(0.25));
        painter.rect_stroke(r, 2.0, Stroke::new(1.0, blue), StrokeKind::Inside);
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            format!("{}", i + 1),
            small.clone(),
            Color32::WHITE,
        );
    }
    if p.main_slot.is_valid() {
        let r = map(&p.main_slot).shrink(1.0);
        painter.rect_filled(r, 2.0, ACCENT.gamma_multiply(0.25));
        painter.rect_stroke(r, 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            t("btn.main"),
            FontId::proportional(13.0),
            Color32::WHITE,
        );
    }
}

/// Tabelle der Charakter-Regeln: Reihenfolge, Name, Hotkey, Priorität, Kerne.
fn rules_editor(app: &mut App, ui: &mut Ui, running: &[String]) {
    let cores = app.snap.cores.max(1);
    let recorder = &mut app.recorder;
    let p = app.cfg.active_profile_mut();
    let mut action: Option<(usize, i32)> = None; // (Index, -1 hoch / +1 runter / 0 löschen)

    egui::Grid::new("rules")
        .striped(true)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            for key in ["col.character", "col.hotkey", "col.prio_active", "col.cores"] {
                ui.label(RichText::new(t(key)).strong());
            }
            ui.label("");
            ui.end_row();
            let n = p.clients.len();
            for (i, rule) in p.clients.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() {
                        action = Some((i, -1));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).clicked() {
                        action = Some((i, 1));
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut rule.character).desired_width(170.0));
                hotkey_field(ui, ("rule_hotkey", i), &mut rule.hotkey, 120.0, recorder);
                opt_priority_combo(ui, ("rule_prio", i), &mut rule.priority);
                ui.menu_button(cores_label(&rule.cores), |ui| {
                    ui.set_min_width(260.0);
                    cores_editor(ui, &mut rule.cores, cores);
                });
                if ui.small_button("×").clicked() {
                    action = Some((i, 0));
                }
                ui.end_row();
            }
        });

    match action {
        Some((i, 0)) => {
            p.clients.remove(i);
        }
        Some((i, d)) => {
            let j = (i as i32 + d) as usize;
            p.clients.swap(i, j);
        }
        None => {}
    }

    ui.horizontal(|ui| {
        if ui.button(t("lay.add_rule")).clicked() {
            p.clients.push(ClientRule::default());
        }
        let missing: Vec<&String> = running.iter().filter(|n| p.rule(n).is_none()).collect();
        if ui
            .add_enabled(
                !missing.is_empty(),
                egui::Button::new(tf("lay.add_logged_in", &[&missing.len()])),
            )
            .clicked()
        {
            for name in missing {
                p.clients.push(ClientRule {
                    character: name.clone(),
                    ..Default::default()
                });
            }
        }
    });
}
