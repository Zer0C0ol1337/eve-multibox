//! Konfiguration: Profile (Fenster-Layouts), Vorschau, Leistung und Hotkeys.
//!
//! Wird als JSON unter `%APPDATA%\EveMultibox\config.json` gespeichert.
//! Alle Strukturen nutzen `#[serde(default)]`, damit ältere oder unvollständige
//! Dateien weiterhin geladen werden können.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use crate::i18n::{Lang, t};

/// Rechteck in virtuellen Bildschirmkoordinaten (alle Monitore zusammen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Erzeugt ein Rechteck aus Position und Größe.
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Rechter Rand (exklusiv).
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    /// Unterer Rand (exklusiv).
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// Prüft, ob das Rechteck eine sinnvolle Größe hat.
    pub fn is_valid(&self) -> bool {
        self.w > 0 && self.h > 0
    }
}

/// Windows-Prioritätsklassen, die wir setzen. „Echtzeit“ ist bewusst nicht
/// enthalten – sie kann das ganze System einfrieren.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Priority {
    Idle,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
}

impl Priority {
    /// Alle wählbaren Stufen in aufsteigender Reihenfolge.
    pub const ALL: [Priority; 5] = [
        Priority::Idle,
        Priority::BelowNormal,
        Priority::Normal,
        Priority::AboveNormal,
        Priority::High,
    ];

    /// Bezeichnung in der aktiven Sprache.
    pub fn label(self) -> &'static str {
        t(match self {
            Priority::Idle => "prio.idle",
            Priority::BelowNormal => "prio.below",
            Priority::Normal => "prio.normal",
            Priority::AboveNormal => "prio.above",
            Priority::High => "prio.high",
        })
    }
}

/// Einstellungen für einen bestimmten Charakter innerhalb eines Profils.
/// Die Reihenfolge der Regeln bestimmt Wechsel-Reihenfolge und Nebenfenster-Plätze.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ClientRule {
    /// Charaktername, wie er im Fenstertitel „EVE - Name“ steht.
    pub character: String,
    /// Hotkey zum direkten Umschalten auf diesen Client, z. B. „Ctrl+F1“ (leer = keiner).
    pub hotkey: String,
    /// Feste Priorität im Vordergrund (None = Vorgabe aus den Leistungseinstellungen).
    pub priority: Option<Priority>,
    /// Logische CPU-Kerne, auf denen der Client laufen darf (leer = alle).
    pub cores: Vec<u32>,
}

/// Ein Fenster-Layout: ein Hauptfenster-Platz und beliebig viele Nebenfenster-Plätze.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    /// Platz des Hauptfensters (aktiver Haupt-Charakter).
    pub main_slot: Rect,
    /// Plätze der Nebenfenster, der Reihe nach belegt.
    pub secondary_slots: Vec<Rect>,
    /// Fensterrahmen und Titelleiste entfernen.
    pub borderless: bool,
    /// Layout automatisch anwenden, wenn ein Client startet oder sich ein Charakter einloggt.
    pub auto_apply: bool,
    /// Beim Umschalten per Hotkey/Vorschau den Client gleichzeitig ins Hauptfenster tauschen.
    pub swap_on_activate: bool,
    /// Charakter, der beim Start das Hauptfenster bekommt (leer = Vordergrund/erster).
    pub default_main: String,
    /// Charakter-Regeln (Hotkey, Priorität, Kerne) in gewünschter Reihenfolge.
    pub clients: Vec<ClientRule>,
}

impl Default for Profile {
    /// Standardprofil: 1920×1080-Hauptfenster links, drei Nebenfenster rechts daneben.
    fn default() -> Self {
        Self {
            name: "Standard".into(),
            main_slot: Rect::new(0, 0, 1920, 1080),
            secondary_slots: vec![
                Rect::new(1920, 0, 960, 540),
                Rect::new(2880, 0, 960, 540),
                Rect::new(1920, 540, 960, 540),
            ],
            borderless: false,
            auto_apply: false,
            swap_on_activate: false,
            default_main: String::new(),
            clients: Vec::new(),
        }
    }
}

