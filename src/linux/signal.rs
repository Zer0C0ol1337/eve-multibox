//! Weckruf der Engine: andere Threads (Tray-Menü, Anfrage einer zweiten Instanz,
//! Oberfläche) legen ein Signal ab und wecken den Engine-Thread über ein eventfd,
//! auf das dieser per `poll` neben der X-Verbindung wartet.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Signale an die Engine, die nicht von der Oberfläche kommen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// Dashboard anzeigen (Tray-Klick oder zweite Instanz).
    Show,
    /// Vorschauen ein-/ausblenden.
    TogglePreviews,
    /// Layout anwenden.
    ApplyLayout,
    /// Programm beenden.
    Quit,
}

/// Warteschlange der Signale.
static PENDING: Mutex<Vec<Signal>> = Mutex::new(Vec::new());
/// eventfd der Engine (-1 = noch keine Engine).
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);
/// Beenden per Unix-Signal (SIGINT/SIGTERM/SIGHUP) angefordert.
static TERMINATE: AtomicBool = AtomicBool::new(false);

/// Signal-Handler: nur async-signal-sichere Operationen (Atomics und `write`).
extern "C" fn on_terminate(_sig: libc::c_int) {
    TERMINATE.store(true, Ordering::SeqCst);
    let fd = WAKE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        let one: u64 = 1;
        unsafe { libc::write(fd, &one as *const u64 as *const libc::c_void, 8) };
    }
}

/// Fängt SIGINT, SIGTERM und SIGHUP ab, damit beim Beenden (Strg+C, Abmelden, kill)
/// Priorität und Kerne der Clients sauber zurückgesetzt werden.
pub fn install_terminate_handlers() {
    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_terminate as *const () as usize;
            sa.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

/// Legt ein neues eventfd an und meldet es als Weckziel an.
pub fn create_wake_fd() -> i32 {
    let fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    WAKE_FD.store(fd, Ordering::SeqCst);
    fd
}

/// Weckt die Engine (sicher aus jedem Thread).
pub fn wake() {
    let fd = WAKE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        let one: u64 = 1;
        unsafe { libc::write(fd, &one as *const u64 as *const libc::c_void, 8) };
    }
}

/// Setzt den Weckzähler zurück (nach dem Aufwachen aufrufen).
pub fn drain_wake(fd: i32) {
    let mut buf: u64 = 0;
    unsafe { libc::read(fd, &mut buf as *mut u64 as *mut libc::c_void, 8) };
}

/// Legt ein Signal ab und weckt die Engine.
pub fn post(signal: Signal) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push(signal);
    wake();
}

/// Holt alle offenen Signale ab (ein Beenden per Unix-Signal erscheint als `Quit`).
pub fn take() -> Vec<Signal> {
    let mut list = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    if TERMINATE.swap(false, Ordering::SeqCst) {
        list.push(Signal::Quit);
    }
    list
}

/// Meldet das Weckziel ab und schließt das eventfd.
pub fn close_wake_fd(fd: i32) {
    let _ = WAKE_FD.compare_exchange(fd, -1, Ordering::SeqCst, Ordering::SeqCst);
    if fd >= 0 {
        unsafe { libc::close(fd) };
    }
}
