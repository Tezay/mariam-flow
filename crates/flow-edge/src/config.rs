//! Persisted appliance configuration.
//!
//! The appliance keeps its configuration in one human-readable JSON file.
//! That is deliberate: on-site troubleshooting sometimes means reading (or
//! repairing) the configuration over a serial console or from the SD card
//! pulled out of a dead unit, and a text file survives that. Historical
//! series — node health, estimates, events — go to SQLite instead, where
//! queries and retention belong.
//!
//! The file is validated on every load and every save, so the rest of the
//! daemon can treat an [`ApplianceConfig`] as known good. Saves are atomic
//! (write to a temporary file, then rename): a power cut during a write
//! leaves the previous configuration intact rather than a truncated one.
//!
//! [`SiteTuning`] mirrors the `site.json` shape already consumed by
//! `csi-infer` and `flow-api`, field for field, so a tuning file produced
//! during lab work can be pasted into the appliance configuration and vice
//! versa.

use std::collections::HashSet;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;

use flow_core::{ClassMapping, NodeRole};
use flow_infer::{WaitConfig, WaitEstimator};
use flow_ingest::MacAddr;
use serde::{Deserialize, Serialize};

use crate::error::{ConfigError, StoreError};
use crate::schedule::ServiceWindow;
use crate::store::{read_to_string, write_atomic};

/// Wi-Fi channel the sensor access point runs on unless configured
/// otherwise.
///
/// A station only senses CSI on the channel it is associated with, so this
/// value must match the transmitter's channel; 1, 6 and 11 are the
/// non-overlapping 2.4 GHz channels and 6 is the project default.
pub const DEFAULT_SENSOR_CHANNEL: u8 = 6;

/// Default analysis window, in µs — must match the training window.
pub const DEFAULT_WINDOW_US: u64 = 5_000_000;

/// Default emission period, in µs of stream time.
pub const DEFAULT_HOP_US: u64 = 1_000_000;

/// The complete persisted configuration of one appliance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplianceConfig {
    /// Who this appliance is and where it is installed.
    pub identity: Identity,
    /// Radio and uplink configuration.
    pub network: NetworkConfig,
    /// Sensing nodes paired with this appliance.
    #[serde(default)]
    pub nodes: Vec<PairedNode>,
    /// Per-site wait-estimation parameters, absent until the site is
    /// calibrated.
    #[serde(default)]
    pub site: Option<SiteTuning>,
    /// What each density class means at this site.
    ///
    /// Decided once per site rather than per capture: two people labelling
    /// the same queue must place the boundaries in the same place, and
    /// re-answering the question every session is how they drift apart. Each
    /// recorded session carries a copy, so the stored format stays readable
    /// on its own.
    #[serde(default)]
    pub classes: Option<ClassMapping>,
    /// When the site serves, or `None` while no schedule is set.
    ///
    /// Absent means always open: an appliance whose hours have not been
    /// declared must keep estimating rather than fall silent.
    #[serde(default)]
    pub service: Option<ServiceWindow>,
    /// Set once the installer explicitly closes the guided installation.
    ///
    /// Kept here rather than derived, so that a completed installation
    /// never falls back into the wizard because a node happens to be
    /// unplugged.
    #[serde(default)]
    pub onboarding_completed: bool,
}

/// Identity of the appliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Stable kit identifier, printed on the appliance label and used in
    /// support exchanges.
    pub kit_id: String,
    /// Human-readable site name, chosen by the installer during
    /// onboarding.
    #[serde(default)]
    pub site_name: Option<String>,
}

