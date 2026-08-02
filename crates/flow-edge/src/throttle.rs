//! Progressive slow-down of failed logins.
//!
//! Verifying a secret costs an Argon2id hash on purpose, which is what
//! makes guessing expensive. That same cost is a liability if the endpoint
//! is left open: every attempt claims 19 MiB and a slice of a slow CPU, so
//! an unthrottled login is both a guessing oracle and a way to exhaust the
//! appliance. Throttling is therefore a requirement of exposing the
//! credential at all, not a refinement (ADR 0010).
//!
//! The policy is a doubling delay per client rather than a lockout. A
//! lockout would hand anyone on the site network a denial of service: fail
//! deliberately a few times and the installer is locked out too. Here the
//! *cost* of guessing explodes — a handful of free attempts, then 1 s, 2 s,
//! 4 s and so on up to a five-minute ceiling — while a legitimate operator
//! who mistypes is only ever delayed, never shut out.
//!
//! Clients are keyed by source address. The appliance sits on its own
//! networks and is never behind a proxy, so the socket address is the
//! honest identity; forwarding headers are attacker-controlled and are
//! deliberately not consulted.

use std::collections::HashMap;
use std::net::IpAddr;

/// Failures tolerated before any delay is imposed.
const FREE_ATTEMPTS: u32 = 3;

/// Delay imposed on the first throttled attempt, in µs.
const BASE_DELAY_US: u64 = 1_000_000;

/// Ceiling on the imposed delay, in µs.
const MAX_DELAY_US: u64 = 5 * 60 * 1_000_000;

/// How long after its block expires a client's history is forgotten, in µs.
///
/// Without this the map would remember every address forever; with it, a
/// client that stops trying eventually starts from a clean slate.
const FORGET_AFTER_US: u64 = 60 * 60 * 1_000_000;

/// Upper bound on tracked clients, so a spread-out attack cannot grow the
/// map without limit.
const MAX_CLIENTS: usize = 1024;

#[derive(Debug, Clone, Copy, Default)]
struct Entry {
    failures: u32,
    blocked_until_us: u64,
    /// Attempts refused during the block in progress, beyond the first.
    ///
    /// Counted rather than journalled one by one: a refusal costs the client
    /// a round trip and nothing else, so writing a row for each hands anyone
    /// on the network a way to evict the whole journal — including the record
    /// of their own attempts.
    suppressed: u32,
    /// Whether the block in progress has already been recorded.
    announced: bool,
}

/// Why an attempt was refused before it was even checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// Remaining wait, in µs.
    pub wait_us: u64,
    /// Whether this is the first refusal of the block in progress.
    pub first: bool,
}

/// Per-client failure history.
#[derive(Debug, Default)]
pub struct Throttle {
    clients: HashMap<IpAddr, Entry>,
}

impl Throttle {
    /// An empty throttle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `client` may attempt a login now.
    ///
    /// # Errors
    ///
    /// [`Refusal`], carrying the remaining wait and whether this refusal is
    /// the first of the block — which is the only one worth a journal entry.
    pub fn check(&mut self, client: IpAddr, now_us: u64) -> Result<(), Refusal> {
        let Some(entry) = self.clients.get_mut(&client) else {
            return Ok(());
        };
        if entry.blocked_until_us <= now_us {
            return Ok(());
        }
        let first = !entry.announced;
        if first {
            entry.announced = true;
        } else {
            entry.suppressed = entry.suppressed.saturating_add(1);
        }
        Err(Refusal {
            wait_us: entry.blocked_until_us - now_us,
            first,
        })
    }

    /// Takes the attempts refused silently since the last time this was asked.
    ///
    /// Reported as one line rather than as none: a journal that hides a flood
    /// is as misleading as one drowned by it.
    pub fn take_suppressed(&mut self, client: IpAddr) -> u32 {
        self.clients
            .get_mut(&client)
            .map_or(0, |entry| std::mem::take(&mut entry.suppressed))
    }

    /// Records a failed attempt and returns the delay now imposed, in µs.
    pub fn record_failure(&mut self, client: IpAddr, now_us: u64) -> u64 {
        self.forget_stale(now_us);
        let entry = self.clients.entry(client).or_default();
        entry.failures = entry.failures.saturating_add(1);

        let delay = delay_for(entry.failures);
        entry.blocked_until_us = now_us.saturating_add(delay);
        // A fresh block is a fresh thing to say.
        entry.announced = false;
        delay
    }

    /// Clears a client's history after a successful login.
    pub fn record_success(&mut self, client: IpAddr) {
        self.clients.remove(&client);
    }

    /// Number of clients currently tracked.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.clients.len()
    }

    fn forget_stale(&mut self, now_us: u64) {
        self.clients
            .retain(|_, entry| now_us.saturating_sub(entry.blocked_until_us) < FORGET_AFTER_US);
        if self.clients.len() >= MAX_CLIENTS {
            // Evict whoever is closest to being forgotten anyway.
            let oldest = self
                .clients
                .iter()
                .min_by_key(|(_, entry)| entry.blocked_until_us)
                .map(|(client, _)| *client);
            if let Some(client) = oldest {
                self.clients.remove(&client);
            }
        }
    }
}

