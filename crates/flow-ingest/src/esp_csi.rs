//! Parser for the text frame format emitted by `esp-csi`-based nodes.
//!
//! Nodes based on the `espressif/esp-csi` examples emit one CSV line per
//! CSI measurement, starting with a `CSI_DATA` marker and ending with a
//! quoted array of interleaved I/Q integers. The column set depends on the
//! chip family:
//!
//! - **ESP32-C6 family** (C5/C6/C61), 15 columns:
//!   `type,seq,mac,rssi,rate,noise_floor,fft_gain,agc_gain,channel,`
//!   `local_timestamp,sig_len,rx_format,len,first_word,data`
//! - **Classic ESP32 variants**, 25 columns:
//!   `type,id,mac,rssi,rate,sig_mode,mcs,bandwidth,smoothing,not_sounding,`
//!   `aggregation,stbc,fec_coding,sgi,noise_floor,ampdu_cnt,channel,`
//!   `secondary_channel,local_timestamp,ant,sig_len,rx_format,len,`
//!   `first_word,data`
//!
//! The parser is transport-agnostic: lines may come from a serial capture,
//! a recorded file, or a UDP datagram. The layout is detected from the
//! column count.
//!
//! Per the ESP-IDF Wi-Fi driver documentation, each sub-carrier is encoded
//! as two signed integers, **imaginary part first, real part second**. On
//! the C6 family the first four data bytes may be invalid when the node
//! reports `first_word` as invalid (hardware limitation); the parser
//! preserves both the data and the flag and leaves the policy to
//! downstream consumers.

use core::fmt;
use core::str::FromStr;

use thiserror::Error;

/// A node MAC address (EUI-48).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddr(pub [u8; 6]);

impl FromStr for MacAddr {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, ParseError> {
        let mut bytes = [0u8; 6];
        let mut parts = s.split(':');
        for byte in &mut bytes {
            let part = parts.next().ok_or(ParseError::InvalidMac)?;
            if part.is_empty() || part.len() > 2 {
                return Err(ParseError::InvalidMac);
            }
            *byte = u8::from_str_radix(part, 16).map_err(|_| ParseError::InvalidMac)?;
        }
        if parts.next().is_some() {
            return Err(ParseError::InvalidMac);
        }
        Ok(MacAddr(bytes))
    }
}

impl fmt::Display for MacAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// Column layout variant of a `CSI_DATA` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineFormat {
    /// ESP32-C5/C6/C61 family layout (15 columns).
    Esp32C6Family,
    /// Layout of the other ESP32 variants (25 columns).
    Esp32Classic,
}

/// One CSI measurement parsed from a `CSI_DATA` line, before conversion to
/// the canonical [`flow_core::CsiFrame`].
///
/// Fields are kept exactly as reported by the node; nothing is derived or
/// trusted yet. Columns not listed here are validated as integers during
/// parsing but not retained (diagnostic values with no downstream
/// consumer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCsiFrame {
    /// Column layout the line was parsed as.
    pub format: LineFormat,
    /// Node-side sequence number of the measurement.
    pub seq: u32,
    /// MAC address of the transmitter of the sensed packet.
    pub mac: MacAddr,
    /// RSSI in dBm as reported by the node radio.
    pub rssi: i8,
    /// Raw PHY rate field as reported by the node.
    pub rate: u8,
    /// MCS index; only present in the classic layout.
    pub mcs: Option<u8>,
    /// Noise floor as reported by the node radio.
    pub noise_floor: i16,
    /// Primary Wi-Fi channel.
    pub channel: u8,
    /// Node-local reception timestamp in microseconds. Wraps around; kept
    /// for diagnostics only — canonical timestamps are assigned by the
    /// edge at reception.
    pub local_timestamp: u32,
    /// Length of the sensed packet, in bytes.
    pub sig_len: u32,
    /// Whether the node flagged the first four data bytes as invalid
    /// (C6-family hardware limitation).
    pub first_word_invalid: bool,
    /// Interleaved I/Q values, imaginary part first for each sub-carrier.
    pub data: Vec<i16>,
}

