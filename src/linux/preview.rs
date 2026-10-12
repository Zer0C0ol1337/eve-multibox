//! Live-Vorschau aller Clients unter X11/XWayland über XComposite + XRender.
//!
//! Jede Vorschau ist ein randloses Override-Redirect-Fenster. Der X-Server skaliert das
//! Bild des EVE-Fensters (per Composite umgeleitet) direkt in dieses Fenster – es wird
//! nichts in den Client eingegriffen oder an ihn gesendet.
//!
//! Bedienung wie unter Windows: Linksklick = fokussieren, Rechtsklick = ins Hauptfenster,
//! innen ziehen = verschieben, am Rand/an der Ecke ziehen = Größe ändern; Kanten rasten
//! an anderen Vorschauen und am Bildschirmrand ein.

use std::collections::HashMap;

use evemb::config::{PreviewSettings, Rect};
use evemb::layout::{self, Edges};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{ConnectionExt as _, Redirect};
use x11rb::protocol::render::{self, ConnectionExt as _, CreatePictureAux, PictOp, Pictformat, Picture, Transform};
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

use super::label::{Colors, LabelRenderer};
use super::x11::X;
use crate::engine_api::Hwnd;

/// Höhe der Titelzeile über dem Bild in Pixeln.
const TITLE_H: i32 = 24;
/// Rahmenbreite (für die Hervorhebung des aktiven Clients).
const BORDER: i32 = 2;
/// Breite der Greifzone am Rand für das Größe-Ziehen.
const RESIZE_EDGE: i32 = 7;
/// Kleinste erlaubte Bildgröße.
const MIN_THUMB: (i32, i32) = (64, 36);
/// Mausweg in Pixeln, ab dem ein Klick als Ziehen gilt.
const DRAG_THRESHOLD: i32 = 5;

/// Farben (0xRRGGBB, TrueColor) – passend zum Dashboard.
const COLOR_FRAME: u32 = 0x1E242C;
const COLOR_FRAME_ACTIVE: u32 = 0xFFB000;
const COLOR_TITLE_BG: u32 = 0x0E1217;
const COLOR_TEXT: u32 = 0xD6DCE4;
const COLOR_TEXT_ACTIVE: u32 = 0xFFC640;
/// Farben der gerenderten Titelzeile.
const LABEL_COLORS: Colors = Colors {
    background: COLOR_TITLE_BG,
    text: COLOR_TEXT,
    text_active: COLOR_TEXT_ACTIVE,
    accent: COLOR_FRAME_ACTIVE,
};

/// Glyphen im X-Cursor-Font.
const XC_BOTTOM_LEFT: u16 = 12;
const XC_BOTTOM_RIGHT: u16 = 14;
const XC_HAND2: u16 = 60;
const XC_H_DOUBLE: u16 = 108;
const XC_V_DOUBLE: u16 = 116;
const XC_TOP_LEFT: u16 = 134;
const XC_TOP_RIGHT: u16 = 136;

