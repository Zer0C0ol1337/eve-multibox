//! Linux-Backend: Fensterverwaltung über X11 bzw. XWayland (EVE läuft unter Proton/Wine
//! als X11-Client), Prozesswerkzeuge über /proc und libc, Tray über StatusNotifierItem.
//!
//! Aufbau analog zum Windows-Backend:
//! * `x11`       – Verbindung, Atome, Fenster finden/fokussieren/verschieben, Monitore
//! * `keys`      – globale Hotkeys (XGrabKey)
//! * `preview`   – Live-Vorschau per XComposite/XRender
//! * `procstat`  – CPU-Zeit, Speicher, Priorität (nice), Kern-Affinität, Speicher auslagern
//! * `tray`      – Symbol im Infobereich
//! * `signal`    – Weckruf der Engine aus anderen Threads (Tray, zweite Instanz)
//! * `engine`    – Engine-Thread mit gleicher API wie unter Windows
//! * `platform`  – Fassade für Start und Oberfläche

pub mod engine;
mod keys;
mod label;
pub mod platform;
mod preview;
mod procstat;
mod signal;
mod tray;
mod x11;
