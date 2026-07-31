use serde::{Deserialize, Serialize};

use crate::TimestampUs;
use crate::density::DensityClass;

/// One ground-truth annotation, matching one line of `labels.ndjson`.
///
/// Labels are produced during supervised calibration, in one of two ways:
///
/// - **From a reference sensor**: an on-site counting device measures the
///   exact number of people in the zone; `count` carries that measurement
///   and `class` is derived from it using the site-specific thresholds
///   recorded in the session's [`ClassMapping`].
/// - **Manually**: an installer selects the live density class through the
///   labeling tool; `count` is absent.
///
/// Storing the raw count alongside the derived class keeps the dataset
/// re-derivable: class boundaries can be revisited later (e.g. finer
/// granularity at high density) without recapturing any session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Timestamp of the annotation, in microseconds (edge clock).
    pub ts_us: TimestampUs,
    /// Density class observed at that instant.
    pub class: DensityClass,
    /// Exact people count measured by a reference sensor, when available.
    ///
    /// `None` for manually produced labels. Omitted from the serialized
    /// form when absent, so manual and sensor-derived labels coexist in the
    /// same `labels.ndjson`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// Radio role of a sensing node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    /// Dedicated transmitter generating the reference traffic.
    Tx,
    /// Receiver extracting CSI and streaming it to the edge.
    Rx,
}

/// Physical placement of one node for a capture session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePlacement {
    /// Node identifier referenced by [`CsiFrame::node_id`](crate::CsiFrame).
    pub node_id: String,
    /// Radio role of the node.
    pub role: NodeRole,
    /// Free-text description of the physical position
    /// (e.g. `"wall-mounted, 2.1 m, left of entrance"`).
    pub position: String,
}

/// Site-specific meaning of each density class.
///
/// The four classes are frozen, but what they concretely mean depends on the
/// site: in a small lab zone, `saturated` may mean "3+ people", while in a
/// restaurant queue it may mean "queue past the door". For sessions labeled
/// from reference-sensor counts, these descriptions record the count
/// thresholds used to derive each class (e.g. `"12-25 people in zone"`).
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassMapping {
    /// Meaning of [`DensityClass::Empty`] at this site.
    pub empty: String,
    /// Meaning of [`DensityClass::Low`] at this site.
    pub low: String,
    /// Meaning of [`DensityClass::Medium`] at this site.
    pub medium: String,
    /// Meaning of [`DensityClass::Saturated`] at this site.
    pub saturated: String,
}

impl ClassMapping {
    /// Returns the site-specific description of `class`.
    #[must_use]
    pub fn describe(&self, class: DensityClass) -> &str {
        match class {
            DensityClass::Empty => &self.empty,
            DensityClass::Low => &self.low,
            DensityClass::Medium => &self.medium,
            DensityClass::Saturated => &self.saturated,
        }
    }
}

/// Metadata of one capture session, matching `meta.json`.
///
/// A session is one continuous capture at one site; recorded sessions are
/// immutable. The metadata carries everything needed to interpret the
/// session's frames and labels later (and to publish the session in a
/// dataset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique session identifier (also the session directory name).
    pub session_id: String,
    /// Site identifier (e.g. `"lab-a"`).
    pub site: String,
    /// Free-text description of the physical environment.
    pub environment: String,
    /// Wi-Fi channel used by the dedicated transmitter.
    pub wifi_channel: u8,
    /// Placement of every node involved in the capture.
    pub nodes: Vec<NodePlacement>,
    /// Version of the firmware running on the nodes.
    pub firmware_version: String,
    /// Version of the edge software that recorded the session.
    pub software_version: String,
    /// Site-specific meaning of each density class.
    pub class_mapping: ClassMapping,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_label_serializes_without_count_field() {
        let label = Label {
            ts_us: 1_720_000_000_000_000,
            class: DensityClass::Medium,
            count: None,
        };
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, r#"{"ts_us":1720000000000000,"class":2}"#);
    }

    #[test]
    fn sensor_label_serializes_with_count_field() {
        let label = Label {
            ts_us: 1_720_000_000_000_000,
            class: DensityClass::Saturated,
            count: Some(47),
        };
        let json = serde_json::to_string(&label).unwrap();
        assert_eq!(json, r#"{"ts_us":1720000000000000,"class":3,"count":47}"#);
    }

    #[test]
    fn label_deserializes_from_canonical_ndjson_line() {
        let label: Label = serde_json::from_str(r#"{"ts_us":42,"class":0}"#).unwrap();
        assert_eq!(
            label,
            Label {
                ts_us: 42,
                class: DensityClass::Empty,
                count: None
            }
        );
    }

    #[test]
    fn label_deserializes_with_count() {
        let label: Label = serde_json::from_str(r#"{"ts_us":42,"class":3,"count":47}"#).unwrap();
        assert_eq!(
            label,
            Label {
                ts_us: 42,
                class: DensityClass::Saturated,
                count: Some(47)
            }
        );
    }

    #[test]
    fn mixed_label_lines_coexist_in_one_stream() {
        let lines = [
            r#"{"ts_us":1,"class":2,"count":18}"#,
            r#"{"ts_us":2,"class":2}"#,
        ];
        let labels: Vec<Label> = lines
            .iter()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(labels[0].count, Some(18));
        assert_eq!(labels[1].count, None);
        assert_eq!(labels[0].class, labels[1].class);
    }

    #[test]
    fn label_rejects_invalid_class() {
        assert!(serde_json::from_str::<Label>(r#"{"ts_us":42,"class":9}"#).is_err());
    }

    #[test]
    fn node_role_uses_lowercase_encoding() {
        assert_eq!(serde_json::to_string(&NodeRole::Tx).unwrap(), r#""tx""#);
        assert_eq!(serde_json::to_string(&NodeRole::Rx).unwrap(), r#""rx""#);
    }

    #[test]
    fn class_mapping_describes_every_class() {
        let mapping = ClassMapping {
            empty: "0 people".into(),
            low: "1 person".into(),
            medium: "2 people".into(),
            saturated: "3+ people".into(),
        };
        assert_eq!(mapping.describe(DensityClass::Empty), "0 people");
        assert_eq!(mapping.describe(DensityClass::Saturated), "3+ people");
    }

    #[test]
    fn session_meta_json_round_trip() {
        let meta = SessionMeta {
            session_id: "2026-07-10-lab-a-001".into(),
            site: "lab-a".into(),
            environment: "4 m² office corner, one desk, closed door".into(),
            wifi_channel: 6,
            nodes: vec![
                NodePlacement {
                    node_id: "tx-1".into(),
                    role: NodeRole::Tx,
                    position: "shelf, 1.8 m".into(),
                },
                NodePlacement {
                    node_id: "rx-1".into(),
                    role: NodeRole::Rx,
                    position: "opposite wall, 1.2 m".into(),
                },
            ],
            firmware_version: "0.1.0".into(),
            software_version: "0.1.0".into(),
            class_mapping: ClassMapping {
                empty: "0 people".into(),
                low: "1 person".into(),
                medium: "2 people".into(),
                saturated: "3+ people".into(),
            },
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let back: SessionMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }
}
