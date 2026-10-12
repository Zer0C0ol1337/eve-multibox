//! Dashboard: System-Übersicht, Warnungen und Tabelle aller Clients mit
//! CPU-Verlauf, Arbeitsspeicher, Priorität, Kernen und Schnellaktionen.

use std::collections::VecDeque;

use eframe::egui::{self, Color32, RichText, Sense, Stroke, Ui, pos2, vec2};
use evemb::config::cores_label;
use evemb::i18n::{t, tf};

use super::{ACCENT, App, ERROR, HISTORY_LEN, WARN, format_bytes, hint, theme};
use crate::engine::{ClientInfo, Command};

/// Zeichnet den Dashboard-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    let snap = app.snap.clone();
    summary(app, ui);

    if !snap.warnings.is_empty() || !snap.hotkey_errors.is_empty() {
        ui.add_space(10.0);
        theme::card()
            .fill(Color32::from_rgb(38, 30, 14))
            .stroke(Stroke::new(1.0, Color32::from_rgb(92, 70, 20)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for w in &snap.warnings {
                    ui.colored_label(WARN, format!("⚠ {w}"));
                }
                if !snap.hotkey_errors.is_empty() {
                    ui.colored_label(
                        ERROR,
                        format!("⚠ {}", tf("dash.warn_hotkeys", &[&snap.hotkey_errors.len()])),
                    );
                }
            });
    }

    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(t("dash.clients"))
                .strong()
                .size(16.0)
                .color(Color32::WHITE),
        );
        ui.label(RichText::new(format!("{}", snap.clients.len())).color(theme::TEXT_WEAK));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(t("dash.trim_bg"))
                .on_hover_text(t("dash.trim_bg_hint"))
                .clicked()
            {
                app.engine.send(Command::TrimBackground);
            }
            if ui.button(t("top.apply_layout")).clicked() {
                app.engine.send(Command::ApplyLayout);
            }
        });
    });
    ui.add_space(6.0);

    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        if snap.clients.is_empty() {
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(t("dash.no_clients")).size(16.0).color(Color32::WHITE));
                ui.add_space(4.0);
                hint(ui, t("dash.no_clients_hint"));
            });
            ui.add_space(18.0);
        } else {
            client_table(app, ui, &snap.clients);
        }
    });
}

/// Kopfbereich mit System-CPU, System-RAM und EVE-Summen als vier gleich breite Karten.
fn summary(app: &App, ui: &mut Ui) {
    let snap = &app.snap;
    let eve_cpu: f32 = snap.clients.iter().map(|c| c.cpu).sum();
    let eve_ram: u64 = snap.clients.iter().map(|c| c.mem.working_set).sum();
    let used = snap.ram_total.saturating_sub(snap.ram_avail);
    let ram_frac = if snap.ram_total > 0 {
        used as f32 / snap.ram_total as f32
    } else {
        0.0
    };
    let logged_in = snap.clients.iter().filter(|c| c.logged_in).count();

    ui.columns(4, |cols| {
        stat_card(
            &mut cols[0],
            t("dash.sys_cpu"),
            &format!("{:.0} %", snap.sys_cpu),
            Some(snap.sys_cpu / 100.0),
            &tf("dash.cores", &[&snap.cores]),
        );
        stat_card(
            &mut cols[1],
            t("dash.sys_ram"),
            &format!("{} / {}", format_bytes(used), format_bytes(snap.ram_total)),
            Some(ram_frac),
            &tf("dash.free", &[&format_bytes(snap.ram_avail)]),
        );
        stat_card(
            &mut cols[2],
            t("dash.eve_total"),
            &format!("{eve_cpu:.1} % {}", t("col.cpu")),
            None,
            &tf("dash.working_set", &[&format_bytes(eve_ram)]),
        );
        stat_card(
            &mut cols[3],
            t("dash.clients"),
            &format!("{}", snap.clients.len()),
            None,
            &tf("dash.logged_in", &[&logged_in]),
        );
    });
}

/// Eine Kennzahl-Karte mit optionalem Füllbalken.
fn stat_card(ui: &mut Ui, title: &str, value: &str, bar: Option<f32>, sub: &str) {
    theme::card().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 5.0;
        ui.label(RichText::new(title.to_uppercase()).size(11.0).color(theme::TEXT_WEAK));
        ui.label(RichText::new(value).strong().size(21.0).color(Color32::WHITE));
        match bar {
            Some(f) => {
                let color = if f > 0.9 {
                    ERROR
                } else if f > 0.75 {
                    WARN
                } else {
                    ACCENT
                };
                theme::meter(ui, f, color);
            }
            None => {
                ui.add_space(6.0);
            }
        }
        ui.label(RichText::new(sub).small().color(theme::TEXT_WEAK));
    });
}

