//! The device secret: the credential printed on an appliance's label.
//!
//! Every appliance leaves preparation with its own secret, generated here,
//! shown once, and printed on the label (and in the QR code that spares the
//! installer from typing it). The appliance itself only ever stores a hash
//! of it — see [`AdminCredential`](crate::AdminCredential).
//!
//! # Format
//!
//! Twenty characters over a 32-symbol alphabet, displayed in groups of
//! four: `K7M4-9PQR-2WXY-6BTN-3HFD`.
//!
//! The alphabet is Crockford's base32 — the digits plus the uppercase
//! letters, minus `I`, `L`, `O` and `U`. Dropping those four letters is
//! what makes the digits safe to keep: with no `O` there is nothing to
//! confuse `0` with, and with no `I` or `L` nothing to confuse `1` with.
//! (`U` goes for an unrelated reason — its absence keeps generated secrets
//! from spelling anything unfortunate.) The result is exactly 32 symbols,
//! so each character carries exactly 5 bits and no character is ever
//! ambiguous when read off a label.
//!
//! # Strength
//!
//! 20 characters × 5 bits = **100 bits of entropy**, and the number is
//! honest because it describes the *generation process*: each character is
//! drawn uniformly and independently from the alphabet by the operating
//! system's cryptographic generator. Counting the characters of the printed
//! form instead — the "charset entropy" reported by online strength
//! meters — measures nothing, since it would score a string of twenty
//! identical characters just as highly.
//!
//! # Entry
//!
//! [`DeviceSecret::parse`] is deliberately forgiving of transcription:
//! case is ignored, dashes and spaces may be placed anywhere or omitted,
//! and the characters excluded from the alphabet are folded onto the digit
//! they resemble (`I` and `L` to `1`, `O` to `0`). Hashing and verification
//! always use the normalized form, so a secret typed as
//! `k7m4 9pqr2wxy-6btn3hfd` verifies against one printed as
//! `K7M4-9PQR-2WXY-6BTN-3HFD`.

use std::fmt;

use argon2::password_hash::rand_core::{OsRng, RngCore};

use crate::error::SecretError;

/// Crockford base32: digits plus uppercase letters, less `I`, `L`, `O`, `U`.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Number of characters in a device secret.
const SECRET_CHARS: usize = 20;

/// Characters per displayed group.
const GROUP: usize = 4;

/// Entropy of a generated device secret, in bits.
pub const SECRET_ENTROPY_BITS: u32 = 100;

/// A device secret, held in its normalized form.
///
/// `Debug` deliberately redacts the value: a secret that reaches a log
/// line, a panic message or an error report has escaped.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSecret(String);

impl DeviceSecret {
    /// Draws a fresh secret from the operating system's cryptographic
    /// random generator.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0u8; SECRET_CHARS];
        OsRng.fill_bytes(&mut bytes);
        // Taking the low 5 bits of a uniform byte is itself uniform over
        // the alphabet, with no bias to reject: 256 is exactly 8 × 32, so
        // every symbol is reachable from the same number of byte values.
        let text = bytes
            .iter()
            .map(|byte| char::from(ALPHABET[usize::from(byte & 0x1F)]))
            .collect();
        Self(text)
    }

    /// Reads a secret as a human may have typed or dictated it.
    ///
    /// # Errors
    ///
    /// [`SecretError::InvalidCharacter`] for a character that is neither in
    /// the alphabet nor foldable onto it, or [`SecretError::InvalidLength`]
    /// when the result is not exactly [`SECRET_CHARS`] characters long.
    pub fn parse(input: &str) -> Result<Self, SecretError> {
        let mut normalized = String::with_capacity(SECRET_CHARS);
        for raw in input.chars() {
            // Separators are presentation, not content.
            if raw == '-' || raw.is_whitespace() {
                continue;
            }
            let upper = raw.to_ascii_uppercase();
            let folded = match upper {
                'I' | 'L' => '1',
                'O' => '0',
                other => other,
            };
            if !ALPHABET.contains(&u8::try_from(folded).unwrap_or(0)) {
                return Err(SecretError::InvalidCharacter(raw));
            }
            normalized.push(folded);
        }
        if normalized.chars().count() != SECRET_CHARS {
            return Err(SecretError::InvalidLength {
                expected: SECRET_CHARS,
                got: normalized.chars().count(),
            });
        }
        Ok(Self(normalized))
    }

    /// The normalized form — what is hashed and verified.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceSecret {
    /// The printed form: groups of four separated by dashes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, chunk) in self.0.as_bytes().chunks(GROUP).enumerate() {
            if index > 0 {
                f.write_str("-")?;
            }
            f.write_str(std::str::from_utf8(chunk).map_err(|_| fmt::Error)?)?;
        }
        Ok(())
    }
}

