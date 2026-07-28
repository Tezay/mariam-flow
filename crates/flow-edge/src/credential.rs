//! The administrator credential the appliance checks logins against.
//!
//! The appliance never stores the device secret. It stores an Argon2id
//! hash of it, in PHC string form:
//!
//! ```text
//! $argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub...
//! ```
//!
//! Argon2id is a *memory-hard* password hash: verifying one secret
//! deliberately costs both time and a fixed amount of memory, which is what
//! stops an attacker holding the file from trying billions of candidates on
//! commodity graphics hardware. The parameters are recorded inside the
//! string, so raising them later leaves existing credentials verifiable.
//!
//! The credential lives in the data directory rather than in the appliance
//! configuration, because it changes on a different rhythm — a lost label
//! is not a change of site — and because the recovery procedure below
//! rewrites it without touching anything else.
//!
//! # Recovery
//!
//! An appliance whose label has been lost would otherwise be unusable. The
//! escape hatch is physical: write the new secret into a file on the card's
//! boot partition — the one readable from an ordinary desktop — and power
//! the appliance back on. At startup it consumes that file, replaces the
//! credential and deletes the file. Physical possession of the card is the
//! authorisation; nothing is exposed over the network.

use std::path::Path;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};

use crate::error::{CredentialError, StoreError};
use crate::secret::DeviceSecret;
use crate::store::{read_to_string, write_atomic};

/// File name of the credential inside the data directory.
pub const CREDENTIAL_FILE: &str = "admin.json";

/// Memory cost, in KiB. The OWASP baseline for Argon2id.
const ARGON2_MEMORY_KIB: u32 = 19_456;
/// Iteration count.
const ARGON2_ITERATIONS: u32 = 2;
/// Lanes. One, because logins are serialized anyway.
const ARGON2_LANES: u32 = 1;

/// The stored administrator credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminCredential {
    /// Argon2id hash of the device secret, in PHC string form.
    ///
    /// The hash embeds its own salt and cost parameters; there is nothing
    /// else to keep alongside it.
    secret_hash: String,
    /// When this credential was established, in µs since the Unix epoch.
    updated_us: u64,
}

impl AdminCredential {
    /// Hashes `secret` into a fresh credential.
    ///
    /// # Errors
    ///
    /// [`CredentialError::Hashing`] if the hasher rejects the parameters.
    pub fn establish(secret: &DeviceSecret, now_us: u64) -> Result<Self, CredentialError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = hasher()?
            .hash_password(secret.as_str().as_bytes(), &salt)
            .map_err(|err| CredentialError::Hashing(err.to_string()))?
            .to_string();
        Ok(Self {
            secret_hash: hash,
            updated_us: now_us,
        })
    }

    /// Checks a secret as presented by a client.
    ///
    /// Input is normalized first, so the casing and separators a human
    /// typed are irrelevant. Anything that is not a well-formed secret
    /// simply fails: it cannot be the right one.
    ///
    /// The comparison inside the verifier is constant-time, so a wrong
    /// secret reveals nothing about how much of it was right.
    #[must_use]
    pub fn verify(&self, presented: &str) -> bool {
        let Ok(secret) = DeviceSecret::parse(presented) else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(&self.secret_hash) else {
            return false;
        };
        let Ok(argon2) = hasher() else {
            return false;
        };
        argon2
            .verify_password(secret.as_str().as_bytes(), &parsed)
            .is_ok()
    }

    /// When this credential was established, in µs since the Unix epoch.
    #[must_use]
    pub fn updated_us(&self) -> u64 {
        self.updated_us
    }

    /// Reads the credential stored in `data_dir`.
    ///
    /// # Errors
    ///
    /// [`CredentialError::Missing`] when the appliance has not been
    /// provisioned, or the underlying read or parse failure.
    pub fn load(data_dir: &Path) -> Result<Self, CredentialError> {
        let path = data_dir.join(CREDENTIAL_FILE);
        if !path.is_file() {
            return Err(CredentialError::Missing { path });
        }
        let text = read_to_string(&path)?;
        let credential: Self = serde_json::from_str(&text).map_err(|source| StoreError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok(credential)
    }

    /// Writes the credential into `data_dir`, atomically and owner-only.
    ///
    /// # Errors
    ///
    /// The underlying write failure.
    pub fn save(&self, data_dir: &Path) -> Result<(), CredentialError> {
        let path = data_dir.join(CREDENTIAL_FILE);
        let mut text = serde_json::to_string_pretty(self).map_err(|source| StoreError::Parse {
            path: path.clone(),
            source,
        })?;
        text.push('\n');
        write_atomic(&path, &text)?;
        Ok(())
    }
}

