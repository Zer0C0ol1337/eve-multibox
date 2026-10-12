//! Plattform-Fassade: alles, was Start, Oberfläche und Diagnose vom Betriebssystem
//! brauchen – mit gleicher API für Windows (`win_backend`) und Linux (`x11_backend`).

pub use crate::engine_api::{Hwnd, MonitorInfo};
use evemb::config::Config;
use evemb::i18n::Lang;

pub use imp::*;

/// Sprache gemäß Einstellung oder – bei „automatisch“ – gemäß Betriebssystem.
pub fn resolve_language(cfg: &Config) -> Lang {
    cfg.general.language.unwrap_or_else(system_language)
}

/// Windows-Umsetzung (Win32).
#[cfg(win_backend)]
mod imp {
    use std::path::{Path, PathBuf};

    use evemb::i18n::{Lang, t};
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW, PostMessageW,
    };

    use super::MonitorInfo;
    use crate::{tray, win};

    /// Übersetzungsschlüssel für die Autostart-Option.
    pub const AUTOSTART_LABEL: &str = "set.autostart";
    /// Übersetzungsschlüssel für den Autostart-Hinweis.
    pub const AUTOSTART_HINT: &str = "set.autostart_hint";
    /// Übersetzungsschlüssel, wenn keine chinesische Schrift gefunden wurde.
    pub const FONT_MISSING: &str = "set.font_missing";
    /// Übersetzungsschlüssel für die Erklärung von „Speicher freigeben“.
    pub const TRIM_HINT: &str = "perf.trim_hint";

    /// Startvorbereitung: echte Pixelkoordinaten (sonst stimmen Positionen bei 125 %/150 % nicht).
    pub fn init() {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    }

    /// Kommandozeilen-Befehle gibt es nur unter Linux.
    pub fn handle_cli() -> bool {
        false
    }

    /// Sprache der Windows-Oberfläche.
    pub fn system_language() -> Lang {
        Lang::from_windows_langid(win::ui_language())
    }

    /// Verhindert einen zweiten Start. Läuft schon eine Instanz, wird deren Dashboard
    /// nach vorne geholt. Das Mutex-Handle bleibt bis Programmende offen.
    pub fn ensure_single_instance() -> bool {
        let name = win::wide("Local\\EveMultiboxSingleInstance");
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if h.is_null() || GetLastError() != ERROR_ALREADY_EXISTS {
                return true;
            }
            let class = win::wide(tray::TRAY_CLASS);
            let existing = FindWindowW(class.as_ptr(), std::ptr::null());
            if !existing.is_null() {
                PostMessageW(existing, tray::WM_TRAY_SHOW, 0, 0);
            } else {
                let text = win::wide(t("main.already_running"));
                let title = win::wide("EVE Multibox");
                MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    title.as_ptr(),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
        }
        false
    }

    /// Zeigt ein Fehler-Meldungsfenster an (blockiert, bis es geschlossen wird).
    pub fn error_box(title: &str, text: &str) {
        let text = win::wide(text);
        let title = win::wide(title);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    /// Alle Monitore (Hauptmonitor zuerst).
    pub fn monitors() -> Vec<MonitorInfo> {
        win::monitors()
    }

    /// Öffnet einen Ordner im Explorer.
    pub fn open_folder(path: &Path) {
        win::open_in_explorer(path);
    }

    /// Mögliche Schriftdateien für Chinesisch.
    pub fn cjk_font_candidates() -> Vec<PathBuf> {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        ["msyh.ttc", "msyhl.ttc", "simsun.ttc", "simhei.ttf", "Deng.ttf"]
            .iter()
            .map(|f| Path::new(&windir).join("Fonts").join(f))
            .collect()
    }

    /// Fehlende System-Bibliotheken (nur Linux relevant – Windows bringt alles mit).
    pub fn missing_libraries() -> Option<String> {
        None
    }

    /// Hinweis zum Anzeige-System (nur Linux relevant).
    pub fn display_note() -> Option<(&'static str, bool)> {
        None
    }

    /// Befehl, um fehlende Prozessrechte nachzurüsten (nur Linux relevant).
    pub fn process_rights_fix() -> Option<String> {
        None
    }

    /// Hat das Programm erweiterte Prozessrechte? (Unter Windows nicht anzeigen.)
    pub fn show_process_rights() -> bool {
        false
    }

    /// Ist ein Tray-Symbol verfügbar? (Unter Windows immer.)
    pub fn tray_available() -> bool {
        true
    }

    /// Autostart über die Registry.
    pub mod autostart {
        pub use crate::win::autostart::{is_enabled, refresh_path, set};
    }
}

/// Linux-Umsetzung (X11/XWayland).
#[cfg(x11_backend)]
mod imp {
    pub use crate::linux::platform::*;
}
