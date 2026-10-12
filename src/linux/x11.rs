//! X11-Zugriff: Verbindung, Atome, EVE-Fenster finden, fokussieren, minimieren,
//! verschieben (inkl. rahmenlos), Fensterrechtecke und Monitore.
//!
//! Funktioniert unter einer X11-Sitzung und unter Wayland über XWayland – EVE läuft
//! unter Proton/Wine als X11-Client. Mit Fenstermanager (EWMH) werden die üblichen
//! `_NET_*`-Nachrichten verwendet, ohne Fenstermanager (z. B. Xvfb) direkte Aufrufe.

use evemb::config::Rect;
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::engine_api::MonitorInfo;

x11rb::atom_manager! {
    /// Alle benötigten Atome, einmal beim Verbinden abgefragt.
    pub Atoms: AtomsCookie {
        _NET_SUPPORTED,
        _NET_SUPPORTING_WM_CHECK,
        _NET_CLIENT_LIST,
        _NET_ACTIVE_WINDOW,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_STATE,
        _NET_WM_STATE_HIDDEN,
        _NET_WM_STATE_MAXIMIZED_VERT,
        _NET_WM_STATE_MAXIMIZED_HORZ,
        _NET_WM_STATE_FULLSCREEN,
        _NET_FRAME_EXTENTS,
        _NET_WORKAREA,
        _NET_MOVERESIZE_WINDOW,
        _NET_WM_WINDOW_OPACITY,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_UTILITY,
        _MOTIF_WM_HINTS,
        WM_CHANGE_STATE,
        WM_STATE,
        UTF8_STRING,
    }
}

/// Ist ein Titel der eines EVE-Clients („EVE“ am Login, sonst „EVE - Name“)?
pub fn is_eve_title(title: &str) -> bool {
    title == "EVE" || title.starts_with("EVE - ")
}

/// Verbindung zum X-Server samt häufig gebrauchter Werte.
pub struct X {
    pub conn: RustConnection,
    pub root: Window,
    pub atoms: Atoms,
    pub root_visual: Visualid,
    pub screen_w: u16,
    pub screen_h: u16,
    /// Läuft ein EWMH-Fenstermanager?
    pub wm_present: bool,
    /// Unterstützt der Fenstermanager `_NET_MOVERESIZE_WINDOW`?
    supports_moveresize: bool,
    /// Pflegt der Fenstermanager die EWMH-Fensterliste? (Weston/WSLg z. B. nicht.)
    full_ewmh: bool,
    /// Prozess-ID dieses Programms (zum Erkennen eigener Fenster).
    pub own_pid: u32,
}

impl X {
    /// Verbindet sich mit `$DISPLAY` und liest Bildschirm, Atome und Fenstermanager-Infos.
    pub fn connect() -> Result<X, String> {
        let (conn, screen_num) = x11rb::connect(None).map_err(|e| e.to_string())?;
        let atoms = Atoms::new(&conn)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        let screen = &conn.setup().roots[screen_num];
        let (root, root_visual, screen_w, screen_h) = (
            screen.root,
            screen.root_visual,
            screen.width_in_pixels,
            screen.height_in_pixels,
        );
        let mut x = X {
            conn,
            root,
            atoms,
            root_visual,
            screen_w,
            screen_h,
            wm_present: false,
            supports_moveresize: false,
            full_ewmh: false,
            own_pid: std::process::id(),
        };
        x.refresh_wm_info();
        Ok(x)
    }

    /// Prüft, ob ein EWMH-Fenstermanager läuft und was er unterstützt.
    pub fn refresh_wm_info(&mut self) {
        let check = self.prop32(self.root, self.atoms._NET_SUPPORTING_WM_CHECK, AtomEnum::WINDOW.into());
        self.wm_present = check.first().is_some_and(|&w| w != 0);
        let supported = self.prop32(self.root, self.atoms._NET_SUPPORTED, AtomEnum::ATOM.into());
        self.supports_moveresize = self.wm_present && supported.contains(&self.atoms._NET_MOVERESIZE_WINDOW);
        self.full_ewmh = self.wm_present && supported.contains(&self.atoms._NET_CLIENT_LIST);
    }

