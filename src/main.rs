//! EVE Multibox – kostenloses, EULA-konformes Multibox-Werkzeug für EVE Online
//! für Windows und Linux. © 2026 LeggaSchnegge
//!
//! Funktionen: Fenster-Layouts pro Profil, Live-Vorschau mit Klick/Hotkey-Umschaltung,
//! CPU-Werkzeuge (Affinität, Priorität, Drosselung), Speicher-Werkzeuge, Dashboard,
//! Tray-Symbol, Autostart und sechs Sprachen.
//! Bewusst NICHT enthalten: Input-Broadcasting oder sonstiges Senden von Eingaben an Clients.
//!
//! Aufbau: Oberfläche (`ui`), Konfiguration und Logik (Bibliothek `evemb`) sind gemeinsam;
//! darunter liegt je ein Backend – Win32 (`win_backend`) oder X11/XWayland (`x11_backend`).

// Im Windows-Release-Build kein Konsolenfenster öffnen.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod diag;
mod engine_api;
mod platform;
mod ui;

// Windows-Backend.
#[cfg(win_backend)]
mod engine;
#[cfg(win_backend)]
mod preview;
#[cfg(win_backend)]
mod procstat;
#[cfg(win_backend)]
mod tray;
#[cfg(win_backend)]
mod win;

// Linux-Backend; `crate::engine` zeigt dort auf die X11-Engine.
#[cfg(x11_backend)]
mod linux;
#[cfg(x11_backend)]
use linux::engine;

use std::sync::Arc;

use eframe::egui;
use evemb::i18n::{self, tf};

/// Fensteroptionen für das Dashboard (Grafik-Backend, Icon, Sichtbarkeit).
fn native_options(renderer: eframe::Renderer, visible: bool) -> eframe::NativeOptions {
    let icon = egui::IconData {
        rgba: evemb::icon::icon_rgba(64),
        width: 64,
        height: 64,
    };
    eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_title("EVE Multibox")
            .with_app_id("eve-multibox")
            .with_icon(Arc::new(icon))
            .with_visible(visible)
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([860.0, 540.0])
            // Eigene Titelleiste im EVE-Stil statt des Systemrahmens (siehe ui/theme.rs).
            .with_decorations(false),
        ..Default::default()
    }
}

/// Einstiegspunkt: Diagnose, Konfiguration, Sprache, Einzelinstanz, Dashboard.
/// Scheitert OpenGL (z. B. alter/fehlender Treiber), wird automatisch wgpu versucht.
fn main() {
    platform::init();
    let cfg = evemb::config::load();
    i18n::set_lang(platform::resolve_language(&cfg));
    if platform::handle_cli() {
        return;
    }
    // Erst prüfen, dann protokollieren: Eine zweite Instanz darf das Protokoll der
    // laufenden nicht leeren – sie holt nur deren Dashboard nach vorne und endet.
    if !platform::ensure_single_instance() {
        return;
    }
    diag::start_log();
    diag::install_panic_hook();
    diag::log(&format!("Konfiguration: {}", evemb::config::config_path().display()));
    platform::autostart::refresh_path();
    // Linux: fehlende Grafik-Bibliotheken verständlich melden statt abzustürzen.
    if let Some(msg) = platform::missing_libraries() {
        diag::log("Start abgebrochen: System-Bibliotheken fehlen");
        diag::error_box(&msg);
        return;
    }

    // `--minimized` kommt vom Autostart; alternativ per Einstellung.
    let start_hidden = std::env::args().any(|a| a == "--minimized") || cfg.general.start_minimized;

    let mut errors = Vec::new();
    for (name, renderer) in [("OpenGL", eframe::Renderer::Glow), ("wgpu", eframe::Renderer::Wgpu)] {
        diag::log(&format!("Starte Oberfläche mit {name}"));
        let cfg = cfg.clone();
        let result = eframe::run_native(
            "EVE Multibox",
            native_options(renderer, !start_hidden),
            Box::new(move |cc| {
                diag::log("Fenster erstellt, starte Engine");
                Ok(Box::new(ui::App::new(cc, cfg, start_hidden)))
            }),
        );
        match result {
            Ok(()) => {
                diag::log("Normal beendet");
                return;
            }
            Err(e) => {
                diag::log(&format!("{name} fehlgeschlagen: {e}"));
                errors.push(format!("{name}: {e}"));
            }
        }
    }
    diag::error_box(&tf(
        "main.window_failed",
        &[&errors.join("\n"), &diag::log_path().display()],
    ));
}
