//! Recording a labeled capture session from the appliance.
//!
//! The appliance already knows most of what a session's metadata asks for —
//! the site, the nodes, the radio channel, its own version. Only what it
//! cannot know is requested from the operator: where each node physically
//! sits, and what the density classes mean at this site.

use flow_core::{ClassMapping, NodePlacement, SessionMeta};
use serde::{Deserialize, Serialize};

use crate::config::ApplianceConfig;

/// What the operator supplies when starting a session.
///
/// Everything here is a fact about the room rather than about the appliance,
/// which is why none of it can be derived.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRequest {
    /// Free-text description of the physical environment.
    #[serde(default)]
    pub environment: String,
    /// Where each node sits, keyed by node id. Nodes left out record an
    /// empty position rather than blocking the session.
    #[serde(default)]
    pub positions: std::collections::BTreeMap<String, String>,
    /// What each density class means at this site.
    #[serde(default)]
    pub class_mapping: ClassMapping,
}

/// Builds a session identifier from the appliance clock.
///
/// Sortable, unique per second, and safe as a directory name — the session id
/// *is* the directory, so anything a path would object to is excluded by
/// construction rather than rejected later.
#[must_use]
pub fn session_id(now_us: u64, kit_id: &str) -> String {
    let seconds = i64::try_from(now_us / 1_000_000).unwrap_or(0);
    let stamp = jiff::Timestamp::from_second(seconds).map_or_else(
        |_| seconds.to_string(),
        |ts| ts.strftime("%Y%m%dT%H%M%SZ").to_string(),
    );
    format!("{}-{stamp}", slug(kit_id))
}

/// Keeps only what a directory name may safely carry.
fn slug(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "kit".to_owned()
    } else {
        trimmed
    }
}

/// Assembles the metadata for a session about to be recorded.
///
/// The node list comes from the pairing rather than from the request: a
/// session that names nodes the appliance is not listening to would record
/// frames it cannot attribute.
#[must_use]
pub fn session_meta(
    config: &ApplianceConfig,
    request: &SessionRequest,
    session_id: String,
) -> SessionMeta {
    SessionMeta {
        session_id,
        site: config
            .identity
            .site_name
            .clone()
            .unwrap_or_else(|| config.identity.kit_id.clone()),
        environment: request.environment.clone(),
        wifi_channel: config.network.sensor_ap.channel,
        nodes: config
            .nodes
            .iter()
            .map(|node| NodePlacement {
                node_id: node.node_id.clone(),
                role: node.role,
                position: request
                    .positions
                    .get(&node.node_id)
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
        // The nodes are pre-flashed per kit and the appliance has no way to
        // ask them; recorded as unknown rather than guessed.
        firmware_version: String::new(),
        software_version: env!("CARGO_PKG_VERSION").to_owned(),
        class_mapping: request.class_mapping.clone(),
    }
}

/// Whether any session has been recorded and sealed at this site.
///
/// A directory still carrying the recording suffix does not count: it is a
/// capture that never finished, and training on it would be training on a
/// truncated file.
#[must_use]
pub fn has_sealed_session(sessions_root: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(sessions_root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.path().is_dir() && !entry.file_name().to_string_lossy().ends_with(".recording")
    })
}

#[cfg(test)]
mod tests {
    use flow_core::NodeRole;

    use super::*;
    use crate::config::PairedNode;

    const NOW: u64 = 1_785_500_000_000_000;

    fn installed() -> ApplianceConfig {
        let mut config = ApplianceConfig::factory("KIT-0042", "mariam-flow-0042", "correct-horse");
        config.identity.site_name = Some("RU EFREI".into());
        config.nodes = vec![
            PairedNode {
                node_id: "tx-1".into(),
                role: NodeRole::Tx,
                mac: Some("1a:00:00:00:00:00".into()),
                address: None,
            },
            PairedNode {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                mac: None,
                address: Some("192.168.4.51".parse().unwrap()),
            },
        ];
        config
    }

    #[test]
    fn a_site_counts_as_captured_only_once_a_session_is_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        assert!(!has_sealed_session(&sessions), "nothing recorded yet");

        std::fs::create_dir_all(sessions.join("kit-0042-20260731T120000Z.recording")).unwrap();
        // A capture in progress is not a capture: training on a truncated
        // file would be worse than having none.
        assert!(!has_sealed_session(&sessions));

        std::fs::create_dir(sessions.join("kit-0042-20260731T120000Z")).unwrap();
        assert!(has_sealed_session(&sessions));
    }

    #[test]
    fn a_session_id_sorts_by_time_and_names_its_kit() {
        let earlier = session_id(NOW, "KIT-0042");
        let later = session_id(NOW + 60_000_000, "KIT-0042");

        assert!(earlier.starts_with("kit-0042-"));
        assert!(earlier < later, "{earlier} should sort before {later}");
    }

    #[test]
    fn a_session_id_carries_nothing_a_path_would_object_to() {
        // The id is the directory name, so the unsafe characters are excluded
        // here rather than rejected by the writer afterwards.
        let id = session_id(NOW, "../etc/passwd");
        assert!(!id.contains('/'));
        assert!(!id.contains('.'));
    }

    #[test]
    fn a_kit_with_no_usable_name_still_yields_an_id() {
        let id = session_id(NOW, "///");
        assert!(id.starts_with("kit-"));
    }

    #[test]
    fn metadata_takes_its_nodes_from_the_pairing() {
        // A session naming nodes the appliance is not listening to would
        // record frames it cannot attribute.
        let request = SessionRequest {
            positions: [("rx-1".to_owned(), "left of the entrance".to_owned())]
                .into_iter()
                .collect(),
            ..SessionRequest::default()
        };

        let meta = session_meta(&installed(), &request, "session-1".into());

        assert_eq!(meta.nodes.len(), 2);
        assert_eq!(meta.nodes[1].position, "left of the entrance");
        assert_eq!(meta.wifi_channel, 6);
        assert_eq!(meta.site, "RU EFREI");
    }

    #[test]
    fn a_node_left_out_of_the_request_records_an_empty_position() {
        // Blocking the session over a missing description would cost a
        // capture; an empty position is recoverable, a lost session is not.
        let meta = session_meta(&installed(), &SessionRequest::default(), "session-1".into());

        assert!(meta.nodes.iter().all(|node| node.position.is_empty()));
    }

    #[test]
    fn an_unnamed_site_falls_back_to_the_kit() {
        let mut config = installed();
        config.identity.site_name = None;

        let meta = session_meta(&config, &SessionRequest::default(), "session-1".into());

        assert_eq!(meta.site, "KIT-0042");
    }

    #[test]
    fn the_software_version_is_the_one_that_recorded_it() {
        let meta = session_meta(&installed(), &SessionRequest::default(), "session-1".into());

        assert_eq!(meta.software_version, env!("CARGO_PKG_VERSION"));
    }
}
