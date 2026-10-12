//! Leistungs-Tab: Drosselung inaktiver Clients (Priorität, Hintergrund-Kerne,
//! Minimieren) sowie Speicher-Warnungen und automatisches Freigeben.

use eframe::egui::{self, RichText, Ui};
use evemb::config::cores_label;
use evemb::i18n::t;

use super::{ACCENT, App, WARN, cores_editor, hint, priority_combo, section};
use crate::engine::Command;
use crate::platform;

/// Zeichnet den Leistungs-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    let cores = app.snap.cores.max(1);
    let p = &mut app.cfg.performance;

    process_rights(ui);

    section(ui, t("perf.section_cpu"));
    ui.checkbox(&mut p.throttle_inactive, t("perf.throttle"));
    hint(ui, t("perf.throttle_hint"));
    ui.add_enabled_ui(p.throttle_inactive, |ui| {
        egui::Grid::new("throttle").spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t("perf.active_prio"));
            priority_combo(ui, "active_prio", &mut p.active_priority);
            ui.end_row();
            ui.label(t("perf.inactive_prio"));
            priority_combo(ui, "inactive_prio", &mut p.inactive_priority);
            ui.end_row();
            ui.label(t("perf.bg_cores"));
            ui.menu_button(cores_label(&p.background_cores), |ui| {
                ui.set_min_width(260.0);
                cores_editor(ui, &mut p.background_cores, cores);
            });
            ui.end_row();
        });
        hint(ui, t("perf.bg_cores_hint"));
    });
    hint(ui, t("perf.rules_hint"));

    ui.add_space(6.0);
    ui.checkbox(&mut p.minimize_inactive, t("perf.minimize"));
    if p.minimize_inactive {
        ui.label(RichText::new(t("perf.minimize_warn")).color(WARN).small());
    }

    section(ui, t("perf.section_ram"));
    egui::Grid::new("ram").spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(t("perf.ram_warn"));
        ui.add(
            egui::Slider::new(&mut p.ram_warn_mb, 0..=8192)
                .step_by(100.0)
                .suffix(" MB"),
        )
        .on_hover_text(t("perf.ram_warn_hint"));
        ui.end_row();
        ui.label(t("perf.sys_warn"));
        ui.add(egui::Slider::new(&mut p.system_ram_warn_percent, 0..=50).suffix(" %"));
        ui.end_row();
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.checkbox(&mut p.auto_trim, t("perf.auto_trim"));
        ui.add_enabled(
            p.auto_trim,
            egui::DragValue::new(&mut p.auto_trim_minutes)
                .range(1..=240)
                .suffix(" min"),
        );
    });
    if ui.button(t("perf.trim_now")).clicked() {
        app.engine.send(Command::TrimBackground);
    }
    hint(ui, t(platform::TRIM_HINT));
}

/// Linux: Hinweis, ob erweiterte Prozessrechte (CAP_SYS_NICE) vorhanden sind,
/// samt kopierbarem Befehl zum Nachrüsten. Unter Windows wird nichts angezeigt.
fn process_rights(ui: &mut Ui) {
    if !platform::show_process_rights() {
        return;
    }
    ui.add_space(6.0);
    match platform::process_rights_fix() {
        None => {
            ui.label(RichText::new(format!("✔ {}", t("perf.rights_ok"))).color(ACCENT));
        }
        Some(cmd) => {
            ui.colored_label(WARN, t("perf.rights_missing"));
            ui.horizontal(|ui| {
                ui.label(RichText::new(&cmd).monospace());
                if ui.small_button(t("btn.copy")).clicked() {
                    ui.ctx().copy_text(cmd.clone());
                }
            });
        }
    }
}