/// Radio and uplink configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// The access point the sensing nodes join. Always on: it *is* the
    /// sensor network.
    pub sensor_ap: SensorAp,
    /// How the appliance reaches the site network, or `None` while the
    /// installer has not decided yet.
    ///
    /// `Some(Uplink::Offline)` is a deliberate choice to run without any
    /// site network and is a fully supported mode; `None` only means the
    /// question has not been answered.
    #[serde(default)]
    pub uplink: Option<Uplink>,
    /// What the site's network was found to ask of a device joining it.
    ///
    /// Kept apart from `uplink`, which says what the appliance will do: the
    /// survey stays true when the appliance is left offline because the site
    /// demands something it cannot yet offer, and it is what the request sent
    /// to the site's network administrator is built from.
    #[serde(default)]
    pub survey: Option<NetworkSurvey>,
}

/// What a device is asked for when it joins the site's network.
///
/// Phrased as the installer experiences it rather than by protocol, because
/// that is the question they can answer: what a phone asks when it joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SiteAuthentication {
    /// Nothing at all.
    Nothing,
    /// One password, shared by everyone.
    SharedPassword,
    /// A personal account — a name and a password (802.1X with EAP).
    Account,
    /// A certificate installed on the device beforehand (EAP-TLS).
    Certificate,
    /// A web page to sign in on after joining (a captive portal).
    SignInPage,
    /// The installer could not say.
    Unknown,
}

impl SiteAuthentication {
    /// Whether an appliance can join a network that asks this, today.
    ///
    /// The three it cannot need the site's network administrator to act
    /// (ADR 0018), which is why the survey records the answer even when the
    /// appliance ends up offline.
    #[must_use]
    pub fn joinable(self) -> bool {
        matches!(self, Self::Nothing | Self::SharedPassword)
    }
}

/// What the installer found out about the site's network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSurvey {
    /// What joining the network asks for.
    pub authentication: SiteAuthentication,
    /// Devices must be declared before they are allowed on.
    #[serde(default)]
    pub registration_required: bool,
    /// The site hands out a fixed address rather than using DHCP.
    #[serde(default)]
    pub fixed_address: bool,
}

/// The access point hosted for the sensing nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorAp {
    /// Network name announced to the nodes.
    pub ssid: String,
    /// WPA2 passphrase, 8..=63 characters.
    pub passphrase: String,
    /// 2.4 GHz channel; must match the transmitter's channel.
    #[serde(default = "default_sensor_channel")]
    pub channel: u8,
}

/// How the appliance reaches the site network.
///
/// The built-in radio is reserved for the sensor access point, so every
/// connected variant runs on a *second* interface (a USB Wi-Fi adapter or
/// a USB Ethernet adapter). Both are configured through the same code
/// path; only the interface and the link-layer settings differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Uplink {
    /// No site network at all — a fully supported mode, not a failure.
    /// Estimates stay on the appliance and are read on site.
    Offline,
    /// Wi-Fi client on the dedicated USB adapter.
    Wifi {
        /// Network name to join.
        ssid: String,
        /// Link-layer security of that network.
        security: WifiSecurity,
        /// How an address is obtained on it.
        #[serde(default)]
        addressing: Addressing,
    },
    /// Wired client on a USB Ethernet adapter.
    Ethernet {
        /// How an address is obtained on it.
        #[serde(default)]
        addressing: Addressing,
    },
}

/// Link-layer security of an uplink Wi-Fi network.
///
/// Enterprise authentication (WPA2/3-Enterprise, 802.1X with EAP) is a
/// planned addition; it slots in as a further variant without disturbing
/// the stored shape of the other two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WifiSecurity {
    /// Unauthenticated network.
    Open,
    /// WPA2/WPA3-Personal with a shared passphrase.
    WpaPersonal {
        /// The network passphrase, 8..=63 characters.
        passphrase: String,
    },
}

/// How an interface obtains its IP configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub enum Addressing {
    /// Address assigned by the site's DHCP server. A reservation on the
    /// interface's MAC keeps it stable, which is what the network handout
    /// asks for.
    #[default]
    Dhcp,
    /// Fixed address supplied by the site's network administrator.
    Static {
        /// Address of the appliance on the site network.
        address: IpAddr,
        /// Prefix length of the site subnet (for example 24).
        prefix_len: u8,
        /// Default gateway.
        gateway: IpAddr,
        /// Name servers to use.
        #[serde(default)]
        dns: Vec<IpAddr>,
    },
}

