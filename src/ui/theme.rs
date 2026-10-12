//! Erscheinungsbild der Oberfläche: Farbpalette, egui-Stil, eigene Titelleiste (statt des
//! Systemrahmens), Fenster-Ränder zum Größe-Ziehen und kleine gezeichnete Bausteine
//! (Karten, Tabs, Akzent-Knöpfe, Plaketten, Balken).
//!
//! Alle Symbole (Minimieren, Maximieren, Schließen, Punkte) werden als Vektoren gezeichnet –
//! so sehen sie auf jedem System gleich aus und hängen nicht von vorhandenen Schriftzeichen ab.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Margin, Pos2, Rect, Response, RichText,
    Sense, Shadow, Stroke, StrokeKind, TextStyle, Ui, ViewportCommand, pos2, vec2,
};
use egui::viewport::ResizeDirection;

/// Hintergrund von Titelleiste, Tableiste und Statusleiste.
pub const BG0: Color32 = Color32::from_rgb(10, 13, 18);
/// Hintergrund des Inhaltsbereichs.
pub const BG1: Color32 = Color32::from_rgb(16, 20, 26);
/// Kartenfläche.
pub const CARD: Color32 = Color32::from_rgb(22, 27, 34);
/// Hervorgehobene Fläche (Knöpfe, Eingabefelder).
pub const RAISED: Color32 = Color32::from_rgb(30, 37, 47);
/// Feine Linien und Ränder.
pub const LINE: Color32 = Color32::from_rgb(40, 47, 58);
/// Normale Schrift.
pub const TEXT: Color32 = Color32::from_rgb(214, 220, 228);
/// Gedimmte Schrift.
pub const TEXT_WEAK: Color32 = Color32::from_rgb(132, 142, 156);
/// Akzentfarbe (EVE-Orange).
pub const ACCENT: Color32 = Color32::from_rgb(255, 176, 0);
/// Blau für „gedrosselt“.
pub const BLUE: Color32 = Color32::from_rgb(110, 165, 255);
/// Grün für „OK“-Zustände.
pub const GREEN: Color32 = Color32::from_rgb(90, 200, 130);
/// Rot des Schließen-Knopfs beim Überfahren.
const CLOSE_RED: Color32 = Color32::from_rgb(196, 43, 28);

/// Höhe der eigenen Titelleiste.
const TITLE_BAR_H: f32 = 36.0;
/// Breite der Fenster-Knöpfe rechts oben.
const CAPTION_BTN_W: f32 = 44.0;
/// Breite der unsichtbaren Greifzone am Fensterrand.
const RESIZE_ZONE: f32 = 5.0;

/// Setzt Farben, Abstände, Rundungen und Schriftgrößen für die ganze Oberfläche.
pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG1;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    v.extreme_bg_color = BG0;
    v.faint_bg_color = Color32::from_rgb(19, 24, 31);
    v.code_bg_color = BG0;
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = Color32::from_rgb(255, 190, 60);
    v.error_fg_color = Color32::from_rgb(255, 110, 100);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(255, 176, 0, 70);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.override_text_color = None;

    // 4 px: Knöpfe wirken modern, Checkboxen bleiben als Kästchen erkennbar.
    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = CARD;
    w.noninteractive.weak_bg_fill = CARD;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = RAISED;
    w.inactive.weak_bg_fill = RAISED;
    w.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(46, 55, 68));
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = Color32::from_rgb(38, 47, 60);
    w.hovered.weak_bg_fill = Color32::from_rgb(38, 47, 60);
    w.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 176, 0, 150));
    w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = Color32::from_rgb(48, 58, 74);
    w.active.weak_bg_fill = Color32::from_rgb(48, 58, 74);
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = Color32::from_rgb(38, 47, 60);
    w.open.weak_bg_fill = Color32::from_rgb(38, 47, 60);
    w.open.bg_stroke = Stroke::new(1.0, ACCENT);
    w.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    w.open.corner_radius = radius;
    ctx.set_visuals(v);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = vec2(8.0, 7.0);
        s.button_padding = vec2(10.0, 4.0);
        s.interact_size.y = 24.0;
        s.menu_margin = Margin::same(6);
        s.combo_width = 140.0;
    });
}

