//! Linux-Teil der Plattform-Fassade: Start, Einzelinstanz, Meldungen, Monitore,
//! Ordner öffnen, Schriften, Anzeige-System, Prozessrechte und Autostart.

use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use evemb::i18n::{Lang, t, tf};

use super::procstat;
use super::signal::{self, Signal};
use super::x11::X;
use crate::engine_api::MonitorInfo;

/// Übersetzungsschlüssel für die Autostart-Option.
pub const AUTOSTART_LABEL: &str = "set.autostart_linux";
/// Übersetzungsschlüssel für den Autostart-Hinweis.
pub const AUTOSTART_HINT: &str = "set.autostart_hint_linux";
/// Übersetzungsschlüssel, wenn keine chinesische Schrift gefunden wurde.
pub const FONT_MISSING: &str = "set.font_missing_linux";
/// Übersetzungsschlüssel für die Erklärung von „Speicher freigeben“.
pub const TRIM_HINT: &str = "perf.trim_hint_linux";

/// Startvorbereitung: Unter Wayland mit XWayland läuft auch das Dashboard über XWayland.
/// So funktionieren Ausblenden ins Tray und „Hotkeys nur bei aktivem EVE/Dashboard“.
/// Die Wayland-Sitzung wird für die Anzeige im Einstellungen-Tab vermerkt.
pub fn init() {
    signal::install_terminate_handlers();
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("WAYLAND_DISPLAY").is_some() {
        // SAFETY: wird als Allererstes in `main` aufgerufen, bevor weitere Threads existieren.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::set_var("EVE_MULTIBOX_WAYLAND", "1");
        }
    }
}

/// Kommandozeilen-Befehle (`--install`, `--uninstall`, `--version`, `--help`).
/// Liefert `true`, wenn ein Befehl ausgeführt wurde und das Programm enden soll.
pub fn handle_cli() -> bool {
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        "--install" => {
            match install::install() {
                Ok(path) => {
                    println!("{}", tf("cli.installed", &[&path.display()]));
                    if let Some(cmd) = process_rights_fix() {
                        println!("{}\n  {cmd}", t("cli.setcap"));
                    }
                }
                Err(e) => eprintln!("{}", tf("cli.failed", &[&e])),
            }
            true
        }
        "--uninstall" => {
            match install::uninstall() {
                Ok(()) => {
                    let dir = evemb::config::config_path();
                    let dir = dir.parent().unwrap_or(&dir);
                    println!("{}", tf("cli.uninstalled", &[&dir.display()]));
                }
                Err(e) => eprintln!("{}", tf("cli.failed", &[&e])),
            }
            true
        }
        "--version" | "-V" => {
            println!("EVE Multibox {} · © 2026 LeggaSchnegge", env!("CARGO_PKG_VERSION"));
            true
        }
        "--help" | "-h" => {
            println!("{}", t("cli.usage"));
            true
        }
        _ => false,
    }
}

/// Systemsprache aus den üblichen Locale-Variablen.
pub fn system_language() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .map_or(Lang::En, |v| Lang::from_locale(&v))
}

/// Name des abstrakten Sockets für die Einzelinstanz (pro Benutzer).
fn instance_socket_name() -> String {
    format!("eve-multibox-{}", unsafe { libc::getuid() })
}

/// Verhindert einen zweiten Start. Läuft schon eine Instanz, wird deren Dashboard
/// nach vorne geholt. Die erste Instanz lauscht dafür auf einem abstrakten Unix-Socket.
pub fn ensure_single_instance() -> bool {
    let name = instance_socket_name();
    let Ok(addr) = SocketAddr::from_abstract_name(name.as_bytes()) else {
        return true;
    };
    match UnixListener::bind_addr(&addr) {
        Ok(listener) => {
            let _ = std::thread::Builder::new().name("instance".into()).spawn(move || {
                for mut stream in listener.incoming().flatten() {
                    let mut buf = [0u8; 8];
                    if stream.read(&mut buf).is_ok_and(|n| &buf[..n] == b"show") {
                        signal::post(Signal::Show);
                    }
                }
            });
            true
        }
        Err(_) => {
            if let Ok(mut stream) = UnixStream::connect_addr(&addr) {
                let _ = stream.write_all(b"show");
            }
            false
        }
    }
}

