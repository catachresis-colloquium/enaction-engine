// SPDX-License-Identifier: AGPL-3.0-or-later
//! The two-part contract version and its compatibility rule.
//!
//! This is the workspace's _proved leaf_: the only crate that depends on
//! `creusot-std`, and one that Creusot proves as a whole. A gate that proves
//! every crate carrying the proof library as a whole then proves exactly
//! this crate, with no harness and no module left out.
//!
//! A contract belongs here only when Creusot can translate the whole crate.
//! Code that Creusot cannot translate yet stays in its own crate without
//! contracts, and its obligations are recorded as open in
//! `docs/status/PROOF-NEEDS.adoc`. Do not add a dependency to this crate to
//! make such code fit: the proof scope would stop being the whole crate.
//!
//! The kernel contract (`enaction-contracts`) and the accelerator contract
//! (`enaction-accelerator`) are different contracts with different version
//! values. They share this type and this rule.

use creusot_std::macros::ensures;
use creusot_std::prelude::DeepModel;
#[cfg(all(feature = "serde", not(creusot)))]
use serde::{Deserialize, Serialize};

/// A contract version: a compatibility major and an additive minor.
///
/// Deliberately two-part, not semver. A patch level would imply releases the
/// contracts do not have; what a loader needs is exactly a compatibility
/// major and an additive minor.
#[derive(Debug, Clone, Copy, DeepModel)]
#[cfg_attr(not(creusot), derive(PartialEq, Eq, PartialOrd, Ord, Hash))]
#[cfg_attr(
    all(feature = "serde", not(creusot)),
    derive(Serialize, Deserialize),
    serde(deny_unknown_fields)
)]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
}

impl ContractVersion {
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Whether a host at `self` accepts something declared at `declared`.
    ///
    /// Same major, and the declared minor is at or below the host's: a
    /// package or an operation may use only features the host already knows.
    /// An additive feature that the host lacks is as unloadable as a wrong
    /// major (CAC-KERNEL §6, ADR-0019).
    ///
    /// Creusot checks deductively that the implementation returns exactly
    /// "same major and no newer minor".
    #[must_use]
    #[ensures(result == (self.major == declared.major && declared.minor <= self.minor))]
    pub const fn accepts(self, declared: Self) -> bool {
        self.major == declared.major && declared.minor <= self.minor
    }
}

#[cfg(not(creusot))]
impl std::fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn v(major: u16, minor: u16) -> ContractVersion {
        ContractVersion::new(major, minor)
    }

    #[test]
    fn a_host_accepts_its_own_version_and_older_minors() {
        assert!(v(1, 2).accepts(v(1, 2)));
        assert!(v(1, 2).accepts(v(1, 0)));
    }

    #[test]
    fn a_wrong_major_is_rejected_in_both_directions() {
        assert!(!v(1, 0).accepts(v(2, 0)), "newer major: unknown semantics");
        assert!(
            !v(2, 0).accepts(v(1, 0)),
            "older major: incompatible by rule"
        );
    }

    #[test]
    fn a_newer_minor_than_the_host_knows_is_rejected() {
        // Additive features the host lacks are as unloadable as a wrong major.
        assert!(!v(1, 0).accepts(v(1, 1)));
    }

    #[test]
    fn the_rule_is_the_stated_one_on_every_small_pair() {
        // The Creusot contract says this for all values; this checks the
        // compiled code against the same statement on a small exhaustive
        // domain, so the test suite does not rest on the prover alone.
        for hm in 0..4u16 {
            for hn in 0..4u16 {
                for dm in 0..4u16 {
                    for dn in 0..4u16 {
                        assert_eq!(
                            v(hm, hn).accepts(v(dm, dn)),
                            hm == dm && dn <= hn,
                            "host {hm}.{hn}, declared {dm}.{dn}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn it_prints_as_major_dot_minor() {
        assert_eq!(v(2, 0).to_string(), "2.0");
    }
}