impl Profile {
    /// Sucht die Regel für einen Charakter (Groß-/Kleinschreibung egal).
    pub fn rule(&self, character: &str) -> Option<&ClientRule> {
        self.clients
            .iter()
            .find(|r| r.character.eq_ignore_ascii_case(character))
    }
}

/// Einstellungen der Live-Vorschau (DWM-Thumbnails wie bei EVE-O Preview).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewSettings {
    pub enabled: bool,
    pub width: i32,
    pub height: i32,
    /// Deckkraft 30–255.
    pub opacity: u8,
    /// Charaktername über dem Bild anzeigen.
    pub show_titles: bool,
    /// Vorschau des gerade aktiven Clients ausblenden.
    pub hide_active: bool,
    /// Vorschauen nur zeigen, solange ein EVE-Client im Vordergrund ist.
    pub only_when_eve_focused: bool,
    /// Aktiven Client farbig umranden.
    pub highlight_active: bool,
    /// Startpunkt für automatisch platzierte Vorschauen.
    pub start_x: i32,
    pub start_y: i32,
    /// Gespeicherte Positionen je Charakter (per Ziehen gesetzt).
    pub positions: BTreeMap<String, (i32, i32)>,
    /// Gespeicherte Bildgrößen je Charakter (per Größe-Ziehen gesetzt; sonst width/height).
    pub sizes: BTreeMap<String, (i32, i32)>,
    /// Beim Ziehen an anderen Vorschauen und Bildschirmrändern einrasten.
    pub snap: bool,
    /// Fangbereich fürs Einrasten in Pixeln.
    pub snap_distance: i32,
    /// Abstand zwischen angedockten bzw. automatisch angeordneten Vorschauen.
    pub gap: i32,
    /// Automatische Anordnung als Spalte statt als Reihe.
    pub vertical: bool,
    /// Umbruch nach so vielen Vorschauen (0 = nie).
    pub per_line: u32,
}

impl Default for PreviewSettings {
    /// Vernünftige Vorgaben: 320×180, kaum transparent, mit Titel und Rahmen.
    fn default() -> Self {
        Self {
            enabled: true,
            width: 320,
            height: 180,
            opacity: 230,
            show_titles: true,
            hide_active: false,
            only_when_eve_focused: false,
            highlight_active: true,
            start_x: 20,
            start_y: 20,
            positions: BTreeMap::new(),
            sizes: BTreeMap::new(),
            snap: true,
            snap_distance: 20,
            gap: 2,
            vertical: false,
            per_line: 0,
        }
    }
}

/// CPU- und Speicher-Werkzeuge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceSettings {
    /// Inaktive Clients drosseln (niedrigere Priorität, optional eigene Kerne).
    pub throttle_inactive: bool,
    /// Priorität des Vordergrund-Clients, falls keine Charakter-Regel greift.
    pub active_priority: Priority,
    /// Priorität der Hintergrund-Clients beim Drosseln.
    pub inactive_priority: Priority,
    /// Kerne für gedrosselte Hintergrund-Clients (leer = Kerne der Regel bzw. alle).
    pub background_cores: Vec<u32>,
    /// Client minimieren, sobald ein anderer EVE-Client in den Vordergrund kommt.
    pub minimize_inactive: bool,
    /// Warnung, wenn ein Client mehr als so viele MB Arbeitsspeicher belegt.
    pub ram_warn_mb: u64,
    /// Warnung, wenn weniger als so viel Prozent System-RAM frei sind.
    pub system_ram_warn_percent: u8,
    /// Arbeitsspeicher von Hintergrund-Clients regelmäßig freigeben.
    pub auto_trim: bool,
    pub auto_trim_minutes: u32,
}