    /// Liest eine 32-Bit-Eigenschaft (leer, wenn nicht vorhanden).
    pub fn prop32(&self, w: Window, atom: Atom, type_: Atom) -> Vec<u32> {
        self.conn
            .get_property(false, w, atom, type_, 0, 4096)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    /// Liest eine 8-Bit-Eigenschaft als Bytes.
    fn prop_bytes(&self, w: Window, atom: Atom, type_: Atom) -> Option<Vec<u8>> {
        let reply = self
            .conn
            .get_property(false, w, atom, type_, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        (reply.format == 8 && !reply.value.is_empty()).then_some(reply.value)
    }

    /// Alle Top-Level-Fenster. Reihenfolge der Quellen:
    /// 1. `_NET_CLIENT_LIST` (KDE, GNOME, Xfce, Cinnamon …),
    /// 2. mit Fenstermanager ohne Liste (z. B. Weston/WSLg): Fenster mit `WM_STATE` im Baum,
    /// 3. ohne Fenstermanager: sichtbare, normale Kinder des Wurzelfensters.
    pub fn client_windows(&self) -> Vec<Window> {
        let list = self.prop32(self.root, self.atoms._NET_CLIENT_LIST, AtomEnum::WINDOW.into());
        if !list.is_empty() {
            return list;
        }
        let children = self.children(self.root);
        if self.wm_present {
            let mut found = Vec::new();
            for w in children {
                self.find_wm_state_clients(w, 3, &mut found);
            }
            return found;
        }
        children
            .into_iter()
            .filter(|&w| {
                self.conn
                    .get_window_attributes(w)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .is_some_and(|a| a.map_state == MapState::VIEWABLE && !a.override_redirect)
            })
            .collect()
    }

    /// Direkte Kindfenster (leer bei Fehler).
    fn children(&self, w: Window) -> Vec<Window> {
        self.conn
            .query_tree(w)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|t| t.children)
            .unwrap_or_default()
    }

    /// Oberstes Fenster unter der Wurzel, das `w` enthält (bei Fenstermanagern der Rahmen).
    fn top_level_of(&self, w: Window) -> Window {
        let mut cur = w;
        for _ in 0..8 {
            let Some(tree) = self.conn.query_tree(cur).ok().and_then(|c| c.reply().ok()) else {
                break;
            };
            if tree.parent == self.root || tree.parent == 0 {
                break;
            }
            cur = tree.parent;
        }
        cur
    }

    /// Sucht unterhalb eines Rahmenfensters die eigentlichen Client-Fenster: Laut ICCCM
    /// setzt der Fenstermanager auf jedes verwaltete Fenster die Eigenschaft `WM_STATE`.
    fn find_wm_state_clients(&self, w: Window, depth: u8, out: &mut Vec<Window>) {
        if !self.prop32(w, self.atoms.WM_STATE, self.atoms.WM_STATE).is_empty() {
            out.push(w);
            return;
        }
        if depth > 0 {
            for child in self.children(w) {
                self.find_wm_state_clients(child, depth - 1, out);
            }
        }
    }

    /// Fenstertitel (`_NET_WM_NAME` in UTF-8, sonst `WM_NAME`).
    pub fn title(&self, w: Window) -> String {
        if let Some(bytes) = self.prop_bytes(w, self.atoms._NET_WM_NAME, self.atoms.UTF8_STRING) {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
        self.prop_bytes(w, AtomEnum::WM_NAME.into(), AtomEnum::ANY.into())
            .map(|b| b.iter().map(|&c| c as char).collect())
            .unwrap_or_default()
    }

    /// Prozess-ID eines Fensters (`_NET_WM_PID`).
    pub fn pid(&self, w: Window) -> Option<u32> {
        self.prop32(w, self.atoms._NET_WM_PID, AtomEnum::CARDINAL.into())
            .first()
            .copied()
            .filter(|&p| p > 0)
    }

    /// `WM_CLASS` (Instanz und Klasse) in Kleinbuchstaben.
    pub fn wm_class(&self, w: Window) -> String {
        self.prop_bytes(w, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into())
            .map(|b| String::from_utf8_lossy(&b).replace('\0', " ").to_ascii_lowercase())
            .unwrap_or_default()
    }

    /// Gehört das Fenster zu EVE Multibox selbst?
    pub fn is_own_window(&self, w: Window) -> bool {
        w != 0 && self.pid(w) == Some(self.own_pid)
    }

    /// Erkennt einen EVE-Client: passender Titel und Wine/Proton-Prozess `exefile.exe`
    /// (über WM_CLASS oder den Prozessnamen).
    pub fn is_eve_window(&self, w: Window, title: &str) -> bool {
        if !is_eve_title(title) {
            return false;
        }
        let class = self.wm_class(w);
        if class.contains("exefile") || class.contains("steam_app_8500") {
            return true;
        }
        self.pid(w).is_some_and(|pid| {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            comm.to_ascii_lowercase().contains("exefile")
                || String::from_utf8_lossy(&cmdline)
                    .to_ascii_lowercase()
                    .contains("exefile.exe")
        })
    }

    /// Ist das Fenster minimiert bzw. nicht sichtbar?
    pub fn is_minimized(&self, w: Window) -> bool {
        let state = self.prop32(w, self.atoms._NET_WM_STATE, AtomEnum::ATOM.into());
        if state.contains(&self.atoms._NET_WM_STATE_HIDDEN) {
            return true;
        }
        let wm_state = self.prop32(w, self.atoms.WM_STATE, self.atoms.WM_STATE);
        if wm_state.first() == Some(&3) {
            return true; // IconicState
        }
        !self.wm_present
            && self
                .conn
                .get_window_attributes(w)
                .ok()
                .and_then(|c| c.reply().ok())
                .is_some_and(|a| a.map_state != MapState::VIEWABLE)
    }

    /// Aktuell aktives Fenster (0 = keines).
    pub fn active_window(&self) -> Window {
        if self.wm_present {
            return self
                .prop32(self.root, self.atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW.into())
                .first()
                .copied()
                .unwrap_or(0);
        }
        self.conn
            .get_input_focus()
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.focus)
            .filter(|&w| w > 1)
            .unwrap_or(0)
    }

    /// Schickt eine EWMH-Client-Nachricht an das Wurzelfenster.
    fn send_root_message(&self, w: Window, type_: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, w, type_, data);
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        let _ = self.conn.send_event(false, self.root, mask, ev);
    }

