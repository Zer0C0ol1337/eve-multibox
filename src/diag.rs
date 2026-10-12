//! Diagnose: Startprotokoll und Absturzbericht.
//!
//! Der Release-Build hat unter Windows kein Konsolenfenster – ohne dieses Modul würde ein
//! Fehler beim Start spurlos verschwinden. Alles landet daher in `eve-multibox.log`
//! neben der Konfiguration, schwere Fehler zusätzlich in einem Meldungsfenster.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use evemb::i18n::{t, tf};

use crate::platform;

/// Pfad der Protokolldatei (neben der Konfiguration).
pub fn log_path() -> PathBuf {
    evemb::config::config_path().with_file_name("eve-multibox.log")
}

/// Leert das Protokoll beim Programmstart, damit es nicht endlos wächst.
pub fn start_log() {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, "");
    log(&format!(
        "EVE Multibox {} startet ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    ));
}

/// Hängt eine Zeile mit Zeitstempel (Sekunden seit 1970) ans Protokoll an
/// und gibt sie im Debug-Build zusätzlich auf der Konsole aus.
pub fn log(msg: &str) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{secs}] {msg}\n");
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Zeigt eine Fehlermeldung in einem Meldungsfenster an.
pub fn error_box(text: &str) {
    platform::error_box(t("crash.title"), text);
}

/// Fängt Panics aus allen Threads ab: ins Protokoll schreiben und dem Nutzer anzeigen.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        log(&format!("ABSTURZ im Thread „{thread}“: {info}\n{backtrace}"));
        error_box(&tf("crash.text", &[&thread, &info, &log_path().display()]));
    }));
}
