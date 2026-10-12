//! Prozess-Werkzeuge unter Linux: CPU-Zeit und Speicher aus /proc, Priorität (nice),
//! Kern-Affinität, Speicher auslagern (MADV_PAGEOUT) sowie systemweite CPU-/RAM-Werte.
//!
//! Priorität und Affinität gelten unter Linux pro Thread – deshalb werden immer alle
//! Threads des EVE-Prozesses (/proc/PID/task) angepasst.
//!
//! Rechte: Eine niedrigere Priorität (höheres nice) darf jeder setzen, zurück auf normal
//! oder höher nur mit CAP_SYS_NICE (bzw. passendem RLIMIT_NICE). Speicher auslagern
//! braucht ebenfalls CAP_SYS_NICE. Affinität geht immer für eigene Prozesse.

use std::time::Instant;

use evemb::config::Priority;

pub use crate::engine_api::MemInfo;

/// Bit von CAP_SYS_NICE in den Capability-Masken.
const CAP_SYS_NICE: u32 = 23;
/// Maximal so viele Speicherbereiche je `process_madvise`-Aufruf.
const IOV_BATCH: usize = 512;

/// nice-Wert zu einer Prioritätsstufe.
fn nice_of(p: Priority) -> i32 {
    match p {
        Priority::Idle => 19,
        Priority::BelowNormal => 10,
        Priority::Normal => 0,
        Priority::AboveNormal => -5,
        Priority::High => -10,
    }
}

/// Prioritätsstufe zu einem nice-Wert (nächstliegende Stufe).
fn priority_of(nice: i32) -> Priority {
    match nice {
        n if n >= 15 => Priority::Idle,
        n if n >= 5 => Priority::BelowNormal,
        n if n > -3 => Priority::Normal,
        n if n > -8 => Priority::AboveNormal,
        _ => Priority::High,
    }
}

/// Hat dieses Programm CAP_SYS_NICE (effektiv)?
pub fn has_cap_sys_nice() -> bool {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix("CapEff:"))
        .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
        .is_some_and(|caps| caps & (1 << CAP_SYS_NICE) != 0)
}

/// Darf dieses Programm die Priorität wieder auf „Normal“ zurücksetzen?
/// (CAP_SYS_NICE oder ein RLIMIT_NICE, das nice 0 erlaubt.)
pub fn can_restore_priority() -> bool {
    if has_cap_sys_nice() {
        return true;
    }
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let ok = unsafe { libc::getrlimit(libc::RLIMIT_NICE, &mut lim) } == 0;
    // Erlaubtes Minimum für nice ist 20 - rlim_cur.
    ok && lim.rlim_cur >= 20
}

/// Ein beobachteter Prozess.
pub struct Proc {
    pid: u32,
}

impl Proc {
    /// Beobachtet den Prozess, falls er existiert.
    pub fn open(pid: u32) -> Option<Self> {
        std::path::Path::new(&format!("/proc/{pid}"))
            .exists()
            .then_some(Self { pid })
    }

    /// Thread-IDs des Prozesses.
    fn threads(&self) -> Vec<i32> {
        std::fs::read_dir(format!("/proc/{}/task", self.pid))
            .map(|rd| rd.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
            .unwrap_or_default()
    }

    /// Verbrauchte CPU-Zeit (User + System) in Sekunden.
    pub fn cpu_time(&self) -> Option<f64> {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", self.pid)).ok()?;
        // Der Prozessname steht in Klammern und kann Leerzeichen enthalten.
        let rest = &stat[stat.rfind(')')? + 2..];
        let fields: Vec<&str> = rest.split_whitespace().collect();
        let utime: f64 = fields.get(11)?.parse().ok()?;
        let stime: f64 = fields.get(12)?.parse().ok()?;
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
        Some((utime + stime) / ticks)
    }

    /// Arbeitsspeicher (RSS) und privater Speicher (anonymer RSS + Swap).
    pub fn memory(&self) -> Option<MemInfo> {
        let status = std::fs::read_to_string(format!("/proc/{}/status", self.pid)).ok()?;
        let kb = |key: &str| -> u64 {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next()?.parse().ok())
                .unwrap_or(0)
        };
        Some(MemInfo {
            working_set: kb("VmRSS:") * 1024,
            private: (kb("RssAnon:") + kb("VmSwap:")) * 1024,
        })
    }