/// One sensing node paired with this appliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedNode {
    /// Logical id, matching the `node_id` used in capture sessions
    /// (`tx-1`, `rx-1`, `rx-2`).
    pub node_id: String,
    /// Whether the node transmits or receives.
    pub role: NodeRole,
    /// Hardware address, as seen on the sensor access point.
    ///
    /// Required of a transmitter, which is known by nothing else. A receiver
    /// is identified by its source address at intake (ADR 0007), so its MAC is
    /// recorded only when a DHCP lease has revealed it.
    #[serde(default)]
    pub mac: Option<String>,
    /// Address reserved for this node on the sensor network.
    ///
    /// A receiver without one cannot be told apart from its sibling. The
    /// transmitter never joins the access point and therefore has none.
    #[serde(default)]
    pub address: Option<IpAddr>,
}

/// Per-site wait-estimation parameters.
///
/// Field-for-field mirror of the `site.json` files consumed by
/// `csi-infer` and `flow-api`; [`SiteTuning::wait_config`] converts it to
/// the domain type owned by `flow-infer`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SiteTuning {
    /// Calibrated people count per density class, in class order.
    pub people_per_class: [f32; 4],
    /// Service rate λ, in people served per minute.
    pub service_rate_per_min: f32,
    /// Time constant τ of the output smoothing, in seconds.
    pub smoothing_tau_s: f32,
    /// Hysteresis half-width on the 0–3 level scale.
    pub hysteresis_margin: f32,
    /// Confidence below which an estimate is flagged unreliable.
    pub min_confidence: f32,
    /// Analysis window in µs — must match the training window.
    #[serde(default = "default_window_us")]
    pub window_us: u64,
    /// Emission period in µs of stream time.
    #[serde(default = "default_hop_us")]
    pub hop_us: u64,
}

fn default_sensor_channel() -> u8 {
    DEFAULT_SENSOR_CHANNEL
}

fn default_window_us() -> u64 {
    DEFAULT_WINDOW_US
}

fn default_hop_us() -> u64 {
    DEFAULT_HOP_US
}

impl SiteTuning {
    /// Converts to the wait-estimator configuration owned by `flow-infer`.
    #[must_use]
    pub fn wait_config(&self) -> WaitConfig {
        WaitConfig {
            people_per_class: self.people_per_class,
            service_rate_per_min: self.service_rate_per_min,
            smoothing_tau_s: self.smoothing_tau_s,
            hysteresis_margin: self.hysteresis_margin,
            min_confidence: self.min_confidence,
        }
    }

    /// Checks that the tuning is usable.
    ///
    /// The estimator's own constructor is the single source of truth for
    /// what a valid tuning is, so this builds one and discards it rather
    /// than restating its rules.
    ///
    /// # Errors
    ///
    /// [`ConfigError::SiteTuning`] for a rejected estimator parameter, or
    /// [`ConfigError::NotPositive`] for a zero window or hop.
    pub fn validate(&self) -> Result<(), ConfigError> {
        WaitEstimator::new(self.wait_config())
            .map_err(|err| ConfigError::SiteTuning(err.to_string()))?;
        if self.window_us == 0 {
            return Err(ConfigError::NotPositive { field: "window_us" });
        }
        if self.hop_us == 0 {
            return Err(ConfigError::NotPositive { field: "hop_us" });
        }
        Ok(())
    }
}

