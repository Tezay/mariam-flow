//! Turning observed senders into a pairing an installer can confirm.
//!
//! A receiver is a source address; the transmitter never joins the network and
//! appears only as the MAC inside the frames receivers report, so it is found
//! by agreement between them rather than seen directly (ADR 0016).

use std::collections::HashMap;
use std::net::IpAddr;

use flow_ingest::SenderObservation;
use serde::Serialize;

/// A sender that is streaming but not yet paired.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub address: IpAddr,
    pub datagrams: u64,
    pub datagrams_per_second: f32,
    /// Transmitter MACs it reports having sensed — possibly several.
    pub tx_macs: Vec<String>,
    pub first_seen_us: u64,
    pub last_seen_us: u64,
}

/// One node the appliance offers to pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProposedReceiver {
    pub node_id: String,
    pub address: IpAddr,
}

/// The pairing the appliance offers, for the installer to confirm.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Proposal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_mac: Option<String>,
    /// How many receivers reported that MAC, so a screen can distinguish a
    /// transmitter seen by one node from one seen by all of them.
    pub tx_agreement: usize,
    /// In the order they were first heard.
    pub receivers: Vec<ProposedReceiver>,
}

/// What is streaming, and what the appliance suggests doing about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Discovery {
    pub candidates: Vec<Candidate>,
    pub proposal: Proposal,
}

/// Below this, a sender is noise rather than a node.
///
/// A sensing node streams continuously; offering a handful of stray
/// datagrams as a node invites an installer to accept it.
const MIN_DATAGRAMS: u64 = 10;

/// Builds the offer from what the intake has observed.
///
/// `paired` are the node identifiers already in the configuration, so a node
/// replaced on a running installation is offered the first free identifier
/// rather than one that is taken.
#[must_use]
pub fn discover(observations: &[SenderObservation], paired: &[String], now_us: u64) -> Discovery {
    let candidates: Vec<Candidate> = observations
        .iter()
        .filter(|observation| observation.datagrams >= MIN_DATAGRAMS)
        .map(|observation| Candidate {
            address: observation.source,
            datagrams: observation.datagrams,
            datagrams_per_second: rate(observation, now_us),
            tx_macs: observation
                .tx_macs
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            first_seen_us: observation.first_seen_us,
            last_seen_us: observation.last_seen_us,
        })
        .collect();

    let (tx_mac, tx_agreement) = agreed_transmitter(&candidates);
    let receivers = number_receivers(&candidates, paired);

    Discovery {
        candidates,
        proposal: Proposal {
            tx_mac,
            tx_agreement,
            receivers,
        },
    }
}

/// Datagrams per second over the span the sender has been heard for.
///
/// Measured on the observation's own window, never against the wall clock: a
/// replayed capture reconstructs its timestamps and would report a healthy
/// node as idle.
fn rate(observation: &SenderObservation, now_us: u64) -> f32 {
    let end = observation.last_seen_us.max(now_us);
    let span = end.saturating_sub(observation.first_seen_us);
    if span == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let seconds = span as f32 / 1_000_000.0;
    #[allow(clippy::cast_precision_loss)]
    let datagrams = observation.datagrams as f32;
    datagrams / seconds
}

/// The transmitter MAC the most receivers report, and how many reported it.
///
/// A receiver reports every transmitter it sensed, so its own list may hold a
/// passing laptop. Ties break on the MAC, so the offer is stable across
/// requests rather than following hash order.
fn agreed_transmitter(candidates: &[Candidate]) -> (Option<String>, usize) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for candidate in candidates {
        for mac in &candidate.tx_macs {
            *counts.entry(mac.as_str()).or_insert(0) += 1;
        }
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    match ranked.first() {
        Some((mac, count)) => (Some((*mac).to_owned()), *count),
        None => (None, 0),
    }
}

/// Offers `rx-N` identifiers, skipping any the configuration already holds.
fn number_receivers(candidates: &[Candidate], paired: &[String]) -> Vec<ProposedReceiver> {
    let mut taken: Vec<String> = paired.to_vec();
    candidates
        .iter()
        .map(|candidate| {
            let node_id = first_free(&taken);
            taken.push(node_id.clone());
            ProposedReceiver {
                node_id,
                address: candidate.address,
            }
        })
        .collect()
}

