//! Vorschau-Tab: Standardgröße, Deckkraft, Anzeigeoptionen, automatische Anordnung,
//! Einrasten sowie gespeicherte Positionen und Größen der Live-Vorschaufenster.

use eframe::egui::{self, Ui};
use evemb::i18n::{t, tf};

use super::{App, hint, section};
use crate::engine::Command;

/// Zeichnet den Vorschau-Tab.
pub fn show(app: &mut App, ui: &mut Ui) {
    let previews_on = app.snap.previews_on;
    let s = &mut app.cfg.preview;

    section(ui, t("pv.section_live"));
    ui.checkbox(&mut s.enabled, t("pv.enabled"));
    ui.horizontal(|ui| {
        hint(
            ui,
            if previews_on {
                t("pv.runtime_on")
            } else {
                t("pv.runtime_off")
            },
        );
        if ui.small_button(t("pv.toggle")).clicked() {
            app.engine.send(Command::TogglePreviews);
        }
    });
    hint(ui, t("pv.usage"));

    section(ui, t("pv.section_size"));
    hint(ui, t("pv.size_hint"));
    egui::Grid::new("preview_grid").spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(t("pv.width"));
        ui.add(egui::Slider::new(&mut s.width, 64..=960).suffix(" px"));
        ui.end_row();
        ui.label(t("pv.height"));
        ui.add(egui::Slider::new(&mut s.height, 36..=540).suffix(" px"));
        ui.end_row();
        ui.label("");
        ui.horizontal(|ui| {
            for (label, w, h) in [
                ("16:9 mini", 192, 108),
                ("16:9 S", 256, 144),
                ("16:9 M", 320, 180),
                ("16:9 L", 480, 270),
                ("16:10", 320, 200),
            ] {
                if ui.small_button(label).clicked() {
                    s.width = w;
                    s.height = h;
                }
            }
        });
        ui.end_row();
        ui.label(t("pv.opacity"));
        let mut pct = (s.opacity as f32 / 255.0 * 100.0).round() as u32;
        if ui.add(egui::Slider::new(&mut pct, 15..=100).suffix(" %")).changed() {
            s.opacity = (pct as f32 / 100.0 * 255.0).round() as u8;
        }
        ui.end_row();
    });

    section(ui, t("pv.section_display"));
    ui.checkbox(&mut s.show_titles, t("pv.show_titles"));
    ui.checkbox(&mut s.highlight_active, t("pv.highlight"));
    ui.checkbox(&mut s.hide_active, t("pv.hide_active"));
    ui.checkbox(&mut s.only_when_eve_focused, t("pv.only_eve"));

    section(ui, t("pv.section_arrange"));
    ui.checkbox(&mut s.snap, t("pv.snap"));
    egui::Grid::new("preview_arrange").spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label(t("pv.snap_distance"));
        ui.add_enabled(s.snap, egui::Slider::new(&mut s.snap_distance, 2..=60).suffix(" px"));
        ui.end_row();
        ui.label(t("pv.gap"));
        ui.add(egui::Slider::new(&mut s.gap, 0..=40).suffix(" px"));
        ui.end_row();
        ui.label(t("pv.auto_arrange"));
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.vertical, false, t("pv.as_row"));
            ui.radio_value(&mut s.vertical, true, t("pv.as_column"));
        });
        ui.end_row();
        ui.label(t("pv.wrap_after"));
        ui.add(
            egui::DragValue::new(&mut s.per_line)
                .range(0..=20)
                .suffix(t("pv.previews_suffix")),
        )
        .on_hover_text(t("pv.wrap_hint"));
        ui.end_row();
        ui.label(t("pv.start"));
        ui.horizontal(|ui| {
            ui.label("X");
            ui.add(egui::DragValue::new(&mut s.start_x).speed(2));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut s.start_y).speed(2));
        });
        ui.end_row();
    });
    hint(ui, t("pv.auto_hint"));
    ui.horizontal(|ui| {
        if ui
            .button(t("pv.rearrange"))
            .on_hover_text(t("pv.rearrange_hint"))
            .clicked()
        {
            s.positions.clear();
        }
        if ui
            .add_enabled(!s.sizes.is_empty(), egui::Button::new(t("pv.reset_sizes")))
            .on_hover_text(t("pv.reset_sizes_hint"))
            .clicked()
        {
            s.sizes.clear();
        }
    });

    section(ui, t("pv.section_saved"));
    let mut names: Vec<String> = s.positions.keys().chain(s.sizes.keys()).cloned().collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        hint(ui, t("pv.none_saved"));
    }
    let mut reset = None;
    egui::Grid::new("preview_positions")
        .striped(true)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for name in &names {
                ui.label(name);
                ui.label(
                    s.positions
                        .get(name)
                        .map_or("auto".to_string(), |(x, y)| tf("pv.pos", &[x, y])),
                );
                ui.label(
                    s.sizes
                        .get(name)
                        .map_or(t("pv.section_size").to_string(), |(w, h)| format!("{w}×{h}")),
                );
                if ui.small_button(t("pv.reset")).clicked() {
                    reset = Some(name.clone());
                }
                ui.end_row();
            }
        });
    if let Some(name) = reset {
        s.positions.remove(&name);
        s.sizes.remove(&name);
    }

    ui.add_space(10.0);
    hint(ui, t("pv.minimized_note"));
}
