// SPDX-License-Identifier: AGPL-3.0-or-later
//! The runner-output envelope `enaction-runner-output/1` and the caller's
//! policy document `enaction-policies/1` (CAC-KERNEL §9).
//!
//! A runner emits the envelope. The caller, never the runner, supplies the
//! policies: a runner that chose its own thresholds could legitimise its own
//! beliefs. Parsing is strict: the envelope and every nested contract type
//! reject unknown members before anything is canonicalised or hashed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::canonical::{self, CanonicalFault, Value};
use crate::package::{EpistemicPackage, validate_package};
use crate::{DepthPolicy, KERNEL_CONTRACT_VERSION, Proposition, StatusPolicy};

/// The contract tag of the runner-output envelope.
pub const RUNNER_OUTPUT_CONTRACT: &str = "enaction-runner-output/1";

/// The contract tag of the caller's policy document.
pub const POLICIES_CONTRACT: &str = "enaction-policies/1";

/// What a runner emits: the epistemic package and the world truth that the
/// package cannot carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerOutput {
    /// Always [`RUNNER_OUTPUT_CONTRACT`].
    pub contract: String,
    pub package: EpistemicPackage,
    /// What is true in the world, as opaque atoms. A set: an atom appears at
    /// most once. Frame declarations, which would let the validator check an
    /// atom against a declared hypothesis, are deferred (completion-plan
    /// step 11 as narrowed by WEEVIL-HOST-ROUTE).
    pub world_truth: Vec<Proposition>,
}

/// The caller's policies, resolved from the profile. Never from the runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    /// Always [`POLICIES_CONTRACT`].
    pub contract: String,
    pub status: StatusPolicy,
    pub depth: DepthPolicy,
}

/// One validation fault in an input document: the stable field path, the
/// rule by stable name, and prose for a human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub path: String,
    pub rule: &'static str,
    pub detail: String,
}

impl Fault {
    #[must_use]
    pub fn new(path: impl Into<String>, rule: &'static str, detail: impl Into<String>) -> Self {
        Fault {
            path: path.into(),
            rule,
            detail: detail.into(),
        }
    }
}

impl From<CanonicalFault> for Fault {
    fn from(fault: CanonicalFault) -> Self {
        Fault {
            path: fault.path().to_string(),
            rule: fault.rule(),
            detail: fault.to_string(),
        }
    }
}

impl std::fmt::Display for Fault {
    /// `<rule>\t<path>\t<detail>`: the line `enaction-validate` prints.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let detail = self.detail.replace(['\t', '\n'], " ");
        write!(f, "{}\t{}\t{}", self.rule, self.path, detail)
    }
}

/// Decode a contract document whose first check is its tag. A tag that
/// differs is refused as `UnknownField` at `contract`, the rule the mind
/// trace contract names for it.
pub(crate) fn check_contract(found: &str, want: &str) -> Option<Fault> {
    (found != want).then(|| {
        Fault::new(
            "contract",
            "UnknownField",
            format!("contract {found:?} is not {want:?}"),
        )
    })
}

/// Decode and check the caller's policy document.
///
/// # Errors
///
/// The faults found. The caller treats them as an invocation failure: the
/// policies are the caller's own configuration, not the input under test.
pub fn parse_policies(bytes: &[u8]) -> Result<Policies, Fault> {
    let policies: Policies = canonical::decode(bytes)?;
    match check_contract(&policies.contract, POLICIES_CONTRACT) {
        Some(fault) => Err(fault),
        None => Ok(policies),
    }
}

/// Parse an envelope strictly and validate it against the caller's
/// policies. On success returns the envelope and its parsed value, whose
/// [`canonical::to_jcs`] bytes are the envelope's canonical bytes.
///
/// # Errors
///
/// Every fault found. A document that is not canonicalisable or not the
/// envelope's shape yields one fault, since nothing beneath it can be read.
pub fn validate_runner_output(
    bytes: &[u8],
    policies: &Policies,
) -> Result<(RunnerOutput, Value), Vec<Fault>> {
    let value = canonical::parse(bytes).map_err(|f| vec![f.into()])?;
    let output: RunnerOutput = canonical::decode_value(&value).map_err(|f| vec![f.into()])?;
    let mut faults = Vec::new();
    if let Some(fault) = check_contract(&output.contract, RUNNER_OUTPUT_CONTRACT) {
        faults.push(fault);
    }
    if let Err(package_faults) = validate_package(
        &output.package,
        KERNEL_CONTRACT_VERSION,
        &policies.depth,
        &policies.status,
    ) {
        faults.extend(
            package_faults
                .into_iter()
                .map(|f| Fault::new(format!("package.{}", f.path()), f.rule(), format!("{f:?}"))),
        );
    }
    let mut atoms = BTreeSet::new();
    for (i, atom) in output.world_truth.iter().enumerate() {
        if !atoms.insert(atom) {
            faults.push(Fault::new(
                format!("world_truth[{i}]"),
                "DuplicateTruthAtom",
                format!("{:?} appears twice", atom.0),
            ));
        }
    }
    if faults.is_empty() {
        Ok((output, value))
    } else {
        Err(faults)
    }
}
