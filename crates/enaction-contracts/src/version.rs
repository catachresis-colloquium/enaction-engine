// SPDX-License-Identifier: AGPL-3.0-or-later
//! Contract and profile versioning (CAC-KERNEL §5–§6, ADR-0009).

use creusot_std::macros::ensures;
use creusot_std::prelude::DeepModel;
#[cfg(not(creusot))]
use serde::{Deserialize, Serialize};

/// A contract version: the compatibility unit of the CAC kernel.
///
/// Deliberately two-part, not semver. A patch level would imply releases the
/// contract does not have; what loaders need is exactly a compatibility major
/// and an additive minor.
#[derive(Debug, Clone, Copy, DeepModel)]
#[cfg_attr(
    not(creusot),
    derive(PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)
)]
#[cfg_attr(not(creusot), serde(deny_unknown_fields))]
pub struct ContractVersion {
    pub major: u16,
    pub minor: u16,
}

/// The kernel contract version this crate expresses: 2.0 (ADR-0030).
///
/// Bump `minor` for additive change, `major` for anything a v1 loader could
/// misread. The prose contract is `docs/architecture/CAC-KERNEL.adoc`; this
/// constant is its executable identity. Its state is [`KERNEL_CONTRACT_STATE`].
pub const KERNEL_CONTRACT_VERSION: ContractVersion = ContractVersion { major: 2, minor: 0 };

/// The state of a contract version (ADR-0030, "The contract is 2.0, and it
/// has a state").
///
/// While a version is a [`Draft`](ContractState::Draft), its shape and rules
/// may still change with no further bump, and nothing may be published as
/// conforming to it. The release gate (`scripts/check-contract-released.sh`)
/// refuses a release until the state is [`Released`](ContractState::Released).
/// The state is a fact about the host: the wire does not carry it.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(creusot), derive(PartialEq, Eq))]
pub enum ContractState {
    /// The shape and the rules may still change. Not for publication.
    Draft,
    /// The shape and the rules are fixed for this version.
    Released,
}

#[cfg(not(creusot))]
impl ContractState {
    /// The state as the host prints it: `draft` or `released`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ContractState::Draft => "draft",
            ContractState::Released => "released",
        }
    }
}

/// The state of [`KERNEL_CONTRACT_VERSION`]. 2.0 is a draft until the owner
/// rules it released (ADR-0030, stage C).
pub const KERNEL_CONTRACT_STATE: ContractState = ContractState::Draft;

impl ContractVersion {
    /// Whether a host at `self` accepts a package declared at `declared`.
    ///
    /// Same major, and the declared minor is at or below the host's — a
    /// package may use only features the host already knows. CAC-KERNEL §6
    /// mandates rejecting unknown incompatible majors; the minor rule is this
    /// crate's ruling (recorded in ADR-0019) since additive features a host
    /// lacks are exactly as unloadable as a wrong major.
    #[must_use]
    #[ensures(result == (self.major == declared.major && declared.minor <= self.minor))]
    pub fn accepts(self, declared: ContractVersion) -> bool {
        self.major == declared.major && declared.minor <= self.minor
    }
}

#[cfg(not(creusot))]
impl std::fmt::Display for ContractVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A profile reference: game- or scenario-specific vocabulary and policy,
/// declared without changing kernel semantics (CAC-KERNEL §5).
#[cfg(not(creusot))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRef {
    /// Namespaced identifier, e.g. `"idaptik/esm/v1"` or `"slavia/esm/v1"`.
    /// The kernel never interprets the segments.
    pub id: String,
    /// The profile's own version.
    pub version: ContractVersion,
    /// The kernel contract version the profile targets.
    pub targets_kernel: ContractVersion,
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn v(major: u16, minor: u16) -> ContractVersion {
        ContractVersion { major, minor }
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
}