/// Outcome of looking for a pending recovery request at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetOutcome {
    /// No recovery file was present; the stored credential still stands.
    None,
    /// The credential was replaced from the recovery file, which has been
    /// consumed.
    Applied,
}

/// Consumes a pending recovery file, if one is present.
///
/// The file holds nothing but the new secret. On success the credential is
/// replaced and the file is deleted, so the secret does not linger in clear
/// text on the card.
///
/// A malformed file is left in place and reported: the operator's intent is
/// unambiguous, and silently ignoring it would leave them locked out with
/// no clue why. The caller decides whether that is fatal — it should not
/// be, since refusing to boot would take a working installation offline
/// over a mistyped secret.
///
/// # Errors
///
/// [`CredentialError::Secret`] if the file does not hold a well-formed
/// secret, or the underlying read or write failure.
pub fn apply_pending_reset(
    reset_file: &Path,
    data_dir: &Path,
    now_us: u64,
) -> Result<ResetOutcome, CredentialError> {
    if !reset_file.is_file() {
        return Ok(ResetOutcome::None);
    }
    let contents = read_to_string(reset_file)?;
    let secret = DeviceSecret::parse(contents.trim())?;

    AdminCredential::establish(&secret, now_us)?.save(data_dir)?;

    std::fs::remove_file(reset_file).map_err(|source| StoreError::Write {
        path: reset_file.to_path_buf(),
        source,
    })?;
    Ok(ResetOutcome::Applied)
}