/// Failure to parse a `CSI_DATA` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseError {
    /// The line does not start with the `CSI_DATA` marker. Callers reading
    /// mixed output (e.g. a serial log) can skip such lines cheaply.
    #[error("not a CSI_DATA line")]
    NotCsiData,
    /// The line does not end with a quoted `"[...]"` data array.
    #[error("missing trailing data array")]
    MissingDataArray,
    /// The column count matches no known layout.
    #[error("unknown layout: {columns} columns")]
    UnknownLayout {
        /// Number of columns found (including the data array).
        columns: usize,
    },
    /// A column that should hold an integer does not.
    #[error("invalid integer in column `{column}`")]
    InvalidInteger {
        /// Name of the offending column.
        column: &'static str,
    },
    /// The MAC column is not a valid EUI-48 address.
    #[error("invalid MAC address")]
    InvalidMac,
    /// The data array holds an odd number of values (complete I/Q pairs
    /// are expected).
    #[error("odd I/Q value count: {count}")]
    OddIqCount {
        /// Number of values in the data array.
        count: usize,
    },
    /// The declared `len` does not match the data array length.
    #[error("declared len {declared} does not match {actual} data values")]
    LenMismatch {
        /// Value of the `len` column.
        declared: usize,
        /// Actual number of values in the data array.
        actual: usize,
    },
}

/// Parses one `CSI_DATA` line into a [`RawCsiFrame`].
///
/// The layout (C6 family or classic) is detected from the column count.
/// Leading/trailing whitespace and CR/LF are tolerated.
///
/// # Errors
///
/// See [`ParseError`]; in particular, lines that do not start with
/// `CSI_DATA` yield [`ParseError::NotCsiData`] so that interleaved log
/// output can be skipped cheaply.
pub fn parse_line(line: &str) -> Result<RawCsiFrame, ParseError> {
    let line = line.trim();
    if !line.starts_with("CSI_DATA,") {
        return Err(ParseError::NotCsiData);
    }
    let rest = line
        .strip_suffix("]\"")
        .ok_or(ParseError::MissingDataArray)?;
    let (meta, iq) = rest
        .split_once(",\"[")
        .ok_or(ParseError::MissingDataArray)?;
    let cols: Vec<&str> = meta.split(',').collect();
    let data = parse_iq(iq)?;
    match cols.len() {
        14 => parse_c6(&cols, data),
        24 => parse_classic(&cols, data),
        n => Err(ParseError::UnknownLayout { columns: n + 1 }),
    }
}

fn parse_iq(s: &str) -> Result<Vec<i16>, ParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|v| {
            v.trim()
                .parse::<i16>()
                .map_err(|_| ParseError::InvalidInteger { column: "data" })
        })
        .collect()
}

fn int<T: FromStr>(cols: &[&str], idx: usize, column: &'static str) -> Result<T, ParseError> {
    cols[idx]
        .trim()
        .parse::<T>()
        .map_err(|_| ParseError::InvalidInteger { column })
}

fn check_data(declared: usize, data: &[i16]) -> Result<(), ParseError> {
    if data.len() % 2 != 0 {
        return Err(ParseError::OddIqCount { count: data.len() });
    }
    if declared != data.len() {
        return Err(ParseError::LenMismatch {
            declared,
            actual: data.len(),
        });
    }
    Ok(())
}

