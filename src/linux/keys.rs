//! Globale Hotkeys unter X11 über `XGrabKey` auf dem Wurzelfenster.
//!
//! Die Hotkey-Texte werden wie unter Windows mit `evemb::hotkey` geparst (Win32-VK-Codes)
//! und hier auf X11-Keysyms und Keycodes übersetzt. NumLock und CapsLock dürfen beim
//! Drücken an sein – dafür wird jede Kombination zusätzlich mit diesen Sperrtasten gegriffen.
//! Unter XWayland greifen die Hotkeys nur, solange ein X11-Fenster (EVE) aktiv ist.

use std::collections::{HashMap, HashSet};

use evemb::hotkey::{Hotkey, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;

/// Keysym der NumLock-Taste.
const XK_NUM_LOCK: u32 = 0xff7f;

/// Win32-VK-Code → X11-Keysym.
fn keysym_of(vk: u32) -> Option<u32> {
    Some(match vk {
        0x41..=0x5A => vk + 0x20,            // A–Z → a–z
        0x30..=0x39 => vk,                   // 0–9
        0x70..=0x87 => 0xffbe + (vk - 0x70), // F1–F24
        0x60..=0x69 => 0xffb0 + (vk - 0x60), // Num0–Num9
        0x09 => 0xff09,                      // Tab
        0x0D => 0xff0d,                      // Enter
        0x13 => 0xff13,                      // Pause
        0x1B => 0xff1b,                      // Esc
        0x20 => 0x0020,                      // Leertaste
        0x21 => 0xff55,                      // Bild auf
        0x22 => 0xff56,                      // Bild ab
        0x23 => 0xff57,                      // Ende
        0x24 => 0xff50,                      // Pos1
        0x25 => 0xff51,                      // Links
        0x26 => 0xff52,                      // Hoch
        0x27 => 0xff53,                      // Rechts
        0x28 => 0xff54,                      // Runter
        0x2D => 0xff63,                      // Einfg
        0x2E => 0xffff,                      // Entf
        0x6A => 0xffaa,                      // Num *
        0x6B => 0xffab,                      // Num +
        0x6D => 0xffad,                      // Num -
        0x6E => 0xffae,                      // Num ,
        0x6F => 0xffaf,                      // Num /
        0x91 => 0xff14,                      // Rollen
        _ => return None,
    })
}

/// Win32-Modifier-Flags → X11-Modifier-Maske.
fn x_mods(mods: u32) -> u16 {
    let mut m = 0u16;
    if mods & MOD_SHIFT != 0 {
        m |= u16::from(ModMask::SHIFT);
    }
    if mods & MOD_CONTROL != 0 {
        m |= u16::from(ModMask::CONTROL);
    }
    if mods & MOD_ALT != 0 {
        m |= u16::from(ModMask::M1);
    }
    if mods & MOD_WIN != 0 {
        m |= u16::from(ModMask::M4);
    }
    m
}

/// Ergebnis eines Registrierungsversuchs.
pub enum GrabError {
    /// Taste gibt es auf dieser Tastatur nicht.
    NoKey,
    /// Ein anderes Programm hat die Kombination schon belegt.
    Taken,
}

/// Verwaltet die gegriffenen Tasten und erkennt, welche Aktion gedrückt wurde.
pub struct KeyGrabber {
    /// Keysym → Keycode laut aktueller Tastaturbelegung.
    keymap: HashMap<u32, Keycode>,
    /// Maske der NumLock-Taste (meist Mod2).
    numlock: u16,
    /// Registrierte Kombinationen (Keycode, Modifier) → Aktions-ID.
    grabs: HashMap<(Keycode, u16), i32>,
    /// Gerade gedrückte Keycodes (gegen Tastenwiederholung).
    held: HashSet<Keycode>,
    /// Letztes Loslassen (Keycode, Zeit) – erkennt Wiederholungen ohne „detectable autorepeat“.
    last_release: Option<(Keycode, u32)>,
}

impl KeyGrabber {
    /// Liest Tastaturbelegung und NumLock-Maske.
    pub fn new(conn: &impl Connection) -> Self {
        let mut g = KeyGrabber {
            keymap: HashMap::new(),
            numlock: u16::from(ModMask::M2),
            grabs: HashMap::new(),
            held: HashSet::new(),
            last_release: None,
        };
        g.reload_mapping(conn);
        g
    }

    /// Liest die Tastaturbelegung neu (z. B. nach einem MappingNotify).
    pub fn reload_mapping(&mut self, conn: &impl Connection) {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        self.keymap.clear();
        if let Some(map) = conn
            .get_keyboard_mapping(min, max - min + 1)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            let per = map.keysyms_per_keycode.max(1) as usize;
            for (i, syms) in map.keysyms.chunks(per).enumerate() {
                let code = min + i as u8;
                for &sym in syms.iter().filter(|&&s| s != 0) {
                    self.keymap.entry(sym).or_insert(code);
                }
            }
        }
        // Welche Modifier-Ebene enthält NumLock?
        if let (Some(&nl), Some(mm)) = (
            self.keymap.get(&XK_NUM_LOCK),
            conn.get_modifier_mapping().ok().and_then(|c| c.reply().ok()),
        ) {
            let per = mm.keycodes_per_modifier() as usize;
            for (bit, codes) in mm.keycodes.chunks(per.max(1)).enumerate() {
                if codes.contains(&nl) {
                    self.numlock = 1 << bit;
                }
            }
        }
    }

    /// Alle Varianten einer Modifier-Maske mit/ohne NumLock und CapsLock.
    fn lock_variants(&self, base: u16) -> [u16; 4] {
        let caps = u16::from(ModMask::LOCK);
        [base, base | caps, base | self.numlock, base | caps | self.numlock]
    }

    /// Greift einen Hotkey für die Aktions-ID `id`.
    pub fn grab(&mut self, conn: &impl Connection, root: Window, hk: Hotkey, id: i32) -> Result<(), GrabError> {
        let code = keysym_of(hk.vk)
            .and_then(|s| self.keymap.get(&s).copied())
            .ok_or(GrabError::NoKey)?;
        let base = x_mods(hk.mods);
        let mut done: Vec<u16> = Vec::new();
        for m in self.lock_variants(base) {
            let ok = conn
                .grab_key(false, root, m.into(), code, GrabMode::ASYNC, GrabMode::ASYNC)
                .ok()
                .and_then(|c| c.check().ok())
                .is_some();
            if !ok {
                for &u in &done {
                    let _ = conn.ungrab_key(code, root, u.into());
                }
                return Err(GrabError::Taken);
            }
            done.push(m);
        }
        self.grabs.insert((code, base), id);
        Ok(())
    }

    /// Gibt alle gegriffenen Tasten frei.
    pub fn ungrab_all(&mut self, conn: &impl Connection, root: Window) {
        let grabs: Vec<(Keycode, u16)> = self.grabs.keys().copied().collect();
        for (code, base) in grabs {
            for m in self.lock_variants(base) {
                let _ = conn.ungrab_key(code, root, m.into());
            }
        }
        self.grabs.clear();
        self.held.clear();
    }

    /// Wertet ein KeyPress aus: Aktions-ID oder None (unbekannt bzw. Tastenwiederholung).
    pub fn on_press(&mut self, ev: &KeyPressEvent) -> Option<i32> {
        let repeat =
            self.held.contains(&ev.detail) || self.last_release.is_some_and(|(c, t)| c == ev.detail && t == ev.time);
        self.held.insert(ev.detail);
        if repeat {
            return None;
        }
        let relevant =
            u16::from(ModMask::SHIFT) | u16::from(ModMask::CONTROL) | u16::from(ModMask::M1) | u16::from(ModMask::M4);
        let state = u16::from(ev.state) & relevant;
        self.grabs.get(&(ev.detail, state)).copied()
    }

    /// Merkt sich ein KeyRelease (für die Erkennung von Tastenwiederholungen).
    pub fn on_release(&mut self, ev: &KeyReleaseEvent) {
        self.held.remove(&ev.detail);
        self.last_release = Some((ev.detail, ev.time));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wichtige Tasten werden auf die richtigen Keysyms abgebildet.
    #[test]
    fn keysyms() {
        assert_eq!(keysym_of(0x41), Some(0x61)); // A → a
        assert_eq!(keysym_of(0x70), Some(0xffbe)); // F1
        assert_eq!(keysym_of(0x87), Some(0xffbe + 23)); // F24
        assert_eq!(keysym_of(0x22), Some(0xff56)); // PageDown
        assert_eq!(keysym_of(0x65), Some(0xffb5)); // Num5
        assert_eq!(keysym_of(0xFF), None);
    }

    /// Modifier-Flags werden korrekt übersetzt.
    #[test]
    fn modifiers() {
        let m = x_mods(MOD_CONTROL | MOD_ALT);
        assert_eq!(m, u16::from(ModMask::CONTROL) | u16::from(ModMask::M1));
        assert_eq!(x_mods(MOD_WIN), u16::from(ModMask::M4));
    }
}
