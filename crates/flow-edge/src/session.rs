//! Sessions established by a successful login.
//!
//! Sessions live in memory only. That is a deliberate choice rather than a
//! shortcut: nothing bearer-shaped is written to the card, and a reboot
//! ends every session — an appliance that has just been power-cycled is not
//! one anybody should still be logged into. The cost is that a power cut
//! mid-installation means logging in again, which the QR code on the label
//! makes a two-second affair.
//!
//! Two clocks bound a session, because they answer different questions:
//!
//! - the **idle timeout** expires a session left untouched, which is what
//!   protects a phone put down on a counter and forgotten;
//! - the **absolute lifetime** expires a session no matter how actively it
//!   is used, so a stolen token cannot be kept alive indefinitely by an
//!   attacker who simply keeps using it.
//!
//! Tokens are 256 bits from the system's cryptographic generator. They are
//! opaque bearer values: anything that holds one is treated as the
//! administrator, which is why they never leave the `HttpOnly` cookie they
//! are delivered in.

use std::collections::HashMap;

use argon2::password_hash::rand_core::{OsRng, RngCore};

/// A session left untouched for this long expires, in µs.
pub const IDLE_TIMEOUT_US: u64 = 12 * 60 * 60 * 1_000_000;

/// A session expires this long after it was opened however active it is,
/// in µs.
pub const ABSOLUTE_LIFETIME_US: u64 = 7 * 24 * 60 * 60 * 1_000_000;

/// Upper bound on concurrently valid sessions.
///
/// An appliance serves a handful of people. The cap keeps a flood of
/// logins from growing the map without limit on a 512 MB machine; expired
/// entries are dropped first, and only then the least recently used.
const MAX_SESSIONS: usize = 64;

/// Bytes of randomness per session token.
const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, Copy)]
struct Session {
    created_us: u64,
    last_seen_us: u64,
}

/// The set of sessions currently valid.
#[derive(Debug)]
pub struct SessionStore {
    sessions: HashMap<String, Session>,
    idle_timeout_us: u64,
    absolute_lifetime_us: u64,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(IDLE_TIMEOUT_US, ABSOLUTE_LIFETIME_US)
    }
}

impl SessionStore {
    /// Builds a store with the given bounds.
    #[must_use]
    pub fn new(idle_timeout_us: u64, absolute_lifetime_us: u64) -> Self {
        Self {
            sessions: HashMap::new(),
            idle_timeout_us,
            absolute_lifetime_us,
        }
    }

    /// Opens a session and returns its token.
    ///
    /// The token is returned once and never stored anywhere else; the
    /// store keeps only the key it is looked up by.
    pub fn open(&mut self, now_us: u64) -> String {
        self.drop_expired(now_us);
        if self.sessions.len() >= MAX_SESSIONS {
            self.drop_least_recently_used();
        }
        let token = random_token();
        self.sessions.insert(
            token.clone(),
            Session {
                created_us: now_us,
                last_seen_us: now_us,
            },
        );
        token
    }

    /// Checks a token and, if it is still valid, marks it as just used.
    ///
    /// Touching on every request is what makes the idle timeout a sliding
    /// window rather than a fixed one.
    pub fn touch(&mut self, token: &str, now_us: u64) -> bool {
        let Some(session) = self.sessions.get(token).copied() else {
            return false;
        };
        if self.is_expired(&session, now_us) {
            self.sessions.remove(token);
            return false;
        }
        if let Some(session) = self.sessions.get_mut(token) {
            session.last_seen_us = now_us;
        }
        true
    }

    /// Ends a session. Unknown tokens are ignored, so logging out twice is
    /// harmless.
    pub fn close(&mut self, token: &str) {
        self.sessions.remove(token);
    }

    /// Number of sessions currently held, expired ones included until they
    /// are next swept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether no session is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    fn is_expired(&self, session: &Session, now_us: u64) -> bool {
        now_us.saturating_sub(session.last_seen_us) > self.idle_timeout_us
            || now_us.saturating_sub(session.created_us) > self.absolute_lifetime_us
    }

    fn drop_expired(&mut self, now_us: u64) {
        let expired: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| self.is_expired(session, now_us))
            .map(|(token, _)| token.clone())
            .collect();
        for token in expired {
            self.sessions.remove(&token);
        }
    }

    fn drop_least_recently_used(&mut self) {
        let oldest = self
            .sessions
            .iter()
            .min_by_key(|(_, session)| session.last_seen_us)
            .map(|(token, _)| token.clone());
        if let Some(token) = oldest {
            self.sessions.remove(&token);
        }
    }
}

