// SPDX-License-Identifier: AGPL-3.0-or-later
//! The normative reduction contract (CAC-KERNEL §2), as a trait.
//!
//! This module holds the *signature* and the replay harness. It deliberately
//! contains no reduction implementation: the working evidence ledger stays in
//! IDApTIK until ADR-0017's extraction trigger fires, and shipping a reducer
//! here would violate ADR-0012's rule against inventing behaviour ahead of a
//! proving ground.

use serde::{Deserialize, Serialize};

use crate::DepthPolicy;
use crate::StatusPolicy;
use crate::belief::{AffectAnnotation, Belief, EpistemicEvent};
use crate::version::{ContractVersion, ProfileRef};

/// A diagnostic: the stable field path and the rule that failed (CAC-KERNEL
/// §6), plus prose for a human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    /// Stable field path, e.g. `"events[3].confidence"`.
    pub path: String,
    /// The compatibility or contract rule that failed, by name.
    pub rule: String,
    pub message: String,
}

/// Something a reduction derived from an event.
///
/// CAC-KERNEL §2 also names `Hypothesis` and `Intention`; nothing in the
/// estate defines their shape yet, and inventing one here would be exactly
/// the speculation ADR-0012 forbids. The enum is `#[non_exhaustive]` so they
/// can arrive as additive minors when a proving ground produces them.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Derived {
    Belief(Belief),
    Affect(AffectAnnotation),
}

/// Everything a reduction may lawfully consult besides the prior state and
/// the event itself. If it is not in here, it MUST NOT influence the result —
/// that is what makes replay possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReductionContext {
    pub kernel: ContractVersion,
    pub profile: ProfileRef,
    pub depth: DepthPolicy,
    pub status_policy: StatusPolicy,
}

/// The result of one reduction step (CAC-KERNEL §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReductionResult<S> {
    pub state: S,
    pub derived: Vec<Derived>,
    /// The step's entry in the engine trace. The entry type is the trace
    /// crate's — one authority for what a trace event is.
    pub trace_entry: enaction_trace::TraceEvent,
    pub diagnostics: Vec<Diagnostic>,
}

/// The §2 normative reduction contract.
///
/// MUSTs, enforced by the conformance harness rather than the type system:
///
/// * the reduction is a pure function of `(kernel version, profile version,
///   prior state, event, context)`;
/// * for identical inputs the result is canonically equivalent byte for byte
///   (ADR-0015);
/// * wall-clock time, process identity, mailbox arrival order, hash-map
///   iteration order and renderer state MUST NOT alter the result;
/// * the reduction is event-sourced: a snapshot is an optimisation and MUST
///   be reconstructible from the ordered event trace.
pub trait EpistemicReduce {
    type State: Clone + PartialEq;

    fn reduce(
        &self,
        prior: &Self::State,
        event: &EpistemicEvent,
        ctx: &ReductionContext,
    ) -> ReductionResult<Self::State>;
}

/// Fold `events` through `reducer` from `initial`, in the order given.
///
/// The caller supplies events already in `(tick, sequence)` order — packages
/// are validated to be so — and the executable half of the determinism MUST
/// is that folding the same inputs twice yields equal states. This crate's
/// toy-reducer test does exactly that; consumers' conformance tests should
/// too.
pub fn replay<R: EpistemicReduce>(
    reducer: &R,
    initial: R::State,
    events: &[EpistemicEvent],
    ctx: &ReductionContext,
) -> R::State {
    events.iter().fold(initial, |state, event| {
        reducer.reduce(&state, event, ctx).state
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::belief::{EventId, EventKind, Proposition, Provenance, Subject};
    use crate::version::KERNEL_CONTRACT_VERSION;
    use enaction_trace::{Mass, TropeId};
    use std::collections::BTreeMap;

    /// A toy reducer: total confidence committed per proposition. Enough to
    /// exercise the trait shape and the replay harness; NOT a model of the
    /// evidence ledger.
    struct TotalledMass;

    impl EpistemicReduce for TotalledMass {
        type State = BTreeMap<String, Mass>;

        fn reduce(
            &self,
            prior: &Self::State,
            event: &EpistemicEvent,
            _ctx: &ReductionContext,
        ) -> ReductionResult<Self::State> {
            let mut state = prior.clone();
            let entry = state
                .entry(event.proposition.0.clone())
                .or_insert(Mass::ZERO);
            *entry = entry.saturating_add(event.confidence);
            ReductionResult {
                state,
                derived: vec![],
                trace_entry: enaction_trace::TraceEvent {
                    id: event.event_id.0,
                    tick: event.tick,
                    trope: TropeId(1),
                    value_milli: i32::from(event.confidence.get()),
                    confidence: Some(event.confidence),
                    revision: Default::default(),
                    caused_by: vec![],
                },
                diagnostics: vec![],
            }
        }
    }

    fn ctx() -> ReductionContext {
        ReductionContext {
            kernel: KERNEL_CONTRACT_VERSION,
            profile: ProfileRef {
                id: "test/esm/v1".into(),
                version: ContractVersion { major: 1, minor: 0 },
                targets_kernel: KERNEL_CONTRACT_VERSION,
            },
            depth: DepthPolicy::default(),
            status_policy: StatusPolicy {
                id: "test/status/v1".into(),
                rejected_ceiling: Mass::new(1_000).unwrap(),
                unknown_band: Mass::new(6_000).unwrap(),
                believed_floor: Mass::new(7_000).unwrap(),
                suspected_floor: Mass::new(5_000).unwrap(),
            },
        }
    }

    fn observation(id: u64, tick: u64, proposition: &str, confidence: u16) -> EpistemicEvent {
        EpistemicEvent {
            event_id: EventId(id),
            sequence: id,
            tick,
            observer: "guard".into(),
            subject: Subject::World,
            kind: EventKind::Observation,
            proposition: Proposition(proposition.into()),
            confidence: Mass::new(confidence).unwrap(),
            provenance: Provenance {
                origin: "sight".into(),
                channel: None,
            },
            affect: None,
            conation: None,
        }
    }

    #[test]
    fn replaying_the_same_events_yields_equal_states() {
        let events = vec![
            observation(1, 1, "door open", 3_000),
            observation(2, 1, "door open", 4_000),
            observation(3, 2, "light on", 9_000),
        ];
        let first = replay(&TotalledMass, BTreeMap::new(), &events, &ctx());
        let second = replay(&TotalledMass, BTreeMap::new(), &events, &ctx());
        assert_eq!(first, second, "determinism: same inputs, equal outputs");
        assert_eq!(first["door open"], Mass::new(7_000).unwrap());
    }

    #[test]
    fn a_belief_does_not_move_when_no_event_names_its_holder() {
        // The provenance invariant in miniature, mirroring the trace crate's
        // Sally-Anne: state about "door open" is untouched by evidence about
        // something else.
        let prior = replay(
            &TotalledMass,
            BTreeMap::new(),
            &[observation(1, 1, "door open", 3_000)],
            &ctx(),
        );
        let after = replay(
            &TotalledMass,
            prior.clone(),
            &[observation(2, 2, "light on", 9_000)],
            &ctx(),
        );
        assert_eq!(
            after["door open"], prior["door open"],
            "no evidence, no movement"
        );
    }
}
