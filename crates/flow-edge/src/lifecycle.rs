//! Appliance lifecycle: how far the guided installation has got, and what
//! the CSI stream is currently being used for.
//!
//! Two independent questions live here, and keeping them apart avoids a
//! lot of confusion later:
//!
//! - **Installation progress.** Which step of the wizard the installer is
//!   on. This is *derived from facts* ([`Readiness`]) rather than stored as
//!   a cursor, so it cannot drift away from reality — an appliance that
//!   reboots mid-installation resumes exactly where the facts say it is.
//!   The single stored bit is [`ApplianceConfig::onboarding_completed`],
//!   which stops a finished installation from falling back into the wizard
//!   just because a node is temporarily unplugged.
//! - **Runtime activity.** Calibration and live inference both consume the
//!   one UDP stream from the receivers, so at most one may run at a time.
//!   [`Runtime`] makes that exclusivity a type-level rule instead of a
//!   convention: every start requires the stream to be idle, and callers
//!   that want to switch must stop first — an explicit act, never an
//!   accidental one.
//!
//! [`ApplianceConfig::onboarding_completed`]: crate::ApplianceConfig::onboarding_completed

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::config::ApplianceConfig;
use crate::error::TransitionError;

/// One step of the guided installation.
///
/// The order of the variants is the order of the wizard, and
/// [`Readiness::stage`] relies on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Name the site this appliance is installed at.
    Site,
    /// Pair the sensing nodes and confirm they are streaming.
    Nodes,
    /// Decide how (or whether) the appliance reaches the site network.
    Network,
    /// Calibrate the site and obtain a density model.
    Calibration,
    /// Every step is satisfied; the installation can be closed.
    Complete,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::Site => "site identification",
            Self::Nodes => "node pairing",
            Self::Network => "network connection",
            Self::Calibration => "calibration",
            Self::Complete => "completion",
        };
        f.write_str(text)
    }
}

/// The facts the installation wizard reasons about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    /// The installer has named the site.
    pub site_named: bool,
    /// A transmitter and at least one receiver are paired.
    pub nodes_paired: bool,
    /// The uplink question has been answered — including a deliberate
    /// choice to stay offline.
    pub uplink_decided: bool,
    /// A calibration session has been recorded at this site.
    ///
    /// This, and not the model, is what finishes an installation: recording
    /// produces the data, and the model comes back from training days later.
    /// Holding the wizard open until then would lock the site out of every
    /// other screen in the meantime.
    pub site_captured: bool,
    /// A density model is installed and the site is tuned, so live
    /// inference can actually run.
    ///
    /// Reported but not required to finish installing — the live view says
    /// what is missing, and the model is imported from the settings.
    pub model_ready: bool,
}

impl Readiness {
    /// Reads the facts off the stored configuration.
    ///
    /// `model_installed` and `site_captured` are filesystem facts — is there
    /// an active model artifact, is there a sealed session — which the
    /// configuration alone cannot answer.
    #[must_use]
    pub fn evaluate(config: &ApplianceConfig, model_installed: bool, site_captured: bool) -> Self {
        Self {
            site_named: config.identity.site_name.is_some(),
            nodes_paired: config.transmitter().is_some() && !config.rx_node_ids().is_empty(),
            uplink_decided: config.network.uplink.is_some(),
            site_captured,
            model_ready: model_installed && config.site.is_some(),
        }
    }

    /// The first step that is not satisfied, or [`Stage::Complete`].
    #[must_use]
    pub fn stage(&self) -> Stage {
        if !self.site_named {
            Stage::Site
        } else if !self.nodes_paired {
            Stage::Nodes
        } else if !self.uplink_decided {
            Stage::Network
        } else if !self.site_captured {
            Stage::Calibration
        } else {
            Stage::Complete
        }
    }

    /// Whether every step is satisfied.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.stage() == Stage::Complete
    }

    /// Checks that the installation may be closed.
    ///
    /// # Errors
    ///
    /// [`TransitionError::Incomplete`] naming the outstanding step.
    pub fn ensure_complete(&self) -> Result<(), TransitionError> {
        match self.stage() {
            Stage::Complete => Ok(()),
            stage => Err(TransitionError::Incomplete { stage }),
        }
    }
}

/// What the appliance is doing at the product level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum Phase {
    /// The guided installation is still running.
    Onboarding {
        /// The step to present to the installer.
        stage: Stage,
    },
    /// The installation is closed; the appliance is in normal service.
    Operational,
}

