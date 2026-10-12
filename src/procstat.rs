//! Prozess-Werkzeuge: CPU-Zeit, Arbeitsspeicher, Priorität, Kern-Affinität,
//! Arbeitsspeicher freigeben sowie systemweite CPU-/RAM-Werte.

use std::time::Instant;

use evemb::config::Priority;
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO};
use windows_sys::Win32::System::Threading::*;

pub use crate::engine_api::MemInfo;

/// Besitzendes Prozess-Handle, wird beim Drop geschlossen.
pub struct ProcHandle(HANDLE);

// SAFETY: Ein Prozess-Handle ist ein Kernel-Objekt-Verweis und darf threadübergreifend genutzt werden.
unsafe impl Send for ProcHandle {}

impl Drop for ProcHandle {
    /// Schließt das Handle.
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Wandelt eine FILETIME in 100-ns-Einheiten um.
fn ft(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

impl ProcHandle {
    /// Öffnet einen Prozess mit allen benötigten Rechten; klappt das nicht
    /// (z. B. EVE läuft als Admin), wenigstens nur lesend.
    pub fn open(pid: u32) -> Option<Self> {
        let full = PROCESS_QUERY_INFORMATION
            | PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_SET_INFORMATION
            | PROCESS_SET_QUOTA
            | PROCESS_VM_READ;
        for rights in [full, PROCESS_QUERY_LIMITED_INFORMATION] {
            let h = unsafe { OpenProcess(rights, 0, pid) };
            if !h.is_null() {
                return Some(Self(h));
            }
        }
        None
    }

    /// Verbrauchte CPU-Zeit (Kernel + User) in 100-ns-Einheiten.
    pub fn cpu_time(&self) -> Option<u64> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
        let ok = unsafe { GetProcessTimes(self.0, &mut c, &mut e, &mut k, &mut u) };
        (ok != 0).then(|| ft(k) + ft(u))
    }

    /// Working Set und privater Speicher.
    pub fn memory(&self) -> Option<MemInfo> {
        let mut pmc: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = unsafe {
            K32GetProcessMemoryInfo(
                self.0,
                &mut pmc as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
                pmc.cb,
            )
        };
        (ok != 0).then_some(MemInfo {
            working_set: pmc.WorkingSetSize as u64,
            private: pmc.PrivateUsage as u64,
        })
    }

    /// Aktuelle Prioritätsklasse (Echtzeit wird als „Hoch“ gemeldet).
    pub fn priority(&self) -> Option<Priority> {
        let class = unsafe { GetPriorityClass(self.0) };
        Some(match class {
            IDLE_PRIORITY_CLASS => Priority::Idle,
            BELOW_NORMAL_PRIORITY_CLASS => Priority::BelowNormal,
            NORMAL_PRIORITY_CLASS => Priority::Normal,
            ABOVE_NORMAL_PRIORITY_CLASS => Priority::AboveNormal,
            HIGH_PRIORITY_CLASS | REALTIME_PRIORITY_CLASS => Priority::High,
            _ => return None,
        })
    }

    /// Setzt die Prioritätsklasse.
    pub fn set_priority(&self, p: Priority) -> bool {
        let class = match p {
            Priority::Idle => IDLE_PRIORITY_CLASS,
            Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            Priority::Normal => NORMAL_PRIORITY_CLASS,
            Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            Priority::High => HIGH_PRIORITY_CLASS,
        };
        unsafe { SetPriorityClass(self.0, class) != 0 }
    }

    /// Affinitätsmaske des Prozesses und die des Systems.
    pub fn affinity(&self) -> Option<(u64, u64)> {
        let (mut proc_mask, mut sys_mask) = (0usize, 0usize);
        let ok = unsafe { GetProcessAffinityMask(self.0, &mut proc_mask, &mut sys_mask) };
        (ok != 0).then_some((proc_mask as u64, sys_mask as u64))
    }

    /// Setzt die Affinitätsmaske (muss eine Teilmenge der Systemmaske sein).
    pub fn set_affinity(&self, mask: u64) -> bool {
        mask != 0 && unsafe { SetProcessAffinityMask(self.0, mask as usize) != 0 }
    }

    /// Gibt den Arbeitsspeicher frei (Seiten wandern in die Standby-Liste/Auslagerung).
    pub fn trim(&self) -> bool {
        unsafe { K32EmptyWorkingSet(self.0) != 0 }
    }
}

/// Misst die CPU-Auslastung eines Prozesses über die Zeit zwischen zwei Messungen.
#[derive(Default)]
pub struct CpuMeter {
    last: Option<(u64, Instant)>,
}

impl CpuMeter {
    /// Neue Messung; liefert die Auslastung in Prozent der Gesamt-CPU (0–100).
    pub fn sample(&mut self, cpu_time: u64, cores: u32) -> f32 {
        let now = Instant::now();
        let pct = match self.last {
            Some((prev, at)) => {
                let wall = now.duration_since(at).as_nanos() as f64 / 100.0; // in 100 ns
                let used = cpu_time.saturating_sub(prev) as f64;
                if wall > 0.0 {
                    (used / (wall * cores.max(1) as f64) * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.last = Some((cpu_time, now));
        pct as f32
    }
}

/// Systemweite CPU-Auslastung aus GetSystemTimes.
#[derive(Default)]
pub struct SystemCpu {
    last: Option<(u64, u64, u64)>,
}

impl SystemCpu {
    /// Neue Messung; liefert die Gesamtauslastung in Prozent.
    pub fn sample(&mut self) -> f32 {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut idle, mut kernel, mut user) = (zero, zero, zero);
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return 0.0;
        }
        let now = (ft(idle), ft(kernel), ft(user));
        let pct = match self.last {
            Some((i0, k0, u0)) => {
                let idle_d = now.0.saturating_sub(i0) as f64;
                // Kernelzeit enthält die Leerlaufzeit bereits.
                let total = (now.1.saturating_sub(k0) + now.2.saturating_sub(u0)) as f64;
                if total > 0.0 {
                    ((total - idle_d) / total * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        self.last = Some(now);
        pct as f32
    }
}

/// Gesamter und verfügbarer physischer Arbeitsspeicher in Bytes.
pub fn system_memory() -> (u64, u64) {
    let mut ms: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut ms) } == 0 {
        return (0, 0);
    }
    (ms.ullTotalPhys, ms.ullAvailPhys)
}

/// Anzahl logischer Prozessoren (höchstens 64 – eine Prozessorgruppe).
pub fn logical_cores() -> u32 {
    let mut si: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { GetSystemInfo(&mut si) };
    si.dwNumberOfProcessors.clamp(1, 64)
}
