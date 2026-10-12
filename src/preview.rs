//! Live-Vorschau aller Clients über DWM-Thumbnails (dieselbe Technik wie EVE-O Preview).
//!
//! Jede Vorschau ist ein kleines, randloses Topmost-Fenster, in das Windows (DWM)
//! das Bild des EVE-Fensters live hineinzeichnet. Es wird nichts abgefilmt oder
//! in den Client eingegriffen.
//!
//! Bedienung:
//! * Linksklick = Client fokussieren, Rechtsklick = ins Hauptfenster tauschen
//! * Linke Maustaste gedrückt halten und ziehen = Vorschau verschieben
//! * Am Rand bzw. an der Ecke ziehen = Größe frei ändern (Mauszeiger zeigt die Richtung)
//! * Beim Verschieben und Größe-Ziehen rasten die Kanten an anderen Vorschauen
//!   und am Bildschirmrand ein (abschaltbar). Position und Größe werden je Charakter gespeichert.
//!
//! Alle Funktionen laufen im Engine-Thread. Die Fensterprozedur greift nie auf die
//! Engine zu, sondern legt Ereignisse in eine Warteschlange und weckt die Engine per
//! Nachricht – so gibt es keine Wiedereintritts-Probleme.

use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use evemb::config::{PreviewSettings, Rect};
use evemb::layout::{self, Edges};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::win::{self, Hwnd, wide};

/// Nachricht an das Engine-Fenster: „Es liegen Vorschau-Ereignisse an.“
pub const WM_PREVIEW_EVENT: u32 = WM_APP + 2;

/// Höhe der Titelzeile über dem Bild in Pixeln.
const TITLE_H: i32 = 24;
/// Rahmenbreite (für die Hervorhebung des aktiven Clients).
const BORDER: i32 = 2;
/// Breite der unsichtbaren Greifzone am Rand für das Größe-Ziehen.
const RESIZE_EDGE: i32 = 7;
/// Kleinste erlaubte Bildgröße.
const MIN_THUMB: (i32, i32) = (64, 36);
/// Mausweg in Pixeln, ab dem ein Klick als Ziehen gilt.
const DRAG_THRESHOLD: i32 = 5;
/// Klassenname der Vorschaufenster.
const CLASS_NAME: &str = "EveMultiboxPreview";

/// Fenster, das über Vorschau-Ereignisse benachrichtigt wird (das Engine-Fenster).
static NOTIFY_TARGET: AtomicIsize = AtomicIsize::new(0);

/// Ereignis aus einem Vorschaufenster.
#[derive(Clone, Debug)]
pub enum PreviewEvent {
    /// Linksklick: Quell-Client fokussieren.
    Click(Hwnd),
    /// Rechtsklick: Quell-Client ins Hauptfenster tauschen.
    MakeMain(Hwnd),
    /// Vorschau wurde verschoben oder in der Größe geändert:
    /// linke obere Ecke des Fensters und neue Bildgröße (ohne Rahmen/Titel).
    Moved {
        source: Hwnd,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
}

/// Was die Fensterprozedur zum Zeichnen, für Klicks und fürs Größe-Ziehen wissen muss.
#[derive(Clone, Debug, PartialEq)]
struct PaintInfo {
    source: Hwnd,
    thumb: isize,
    label: String,
    active: bool,
    show_title: bool,
    highlight: bool,
}

thread_local! {
    /// Infos je Vorschaufenster (nur kurz ausgeliehen, nie über Win32-Aufrufe hinweg).
    static PAINT: RefCell<HashMap<Hwnd, PaintInfo>> = RefCell::new(HashMap::new());
    /// Warteschlange der Ereignisse für die Engine.
    static EVENTS: RefCell<Vec<PreviewEvent>> = const { RefCell::new(Vec::new()) };
    /// Vorschaufenster, das gerade gezogen wird (0 = keins) – die Engine fasst es dann nicht an.
    static DRAGGING: Cell<Hwnd> = const { Cell::new(0) };
    /// Einrast-Einstellungen für die Fensterprozedur: (aktiv, Fangbereich, Abstand).
    static SNAP: Cell<(bool, i32, i32)> = const { Cell::new((true, 20, 2)) };
}

/// Legt das Fenster fest, das bei Ereignissen geweckt wird.
pub fn set_notify_target(h: Hwnd) {
    NOTIFY_TARGET.store(h, Ordering::SeqCst);
}

/// Holt alle angefallenen Vorschau-Ereignisse ab.
pub fn take_events() -> Vec<PreviewEvent> {
    EVENTS.with(|e| std::mem::take(&mut *e.borrow_mut()))
}

/// Reiht ein Ereignis ein und weckt die Engine.
fn push_event(ev: PreviewEvent) {
    EVENTS.with(|e| e.borrow_mut().push(ev));
    let target = NOTIFY_TARGET.load(Ordering::SeqCst);
    if target != 0 {
        unsafe { PostMessageW(win::hwnd(target), WM_PREVIEW_EVENT, 0, 0) };
    }
}

/// Infos eines Vorschaufensters (Kopie).
fn info_of(wnd: Hwnd) -> Option<PaintInfo> {
    PAINT.with(|p| p.borrow().get(&wnd).cloned())
}

/// Aktuelle Mausposition in Bildschirmkoordinaten.
fn cursor_pos() -> (i32, i32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}

/// Leeres RECT als Startwert.
const fn empty_rect() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }
}

