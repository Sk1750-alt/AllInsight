//! Processor load the way Task Manager measures it.
//!
//! Since Windows 8, Task Manager's CPU figure is `% Processor Utility`, not
//! `% Processor Time`. Utility scales busy time by how fast the cores were
//! actually running, so a laptop boosting to 4.8 GHz on a 2.4 GHz base clock
//! reads far higher than raw time does. sysinfo reports time, which is why
//! AllInsight's number used to sit well below Task Manager's. The same
//! counter set also gives the live clock speed, where sysinfo only knows the
//! base frequency.
//!
//! Utility can exceed 100 on a boosting core; Task Manager clamps it, and so
//! does this.

use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
};

/// `ERROR_SUCCESS`.
const OK: u32 = 0;

pub struct ProcessorCounters {
    query: isize,
    utility: isize,
    performance: isize,
    frequency: isize,
    /// Rate counters need two collections before they mean anything.
    primed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ProcessorReading {
    pub utility_percent: f32,
    pub current_mhz: Option<u64>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl ProcessorCounters {
    pub fn open() -> Option<Self> {
        let mut query = 0isize;
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != OK {
            return None;
        }
        let add = |path: &str| -> Option<isize> {
            let mut counter = 0isize;
            let path = wide(path);
            (unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) } == OK)
                .then_some(counter)
        };
        let utility = add(r"\Processor Information(_Total)\% Processor Utility");
        let performance = add(r"\Processor Information(_Total)\% Processor Performance");
        let frequency = add(r"\Processor Information(_Total)\Processor Frequency");
        let Some(utility) = utility else {
            unsafe { PdhCloseQuery(query) };
            return None;
        };
        unsafe { PdhCollectQueryData(query) };
        Some(Self {
            query,
            utility,
            performance: performance.unwrap_or(0),
            frequency: frequency.unwrap_or(0),
            primed: false,
        })
    }

    fn value(counter: isize) -> Option<f64> {
        if counter == 0 {
            return None;
        }
        let mut value: PDH_FMT_COUNTERVALUE = unsafe { std::mem::zeroed() };
        let status = unsafe {
            PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, std::ptr::null_mut(), &mut value)
        };
        (status == OK && value.CStatus == OK).then(|| unsafe { value.Anonymous.doubleValue })
    }

    /// Collect and read. `None` until the counters have two samples to
    /// compare, and whenever Windows declines to answer.
    pub fn read(&mut self) -> Option<ProcessorReading> {
        if unsafe { PdhCollectQueryData(self.query) } != OK {
            return None;
        }
        if !self.primed {
            self.primed = true;
            return None;
        }
        let utility = Self::value(self.utility)?;
        let current_mhz = match (Self::value(self.frequency), Self::value(self.performance)) {
            (Some(base), Some(perf)) if base > 0.0 && perf > 0.0 => {
                Some((base * perf / 100.0).round() as u64)
            }
            _ => None,
        };
        Some(ProcessorReading {
            utility_percent: utility.clamp(0.0, 100.0) as f32,
            current_mhz,
        })
    }
}

impl Drop for ProcessorCounters {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}