impl Phase {
    /// Derives the phase from the facts and the stored completion flag.
    #[must_use]
    pub fn of(readiness: Readiness, onboarding_completed: bool) -> Self {
        if onboarding_completed {
            Self::Operational
        } else {
            Self::Onboarding {
                stage: readiness.stage(),
            }
        }
    }
}

/// Exclusive use of the CSI stream.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum RuntimeMode {
    /// Nothing consumes the stream.
    #[default]
    Idle,
    /// A labeled capture session is recording.
    Calibrating {
        /// Session being recorded.
        session_id: String,
        /// When the capture began, so a screen reloaded mid-capture still
        /// shows how long it has been running.
        started_us: u64,
    },
    /// The live pipeline is producing wait-time estimates.
    Live,
}

impl RuntimeMode {
    /// Short label used in diagnostics and error messages.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Calibrating { .. } => "calibration",
            Self::Live => "live",
        }
    }

    /// Whether the stream is currently claimed.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// Guard over the single CSI stream.
///
/// Only one activity may hold the stream. Switching is deliberate: stop,
/// then start. That is what keeps a stray "start live" request from
/// silently killing a calibration session an installer is halfway through.
#[derive(Debug, Clone, Default)]
pub struct Runtime {
    mode: RuntimeMode,
}

impl Runtime {
    /// A runtime with an idle stream.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current mode.
    #[must_use]
    pub fn mode(&self) -> &RuntimeMode {
        &self.mode
    }

    /// Claims the stream for a calibration session.
    ///
    /// # Errors
    ///
    /// [`TransitionError::StreamBusy`] if anything already holds it.
    pub fn start_calibration(
        &mut self,
        session_id: impl Into<String>,
        started_us: u64,
    ) -> Result<(), TransitionError> {
        self.ensure_idle()?;
        self.mode = RuntimeMode::Calibrating {
            session_id: session_id.into(),
            started_us,
        };
        Ok(())
    }

    /// Claims the stream for live inference.
    ///
    /// # Errors
    ///
    /// [`TransitionError::StreamBusy`] if anything already holds it, or
    /// [`TransitionError::NoModel`] when no model is ready — an estimate
    /// without a model is not a degraded estimate, it is no estimate.
    pub fn start_live(&mut self, model_ready: bool) -> Result<(), TransitionError> {
        self.ensure_idle()?;
        if !model_ready {
            return Err(TransitionError::NoModel);
        }
        self.mode = RuntimeMode::Live;
        Ok(())
    }

    /// Releases the stream, returning what was holding it.
    pub fn stop(&mut self) -> RuntimeMode {
        std::mem::replace(&mut self.mode, RuntimeMode::Idle)
    }

