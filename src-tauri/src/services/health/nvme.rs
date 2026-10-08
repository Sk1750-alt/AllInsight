//! Reading a drive's own health log without administrator permission.
//!
//! `MSFT_StorageReliabilityCounter` is only readable by an elevated process on
//! most machines, which left every drive at "Unknown" for a standard user.
//! Windows will, however, answer `IOCTL_STORAGE_QUERY_PROPERTY` on a disk
//! handle opened with no access rights at all, and two of its queries carry
//! what matters:
//!
//! - the NVMe SMART / Health Information log (log page 2): wear, temperature,
//!   power-on hours, media errors and the critical-warning flags
//! - the device temperature property, which SATA drives also answer
//!
//! A handle opened with zero access can read properties but cannot read or
//! write a single sector, so this adds no capability beyond reporting.

/// The fields of the NVMe health log AllInsight uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NvmeHealth {
    pub critical_warning: u8,
    pub temperature_celsius: Option<i32>,
    pub available_spare_percent: u8,
    pub percentage_used: u8,
    pub power_on_hours: u64,
    pub media_errors: u64,
    pub unsafe_shutdowns: u64,
}

fn le_u64(bytes: &[u8], at: usize) -> u64 {
    // The log stores 128-bit counters; the low 64 bits are more than enough.
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(buf)
}

/// Parse the 512-byte NVMe SMART / Health Information log page.
pub fn parse_health_log(log: &[u8]) -> Option<NvmeHealth> {
    if log.len() < 192 {
        return None;
    }
    let kelvin = u16::from_le_bytes([log[1], log[2]]);
    Some(NvmeHealth {
        critical_warning: log[0],
        temperature_celsius: (kelvin > 0).then(|| i32::from(kelvin) - 273),
        available_spare_percent: log[3],
        percentage_used: log[5],
        power_on_hours: le_u64(log, 128),
        unsafe_shutdowns: le_u64(log, 144),
        media_errors: le_u64(log, 160),
    })
}

/// The critical-warning bits, in words.
pub fn warning_labels(bits: u8) -> Vec<String> {
    let mut out = Vec::new();
    if bits & 0x01 != 0 {
        out.push("Predictive failure".to_string()); // spare below threshold
    }
    if bits & 0x02 != 0 {
        out.push("Stressed".to_string()); // temperature out of range
    }
    if bits & 0x04 != 0 {
        out.push("Predictive failure".to_string()); // reliability degraded
    }
    if bits & 0x08 != 0 {
        out.push("Predictive failure".to_string()); // read-only mode
    }
    if bits & 0x10 != 0 {
        out.push("Degraded".to_string()); // volatile backup failed
    }
    out.dedup();
    out
}

#[cfg(windows)]
mod ioctl {
    use super::{parse_health_log, NvmeHealth};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        NVMeDataTypeLogPage, PropertyStandardQuery, ProtocolTypeNvme,
        StorageDeviceProtocolSpecificProperty, StorageDeviceTemperatureProperty,
        IOCTL_STORAGE_QUERY_PROPERTY,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// `NVME_LOG_PAGE_HEALTH_INFO`.
    const HEALTH_LOG_PAGE: u32 = 2;
    const LOG_LEN: usize = 512;
    /// `sizeof(STORAGE_PROTOCOL_SPECIFIC_DATA)`.
    const PROTOCOL_DATA_LEN: usize = 40;

    struct Disk(HANDLE);

    impl Drop for Disk {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn open(disk: u32) -> Option<Disk> {
        let name: Vec<u16> = format!("\\\\.\\PhysicalDrive{disk}")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // Zero desired access: properties can be queried, sectors cannot be
        // read or written, and no elevation is needed.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (handle != INVALID_HANDLE_VALUE && !handle.is_null()).then_some(Disk(handle))
    }