fn hasher() -> Result<Argon2<'static>, CredentialError> {
    let params = Params::new(ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_LANES, None)
        .map_err(|err| CredentialError::Hashing(err.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;

    fn secret() -> DeviceSecret {
        DeviceSecret::parse("K7M4-9PQR-2WXY-6BTN-3HFD").unwrap()
    }

    #[test]
    fn the_stored_credential_is_a_hash_not_the_secret() {
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        let stored = serde_json::to_string(&credential).unwrap();

        assert!(
            !stored.contains("K7M4"),
            "the secret itself must never be stored"
        );
        assert!(!stored.contains("K7M49PQR2WXY6BTN3HFD"));
        assert!(credential.secret_hash.starts_with("$argon2id$"));
        assert_eq!(credential.updated_us(), NOW);
    }

    #[test]
    fn the_recorded_parameters_are_the_ones_we_chose() {
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        let hash = PasswordHash::new(&credential.secret_hash).unwrap();
        let params = Params::try_from(&hash).unwrap();

        assert_eq!(params.m_cost(), ARGON2_MEMORY_KIB);
        assert_eq!(params.t_cost(), ARGON2_ITERATIONS);
        assert_eq!(params.p_cost(), ARGON2_LANES);
        assert_eq!(hash.version, Some(Version::V0x13.into()));
    }

    #[test]
    fn the_right_secret_verifies_however_it_was_typed() {
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        for presented in [
            "K7M4-9PQR-2WXY-6BTN-3HFD",
            "k7m4-9pqr-2wxy-6btn-3hfd",
            "K7M49PQR2WXY6BTN3HFD",
            " K7M4 9PQR 2WXY 6BTN 3HFD ",
        ] {
            assert!(credential.verify(presented), "{presented:?} should verify");
        }
    }

    #[test]
    fn a_wrong_or_malformed_secret_never_verifies() {
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        for presented in [
            "K7M4-9PQR-2WXY-6BTN-3HFE", // one character off
            "K7M4-9PQR-2WXY-6BTN",      // too short
            "",
            "not a secret at all",
        ] {
            assert!(
                !credential.verify(presented),
                "{presented:?} must not verify"
            );
        }
    }

    #[test]
    fn two_credentials_for_one_secret_differ_by_their_salt() {
        let first = AdminCredential::establish(&secret(), NOW).unwrap();
        let second = AdminCredential::establish(&secret(), NOW).unwrap();

        assert_ne!(
            first.secret_hash, second.secret_hash,
            "a fresh salt per credential hides that two appliances share a secret"
        );
        assert!(first.verify("K7M4-9PQR-2WXY-6BTN-3HFD"));
        assert!(second.verify("K7M4-9PQR-2WXY-6BTN-3HFD"));
    }

    #[test]
    fn a_saved_credential_reloads_and_still_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        credential.save(dir.path()).unwrap();

        let reloaded = AdminCredential::load(dir.path()).unwrap();
        assert_eq!(reloaded, credential);
        assert!(reloaded.verify("K7M4-9PQR-2WXY-6BTN-3HFD"));
    }

    #[test]
    fn an_unprovisioned_appliance_says_so_and_names_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = AdminCredential::load(dir.path()).unwrap_err();
        assert!(matches!(err, CredentialError::Missing { .. }));
        assert!(err.to_string().contains("flow-edge provision"));
        assert!(err.to_string().contains(CREDENTIAL_FILE));
    }

    #[test]
    fn no_recovery_file_leaves_the_credential_alone() {
        let dir = tempfile::tempdir().unwrap();
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        credential.save(dir.path()).unwrap();

        let outcome = apply_pending_reset(&dir.path().join("absent"), dir.path(), NOW).unwrap();
        assert_eq!(outcome, ResetOutcome::None);
        assert_eq!(AdminCredential::load(dir.path()).unwrap(), credential);
    }

    #[test]
    fn a_recovery_file_replaces_the_credential_and_is_consumed() {
        let dir = tempfile::tempdir().unwrap();
        AdminCredential::establish(&secret(), NOW)
            .unwrap()
            .save(dir.path())
            .unwrap();

        let reset_file = dir.path().join("mariam-flow-secret-reset");
        // Written from a desktop: trailing newline, lowercase, no dashes.
        fs::write(&reset_file, "n8ktvzqe4bshp2gr7wxm\n").unwrap();

        let outcome = apply_pending_reset(&reset_file, dir.path(), NOW + 1).unwrap();
        assert_eq!(outcome, ResetOutcome::Applied);
        assert!(
            !reset_file.exists(),
            "the new secret must not linger in clear text"
        );

        let credential = AdminCredential::load(dir.path()).unwrap();
        assert!(credential.verify("N8KT-VZQE-4BSH-P2GR-7WXM"));
        assert!(
            !credential.verify("K7M4-9PQR-2WXY-6BTN-3HFD"),
            "old secret revoked"
        );
        assert_eq!(credential.updated_us(), NOW + 1);
    }

    #[test]
    fn recovery_works_on_an_appliance_that_has_no_credential_yet() {
        let dir = tempfile::tempdir().unwrap();
        let reset_file = dir.path().join("reset");
        fs::write(&reset_file, "N8KT-VZQE-4BSH-P2GR-7WXM").unwrap();

        assert_eq!(
            apply_pending_reset(&reset_file, dir.path(), NOW).unwrap(),
            ResetOutcome::Applied
        );
        assert!(
            AdminCredential::load(dir.path())
                .unwrap()
                .verify("n8ktvzqe4bshp2gr7wxm")
        );
    }

    #[test]
    fn a_malformed_recovery_file_is_reported_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let credential = AdminCredential::establish(&secret(), NOW).unwrap();
        credential.save(dir.path()).unwrap();

        let reset_file = dir.path().join("reset");
        fs::write(&reset_file, "too-short").unwrap();

        let err = apply_pending_reset(&reset_file, dir.path(), NOW).unwrap_err();
        assert!(matches!(err, CredentialError::Secret(_)));
        assert!(
            reset_file.exists(),
            "the operator's request is not discarded"
        );
        assert_eq!(
            AdminCredential::load(dir.path()).unwrap(),
            credential,
            "a mistyped secret must not revoke the working one"
        );
    }
}
