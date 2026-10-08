//! Battery status and health.
//!
//! Charge level and charging state come from `GetSystemPowerStatus`, which is
//! always available. Design capacity, full-charge capacity and cycle count come
//! from the ACPI battery classes in the `root\wmi` namespace, which many
//! laptops publish and some do not. Health is only reported when both
//! capacities are known: a percentage derived from one of them would be a
//! guess dressed up as a measurement.

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSource {
    Battery,
    AcPower,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    /// False on a desktop, or when Windows reports no battery.
    pub present: bool,
    pub power_source: PowerSource,
    pub charging: bool,
    /// 0-100, or `None` when Windows reports the level as unknown.
    pub charge_percent: Option<u8>,
    /// Seconds of runtime left, when Windows can estimate it.
    pub runtime_seconds: Option<u32>,
    pub design_capacity_mwh: Option<u32>,
    pub full_charge_capacity_mwh: Option<u32>,
    pub remaining_capacity_mwh: Option<u32>,
    pub cycle_count: Option<u32>,
    pub voltage_mv: Option<u32>,
    pub chemistry: Option<String>,
    pub manufacturer: Option<String>,
    /// Full-charge capacity as a share of design capacity. Present only when
    /// both figures were read from the hardware.
    pub health_percent: Option<u8>,
    pub notes: Vec<String>,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self {
            present: false,
            power_source: PowerSource::Unknown,
            charging: false,
            charge_percent: None,
            runtime_seconds: None,
            design_capacity_mwh: None,
            full_charge_capacity_mwh: None,
            remaining_capacity_mwh: None,
            cycle_count: None,
            voltage_mv: None,
            chemistry: None,
            manufacturer: None,
            health_percent: None,
            notes: Vec::new(),
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use serde::Deserialize as De;
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use wmi::{COMLibrary, WMIConnection};

    #[derive(De, Debug)]
    #[serde(rename = "BatteryStaticData")]
    #[serde(rename_all = "PascalCase")]
    struct StaticData {
        designed_capacity: Option<u32>,
        chemistry: Option<String>,
        manufacture_name: Option<String>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "BatteryFullChargedCapacity")]
    #[serde(rename_all = "PascalCase")]
    struct FullCharged {
        full_charged_capacity: Option<u32>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "BatteryCycleCount")]
    #[serde(rename_all = "PascalCase")]
    struct CycleCount {
        cycle_count: Option<u32>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "BatteryStatus")]
    #[serde(rename_all = "PascalCase")]
    struct RuntimeStatus {
        remaining_capacity: Option<u32>,
        voltage: Option<u32>,
        charging: Option<bool>,
    }

    pub fn status() -> Result<BatteryStatus> {
        let mut out = BatteryStatus::default();
        let mut notes = Vec::new();

        // Layer one: always available.
        let mut power = SYSTEM_POWER_STATUS {
            ACLineStatus: 255,
            BatteryFlag: 255,
            BatteryLifePercent: 255,
            SystemStatusFlag: 0,
            BatteryLifeTime: u32::MAX,
            BatteryFullLifeTime: u32::MAX,
        };
        let ok = unsafe { GetSystemPowerStatus(&mut power) } != 0;
        if ok {
            // BatteryFlag bit 7 (128) means "no system battery".
            out.present = power.BatteryFlag != 128 && power.BatteryFlag != 255;
            out.power_source = match power.ACLineStatus {
                0 => PowerSource::Battery,
                1 => PowerSource::AcPower,
                _ => PowerSource::Unknown,
            };
            // Bit 3 (8) means charging.
            out.charging = power.BatteryFlag != 255 && power.BatteryFlag & 8 != 0;
            if power.BatteryLifePercent <= 100 {
                out.charge_percent = Some(power.BatteryLifePercent);
            }
            if power.BatteryLifeTime != u32::MAX {
                out.runtime_seconds = Some(power.BatteryLifeTime);
            }
        } else {
            notes.push("Windows did not report power status for this device.".to_string());
        }

        if !out.present {
            notes.push("No battery is installed in this device.".to_string());
            out.notes = notes;
            return Ok(out);
        }

        // Layer two: the ACPI battery classes. Optional by design.
        let detail = (|| -> Option<()> {
            let com = COMLibrary::new().ok()?;
            let wmi = WMIConnection::with_namespace_path("root\\wmi", com).ok()?;

            if let Ok(rows) = wmi.query::<StaticData>() {
                if let Some(first) = rows.into_iter().next() {
                    out.design_capacity_mwh = first.designed_capacity.filter(|v| *v > 0);
                    out.chemistry = first.chemistry.map(|c| c.trim().to_string());
                    out.manufacturer = first.manufacture_name.map(|c| c.trim().to_string());
                }
            }
            if let Ok(rows) = wmi.query::<FullCharged>() {
                if let Some(first) = rows.into_iter().next() {
                    out.full_charge_capacity_mwh = first.full_charged_capacity.filter(|v| *v > 0);
                }
            }
            if let Ok(rows) = wmi.query::<CycleCount>() {
                if let Some(first) = rows.into_iter().next() {
                    out.cycle_count = first.cycle_count.filter(|v| *v > 0);
                }
            }
            if let Ok(rows) = wmi.query::<RuntimeStatus>() {
                if let Some(first) = rows.into_iter().next() {
                    out.remaining_capacity_mwh = first.remaining_capacity.filter(|v| *v > 0);
                    out.voltage_mv = first.voltage.filter(|v| *v > 0);
                    if let Some(charging) = first.charging {
                        out.charging = charging;
                    }
                }
            }
            Some(())
        })();

        if detail.is_none() {
            notes.push("Detailed battery data could not be read on this device.".to_string());
        }

        match (out.design_capacity_mwh, out.full_charge_capacity_mwh) {
            (Some(design), Some(full)) if design > 0 => {
                let ratio = (full as f64 / design as f64 * 100.0).round();
                let percent = ratio.clamp(0.0, 100.0) as u8;
                out.health_percent = Some(percent);
                notes.push(format!(
                    "This battery currently holds approximately {percent}% of its original design capacity."
                ));
                if percent < 60 {
                    notes.push("Capacity has fallen well below the original design. Replacement is worth considering.".to_string());
                }
            }
            _ => {
                notes.push(
                    "This device does not report design capacity, so battery health cannot be calculated."
                        .to_string(),
                );
            }
        }

        out.notes = notes;
        Ok(out)
    }
}

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::*;
    pub fn status() -> Result<BatteryStatus> {
        let mut out = BatteryStatus::default();
        out.notes = vec!["No battery information is available on this system yet.".into()];
        Ok(out)
    }
}

/// Gather battery status. The ACPI battery classes are read through WMI, so
/// this runs on its own thread for the same reason drive health does.
pub fn status() -> Result<BatteryStatus> {
    match crate::services::wmi_thread::run("battery", imp::status) {
        Ok(result) => result,
        Err(message) => Ok(BatteryStatus {
            notes: vec![message],
            ..BatteryStatus::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_battery_report_never_invents_health() {
        let status = status().expect("battery query must not fail");
        if status.design_capacity_mwh.is_none() || status.full_charge_capacity_mwh.is_none() {
            assert!(
                status.health_percent.is_none(),
                "health must not be reported without both capacities"
            );
        }
        if let Some(p) = status.charge_percent {
            assert!(p <= 100);
        }
        assert!(
            !status.notes.is_empty(),
            "the user always gets an explanation"
        );
    }

    #[test]
    fn a_device_without_a_battery_says_so_plainly() {
        let status = status().unwrap();
        if !status.present {
            assert!(status
                .notes
                .iter()
                .any(|n| n.contains("No battery") || n.contains("did not report")));
            assert!(status.health_percent.is_none());
        }
    }
}