    fn ensure_idle(&self) -> Result<(), TransitionError> {
        if self.mode.is_busy() {
            return Err(TransitionError::StreamBusy {
                current: self.mode.label(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use flow_core::NodeRole;

    use super::*;
    use crate::config::{PairedNode, SiteTuning, Uplink};

    fn configured() -> ApplianceConfig {
        let mut config =
            ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", "correct-horse-battery");
        config.identity.site_name = Some("RU EFREI".into());
        config.nodes = vec![
            PairedNode {
                node_id: "tx-1".into(),
                role: NodeRole::Tx,
                mac: Some("1a:00:00:00:00:00".into()),
                address: None,
                position: None,
            },
            PairedNode {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                mac: Some("aa:bb:cc:00:00:01".into()),
                address: Some("192.168.4.51".parse().unwrap()),
                position: None,
            },
        ];
        config.network.uplink = Some(Uplink::Offline);
        config.site = Some(SiteTuning {
            people_per_class: [0.0, 4.0, 12.0, 25.0],
            service_rate_per_min: 6.0,
            smoothing_tau_s: 30.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
            window_us: 5_000_000,
            hop_us: 1_000_000,
        });
        config
    }

    #[test]
    fn a_factory_appliance_starts_at_the_first_step() {
        let config = ApplianceConfig::factory("KIT-0001", "ssid-0001", "passphrase");
        let readiness = Readiness::evaluate(&config, false, false);
        assert_eq!(readiness.stage(), Stage::Site);
        assert_eq!(
            Phase::of(readiness, config.onboarding_completed),
            Phase::Onboarding { stage: Stage::Site }
        );
    }

    #[test]
    fn the_wizard_advances_one_fact_at_a_time() {
        let full = configured();

        let mut config = ApplianceConfig::factory("KIT-0001", "ssid-0001", "passphrase");
        assert_eq!(
            Readiness::evaluate(&config, true, true).stage(),
            Stage::Site
        );

        config.identity.site_name = full.identity.site_name.clone();
        assert_eq!(
            Readiness::evaluate(&config, true, true).stage(),
            Stage::Nodes
        );

        config.nodes = full.nodes.clone();
        assert_eq!(
            Readiness::evaluate(&config, true, true).stage(),
            Stage::Network
        );

        config.network.uplink = full.network.uplink.clone();
        config.site = full.site;
        assert_eq!(
            Readiness::evaluate(&config, true, false).stage(),
            Stage::Calibration,
            "nothing captured at this site yet"
        );

        // Recording is what finishes an installation, not the model: the
        // model comes back from training days later, and holding the wizard
        // open until then would lock the site out of every other screen.
        assert_eq!(
            Readiness::evaluate(&config, false, true).stage(),
            Stage::Complete,
            "captured, model still to come"
        );
    }

    #[test]
    fn staying_offline_counts_as_answering_the_network_question() {
        let mut config = configured();
        config.network.uplink = None;
        assert_eq!(
            Readiness::evaluate(&config, true, true).stage(),
            Stage::Network
        );

        config.network.uplink = Some(Uplink::Offline);
        assert!(Readiness::evaluate(&config, true, true).is_complete());
    }

    #[test]
    fn a_tuned_site_without_a_model_artifact_is_not_ready() {
        let config = configured();
        assert_eq!(
            Readiness::evaluate(&config, false, false).stage(),
            Stage::Calibration
        );
        assert!(Readiness::evaluate(&config, true, true).is_complete());
    }

    #[test]
    fn receivers_alone_do_not_count_as_paired() {
        let mut config = configured();
        config.nodes.retain(|node| node.role == NodeRole::Rx);
        assert_eq!(
            Readiness::evaluate(&config, true, true).stage(),
            Stage::Nodes
        );
    }

    #[test]
    fn onboarding_cannot_be_closed_early_and_names_the_missing_step() {
        let mut config = configured();
        config.identity.site_name = None;
        let readiness = Readiness::evaluate(&config, true, true);
        assert_eq!(
            readiness.ensure_complete(),
            Err(TransitionError::Incomplete { stage: Stage::Site })
        );
    }

    #[test]
    fn a_completed_installation_does_not_fall_back_into_the_wizard() {
        let mut config = configured();
        config.onboarding_completed = true;
        // A node is unplugged after the installation was closed.
        config.nodes.clear();
        let readiness = Readiness::evaluate(&config, true, true);

        assert_eq!(readiness.stage(), Stage::Nodes, "the fact is reported");
        assert_eq!(
            Phase::of(readiness, config.onboarding_completed),
            Phase::Operational,
            "but the appliance stays in service"
        );
    }

    #[test]
    fn the_stream_admits_one_consumer_at_a_time() {
        let mut runtime = Runtime::new();
        assert_eq!(*runtime.mode(), RuntimeMode::Idle);

        runtime.start_calibration("s-001", 0).unwrap();
        assert_eq!(
            runtime.start_live(true),
            Err(TransitionError::StreamBusy {
                current: "calibration"
            })
        );
        assert_eq!(
            runtime.start_calibration("s-002", 0),
            Err(TransitionError::StreamBusy {
                current: "calibration"
            })
        );
        assert_eq!(
            *runtime.mode(),
            RuntimeMode::Calibrating {
                session_id: "s-001".into(),
                started_us: 0,
            },
            "the running session is untouched by refused requests"
        );
    }

    #[test]
    fn switching_requires_stopping_first() {
        let mut runtime = Runtime::new();
        runtime.start_live(true).unwrap();
        assert_eq!(
            runtime.start_calibration("s-001", 0),
            Err(TransitionError::StreamBusy { current: "live" })
        );

        assert_eq!(runtime.stop(), RuntimeMode::Live);
        runtime.start_calibration("s-001", 0).unwrap();
        assert_eq!(runtime.mode().label(), "calibration");
    }

    #[test]
    fn live_inference_refuses_to_start_without_a_model() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.start_live(false), Err(TransitionError::NoModel));
        assert_eq!(*runtime.mode(), RuntimeMode::Idle, "nothing was claimed");
    }

    #[test]
    fn stopping_an_idle_stream_is_harmless() {
        let mut runtime = Runtime::new();
        assert_eq!(runtime.stop(), RuntimeMode::Idle);
        assert_eq!(*runtime.mode(), RuntimeMode::Idle);
    }
}