/// Ereignis aus einem Vorschaufenster.
#[derive(Clone, Debug)]
pub enum PreviewEvent {
    /// Linksklick: Quell-Client fokussieren.
    Click(Hwnd),
    /// Rechtsklick: Quell-Client ins Hauptfenster tauschen.
    MakeMain(Hwnd),
    /// Vorschau wurde verschoben oder in der Größe geändert (Fensterecke + Bildgröße).
    Moved {
        source: Hwnd,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
}

/// Ein Eintrag, den die Engine anzeigen möchte.
#[derive(Clone, Debug)]
pub struct PreviewItem {
    /// Fenster des EVE-Clients.
    pub source: Hwnd,
    /// Schlüssel für gespeicherte Positionen/Größen (Charaktername).
    pub key: String,
    /// Angezeigter Text.
    pub label: String,
    /// Ist gerade im Vordergrund.
    pub active: bool,
    /// Ist das Hauptfenster (Raute in der Titelzeile).
    pub main: bool,
    /// Soll sichtbar sein.
    pub visible: bool,
    /// Quelle ist minimiert (liefert kein Bild).
    pub minimized: bool,
}

/// Höhe der Titelzeile je nach Einstellung.
fn title_height(show: bool) -> i32 {
    if show { TITLE_H } else { 0 }
}

/// Fenstergröße für eine Bildgröße (plus Rahmen und Titel).
fn window_size(thumb: (i32, i32), show_title: bool) -> (i32, i32) {
    (thumb.0 + 2 * BORDER, thumb.1 + title_height(show_title) + 2 * BORDER)
}

/// Bildgröße aus einer Fenstergröße (Gegenstück zu `window_size`).
fn thumb_size(w: i32, h: i32, show_title: bool) -> (i32, i32) {
    (
        (w - 2 * BORDER).max(MIN_THUMB.0),
        (h - 2 * BORDER - title_height(show_title)).max(MIN_THUMB.1),
    )
}

/// Wandelt eine Kommazahl in das XRender-Festkommaformat (16.16) um.
fn fixed(v: f64) -> render::Fixed {
    (v * 65536.0).round() as render::Fixed
}

/// Nur druckbare ASCII-Zeichen – für die Notlösung mit dem X-Kernfont „fixed“, falls das
/// Titelbild nicht per `PutImage` gezeichnet werden kann (ungewöhnliche Farbtiefe).
fn ascii_label(s: &str) -> String {
    s.chars()
        .filter(|c| (' '..='~').contains(c))
        .collect::<String>()
        .trim()
        .to_string()
}

/// Laufender Zieh-Vorgang mit gedrückter linker Maustaste.
#[derive(Clone, Copy)]
struct Drag {
    wnd: Window,
    start_cursor: (i32, i32),
    start_rect: Rect,
    /// `None` = verschieben, sonst Größe ändern an diesen Kanten.
    edges: Option<Edges>,
    /// Schwelle überschritten – es wird wirklich gezogen (sonst ist es ein Klick).
    active: bool,
}

/// Bild einer Quelle samt zuletzt gesetzter Skalierung: (Picture, Quellgröße, Zielgröße).
type SourcePicture = (Picture, (u16, u16), (i32, i32));

/// Zustand eines Vorschaufensters.
struct PreviewWin {
    wnd: Window,
    gc: Gcontext,
    dst: Picture,
    /// Bild der Quelle samt zuletzt gesetzter Skalierung (Quellgröße, Zielgröße).
    src: Option<SourcePicture>,
    source: Window,
    source_size: (u16, u16),
    rect: Rect,
    visible: bool,
    alpha: u8,
    label: String,
    active: bool,
    main: bool,
    minimized: bool,
    show_title: bool,
    highlight: bool,
    cursor_zone: Option<Option<Edges>>,
}

/// Alles, was zum Zeichnen der Titelzeile gebraucht wird.
struct Deco {
    /// Geglättete Schrift (None = Notlösung mit Kernfont).
    label: Option<LabelRenderer>,
    /// Farbtiefe des Bildschirms (24 oder 32 = Titelbild per `PutImage` möglich).
    depth: u8,
    /// X-Kernfont für die Notlösung und dessen Zeichenbreite.
    font: Font,
    char_width: i32,
}

/// Verwaltet alle Vorschaufenster.
pub struct PreviewManager {
    /// Composite und Render vorhanden?
    pub supported: bool,
    dst_format: Pictformat,
    formats: HashMap<Visualid, Pictformat>,
    deco: Deco,
    cursor_font: Font,
    cursors: HashMap<u16, Cursor>,
    wins: HashMap<Hwnd, PreviewWin>,
    by_window: HashMap<Window, Hwnd>,
    drag: Option<Drag>,
    events: Vec<PreviewEvent>,
    snap: (bool, i32, i32),
    /// Arbeitsbereiche der Monitore (zum Einrasten am Bildschirmrand).
    pub bounds: Vec<Rect>,
}

impl PreviewManager {
    /// Prüft Composite/Render und legt Schrift und Mauszeiger an.
    pub fn new(x: &X) -> Self {
        let c = &x.conn;
        let composite_ok = c
            .composite_query_version(0, 4)
            .ok()
            .and_then(|r| r.reply().ok())
            .is_some();
        let render_formats = c
            .render_query_version(0, 11)
            .ok()
            .and_then(|r| r.reply().ok())
            .and_then(|_| c.render_query_pict_formats().ok().and_then(|r| r.reply().ok()));
        let mut formats = HashMap::new();
        if let Some(pf) = &render_formats {
            for screen in &pf.screens {
                for depth in &screen.depths {
                    for v in &depth.visuals {
                        formats.insert(v.visual, v.format);
                    }
                }
            }
        }
        let dst_format = formats.get(&x.root_visual).copied().unwrap_or(0);
        let supported = composite_ok && dst_format != 0;

        let font = c.generate_id().unwrap_or(0);
        let _ = c.open_font(font, b"fixed");
        let char_width = c
            .query_font(font)
            .ok()
            .and_then(|r| r.reply().ok())
            .map(|f| f.max_bounds.character_width as i32)
            .filter(|&w| w > 0)
            .unwrap_or(6);
        let cursor_font = c.generate_id().unwrap_or(0);
        let _ = c.open_font(cursor_font, b"cursor");

        // PutImage mit 32 Bit je Pixel, niederwertiges Byte zuerst (Xorg/XWayland auf x86).
        let setup = c.setup();
        let depth = setup.roots.first().map_or(0, |r| r.root_depth);
        let bpp32 = setup
            .pixmap_formats
            .iter()
            .any(|f| f.depth == depth && f.bits_per_pixel == 32);
        let lsb = setup.image_byte_order == ImageOrder::LSB_FIRST;
        let label = if (depth == 24 || depth == 32) && bpp32 && lsb {
            LabelRenderer::new()
        } else {
            None
        };

        PreviewManager {
            supported,
            dst_format,
            formats,
            deco: Deco {
                label,
                depth,
                font,
                char_width,
            },
            cursor_font,
            cursors: HashMap::new(),
            wins: HashMap::new(),
            by_window: HashMap::new(),
            drag: None,
            events: Vec::new(),
            snap: (true, 20, 2),
            bounds: Vec::new(),
        }
    }