fn parse_c6(cols: &[&str], data: Vec<i16>) -> Result<RawCsiFrame, ParseError> {
    let seq = int::<u32>(cols, 1, "seq")?;
    let mac: MacAddr = cols[2].trim().parse()?;
    let rssi = int::<i8>(cols, 3, "rssi")?;
    let rate = int::<u8>(cols, 4, "rate")?;
    let noise_floor = int::<i16>(cols, 5, "noise_floor")?;
    let _fft_gain = int::<i32>(cols, 6, "fft_gain")?;
    let _agc_gain = int::<i32>(cols, 7, "agc_gain")?;
    let channel = int::<u8>(cols, 8, "channel")?;
    let local_timestamp = int::<u32>(cols, 9, "local_timestamp")?;
    let sig_len = int::<u32>(cols, 10, "sig_len")?;
    let _rx_format = int::<i32>(cols, 11, "rx_format")?;
    let len = int::<usize>(cols, 12, "len")?;
    let first_word = int::<i32>(cols, 13, "first_word")?;
    check_data(len, &data)?;
    Ok(RawCsiFrame {
        format: LineFormat::Esp32C6Family,
        seq,
        mac,
        rssi,
        rate,
        mcs: None,
        noise_floor,
        channel,
        local_timestamp,
        sig_len,
        first_word_invalid: first_word != 0,
        data,
    })
}