/// Rahmen für eine Karte (abgerundete, leicht abgesetzte Fläche).
pub fn card() -> Frame {
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
}

/// Knopf in Akzentfarbe für die wichtigste Aktion eines Bereichs.
pub fn accent_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(BG0).strong())
            .fill(ACCENT)
            .stroke(Stroke::NONE),
    )
}

/// Ein Tab der Tableiste: gedimmt, beim Überfahren hinterlegt, ausgewählt mit Akzent-Unterstrich.
pub fn tab(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let font = FontId::proportional(14.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, TEXT);
    let size = vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    if selected || hovered {
        let fill = if selected {
            Color32::from_rgb(24, 30, 39)
        } else {
            Color32::from_rgb(20, 25, 32)
        };
        ui.painter().rect_filled(rect.shrink2(vec2(0.0, 3.0)), 6.0, fill);
    }
    let color = if selected {
        Color32::WHITE
    } else if hovered {
        TEXT
    } else {
        TEXT_WEAK
    };
    ui.painter()
        .galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    if selected {
        let line = Rect::from_center_size(
            pos2(rect.center().x, rect.bottom() - 1.5),
            vec2(rect.width() - 16.0, 3.0),
        );
        ui.painter().rect_filled(line, 1.5, ACCENT);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Kleine farbige Plakette (z. B. für den Client-Status).
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = galley.size() + vec2(16.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 34);
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp
}

/// Schmaler, abgerundeter Füllbalken (0.0–1.0) über die verfügbare Breite.
pub fn meter(ui: &mut Ui, fraction: f32, color: Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, BG0);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        let fill = Rect::from_min_size(rect.min, vec2((rect.width() * f).max(6.0), rect.height()));
        ui.painter().rect_filled(fill, 3.0, color);
    }
}

/// Abschnittsüberschrift mit Akzentstrich links.
pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(3.0, 16.0), Sense::hover());
        ui.painter().rect_filled(rect, 1.5, ACCENT);
        ui.label(RichText::new(title).strong().size(15.5).color(Color32::WHITE));
    });
    ui.add_space(4.0);
}

/// Was die Titelleiste in diesem Bild ausgelöst hat.
#[derive(Default)]
pub struct TitleBarAction {
    /// Schließen-Knopf gedrückt (wie das X des Systemrahmens).
    pub close: bool,
}

/// Eigene Titelleiste: Icon, Name, Version, Zieh-Fläche (Doppelklick = maximieren) und
/// gezeichnete Knöpfe für Minimieren, Maximieren/Wiederherstellen und Schließen.
pub fn title_bar(ui: &mut Ui, icon: &egui::TextureHandle) -> TitleBarAction {
    let mut action = TitleBarAction::default();
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let bar = ui.max_rect();

    // Zieh-Fläche zuerst anlegen, damit die Knöpfe darüber liegen und Klicks erhalten.
    let drag = ui.interact(bar, ui.id().with("title_drag"), Sense::click_and_drag());
    if drag.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if drag.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    // Links: Icon, Name und Version.
    let painter = ui.painter();
    let icon_rect = Rect::from_center_size(pos2(bar.left() + 22.0, bar.center().y), vec2(20.0, 20.0));
    painter.image(
        icon.id(),
        icon_rect,
        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    let name = painter.text(
        pos2(icon_rect.right() + 10.0, bar.center().y),
        Align2::LEFT_CENTER,
        "EVE Multibox",
        FontId::proportional(15.0),
        Color32::WHITE,
    );
    painter.text(
        pos2(name.right() + 8.0, bar.center().y + 1.0),
        Align2::LEFT_CENTER,
        format!("v{}", env!("CARGO_PKG_VERSION")),
        FontId::proportional(11.5),
        TEXT_WEAK,
    );

    // Rechts: Schließen, Maximieren, Minimieren (von rechts nach links).
    let mut x = bar.right();
    for kind in [Caption::Close, Caption::Maximize, Caption::Minimize] {
        let rect = Rect::from_min_max(pos2(x - CAPTION_BTN_W, bar.top()), pos2(x, bar.bottom()));
        x -= CAPTION_BTN_W;
        let resp = ui.interact(rect, ui.id().with(kind as u8), Sense::click());
        let hover_fill = if kind == Caption::Close {
            CLOSE_RED
        } else {
            Color32::from_rgb(32, 39, 50)
        };
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, hover_fill);
        }
        let color = if resp.hovered() { Color32::WHITE } else { TEXT };
        draw_caption_glyph(ui.painter(), rect.center(), kind, maximized, color);
        if resp.clicked() {
            match kind {
                Caption::Close => action.close = true,
                Caption::Maximize => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                Caption::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            }
        }
    }
    action
}