fn first_free(taken: &[String]) -> String {
    (1..)
        .map(|n| format!("rx-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| "rx-1".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 10_000_000;

    fn observed(last_octet: u8, datagrams: u64, macs: &[&str]) -> SenderObservation {
        SenderObservation {
            source: IpAddr::from([192, 168, 4, last_octet]),
            tx_macs: macs.iter().map(|mac| mac.parse().unwrap()).collect(),
            datagrams,
            first_seen_us: 0,
            last_seen_us: NOW,
            sampled: 0,
        }
    }

    const TX: &str = "1a:00:00:00:00:00";
    const STRAY: &str = "de:ad:be:ef:00:01";

    #[test]
    fn two_receivers_and_the_transmitter_they_agree_on() {
        let discovery = discover(
            &[observed(51, 500, &[TX]), observed(52, 480, &[TX])],
            &[],
            NOW,
        );

        assert_eq!(discovery.proposal.tx_mac.as_deref(), Some(TX));
        assert_eq!(discovery.proposal.tx_agreement, 2);
        assert_eq!(
            discovery.proposal.receivers,
            vec![
                ProposedReceiver {
                    node_id: "rx-1".into(),
                    address: IpAddr::from([192, 168, 4, 51])
                },
                ProposedReceiver {
                    node_id: "rx-2".into(),
                    address: IpAddr::from([192, 168, 4, 52])
                },
            ]
        );
    }

    #[test]
    fn the_transmitter_is_the_one_the_receivers_share_not_the_loudest_stray() {
        // rx-1 also sensed a passing device. Only the MAC both receivers
        // report can be the transmitter lighting up the room they watch.
        let discovery = discover(
            &[observed(51, 500, &[STRAY, TX]), observed(52, 480, &[TX])],
            &[],
            NOW,
        );

        assert_eq!(discovery.proposal.tx_mac.as_deref(), Some(TX));
        assert_eq!(discovery.proposal.tx_agreement, 2);
    }

    #[test]
    fn a_lone_receiver_still_offers_what_it_saw() {
        // Agreement of one is all there is during an installation where the
        // second receiver is not powered yet; the installer confirms it.
        let discovery = discover(&[observed(51, 500, &[TX])], &[], NOW);

        assert_eq!(discovery.proposal.tx_mac.as_deref(), Some(TX));
        assert_eq!(discovery.proposal.tx_agreement, 1);
    }

    #[test]
    fn nothing_is_offered_as_a_transmitter_when_no_mac_was_read() {
        let discovery = discover(&[observed(51, 500, &[])], &[], NOW);

        assert!(discovery.proposal.tx_mac.is_none());
        assert_eq!(discovery.proposal.tx_agreement, 0);
        assert_eq!(discovery.proposal.receivers.len(), 1);
    }

    #[test]
    fn stray_datagrams_are_not_offered_as_a_node() {
        let discovery = discover(
            &[observed(51, 500, &[TX]), observed(99, 3, &[STRAY])],
            &[],
            NOW,
        );

        assert_eq!(discovery.candidates.len(), 1);
        assert_eq!(
            discovery.candidates[0].address,
            IpAddr::from([192, 168, 4, 51])
        );
    }

    #[test]
    fn a_replacement_node_is_offered_the_first_free_identifier() {
        // rx-1 and rx-3 are already configured; the box just plugged in
        // must not be offered an identifier that is taken.
        let discovery = discover(
            &[observed(52, 500, &[TX])],
            &["rx-1".to_owned(), "rx-3".to_owned()],
            NOW,
        );

        assert_eq!(discovery.proposal.receivers[0].node_id, "rx-2");
    }

    #[test]
    fn several_replacements_are_numbered_without_collision() {
        let discovery = discover(
            &[observed(52, 500, &[TX]), observed(53, 500, &[TX])],
            &["rx-1".to_owned()],
            NOW,
        );

        let ids: Vec<&str> = discovery
            .proposal
            .receivers
            .iter()
            .map(|receiver| receiver.node_id.as_str())
            .collect();
        assert_eq!(ids, vec!["rx-2", "rx-3"]);
    }

    #[test]
    fn the_same_observations_always_yield_the_same_offer() {
        // Two MACs reported once each: without a tie-break the offer would
        // follow hash order and change between requests.
        let observations = [observed(51, 500, &[STRAY]), observed(52, 500, &[TX])];
        let first = discover(&observations, &[], NOW);
        let second = discover(&observations, &[], NOW);

        assert_eq!(first.proposal, second.proposal);
        assert_eq!(first.proposal.tx_mac.as_deref(), Some(TX));
    }

    #[test]
    fn the_rate_is_measured_on_the_observation_window() {
        let discovery = discover(&[observed(51, 1_000, &[TX])], &[], NOW);

        assert!((discovery.candidates[0].datagrams_per_second - 100.0).abs() < 0.5);
    }

    #[test]
    fn nothing_streaming_means_nothing_offered() {
        let discovery = discover(&[], &[], NOW);

        assert!(discovery.candidates.is_empty());
        assert_eq!(discovery.proposal, Proposal::default());
    }
}
