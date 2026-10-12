//! Symbol im Infobereich unter Linux über StatusNotifierItem (D-Bus).
//!
//! Funktioniert mit KDE Plasma, Xfce, Cinnamon, MATE, LXQt und GNOME mit der
//! Erweiterung „AppIndicator“. Klicks und Menüeinträge werden als `Signal` an die
//! Engine weitergereicht; das Menü wird bei jedem Öffnen in der aktiven Sprache gebaut.

use std::sync::atomic::{AtomicBool, Ordering};

use evemb::i18n::{t, tf};
use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};

use super::signal::{self, Signal};

/// Konnte das Tray-Symbol angelegt werden?
static AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Ist ein Tray-Symbol verfügbar?
pub fn available() -> bool {
    AVAILABLE.load(Ordering::SeqCst)
}

/// Inhalt des Tray-Symbols.
struct EveTray {
    /// Anzahl erkannter Clients (für den Tooltip).
    clients: usize,
}

impl ksni::Tray for EveTray {
    /// Eindeutige Kennung.
    fn id(&self) -> String {
        "eve-multibox".into()
    }

    /// Titel des Symbols.
    fn title(&self) -> String {
        "EVE Multibox".into()
    }

    /// Linksklick öffnet das Dashboard.
    fn activate(&mut self, _x: i32, _y: i32) {
        signal::post(Signal::Show);
    }

    /// Symbol als ARGB32-Bilder in zwei Größen (aus dem gemeinsamen App-Icon).
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22u32, 48]
            .iter()
            .map(|&size| {
                let rgba = evemb::icon::icon_rgba(size);
                let data = rgba.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
                ksni::Icon {
                    width: size as i32,
                    height: size as i32,
                    data,
                }
            })
            .collect()
    }

    /// Tooltip mit der Anzahl der Clients.
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "EVE Multibox".into(),
            description: tf("tray.tooltip", &[&self.clients]),
            ..Default::default()
        }
    }

    /// Kontextmenü: Dashboard, Vorschau ein/aus, Layout anwenden, Beenden.
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let item = |label: &str, sig: Signal| -> MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |_| signal::post(sig)),
                ..Default::default()
            }
            .into()
        };
        vec![
            item(t("tray.show"), Signal::Show),
            item(t("hk.toggle"), Signal::TogglePreviews),
            item(t("top.apply_layout"), Signal::ApplyLayout),
            MenuItem::Separator,
            item(t("tray.quit"), Signal::Quit),
        ]
    }
}

/// Das laufende Tray-Symbol; entfernt sich beim Drop.
pub struct Tray {
    handle: ksni::blocking::Handle<EveTray>,
    clients: usize,
}

impl Tray {
    /// Meldet das Symbol beim Infobereich an (None, wenn es keinen gibt).
    pub fn create() -> Option<Tray> {
        match (EveTray { clients: 0 }).spawn() {
            Ok(handle) => {
                AVAILABLE.store(true, Ordering::SeqCst);
                Some(Tray { handle, clients: 0 })
            }
            Err(e) => {
                crate::diag::log(&format!("Tray-Symbol nicht verfügbar: {e}"));
                AVAILABLE.store(false, Ordering::SeqCst);
                None
            }
        }
    }

    /// Aktualisiert den Tooltip (nur bei Änderung).
    pub fn set_clients(&mut self, clients: usize) {
        if self.clients != clients {
            self.clients = clients;
            self.handle.update(|t| t.clients = clients);
        }
    }
}

impl Drop for Tray {
    /// Symbol abmelden.
    fn drop(&mut self) {
        self.handle.shutdown().wait();
        AVAILABLE.store(false, Ordering::SeqCst);
    }
}
