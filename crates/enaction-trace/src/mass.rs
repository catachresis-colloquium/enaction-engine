// SPDX-License-Identifier: MPL-2.0
//! The kernel confidence type (ADR-0016).
//!
//! [`Mass`] is a **justification budget** in units of 1/10,000. It is NOT a
//! probability — it does not sum to one across a partition, because tracking
//! *uncommitted* mass separately is the entire point of the substrate it
//! serves. It is NOT a truth value. It is NOT comparable across frames.
//!
//! It lives in this crate, rather than in `enaction-contracts`, because a
//! trace event may carry a confidence and the contracts crate depends on this
//! one — one authority for the type, no cycle.

use serde::{Deserialize, Serialize};

/// A justification budget in units of 1/10,000.
///
/// NOT a probability. NOT a truth value. NOT comparable across frames.
///
/// **A conversion from an intent read to `Mass` is FORBIDDEN in the kernel**
/// (ADR-0016). An intent read is the cardinality of a solution set — a
/// categorically different kind of thing — and the kernel promotes it verbatim
/// alongside `Mass` rather than converting between them. If a numeric conative
/// layer needs a number, a profile declares a named, versioned read policy and
/// records the policy identifier in the trace, so the lossy step is auditable.
/// This comment sits at the definition site because someone will try.
///
/// Stored as `u16` so "someone stored 40,000" is unrepresentable, and combined
/// with widened `u32` arithmetic so combination cannot overflow. Serde
/// deserialization routes through the same bound check as [`Mass::new`], so a
/// hostile or corrupt trace cannot smuggle an out-of-range value in either.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(try_from = "u16", into = "u16")]
pub struct Mass(u16);

/// The value offered to [`Mass`] exceeded its scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MassOutOfRange(pub u16);

impl std::fmt::Display for MassOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mass {} exceeds the scale ceiling of {}",
            self.0,
            Mass::SCALE
        )
    }
}

impl std::error::Error for MassOutOfRange {}

impl Mass {
    /// No justification committed.
    pub const ZERO: Mass = Mass(0);
    /// The whole budget committed.
    pub const FULL: Mass = Mass(Self::SCALE);
    /// Units per whole: 1/10,000ths.
    pub const SCALE: u16 = 10_000;

    /// A mass, if `value` is within scale. A content gap must not panic, so
    /// the out-of-range case is a value, not an abort.
    #[must_use]
    pub fn new(value: u16) -> Option<Mass> {
        (value <= Self::SCALE).then_some(Mass(value))
    }

    /// A mass, clamping anything beyond scale to [`Mass::FULL`].
    ///
    /// For host-side quantisation where clamping is the *declared* policy.
    /// Kernel-side combination never clamps silently — see the saturating
    /// operations, whose saturation point is the scale itself.
    #[must_use]
    pub fn saturating(value: u16) -> Mass {
        Mass(value.min(Self::SCALE))
    }

    /// The raw value, in 1/10,000ths.
    #[must_use]
    pub fn get(self) -> u16 {
        self.0
    }

    /// Combine two budgets, saturating at [`Mass::FULL`].
    ///
    /// Widened `u32` arithmetic internally, so the addition itself cannot
    /// overflow before the saturation check (ADR-0016).
    #[must_use]
    pub fn saturating_add(self, other: Mass) -> Mass {
        let sum = u32::from(self.0) + u32::from(other.0);
        Mass(u16::try_from(sum.min(u32::from(Self::SCALE))).expect("bounded by SCALE"))
    }

    /// Withdraw a budget, saturating at [`Mass::ZERO`].
    #[must_use]
    pub fn saturating_sub(self, other: Mass) -> Mass {
        Mass(self.0.saturating_sub(other.0))
    }
}

impl TryFrom<u16> for Mass {
    type Error = MassOutOfRange;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Mass::new(value).ok_or(MassOutOfRange(value))
    }
}

impl From<Mass> for u16 {
    fn from(mass: Mass) -> u16 {
        mass.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_is_the_hard_ceiling() {
        assert_eq!(Mass::new(10_000), Some(Mass::FULL));
        assert_eq!(Mass::new(10_001), None, "beyond scale is not a mass");
        assert_eq!(Mass::saturating(40_000), Mass::FULL);
        assert_eq!(Mass::new(0), Some(Mass::ZERO));
    }

    #[test]
    fn combination_saturates_instead_of_overflowing() {
        // u16::MAX would overflow un-widened addition of two large masses;
        // the widened path saturates at FULL instead.
        assert_eq!(Mass::FULL.saturating_add(Mass::FULL), Mass::FULL);
        let nearly = Mass::new(9_999).unwrap();
        assert_eq!(nearly.saturating_add(Mass::new(2).unwrap()), Mass::FULL);
        assert_eq!(Mass::ZERO.saturating_sub(Mass::FULL), Mass::ZERO);
        assert_eq!(
            Mass::FULL.saturating_sub(Mass::new(4_000).unwrap()),
            Mass::new(6_000).unwrap()
        );
    }

    #[test]
    fn deserialization_cannot_smuggle_an_out_of_range_value() {
        // The bound is enforced at the serde boundary, not only in `new`:
        // a corrupt or hostile trace fails to parse rather than yielding an
        // invalid Mass.
        assert!(serde_json::from_str::<Mass>("10001").is_err());
        let mass: Mass = serde_json::from_str("10000").unwrap();
        assert_eq!(mass, Mass::FULL);
        assert_eq!(serde_json::to_string(&Mass::FULL).unwrap(), "10000");
    }

    #[test]
    fn ordering_is_total_and_matches_the_raw_scale() {
        let mut masses = [Mass::FULL, Mass::ZERO, Mass::new(5_000).unwrap()];
        masses.sort();
        assert_eq!(
            masses.iter().map(|m| m.get()).collect::<Vec<_>>(),
            vec![0, 5_000, 10_000]
        );
    }
}
