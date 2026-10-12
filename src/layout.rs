//! Layout-Logik ohne Win32: Charaktername aus Fenstertitel, Wechsel-Reihenfolge,
//! Zuordnung Client → Platz und Vorlagen-Generatoren für Monitore.

use crate::config::{Profile, Rect};

/// Liest den Charakternamen aus einem EVE-Fenstertitel („EVE - Name“).
/// Am Login-Bildschirm heißt das Fenster nur „EVE“ → `None`.
pub fn character_from_title(title: &str) -> Option<String> {
    let name = title.strip_prefix("EVE - ")?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Reihenfolge der Clients zum Durchschalten und für die Platzvergabe:
/// zuerst Charaktere mit Regel (in Regel-Reihenfolge), dann übrige alphabetisch,
/// zuletzt Clients am Login-Bildschirm.
pub fn cycle_order(chars: &[Option<&str>], profile: &Profile) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..chars.len()).collect();
    idx.sort_by_key(|&i| {
        let rule_pos = chars[i]
            .and_then(|c| profile.clients.iter().position(|r| r.character.eq_ignore_ascii_case(c)))
            .unwrap_or(usize::MAX);
        let login = chars[i].is_none();
        let name = chars[i].map(str::to_lowercase).unwrap_or_default();
        (login, rule_pos, name, i)
    });
    idx
}

/// Eine geplante Fensterplatzierung: Index in der Client-Liste + Zielrechteck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub index: usize,
    pub rect: Rect,
}

/// Plant das Layout: der Haupt-Client kommt in den Hauptplatz, alle anderen der
/// Reihe nach in die Nebenplätze. Clients ohne freien Platz bleiben unberührt.
pub fn plan_layout(chars: &[Option<&str>], profile: &Profile, main: Option<usize>) -> Vec<Placement> {
    let order = cycle_order(chars, profile);
    let main = main.filter(|&m| m < chars.len()).or(order.first().copied());
    let Some(main) = main else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(chars.len());
    if profile.main_slot.is_valid() {
        out.push(Placement {
            index: main,
            rect: profile.main_slot,
        });
    }
    let others = order.into_iter().filter(|&i| i != main);
    let slots = profile.secondary_slots.iter().filter(|r| r.is_valid());
    for (index, &rect) in others.zip(slots) {
        out.push(Placement { index, rect });
    }
    out
}

/// Nächster (oder vorheriger) Client in der Wechsel-Reihenfolge, ausgehend vom aktuellen.
pub fn cycle_step(order: &[usize], current: Option<usize>, forward: bool) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let Some(pos) = current.and_then(|c| order.iter().position(|&i| i == c)) else {
        return Some(order[0]);
    };
    let n = order.len();
    let next = if forward { (pos + 1) % n } else { (pos + n - 1) % n };
    Some(order[next])
}

/// Vorlage „Haupt + Raster“: Hauptfenster füllt `main_area`, Nebenfenster bilden ein
/// Raster `cols`×`rows` in `grid_area` (gleicher oder anderer Monitor).
pub fn preset_main_plus_grid(main_area: Rect, grid_area: Rect, cols: u32, rows: u32) -> (Rect, Vec<Rect>) {
    let cols = cols.max(1) as i32;
    let rows = rows.max(1) as i32;
    let cw = grid_area.w / cols;
    let ch = grid_area.h / rows;
    let mut slots = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            slots.push(Rect::new(grid_area.x + c * cw, grid_area.y + r * ch, cw, ch));
        }
    }
    (main_area, slots)
}

/// Vorlage „Alle gleich groß, gestapelt“: Haupt- und alle Nebenfenster belegen denselben
/// ganzen Monitor. Umgeschaltet wird per Vorschau/Hotkey – der aktive Client liegt vorne.
pub fn preset_stacked(area: Rect, clients: u32) -> (Rect, Vec<Rect>) {
    let secondaries = clients.max(1) as usize - 1;
    (area, vec![area; secondaries])
}

/// Vorlage „Haupt + Seitenleiste“ auf einem Monitor: rechts (oder links) eine Leiste
/// mit `percent` % der Breite, darin `count` Nebenfenster übereinander.
pub fn preset_main_with_sidebar(area: Rect, count: u32, percent: u32, sidebar_left: bool) -> (Rect, Vec<Rect>) {
    let count = count.max(1) as i32;
    let percent = percent.clamp(10, 50) as i32;
    let side_w = area.w * percent / 100;
    let main_w = area.w - side_w;
    let (main_x, side_x) = if sidebar_left {
        (area.x + side_w, area.x)
    } else {
        (area.x, area.x + main_w)
    };
    let main = Rect::new(main_x, area.y, main_w, area.h);
    let sh = area.h / count;
    let slots = (0..count)
        .map(|i| Rect::new(side_x, area.y + i * sh, side_w, sh))
        .collect();
    (main, slots)
}

