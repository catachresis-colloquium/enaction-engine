// SPDX-License-Identifier: AGPL-3.0-or-later
//! The estate's ESM/CAC contract, as types.
//!
//! The normative prose is `docs/architecture/CAC-KERNEL.adoc` (promoted out of
//! IDApTIK's `esm-contract-and-repo-boundaries.md`); this crate is its
//! executable expression. The prose stays normative — where the two disagree,
//! the prose wins and the crate has a bug.
//!
//! # What is here
//!
//! * **Versioning** ([`version`]): [`ContractVersion`], the compatibility
//!   rule, and profile references.
//! * **The data model** ([`belief`]): epistemic events, beliefs, provenance,
//!   the belief–plausibility interval and the interval-derived status policy
//!   (ADR-0016), and the affect/conation annotations whose *shapes* enforce
//!   ADR-0014's crossing rules — affect buys attention, never belief;
//!   conation publishes only its argmax.
//! * **The reduction contract** ([`reduce`]): CAC-KERNEL §2 as a trait, with
//!   a replay harness for the determinism MUST.
//! * **Package validation** ([`package`]): everything a package MUST declare
//!   and everything a loader MUST reject, as a validator that reports every
//!   fault. It computes the package digest itself.
//! * **Canonical bytes** ([`canonical`]): RFC 8785 JCS over integer-only
//!   I-JSON (ADR-0024), the bytes every digest and parity check uses.
//! * **The runner seam** ([`envelope`]): the `enaction-runner-output/1`
//!   envelope and the caller's `enaction-policies/1` document, both strict,
//!   validated by the `enaction-validate` binary.
//! * **The mind trace** ([`mind`]): `enaction-mind-trace/1`, its named
//!   rules, and the derived commitment override.
//! * **Models** ([`tom`]): a belief of order 1 or more is compared with the
//!   modelled bearer's own belief of one order less, never with world
//!   truth; [`model_timelines`] derives its verdicts over time (ADR-0028).
//!
//! # What is refused
//!
//! * **No reduction implementation.** The working evidence ledger lives in
//!   IDApTIK until ADR-0017's extraction trigger fires.
//! * **No game vocabulary.** [`Proposition`] is opaque; the kernel never
//!   reads a game word (the same rule as `enaction_trace::Trope::kind`).
//! * **No proposition language.** CAC-KERNEL §8's hardest open problem stays
//!   open; settling it by accident in a type definition would be worse than
//!   not settling it.
//! * **No UMS dependency, ever.** The studio consumes this crate's field
//!   names and bounds as data; the engine must never import the studio
//!   (ADR-0013).
//!
//! The confidence type itself, [`Mass`], lives in `enaction-trace` — a trace
//! event may carry a confidence, and one authority for the type with no
//! dependency cycle beats a tidier-looking layering.

#![forbid(unsafe_code)]

pub mod belief;
pub mod canonical;
pub mod envelope;
pub mod mind;
pub mod package;
pub mod reduce;
pub mod tom;
pub mod version;

pub use belief::{
    AffectAnnotation, Belief, BeliefId, BeliefInterval, BeliefStatus, ConativeAnnotation,
    DepthPolicy, EpistemicEvent, EventId, EventKind, Proposition, Provenance, StatusPolicy,
    Subject,
};
pub use enaction_trace::Mass;
pub use envelope::{
    Fault, POLICIES_CONTRACT, Policies, RUNNER_OUTPUT_CONTRACT, RunnerOutput, parse_policies,
    validate_runner_output,
};
pub use package::{
    EpistemicPackage, PackageFault, PackageManifest, SeedPolicy, package_digest, validate_package,
};
pub use reduce::{Derived, Diagnostic, EpistemicReduce, ReductionContext, ReductionResult};
pub use tom::{
    ModelFault, ModelTimeline, ModelVerdict, Segment, model_timelines, stale_divergence,
};
pub use version::{ContractVersion, KERNEL_CONTRACT_VERSION, ProfileRef};
