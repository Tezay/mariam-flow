use core::fmt;

use serde::{Deserialize, Serialize};

use crate::error::InvalidDensityClass;

/// Crowd-density class of the monitored queue zone.
///
/// The four classes are the frozen output space of the density classifier
/// and the unit stored in `labels.ndjson`. On disk and on the wire they are
/// encoded as the integers `0..=3`; the discriminants below are part of the
/// canonical format and must never change.
///
/// The derived ordering follows increasing density
/// (`Empty < Low < Medium < Saturated`), which downstream smoothing and
/// hysteresis logic relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum DensityClass {
    /// No detectable presence in the zone.
    Empty = 0,
    /// Sparse presence, no meaningful queue.
    Low = 1,
    /// Established queue, moderate density.
    Medium = 2,
    /// Zone at or near capacity.
    Saturated = 3,
}

impl DensityClass {
    /// All classes, in increasing density order.
    pub const ALL: [Self; 4] = [Self::Empty, Self::Low, Self::Medium, Self::Saturated];

    /// The canonical integer encoding of this class.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl From<DensityClass> for u8 {
    fn from(class: DensityClass) -> Self {
        class.as_u8()
    }
}

impl TryFrom<u8> for DensityClass {
    type Error = InvalidDensityClass;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Empty),
            1 => Ok(Self::Low),
            2 => Ok(Self::Medium),
            3 => Ok(Self::Saturated),
            other => Err(InvalidDensityClass(other)),
        }
    }
}

impl fmt::Display for DensityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Empty => "empty",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::Saturated => "saturated",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_encoding_is_frozen() {
        assert_eq!(DensityClass::Empty.as_u8(), 0);
        assert_eq!(DensityClass::Low.as_u8(), 1);
        assert_eq!(DensityClass::Medium.as_u8(), 2);
        assert_eq!(DensityClass::Saturated.as_u8(), 3);
    }

    #[test]
    fn u8_round_trip() {
        for class in DensityClass::ALL {
            assert_eq!(DensityClass::try_from(class.as_u8()), Ok(class));
        }
    }

    #[test]
    fn rejects_out_of_range_values() {
        for value in [4u8, 42, u8::MAX] {
            assert_eq!(
                DensityClass::try_from(value),
                Err(InvalidDensityClass(value))
            );
        }
    }

    #[test]
    fn serializes_as_bare_integer() {
        let json = serde_json::to_string(&DensityClass::Medium).unwrap();
        assert_eq!(json, "2");
    }

    #[test]
    fn deserializes_from_bare_integer() {
        let class: DensityClass = serde_json::from_str("3").unwrap();
        assert_eq!(class, DensityClass::Saturated);
    }

    #[test]
    fn deserialization_rejects_out_of_range_values() {
        assert!(serde_json::from_str::<DensityClass>("4").is_err());
    }

    #[test]
    fn ordering_follows_density() {
        assert!(DensityClass::Empty < DensityClass::Low);
        assert!(DensityClass::Low < DensityClass::Medium);
        assert!(DensityClass::Medium < DensityClass::Saturated);
    }

    #[test]
    fn display_names() {
        let names: Vec<String> = DensityClass::ALL.iter().map(ToString::to_string).collect();
        assert_eq!(names, ["empty", "low", "medium", "saturated"]);
    }
}