impl fmt::Debug for DeviceSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceSecret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn the_alphabet_is_exactly_five_bits_per_character() {
        assert_eq!(ALPHABET.len(), 32);
        assert!(ALPHABET.len().is_power_of_two());
        assert_eq!(
            u32::try_from(SECRET_CHARS).unwrap() * ALPHABET.len().ilog2(),
            SECRET_ENTROPY_BITS
        );
    }

    #[test]
    fn the_alphabet_excludes_every_confusable_letter() {
        for excluded in *b"ILOU" {
            assert!(
                !ALPHABET.contains(&excluded),
                "{} must not be in the alphabet",
                char::from(excluded)
            );
        }
        // The digits stay precisely because their look-alikes are gone.
        assert!(ALPHABET.contains(&b'0') && ALPHABET.contains(&b'1'));

        let unique: HashSet<u8> = ALPHABET.iter().copied().collect();
        assert_eq!(unique.len(), ALPHABET.len(), "no repeated symbol");
    }

    #[test]
    fn a_generated_secret_has_the_printed_shape() {
        let secret = DeviceSecret::generate();
        assert_eq!(secret.as_str().chars().count(), SECRET_CHARS);
        assert!(secret.as_str().bytes().all(|byte| ALPHABET.contains(&byte)));
        let printed = secret.to_string();
        assert_eq!(printed.len(), SECRET_CHARS + 4, "five groups, four dashes");
        assert_eq!(printed.matches('-').count(), 4);
        for group in printed.split('-') {
            assert_eq!(group.len(), GROUP);
        }
    }

    #[test]
    fn generation_uses_the_whole_alphabet_and_does_not_repeat_itself() {
        let mut seen = HashSet::new();
        let mut secrets = HashSet::new();
        for _ in 0..200 {
            let secret = DeviceSecret::generate();
            seen.extend(secret.as_str().bytes());
            secrets.insert(secret.as_str().to_owned());
        }
        // 4000 draws over 32 symbols: a missing symbol would mean the
        // generator is not covering its alphabet.
        assert_eq!(seen.len(), ALPHABET.len(), "every symbol must be reachable");
        assert_eq!(secrets.len(), 200, "generated secrets must not collide");
    }

    #[test]
    fn the_printed_form_round_trips() {
        let secret = DeviceSecret::generate();
        assert_eq!(DeviceSecret::parse(&secret.to_string()).unwrap(), secret);
    }

    #[test]
    fn transcription_slips_are_forgiven() {
        let canonical = DeviceSecret::parse("K7M4-9PQR-2WXY-6BTN-3HFD").unwrap();

        for variant in [
            "k7m4-9pqr-2wxy-6btn-3hfd",   // lowercase
            "K7M49PQR2WXY6BTN3HFD",       // no separators
            "K7M4 9PQR 2WXY 6BTN 3HFD",   // spaces instead of dashes
            "  K7M4-9PQR-2WXY-6BTN-3HFD", // stray whitespace
            "K7M4-9PQR-2WXY-6BTN-3HFD\n",
        ] {
            assert_eq!(
                DeviceSecret::parse(variant).unwrap(),
                canonical,
                "{variant:?} should normalize to the same secret"
            );
        }
    }

    #[test]
    fn look_alike_letters_fold_onto_their_digit() {
        // A reader who transcribes 1 as I or L, or 0 as O, still gets in.
        let canonical = DeviceSecret::parse("1000-0000-0000-0000-0001").unwrap();
        assert_eq!(
            DeviceSecret::parse("IOOO-OOOO-OOOO-OOOO-OOOL").unwrap(),
            canonical
        );
        assert_eq!(
            DeviceSecret::parse("lOOO-oooo-OOOO-oooo-ooo1").unwrap(),
            canonical
        );
    }

    #[test]
    fn a_character_outside_the_alphabet_is_rejected() {
        // U is excluded on purpose and is not folded onto anything.
        assert_eq!(
            DeviceSecret::parse("U7M4-9PQR-2WXY-6BTN-3HFD"),
            Err(SecretError::InvalidCharacter('U'))
        );
        assert_eq!(
            DeviceSecret::parse("K7M4-9PQR-2WXY-6BTN-3HF!"),
            Err(SecretError::InvalidCharacter('!'))
        );
    }

    #[test]
    fn a_secret_of_the_wrong_length_is_rejected() {
        assert_eq!(
            DeviceSecret::parse("K7M4-9PQR-2WXY"),
            Err(SecretError::InvalidLength {
                expected: 20,
                got: 12
            })
        );
        assert_eq!(
            DeviceSecret::parse(""),
            Err(SecretError::InvalidLength {
                expected: 20,
                got: 0
            })
        );
    }

    #[test]
    fn debug_never_reveals_the_secret() {
        let secret = DeviceSecret::parse("K7M4-9PQR-2WXY-6BTN-3HFD").unwrap();
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "DeviceSecret(<redacted>)");
        assert!(!rendered.contains("K7M4"));
        // And the same when nested inside another value, which is how a
        // secret would realistically leak into a log line.
        assert!(!format!("{:?}", Some(secret)).contains("K7M4"));
    }
}
