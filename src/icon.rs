// App-Icon von EVE Multibox, per Code gezeichnet (keine Bilddatei nötig).
//
// Motiv: dunkle, abgerundete Kachel mit orangem Ring, darin ein 2×2-Raster aus
// Fenstern – eines davon orange (der aktive Client).
//
// Hinweis: Diese Datei wird auch von build.rs per include! eingebunden (für das
// .exe-Icon). Deshalb hier nur normale Kommentare und keine `use`-Anweisungen.

/// Zeichnet das Icon in der Größe `size`×`size` und liefert RGBA-Pixel (zeilenweise von oben).
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let s = size.max(8) as f32;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    // 4×4-Überabtastung für weiche Kanten.
    const SUB: u32 = 4;
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let x = (px as f32 + (sx as f32 + 0.5) / SUB as f32) / s;
                    let y = (py as f32 + (sy as f32 + 0.5) / SUB as f32) / s;
                    let c = icon_sample(x, y);
                    // Vormultipliziert aufsummieren.
                    acc[0] += c[0] * c[3];
                    acc[1] += c[1] * c[3];
                    acc[2] += c[2] * c[3];
                    acc[3] += c[3];
                }
            }
            let n = (SUB * SUB) as f32;
            let a = acc[3] / n;
            let (r, g, b) = if a > 0.0 {
                (acc[0] / n / a, acc[1] / n / a, acc[2] / n / a)
            } else {
                (0.0, 0.0, 0.0)
            };
            out.extend_from_slice(&[
                (r * 255.0).round().clamp(0.0, 255.0) as u8,
                (g * 255.0).round().clamp(0.0, 255.0) as u8,
                (b * 255.0).round().clamp(0.0, 255.0) as u8,
                (a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
    out
}

/// Farbe (RGBA, 0–1) an einer Stelle im Einheitsquadrat (0..1, 0..1).
fn icon_sample(x: f32, y: f32) -> [f32; 4] {
    const BG: [f32; 4] = [0.086, 0.094, 0.114, 1.0];
    const ORANGE: [f32; 4] = [1.0, 0.69, 0.0, 1.0];
    const LIGHT: [f32; 4] = [0.80, 0.83, 0.88, 1.0];
    const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

    // Abgerundete Kachel.
    if !icon_in_rounded_rect(x, y, 0.02, 0.02, 0.98, 0.98, 0.22) {
        return CLEAR;
    }
    // Orangener Ring nahe am Rand.
    let ring_outer = icon_in_rounded_rect(x, y, 0.08, 0.08, 0.92, 0.92, 0.17);
    let ring_inner = icon_in_rounded_rect(x, y, 0.14, 0.14, 0.86, 0.86, 0.12);
    if ring_outer && !ring_inner {
        return ORANGE;
    }
    // 2×2-Fensterraster; oben links ist der aktive (orange) Client.
    let cells = [
        (0.24, 0.24, 0.47, 0.47, ORANGE),
        (0.53, 0.24, 0.76, 0.47, LIGHT),
        (0.24, 0.53, 0.47, 0.76, LIGHT),
        (0.53, 0.53, 0.76, 0.76, LIGHT),
    ];
    for (x0, y0, x1, y1, color) in cells {
        if icon_in_rounded_rect(x, y, x0, y0, x1, y1, 0.04) {
            return color;
        }
    }
    BG
}

/// Liegt der Punkt in einem Rechteck mit abgerundeten Ecken (Radius `r`)?
fn icon_in_rounded_rect(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
    if x < x0 || x > x1 || y < y0 || y > y1 {
        return false;
    }
    let cx = x.clamp(x0 + r, x1 - r);
    let cy = y.clamp(y0 + r, y1 - r);
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}

/// Ein Icon-Bild im Windows-Ressourcenformat (BITMAPINFOHEADER + BGRA von unten + Maske).
/// Wird für `CreateIconFromResourceEx` (Tray) und als Eintrag der .ico-Datei genutzt.
pub fn icon_dib(size: u32) -> Vec<u8> {
    let rgba = icon_rgba(size);
    let mask_row = size.div_ceil(32) * 4;
    let pixel_bytes = size * size * 4;
    let mask_bytes = mask_row * size;
    let mut out = Vec::with_capacity((40 + pixel_bytes + mask_bytes) as usize);
    // BITMAPINFOHEADER (40 Byte); Höhe doppelt wegen Farb- und Maskenteil.
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // Ebenen
    out.extend_from_slice(&32u16.to_le_bytes()); // Bit pro Pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(pixel_bytes + mask_bytes).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // Auflösung + Farbtabelle ungenutzt
    // Pixel von unten nach oben, BGRA.
    for row in (0..size).rev() {
        for col in 0..size {
            let i = ((row * size + col) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // UND-Maske: alles 0 (die Transparenz steckt im Alphakanal).
    out.extend(std::iter::repeat_n(0u8, mask_bytes as usize));
    out
}

/// Komplette .ico-Datei mit mehreren Größen.
pub fn icon_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| icon_dib(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserviert
    out.extend_from_slice(&1u16.to_le_bytes()); // Typ: Icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0u8 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

/// Icon als PNG-Datei (unkomprimiert, ohne zusätzliche Abhängigkeiten) – für Linux-Menüeinträge.
#[allow(dead_code)]
pub fn icon_png(size: u32) -> Vec<u8> {
    let rgba = icon_rgba(size);
    // Rohdaten: je Zeile ein Filter-Byte (0) plus RGBA-Pixel.
    let mut raw = Vec::with_capacity(((size * 4 + 1) * size) as usize);
    for row in rgba.chunks((size * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    // zlib-Strom aus „stored“-Blöcken (je max. 65535 Byte).
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&png_adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 Bit, RGBA, Standard-Kompression/Filter, kein Interlace
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &zlib);
    png_chunk(&mut out, b"IEND", &[]);
    out
}

/// Hängt einen PNG-Chunk (Länge, Typ, Daten, CRC) an.
#[allow(dead_code)]
fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = png_crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// CRC-32 (wie in PNG/zlib verwendet).
#[allow(dead_code)]
fn png_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Adler-32-Prüfsumme (Ende des zlib-Stroms).
#[allow(dead_code)]
fn png_adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Größe, Transparenz der Ecken und deckende Mitte stimmen.
    #[test]
    fn icon_pixels() {
        let px = icon_rgba(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert_eq!(px[3], 0, "Ecke oben links ist transparent");
        let mid = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(px[mid + 3], 255, "Mitte ist deckend");
    }

    /// PNG-Signatur, IHDR und Prüfsummen stimmen (Referenzwerte aus zlib).
    #[test]
    fn png_layout() {
        assert_eq!(png_crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(png_adler32(b"Wikipedia"), 0x11E6_0398);
        let png = icon_png(16);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    /// Die .ico-Datei hat Kopf, Verzeichnis und die erwartete Gesamtlänge.
    #[test]
    fn ico_layout() {
        let ico = icon_ico(&[16, 32]);
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 2, 0]);
        let expected = 6 + 2 * 16 + icon_dib(16).len() + icon_dib(32).len();
        assert_eq!(ico.len(), expected);
    }
}