    /// Mauszeiger aus dem X-Cursor-Font (einmal angelegt, dann wiederverwendet).
    fn cursor(&mut self, x: &X, glyph: u16) -> Cursor {
        if let Some(&c) = self.cursors.get(&glyph) {
            return c;
        }
        let Ok(id) = x.conn.generate_id() else { return 0 };
        let _ = x.conn.create_glyph_cursor(
            id,
            self.cursor_font,
            self.cursor_font,
            glyph,
            glyph + 1,
            0,
            0,
            0,
            0xFFFF,
            0xFFFF,
            0xFFFF,
        );
        self.cursors.insert(glyph, id);
        id
    }

    /// Passender Mauszeiger für eine Greifzone.
    fn cursor_for(&mut self, x: &X, zone: Option<Edges>) -> Cursor {
        let glyph = match zone {
            None => XC_HAND2,
            Some(e) if e.left && e.top => XC_TOP_LEFT,
            Some(e) if e.right && e.top => XC_TOP_RIGHT,
            Some(e) if e.left && e.bottom => XC_BOTTOM_LEFT,
            Some(e) if e.right && e.bottom => XC_BOTTOM_RIGHT,
            Some(e) if e.left || e.right => XC_H_DOUBLE,
            Some(_) => XC_V_DOUBLE,
        };
        self.cursor(x, glyph)
    }

    /// Vorschaufenster, das gerade gezogen wird (0 = keins).
    pub fn dragging_source(&self) -> Hwnd {
        self.drag
            .filter(|d| d.active)
            .and_then(|d| self.by_window.get(&d.wnd).copied())
            .unwrap_or(0)
    }

