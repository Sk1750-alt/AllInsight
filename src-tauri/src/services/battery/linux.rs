//! Battery status on Linux, from the kernel's power-supply class.
//!
//! `/sys/class/power_supply` is world-readable and is what UPower itself
//! reads, so no daemon or privilege is involved. Drivers report either energy
//! (µWh) or charge (µAh) figures; charge figures are converted to energy with
//! the design voltage, and only when that voltage is actually reported.

use std::collections::HashMap;
use std::path::Path;

use super::{BatteryStatus, PowerSource};
use crate::error::Result;

const POWER_SUPPLY: &str = "/sys/class/power_supply";

/// Every readable attribute of one power-supply directory, trimmed.
fn read_attributes(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(value) = std::fs::read_to_string(&path) {
            out.insert(
                entry.file_name().to_string_lossy().into_owned(),
                value.trim().to_string(),
            );
        }
    }
    out
}

fn number(attrs: &HashMap<String, String>, key: &str) -> Option<u64> {
    attrs.get(key)?.parse().ok()
}

/// Build a status from one battery's attributes and whether mains power is
/// online. Pure, so it is tested against attribute sets from real laptops.
pub(super) fn from_attributes(
    attrs: &HashMap<String, String>,
    mains_online: Option<bool>,
) -> BatteryStatus {
    let mut out = BatteryStatus {
        present: attrs.get("present").map(|p| p == "1").unwrap_or(true),
        ..Default::default()
    };

    let status = attrs.get("status").map(String::as_str).unwrap_or("Unknown");
    out.charging = status == "Charging";
    out.power_source = match (mains_online, status) {
        (Some(true), _) | (_, "Charging" | "Full" | "Not charging") => PowerSource::AcPower,
        (Some(false), _) | (_, "Discharging") => PowerSource::Battery,
        _ => PowerSource::Unknown,
    };
    out.charge_percent = number(attrs, "capacity").map(|c| c.min(100) as u8);

    // µV, µWh and µAh throughout; the struct wants mV and mWh.
    let design_voltage = number(attrs, "voltage_min_design").filter(|v| *v > 0);
    out.voltage_mv = number(attrs, "voltage_now").map(|v| (v / 1000) as u32);
    let to_mwh = |energy_key: &str, charge_key: &str| -> Option<u32> {
        if let Some(e) = number(attrs, energy_key) {
            return Some((e / 1000) as u32);
        }
        let charge = number(attrs, charge_key)?;
        let volts = design_voltage?;
        // µAh * µV = 1e-12 Wh; divide by 1e9 for mWh.
        Some((charge.saturating_mul(volts) / 1_000_000_000) as u32)
    };
    out.design_capacity_mwh = to_mwh("energy_full_design", "charge_full_design").filter(|v| *v > 0);
    out.full_charge_capacity_mwh = to_mwh("energy_full", "charge_full").filter(|v| *v > 0);
    out.remaining_capacity_mwh = to_mwh("energy_now", "charge_now");

    out.cycle_count = number(attrs, "cycle_count")
        .filter(|c| *c > 0)
        .map(|c| c as u32);
    out.chemistry = attrs.get("technology").filter(|t| *t != "Unknown").cloned();
    out.manufacturer = attrs.get("manufacturer").filter(|m| !m.is_empty()).cloned();

    if let (Some(design), Some(full)) = (out.design_capacity_mwh, out.full_charge_capacity_mwh) {
        out.health_percent = Some(((full as f64 / design as f64) * 100.0).round().min(100.0) as u8);
    }

    // Runtime from the present draw, only while discharging.
    if status == "Discharging" {
        let draw_mw = number(attrs, "power_now").map(|p| p / 1000).or_else(|| {
            let current = number(attrs, "current_now")?;
            let volts = number(attrs, "voltage_now")?;
            Some(current.saturating_mul(volts) / 1_000_000_000)
        });
        if let (Some(draw), Some(remaining)) =
            (draw_mw.filter(|d| *d > 0), out.remaining_capacity_mwh)
        {
            out.runtime_seconds = Some(((remaining as u64 * 3600) / draw) as u32);
        }
    }

    if out.health_percent.is_none() {
        out.notes.push(
            "The battery did not report both its design and full-charge capacity, so its health cannot be calculated."
                .into(),
        );
    }
    if out.cycle_count.is_none() {
        out.notes
            .push("The battery did not report a cycle count.".into());
    }
    if out.notes.is_empty() {
        out.notes
            .push("Read from the kernel's battery driver.".into());
    }
    out
}

