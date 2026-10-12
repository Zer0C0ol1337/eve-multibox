//! Dünne, sichere Hüllen um die benötigten Win32-Fensterfunktionen:
//! EVE-Fenster finden, fokussieren, verschieben, Rahmen entfernen, Monitore auflisten.
//!
//! Fenster-Handles werden außerhalb dieses Moduls als `Hwnd` (= `isize`) geführt,
//! weil rohe Zeiger nicht zwischen Threads verschickt werden dürfen.

use std::ffi::c_void;

use evemb::config::Rect;
use evemb::layout::character_from_title;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows_sys::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Thread-sichere Darstellung eines Fenster-Handles.
pub use crate::engine_api::Hwnd;

/// Fensterklasse des EVE-Clients.
const EVE_CLASS: &str = "trinityWindow";
/// Programmdatei des EVE-Clients (Rückfallprüfung).
const EVE_EXE: &str = "exefile.exe";

/// Wandelt das Send-fähige Handle zurück in ein Win32-HWND.
pub fn hwnd(h: Hwnd) -> HWND {
    h as HWND
}

/// Wandelt ein Win32-HWND in die Send-fähige Darstellung um.
pub fn handle(h: HWND) -> Hwnd {
    h as Hwnd
}

/// Wandelt einen Rust-String in einen nullterminierten UTF-16-Puffer um.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Ein gefundenes EVE-Client-Fenster.
#[derive(Clone, Debug)]
pub struct EveWindow {
    pub hwnd: Hwnd,
    pub pid: u32,
    pub character: Option<String>,
}

/// Liest den Fenstertitel.
pub fn window_text(h: Hwnd) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd(h), buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Liest den Klassennamen eines Fensters.
fn class_name(h: Hwnd) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd(h), buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Callback für EnumWindows: sammelt alle Top-Level-Fenster.
unsafe extern "system" fn collect_windows(h: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: lparam zeigt auf den Vec aus `find_eve_windows`, der während EnumWindows lebt.
    let list = unsafe { &mut *(lparam as *mut Vec<Hwnd>) };
    list.push(handle(h));
    1
}

/// Prozess-ID eines Fensters.
pub fn window_pid(h: Hwnd) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd(h), &mut pid) };
    pid
}

/// Gehört das Fenster zu EVE Multibox selbst (Dashboard oder Vorschau)?
pub fn is_own_window(h: Hwnd) -> bool {
    h != 0 && window_pid(h) == unsafe { GetCurrentProcessId() }
}

/// Dateiname der Programmdatei eines Prozesses (klein geschrieben), z. B. `exefile.exe`.
fn process_exe_name(pid: u32) -> Option<String> {
    unsafe {
        let ph = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if ph.is_null() {
            return None;
        }
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(ph, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(ph);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(str::to_ascii_lowercase)
    }
}

/// Erkennt einen EVE-Client: Fensterklasse `trinityWindow` oder – falls CCP die
/// Klasse ändert – ein „EVE“-Titel von `exefile.exe`.
fn is_eve_window(h: Hwnd, title: &str) -> bool {
    if class_name(h) == EVE_CLASS {
        return true;
    }
    (title == "EVE" || title.starts_with("EVE - ")) && process_exe_name(window_pid(h)).is_some_and(|n| n == EVE_EXE)
}

/// Findet alle sichtbaren EVE-Client-Fenster.
///
/// Fenster des eigenen Prozesses werden zuerst aussortiert: `GetWindowTextW` auf ein
/// Fenster eines anderen Threads im selben Prozess schickt diesem synchron eine
/// Nachricht – wartet der gerade auf die Engine, hängen beide für immer (Deadlock).
/// Bei fremden Prozessen liest Windows den Titel dagegen ohne Nachricht aus.
pub fn find_eve_windows() -> Vec<EveWindow> {
    let mut all: Vec<Hwnd> = Vec::new();
    unsafe {
        EnumWindows(Some(collect_windows), &mut all as *mut Vec<Hwnd> as LPARAM);
    }
    let own_pid = unsafe { GetCurrentProcessId() };
    all.into_iter()
        .filter(|&h| window_pid(h) != own_pid)
        .filter(|&h| unsafe { IsWindowVisible(hwnd(h)) } != 0)
        .filter_map(|h| {
            let title = window_text(h);
            is_eve_window(h, &title).then(|| EveWindow {
                hwnd: h,
                pid: window_pid(h),
                character: character_from_title(&title),
            })
        })
        .collect()
}

/// Aktuelles Vordergrundfenster.
pub fn foreground() -> Hwnd {
    handle(unsafe { GetForegroundWindow() })
}

/// Prüft, ob das Fenster minimiert ist.
pub fn is_minimized(h: Hwnd) -> bool {
    unsafe { IsIconic(hwnd(h)) != 0 }
}

/// Wandelt ein Win32-RECT in unser Rect um.
fn from_rect(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Fensterrechteck inklusive unsichtbarer Win10/11-Ränder.
fn raw_window_rect(h: Hwnd) -> Option<RECT> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    (unsafe { GetWindowRect(hwnd(h), &mut r) } != 0).then_some(r)
}

/// Sichtbares Fensterrechteck (ohne die unsichtbaren DWM-Schattenränder).
fn visible_rect_raw(h: Hwnd) -> Option<RECT> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd(h),
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut r as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if hr == 0 { Some(r) } else { raw_window_rect(h) }
}