/// Tabelle aller Clients mit Aktionen je Zeile.
fn client_table(app: &mut App, ui: &mut Ui, clients: &[ClientInfo]) {
    let warn_bytes = app.cfg.performance.ram_warn_mb * 1024 * 1024;
    egui::Grid::new("clients")
        .striped(true)
        .spacing([18.0, 9.0])
        .min_row_height(26.0)
        .show(ui, |ui| {
            ui.label("");
            for key in [
                "col.character",
                "col.status",
                "col.cpu",
                "col.history",
                "col.ram",
                "col.private",
                "col.priority",
                "col.cores",
            ] {
                ui.label(RichText::new(t(key).to_uppercase()).size(11.0).color(theme::TEXT_WEAK));
            }
            ui.label("");
            ui.end_row();

            for c in clients {
                main_marker(ui, c.main);
                let name = RichText::new(&c.name).size(14.5);
                ui.label(if c.foreground {
                    name.strong().color(ACCENT)
                } else {
                    name.color(Color32::WHITE)
                })
                .on_hover_text(format!("PID {}", c.pid));
                status_label(ui, c);
                ui.label(format!("{:.1} %", c.cpu));
                let empty = VecDeque::new();
                sparkline(ui, app.history.get(&c.hwnd).unwrap_or(&empty));
                let ram = RichText::new(format_bytes(c.mem.working_set));
                ui.label(if warn_bytes > 0 && c.mem.working_set > warn_bytes {
                    ram.color(WARN)
                } else {
                    ram
                });
                ui.label(RichText::new(format_bytes(c.mem.private)).color(theme::TEXT_WEAK));
                ui.label(c.priority.map_or("?", |p| p.label()));
                ui.label(RichText::new(cores_label(&c.cores)).color(theme::TEXT_WEAK));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.small_button(t("btn.focus")).clicked() {
                        app.engine.send(Command::Activate(c.hwnd));
                    }
                    if ui
                        .small_button(t("btn.main"))
                        .on_hover_text(t("btn.main_hint"))
                        .clicked()
                    {
                        app.engine.send(Command::MakeMain(c.hwnd));
                    }
                    if ui
                        .small_button(t("btn.trim"))
                        .on_hover_text(t("btn.trim_hint"))
                        .clicked()
                    {
                        app.engine.send(Command::TrimClient(c.hwnd));
                    }
                });
                ui.end_row();
            }
        });
}

/// Status eines Clients als farbige Plakette (aktiv, Hintergrund, gedrosselt, minimiert, Login).
fn status_label(ui: &mut Ui, c: &ClientInfo) {
    let (key, color) = if c.limited {
        ("status.no_rights", ERROR)
    } else if !c.logged_in {
        ("status.login", Color32::from_rgb(150, 158, 170))
    } else if c.foreground {
        ("status.active", ACCENT)
    } else if c.minimized {
        ("status.minimized", Color32::from_rgb(150, 158, 170))
    } else if c.throttled {
        ("status.throttled", theme::BLUE)
    } else {
        ("status.background", theme::GREEN)
    };
    theme::badge(ui, t(key), color);
}

/// Mini-Liniendiagramm des CPU-Verlaufs (Skala passt sich dem Maximum an).
fn sparkline(ui: &mut Ui, data: &VecDeque<f32>) {
    let (resp, painter) = ui.allocate_painter(vec2(120.0, 22.0), Sense::hover());
    let r = resp.rect;
    painter.rect_filled(r, 4.0, ui.visuals().extreme_bg_color);
    if data.len() < 2 {
        return;
    }
    let peak = data.iter().copied().fold(0.0f32, f32::max);
    let max = (peak * 1.2).max(5.0);
    let step = r.width() / (HISTORY_LEN - 1) as f32;
    let n = data.len();
    let points: Vec<_> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            pos2(
                r.right() - (n - 1 - i) as f32 * step,
                r.bottom() - (v / max).clamp(0.0, 1.0) * r.height(),
            )
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, ACCENT)));
    resp.on_hover_text(tf("dash.spark_hint", &[&format!("{peak:.1}"), &format!("{max:.0}")]));
}

/// Markierung des Hauptfensters als gezeichneter Punkt (unabhängig von der Schriftart).
fn main_marker(ui: &mut Ui, is_main: bool) {
    let (resp, painter) = ui.allocate_painter(vec2(12.0, 12.0), Sense::hover());
    if is_main {
        painter.circle_filled(resp.rect.center(), 4.5, ACCENT);
        resp.on_hover_text(t("dash.main_marker"));
    }
}
