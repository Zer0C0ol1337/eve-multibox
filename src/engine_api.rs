//! Gemeinsame Schnittstelle zwischen Oberfläche und Engine – für Windows und Linux gleich.
//!
//! Beide Engines (Win32 bzw. X11) stellen ein `EngineHandle` mit derselben API bereit
//! und tauschen genau diese Typen mit der Oberfläche aus.

use evemb::config::{Config, Priority, Rect};

/// Fenster-Kennung, threadsicher als Zahl (Windows: HWND, X11: Window-ID).
pub type Hwnd = isize;

/// Speicherwerte eines Prozesses in Bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemInfo {
    /// Physisch belegter Arbeitsspeicher (Windows: Working Set, Linux: RSS).
    pub working_set: u64,
    /// Privater Speicher (Windows: Commit, Linux: anonymer RSS + Swap).
    pub private: u64,
}

/// Informationen zu einem angeschlossenen Monitor.
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    /// Gerätename, z. B. `\\.\DISPLAY1` oder `DP-1`.
    pub name: String,
    /// Gesamte Fläche in virtuellen Bildschirmkoordinaten.
    pub area: Rect,
    /// Arbeitsbereich ohne Taskleiste/Panels.
    pub work: Rect,
    pub primary: bool,
}

/// Befehle von der Oberfläche an die Engine.
#[derive(Debug)]
pub enum Command {
    /// Neue Konfiguration übernehmen.
    UpdateConfig(Box<Config>),
    /// Client in den Vordergrund holen.
    Activate(Hwnd),
    /// Client ins Hauptfenster tauschen.
    MakeMain(Hwnd),
    /// Layout des aktiven Profils anwenden.
    ApplyLayout,
    /// Aktuelle Fensterpositionen als Layout erfassen.
    CaptureLayout,
    /// Arbeitsspeicher eines Clients freigeben.
    TrimClient(Hwnd),
    /// Arbeitsspeicher aller Hintergrund-Clients freigeben.
    TrimBackground,
    /// Vorschauen ein-/ausblenden (zur Laufzeit).
    TogglePreviews,
    /// Hotkeys vorübergehend abmelden (z. B. während ein Hotkey aufgenommen wird).
    SuspendHotkeys(bool),
    /// Engine beenden (Einstellungen der Clients werden zurückgesetzt).
    Quit,
}

/// Ereignisse von der Engine an die Oberfläche.
#[derive(Debug, Clone)]
pub enum Event {
    /// Eine Vorschau wurde verschoben/vergrößert → Position und Bildgröße speichern
    /// (`size = None`: Standardgröße, keine eigene Größe speichern).
    PreviewMoved {
        key: String,
        x: i32,
        y: i32,
        size: Option<(i32, i32)>,
    },
    /// Ergebnis von `CaptureLayout`: Rechtecke und Charaktere (Haupt zuerst; None = Login-Bildschirm).
    LayoutCaptured {
        main: Rect,
        secondaries: Vec<Rect>,
        characters: Vec<Option<String>>,
    },
    /// Statusmeldung für das Protokoll.
    Message(String),
    /// Tray: Dashboard anzeigen.
    ShowDashboard,
    /// Tray: Programm beenden.
    QuitRequested,
}

/// Anzeigedaten eines Clients.
#[derive(Clone, Debug, Default)]
pub struct ClientInfo {
    pub hwnd: Hwnd,
    pub pid: u32,
    /// Charaktername oder „Login (PID)“.
    pub name: String,
    pub logged_in: bool,
    /// CPU-Anteil an der Gesamt-CPU in Prozent.
    pub cpu: f32,
    pub mem: MemInfo,
    pub priority: Option<Priority>,
    /// Erlaubte Kerne (leer = alle).
    pub cores: Vec<u32>,
    pub foreground: bool,
    pub main: bool,
    pub minimized: bool,
    pub throttled: bool,
    /// Prozess-Einstellungen konnten nicht gesetzt werden (fehlende Rechte).
    pub limited: bool,
}

/// Gesamtzustand für das Dashboard.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Zählt bei jeder Veröffentlichung hoch.
    pub seq: u64,
    pub clients: Vec<ClientInfo>,
    pub sys_cpu: f32,
    pub ram_total: u64,
    pub ram_avail: u64,
    pub cores: u32,
    pub warnings: Vec<String>,
    pub hotkey_errors: Vec<String>,
    pub previews_on: bool,
}