    /// Ist das X-Fenster eines unserer Vorschaufenster?
    pub fn owns(&self, w: Window) -> bool {
        self.by_window.contains_key(&w)
    }

    /// Holt alle angefallenen Vorschau-Ereignisse ab.
    pub fn take_events(&mut self) -> Vec<PreviewEvent> {
        std::mem::take(&mut self.events)
    }

    /// Bringt alle Vorschaufenster auf den gewünschten Stand: anlegen, verschieben,
    /// Größe setzen, ein-/ausblenden, Quelle aktualisieren und verwaiste entfernen.
    pub fn sync(&mut self, x: &X, items: &[PreviewItem], s: &PreviewSettings) {
        if !self.supported {
            return;
        }
        self.snap = (s.snap, s.snap_distance, s.gap);
        let default_size = window_size((s.width, s.height), s.show_titles);
        let dragging = self.dragging_source();
        let mut seen = Vec::with_capacity(items.len());

        for (i, item) in items.iter().enumerate() {
            seen.push(item.source);
            let thumb = s.sizes.get(&item.key).copied().unwrap_or((s.width, s.height));
            let (ww, wh) = window_size(thumb, s.show_titles);
            let (px, py) = s.positions.get(&item.key).copied().unwrap_or_else(|| {
                layout::auto_preview_pos(i, (s.start_x, s.start_y), default_size, s.gap, s.vertical, s.per_line)
            });
            let rect = Rect::new(px, py, ww, wh);

            if !self.wins.contains_key(&item.source) {
                match self.create(x, item.source as Window, rect, s.opacity) {
                    Some(w) => {
                        self.by_window.insert(w.wnd, item.source);
                        self.wins.insert(item.source, w);
                    }
                    None => continue,
                }
            }
            let w = self.wins.get_mut(&item.source).expect("gerade angelegt");

            let deco_changed = w.label != item.label
                || w.active != item.active
                || w.main != item.main
                || w.show_title != s.show_titles
                || w.highlight != s.highlight_active
                || w.minimized != item.minimized;
            w.label = item.label.clone();
            w.active = item.active;
            w.main = item.main;
            w.show_title = s.show_titles;
            w.highlight = s.highlight_active;
            w.minimized = item.minimized;

            // Größe der Quelle aktualisieren (ändert die Skalierung).
            if let Some(size) = x.client_size(w.source) {
                w.source_size = size;
            }

            if item.source != dragging && w.rect != rect {
                let aux = ConfigureWindowAux::new()
                    .x(rect.x)
                    .y(rect.y)
                    .width(rect.w as u32)
                    .height(rect.h as u32);
                let _ = x.conn.configure_window(w.wnd, &aux);
                w.rect = rect;
            }
            if w.alpha != s.opacity {
                x.set_opacity(w.wnd, s.opacity);
                w.alpha = s.opacity;
            }
            if item.visible && !w.visible {
                let _ = x.conn.map_window(w.wnd);
                w.visible = true;
            } else if !item.visible && w.visible {
                let _ = x.conn.unmap_window(w.wnd);
                w.visible = false;
            }
            if w.visible {
                // Über EVE-Fenstern halten.
                let _ = x
                    .conn
                    .configure_window(w.wnd, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
            }
            if deco_changed {
                Self::draw_decorations(x, &self.deco, w);
            }
        }

        let stale: Vec<Hwnd> = self.wins.keys().copied().filter(|k| !seen.contains(k)).collect();
        for src in stale {
            if let Some(w) = self.wins.remove(&src) {
                self.by_window.remove(&w.wnd);
                Self::destroy(x, w);
            }
        }
    }

    /// Legt ein (zunächst unsichtbares) Vorschaufenster an und leitet die Quelle um.
    fn create(&mut self, x: &X, source: Window, rect: Rect, alpha: u8) -> Option<PreviewWin> {
        let c = &x.conn;
        let wnd = c.generate_id().ok()?;
        let hand = self.cursor(x, XC_HAND2);
        let events =
            EventMask::EXPOSURE | EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION;
        let aux = CreateWindowAux::new()
            .override_redirect(1)
            .background_pixel(0)
            .event_mask(events)
            .cursor(hand);
        c.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            wnd,
            x.root,
            rect.x as i16,
            rect.y as i16,
            rect.w.max(1) as u16,
            rect.h.max(1) as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &aux,
        )
        .ok()?;
        let title = b"EVE Multibox Preview";
        let _ = c.change_property8(PropMode::REPLACE, wnd, AtomEnum::WM_NAME, AtomEnum::STRING, title);
        let _ = c.change_property8(PropMode::REPLACE, wnd, x.atoms._NET_WM_NAME, x.atoms.UTF8_STRING, title);
        let _ = c.change_property8(
            PropMode::REPLACE,
            wnd,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"eve-multibox\0EveMultibox\0",
        );
        let _ = c.change_property32(
            PropMode::REPLACE,
            wnd,
            x.atoms._NET_WM_PID,
            AtomEnum::CARDINAL,
            &[x.own_pid],
        );
        let _ = c.change_property32(
            PropMode::REPLACE,
            wnd,
            x.atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[x.atoms._NET_WM_WINDOW_TYPE_UTILITY],
        );
        x.set_opacity(wnd, alpha);

        let gc = c.generate_id().ok()?;
        let _ = c.create_gc(gc, wnd, &CreateGCAux::new().font(self.deco.font).graphics_exposures(0));
        let dst = c.generate_id().ok()?;
        let _ = c.render_create_picture(dst, wnd, self.dst_format, &CreatePictureAux::new());

        // Quelle umleiten, damit ihr Inhalt auch verdeckt verfügbar ist (mit Compositor ohnehin der Fall).
        let _ = c.composite_redirect_window(source, Redirect::AUTOMATIC);

        Some(PreviewWin {
            wnd,
            gc,
            dst,
            src: None,
            source,
            source_size: (0, 0),
            rect,
            visible: false,
            alpha,
            label: String::new(),
            active: false,
            main: false,
            minimized: false,
            show_title: true,
            highlight: true,
            cursor_zone: None,
        })
    }