/// Wandelt ein Win32-RECT in unser Rect um.
fn to_rect(r: &RECT) -> Rect {
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

/// Wandelt unser Rect in ein Win32-RECT um.
fn to_win_rect(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.right(),
        bottom: r.bottom(),
    }
}

/// Höhe der Titelzeile je nach Einstellung.
fn title_height(show_title: bool) -> i32 {
    if show_title { TITLE_H } else { 0 }
}

/// Fenstergröße für eine gegebene Bildgröße (plus Rahmen und Titel).
fn window_size(thumb: (i32, i32), show_title: bool) -> (i32, i32) {
    (thumb.0 + 2 * BORDER, thumb.1 + title_height(show_title) + 2 * BORDER)
}

/// Bildgröße aus einer Fenstergröße (Gegenstück zu `window_size`).
fn thumb_size(win_w: i32, win_h: i32, show_title: bool) -> (i32, i32) {
    (
        (win_w - 2 * BORDER).max(MIN_THUMB.0),
        (win_h - 2 * BORDER - title_height(show_title)).max(MIN_THUMB.1),
    )
}

/// Zielbereich des Thumbnails innerhalb eines Fensters der gegebenen Größe.
fn dest_rect(win_w: i32, win_h: i32, show_title: bool) -> RECT {
    RECT {
        left: BORDER,
        top: BORDER + title_height(show_title),
        right: win_w - BORDER,
        bottom: win_h - BORDER,
    }
}

/// Setzt Zielbereich, Sichtbarkeit und Optionen eines Thumbnails.
fn update_thumbnail(thumb: isize, dest: RECT) {
    let props = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
        rcDestination: dest,
        rcSource: empty_rect(),
        opacity: 255,
        fVisible: 1,
        fSourceClientAreaOnly: 1,
    };
    unsafe { DwmUpdateThumbnailProperties(thumb, &props) };
}

/// Registriert die Fensterklasse der Vorschaufenster (einmal pro Prozess).
pub fn register_class() {
    let class = wide(CLASS_NAME);
    unsafe {
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(std::ptr::null()),
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_HAND),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
    }
}

/// Greifzone eines Punkts im Fenster: `Some(Kanten)` am Rand/an der Ecke (Größe ändern),
/// `None` in der Innenfläche (verschieben).
fn grab_zone(r: &RECT, x: i32, y: i32) -> Option<Edges> {
    let e = Edges {
        left: x < r.left + RESIZE_EDGE,
        right: x >= r.right - RESIZE_EDGE,
        top: y < r.top + RESIZE_EDGE,
        bottom: y >= r.bottom - RESIZE_EDGE,
    };
    (e.left || e.right || e.top || e.bottom).then_some(e)
}