    fn query(disk: &Disk, input: &mut [u8], output: &mut [u8]) -> Option<usize> {
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                disk.0,
                IOCTL_STORAGE_QUERY_PROPERTY,
                input.as_mut_ptr().cast(),
                input.len() as u32,
                output.as_mut_ptr().cast(),
                output.len() as u32,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(returned as usize)
    }

    fn put_u32(buf: &mut [u8], at: usize, v: u32) {
        buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn get_u32(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
    }

    /// The NVMe health log of physical disk `disk`.
    pub fn nvme_health(disk: u32) -> Option<NvmeHealth> {
        let handle = open(disk)?;
        // STORAGE_PROPERTY_QUERY { PropertyId, QueryType } followed by
        // STORAGE_PROTOCOL_SPECIFIC_DATA and room for the log itself.
        let mut buf = vec![0u8; 8 + PROTOCOL_DATA_LEN + LOG_LEN];
        put_u32(&mut buf, 0, StorageDeviceProtocolSpecificProperty as u32);
        put_u32(&mut buf, 4, PropertyStandardQuery as u32);
        let p = 8;
        put_u32(&mut buf, p, ProtocolTypeNvme as u32);
        put_u32(&mut buf, p + 4, NVMeDataTypeLogPage as u32);
        put_u32(&mut buf, p + 8, HEALTH_LOG_PAGE);
        put_u32(&mut buf, p + 12, 0);
        put_u32(&mut buf, p + 16, PROTOCOL_DATA_LEN as u32);
        put_u32(&mut buf, p + 20, LOG_LEN as u32);

        let mut out = buf.clone();
        let returned = query(&handle, &mut buf, &mut out)?;
        // STORAGE_PROTOCOL_DATA_DESCRIPTOR { Version, Size, ProtocolSpecificData }.
        let offset = get_u32(&out, 8 + 16) as usize;
        let length = get_u32(&out, 8 + 20) as usize;
        let start = 8 + offset;
        if length < 192 || start + length > out.len() || start + length > returned {
            return None;
        }
        parse_health_log(&out[start..start + length])
    }

    /// The drive's own temperature sensor, which SATA drives also report.
    pub fn temperature(disk: u32) -> Option<i32> {
        let handle = open(disk)?;
        let mut input = vec![0u8; 12];
        put_u32(&mut input, 0, StorageDeviceTemperatureProperty as u32);
        put_u32(&mut input, 4, PropertyStandardQuery as u32);
        let mut out = vec![0u8; 256];
        let returned = query(&handle, &mut input, &mut out)?;
        // STORAGE_TEMPERATURE_DATA_DESCRIPTOR: InfoCount at 12, then the
        // first STORAGE_TEMPERATURE_INFO at 24 with Temperature at +2.
        if returned < 28 || u16::from_le_bytes([out[12], out[13]]) == 0 {
            return None;
        }
        let celsius = i16::from_le_bytes([out[26], out[27]]);
        (celsius > 0 && celsius < 150).then_some(i32::from(celsius))
    }
}

#[cfg(windows)]
pub use ioctl::{nvme_health, temperature};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_health_log_is_parsed() {
        let mut log = vec![0u8; 512];
        log[0] = 0x04;
        log[1..3].copy_from_slice(&310u16.to_le_bytes());
        log[3] = 100;
        log[5] = 7;
        log[128..136].copy_from_slice(&4321u64.to_le_bytes());
        log[144..152].copy_from_slice(&12u64.to_le_bytes());
        log[160..168].copy_from_slice(&0u64.to_le_bytes());
        let h = parse_health_log(&log).unwrap();
        assert_eq!(h.temperature_celsius, Some(37));
        assert_eq!(h.percentage_used, 7);
        assert_eq!(h.power_on_hours, 4321);
        assert_eq!(h.unsafe_shutdowns, 12);
        assert_eq!(
            warning_labels(h.critical_warning),
            vec!["Predictive failure"]
        );
        assert!(parse_health_log(&log[..100]).is_none());
    }
}