/// Zeigt eine Fehlermeldung an (zenity, kdialog oder xmessage – sonst nur Konsole).
pub fn error_box(title: &str, text: &str) {
    eprintln!("{title}: {text}");
    let tries: [(&str, Vec<&str>); 3] = [
        ("zenity", vec!["--error", "--title", title, "--text", text]),
        ("kdialog", vec!["--title", title, "--error", text]),
        ("xmessage", vec!["-center", text]),
    ];
    for (cmd, args) in tries {
        let ok = Command::new(cmd)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok();
        if ok {
            return;
        }
    }
}

/// Bibliotheken, die das Dashboard-Fenster (winit/OpenGL) beim Start nachlädt:
/// Dateiname plus Paketname für apt (Debian/Ubuntu/Mint/WSL), dnf (Fedora) und pacman (Arch).
/// Einträge mit mehreren Dateien gelten als vorhanden, sobald eine davon ladbar ist.
const REQUIRED_LIBS: [(&[&str], &str, &str, &str); 7] = [
    (&["libX11.so.6"], "libx11-6", "libX11", "libx11"),
    (&["libX11-xcb.so.1"], "libx11-xcb1", "libX11-xcb", "libx11"),
    (&["libXcursor.so.1"], "libxcursor1", "libXcursor", "libxcursor"),
    (&["libXi.so.6"], "libxi6", "libXi", "libxi"),
    (&["libxkbcommon.so.0"], "libxkbcommon0", "libxkbcommon", "libxkbcommon"),
    (
        &["libxkbcommon-x11.so.0"],
        "libxkbcommon-x11-0",
        "libxkbcommon-x11",
        "libxkbcommon-x11",
    ),
    (
        &["libEGL.so.1", "libGL.so.1"],
        "libegl1 libgl1-mesa-dri",
        "mesa-libEGL mesa-dri-drivers",
        "mesa",
    ),
];

/// Lässt sich eine Bibliothek laden? (Wird sofort wieder freigegeben.)
fn library_loadable(name: &str) -> bool {
    let Ok(c) = std::ffi::CString::new(name) else {
        return false;
    };
    // SAFETY: dlopen mit gültigem C-String; das Handle wird direkt wieder geschlossen.
    unsafe {
        let h = libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL);
        if h.is_null() {
            return false;
        }
        libc::dlclose(h);
    }
    true
}

/// Paketmanager der Distribution aus /etc/os-release (apt, dnf oder pacman).
fn package_manager() -> &'static str {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let ids: String = release
        .lines()
        .filter(|l| l.starts_with("ID=") || l.starts_with("ID_LIKE="))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if ids.contains("fedora") || ids.contains("rhel") {
        "dnf"
    } else if ids.contains("arch") {
        "pacman"
    } else {
        "apt"
    }
}

/// Prüft vor dem Öffnen des Fensters, ob alle Grafik-Bibliotheken da sind. Fehlt etwas
/// (typisch bei WSL oder Minimal-Installationen), kommt eine verständliche Meldung mit
/// passendem Installationsbefehl statt eines Absturzes.
pub fn missing_libraries() -> Option<String> {
    let pm = package_manager();
    let mut files = Vec::new();
    let mut packages: Vec<&str> = Vec::new();
    for (names, apt, dnf, pacman) in REQUIRED_LIBS {
        if names.iter().any(|n| library_loadable(n)) {
            continue;
        }
        files.push(names.join(" / "));
        let pkg = match pm {
            "dnf" => dnf,
            "pacman" => pacman,
            _ => apt,
        };
        for p in pkg.split_whitespace() {
            if !packages.contains(&p) {
                packages.push(p);
            }
        }
    }
    if files.is_empty() {
        return None;
    }
    let cmd = match pm {
        "dnf" => format!("sudo dnf install {}", packages.join(" ")),
        "pacman" => format!("sudo pacman -S {}", packages.join(" ")),
        _ => format!("sudo apt install {}", packages.join(" ")),
    };
    Some(tf("main.libs_missing", &[&files.join("\n  "), &cmd]))
}

/// Alle Monitore (Hauptmonitor zuerst); ohne X11 leer.
pub fn monitors() -> Vec<MonitorInfo> {
    X::connect().map(|x| x.monitors()).unwrap_or_default()
}