pub fn status() -> Result<BatteryStatus> {
    let Ok(entries) = std::fs::read_dir(POWER_SUPPLY) else {
        return Ok(BatteryStatus {
            notes: vec!["No battery information is available on this system.".into()],
            ..BatteryStatus::default()
        });
    };

    let mut battery = None;
    let mut mains_online = None;
    for entry in entries.flatten() {
        let attrs = read_attributes(&entry.path());
        match attrs.get("type").map(String::as_str) {
            // `scope=Device` marks a wireless mouse or keyboard battery, which
            // is not the one the user means.
            Some("Battery") if attrs.get("scope").map(|s| s != "Device").unwrap_or(true) => {
                battery.get_or_insert(attrs);
            }
            Some("Mains") => {
                let online = attrs.get("online").map(|o| o == "1");
                mains_online = Some(mains_online.unwrap_or(false) || online.unwrap_or(false));
            }
            _ => {}
        }
    }

    match battery {
        Some(attrs) => Ok(from_attributes(&attrs, mains_online)),
        None => Ok(BatteryStatus {
            power_source: if mains_online == Some(true) {
                PowerSource::AcPower
            } else {
                PowerSource::Unknown
            },
            notes: vec!["No battery was found. This looks like a desktop computer.".into()],
            ..BatteryStatus::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn an_energy_reporting_battery_is_read() {
        let a = attrs(&[
            ("type", "Battery"),
            ("status", "Discharging"),
            ("present", "1"),
            ("capacity", "64"),
            ("energy_full_design", "57000000"),
            ("energy_full", "51300000"),
            ("energy_now", "32832000"),
            ("power_now", "8208000"),
            ("cycle_count", "212"),
            ("technology", "Li-ion"),
            ("manufacturer", "SMP"),
        ]);
        let s = from_attributes(&a, Some(false));
        assert!(s.present && !s.charging);
        assert_eq!(s.power_source, PowerSource::Battery);
        assert_eq!(s.charge_percent, Some(64));
        assert_eq!(s.design_capacity_mwh, Some(57_000));
        assert_eq!(s.health_percent, Some(90));
        assert_eq!(s.runtime_seconds, Some(4 * 3600));
        assert_eq!(s.cycle_count, Some(212));
    }

    #[test]
    fn a_charge_reporting_battery_is_converted_with_its_design_voltage() {
        let a = attrs(&[
            ("status", "Charging"),
            ("capacity", "80"),
            ("voltage_min_design", "11400000"),
            ("charge_full_design", "4000000"),
            ("charge_full", "3000000"),
            ("charge_now", "2400000"),
        ]);
        let s = from_attributes(&a, None);
        assert!(s.charging);
        assert_eq!(s.power_source, PowerSource::AcPower);
        assert_eq!(s.design_capacity_mwh, Some(45_600));
        assert_eq!(s.health_percent, Some(75));
        assert!(s.runtime_seconds.is_none());
    }

    #[test]
    fn missing_capacities_mean_no_health_figure() {
        let s = from_attributes(
            &attrs(&[("status", "Full"), ("capacity", "100")]),
            Some(true),
        );
        assert!(s.health_percent.is_none());
        assert!(!s.notes.is_empty());
    }
}
