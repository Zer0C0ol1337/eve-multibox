//! Einstellungen-Tab: Sprache, Anzeige-System (Linux), Tray-Verhalten, Autostart,
//! Dateipfade und „Über“ (Version, Copyright, Kontakt, Markenhinweis).

use eframe::egui::{self, RichText, Ui};
use evemb::config;
use evemb::i18n::{Lang, t, tf};

use super::{ACCENT, App, COPYRIGHT, CjkFont, ERROR, WARN, hint, section};
use crate::{diag, platform};

/// Zeichnet den Einstellungen-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    section(ui, t("set.section_general"));
    ui.horizontal(|ui| {
        ui.label(t("set.language"));
        let g = &mut app.cfg.general;
        let current = g.language.map_or(t("set.lang_auto"), Lang::native_name);
        egui::ComboBox::from_id_salt("language")
            .selected_text(current)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut g.language, None, t("set.lang_auto"));
                for lang in Lang::ALL {
                    ui.selectable_value(&mut g.language, Some(lang), lang.native_name());
                }
            });
    });
    if app.cjk_font == CjkFont::Missing {
        ui.colored_label(WARN, t(platform::FONT_MISSING));
    }

    if let Some((key, warning)) = platform::display_note() {
        section(ui, t("set.section_display"));
        if warning {
            ui.colored_label(WARN, t(key));
        } else {
            ui.label(t(key));
        }
    }

    section(ui, t("set.section_tray"));
    let g = &mut app.cfg.general;
    ui.checkbox(&mut g.close_to_tray, t("set.close_to_tray"));
    ui.checkbox(&mut g.start_minimized, t("set.start_minimized"));
    hint(ui, t("set.tray_hint"));
    if !platform::tray_available() {
        ui.colored_label(WARN, t("set.no_tray"));
    }

    section(ui, t("set.section_autostart"));
    let mut enabled = app.autostart;
    if ui.checkbox(&mut enabled, t(platform::AUTOSTART_LABEL)).changed() {
        match platform::autostart::set(enabled) {
            Ok(()) => app.autostart_error = None,
            Err(e) => app.autostart_error = Some(tf("set.autostart_failed", &[&e])),
        }
        app.autostart = platform::autostart::is_enabled();
    }
    hint(ui, t(platform::AUTOSTART_HINT));
    if let Some(err) = &app.autostart_error {
        ui.colored_label(ERROR, err);
    }

    section(ui, t("set.section_files"));
    let cfg_path = config::config_path();
    egui::Grid::new("files").spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(t("set.config_file"));
        ui.label(RichText::new(cfg_path.display().to_string()).monospace().small());
        ui.end_row();
        ui.label(t("set.log_file"));
        ui.label(
            RichText::new(diag::log_path().display().to_string())
                .monospace()
                .small(),
        );
        ui.end_row();
    });
    if ui.button(t("set.open_folder")).clicked()
        && let Some(dir) = cfg_path.parent()
    {
        platform::open_folder(dir);
    }

    section(ui, t("set.section_about"));
    ui.label(RichText::new("EVE Multibox").strong().size(20.0).color(ACCENT));
    ui.label(tf("set.version", &[&env!("CARGO_PKG_VERSION")]));
    ui.label(t("set.about_text"));
    ui.add_space(6.0);
    ui.label(RichText::new(COPYRIGHT).strong());
    ui.label(t("set.contact"));
    ui.add_space(6.0);
    hint(ui, t("set.ccp"));
}