/// Rastet ein verschobenes Rechteck (Vorschaufenster) an benachbarten Fenstern und an
/// Bildschirmrändern ein. Pro Achse wird der nächste Kandidat innerhalb von `distance`
/// Pixeln genommen: Kante an Kante (mit `gap` Abstand) oder bündig an derselben Linie.
/// Andere Fenster zählen nur, wenn sie auf der jeweils anderen Achse in der Nähe liegen.
pub fn snap_rect(r: Rect, others: &[Rect], bounds: &[Rect], distance: i32, gap: i32) -> Rect {
    let mut xs: Vec<i32> = Vec::new();
    let mut ys: Vec<i32> = Vec::new();
    for o in others {
        let near_vert = r.y < o.bottom() + distance && r.bottom() > o.y - distance;
        let near_horz = r.x < o.right() + distance && r.right() > o.x - distance;
        if near_vert {
            xs.extend([o.right() + gap, o.x - r.w - gap, o.x, o.right() - r.w]);
        }
        if near_horz {
            ys.extend([o.bottom() + gap, o.y - r.h - gap, o.y, o.bottom() - r.h]);
        }
    }
    for b in bounds {
        xs.extend([b.x, b.right() - r.w]);
        ys.extend([b.y, b.bottom() - r.h]);
    }
    let best = |current: i32, candidates: &[i32]| {
        candidates
            .iter()
            .copied()
            .filter(|c| (c - current).abs() <= distance)
            .min_by_key(|c| (c - current).abs())
            .unwrap_or(current)
    };
    Rect::new(best(r.x, &xs), best(r.y, &ys), r.w, r.h)
}

/// Welche Kanten beim Größe-Ziehen bewegt werden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

/// Rastet beim Größe-Ziehen die bewegten Kanten an Nachbarfenstern (Kante an Kante mit
/// `gap` oder bündig) und an Bildschirmrändern ein; unbewegte Kanten bleiben, wo sie sind.
pub fn snap_resize(r: Rect, edges: Edges, others: &[Rect], bounds: &[Rect], distance: i32, gap: i32) -> Rect {
    let (mut lx, mut rx, mut ty, mut by) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for o in others {
        let near_vert = r.y < o.bottom() + distance && r.bottom() > o.y - distance;
        let near_horz = r.x < o.right() + distance && r.right() > o.x - distance;
        if near_vert {
            lx.extend([o.right() + gap, o.x]);
            rx.extend([o.x - gap, o.right()]);
        }
        if near_horz {
            ty.extend([o.bottom() + gap, o.y]);
            by.extend([o.y - gap, o.bottom()]);
        }
    }
    for b in bounds {
        lx.push(b.x);
        rx.push(b.right());
        ty.push(b.y);
        by.push(b.bottom());
    }
    let pick = |current: i32, candidates: &[i32]| {
        candidates
            .iter()
            .copied()
            .filter(|c| (c - current).abs() <= distance)
            .min_by_key(|c| (c - current).abs())
            .unwrap_or(current)
    };
    let left = if edges.left { pick(r.x, &lx) } else { r.x };
    let top = if edges.top { pick(r.y, &ty) } else { r.y };
    let right = if edges.right { pick(r.right(), &rx) } else { r.right() };
    let bottom = if edges.bottom {
        pick(r.bottom(), &by)
    } else {
        r.bottom()
    };
    Rect::new(left, top, right - left, bottom - top)
}

/// Greifzone eines Punkts (x, y) im Rechteck `r`: `Some(Kanten)` innerhalb von `edge`
/// Pixeln am Rand bzw. an der Ecke (Größe ändern), `None` in der Innenfläche (verschieben).
pub fn grab_edges(r: Rect, x: i32, y: i32, edge: i32) -> Option<Edges> {
    let e = Edges {
        left: x < r.x + edge,
        right: x >= r.right() - edge,
        top: y < r.y + edge,
        bottom: y >= r.bottom() - edge,
    };
    (e.left || e.right || e.top || e.bottom).then_some(e)
}