impl ApplianceConfig {
    /// Builds the configuration an appliance leaves the factory with: it
    /// knows its identity and its sensor access point, and nothing else.
    #[must_use]
    pub fn factory(
        kit_id: impl Into<String>,
        ssid: impl Into<String>,
        passphrase: impl Into<String>,
    ) -> Self {
        Self {
            identity: Identity {
                kit_id: kit_id.into(),
                site_name: None,
            },
            network: NetworkConfig {
                sensor_ap: SensorAp {
                    ssid: ssid.into(),
                    passphrase: passphrase.into(),
                    channel: DEFAULT_SENSOR_CHANNEL,
                },
                uplink: None,
                survey: None,
            },
            classes: None,
            nodes: Vec::new(),
            site: None,
            service: None,
            onboarding_completed: false,
        }
    }

    /// Logical ids of the paired receivers, sorted.
    ///
    /// Sorted because the model's input vector concatenates per-node
    /// features in that order: a different order silently feeds the model
    /// the wrong columns.
    #[must_use]
    pub fn rx_node_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .nodes
            .iter()
            .filter(|node| node.role == NodeRole::Rx)
            .map(|node| node.node_id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// The paired transmitter, if one has been paired.
    #[must_use]
    pub fn transmitter(&self) -> Option<&PairedNode> {
        self.nodes.iter().find(|node| node.role == NodeRole::Tx)
    }

    /// Checks every invariant the daemon relies on.
    ///
    /// # Errors
    ///
    /// The first [`ConfigError`] found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.identity.kit_id.trim().is_empty() {
            return Err(ConfigError::Empty { field: "kit_id" });
        }
        if let Some(name) = &self.identity.site_name
            && name.trim().is_empty()
        {
            return Err(ConfigError::Empty { field: "site_name" });
        }
        self.network.validate()?;
        self.validate_nodes()?;
        if let Some(site) = &self.site {
            site.validate()?;
        }
        if let Some(service) = &self.service {
            service.validate().map_err(ConfigError::Service)?;
        }
        Ok(())
    }

    fn validate_nodes(&self) -> Result<(), ConfigError> {
        let mut ids = HashSet::new();
        let mut macs = HashSet::new();
        let mut addresses = HashSet::new();
        let mut transmitters = 0usize;

        for node in &self.nodes {
            if node.node_id.trim().is_empty() {
                return Err(ConfigError::Empty { field: "node_id" });
            }
            if !ids.insert(node.node_id.as_str()) {
                return Err(ConfigError::DuplicateNodeId(node.node_id.clone()));
            }
            match &node.mac {
                Some(text) => {
                    let mac = MacAddr::from_str(text).map_err(|_| ConfigError::NodeMac {
                        node_id: node.node_id.clone(),
                        mac: text.clone(),
                    })?;
                    if !macs.insert(mac) {
                        return Err(ConfigError::DuplicateNodeMac(mac.to_string()));
                    }
                }
                // A transmitter has no other identity to be known by.
                None if node.role == NodeRole::Tx => {
                    return Err(ConfigError::Empty { field: "tx mac" });
                }
                None => {}
            }
            if let Some(address) = node.address
                && !addresses.insert(address)
            {
                return Err(ConfigError::DuplicateNodeAddress(address.to_string()));
            }
            if node.role == NodeRole::Tx {
                transmitters += 1;
            }
        }

        if transmitters > 1 {
            return Err(ConfigError::MultipleTransmitters(transmitters));
        }
        Ok(())
    }

    /// Reads and validates the configuration at `path`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the file cannot be read, does not parse, or holds
    /// a configuration the daemon refuses to run with.
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        let text = read_to_string(path)?;
        let config: Self = serde_json::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.validate().map_err(|source| StoreError::Invalid {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(config)
    }

    /// Validates, then writes the configuration to `path` atomically.
    ///
    /// Validating before writing means an invalid configuration can never
    /// reach the disk and lock the appliance out of its next boot.
    ///
    /// # Errors
    ///
    /// [`StoreError::Invalid`] if the configuration is unusable, or the
    /// underlying write failure.
    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        self.validate().map_err(|source| StoreError::Invalid {
            path: path.to_path_buf(),
            source,
        })?;
        let mut text = serde_json::to_string_pretty(self).map_err(|source| StoreError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        text.push('\n');
        write_atomic(path, &text)
    }
}