/// A 256-bit token, hex-encoded so it is safe in a cookie value.
fn random_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(TOKEN_BYTES * 2);
    for byte in bytes {
        use std::fmt::Write;
        // Writing into a String cannot fail; the result is discarded to
        // keep the signature infallible.
        let _ = write!(token, "{byte:02x}");
    }
    token
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;
    const HOUR: u64 = 60 * 60 * 1_000_000;

    #[test]
    fn a_fresh_session_is_valid() {
        let mut store = SessionStore::default();
        let token = store.open(NOW);
        assert!(store.touch(&token, NOW));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn an_unknown_token_is_rejected() {
        let mut store = SessionStore::default();
        store.open(NOW);
        assert!(!store.touch("not-a-token", NOW));
    }

    #[test]
    fn tokens_are_unpredictable_and_never_collide() {
        let mut store = SessionStore::new(u64::MAX, u64::MAX);
        let tokens: HashSet<String> = (0..64).map(|_| store.open(NOW)).collect();
        assert_eq!(tokens.len(), 64, "every token must be distinct");
        for token in &tokens {
            assert_eq!(token.len(), TOKEN_BYTES * 2);
            assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn an_idle_session_expires_and_is_forgotten() {
        let mut store = SessionStore::default();
        let token = store.open(NOW);

        assert!(store.touch(&token, NOW + IDLE_TIMEOUT_US));
        assert!(!store.touch(&token, NOW + IDLE_TIMEOUT_US * 3));
        assert_eq!(store.len(), 0, "an expired session is dropped, not kept");
    }

    #[test]
    fn activity_slides_the_idle_window() {
        let mut store = SessionStore::default();
        let token = store.open(NOW);

        // Used every hour, well past the idle timeout in total elapsed time.
        let mut now = NOW;
        for _ in 0..20 {
            now += HOUR;
            assert!(store.touch(&token, now), "kept alive by use");
        }
        assert!(now - NOW > IDLE_TIMEOUT_US);
    }

    #[test]
    fn the_absolute_lifetime_expires_even_a_busy_session() {
        let mut store = SessionStore::default();
        let token = store.open(NOW);

        let mut now = NOW;
        let mut alive = true;
        for _ in 0..24 * 8 {
            now += HOUR;
            alive = store.touch(&token, now);
            if !alive {
                break;
            }
        }
        assert!(!alive, "constant use must not extend a session forever");
        assert!(now - NOW > ABSOLUTE_LIFETIME_US);
    }

    #[test]
    fn closing_a_session_revokes_it_immediately() {
        let mut store = SessionStore::default();
        let token = store.open(NOW);
        store.close(&token);
        assert!(!store.touch(&token, NOW));
        assert!(store.is_empty());

        store.close(&token); // logging out twice is harmless
    }

    #[test]
    fn opening_a_session_sweeps_expired_ones() {
        let mut store = SessionStore::default();
        let stale = store.open(NOW);
        assert_eq!(store.len(), 1);

        let fresh = store.open(NOW + IDLE_TIMEOUT_US * 2);
        assert_eq!(store.len(), 1, "the stale session was swept");
        assert!(!store.touch(&stale, NOW + IDLE_TIMEOUT_US * 2));
        assert!(store.touch(&fresh, NOW + IDLE_TIMEOUT_US * 2));
    }

    #[test]
    fn the_store_is_bounded() {
        let mut store = SessionStore::new(u64::MAX, u64::MAX);
        let first = store.open(NOW);
        for offset in 1..(MAX_SESSIONS as u64 + 10) {
            store.open(NOW + offset);
        }
        assert_eq!(
            store.len(),
            MAX_SESSIONS,
            "the map cannot grow without limit"
        );
        assert!(
            !store.touch(&first, NOW),
            "the least recently used session is the one evicted"
        );
    }
}