/// Sichtbares Fensterrechteck als Rect (so, wie es der Nutzer wahrnimmt).
pub fn visible_rect(h: Hwnd) -> Option<Rect> {
    visible_rect_raw(h).map(from_rect)
}

/// Entfernt oder ergänzt Titelleiste und Rahmen. Gibt zurück, ob sich etwas geändert hat.
fn set_borderless(h: Hwnd, borderless: bool) -> bool {
    let frame = (WS_CAPTION | WS_THICKFRAME) as isize;
    let style = unsafe { GetWindowLongPtrW(hwnd(h), GWL_STYLE) };
    let new_style = if borderless { style & !frame } else { style | frame };
    if new_style == style {
        return false;
    }
    unsafe { SetWindowLongPtrW(hwnd(h), GWL_STYLE, new_style) };
    true
}

/// Platziert ein Fenster so, dass sein *sichtbarer* Bereich genau `target` entspricht.
/// Minimierte Fenster werden dafür (ohne Aktivierung) wiederhergestellt.
pub fn place_window(h: Hwnd, target: Rect, borderless: bool) {
    unsafe {
        if is_minimized(h) {
            ShowWindow(hwnd(h), SW_SHOWNOACTIVATE);
        }
        // Maximierte Fenster lassen sich nicht verschieben – erst in den Normalzustand.
        if IsZoomed(hwnd(h)) != 0 {
            ShowWindow(hwnd(h), SW_RESTORE);
        }
    }
    let style_changed = set_borderless(h, borderless);
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | if style_changed { SWP_FRAMECHANGED } else { 0 };
    // Erst grob setzen, dann um die unsichtbaren Ränder korrigieren.
    unsafe {
        SetWindowPos(
            hwnd(h),
            std::ptr::null_mut(),
            target.x,
            target.y,
            target.w,
            target.h,
            flags,
        );
    }
    let (Some(win), Some(vis)) = (raw_window_rect(h), visible_rect_raw(h)) else {
        return;
    };
    let (dl, dt) = (vis.left - win.left, vis.top - win.top);
    let (dr, db) = (win.right - vis.right, win.bottom - vis.bottom);
    if dl != 0 || dt != 0 || dr != 0 || db != 0 {
        unsafe {
            SetWindowPos(
                hwnd(h),
                std::ptr::null_mut(),
                target.x - dl,
                target.y - dt,
                target.w + dl + dr,
                target.h + dt + db,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Holt ein Fenster in den Vordergrund (stellt es bei Bedarf wieder her).
///
/// Windows erlaubt das nur dem Prozess mit der letzten Eingabe – das trifft nach
/// einem Hotkey oder Klick auf die Vorschau zu. Als Rückfallebene wird kurz die
/// Eingabe-Warteschlange mit dem aktuellen Vordergrund-Thread verbunden.
/// Es werden dabei keinerlei Tastatur- oder Mauseingaben erzeugt.
pub fn activate(h: Hwnd) -> bool {
    unsafe {
        let w = hwnd(h);
        if IsIconic(w) != 0 {
            ShowWindow(w, SW_RESTORE);
        }
        if SetForegroundWindow(w) != 0 {
            return true;
        }
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let me = GetCurrentThreadId();
        if fg_thread == 0 || fg_thread == me {
            return false;
        }
        AttachThreadInput(me, fg_thread, 1);
        BringWindowToTop(w);
        let ok = SetForegroundWindow(w) != 0;
        AttachThreadInput(me, fg_thread, 0);
        ok
    }
}

/// Minimiert ein Fenster, ohne ein anderes zu aktivieren.
pub fn minimize(h: Hwnd) {
    unsafe {
        ShowWindow(hwnd(h), SW_SHOWMINNOACTIVE);
    }
}

pub use crate::engine_api::MonitorInfo;

/// Callback für EnumDisplayMonitors: liest Fläche, Arbeitsbereich und Namen.
unsafe extern "system" fn collect_monitor(mon: HMONITOR, _dc: HDC, _r: *mut RECT, lparam: LPARAM) -> i32 {
    // SAFETY: lparam zeigt auf den Vec aus `monitors()`, der während des Aufrufs lebt.
    let list = unsafe { &mut *(lparam as *mut Vec<MonitorInfo>) };
    let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe { GetMonitorInfoW(mon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) };
    if ok != 0 {
        let len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        list.push(MonitorInfo {
            name: String::from_utf16_lossy(&info.szDevice[..len]),
            area: from_rect(info.monitorInfo.rcMonitor),
            work: from_rect(info.monitorInfo.rcWork),
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        });
    }
    1
}

/// Listet alle Monitore auf; der Hauptmonitor steht zuerst, danach von links nach rechts.
pub fn monitors() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect_monitor),
            &mut list as *mut Vec<MonitorInfo> as LPARAM,
        );
    }
    list.sort_by_key(|m| (!m.primary, m.area.x, m.area.y));
    list
}