    /// Gibt Bilder, Grafikkontext und Fenster frei und hebt die Umleitung auf.
    fn destroy(x: &X, w: PreviewWin) {
        let c = &x.conn;
        if let Some((pic, _, _)) = w.src {
            let _ = c.render_free_picture(pic);
        }
        let _ = c.render_free_picture(w.dst);
        let _ = c.free_gc(w.gc);
        let _ = c.destroy_window(w.wnd);
        let _ = c.composite_unredirect_window(w.source, Redirect::AUTOMATIC);
    }

    /// Entfernt alle Vorschaufenster.
    pub fn clear(&mut self, x: &X) {
        for (_, w) in self.wins.drain() {
            Self::destroy(x, w);
        }
        self.by_window.clear();
        self.drag = None;
    }

    /// Zeichnet Rahmen und Titelzeile (das Bild malt `paint_one` darüber).
    fn draw_decorations(x: &X, deco: &Deco, w: &PreviewWin) {
        let c = &x.conn;
        let (ww, wh) = (w.rect.w, w.rect.h);
        let frame = if w.active && w.highlight {
            COLOR_FRAME_ACTIVE
        } else {
            COLOR_FRAME
        };
        let _ = c.change_gc(w.gc, &ChangeGCAux::new().foreground(frame));
        let _ = c.poly_fill_rectangle(
            w.wnd,
            w.gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: ww as u16,
                height: wh as u16,
            }],
        );
        let bar_w = ww - 2 * BORDER;
        if w.show_title && bar_w > 0 {
            match &deco.label {
                Some(renderer) => {
                    let img = renderer.render(bar_w as u32, TITLE_H as u32, &w.label, w.active, w.main, LABEL_COLORS);
                    let _ = c.put_image(
                        ImageFormat::Z_PIXMAP,
                        w.wnd,
                        w.gc,
                        bar_w as u16,
                        TITLE_H as u16,
                        BORDER as i16,
                        BORDER as i16,
                        0,
                        deco.depth,
                        &img,
                    );
                }
                None => Self::draw_title_fallback(x, deco.char_width, w, bar_w),
            }
        }
        Self::paint_one(x, w);
    }

    /// Notlösung ohne Titelbild: Leiste mit dem X-Kernfont „fixed“ (nur ASCII).
    fn draw_title_fallback(x: &X, char_width: i32, w: &PreviewWin, bar_w: i32) {
        let c = &x.conn;
        let bar = Rectangle {
            x: BORDER as i16,
            y: BORDER as i16,
            width: bar_w as u16,
            height: TITLE_H as u16,
        };
        let text_color = if w.active { COLOR_TEXT_ACTIVE } else { COLOR_TEXT };
        let _ = c.change_gc(
            w.gc,
            &ChangeGCAux::new().foreground(COLOR_TITLE_BG).background(COLOR_TITLE_BG),
        );
        let _ = c.poly_fill_rectangle(w.wnd, w.gc, &[bar]);
        let max_chars = ((bar_w - 16) / char_width).max(0) as usize;
        let mut text = ascii_label(&w.label);
        if w.main {
            text.push_str(" *");
        }
        if text.len() > max_chars {
            text.truncate(max_chars.saturating_sub(2));
            text.push_str("..");
        }
        let _ = c.change_gc(w.gc, &ChangeGCAux::new().foreground(text_color));
        let baseline = BORDER + TITLE_H / 2 + 4;
        let _ = c.image_text8(w.wnd, w.gc, (BORDER + 8) as i16, baseline as i16, text.as_bytes());
    }

    /// Kopiert das skalierte Bild einer Quelle in ihr Vorschaufenster (bzw. Schwarz ohne Bild).
    fn paint_one(x: &X, w: &PreviewWin) {
        let c = &x.conn;
        let top = BORDER + title_height(w.show_title);
        let (tw, th) = (w.rect.w - 2 * BORDER, w.rect.h - top - BORDER);
        if tw <= 0 || th <= 0 {
            return;
        }
        let area = Rectangle {
            x: BORDER as i16,
            y: top as i16,
            width: tw as u16,
            height: th as u16,
        };
        match (&w.src, w.minimized) {
            (Some((pic, _, _)), false) => {
                let _ = c.render_composite(
                    PictOp::SRC,
                    *pic,
                    x11rb::NONE,
                    w.dst,
                    0,
                    0,
                    0,
                    0,
                    area.x,
                    area.y,
                    area.width,
                    area.height,
                );
            }
            _ => {
                // Kein Bild (minimiert oder Quelle noch nicht bereit): schwarz.
                let _ = c.change_gc(w.gc, &ChangeGCAux::new().foreground(0));
                let _ = c.poly_fill_rectangle(w.wnd, w.gc, &[area]);
            }
        }
    }

    /// Aktualisiert alle sichtbaren Vorschauen (regelmäßig aufrufen, ~30-mal pro Sekunde).
    pub fn paint_all(&mut self, x: &X) {
        if !self.supported {
            return;
        }
        let c = &x.conn;
        for w in self.wins.values_mut().filter(|w| w.visible) {
            let top = BORDER + title_height(w.show_title);
            let target = (w.rect.w - 2 * BORDER, w.rect.h - top - BORDER);
            let (sw, sh) = w.source_size;
            if sw == 0 || sh == 0 || target.0 <= 0 || target.1 <= 0 {
                continue;
            }
            // Bild der Quelle anlegen bzw. Skalierung anpassen.
            let needs_new = w.src.is_none();
            if needs_new {
                let visual = c
                    .get_window_attributes(w.source)
                    .ok()
                    .and_then(|r| r.reply().ok())
                    .map(|a| a.visual);
                let Some(format) = visual.and_then(|v| self.formats.get(&v).copied()) else {
                    continue;
                };
                let Ok(pic) = c.generate_id() else { continue };
                let aux = CreatePictureAux::new().subwindowmode(SubwindowMode::INCLUDE_INFERIORS);
                let _ = c.render_create_picture(pic, w.source, format, &aux);
                let _ = c.render_set_picture_filter(pic, b"bilinear", &[]);
                w.src = Some((pic, (0, 0), (0, 0)));
            }
            if let Some((pic, size, tgt)) = &mut w.src
                && (*size != (sw, sh) || *tgt != target)
            {
                let one = fixed(1.0);
                let transform = Transform {
                    matrix11: fixed(sw as f64 / target.0 as f64),
                    matrix12: 0,
                    matrix13: 0,
                    matrix21: 0,
                    matrix22: fixed(sh as f64 / target.1 as f64),
                    matrix23: 0,
                    matrix31: 0,
                    matrix32: 0,
                    matrix33: one,
                };
                let _ = c.render_set_picture_transform(*pic, transform);
                *size = (sw, sh);
                *tgt = target;
            }
            Self::paint_one(x, w);
        }
    }

    /// Verarbeitet ein X-Ereignis, falls es eines unserer Vorschaufenster betrifft.
    /// Liefert `true`, wenn das Ereignis verbraucht wurde.
    pub fn handle_event(&mut self, x: &X, ev: &x11rb::protocol::Event) -> bool {
        use x11rb::protocol::Event as E;
        match ev {
            E::Expose(e) if self.owns(e.window) && e.count == 0 => {
                if let Some(w) = self.by_window.get(&e.window).and_then(|s| self.wins.get(s)) {
                    Self::draw_decorations(x, &self.deco, w);
                }
                true
            }
            E::ButtonPress(e) if self.owns(e.event) => {
                if e.detail == 1 {
                    let Some(w) = self.by_window.get(&e.event).and_then(|s| self.wins.get(s)) else {
                        return true;
                    };
                    let (rx, ry) = (e.root_x as i32, e.root_y as i32);
                    self.drag = Some(Drag {
                        wnd: e.event,
                        start_cursor: (rx, ry),
                        start_rect: w.rect,
                        edges: layout::grab_edges(w.rect, rx, ry, RESIZE_EDGE),
                        active: false,
                    });
                }
                true
            }
            E::MotionNotify(e) if self.owns(e.event) => {
                let (rx, ry) = (e.root_x as i32, e.root_y as i32);
                match self.drag.filter(|d| d.wnd == e.event) {
                    Some(mut d) => {
                        let (dx, dy) = (rx - d.start_cursor.0, ry - d.start_cursor.1);
                        if !d.active && (dx.abs() > DRAG_THRESHOLD || dy.abs() > DRAG_THRESHOLD) {
                            d.active = true;
                        }
                        if d.active {
                            self.drag_to(x, &d, dx, dy);
                        }
                        self.drag = Some(d);
                    }
                    None => self.update_cursor(x, e.event, rx, ry),
                }
                true
            }
            E::ButtonRelease(e) if self.owns(e.event) => {
                let source = self.by_window.get(&e.event).copied();
                match e.detail {
                    1 => {
                        if let Some(d) = self.drag.take().filter(|d| d.wnd == e.event) {
                            match (d.active, source) {
                                (true, Some(src)) => self.finish_drag(src),
                                (false, Some(src)) => self.events.push(PreviewEvent::Click(src)),
                                _ => {}
                            }
                        }
                    }
                    3 => {
                        if let Some(src) = source {
                            self.events.push(PreviewEvent::MakeMain(src));
                        }
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        }
    }

    /// Wählt den Mauszeiger passend zur Greifzone unter der Maus.
    fn update_cursor(&mut self, x: &X, wnd: Window, rx: i32, ry: i32) {
        let Some(src) = self.by_window.get(&wnd).copied() else {
            return;
        };
        let Some(rect) = self.wins.get(&src).map(|w| w.rect) else {
            return;
        };
        let zone = layout::grab_edges(rect, rx, ry, RESIZE_EDGE);
        if self.wins.get(&src).is_some_and(|w| w.cursor_zone == Some(zone)) {
            return;
        }
        let cursor = self.cursor_for(x, zone);
        let _ = x
            .conn
            .change_window_attributes(wnd, &ChangeWindowAttributesAux::new().cursor(cursor));
        if let Some(w) = self.wins.get_mut(&src) {
            w.cursor_zone = Some(zone);
        }
    }

    /// Führt das Fenster beim Ziehen nach: verschieben oder Größe ändern, inklusive Einrasten.
    fn drag_to(&mut self, x: &X, d: &Drag, dx: i32, dy: i32) {
        let Some(src) = self.by_window.get(&d.wnd).copied() else {
            return;
        };
        let show_title = self.wins.get(&src).is_none_or(|w| w.show_title);
        let min = window_size(MIN_THUMB, show_title);
        let proposed = match d.edges {
            None => Rect::new(d.start_rect.x + dx, d.start_rect.y + dy, d.start_rect.w, d.start_rect.h),
            Some(e) => layout::resize_rect(d.start_rect, e, dx, dy, min),
        };
        let r = self.snap_rect(src, proposed, d.edges, min);
        let aux = ConfigureWindowAux::new()
            .x(r.x)
            .y(r.y)
            .width(r.w as u32)
            .height(r.h as u32);
        let _ = x.conn.configure_window(d.wnd, &aux);
        let resized = d.edges.is_some();
        if let Some(w) = self.wins.get_mut(&src) {
            w.rect = r;
            if resized {
                Self::draw_decorations(x, &self.deco, w);
            }
        }
        if resized {
            self.paint_all(x);
        }
    }

    /// Zieh-Vorgang beenden und neue Position/Größe melden.
    fn finish_drag(&mut self, src: Hwnd) {
        if let Some(w) = self.wins.get(&src) {
            let (tw, th) = thumb_size(w.rect.w, w.rect.h, w.show_title);
            self.events.push(PreviewEvent::Moved {
                source: src,
                x: w.rect.x,
                y: w.rect.y,
                w: tw,
                h: th,
            });
        }
    }

    /// Rastet ein gezogenes Rechteck an anderen Vorschauen und am Monitor-Arbeitsbereich ein.
    fn snap_rect(&self, src: Hwnd, r: Rect, edges: Option<Edges>, min: (i32, i32)) -> Rect {
        let (enabled, distance, gap) = self.snap;
        if !enabled {
            return r;
        }
        let others: Vec<Rect> = self
            .wins
            .iter()
            .filter(|(k, w)| **k != src && w.visible)
            .map(|(_, w)| w.rect)
            .collect();
        // Monitor, auf dem die Mitte des Rechtecks liegt (sonst der erste).
        let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
        let bounds: Vec<Rect> = self
            .bounds
            .iter()
            .copied()
            .filter(|b| cx >= b.x && cx < b.right() && cy >= b.y && cy < b.bottom())
            .take(1)
            .chain(self.bounds.first().copied())
            .take(1)
            .collect();
        match edges {
            None => layout::snap_rect(r, &others, &bounds, distance, gap),
            Some(e) => {
                let s = layout::snap_resize(r, e, &others, &bounds, distance, gap);
                if s.w < min.0 || s.h < min.1 { r } else { s }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fenster- und Bildgröße sind zueinander konsistent.
    #[test]
    fn size_roundtrip() {
        let (w, h) = window_size((320, 180), true);
        assert_eq!(thumb_size(w, h, true), (320, 180));
        let (w, h) = window_size((256, 144), false);
        assert_eq!(thumb_size(w, h, false), (256, 144));
    }

    /// Titel werden auf druckbares ASCII reduziert.
    #[test]
    fn labels() {
        assert_eq!(ascii_label("Legga Schnegge  ★"), "Legga Schnegge");
        assert_eq!(fixed(1.0), 65536);
        assert_eq!(fixed(0.5), 32768);
    }
}