    /// Aktuelle Priorität (aus dem nice-Wert des Hauptthreads).
    pub fn priority(&self) -> Option<Priority> {
        unsafe { *libc::__errno_location() = 0 };
        let nice = unsafe { libc::getpriority(libc::PRIO_PROCESS, self.pid) };
        let err = unsafe { *libc::__errno_location() };
        (err == 0).then(|| priority_of(nice))
    }

    /// Setzt die Priorität für alle Threads. `false`, wenn ein Thread abgelehnt hat.
    pub fn set_priority(&self, p: Priority) -> bool {
        let nice = nice_of(p);
        let mut ok = true;
        for tid in self.threads() {
            if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } != 0 {
                ok = false;
            }
        }
        ok
    }

    /// Affinitätsmaske des Hauptthreads und die Maske aller verfügbaren Kerne.
    pub fn affinity(&self) -> Option<(u64, u64)> {
        let mask = get_affinity(self.pid as i32)?;
        Some((mask, system_mask()))
    }

    /// Setzt die Affinitätsmaske für alle Threads.
    pub fn set_affinity(&self, mask: u64) -> bool {
        if mask == 0 {
            return false;
        }
        let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
        for c in 0..64 {
            if mask & (1u64 << c) != 0 {
                unsafe { libc::CPU_SET(c, &mut set) };
            }
        }
        let mut ok = true;
        for tid in self.threads() {
            let rc = unsafe { libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &set) };
            if rc != 0 {
                ok = false;
            }
        }
        ok
    }

    /// Lagert selten genutzten privaten Speicher aus (MADV_PAGEOUT über process_madvise).
    /// Braucht CAP_SYS_NICE und Linux ≥ 5.10.
    pub fn trim(&self) -> bool {
        let maps = match std::fs::read_to_string(format!("/proc/{}/maps", self.pid)) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let ranges: Vec<libc::iovec> = maps.lines().filter_map(private_writable_range).collect();
        if ranges.is_empty() {
            return false;
        }
        let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, self.pid as libc::pid_t, 0) } as i32;
        if pidfd < 0 {
            return false;
        }
        let mut ok = true;
        for chunk in ranges.chunks(IOV_BATCH) {
            let rc = unsafe {
                libc::syscall(
                    libc::SYS_process_madvise,
                    pidfd,
                    chunk.as_ptr(),
                    chunk.len(),
                    libc::MADV_PAGEOUT,
                    0u32,
                )
            };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                crate::diag::log(&format!("process_madvise für PID {}: {err}", self.pid));
                ok = false;
                break;
            }
        }
        unsafe { libc::close(pidfd) };
        ok
    }
}

/// Zeile aus /proc/PID/maps → Speicherbereich, falls privat und beschreibbar (Heap, anonym).
fn private_writable_range(line: &str) -> Option<libc::iovec> {
    let mut parts = line.split_whitespace();
    let range = parts.next()?;
    let perms = parts.next()?;
    if !perms.starts_with("rw") || !perms.ends_with('p') {
        return None;
    }
    let path = parts.nth(3).unwrap_or("");
    if !(path.is_empty() || path == "[heap]" || path.starts_with("[anon")) {
        return None;
    }
    let (a, b) = range.split_once('-')?;
    let start = usize::from_str_radix(a, 16).ok()?;
    let end = usize::from_str_radix(b, 16).ok()?;
    (end > start).then_some(libc::iovec {
        iov_base: start as *mut libc::c_void,
        iov_len: end - start,
    })
}

/// Affinitätsmaske eines Threads/Prozesses.
fn get_affinity(pid: i32) -> Option<u64> {
    let mut set: libc::cpu_set_t = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::sched_getaffinity(pid, std::mem::size_of::<libc::cpu_set_t>(), &mut set) };
    if rc != 0 {
        return None;
    }
    Some(
        (0..64)
            .filter(|&c| unsafe { libc::CPU_ISSET(c, &set) })
            .fold(0u64, |m, c| m | (1 << c)),
    )
}

/// Maske aller nutzbaren Kerne (höchstens 64).
fn system_mask() -> u64 {
    let n = logical_cores();
    if n >= 64 { u64::MAX } else { (1u64 << n) - 1 }
}

/// Misst die CPU-Auslastung eines Prozesses über die Zeit zwischen zwei Messungen.
#[derive(Default)]
pub struct CpuMeter {
    last: Option<(f64, Instant)>,
}