/// Art eines Fenster-Knopfs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Caption {
    Minimize,
    Maximize,
    Close,
}

/// Zeichnet das Symbol eines Fenster-Knopfs als Linien.
fn draw_caption_glyph(p: &egui::Painter, c: Pos2, kind: Caption, maximized: bool, color: Color32) {
    let s = Stroke::new(1.2, color);
    match kind {
        Caption::Minimize => {
            p.line_segment([pos2(c.x - 5.0, c.y + 0.5), pos2(c.x + 5.0, c.y + 0.5)], s);
        }
        Caption::Maximize if maximized => {
            // Zwei versetzte Rechtecke = „Wiederherstellen“.
            let back = Rect::from_min_size(pos2(c.x - 2.5, c.y - 5.5), vec2(8.0, 8.0));
            let front = Rect::from_min_size(pos2(c.x - 5.5, c.y - 2.5), vec2(8.0, 8.0));
            p.rect_stroke(back, 1.0, s, StrokeKind::Middle);
            p.rect_filled(front, 1.0, BG0);
            p.rect_stroke(front, 1.0, s, StrokeKind::Middle);
        }
        Caption::Maximize => {
            let r = Rect::from_center_size(c, vec2(10.0, 10.0));
            p.rect_stroke(r, 1.0, s, StrokeKind::Middle);
        }
        Caption::Close => {
            p.line_segment([pos2(c.x - 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 5.0)], s);
            p.line_segment([pos2(c.x + 5.0, c.y - 5.0), pos2(c.x - 5.0, c.y + 5.0)], s);
        }
    }
}

/// Höhe der Titelleiste (für das Panel).
pub fn title_bar_height() -> f32 {
    TITLE_BAR_H
}

/// Größe-Ziehen am Fensterrand (ohne Systemrahmen nötig) und eine feine Umrandung,
/// damit sich das Fenster vom Hintergrund abhebt. Bei maximiertem Fenster entfällt beides.
pub fn window_edges(ctx: &egui::Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let screen = ctx.content_rect();
    if !maximized {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window_border"),
        ));
        painter.rect_stroke(screen.shrink(0.5), 0.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
    } else {
        return;
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let z = RESIZE_ZONE;
    let left = pos.x <= screen.left() + z;
    let right = pos.x >= screen.right() - z;
    let top = pos.y <= screen.top() + z;
    let bottom = pos.y >= screen.bottom() - z;
    let dir = match (left, right, top, bottom) {
        (true, _, true, _) => Some((ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest)),
        (_, true, true, _) => Some((ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast)),
        (true, _, _, true) => Some((ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest)),
        (_, true, _, true) => Some((ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast)),
        (true, ..) => Some((ResizeDirection::West, CursorIcon::ResizeWest)),
        (_, true, ..) => Some((ResizeDirection::East, CursorIcon::ResizeEast)),
        (_, _, true, _) => Some((ResizeDirection::North, CursorIcon::ResizeNorth)),
        (_, _, _, true) => Some((ResizeDirection::South, CursorIcon::ResizeSouth)),
        _ => None,
    };
    if let Some((dir, cursor)) = dir {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}
