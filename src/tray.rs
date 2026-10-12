//! Symbol im Infobereich (Tray) mit Menü: Dashboard öffnen, Vorschau ein/aus,
//! Layout anwenden, Beenden.
//!
//! Läuft im Engine-Thread. Das Tray-Symbol hängt an einem eigenen, unsichtbaren
//! Fenster (Menüs brauchen ein echtes Top-Level-Fenster). Befehle gehen als
//! Nachricht `notify_msg` mit dem Befehl in `wParam` an das Engine-Fenster.
//! Eine zweite Programminstanz findet dieses Fenster über den Klassennamen und
//! schickt `WM_TRAY_SHOW`, damit sich das vorhandene Dashboard öffnet.

use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use evemb::i18n::t;
use evemb::icon;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win::{self, Hwnd, wide};

/// Klassenname des unsichtbaren Tray-Fensters (auch für die Zweitinstanz-Erkennung).
pub const TRAY_CLASS: &str = "EveMultiboxTray";
/// Nachricht einer zweiten Instanz: „Dashboard anzeigen“.
pub const WM_TRAY_SHOW: u32 = WM_APP + 10;
/// Rückmeldungen des Tray-Symbols (Klicks).
const WM_TRAY_CALLBACK: u32 = WM_APP + 11;
/// ID unseres Tray-Symbols.
const ICON_ID: u32 = 1;

/// Befehle aus Tray-Menü und Tray-Klick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show = 1,
    TogglePreviews = 2,
    ApplyLayout = 3,
    Quit = 4,
}

impl TrayCommand {
    /// Wandelt den Wert aus `wParam` zurück in einen Befehl.
    pub fn from_raw(v: usize) -> Option<Self> {
        match v {
            1 => Some(Self::Show),
            2 => Some(Self::TogglePreviews),
            3 => Some(Self::ApplyLayout),
            4 => Some(Self::Quit),
            _ => None,
        }
    }
}

/// Ziel für Tray-Befehle (Engine-Fenster) und dessen Nachricht.
static TARGET: AtomicIsize = AtomicIsize::new(0);
static TARGET_MSG: AtomicU32 = AtomicU32::new(0);
/// Von Windows vergebene Nachricht „Taskleiste neu erstellt“ (Explorer-Neustart).
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// Aktuelles Icon-Handle, damit es nach einem Explorer-Neustart wieder eingetragen werden kann.
    static ICON: Cell<isize> = const { Cell::new(0) };
}

/// Schickt einen Befehl an die Engine.
fn post_command(cmd: TrayCommand) {
    let target = TARGET.load(Ordering::SeqCst);
    if target != 0 {
        unsafe { PostMessageW(win::hwnd(target), TARGET_MSG.load(Ordering::SeqCst), cmd as usize, 0) };
    }
}

/// Grunddaten für Shell_NotifyIconW.
fn notify_data(h: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = h;
    nid.uID = ICON_ID;
    nid
}

/// Kopiert einen Text in das Tooltip-Feld (max. 127 Zeichen).
fn set_tip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let w: Vec<u16> = text.encode_utf16().take(nid.szTip.len() - 1).collect();
    nid.szTip[..w.len()].copy_from_slice(&w);
    nid.szTip[w.len()] = 0;
}

/// Trägt das Tray-Symbol ein.
fn add_icon(h: HWND, icon: isize, tip: &str) {
    let mut nid = notify_data(h);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY_CALLBACK;
    nid.hIcon = icon as HICON;
    set_tip(&mut nid, tip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

/// Zeigt das Kontextmenü an der Mausposition und führt die Auswahl aus.
fn show_menu(h: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        let items = [
            (TrayCommand::Show, t("tray.show")),
            (TrayCommand::TogglePreviews, t("hk.toggle")),
            (TrayCommand::ApplyLayout, t("top.apply_layout")),
        ];
        for (cmd, label) in items {
            let text = wide(label);
            AppendMenuW(menu, MF_STRING, cmd as usize, text.as_ptr());
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        let quit = wide(t("tray.quit"));
        AppendMenuW(menu, MF_STRING, TrayCommand::Quit as usize, quit.as_ptr());
        SetMenuDefaultItem(menu, TrayCommand::Show as u32, 0);

        let mut p = POINT { x: 0, y: 0 };
        GetCursorPos(&mut p);
        // Ohne Vordergrund schließt sich das Menü nicht, wenn man daneben klickt.
        SetForegroundWindow(h);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            p.x,
            p.y,
            0,
            h,
            std::ptr::null(),
        );
        PostMessageW(h, WM_NULL, 0, 0);
        DestroyMenu(menu);
        if let Some(cmd) = TrayCommand::from_raw(chosen as usize) {
            post_command(cmd);
        }
    }
}

/// Fensterprozedur des unsichtbaren Tray-Fensters.
unsafe extern "system" fn tray_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            match (lp & 0xFFFF) as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => post_command(TrayCommand::Show),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(h),
                _ => {}
            }
            0
        }
        WM_TRAY_SHOW => {
            post_command(TrayCommand::Show);
            0
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            // Explorer wurde neu gestartet – Symbol erneut eintragen.
            add_icon(h, ICON.with(|i| i.get()), "EVE Multibox");
            0
        }
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

/// Das Tray-Symbol samt unsichtbarem Fenster; entfernt sich beim Drop selbst.
pub struct Tray {
    hwnd: Hwnd,
    icon: isize,
    tip: String,
}

impl Tray {
    /// Legt Fenster und Symbol an. Befehle gehen als `notify_msg` an `notify_target`.
    pub fn create(notify_target: Hwnd, notify_msg: u32) -> Option<Tray> {
        TARGET.store(notify_target, Ordering::SeqCst);
        TARGET_MSG.store(notify_msg, Ordering::SeqCst);
        let class = wide(TRAY_CLASS);
        let title = wide("EVE Multibox");
        let taskbar = wide("TaskbarCreated");
        unsafe {
            TASKBAR_CREATED.store(RegisterWindowMessageW(taskbar.as_ptr()), Ordering::SeqCst);
            let hinst = GetModuleHandleW(std::ptr::null());
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(tray_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            RegisterClassW(&wc);
            let h = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            );
            if h.is_null() {
                return None;
            }
            // Icon in der passenden Größe für den Infobereich (abhängig von der DPI-Skalierung).
            let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 64) as u32;
            let dib = icon::icon_dib(size);
            let hicon = CreateIconFromResourceEx(
                dib.as_ptr(),
                dib.len() as u32,
                1,
                0x0003_0000,
                size as i32,
                size as i32,
                0,
            );
            let icon = hicon as isize;
            ICON.with(|i| i.set(icon));
            add_icon(h, icon, "EVE Multibox");
            Some(Tray {
                hwnd: win::handle(h),
                icon,
                tip: "EVE Multibox".into(),
            })
        }
    }

    /// Aktualisiert den Tooltip (nur bei Änderung).
    pub fn set_tooltip(&mut self, tip: &str) {
        if self.tip == tip {
            return;
        }
        self.tip = tip.to_string();
        let mut nid = notify_data(win::hwnd(self.hwnd));
        nid.uFlags = NIF_TIP;
        set_tip(&mut nid, tip);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }
}

impl Drop for Tray {
    /// Symbol entfernen, Fenster und Icon freigeben.
    fn drop(&mut self) {
        let nid = notify_data(win::hwnd(self.hwnd));
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &nid);
            DestroyWindow(win::hwnd(self.hwnd));
            if self.icon != 0 {
                DestroyIcon(self.icon as HICON);
            }
        }
    }
}
