//! Hotkeys-Tab: globale Hotkeys bearbeiten (eintippen oder aufnehmen), Charakter-Hotkeys
//! anzeigen, Registrierungsfehler melden und die erlaubte Funktionsweise erklären.

use eframe::egui::{self, RichText, Ui};
use evemb::i18n::t;

use super::{App, ERROR, hint, hotkey_field, section};

/// Zeichnet den Hotkeys-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    let errors = app.snap.hotkey_errors.clone();
    let recorder = &mut app.recorder;

    section(ui, t("hkt.section_global"));
    let h = &mut app.cfg.hotkeys;
    ui.checkbox(&mut h.only_when_eve_focused, t("hkt.only_eve"));
    hint(ui, t("hkt.only_eve_hint"));
    ui.add_space(4.0);
    egui::Grid::new("global_hotkeys").spacing([12.0, 6.0]).show(ui, |ui| {
        for (id, label, value, help) in [
            ("hk_next", "hk.next", &mut h.cycle_next, "hkt.help_next"),
            ("hk_prev", "hk.prev", &mut h.cycle_prev, "hkt.help_prev"),
            ("hk_swap", "hk.swap", &mut h.swap_main, "hkt.help_swap"),
            ("hk_layout", "top.apply_layout", &mut h.apply_layout, "hkt.help_layout"),
            ("hk_toggle", "hk.toggle", &mut h.toggle_previews, "hkt.help_toggle"),
        ] {
            ui.label(t(label));
            hotkey_field(ui, id, value, 160.0, recorder);
            ui.label(RichText::new(t(help)).weak().small());
            ui.end_row();
        }
    });

    section(ui, t("hkt.section_chars"));
    let rules: Vec<_> = app
        .cfg
        .active_profile()
        .clients
        .iter()
        .filter(|r| !r.hotkey.trim().is_empty())
        .cloned()
        .collect();
    if rules.is_empty() {
        hint(ui, t("hkt.none_chars"));
    } else {
        egui::Grid::new("char_hotkeys").striped(true).show(ui, |ui| {
            for r in rules {
                ui.label(r.character);
                ui.label(RichText::new(r.hotkey).monospace());
                ui.end_row();
            }
        });
    }

    if !errors.is_empty() {
        section(ui, t("hkt.section_problems"));
        for e in errors {
            ui.colored_label(ERROR, format!("⚠ {e}"));
        }
    }

    section(ui, t("hkt.section_syntax"));
    hint(ui, t("hkt.syntax"));
    hint(ui, t("hkt.examples"));

    section(ui, t("hkt.section_compliant"));
    hint(ui, t("hkt.compliant"));
}
