//! What the appliance can say about the machine it runs on.
//!
//! Read from `/proc` and `/sys` rather than through a crate: these files are
//! the interface Linux offers, and the appliance already has to be small.
//! Every field is optional, because the same binary is developed on a laptop
//! where none of them exist — an absent value is reported as absent rather
//! than invented.
//!
//! Parsing is separated from reading so it can be tested without the files.

use std::fs;

use serde::Serialize;

/// What the machine reports about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SystemReport {
    /// Board model, as the firmware names it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Operating system, as it names itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Kernel release.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel: Option<String>,
    /// Seconds since boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_s: Option<u64>,
    /// Load average over the last minute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_1m: Option<f32>,
    /// Total usable memory, in kibibytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_total_kb: Option<u64>,
    /// Memory available without swapping, in kibibytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_available_kb: Option<u64>,
    /// CPU temperature in degrees Celsius.
    ///
    /// The one figure that explains a Pi quietly slowing down: it throttles
    /// long before it stops.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<f32>,
}

impl SystemReport {
    /// Reads what this machine will say. Absent files leave absent fields.
    #[must_use]
    pub fn read() -> Self {
        Self {
            model: read("/proc/device-tree/model").map(|text| parse_model(&text)),
            os: read("/etc/os-release").and_then(|text| parse_os_release(&text)),
            kernel: read("/proc/sys/kernel/osrelease").map(|text| text.trim().to_owned()),
            uptime_s: read("/proc/uptime").and_then(|text| parse_uptime(&text)),
            load_1m: read("/proc/loadavg").and_then(|text| parse_load(&text)),
            memory_total_kb: read("/proc/meminfo")
                .and_then(|text| parse_meminfo(&text, "MemTotal")),
            memory_available_kb: read("/proc/meminfo")
                .and_then(|text| parse_meminfo(&text, "MemAvailable")),
            temperature_c: read("/sys/class/thermal/thermal_zone0/temp")
                .and_then(|text| parse_temperature(&text)),
        }
    }
}

fn read(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Device-tree strings are NUL-terminated, which would otherwise be displayed.
///
/// Trimmed together rather than in sequence: the terminator and any newline
/// arrive in either order, and stripping one first leaves the other stranded.
fn parse_model(text: &str) -> String {
    text.trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_owned()
}

fn parse_os_release(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|value| value.trim_matches('"').to_owned())
}

fn parse_uptime(text: &str) -> Option<u64> {
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(seconds.max(0.0) as u64)
}

fn parse_load(text: &str) -> Option<f32> {
    text.split_whitespace().next()?.parse().ok()
}

fn parse_meminfo(text: &str, field: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
}

/// The thermal zone reports millidegrees.
fn parse_temperature(text: &str) -> Option<f32> {
    let millidegrees: f32 = text.trim().parse().ok()?;
    Some(millidegrees / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_names_itself_without_its_terminator() {
        // Device-tree strings carry a trailing NUL, which would otherwise be
        // shown to whoever reads the settings screen.
        assert_eq!(
            parse_model("Raspberry Pi 3 Model B Plus Rev 1.3\0"),
            "Raspberry Pi 3 Model B Plus Rev 1.3"
        );
        assert_eq!(
            parse_model("Raspberry Pi Zero 2 W Rev 1.0\0\n"),
            "Raspberry Pi Zero 2 W Rev 1.0"
        );
    }

    #[test]
    fn the_operating_system_is_taken_from_its_pretty_name() {
        let release = "PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nID=debian\n";
        assert_eq!(
            parse_os_release(release).as_deref(),
            Some("Debian GNU/Linux 12 (bookworm)")
        );
    }

    #[test]
    fn an_os_release_without_a_pretty_name_reports_nothing() {
        assert!(parse_os_release("ID=debian\n").is_none());
    }

    #[test]
    fn uptime_keeps_only_whole_seconds() {
        assert_eq!(parse_uptime("128472.34 501234.11\n"), Some(128_472));
    }

    #[test]
    fn the_first_load_average_is_the_recent_one() {
        assert_eq!(parse_load("0.52 0.31 0.24 1/210 4821\n"), Some(0.52));
    }

    #[test]
    fn memory_fields_are_found_by_name_not_by_position() {
        // MemFree sits between the two fields wanted here, and a prefix match
        // alone would take MemTotal for MemAvailable on some kernels.
        let meminfo =
            "MemTotal:         444444 kB\nMemFree:  120000 kB\nMemAvailable:  310000 kB\n";
        assert_eq!(parse_meminfo(meminfo, "MemTotal"), Some(444_444));
        assert_eq!(parse_meminfo(meminfo, "MemAvailable"), Some(310_000));
        assert_eq!(parse_meminfo(meminfo, "Swap"), None);
    }

    #[test]
    fn the_thermal_zone_is_read_in_degrees() {
        assert_eq!(parse_temperature("54321\n"), Some(54.321));
    }

    #[test]
    fn a_machine_that_says_nothing_reports_nothing() {
        // The same binary is developed on a laptop where none of these files
        // exist; absent has to be a value rather than a failure.
        assert!(parse_uptime("").is_none());
        assert!(parse_load("").is_none());
        assert!(parse_temperature("not a number").is_none());
        assert!(parse_meminfo("", "MemTotal").is_none());
    }
}
