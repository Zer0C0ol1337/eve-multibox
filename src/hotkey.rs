//! Parser für Hotkey-Texte wie „Ctrl+Shift+F1“ → Modifier-Flags + Virtual-Key-Code.
//!
//! Die Zahlenwerte entsprechen den Win32-Konstanten (MOD_*, VK_*), sind hier aber
//! fest eingetragen, damit dieses Modul ohne Windows-Abhängigkeit testbar bleibt.

use std::fmt;

use crate::i18n::{t, tf};

/// MOD_ALT
pub const MOD_ALT: u32 = 0x0001;
/// MOD_CONTROL
pub const MOD_CONTROL: u32 = 0x0002;
/// MOD_SHIFT
pub const MOD_SHIFT: u32 = 0x0004;
/// MOD_WIN
pub const MOD_WIN: u32 = 0x0008;

/// Ein geparster Hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    /// Kombination aus MOD_*-Flags.
    pub mods: u32,
    /// Win32-Virtual-Key-Code.
    pub vk: u32,
}

/// Tabelle der benannten Tasten (Name klein geschrieben → VK-Code).
const NAMED_KEYS: &[(&str, u32)] = &[
    ("tab", 0x09),
    ("enter", 0x0D),
    ("return", 0x0D),
    ("pause", 0x13),
    ("esc", 0x1B),
    ("escape", 0x1B),
    ("space", 0x20),
    ("leertaste", 0x20),
    ("pageup", 0x21),
    ("bildauf", 0x21),
    ("pagedown", 0x22),
    ("bildab", 0x22),
    ("end", 0x23),
    ("ende", 0x23),
    ("home", 0x24),
    ("pos1", 0x24),
    ("left", 0x25),
    ("links", 0x25),
    ("up", 0x26),
    ("hoch", 0x26),
    ("right", 0x27),
    ("rechts", 0x27),
    ("down", 0x28),
    ("runter", 0x28),
    ("insert", 0x2D),
    ("einfg", 0x2D),
    ("delete", 0x2E),
    ("entf", 0x2E),
    ("nummul", 0x6A),
    ("numadd", 0x6B),
    ("numsub", 0x6D),
    ("numdec", 0x6E),
    ("numdiv", 0x6F),
    ("scrolllock", 0x91),
    ("rollen", 0x91),
];

/// Parst einen Hotkey-Text. Leerer Text ergibt `Ok(None)` (= kein Hotkey).
///
/// Erlaubt: Modifier `Ctrl`/`Strg`, `Alt`, `Shift`/`Umschalt`, `Win` plus genau
/// eine Taste (`A`–`Z`, `0`–`9`, `F1`–`F24`, `Num0`–`Num9` und benannte Tasten).
pub fn parse(text: &str) -> Result<Option<Hotkey>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let mut mods = 0u32;
    let mut vk: Option<u32> = None;
    for raw in text.split('+') {
        let token = raw.trim().to_ascii_lowercase();
        if token.is_empty() {
            return Err(t("hkp.empty_part").into());
        }
        let modifier = match token.as_str() {
            "ctrl" | "strg" | "control" => Some(MOD_CONTROL),
            "alt" => Some(MOD_ALT),
            "shift" | "umschalt" => Some(MOD_SHIFT),
            "win" | "windows" => Some(MOD_WIN),
            _ => None,
        };
        if let Some(m) = modifier {
            mods |= m;
            continue;
        }
        if vk.is_some() {
            return Err(tf("hkp.multi_key", &[&raw.trim()]));
        }
        vk = Some(key_code(&token).ok_or_else(|| tf("hkp.unknown", &[&raw.trim()]))?);
    }
    let vk = vk.ok_or_else(|| t("hkp.no_key").to_string())?;
    Ok(Some(Hotkey { mods, vk }))
}

/// Ermittelt den VK-Code einer einzelnen (klein geschriebenen) Taste.
fn key_code(token: &str) -> Option<u32> {
    // Einzelnes Zeichen: Buchstabe oder Ziffer.
    if token.len() == 1 {
        let c = token.as_bytes()[0];
        return match c {
            b'a'..=b'z' => Some((c - b'a') as u32 + 0x41),
            b'0'..=b'9' => Some((c - b'0') as u32 + 0x30),
            _ => None,
        };
    }
    // Funktionstasten F1–F24.
    if let Some(n) = token.strip_prefix('f').and_then(|s| s.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then(|| 0x70 + n - 1);
    }
    // Ziffernblock Num0–Num9.
    if let Some(n) = token.strip_prefix("num").and_then(|s| s.parse::<u32>().ok()) {
        return (n <= 9).then(|| 0x60 + n);
    }
    NAMED_KEYS
        .iter()
        .find(|(name, _)| *name == token)
        .map(|&(_, code)| code)
}

/// Lesbarer Name eines VK-Codes (Gegenstück zu `key_code`).
fn key_name(vk: u32) -> String {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        _ => NAMED_KEYS
            .iter()
            .find(|&&(_, code)| code == vk)
            .map(|(name, _)| {
                let mut s = name.to_string();
                if let Some(first) = s.get_mut(0..1) {
                    first.make_ascii_uppercase();
                }
                s
            })
            .unwrap_or_else(|| format!("VK{vk:#04x}")),
    }
}

impl fmt::Display for Hotkey {
    /// Normalisierte Schreibweise, z. B. „Ctrl+Shift+F1“.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.mods & MOD_CONTROL != 0 {
            parts.push("Ctrl".to_string());
        }
        if self.mods & MOD_ALT != 0 {
            parts.push("Alt".to_string());
        }
        if self.mods & MOD_SHIFT != 0 {
            parts.push("Shift".to_string());
        }
        if self.mods & MOD_WIN != 0 {
            parts.push("Win".to_string());
        }
        parts.push(key_name(self.vk));
        write!(f, "{}", parts.join("+"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Typische Kombinationen werden korrekt erkannt.
    #[test]
    fn parses_common() {
        assert_eq!(
            parse("Ctrl+F1").unwrap(),
            Some(Hotkey {
                mods: MOD_CONTROL,
                vk: 0x70
            })
        );
        assert_eq!(
            parse(" strg + umschalt + tab ").unwrap(),
            Some(Hotkey {
                mods: MOD_CONTROL | MOD_SHIFT,
                vk: 0x09
            })
        );
        assert_eq!(parse("Alt+Num5").unwrap().unwrap().vk, 0x65);
        assert_eq!(parse("Win+Q").unwrap().unwrap().vk, 0x51);
    }

    /// Leer = kein Hotkey, Fehler bei Unsinn.
    #[test]
    fn empty_and_errors() {
        assert_eq!(parse("").unwrap(), None);
        assert!(parse("Ctrl+Alt").is_err());
        assert!(parse("Ctrl+A+B").is_err());
        assert!(parse("Ctrl+F25").is_err());
        assert!(parse("Ctrl++A").is_err());
        assert!(parse("Hyper+A").is_err());
    }

    /// Anzeige und Parser sind zueinander konsistent.
    #[test]
    fn display_roundtrip() {
        for s in ["Ctrl+Shift+Tab", "Alt+F12", "Ctrl+Alt+S", "Num0", "Ctrl+PageUp"] {
            let hk = parse(s).unwrap().unwrap();
            assert_eq!(parse(&hk.to_string()).unwrap().unwrap(), hk, "{s}");
        }
    }
}