fn parse_classic(cols: &[&str], data: Vec<i16>) -> Result<RawCsiFrame, ParseError> {
    let seq = int::<u32>(cols, 1, "id")?;
    let mac: MacAddr = cols[2].trim().parse()?;
    let rssi = int::<i8>(cols, 3, "rssi")?;
    let rate = int::<u8>(cols, 4, "rate")?;
    let _sig_mode = int::<i32>(cols, 5, "sig_mode")?;
    let mcs = int::<u8>(cols, 6, "mcs")?;
    let _bandwidth = int::<i32>(cols, 7, "bandwidth")?;
    let _smoothing = int::<i32>(cols, 8, "smoothing")?;
    let _not_sounding = int::<i32>(cols, 9, "not_sounding")?;
    let _aggregation = int::<i32>(cols, 10, "aggregation")?;
    let _stbc = int::<i32>(cols, 11, "stbc")?;
    let _fec_coding = int::<i32>(cols, 12, "fec_coding")?;
    let _sgi = int::<i32>(cols, 13, "sgi")?;
    let noise_floor = int::<i16>(cols, 14, "noise_floor")?;
    let _ampdu_cnt = int::<i32>(cols, 15, "ampdu_cnt")?;
    let channel = int::<u8>(cols, 16, "channel")?;
    let _secondary_channel = int::<i32>(cols, 17, "secondary_channel")?;
    let local_timestamp = int::<u32>(cols, 18, "local_timestamp")?;
    let _ant = int::<i32>(cols, 19, "ant")?;
    let sig_len = int::<u32>(cols, 20, "sig_len")?;
    let _rx_format = int::<i32>(cols, 21, "rx_format")?;
    let len = int::<usize>(cols, 22, "len")?;
    let first_word = int::<i32>(cols, 23, "first_word")?;
    check_data(len, &data)?;
    Ok(RawCsiFrame {
        format: LineFormat::Esp32Classic,
        seq,
        mac,
        rssi,
        rate,
        mcs: Some(mcs),
        noise_floor,
        channel,
        local_timestamp,
        sig_len,
        first_word_invalid: first_word != 0,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const C6_LINE: &str = concat!(
        "CSI_DATA,312,1a:2b:3c:4d:5e:6f,-52,11,-92,4,12,6,183920121,128,1,",
        "6,0,\"[4,3,0,1,1,0]\""
    );

    const CLASSIC_LINE: &str = concat!(
        "CSI_DATA,7,aa:bb:cc:dd:ee:ff,-48,11,1,7,1,0,0,0,0,0,0,-95,0,6,0,",
        "123456,0,57,0,4,0,\"[0,1,1,0]\""
    );

    #[test]
    fn parses_c6_line() {
        let raw = parse_line(C6_LINE).unwrap();
        assert_eq!(raw.format, LineFormat::Esp32C6Family);
        assert_eq!(raw.seq, 312);
        assert_eq!(raw.mac.to_string(), "1a:2b:3c:4d:5e:6f");
        assert_eq!(raw.rssi, -52);
        assert_eq!(raw.rate, 11);
        assert_eq!(raw.mcs, None);
        assert_eq!(raw.noise_floor, -92);
        assert_eq!(raw.channel, 6);
        assert_eq!(raw.local_timestamp, 183_920_121);
        assert_eq!(raw.sig_len, 128);
        assert!(!raw.first_word_invalid);
        assert_eq!(raw.data, [4, 3, 0, 1, 1, 0]);
    }

    #[test]
    fn parses_classic_line() {
        let raw = parse_line(CLASSIC_LINE).unwrap();
        assert_eq!(raw.format, LineFormat::Esp32Classic);
        assert_eq!(raw.seq, 7);
        assert_eq!(raw.mac.to_string(), "aa:bb:cc:dd:ee:ff");
        assert_eq!(raw.mcs, Some(7));
        assert_eq!(raw.noise_floor, -95);
        assert_eq!(raw.channel, 6);
        assert_eq!(raw.sig_len, 57);
        assert_eq!(raw.data, [0, 1, 1, 0]);
    }

    #[test]
    fn tolerates_trailing_crlf() {
        let line = format!("{C6_LINE}\r\n");
        assert!(parse_line(&line).is_ok());
    }

    #[test]
    fn rejects_non_csi_lines() {
        assert_eq!(
            parse_line("I (1234) wifi: connected"),
            Err(ParseError::NotCsiData)
        );
        assert_eq!(parse_line(""), Err(ParseError::NotCsiData));
    }

    #[test]
    fn rejects_unknown_layout() {
        // C6 line with one metadata column removed (fft_gain).
        let line = concat!(
            "CSI_DATA,312,1a:2b:3c:4d:5e:6f,-52,11,-92,12,6,183920121,128,1,",
            "6,0,\"[4,3,0,1,1,0]\""
        );
        assert_eq!(
            parse_line(line),
            Err(ParseError::UnknownLayout { columns: 14 })
        );
    }

    #[test]
    fn rejects_invalid_integer_with_column_name() {
        let line = C6_LINE.replace("-52", "abc");
        assert_eq!(
            parse_line(&line),
            Err(ParseError::InvalidInteger { column: "rssi" })
        );
    }

    #[test]
    fn rejects_invalid_mac() {
        let line = C6_LINE.replace("1a:2b:3c:4d:5e:6f", "1a:2b:3c:4d:5e");
        assert_eq!(parse_line(&line), Err(ParseError::InvalidMac));
    }

    #[test]
    fn rejects_odd_iq_count() {
        let line = C6_LINE.replace("\"[4,3,0,1,1,0]\"", "\"[4,3,0]\"");
        assert_eq!(parse_line(&line), Err(ParseError::OddIqCount { count: 3 }));
    }

    #[test]
    fn rejects_len_mismatch() {
        let line = C6_LINE.replace(",6,0,\"[", ",8,0,\"[");
        assert_eq!(
            parse_line(&line),
            Err(ParseError::LenMismatch {
                declared: 8,
                actual: 6
            })
        );
    }

    #[test]
    fn rejects_missing_data_array() {
        assert_eq!(
            parse_line("CSI_DATA,312,1a:2b:3c:4d:5e:6f,-52"),
            Err(ParseError::MissingDataArray)
        );
    }

    #[test]
    fn mac_addr_round_trips_and_accepts_uppercase() {
        let mac: MacAddr = "AA:0B:cC:1d:Ee:0F".parse().unwrap();
        assert_eq!(mac.to_string(), "aa:0b:cc:1d:ee:0f");
        assert_eq!(mac, MacAddr([0xaa, 0x0b, 0xcc, 0x1d, 0xee, 0x0f]));
    }

    #[test]
    fn mac_addr_rejects_garbage() {
        for bad in [
            "",
            "aa:bb:cc:dd:ee:zz",
            "aa:bb:cc:dd:ee:ff:00",
            "aabbccddeeff",
        ] {
            assert_eq!(
                bad.parse::<MacAddr>(),
                Err(ParseError::InvalidMac),
                "input: {bad}"
            );
        }
    }
}