/// Neues Rechteck beim Größe-Ziehen: bewegte Kanten um (dx, dy) verschieben, Mindestgröße halten.
pub fn resize_rect(start: Rect, e: Edges, dx: i32, dy: i32, min: (i32, i32)) -> Rect {
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.right(), start.bottom());
    if e.left {
        l = (l + dx).min(r - min.0);
    }
    if e.right {
        r = (r + dx).max(l + min.0);
    }
    if e.top {
        t = (t + dy).min(b - min.1);
    }
    if e.bottom {
        b = (b + dy).max(t + min.1);
    }
    Rect::new(l, t, r - l, b - t)
}

/// Automatische Position der `index`-ten Vorschau: lückenlos als Reihe (oder Spalte)
/// ab `start`, mit `gap` Abstand und Umbruch nach `per_line` Vorschauen (0 = nie).
pub fn auto_preview_pos(
    index: usize,
    start: (i32, i32),
    size: (i32, i32),
    gap: i32,
    vertical: bool,
    per_line: u32,
) -> (i32, i32) {
    let (line, pos) = if per_line == 0 {
        (0, index as i32)
    } else {
        ((index / per_line as usize) as i32, (index % per_line as usize) as i32)
    };
    let (step_x, step_y) = (size.0 + gap, size.1 + gap);
    if vertical {
        (start.0 + line * step_x, start.1 + pos * step_y)
    } else {
        (start.0 + pos * step_x, start.1 + line * step_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientRule;

    /// Hilfsfunktion: Profil mit Regeln in vorgegebener Reihenfolge.
    fn profile_with(names: &[&str]) -> Profile {
        Profile {
            clients: names
                .iter()
                .map(|n| ClientRule {
                    character: n.to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Titel werden korrekt zerlegt.
    #[test]
    fn title_parsing() {
        assert_eq!(
            character_from_title("EVE - Legga Schnegge"),
            Some("Legga Schnegge".into())
        );
        assert_eq!(character_from_title("EVE"), None);
        assert_eq!(character_from_title("EVE - "), None);
        assert_eq!(character_from_title("Notepad"), None);
    }

    /// Regeln bestimmen die Reihenfolge, Rest alphabetisch, Login zuletzt.
    #[test]
    fn order_rules_first() {
        let p = profile_with(&["Zed", "Alpha"]);
        let chars = [None, Some("bravo"), Some("Alpha"), Some("Zed"), Some("Charlie")];
        assert_eq!(cycle_order(&chars, &p), vec![3, 2, 1, 4, 0]);
    }

    /// Hauptfenster kommt in den Hauptplatz, Rest in Nebenplätze.
    #[test]
    fn plan_assigns_slots() {
        let p = profile_with(&["A", "B", "C"]);
        let chars = [Some("C"), Some("A"), Some("B")];
        let plan = plan_layout(&chars, &p, Some(2)); // B ist Haupt
        assert_eq!(
            plan[0],
            Placement {
                index: 2,
                rect: p.main_slot
            }
        );
        assert_eq!(plan[1].index, 1); // A
        assert_eq!(plan[2].index, 0); // C
        assert_eq!(plan[1].rect, p.secondary_slots[0]);
    }

    /// Ohne Hauptvorgabe wird der Erste der Reihenfolge Haupt; zu wenige Plätze → Rest bleibt.
    #[test]
    fn plan_without_main_and_overflow() {
        let mut p = profile_with(&[]);
        p.secondary_slots.truncate(1);
        let chars = [Some("b"), Some("a"), Some("c")];
        let plan = plan_layout(&chars, &p, None);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].index, 1);
        assert_eq!(plan[1].index, 0);
    }

    /// Durchschalten läuft im Kreis, auch rückwärts.
    #[test]
    fn cycling() {
        let order = [2, 0, 1];
        assert_eq!(cycle_step(&order, Some(1), true), Some(2));
        assert_eq!(cycle_step(&order, Some(2), false), Some(1));
        assert_eq!(cycle_step(&order, None, true), Some(2));
        assert_eq!(cycle_step(&[], None, true), None);
    }

    /// Raster-Vorlage erzeugt cols×rows gleich große Felder.
    #[test]
    fn grid_preset() {
        let (main, slots) = preset_main_plus_grid(Rect::new(0, 0, 2560, 1440), Rect::new(2560, 0, 1920, 1080), 2, 2);
        assert_eq!(main, Rect::new(0, 0, 2560, 1440));
        assert_eq!(slots.len(), 4);
        assert_eq!(slots[3], Rect::new(2560 + 960, 540, 960, 540));
    }

    /// Gestapelt: alle Plätze sind identisch mit dem Monitor.
    #[test]
    fn stacked_preset() {
        let area = Rect::new(0, 0, 3440, 1440);
        let (main, slots) = preset_stacked(area, 5);
        assert_eq!(main, area);
        assert_eq!(slots, vec![area; 4]);
        assert!(preset_stacked(area, 0).1.is_empty());
    }

    /// Einrasten rechts neben einem Nachbarn und bündig an dessen Oberkante.
    #[test]
    fn snaps_to_neighbour() {
        let other = Rect::new(100, 100, 200, 120);
        let moved = Rect::new(310, 108, 200, 120); // 10 px rechts daneben, 8 px tiefer
        let r = snap_rect(moved, &[other], &[], 15, 0);
        assert_eq!((r.x, r.y), (300, 100));
        let r = snap_rect(moved, &[other], &[], 15, 4);
        assert_eq!((r.x, r.y), (304, 100));
    }

    /// Weit entfernte Fenster und Ränder ändern nichts; Bildschirmrand rastet ein.
    #[test]
    fn snap_far_and_edges() {
        let other = Rect::new(100, 100, 200, 120);
        let far = Rect::new(900, 700, 200, 120);
        assert_eq!(snap_rect(far, &[other], &[], 15, 0), far);
        let screen = Rect::new(0, 0, 1920, 1080);
        let near_edge = Rect::new(7, 1080 - 120 - 5, 200, 120);
        let r = snap_rect(near_edge, &[], &[screen], 15, 0);
        assert_eq!((r.x, r.y), (0, 960));
    }

    /// Größe ziehen: nur die bewegte Kante rastet ein.
    #[test]
    fn resize_snaps_moving_edge() {
        let other = Rect::new(500, 100, 200, 120);
        let r = Rect::new(100, 100, 393, 113); // rechte Kante 7 px vor dem Nachbarn
        let edges = Edges {
            right: true,
            bottom: true,
            ..Default::default()
        };
        let s = snap_resize(r, edges, &[other], &[], 15, 2);
        assert_eq!((s.x, s.y, s.right(), s.bottom()), (100, 100, 498, 220));
        let none = snap_resize(r, Edges::default(), &[other], &[], 15, 2);
        assert_eq!(none, r);
    }

    /// Greifzonen: Ecke, Kante und Innenfläche.
    #[test]
    fn grab_zones() {
        let r = Rect::new(100, 100, 200, 100);
        let corner = grab_edges(r, 102, 102, 7).unwrap();
        assert!(corner.left && corner.top && !corner.right && !corner.bottom);
        let right = grab_edges(r, 297, 150, 7).unwrap();
        assert!(right.right && !right.top);
        assert_eq!(grab_edges(r, 200, 150, 7), None);
    }

    /// Größe ziehen hält die Mindestgröße ein.
    #[test]
    fn resize_respects_minimum() {
        let start = Rect::new(0, 0, 200, 100);
        let e = Edges {
            left: true,
            ..Default::default()
        };
        assert_eq!(resize_rect(start, e, -50, 0, (60, 40)), Rect::new(-50, 0, 250, 100));
        assert_eq!(resize_rect(start, e, 500, 0, (60, 40)), Rect::new(140, 0, 60, 100));
    }

    /// Automatische Anordnung als Reihe mit Umbruch und als Spalte.
    #[test]
    fn auto_positions() {
        assert_eq!(auto_preview_pos(2, (10, 20), (100, 50), 4, false, 0), (218, 20));
        assert_eq!(auto_preview_pos(3, (10, 20), (100, 50), 4, false, 3), (10, 74));
        assert_eq!(auto_preview_pos(2, (10, 20), (100, 50), 4, true, 0), (10, 128));
    }

    /// Seitenleiste teilt die Breite korrekt auf.
    #[test]
    fn sidebar_preset() {
        let (main, slots) = preset_main_with_sidebar(Rect::new(0, 0, 2000, 1200), 3, 25, false);
        assert_eq!(main, Rect::new(0, 0, 1500, 1200));
        assert_eq!(slots[2], Rect::new(1500, 800, 500, 400));
        let (main_l, slots_l) = preset_main_with_sidebar(Rect::new(0, 0, 2000, 1200), 1, 25, true);
        assert_eq!(main_l.x, 500);
        assert_eq!(slots_l[0].x, 0);
    }
}
