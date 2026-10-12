//! Titelzeilen der Linux-Vorschaufenster als fertiges Bild: dunkle Leiste, geglättete
//! Schrift (Ubuntu aus den egui-Standardschriften, also überall gleich), Akzentpunkt für den
//! aktiven Client und eine Raute für das Hauptfenster.
//!
//! Das Bild wird in Software gerechnet und per `PutImage` gezeichnet – so braucht es weder
//! Systemschriften noch zusätzliche X-Erweiterungen.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

/// Schriftgröße in Pixeln.
const FONT_PX: f32 = 13.5;
/// Linker Innenabstand ohne Akzentpunkt.
const PAD_LEFT: f32 = 9.0;
/// Rechter Innenabstand.
const PAD_RIGHT: f32 = 9.0;

/// Farben einer Titelzeile (0xRRGGBB).
#[derive(Clone, Copy)]
pub struct Colors {
    pub background: u32,
    pub text: u32,
    pub text_active: u32,
    pub accent: u32,
}

/// Rendert Titelzeilen mit der eingebetteten Schrift.
pub struct LabelRenderer {
    font: FontRef<'static>,
}

impl LabelRenderer {
    /// Lädt die eingebettete Schrift (None, falls sie wider Erwarten nicht lesbar ist).
    pub fn new() -> Option<Self> {
        FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT)
            .ok()
            .map(|font| LabelRenderer { font })
    }

    /// Erzeugt die Titelzeile als 32-Bit-Pixel (B, G, R, 0 – Format von 24/32-Bit-TrueColor
    /// unter X11 auf x86) mit `width`×`height` Pixeln.
    pub fn render(&self, width: u32, height: u32, label: &str, active: bool, main: bool, colors: Colors) -> Vec<u8> {
        let (w, h) = (width as usize, height as usize);
        let mut px = vec![0f32; w * h * 3];
        let bg = rgb(colors.background);
        for p in px.chunks_mut(3) {
            p.copy_from_slice(&bg);
        }
        let cy = height as f32 / 2.0;

        // Akzentpunkt für den aktiven Client.
        let mut left = PAD_LEFT;
        if active {
            fill_circle(&mut px, w, h, 11.0, cy, 3.5, rgb(colors.accent));
            left = 21.0;
        }
        // Raute rechts für das Hauptfenster.
        let mut right = width as f32 - PAD_RIGHT;
        if main {
            fill_diamond(&mut px, w, h, right - 4.5, cy, 4.5, rgb(colors.accent));
            right -= 15.0;
        }

        let color = rgb(if active { colors.text_active } else { colors.text });
        let text = self.fit(label.trim(), right - left);
        self.draw_text(&mut px, w, h, &text, left, cy, color);

        let mut out = Vec::with_capacity(w * h * 4);
        for p in px.chunks(3) {
            out.extend_from_slice(&[to_u8(p[2]), to_u8(p[1]), to_u8(p[0]), 0]);
        }
        out
    }

    /// Breite eines Textes in Pixeln.
    fn text_width(&self, text: &str) -> f32 {
        let font = self.font.as_scaled(PxScale::from(FONT_PX));
        let mut width = 0.0;
        let mut prev = None;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                width += font.kern(p, id);
            }
            width += font.h_advance(id);
            prev = Some(id);
        }
        width
    }

    /// Kürzt den Text mit „…“, bis er in `max` Pixel passt.
    fn fit(&self, text: &str, max: f32) -> String {
        if max <= 0.0 {
            return String::new();
        }
        if self.text_width(text) <= max {
            return text.to_string();
        }
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if self.text_width(&candidate) <= max {
                return candidate;
            }
        }
        String::new()
    }

    /// Zeichnet Text mit Kantenglättung; `cy` ist die vertikale Mitte.
    #[allow(clippy::too_many_arguments)]
    fn draw_text(&self, px: &mut [f32], w: usize, h: usize, text: &str, x: f32, cy: f32, color: [f32; 3]) {
        let font = self.font.as_scaled(PxScale::from(FONT_PX));
        // Grundlinie so, dass Groß- und Kleinbuchstaben optisch mittig sitzen.
        let baseline = (cy + (font.ascent() + font.descent()) / 2.0).round();
        let mut caret = x.round();
        let mut prev = None;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                caret += font.kern(p, id);
            }
            let glyph = id.with_scale_and_position(PxScale::from(FONT_PX), point(caret, baseline));
            caret += font.h_advance(id);
            prev = Some(id);
            let Some(outline) = self.font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let x = bounds.min.x as i32 + gx as i32;
                let y = bounds.min.y as i32 + gy as i32;
                if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
                    return;
                }
                // Die „Light“-Schrift etwas kräftiger wirken lassen.
                let a = coverage.clamp(0.0, 1.0).powf(0.75);
                blend(px, w, x as usize, y as usize, color, a);
            });
        }
    }
}