impl Default for PerformanceSettings {
    /// Vorgaben: alles aus, nur Warnungen ab 2,5 GB pro Client bzw. <10 % frei.
    fn default() -> Self {
        Self {
            throttle_inactive: false,
            active_priority: Priority::Normal,
            inactive_priority: Priority::BelowNormal,
            background_cores: Vec::new(),
            minimize_inactive: false,
            ram_warn_mb: 2500,
            system_ram_warn_percent: 10,
            auto_trim: false,
            auto_trim_minutes: 15,
        }
    }
}

/// Globale Hotkeys (leer = deaktiviert).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeySettings {
    pub cycle_next: String,
    pub cycle_prev: String,
    pub swap_main: String,
    pub apply_layout: String,
    pub toggle_previews: String,
    /// Hotkeys nur aktiv, solange ein EVE-Client (oder EVE Multibox) im Vordergrund ist.
    /// So werden andere Programme (Browser, Discord …) nicht blockiert.
    pub only_when_eve_focused: bool,
}

impl Default for HotkeySettings {
    /// Vorgaben, die mit EVE-Standardtasten selten kollidieren.
    fn default() -> Self {
        Self {
            cycle_next: "Ctrl+Alt+PageDown".into(),
            cycle_prev: "Ctrl+Alt+PageUp".into(),
            swap_main: "Ctrl+Alt+S".into(),
            apply_layout: "Ctrl+Alt+L".into(),
            toggle_previews: "Ctrl+Alt+P".into(),
            only_when_eve_focused: true,
        }
    }
}

/// Allgemeine Programmeinstellungen (Sprache, Tray).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    /// Oberflächensprache (None = Windows-Sprache übernehmen).
    pub language: Option<Lang>,
    /// Schließen (X) blendet das Dashboard in den Infobereich aus, statt zu beenden.
    pub close_to_tray: bool,
    /// Beim Start nur im Infobereich erscheinen.
    pub start_minimized: bool,
}

impl Default for GeneralSettings {
    /// Vorgaben: Sprache automatisch, X schließt ins Tray, Start mit sichtbarem Dashboard.
    fn default() -> Self {
        Self {
            language: None,
            close_to_tray: true,
            start_minimized: false,
        }
    }
}

/// Gesamte Konfiguration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralSettings,
    pub active_profile: String,
    pub profiles: Vec<Profile>,
    pub preview: PreviewSettings,
    pub performance: PerformanceSettings,
    pub hotkeys: HotkeySettings,
}

impl Default for Config {
    /// Neue Konfiguration mit einem Standardprofil.
    fn default() -> Self {
        Self {
            general: GeneralSettings::default(),
            active_profile: "Standard".into(),
            profiles: vec![Profile::default()],
            preview: PreviewSettings::default(),
            performance: PerformanceSettings::default(),
            hotkeys: HotkeySettings::default(),
        }
    }
}

impl Config {
    /// Repariert ungültige Werte (kein Profil, unbekanntes aktives Profil, Grenzwerte).
    pub fn normalize(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(Profile::default());
        }
        if !self.profiles.iter().any(|p| p.name == self.active_profile) {
            self.active_profile = self.profiles[0].name.clone();
        }
        self.preview.width = self.preview.width.clamp(64, 1920);
        self.preview.height = self.preview.height.clamp(36, 1080);
        self.preview.opacity = self.preview.opacity.max(30);
        self.preview.snap_distance = self.preview.snap_distance.clamp(2, 100);
        self.preview.gap = self.preview.gap.clamp(0, 100);
        self.performance.auto_trim_minutes = self.performance.auto_trim_minutes.max(1);
        self.performance.system_ram_warn_percent = self.performance.system_ram_warn_percent.min(90);
    }

    /// Aktives Profil (nach `normalize` immer vorhanden).
    pub fn active_profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.name == self.active_profile)
            .unwrap_or(&self.profiles[0])
    }

    /// Veränderbarer Zugriff auf das aktive Profil.
    pub fn active_profile_mut(&mut self) -> &mut Profile {
        let idx = self
            .profiles
            .iter()
            .position(|p| p.name == self.active_profile)
            .unwrap_or(0);
        &mut self.profiles[idx]
    }
}