impl CpuMeter {
    /// Neue Messung; liefert die Auslastung in Prozent der Gesamt-CPU (0–100).
    pub fn sample(&mut self, cpu_seconds: f64, cores: u32) -> f32 {
        let now = Instant::now();
        let pct = match self.last {
            Some((prev, at)) => {
                let wall = now.duration_since(at).as_secs_f64();
                if wall > 0.0 {
                    ((cpu_seconds - prev).max(0.0) / (wall * cores.max(1) as f64) * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.last = Some((cpu_seconds, now));
        pct as f32
    }
}

/// Systemweite CPU-Auslastung aus /proc/stat.
#[derive(Default)]
pub struct SystemCpu {
    last: Option<(u64, u64)>,
}

impl SystemCpu {
    /// Neue Messung; liefert die Gesamtauslastung in Prozent.
    pub fn sample(&mut self) -> f32 {
        let Some((total, idle)) = read_cpu_line() else {
            return 0.0;
        };
        let pct = match self.last {
            Some((t0, i0)) => {
                let dt = total.saturating_sub(t0) as f64;
                let di = idle.saturating_sub(i0) as f64;
                if dt > 0.0 {
                    ((dt - di) / dt * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.last = Some((total, idle));
        pct as f32
    }
}

/// Erste Zeile von /proc/stat → (Gesamtzeit, Leerlaufzeit inkl. I/O-Warten).
fn read_cpu_line() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 5 {
        return None;
    }
    let total = v.iter().take(8).sum();
    Some((total, v[3] + v[4]))
}

/// Gesamter und verfügbarer physischer Arbeitsspeicher in Bytes.
pub fn system_memory() -> (u64, u64) {
    let info = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let kb = |key: &str| -> u64 {
        info.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.split_whitespace().next()?.parse().ok())
            .unwrap_or(0)
    };
    (kb("MemTotal:") * 1024, kb("MemAvailable:") * 1024)
}

/// Anzahl logischer Prozessoren (höchstens 64).
pub fn logical_cores() -> u32 {
    let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
    (n.max(1) as u32).min(64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// nice-Werte und Stufen passen zueinander.
    #[test]
    fn nice_roundtrip() {
        for p in Priority::ALL {
            assert_eq!(priority_of(nice_of(p)), p);
        }
    }

    /// Nur private, beschreibbare, anonyme Bereiche werden ausgelagert.
    #[test]
    fn maps_filter() {
        assert!(private_writable_range("7f00-7f10 rw-p 00000000 00:00 0").is_some());
        assert!(private_writable_range("5600-5700 rw-p 00000000 00:00 0   [heap]").is_some());
        assert!(private_writable_range("7f00-7f10 r-xp 00000000 08:01 123 /usr/lib/libc.so").is_none());
        assert!(private_writable_range("7f00-7f10 rw-s 00000000 00:05 9 /dev/shm/x").is_none());
        assert!(private_writable_range("7f00-7f10 rw-p 00000000 08:01 123 /usr/lib/libc.so").is_none());
    }

    /// Priorität, Affinität und Auslagern an einem echten Kindprozess (nur mit CAP_SYS_NICE).
    #[test]
    fn child_process_controls() {
        if !has_cap_sys_nice() {
            return;
        }
        let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        // Warten, bis `sleep` wirklich läuft (vorher gehört der Speicher noch dem Testprozess).
        std::thread::sleep(std::time::Duration::from_millis(150));
        let p = Proc::open(child.id()).unwrap();
        assert!(p.set_priority(Priority::BelowNormal));
        assert_eq!(p.priority(), Some(Priority::BelowNormal));
        assert!(p.set_priority(Priority::Normal));
        assert_eq!(p.priority(), Some(Priority::Normal));
        assert!(p.set_affinity(1));
        assert_eq!(p.affinity().unwrap().0, 1);
        assert!(p.set_affinity(system_mask()));
        assert!(p.trim());
        child.kill().unwrap();
        let _ = child.wait();
    }

    /// Der eigene Prozess liefert sinnvolle Werte.
    #[test]
    fn own_process() {
        let p = Proc::open(std::process::id()).unwrap();
        assert!(p.cpu_time().is_some());
        assert!(p.memory().unwrap().working_set > 0);
        assert!(p.affinity().unwrap().0 != 0);
        assert!(p.priority().is_some());
        let (total, avail) = system_memory();
        assert!(total > 0 && avail <= total);
    }
}