/// Passender Mauszeiger für eine Greifzone.
fn cursor_for(zone: Option<Edges>) -> *const u16 {
    match zone {
        None => IDC_HAND,
        Some(e) if (e.left && e.top) || (e.right && e.bottom) => IDC_SIZENWSE,
        Some(e) if (e.right && e.top) || (e.left && e.bottom) => IDC_SIZENESW,
        Some(e) if e.left || e.right => IDC_SIZEWE,
        Some(_) => IDC_SIZENS,
    }
}

/// Neues Rechteck beim Größe-Ziehen: bewegte Kanten um (dx, dy) verschieben, Mindestgröße halten.
fn resize_rect(start: Rect, e: Edges, dx: i32, dy: i32, min: (i32, i32)) -> Rect {
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.right(), start.bottom());
    if e.left {
        l = (l + dx).min(r - min.0);
    }
    if e.right {
        r = (r + dx).max(l + min.0);
    }
    if e.top {
        t = (t + dy).min(b - min.1);
    }
    if e.bottom {
        b = (b + dy).max(t + min.1);
    }
    Rect::new(l, t, r - l, b - t)
}

/// Fensterrechteck eines Vorschaufensters.
fn window_rect(h: HWND) -> RECT {
    let mut r = empty_rect();
    unsafe { GetWindowRect(h, &mut r) };
    r
}

/// Laufender Zieh-Vorgang mit gedrückter linker Maustaste.
#[derive(Clone, Copy)]
struct Drag {
    wnd: Hwnd,
    start_cursor: (i32, i32),
    start_rect: Rect,
    /// `None` = verschieben, sonst Größe ändern an diesen Kanten.
    edges: Option<Edges>,
    /// Schwelle überschritten – es wird wirklich gezogen (sonst ist es ein Klick).
    active: bool,
}

thread_local! {
    /// Aktueller Zieh-Vorgang (höchstens einer gleichzeitig).
    static DRAG: Cell<Option<Drag>> = const { Cell::new(None) };
}