/// Öffnet einen Ordner im Dateimanager.
pub fn open_folder(path: &Path) {
    let _ = Command::new("xdg-open")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Mögliche Schriftdateien für Chinesisch (übliche Pfade der großen Distributionen).
pub fn cjk_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/google-droid/DroidSansFallbackFull.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

/// Hinweis zum Anzeige-System: (Übersetzungsschlüssel, ist Warnung).
pub fn display_note() -> Option<(&'static str, bool)> {
    Some(if std::env::var_os("DISPLAY").is_none() {
        ("set.display_none", true)
    } else if std::env::var_os("EVE_MULTIBOX_WAYLAND").is_some() {
        ("set.display_xwayland", false)
    } else {
        ("set.display_x11", false)
    })
}

/// Befehl, um CAP_SYS_NICE nachzurüsten (None = Recht schon vorhanden).
pub fn process_rights_fix() -> Option<String> {
    if procstat::has_cap_sys_nice() {
        return None;
    }
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "eve-multibox".into());
    Some(format!("sudo setcap cap_sys_nice+ep \"{exe}\""))
}

/// Unter Linux den Rechte-Hinweis im Leistung-Tab anzeigen.
pub fn show_process_rights() -> bool {
    true
}

/// Ist ein Tray-Symbol verfügbar?
pub fn tray_available() -> bool {
    super::tray::available()
}

/// Autostart über `~/.config/autostart/eve-multibox.desktop` (XDG-Autostart).
pub mod autostart {
    use std::path::PathBuf;

    /// Pfad der Autostart-Datei.
    fn desktop_file() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("autostart").join("eve-multibox.desktop"))
    }

    /// Inhalt der Autostart-Datei für das aktuelle Programm.
    fn desktop_entry() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!(
            "[Desktop Entry]\nType=Application\nName=EVE Multibox\nComment=EVE Online multiboxing helper\n\
             Exec=\"{}\" --minimized\nIcon=eve-multibox\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exe.display()
        ))
    }

    /// Ist der Autostart eingerichtet?
    pub fn is_enabled() -> bool {
        desktop_file().is_some_and(|p| p.exists())
    }

    /// Richtet den Autostart ein oder entfernt ihn.
    pub fn set(enabled: bool) -> Result<(), String> {
        let path = desktop_file().ok_or("HOME nicht gesetzt")?;
        if enabled {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let entry = desktop_entry().ok_or("Programmpfad unbekannt")?;
            std::fs::write(&path, entry).map_err(|e| e.to_string())
        } else {
            match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }

    /// Wurde das Programm verschoben, den gespeicherten Pfad stillschweigend aktualisieren.
    pub fn refresh_path() {
        if let (Some(path), Some(entry)) = (desktop_file(), desktop_entry())
            && path.exists()
            && std::fs::read_to_string(&path).ok().as_deref() != Some(entry.as_str())
        {
            let _ = std::fs::write(&path, entry);
        }
    }
}

/// Startmenü-Eintrag und Icon im Benutzerverzeichnis (`~/.local/share`), ohne sudo.
mod install {
    use std::path::PathBuf;

    /// `$XDG_DATA_HOME` bzw. `~/.local/share`.
    fn data_home() -> Result<PathBuf, String> {
        std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok_or_else(|| "HOME nicht gesetzt".to_string())
    }

    /// Pfad des Startmenü-Eintrags.
    fn desktop_path() -> Result<PathBuf, String> {
        Ok(data_home()?.join("applications").join("eve-multibox.desktop"))
    }

    /// Pfad eines Icons der angegebenen Größe.
    fn icon_path(size: u32) -> Result<PathBuf, String> {
        Ok(data_home()?.join(format!("icons/hicolor/{size}x{size}/apps/eve-multibox.png")))
    }

    /// Schreibt eine Datei und legt fehlende Ordner an.
    fn write(path: &PathBuf, data: &[u8]) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Legt Icons (48 und 256 px) und den Startmenü-Eintrag für das aktuelle Programm an.
    pub fn install() -> Result<PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        for size in [48, 256] {
            write(&icon_path(size)?, &evemb::icon::icon_png(size))?;
        }
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=EVE Multibox\nGenericName=Multiboxing helper\n\
             Comment=EVE Online multiboxing helper (no input broadcasting)\nExec=\"{}\"\nIcon=eve-multibox\n\
             Terminal=false\nCategories=Game;Utility;\nStartupWMClass=eve-multibox\n",
            exe.display()
        );
        let path = desktop_path()?;
        write(&path, entry.as_bytes())?;
        Ok(path)
    }

    /// Entfernt Startmenü-Eintrag, Icons und Autostart (die Konfiguration bleibt).
    pub fn uninstall() -> Result<(), String> {
        let mut paths = vec![desktop_path()?, icon_path(48)?, icon_path(256)?];
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".config/autostart/eve-multibox.desktop"));
        }
        for p in paths {
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", p.display())),
            }
        }
        super::autostart::set(false)
    }
}