/// Sprache der Windows-Oberfläche als LANGID (z. B. 0x0407 = Deutsch).
pub fn ui_language() -> u16 {
    unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() }
}

/// Öffnet einen Ordner (oder eine Datei) im Windows-Explorer.
pub fn open_in_explorer(path: &std::path::Path) {
    let verb = wide("open");
    let target = wide(&path.display().to_string());
    unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Autostart über `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
pub mod autostart {
    use super::wide;
    use windows_sys::Win32::System::Registry::*;

    /// Registry-Pfad der Autostart-Einträge des Benutzers.
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    /// Name unseres Eintrags.
    const VALUE_NAME: &str = "EveMultibox";

    /// Befehlszeile, mit der Windows das Programm startet (im Tray).
    fn command_line() -> Option<String> {
        let exe = std::env::current_exe().ok()?;
        Some(format!("\"{}\" --minimized", exe.display()))
    }

    /// Öffnet den Run-Schlüssel mit den angegebenen Rechten.
    fn open_key(access: REG_SAM_FLAGS) -> Option<HKEY> {
        let path = wide(RUN_KEY);
        let mut key: HKEY = std::ptr::null_mut();
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
        (rc == 0).then_some(key)
    }

    /// Liest den gespeicherten Autostart-Befehl (None = kein Eintrag).
    fn stored_command() -> Option<String> {
        let key = open_key(KEY_READ)?;
        let name = wide(VALUE_NAME);
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut u8,
                &mut len,
            )
        };
        unsafe { RegCloseKey(key) };
        if rc != 0 {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        let end = buf[..chars].iter().position(|&c| c == 0).unwrap_or(chars);
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    /// Ist der Autostart eingerichtet?
    pub fn is_enabled() -> bool {
        stored_command().is_some()
    }

    /// Richtet den Autostart ein oder entfernt ihn.
    pub fn set(enabled: bool) -> Result<(), String> {
        let key = open_key(KEY_SET_VALUE).ok_or("Registry nicht beschreibbar")?;
        let name = wide(VALUE_NAME);
        let rc = if enabled {
            let cmd = command_line().ok_or("Programmpfad unbekannt")?;
            let data = wide(&cmd);
            unsafe {
                RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            }
        } else {
            match unsafe { RegDeleteValueW(key, name.as_ptr()) } {
                2 => 0, // ERROR_FILE_NOT_FOUND: war ohnehin nicht eingetragen
                rc => rc,
            }
        };
        unsafe { RegCloseKey(key) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("Fehlercode {rc}"))
        }
    }

    /// Wurde das Programm verschoben, den gespeicherten Pfad stillschweigend aktualisieren.
    pub fn refresh_path() {
        if let (Some(stored), Some(current)) = (stored_command(), command_line())
            && stored != current
        {
            let _ = set(true);
        }
    }
}