/// Zerlegt 0xRRGGBB in Kanäle (0–255 als f32).
fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 0xFF) as f32, ((c >> 8) & 0xFF) as f32, (c & 0xFF) as f32]
}

/// Rundet einen Farbkanal auf ein Byte.
fn to_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Mischt eine Farbe mit Deckkraft `a` in ein Pixel.
fn blend(px: &mut [f32], w: usize, x: usize, y: usize, color: [f32; 3], a: f32) {
    let i = (y * w + x) * 3;
    for k in 0..3 {
        px[i + k] = px[i + k] * (1.0 - a) + color[k] * a;
    }
}

/// Gefüllter, geglätteter Kreis.
fn fill_circle(px: &mut [f32], w: usize, h: usize, cx: f32, cy: f32, r: f32, color: [f32; 3]) {
    shape(px, w, h, cx, cy, r, color, |dx, dy| r - (dx * dx + dy * dy).sqrt());
}

/// Gefüllte, geglättete Raute (auf der Spitze stehendes Quadrat).
fn fill_diamond(px: &mut [f32], w: usize, h: usize, cx: f32, cy: f32, r: f32, color: [f32; 3]) {
    shape(px, w, h, cx, cy, r, color, |dx, dy| {
        (r - dx.abs() - dy.abs()) * std::f32::consts::FRAC_1_SQRT_2
    });
}

/// Füllt eine Form um (cx, cy); `dist` liefert den Abstand zum Rand (positiv = innen).
#[allow(clippy::too_many_arguments)]
fn shape(
    px: &mut [f32],
    w: usize,
    h: usize,
    cx: f32,
    cy: f32,
    r: f32,
    color: [f32; 3],
    dist: impl Fn(f32, f32) -> f32,
) {
    let (x0, x1) = (
        (cx - r - 1.0).floor().max(0.0) as usize,
        ((cx + r + 1.0).ceil() as usize).min(w),
    );
    let (y0, y1) = (
        (cy - r - 1.0).floor().max(0.0) as usize,
        ((cy + r + 1.0).ceil() as usize).min(h),
    );
    for y in y0..y1 {
        for x in x0..x1 {
            let d = dist(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let a = (d + 0.5).clamp(0.0, 1.0);
            if a > 0.0 {
                blend(px, w, x, y, color, a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: Colors = Colors {
        background: 0x101010,
        text: 0xFFFFFF,
        text_active: 0xFFB000,
        accent: 0xFFB000,
    };

    /// Bildgröße stimmt, Text hinterlässt helle Pixel, Hintergrund bleibt erhalten.
    #[test]
    fn renders_text() {
        let r = LabelRenderer::new().expect("Schrift");
        let img = r.render(200, 24, "Legga Schnegge", false, false, COLORS);
        assert_eq!(img.len(), 200 * 24 * 4);
        assert_eq!(&img[..3], &[0x10, 0x10, 0x10], "Ecke = Hintergrund");
        assert!(img.chunks(4).any(|p| p[0] > 0xC0), "Text sichtbar");
    }

    /// Zu lange Namen werden mit „…“ gekürzt.
    #[test]
    fn truncates_long_names() {
        let r = LabelRenderer::new().expect("Schrift");
        let fitted = r.fit("Ein wirklich sehr sehr langer Charaktername", 80.0);
        assert!(fitted.ends_with('…'));
        assert!(r.text_width(&fitted) <= 80.0);
        assert_eq!(r.fit("Kurz", 80.0), "Kurz");
    }

    /// Aktiver Client bekommt den Akzentpunkt links.
    #[test]
    fn active_dot() {
        let r = LabelRenderer::new().expect("Schrift");
        let img = r.render(100, 24, "", true, false, COLORS);
        let i = (12 * 100 + 11) * 4;
        assert_eq!(&img[i..i + 3], &[0x00, 0xB0, 0xFF], "Punkt in Akzentfarbe (BGR)");
    }
}