/// Delay imposed after `failures` consecutive failures.
///
/// The first [`FREE_ATTEMPTS`] cost nothing — mistyping a twenty-character
/// secret is ordinary. After that the delay doubles each time, up to
/// [`MAX_DELAY_US`].
fn delay_for(failures: u32) -> u64 {
    if failures <= FREE_ATTEMPTS {
        return 0;
    }
    let steps = failures - FREE_ATTEMPTS - 1;
    BASE_DELAY_US
        .checked_shl(steps)
        .unwrap_or(MAX_DELAY_US)
        .min(MAX_DELAY_US)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;

    fn client(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 4, last))
    }

    #[test]
    fn the_first_attempts_are_free() {
        let mut throttle = Throttle::new();
        let client = client(10);

        for _ in 0..FREE_ATTEMPTS {
            assert!(throttle.check(client, NOW).is_ok());
            assert_eq!(throttle.record_failure(client, NOW), 0);
        }
        assert!(
            throttle.check(client, NOW).is_ok(),
            "a mistyped secret must not cost anything"
        );
    }

    #[test]
    fn the_delay_doubles_and_then_stops_growing() {
        assert_eq!(delay_for(FREE_ATTEMPTS), 0);
        assert_eq!(delay_for(FREE_ATTEMPTS + 1), 1_000_000);
        assert_eq!(delay_for(FREE_ATTEMPTS + 2), 2_000_000);
        assert_eq!(delay_for(FREE_ATTEMPTS + 3), 4_000_000);
        assert_eq!(delay_for(FREE_ATTEMPTS + 9), 256_000_000);
        assert_eq!(delay_for(FREE_ATTEMPTS + 10), MAX_DELAY_US, "capped");
        assert_eq!(delay_for(1_000), MAX_DELAY_US, "and stays capped");
    }

    #[test]
    fn a_blocked_client_is_told_how_long_to_wait() {
        let mut throttle = Throttle::new();
        let client = client(10);
        for _ in 0..=FREE_ATTEMPTS {
            throttle.record_failure(client, NOW);
        }

        assert_eq!(throttle.check(client, NOW).unwrap_err().wait_us, 1_000_000);
        assert_eq!(
            throttle.check(client, NOW + 400_000).unwrap_err().wait_us,
            600_000
        );
        assert!(
            throttle.check(client, NOW + 1_000_000).is_ok(),
            "the block lifts on its own"
        );
    }

    #[test]
    fn one_client_cannot_lock_another_out() {
        let mut throttle = Throttle::new();
        let attacker = client(66);
        for _ in 0..20 {
            throttle.record_failure(attacker, NOW);
        }
        assert!(throttle.check(attacker, NOW).is_err());
        assert!(
            throttle.check(client(10), NOW).is_ok(),
            "the installer must stay able to log in"
        );
    }

    #[test]
    fn a_legitimate_operator_is_never_shut_out_permanently() {
        let mut throttle = Throttle::new();
        let client = client(10);
        for _ in 0..1_000 {
            throttle.record_failure(client, NOW);
        }
        // However long the attack ran, the wait is bounded.
        assert_eq!(
            throttle.check(client, NOW).unwrap_err().wait_us,
            MAX_DELAY_US
        );
        assert!(throttle.check(client, NOW + MAX_DELAY_US).is_ok());
    }

    #[test]
    fn success_clears_the_history() {
        let mut throttle = Throttle::new();
        let client = client(10);
        for _ in 0..10 {
            throttle.record_failure(client, NOW);
        }
        assert!(throttle.check(client, NOW).is_err());

        throttle.record_success(client);
        assert!(throttle.check(client, NOW).is_ok());
        assert_eq!(throttle.tracked(), 0);
    }

    #[test]
    fn a_client_that_stops_trying_is_forgotten() {
        let mut throttle = Throttle::new();
        throttle.record_failure(client(10), NOW);
        assert_eq!(throttle.tracked(), 1);

        // Any later failure sweeps histories that have gone quiet.
        throttle.record_failure(client(11), NOW + FORGET_AFTER_US * 2);
        assert_eq!(throttle.tracked(), 1, "the quiet client was forgotten");
    }

    #[test]
    fn tracking_is_bounded_under_a_spread_out_attack() {
        let mut throttle = Throttle::new();
        for index in 0..(MAX_CLIENTS + 200) {
            let octets = u32::try_from(index).unwrap().to_be_bytes();
            let address = IpAddr::V4(Ipv4Addr::new(10, octets[1], octets[2], octets[3]));
            throttle.record_failure(address, NOW);
        }
        assert!(
            throttle.tracked() <= MAX_CLIENTS,
            "tracked {} clients",
            throttle.tracked()
        );
    }
    #[test]
    fn only_the_first_refusal_of_a_block_is_worth_recording() {
        // A refusal is decided before the hash is computed, so it costs the
        // client a round trip and nothing else. One journal row per request
        // would let anyone on the network evict the whole journal — including
        // the record of their own attempts.
        let mut throttle = Throttle::new();
        let client = client(10);
        for _ in 0..=FREE_ATTEMPTS {
            throttle.record_failure(client, NOW);
        }

        assert!(throttle.check(client, NOW).unwrap_err().first);
        for _ in 0..500 {
            assert!(!throttle.check(client, NOW).unwrap_err().first);
        }

        assert_eq!(throttle.take_suppressed(client), 500);
        assert_eq!(throttle.take_suppressed(client), 0, "counted twice");
    }

    #[test]
    fn a_fresh_block_is_a_fresh_thing_to_say() {
        let mut throttle = Throttle::new();
        let client = client(10);
        for _ in 0..=FREE_ATTEMPTS {
            throttle.record_failure(client, NOW);
        }
        assert!(throttle.check(client, NOW).unwrap_err().first);
        assert!(!throttle.check(client, NOW).unwrap_err().first);

        // Another wrong secret earns another block, which is a new event.
        throttle.record_failure(client, NOW);
        assert!(throttle.check(client, NOW).unwrap_err().first);
    }
}