impl NetworkConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_ssid(&self.sensor_ap.ssid)?;
        validate_passphrase(&self.sensor_ap.passphrase)?;
        if !(1..=13).contains(&self.sensor_ap.channel) {
            return Err(ConfigError::Channel(self.sensor_ap.channel));
        }
        match &self.uplink {
            None | Some(Uplink::Offline) => Ok(()),
            Some(Uplink::Ethernet { addressing }) => addressing.validate(),
            Some(Uplink::Wifi {
                ssid,
                security,
                addressing,
            }) => {
                validate_ssid(ssid)?;
                if let WifiSecurity::WpaPersonal { passphrase } = security {
                    validate_passphrase(passphrase)?;
                }
                addressing.validate()
            }
        }
    }
}

impl Addressing {
    fn validate(&self) -> Result<(), ConfigError> {
        match self {
            Self::Dhcp => Ok(()),
            Self::Static {
                address,
                prefix_len,
                ..
            } => {
                let max = if address.is_ipv4() { 32 } else { 128 };
                if *prefix_len == 0 || *prefix_len > max {
                    return Err(ConfigError::NotPositive {
                        field: "prefix_len",
                    });
                }
                Ok(())
            }
        }
    }
}

fn validate_ssid(ssid: &str) -> Result<(), ConfigError> {
    let len = ssid.len();
    if len == 0 || len > 32 {
        return Err(ConfigError::SsidLength(len));
    }
    Ok(())
}