    /// Holt ein Fenster in den Vordergrund (stellt es bei Bedarf wieder her).
    /// Es werden dabei keinerlei Tastatur- oder Mauseingaben erzeugt.
    pub fn activate(&self, w: Window) {
        if self.wm_present {
            // Quelle 2 = „Pager“: Fenstermanager führen den Wechsel ohne Fokusdiebstahl-Schutz aus.
            self.send_root_message(w, self.atoms._NET_ACTIVE_WINDOW, [2, CURRENT_TIME, 0, 0, 0]);
            if !self.full_ewmh {
                // Fenstermanager mit nur teilweiser EWMH-Unterstützung (z. B. Weston/WSLg):
                // zusätzlich den Rahmen nach oben holen und den Fokus direkt setzen.
                let top = self.top_level_of(w);
                let _ = self
                    .conn
                    .configure_window(top, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
                let _ = self.conn.set_input_focus(InputFocus::PARENT, w, CURRENT_TIME);
            }
        } else {
            let _ = self.conn.map_window(w);
            let _ = self
                .conn
                .configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE));
            let _ = self.conn.set_input_focus(InputFocus::POINTER_ROOT, w, CURRENT_TIME);
        }
        let _ = self.conn.flush();
    }

    /// Minimiert ein Fenster.
    pub fn minimize(&self, w: Window) {
        if self.wm_present {
            self.send_root_message(w, self.atoms.WM_CHANGE_STATE, [3, 0, 0, 0, 0]);
        } else {
            let _ = self.conn.unmap_window(w);
        }
        let _ = self.conn.flush();
    }

    /// Breite der Fensterrahmen (links, rechts, oben, unten).
    fn frame_extents(&self, w: Window) -> (i32, i32, i32, i32) {
        let v = self.prop32(w, self.atoms._NET_FRAME_EXTENTS, AtomEnum::CARDINAL.into());
        if v.len() >= 4 {
            (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32)
        } else {
            (0, 0, 0, 0)
        }
    }

    /// Schaltet Titelleiste und Rahmen über `_MOTIF_WM_HINTS` ab bzw. wieder an.
    fn set_borderless(&self, w: Window, borderless: bool) {
        // flags = MWM_HINTS_DECORATIONS (2); decorations = 0 (keine) bzw. 1 (alle)
        let hints = [2u32, 0, if borderless { 0 } else { 1 }, 0, 0];
        let a = self.atoms._MOTIF_WM_HINTS;
        let _ = self.conn.change_property32(PropMode::REPLACE, w, a, a, &hints);
    }

    /// Platziert ein Fenster so, dass sein *sichtbarer* Bereich (inkl. Rahmen) genau `target` entspricht.
    /// Minimierte Fenster werden dafür wiederhergestellt, maximierte zuerst normalisiert.
    pub fn place_window(&self, w: Window, target: Rect, borderless: bool) {
        if self.is_minimized(w) {
            let _ = self.conn.map_window(w);
        }
        if self.wm_present {
            // Maximierung und Vollbild entfernen (Aktion 0 = entfernen).
            let a = &self.atoms;
            self.send_root_message(
                w,
                a._NET_WM_STATE,
                [0, a._NET_WM_STATE_MAXIMIZED_VERT, a._NET_WM_STATE_MAXIMIZED_HORZ, 2, 0],
            );
            self.send_root_message(w, a._NET_WM_STATE, [0, a._NET_WM_STATE_FULLSCREEN, 0, 2, 0]);
        }
        self.set_borderless(w, borderless);
        let (l, r, t, b) = if borderless {
            (0, 0, 0, 0)
        } else {
            self.frame_extents(w)
        };
        let (cx, cy) = (target.x + l, target.y + t);
        let cw = (target.w - l - r).max(1);
        let ch = (target.h - t - b).max(1);
        if self.supports_moveresize {
            // StaticGravity (10) + x/y/Breite/Höhe gesetzt (Bits 8–11) + Quelle „Pager“ (2 << 12).
            let flags = 10 | (0xF << 8) | (2 << 12);
            self.send_root_message(
                w,
                self.atoms._NET_MOVERESIZE_WINDOW,
                [flags, cx as u32, cy as u32, cw as u32, ch as u32],
            );
        } else {
            let aux = ConfigureWindowAux::new().x(cx).y(cy).width(cw as u32).height(ch as u32);
            let _ = self.conn.configure_window(w, &aux);
        }
        let _ = self.conn.flush();
    }

    /// Sichtbares Fensterrechteck inklusive Fenstermanager-Rahmen.
    pub fn visible_rect(&self, w: Window) -> Option<Rect> {
        let geo = self.conn.get_geometry(w).ok()?.reply().ok()?;
        let tr = self.conn.translate_coordinates(w, self.root, 0, 0).ok()?.reply().ok()?;
        let (l, r, t, b) = self.frame_extents(w);
        Some(Rect::new(
            tr.dst_x as i32 - l,
            tr.dst_y as i32 - t,
            geo.width as i32 + l + r,
            geo.height as i32 + t + b,
        ))
    }

    /// Größe des Fensterinhalts (ohne Rahmen).
    pub fn client_size(&self, w: Window) -> Option<(u16, u16)> {
        let geo = self.conn.get_geometry(w).ok()?.reply().ok()?;
        Some((geo.width, geo.height))
    }

    /// Setzt die Deckkraft eines Fensters (wird vom Compositor umgesetzt).
    pub fn set_opacity(&self, w: Window, alpha: u8) {
        let a = self.atoms._NET_WM_WINDOW_OPACITY;
        if alpha == 255 {
            let _ = self.conn.delete_property(w, a);
        } else {
            let value = alpha as u32 * 0x0101_0101;
            let _ = self
                .conn
                .change_property32(PropMode::REPLACE, w, a, AtomEnum::CARDINAL, &[value]);
        }
    }

    /// Alle Monitore; der Hauptmonitor steht zuerst, danach von links nach rechts.
    pub fn monitors(&self) -> Vec<MonitorInfo> {
        let workarea = {
            let v = self.prop32(self.root, self.atoms._NET_WORKAREA, AtomEnum::CARDINAL.into());
            (v.len() >= 4).then(|| Rect::new(v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32))
        };
        let mut list: Vec<MonitorInfo> = Vec::new();
        if let Some(reply) = self
            .conn
            .randr_get_monitors(self.root, true)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            for m in reply.monitors {
                let name = self
                    .conn
                    .get_atom_name(m.name)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                    .unwrap_or_default();
                let area = Rect::new(m.x as i32, m.y as i32, m.width as i32, m.height as i32);
                let work = workarea.and_then(|wa| intersect(area, wa)).unwrap_or(area);
                list.push(MonitorInfo {
                    name,
                    area,
                    work,
                    primary: m.primary,
                });
            }
        }
        if list.is_empty() {
            let area = Rect::new(0, 0, self.screen_w as i32, self.screen_h as i32);
            let work = workarea.and_then(|wa| intersect(area, wa)).unwrap_or(area);
            list.push(MonitorInfo {
                name: "screen".into(),
                area,
                work,
                primary: true,
            });
        }
        if !list.iter().any(|m| m.primary) {
            list[0].primary = true;
        }
        list.sort_by_key(|m| (!m.primary, m.area.x, m.area.y));
        list
    }
}

/// Schnittmenge zweier Rechtecke (None, wenn sie sich nicht überlappen).
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = (a.right().min(b.right()), a.bottom().min(b.bottom()));
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVE-Titel werden erkannt, Launcher und andere Fenster nicht.
    #[test]
    fn eve_titles() {
        assert!(is_eve_title("EVE"));
        assert!(is_eve_title("EVE - Legga Schnegge"));
        assert!(!is_eve_title("EVE Launcher"));
        assert!(!is_eve_title("Firefox"));
    }

    /// Schnittmenge von Monitor und Arbeitsbereich.
    #[test]
    fn intersection() {
        let mon = Rect::new(1920, 0, 1920, 1080);
        let work = Rect::new(0, 32, 3840, 1048);
        assert_eq!(intersect(mon, work), Some(Rect::new(1920, 32, 1920, 1048)));
        assert_eq!(intersect(Rect::new(0, 0, 10, 10), Rect::new(20, 20, 5, 5)), None);
    }
}