/// Wandelt eine Kernliste in eine Affinitätsmaske um (max. 64 Kerne = eine Prozessorgruppe).
pub fn cores_to_mask(cores: &[u32]) -> u64 {
    cores.iter().filter(|&&c| c < 64).fold(0u64, |m, &c| m | (1u64 << c))
}

/// Wandelt eine Affinitätsmaske in eine sortierte Kernliste um.
pub fn mask_to_cores(mask: u64) -> Vec<u32> {
    (0..64).filter(|c| mask & (1u64 << c) != 0).collect()
}

/// Kurze Darstellung einer Kernliste, z. B. „0-3, 6“ (leer = „alle“ in der aktiven Sprache).
pub fn cores_label(cores: &[u32]) -> String {
    if cores.is_empty() {
        return t("cores.all").into();
    }
    let mut sorted = cores.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts = Vec::new();
    let mut start = sorted[0];
    let mut prev = sorted[0];
    for &c in &sorted[1..] {
        if c == prev + 1 {
            prev = c;
            continue;
        }
        parts.push(range_str(start, prev));
        start = c;
        prev = c;
    }
    parts.push(range_str(start, prev));
    parts.join(", ")
}

/// Hilfsfunktion für `cores_label`: „3“ oder „0-3“.
fn range_str(a: u32, b: u32) -> String {
    if a == b { a.to_string() } else { format!("{a}-{b}") }
}

/// Speicherort der Konfigurationsdatei.
/// Windows: `%APPDATA%\EveMultibox\config.json`;
/// Linux: `$XDG_CONFIG_HOME/eve-multibox/config.json` (Standard `~/.config/…`).
/// Ersatzweise neben dem Programm.
pub fn config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("EveMultibox").join("config.json")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("eve-multibox").join("config.json")
    }
}

/// Lädt die Konfiguration; bei fehlender oder defekter Datei gibt es die Vorgaben.
/// Eine defekte Datei wird als `config.json.bak` gesichert statt überschrieben.
pub fn load() -> Config {
    let path = config_path();
    let mut cfg = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Config>(&text) {
            Ok(c) => c,
            Err(_) => {
                let _ = std::fs::copy(&path, path.with_extension("json.bak"));
                Config::default()
            }
        },
        Err(_) => Config::default(),
    };
    cfg.normalize();
    cfg
}

/// Speichert die Konfiguration atomar (erst Temp-Datei, dann umbenennen).
pub fn save(cfg: &Config) -> io::Result<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Masken-Umrechnung funktioniert in beide Richtungen.
    #[test]
    fn mask_roundtrip() {
        let cores = vec![0, 2, 3, 15];
        assert_eq!(mask_to_cores(cores_to_mask(&cores)), cores);
        assert_eq!(cores_to_mask(&[]), 0);
    }

    /// Kernlisten werden zu Bereichen zusammengefasst.
    #[test]
    fn label_ranges() {
        // Sprache ist global und kann von parallelen Tests umgestellt werden.
        let all = ["alle", "all", "tous", "все", "todos", "全部"];
        assert!(all.contains(&cores_label(&[]).as_str()));
        assert_eq!(cores_label(&[3, 0, 1, 2, 6]), "0-3, 6");
        assert_eq!(cores_label(&[5]), "5");
    }

    /// Unvollständiges JSON wird mit Vorgaben aufgefüllt.
    #[test]
    fn partial_json_loads() {
        let mut c: Config = serde_json::from_str(r#"{"preview":{"width":200}}"#).unwrap();
        c.normalize();
        assert_eq!(c.preview.width, 200);
        assert_eq!(c.preview.height, 180);
        assert_eq!(c.active_profile().name, "Standard");
    }

    /// normalize repariert ein unbekanntes aktives Profil.
    #[test]
    fn normalize_fixes_profile() {
        let mut c = Config {
            active_profile: "gibtsnicht".into(),
            ..Default::default()
        };
        c.normalize();
        assert_eq!(c.active_profile, "Standard");
    }
}