fn validate_passphrase(passphrase: &str) -> Result<(), ConfigError> {
    let len = passphrase.chars().count();
    if !(8..=63).contains(&len) {
        return Err(ConfigError::PassphraseLength(len));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn factory() -> ApplianceConfig {
        ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", "correct-horse-battery")
    }

    fn node(node_id: &str, role: NodeRole, mac: &str, address: Option<&str>) -> PairedNode {
        PairedNode {
            node_id: node_id.into(),
            role,
            mac: Some(mac.into()),
            address: address.map(|a| a.parse().unwrap()),
        }
    }

    #[test]
    fn a_receiver_needs_no_mac_but_a_transmitter_does() {
        let mut config = factory();
        config.nodes = vec![PairedNode {
            node_id: "rx-1".into(),
            role: NodeRole::Rx,
            mac: None,
            address: Some("192.168.4.51".parse().unwrap()),
        }];
        config.validate().unwrap();

        config.nodes.push(PairedNode {
            node_id: "tx-1".into(),
            role: NodeRole::Tx,
            mac: None,
            address: None,
        });
        let error = config.validate().unwrap_err();
        assert!(
            matches!(error, ConfigError::Empty { field } if field.contains("mac")),
            "{error}"
        );
    }

    fn tuning() -> SiteTuning {
        SiteTuning {
            people_per_class: [0.0, 4.0, 12.0, 25.0],
            service_rate_per_min: 6.0,
            smoothing_tau_s: 30.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
            window_us: DEFAULT_WINDOW_US,
            hop_us: DEFAULT_HOP_US,
        }
    }

    #[test]
    fn a_factory_configuration_is_valid_and_undecided() {
        let config = factory();
        config.validate().unwrap();
        assert_eq!(config.network.sensor_ap.channel, DEFAULT_SENSOR_CHANNEL);
        assert!(config.network.uplink.is_none(), "uplink not decided yet");
        assert!(!config.onboarding_completed);
        assert!(config.rx_node_ids().is_empty());
        assert!(config.transmitter().is_none());
    }

    #[test]
    fn offline_is_a_decision_distinct_from_no_decision() {
        let mut config = factory();
        config.network.uplink = Some(Uplink::Offline);
        config.validate().unwrap();
        assert_ne!(config.network.uplink, None);
    }

    #[test]
    fn receivers_are_reported_in_sorted_order() {
        let mut config = factory();
        config.nodes = vec![
            node(
                "rx-2",
                NodeRole::Rx,
                "aa:bb:cc:00:00:02",
                Some("192.168.4.52"),
            ),
            node("tx-1", NodeRole::Tx, "1a:00:00:00:00:00", None),
            node(
                "rx-1",
                NodeRole::Rx,
                "aa:bb:cc:00:00:01",
                Some("192.168.4.51"),
            ),
        ];
        config.validate().unwrap();
        assert_eq!(config.rx_node_ids(), vec!["rx-1", "rx-2"]);
        assert_eq!(config.transmitter().unwrap().node_id, "tx-1");
    }

    #[test]
    fn empty_kit_id_is_rejected() {
        let mut config = factory();
        config.identity.kit_id = "  ".into();
        assert_eq!(
            config.validate(),
            Err(ConfigError::Empty { field: "kit_id" })
        );
    }

    #[test]
    fn wifi_limits_are_enforced() {
        let mut config = factory();
        config.network.sensor_ap.ssid = "x".repeat(33);
        assert_eq!(config.validate(), Err(ConfigError::SsidLength(33)));

        let mut config = factory();
        config.network.sensor_ap.passphrase = "short".into();
        assert_eq!(config.validate(), Err(ConfigError::PassphraseLength(5)));

        let mut config = factory();
        config.network.sensor_ap.channel = 14;
        assert_eq!(config.validate(), Err(ConfigError::Channel(14)));
    }

    #[test]
    fn uplink_credentials_are_validated_too() {
        let mut config = factory();
        config.network.uplink = Some(Uplink::Wifi {
            ssid: "campus".into(),
            security: WifiSecurity::WpaPersonal {
                passphrase: "nope".into(),
            },
            addressing: Addressing::Dhcp,
        });
        assert_eq!(config.validate(), Err(ConfigError::PassphraseLength(4)));
    }

    #[test]
    fn static_addressing_rejects_an_impossible_prefix() {
        let mut config = factory();
        config.network.uplink = Some(Uplink::Ethernet {
            addressing: Addressing::Static {
                address: "192.168.1.20".parse().unwrap(),
                prefix_len: 33,
                gateway: "192.168.1.1".parse().unwrap(),
                dns: vec![],
            },
        });
        assert_eq!(
            config.validate(),
            Err(ConfigError::NotPositive {
                field: "prefix_len"
            })
        );
    }

    #[test]
    fn node_identity_collisions_are_rejected() {
        let mut config = factory();
        config.nodes = vec![
            node("rx-1", NodeRole::Rx, "aa:bb:cc:00:00:01", None),
            node("rx-1", NodeRole::Rx, "aa:bb:cc:00:00:02", None),
        ];
        assert_eq!(
            config.validate(),
            Err(ConfigError::DuplicateNodeId("rx-1".into()))
        );

        let mut config = factory();
        config.nodes = vec![
            node("rx-1", NodeRole::Rx, "aa:bb:cc:00:00:01", None),
            node("rx-2", NodeRole::Rx, "AA:BB:CC:00:00:01", None),
        ];
        assert!(matches!(
            config.validate(),
            Err(ConfigError::DuplicateNodeMac(_))
        ));

        let mut config = factory();
        config.nodes = vec![
            node(
                "rx-1",
                NodeRole::Rx,
                "aa:bb:cc:00:00:01",
                Some("192.168.4.51"),
            ),
            node(
                "rx-2",
                NodeRole::Rx,
                "aa:bb:cc:00:00:02",
                Some("192.168.4.51"),
            ),
        ];
        assert!(matches!(
            config.validate(),
            Err(ConfigError::DuplicateNodeAddress(_))
        ));
    }

    #[test]
    fn a_second_transmitter_is_rejected() {
        let mut config = factory();
        config.nodes = vec![
            node("tx-1", NodeRole::Tx, "1a:00:00:00:00:00", None),
            node("tx-2", NodeRole::Tx, "1a:00:00:00:00:01", None),
        ];
        assert_eq!(config.validate(), Err(ConfigError::MultipleTransmitters(2)));
    }

    #[test]
    fn a_malformed_mac_names_the_node_that_carries_it() {
        let mut config = factory();
        config.nodes = vec![node("rx-1", NodeRole::Rx, "not-a-mac", None)];
        assert_eq!(
            config.validate(),
            Err(ConfigError::NodeMac {
                node_id: "rx-1".into(),
                mac: "not-a-mac".into(),
            })
        );
    }

    #[test]
    fn site_tuning_delegates_to_the_wait_estimator() {
        let mut config = factory();
        let mut bad = tuning();
        bad.service_rate_per_min = 0.0;
        config.site = Some(bad);
        assert!(matches!(config.validate(), Err(ConfigError::SiteTuning(_))));

        let mut config = factory();
        let mut bad = tuning();
        bad.hop_us = 0;
        config.site = Some(bad);
        assert_eq!(
            config.validate(),
            Err(ConfigError::NotPositive { field: "hop_us" })
        );
    }

    #[test]
    fn tuning_converts_to_the_domain_type() {
        let tuning = tuning();
        let wait = tuning.wait_config();
        assert_eq!(wait.people_per_class, [0.0, 4.0, 12.0, 25.0]);
        assert!((wait.service_rate_per_min - 6.0).abs() < f32::EPSILON);
        tuning.validate().unwrap();
    }

    #[test]
    fn a_saved_configuration_reloads_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("appliance.json");

        let mut config = factory();
        config.identity.site_name = Some("RU EFREI".into());
        config.network.uplink = Some(Uplink::Wifi {
            ssid: "campus".into(),
            security: WifiSecurity::WpaPersonal {
                passphrase: "campus-secret".into(),
            },
            addressing: Addressing::Dhcp,
        });
        config.nodes = vec![
            node("tx-1", NodeRole::Tx, "1a:00:00:00:00:00", None),
            node(
                "rx-1",
                NodeRole::Rx,
                "aa:bb:cc:00:00:01",
                Some("192.168.4.51"),
            ),
        ];
        config.site = Some(tuning());

        config.save(&path).unwrap();
        assert_eq!(ApplianceConfig::load(&path).unwrap(), config);
    }

    #[cfg(unix)]
    #[test]
    fn a_saved_configuration_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");
        factory().save(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "the file carries Wi-Fi passphrases");
    }

    #[test]
    fn saving_an_invalid_configuration_never_touches_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");

        let mut config = factory();
        config.network.sensor_ap.channel = 99;
        assert!(config.save(&path).is_err());
        assert!(!path.exists(), "an unusable config must not be persisted");
    }

    #[test]
    fn an_invalid_stored_configuration_is_refused_at_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");
        // Channel 99 is structurally well-formed JSON but unusable.
        let mut config = factory();
        config.network.sensor_ap.channel = 6;
        config.save(&path).unwrap();
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("\"channel\": 6", "\"channel\": 99");
        fs::write(&path, text).unwrap();

        assert!(matches!(
            ApplianceConfig::load(&path),
            Err(StoreError::Invalid { .. })
        ));
    }

    #[test]
    fn unknown_uplink_modes_are_reported_as_parse_failures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("appliance.json");
        fs::write(
            &path,
            r#"{"identity":{"kit_id":"K"},"network":{"sensor_ap":{"ssid":"s","passphrase":"12345678"},"uplink":{"mode":"carrier-pigeon"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            ApplianceConfig::load(&path),
            Err(StoreError::Parse { .. })
        ));
    }
}