/// Fensterprozedur der Vorschaufenster: Zeichnen, Klick, Verschieben, Größe ändern.
/// Verschieben und Größe ändern sind komplett selbst umgesetzt (Maus einfangen und
/// Fenster nachführen) – ohne Windows-Rahmen, daher ohne sichtbare Randstreifen.
unsafe extern "system" fn preview_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let wnd = win::handle(h);
    match msg {
        // Vorschau soll den Fokus nie stehlen.
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(h);
            0
        }
        WM_SETCURSOR => {
            let zone = match DRAG.with(|d| d.get()).filter(|d| d.wnd == wnd) {
                Some(d) => d.edges,
                None => {
                    let (x, y) = cursor_pos();
                    grab_zone(&window_rect(h), x, y)
                }
            };
            unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), cursor_for(zone))) };
            1
        }
        WM_SIZE => {
            // Bild beim Größe-Ziehen sofort mitwachsen lassen.
            if let Some(info) = info_of(wnd).filter(|i| i.thumb != 0) {
                let (w, hh) = ((lp & 0xFFFF) as i32, ((lp >> 16) & 0xFFFF) as i32);
                update_thumbnail(info.thumb, dest_rect(w, hh, info.show_title));
            }
            unsafe { InvalidateRect(h, std::ptr::null(), 0) };
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = cursor_pos();
            let r = window_rect(h);
            DRAG.with(|d| {
                d.set(Some(Drag {
                    wnd,
                    start_cursor: (x, y),
                    start_rect: to_rect(&r),
                    edges: grab_zone(&r, x, y),
                    active: false,
                }))
            });
            unsafe { SetCapture(h) };
            0
        }
        WM_MOUSEMOVE => {
            if let Some(mut d) = DRAG.with(|d| d.get()).filter(|d| d.wnd == wnd) {
                let (x, y) = cursor_pos();
                let (dx, dy) = (x - d.start_cursor.0, y - d.start_cursor.1);
                if !d.active && (dx.abs() > DRAG_THRESHOLD || dy.abs() > DRAG_THRESHOLD) {
                    d.active = true;
                    DRAGGING.with(|g| g.set(wnd));
                }
                if d.active {
                    drag_to(h, &d, dx, dy);
                }
                DRAG.with(|c| c.set(Some(d)));
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(d) = DRAG.with(|c| c.take()).filter(|d| d.wnd == wnd) {
                unsafe { ReleaseCapture() };
                if d.active {
                    finish_drag(h, wnd);
                } else if let Some(info) = info_of(wnd) {
                    push_event(PreviewEvent::Click(info.source));
                }
            }
            0
        }
        WM_CAPTURECHANGED => {
            // Maus wurde uns entzogen (z. B. Alt+Tab mitten im Ziehen): Zwischenstand speichern.
            if DRAG.with(|c| c.take()).is_some_and(|d| d.wnd == wnd && d.active) {
                finish_drag(h, wnd);
            }
            0
        }
        WM_RBUTTONUP => {
            if let Some(info) = info_of(wnd) {
                push_event(PreviewEvent::MakeMain(info.source));
            }
            0
        }
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

/// Führt das Fenster beim Ziehen nach: verschieben oder Größe ändern, inklusive Einrasten.
fn drag_to(h: HWND, d: &Drag, dx: i32, dy: i32) {
    let show_title = info_of(d.wnd).is_none_or(|i| i.show_title);
    let min = window_size(MIN_THUMB, show_title);
    let proposed = match d.edges {
        None => Rect::new(d.start_rect.x + dx, d.start_rect.y + dy, d.start_rect.w, d.start_rect.h),
        Some(e) => resize_rect(d.start_rect, e, dx, dy, min),
    };
    let r = snap(d.wnd, proposed, d.edges, min);
    let flags = SWP_NOZORDER | SWP_NOACTIVATE | if d.edges.is_none() { SWP_NOSIZE } else { 0 };
    unsafe { SetWindowPos(h, std::ptr::null_mut(), r.x, r.y, r.w, r.h, flags) };
}

/// Zieh-Vorgang beenden und neue Position/Größe an die Engine melden.
fn finish_drag(h: HWND, wnd: Hwnd) {
    DRAGGING.with(|g| g.set(0));
    let r = window_rect(h);
    if let Some(info) = info_of(wnd) {
        let (w, hh) = thumb_size(r.right - r.left, r.bottom - r.top, info.show_title);
        push_event(PreviewEvent::Moved {
            source: info.source,
            x: r.left,
            y: r.top,
            w,
            h: hh,
        });
    }
}

/// Lässt ein gezogenes Rechteck an den anderen Vorschauen und am Arbeitsbereich des
/// Monitors einrasten. `edges = None` heißt Verschieben, sonst Größe ändern an diesen Kanten.
fn snap(wnd: Hwnd, r: Rect, edges: Option<Edges>, min: (i32, i32)) -> Rect {
    let (enabled, distance, gap) = SNAP.with(|s| s.get());
    if !enabled {
        return r;
    }
    let windows: Vec<Hwnd> = PAINT.with(|p| p.borrow().keys().copied().filter(|&w| w != wnd).collect());
    let others: Vec<Rect> = windows
        .into_iter()
        .filter(|&w| unsafe { IsWindowVisible(win::hwnd(w)) } != 0)
        .map(|w| to_rect(&window_rect(win::hwnd(w))))
        .collect();
    let mut bounds = Vec::new();
    unsafe {
        let wr = to_win_rect(r);
        let mon = MonitorFromRect(&wr, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) != 0 {
            bounds.push(to_rect(&info.rcWork));
        }
    }
    match edges {
        None => layout::snap_rect(r, &others, &bounds, distance, gap),
        Some(e) => {
            let s = layout::snap_resize(r, e, &others, &bounds, distance, gap);
            // Nie unter die Mindestgröße einrasten.
            if s.w < min.0 || s.h < min.1 { r } else { s }
        }
    }
}

/// Farben im GDI-Format 0x00BBGGRR – passend zum Dashboard.
const COLOR_FRAME: u32 = 0x002C241E;
const COLOR_FRAME_ACTIVE: u32 = 0x0000B0FF;
const COLOR_TITLE_BG: u32 = 0x0017120E;
const COLOR_TEXT: u32 = 0x00E4DCD6;
const COLOR_TEXT_ACTIVE: u32 = 0x0040C6FF;

/// Zeichnet Rahmen und Titelzeile (dunkle Leiste, Segoe UI, Akzentpunkt für den aktiven
/// Client); das Bild selbst zeichnet DWM darüber.
fn paint(h: HWND) {
    let info = info_of(win::handle(h));
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(h, &mut ps);
        let mut rc = empty_rect();
        GetClientRect(h, &mut rc);
        let (active, highlight) = info.as_ref().map_or((false, false), |i| (i.active, i.highlight));
        let frame = CreateSolidBrush(if active && highlight {
            COLOR_FRAME_ACTIVE
        } else {
            COLOR_FRAME
        });
        FillRect(dc, &rc, frame);
        DeleteObject(frame);
        if let Some(info) = info.filter(|i| i.show_title) {
            let bar = RECT {
                left: BORDER,
                top: BORDER,
                right: rc.right - BORDER,
                bottom: BORDER + TITLE_H,
            };
            let bg = CreateSolidBrush(COLOR_TITLE_BG);
            FillRect(dc, &bar, bg);
            DeleteObject(bg);

            // Akzentpunkt links für den aktiven Client.
            let mut text_left = bar.left + 9;
            if info.active {
                let cy = (bar.top + bar.bottom) / 2;
                let dot = CreateSolidBrush(COLOR_FRAME_ACTIVE);
                let old_brush = SelectObject(dc, dot);
                let old_pen = SelectObject(dc, GetStockObject(NULL_PEN));
                Ellipse(dc, bar.left + 7, cy - 4, bar.left + 16, cy + 5);
                SelectObject(dc, old_pen);
                SelectObject(dc, old_brush);
                DeleteObject(dot);
                text_left = bar.left + 21;
            }

            let face = wide("Segoe UI Semibold");
            let font = CreateFontW(
                -14,
                0,
                0,
                0,
                FW_SEMIBOLD as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                0,
                face.as_ptr(),
            );
            let old_font = SelectObject(dc, font);
            SetBkMode(dc, TRANSPARENT as _);
            SetTextColor(dc, if info.active { COLOR_TEXT_ACTIVE } else { COLOR_TEXT });
            let mut text_rc = RECT {
                left: text_left,
                right: bar.right - 9,
                ..bar
            };
            let mut text: Vec<u16> = info.label.encode_utf16().collect();
            DrawTextW(
                dc,
                text.as_mut_ptr(),
                text.len() as i32,
                &mut text_rc,
                DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(dc, old_font);
            DeleteObject(font);
        }
        EndPaint(h, &ps);
    }
}

/// Ein Eintrag, den die Engine anzeigen möchte.
#[derive(Clone, Debug)]
pub struct PreviewItem {
    /// Fenster des EVE-Clients.
    pub source: Hwnd,
    /// Schlüssel für gespeicherte Positionen/Größen (Charaktername).
    pub key: String,
    /// Angezeigter Text.
    pub label: String,
    /// Ist gerade im Vordergrund.
    pub active: bool,
    /// Soll sichtbar sein.
    pub visible: bool,
}

/// Zustand eines Vorschaufensters.
struct PreviewWin {
    wnd: Hwnd,
    thumb: isize,
    rect: Rect,
    visible: bool,
    alpha: u8,
    dest: (i32, i32, i32, i32),
}

/// Verwaltet alle Vorschaufenster.
#[derive(Default)]
pub struct PreviewManager {
    wins: HashMap<Hwnd, PreviewWin>,
}

impl PreviewManager {
    /// Bringt alle Vorschaufenster auf den gewünschten Stand: anlegen, verschieben,
    /// Größe setzen, ein-/ausblenden, Thumbnail aktualisieren und verwaiste entfernen.
    pub fn sync(&mut self, items: &[PreviewItem], s: &PreviewSettings) {
        let default_size = window_size((s.width, s.height), s.show_titles);
        let dragging = DRAGGING.with(|d| d.get());
        SNAP.with(|c| c.set((s.snap, s.snap_distance, s.gap)));
        let mut seen = HashSet::new();

        for (i, item) in items.iter().enumerate() {
            seen.insert(item.source);
            let thumb = s.sizes.get(&item.key).copied().unwrap_or((s.width, s.height));
            let (ww, wh) = window_size(thumb, s.show_titles);
            let (x, y) = s.positions.get(&item.key).copied().unwrap_or_else(|| {
                layout::auto_preview_pos(i, (s.start_x, s.start_y), default_size, s.gap, s.vertical, s.per_line)
            });
            let rect = Rect::new(x, y, ww, wh);
            let d = dest_rect(ww, wh, s.show_titles);
            let dest = (d.left, d.top, d.right, d.bottom);

            let w = match self.wins.entry(item.source) {
                Entry::Occupied(e) => e.into_mut(),
                Entry::Vacant(v) => match create_window(rect, s.opacity) {
                    Some(w) => v.insert(w),
                    None => continue,
                },
            };

            // Thumbnail registrieren (bei Fehlschlag im nächsten Durchlauf erneut versuchen).
            if w.thumb == 0 {
                let mut t: isize = 0;
                let hr = unsafe { DwmRegisterThumbnail(win::hwnd(w.wnd), win::hwnd(item.source), &mut t) };
                if hr == 0 {
                    w.thumb = t;
                    w.dest = (0, 0, 0, 0);
                }
            }

            // Infos aktualisieren (nur bei Änderung neu zeichnen).
            let info = PaintInfo {
                source: item.source,
                thumb: w.thumb,
                label: item.label.clone(),
                active: item.active,
                show_title: s.show_titles,
                highlight: s.highlight_active,
            };
            let changed = PAINT.with(|p| p.borrow_mut().insert(w.wnd, info.clone()) != Some(info));
            if changed {
                unsafe { InvalidateRect(win::hwnd(w.wnd), std::ptr::null(), 0) };
            }

            // Position/Größe – nie während der Nutzer die Vorschau zieht.
            if w.wnd != dragging {
                if w.rect != rect || (item.visible && !w.visible) {
                    unsafe {
                        SetWindowPos(
                            win::hwnd(w.wnd),
                            HWND_TOPMOST,
                            rect.x,
                            rect.y,
                            rect.w,
                            rect.h,
                            SWP_NOACTIVATE,
                        );
                    }
                    w.rect = rect;
                }
                if w.thumb != 0 && w.dest != dest {
                    update_thumbnail(w.thumb, d);
                    w.dest = dest;
                }
            }

            if w.alpha != s.opacity {
                unsafe { SetLayeredWindowAttributes(win::hwnd(w.wnd), 0, s.opacity, LWA_ALPHA) };
                w.alpha = s.opacity;
            }

            if item.visible != w.visible {
                let cmd = if item.visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
                unsafe { ShowWindow(win::hwnd(w.wnd), cmd) };
                w.visible = item.visible;
            }
        }

        let stale: Vec<Hwnd> = self.wins.keys().copied().filter(|k| !seen.contains(k)).collect();
        for src in stale {
            if let Some(w) = self.wins.remove(&src) {
                destroy_window(w);
            }
        }
    }

    /// Entfernt alle Vorschaufenster.
    pub fn clear(&mut self) {
        for (_, w) in self.wins.drain() {
            destroy_window(w);
        }
    }
}

/// Legt ein (zunächst unsichtbares), rahmenloses Vorschaufenster an.
fn create_window(rect: Rect, alpha: u8) -> Option<PreviewWin> {
    let class = wide(CLASS_NAME);
    let title = wide("EVE Multibox Vorschau");
    let h = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    if h.is_null() {
        return None;
    }
    unsafe {
        SetLayeredWindowAttributes(h, 0, alpha, LWA_ALPHA);
        // Windows 11: keine abgerundeten Ecken, damit Vorschauen bündig aneinanderliegen.
        let pref: i32 = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(
            h,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &pref as *const i32 as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
    }
    Some(PreviewWin {
        wnd: win::handle(h),
        thumb: 0,
        rect,
        visible: false,
        alpha,
        dest: (0, 0, 0, 0),
    })
}

/// Gibt Thumbnail und Fenster frei.
fn destroy_window(w: PreviewWin) {
    unsafe {
        if w.thumb != 0 {
            DwmUnregisterThumbnail(w.thumb);
        }
        DestroyWindow(win::hwnd(w.wnd));
    }
    PAINT.with(|p| p.borrow_mut().remove(&w.wnd));
}
