// SPDX-License-Identifier: AGPL-3.0-or-later
//! The belief store: a Global Baseline plus per-perspective Deltas (C2).
//!
//! Chronicles doc 20's rule, generalised: *belief is baseline plus own
//! deltas, full stop.* The world's facts are registered once; what an agent
//! (or an agent's model of an agent) believes differently lives in that
//! perspective's delta map, and a query falls through to the baseline when no
//! delta exists.
//!
//! # Keyed by perspective path, not by bearer
//!
//! Deltas are keyed by the **full ascription path** — [`Perspective`] is
//! `holders ++ [bearer]` of the belief trope — not by the bare bearer.
//! Keying by bearer would collapse Sally's beliefs and an observer's *model*
//! of Sally's beliefs into one bucket, destroying exactly the false-belief
//! capability the Sally-Anne test protects. With path keying, the store's
//! delta structure is isomorphic to [`Trope::holders`]: store-minted tropes
//! satisfy `is_ascription_of`, so [`Trace::divergence`] and
//! [`Trace::projections`] work over them with zero new theory-of-mind code.
//! Self-ascription (`["anya", "anya"]`) is legal, as everywhere else.
//!
//! # Trace-backed, therefore replayable
//!
//! Every mutation appends a caused event to the [`Trace`]; the maps are a
//! deterministic fold of it. [`BeliefStore::rebuild`] performs that fold, and
//! `rebuild(serialised-and-restored trace) == original store` is the
//! determinism acceptance test.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::mass::Mass;
use crate::{Domain, Revision, Trace, TraceEvent, Trope, TropeId};

/// Whose belief-set: the full ascription path, `holders ++ [bearer]`.
///
/// Empty is the Global Baseline (ground truth; never asserted, only
/// registered). `["sally"]` is Sally's own beliefs. `["observer", "sally"]`
/// is the observer's model of Sally's beliefs. `["anya", "anya"]` is Anya's
/// model of her own beliefs — introspection can be wrong, so it is legal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Perspective(pub Vec<String>);

impl Perspective {
    /// The `Trope::holders` path this perspective mints: everything but the
    /// final segment.
    #[must_use]
    pub fn holders(&self) -> &[String] {
        self.0.split_last().map_or(&[], |(_, rest)| rest)
    }

    /// The bearer this perspective's beliefs belong to: the final segment.
    #[must_use]
    pub fn bearer(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

/// What a belief is about: the game's word for the resemblance class and its
/// intentional object. The engine never reads either — the [`Trope::kind`]
/// convention.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Subject {
    pub kind: String,
    pub about: Option<String>,
}

/// Whether a delta is currently believed or has been withdrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefStatus {
    Held,
    Retracted,
}

/// One perspective's belief about one subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Belief {
    /// The Epistemic trope this belief lives on.
    pub trope: TropeId,
    /// Content magnitude in milliunits — integer, replay-safe.
    pub value_milli: i32,
    pub confidence: Mass,
    /// The latest supporting (or, when retracted, retracting) event.
    pub source: u64,
    pub status: BeliefStatus,
}

/// What a [`BeliefStore::belief`] query found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeliefView {
    pub value_milli: i32,
    /// `Some` for a held delta; `None` for a baseline fall-through — ground
    /// truth is a fact, not a graded belief.
    pub confidence: Option<Mass>,
    /// The supporting event, where the view came from a delta.
    pub source: Option<u64>,
}

/// A baseline subject's prior, frozen when the subject is registered.
///
/// The ground trope keeps changing as the world moves; the prior does not.
/// It is the value every perspective starts from — the common knowledge the
/// game declares by registering — and it is captured once, from the trope's
/// latest event at registration, so later world events cannot reach a
/// perspective that never observed them (ADR-0022, Proposed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prior {
    /// The ground trope that carries this subject's world truth.
    pub trope: TropeId,
    /// The trope's value at registration, in milliunits.
    pub value_milli: i32,
    /// The event the value was captured from: the prior's whole support, and
    /// so the one world event every perspective counts as observed.
    pub source: u64,
}

/// A mutation the store refuses. A content gap must not panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The baseline is registered, not asserted: an empty perspective cannot
    /// hold a belief.
    EmptyPerspective,
    /// An unsourced belief change is exactly what the trace exists to forbid.
    EmptyProvenance,
    /// Retraction withdraws a held belief; there is none to withdraw.
    NothingToRetract,
    /// A prior is frozen from the ground trope's latest event, and that
    /// trope has no event, or its latest event is a retraction: there is no
    /// held value to freeze.
    NoBaselineValue,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::EmptyPerspective => {
                write!(f, "the baseline is registered, not asserted")
            }
            StoreError::EmptyProvenance => {
                write!(f, "a belief change must cite the evidence that caused it")
            }
            StoreError::NothingToRetract => {
                write!(f, "no held belief exists to retract")
            }
            StoreError::NoBaselineValue => {
                write!(f, "the ground trope holds no value to freeze as a prior")
            }
        }
    }
}

impl std::error::Error for StoreError {}

/// Global Baseline + per-perspective Deltas, as a deterministic fold of a
/// [`Trace`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeliefStore {
    /// Registered ground facts: which trope carries each subject's truth,
    /// and the prior frozen from it at registration.
    baseline: BTreeMap<Subject, Prior>,
    /// Each perspective's beliefs, where they differ from (or duplicate) the
    /// baseline.
    deltas: BTreeMap<Perspective, BTreeMap<Subject, Belief>>,
    /// The Epistemic trope minted for each (perspective, subject) pair.
    subject_tropes: BTreeMap<(Perspective, Subject), TropeId>,
}

impl BeliefStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the ground trope carrying a baseline fact, and freeze its
    /// current value as the subject's [`Prior`].
    ///
    /// The game owns the fact's trope entirely — its bearer (`"world"`, a
    /// door…), its domain (usually Mechanical) and its events. The store
    /// records which trope answers for which subject and the value it held
    /// at this call: its latest event by `(tick, id)`, which must be an
    /// assertion. Registration is the game declaring common knowledge, so
    /// register *before* the events no perspective should know about; a
    /// world event already in the trace here becomes part of every prior.
    /// Re-registering a subject repoints it and freezes a fresh prior.
    ///
    /// # Errors
    ///
    /// [`StoreError::NoBaselineValue`] — the trope has no event, or its
    /// latest event is a retraction. Nothing is registered.
    pub fn register_baseline(
        &mut self,
        trace: &Trace,
        subject: Subject,
        trope: TropeId,
    ) -> Result<Prior, StoreError> {
        let latest = trace
            .events
            .iter()
            .filter(|e| e.trope == trope)
            .max_by_key(|e| e.stamp())
            .filter(|e| e.revision == Revision::Assert)
            .ok_or(StoreError::NoBaselineValue)?;
        let prior = Prior {
            trope,
            value_milli: latest.value_milli,
            source: latest.id,
        };
        self.baseline.insert(subject, prior);
        Ok(prior)
    }

    /// Every registered subject with its frozen prior, in subject order.
    ///
    /// This is what a replay must keep beside the trace: hand it back to
    /// [`rebuild`](Self::rebuild). Priors are not re-derived from the trace,
    /// because by replay time the ground tropes have moved on.
    pub fn baseline(&self) -> impl Iterator<Item = (&Subject, &Prior)> {
        self.baseline.iter()
    }

    /// Assert (or revise) a belief at a perspective.
    ///
    /// Mints the Epistemic belief trope on first use — `holders`/`bearer`
    /// from the perspective, `kind`/`about` from the subject — and appends an
    /// `Assert` event carrying the value, the confidence and the caller's
    /// provenance. A revision of a held belief additionally cites the
    /// superseded event in `caused_by`, so contradiction is explicit in the
    /// trace rather than silently overwritten.
    ///
    /// Ids are minted as `max(existing) + 1` over the trace at call time, so
    /// host-appended events can interleave without collision.
    ///
    /// # Errors
    ///
    /// [`StoreError::EmptyPerspective`] — the baseline is registered, not
    /// asserted. [`StoreError::EmptyProvenance`] — no belief moves without
    /// evidence.
    // Eight arguments, each of a distinct type or meaning (where, about what,
    // when, how much, how sure, why); a bundling struct would rename them
    // without removing any, and every call site currently reads as a sentence.
    #[allow(clippy::too_many_arguments)]
    pub fn assert_belief(
        &mut self,
        trace: &mut Trace,
        perspective: &Perspective,
        subject: &Subject,
        tick: u64,
        value_milli: i32,
        confidence: Mass,
        mut caused_by: Vec<u64>,
    ) -> Result<u64, StoreError> {
        let Some(bearer) = perspective.bearer() else {
            return Err(StoreError::EmptyPerspective);
        };
        if caused_by.is_empty() {
            return Err(StoreError::EmptyProvenance);
        }

        let key = (perspective.clone(), subject.clone());
        let trope = match self.subject_tropes.get(&key) {
            Some(&trope) => trope,
            None => {
                let trope = TropeId(next_trope_id(trace));
                trace.tropes.push(Trope {
                    id: trope,
                    holders: perspective.holders().to_vec(),
                    bearer: bearer.to_string(),
                    about: subject.about.clone(),
                    kind: subject.kind.clone(),
                    domain: Domain::Epistemic,
                });
                self.subject_tropes.insert(key, trope);
                trope
            }
        };

        // A revision supersedes: the contradicted event joins the causes.
        if let Some(prior) = self
            .deltas
            .get(perspective)
            .and_then(|d| d.get(subject))
            .filter(|b| b.status == BeliefStatus::Held)
            && !caused_by.contains(&prior.source)
        {
            caused_by.push(prior.source);
        }

        let id = next_event_id(trace);
        trace.events.push(TraceEvent {
            id,
            tick,
            trope,
            value_milli,
            confidence: Some(confidence),
            revision: Revision::Assert,
            caused_by,
        });
        self.deltas.entry(perspective.clone()).or_default().insert(
            subject.clone(),
            Belief {
                trope,
                value_milli,
                confidence,
                source: id,
                status: BeliefStatus::Held,
            },
        );
        Ok(id)
    }

    /// Retract a held belief: the moment a perspective discovers it was
    /// wrong, as an event.
    ///
    /// The appended `Retract` event cites the contradicting evidence in
    /// `caused_by` and carries the *withdrawn* value, so
    /// [`Trace::latest_value`] still reads the stale content as telemetry
    /// while [`Trace::held_value`] reads `None`.
    ///
    /// # Errors
    ///
    /// [`StoreError::EmptyProvenance`] — retraction without contradicting
    /// evidence is an untraced belief change. [`StoreError::NothingToRetract`]
    /// — including a belief already retracted; re-learning after retraction
    /// is a fresh [`assert_belief`](Self::assert_belief), not a second
    /// withdrawal.
    pub fn retract(
        &mut self,
        trace: &mut Trace,
        perspective: &Perspective,
        subject: &Subject,
        tick: u64,
        caused_by: Vec<u64>,
    ) -> Result<u64, StoreError> {
        if caused_by.is_empty() {
            return Err(StoreError::EmptyProvenance);
        }
        let Some(belief) = self
            .deltas
            .get_mut(perspective)
            .and_then(|d| d.get_mut(subject))
            .filter(|b| b.status == BeliefStatus::Held)
        else {
            return Err(StoreError::NothingToRetract);
        };

        let id = next_event_id(trace);
        trace.events.push(TraceEvent {
            id,
            tick,
            trope: belief.trope,
            value_milli: belief.value_milli,
            confidence: None,
            revision: Revision::Retract,
            caused_by,
        });
        belief.status = BeliefStatus::Retracted;
        belief.source = id;
        Ok(id)
    }

    /// The query the store exists for: what does `perspective` currently
    /// believe about `subject`?
    ///
    /// A held delta answers first. With no delta at all, the registered
    /// baseline answers through the trace. A **retracted** delta answers
    /// `None` — a withdrawn belief is absence of belief, not a silent revert
    /// to the baseline: reverting without an event would be a belief change
    /// that leaves no trace, and re-learning requires a fresh assertion with
    /// fresh provenance.
    #[must_use]
    pub fn belief(
        &self,
        trace: &Trace,
        perspective: &Perspective,
        subject: &Subject,
    ) -> Option<BeliefView> {
        match self.deltas.get(perspective).and_then(|d| d.get(subject)) {
            Some(belief) if belief.status == BeliefStatus::Held => Some(BeliefView {
                value_milli: belief.value_milli,
                confidence: Some(belief.confidence),
                source: Some(belief.source),
            }),
            Some(_) => None, // retracted: absence, reported as such
            None => {
                let prior = self.baseline.get(subject)?;
                Some(BeliefView {
                    value_milli: trace.latest_value(prior.trope)?,
                    confidence: None,
                    source: None,
                })
            }
        }
    }

    /// Rebuild a store by folding a trace, with the same baseline
    /// registrations — the replay path.
    ///
    /// `baseline` is the original store's [`baseline`](Self::baseline), priors
    /// included. The priors are taken as given, never re-captured from
    /// `trace`: the ground tropes have moved since registration, and
    /// re-capturing would hand every perspective the world's current truth.
    ///
    /// Store-managed tropes are recognised as Epistemic tropes whose
    /// assertions all carry a confidence (the store always writes one).
    /// Epistemic tropes with unconfident assertions — hand-built seams,
    /// host-owned epistemic telemetry — are left alone, exactly as they were
    /// never in the original store's maps.
    ///
    /// For a trace the store produced, `rebuild(trace, same_baseline)` is
    /// field-for-field equal to the original store, independent of event
    /// order in the slice (the fold is over the canonical `(tick, id)`
    /// order).
    #[must_use]
    pub fn rebuild(trace: &Trace, baseline: &[(Subject, Prior)]) -> Self {
        let mut store = Self::new();
        for (subject, prior) in baseline {
            store.baseline.insert(subject.clone(), *prior);
        }

        for trope in &trace.tropes {
            if trope.domain != Domain::Epistemic {
                continue;
            }
            let mut events: Vec<&TraceEvent> = trace
                .events
                .iter()
                .filter(|e| e.trope == trope.id)
                .collect();
            events.sort_by_key(|e| e.stamp());
            if events.is_empty()
                || events
                    .iter()
                    .any(|e| e.revision == Revision::Assert && e.confidence.is_none())
            {
                continue; // not store-managed
            }

            let mut perspective = trope.holders.clone();
            perspective.push(trope.bearer.clone());
            let perspective = Perspective(perspective);
            let subject = Subject {
                kind: trope.kind.clone(),
                about: trope.about.clone(),
            };
            store
                .subject_tropes
                .insert((perspective.clone(), subject.clone()), trope.id);

            let mut belief: Option<Belief> = None;
            for event in events {
                match event.revision {
                    Revision::Assert => {
                        belief = Some(Belief {
                            trope: trope.id,
                            value_milli: event.value_milli,
                            confidence: event
                                .confidence
                                .expect("unconfident assertions were filtered above"),
                            source: event.id,
                            status: BeliefStatus::Held,
                        });
                    }
                    Revision::Retract => {
                        if let Some(b) = belief.as_mut() {
                            b.status = BeliefStatus::Retracted;
                            b.source = event.id;
                        }
                    }
                }
            }
            if let Some(belief) = belief {
                store
                    .deltas
                    .entry(perspective)
                    .or_default()
                    .insert(subject, belief);
            }
        }
        store
    }
}

fn next_trope_id(trace: &Trace) -> u64 {
    trace.tropes.iter().map(|t| t.id.0).max().unwrap_or(0) + 1
}

fn next_event_id(trace: &Trace) -> u64 {
    trace.events.iter().map(|e| e.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mass(v: u16) -> Mass {
        Mass::new(v).unwrap()
    }

    fn subject(kind: &str, about: &str) -> Subject {
        Subject {
            kind: kind.into(),
            about: Some(about.into()),
        }
    }

    fn perspective(path: &[&str]) -> Perspective {
        Perspective(path.iter().map(|s| (*s).to_string()).collect())
    }

    /// A world with one registered ground fact and one perceiving agent.
    fn world() -> (Trace, BeliefStore, Subject) {
        let marble = subject("marble_location", "marble");
        let trace = Trace {
            tropes: vec![Trope {
                id: TropeId(1),
                holders: vec![],
                bearer: "world".into(),
                about: Some("marble".into()),
                kind: "marble_location".into(),
                domain: Domain::Mechanical,
            }],
            events: vec![TraceEvent {
                id: 1,
                tick: 1,
                trope: TropeId(1),
                value_milli: 1_000, // BASKET
                confidence: None,
                revision: Revision::Assert,
                caused_by: vec![],
            }],
        };
        let mut store = BeliefStore::new();
        store
            .register_baseline(&trace, marble.clone(), TropeId(1))
            .unwrap();
        assert_eq!(trace.validate(), Ok(()));
        (trace, store, marble)
    }

    #[test]
    fn the_baseline_is_registered_not_asserted() {
        let (mut trace, mut store, marble) = world();
        assert_eq!(
            store.assert_belief(
                &mut trace,
                &perspective(&[]),
                &marble,
                2,
                1_000,
                mass(9_000),
                vec![1],
            ),
            Err(StoreError::EmptyPerspective)
        );
        // The baseline still answers queries, through the trace.
        let view = store.belief(&trace, &perspective(&[]), &marble);
        assert_eq!(
            view,
            Some(BeliefView {
                value_milli: 1_000,
                confidence: None,
                source: None,
            })
        );
    }

    #[test]
    fn a_prior_is_frozen_only_from_a_held_value() {
        let (mut trace, mut store, _) = world();
        let door = subject("door_state", "door");
        trace.tropes.push(Trope {
            id: TropeId(2),
            holders: vec![],
            bearer: "world".into(),
            about: Some("door".into()),
            kind: "door_state".into(),
            domain: Domain::Mechanical,
        });
        assert_eq!(
            store.register_baseline(&trace, door.clone(), TropeId(2)),
            Err(StoreError::NoBaselineValue),
            "a trope with no event has nothing to freeze"
        );
        trace.events.push(TraceEvent {
            id: 2,
            tick: 2,
            trope: TropeId(2),
            value_milli: 1,
            confidence: None,
            revision: Revision::Assert,
            caused_by: vec![],
        });
        trace.events.push(TraceEvent {
            id: 3,
            tick: 3,
            trope: TropeId(2),
            value_milli: 1,
            confidence: None,
            revision: Revision::Retract,
            caused_by: vec![2],
        });
        assert_eq!(
            store.register_baseline(&trace, door.clone(), TropeId(2)),
            Err(StoreError::NoBaselineValue),
            "a withdrawn value is not a value to freeze"
        );
        assert_eq!(store.baseline().count(), 1, "a refusal registers nothing");
    }

    #[test]
    fn no_belief_moves_without_evidence() {
        let (mut trace, mut store, marble) = world();
        assert_eq!(
            store.assert_belief(
                &mut trace,
                &perspective(&["guard"]),
                &marble,
                2,
                1_000,
                mass(9_000),
                vec![],
            ),
            Err(StoreError::EmptyProvenance)
        );
        assert_eq!(
            store.retract(&mut trace, &perspective(&["guard"]), &marble, 2, vec![]),
            Err(StoreError::EmptyProvenance)
        );
        assert_eq!(
            store.retract(&mut trace, &perspective(&["guard"]), &marble, 2, vec![1]),
            Err(StoreError::NothingToRetract),
            "nothing has been asserted yet"
        );
    }

    #[test]
    fn a_delta_answers_before_the_baseline_and_perspectives_stay_separate() {
        let (mut trace, mut store, marble) = world();
        let guard = perspective(&["guard"]);
        let anya = perspective(&["anya"]);

        store
            .assert_belief(&mut trace, &guard, &marble, 2, 2_000, mass(7_000), vec![1])
            .unwrap();

        let guards = store.belief(&trace, &guard, &marble).unwrap();
        assert_eq!(guards.value_milli, 2_000, "his delta, not the baseline");
        assert_eq!(guards.confidence, Some(mass(7_000)));

        let anyas = store.belief(&trace, &anya, &marble).unwrap();
        assert_eq!(
            anyas.value_milli, 1_000,
            "no delta of her own: baseline fall-through"
        );
        assert_eq!(anyas.confidence, None, "ground truth is not graded");
    }

    #[test]
    fn a_revision_cites_the_event_it_supersedes() {
        let (mut trace, mut store, marble) = world();
        let guard = perspective(&["guard"]);
        let first = store
            .assert_belief(&mut trace, &guard, &marble, 2, 1_000, mass(8_000), vec![1])
            .unwrap();
        let second = store
            .assert_belief(&mut trace, &guard, &marble, 4, 2_000, mass(6_000), vec![1])
            .unwrap();
        let revision = trace.events.iter().find(|e| e.id == second).unwrap();
        assert!(
            revision.caused_by.contains(&first),
            "the superseded event is a cause: contradiction is explicit"
        );
        assert_eq!(trace.validate(), Ok(()));
    }

    #[test]
    fn store_minted_tropes_compose_with_theory_of_mind_unmodified() {
        let (mut trace, mut store, marble) = world();
        let sally = perspective(&["sally"]);
        let observer_of_sally = perspective(&["observer", "sally"]);

        store
            .assert_belief(&mut trace, &sally, &marble, 2, 1_000, mass(9_000), vec![1])
            .unwrap();
        store
            .assert_belief(
                &mut trace,
                &observer_of_sally,
                &marble,
                3,
                1_000,
                mass(8_000),
                vec![1],
            )
            .unwrap();

        let sallys = trace
            .tropes
            .iter()
            .find(|t| t.holders.is_empty() && t.bearer == "sally")
            .unwrap();
        let observers = trace
            .tropes
            .iter()
            .find(|t| t.holders == ["observer"])
            .unwrap();
        assert!(
            observers.is_ascription_of(sallys),
            "path keying is isomorphic to Trope::holders"
        );
        assert_eq!(
            trace.divergence(observers),
            Some(0),
            "divergence works over store-minted tropes with zero new code"
        );
        assert_eq!(
            trace.validate_to_order(1),
            Ok(()),
            "the existing depth bound applies unchanged"
        );
    }

    #[test]
    fn the_deception_discovery_scenario_leaves_a_traceable_moment() {
        // The failo test, in Sally-Anne's spirit: stale belief → covert move
        // → contradicting evidence → retraction. The discovery must be a
        // trace, not an error state.
        const BASKET: i32 = 1_000;
        const BOX: i32 = 2_000;

        let (mut trace, mut store, marble) = world();
        let guard = perspective(&["guard"]);
        let observer_of_guard = perspective(&["observer", "guard"]);

        // t=1 the guard perceives the marble in the basket (world event 1).
        store
            .assert_belief(&mut trace, &guard, &marble, 1, BASKET, mass(9_000), vec![1])
            .unwrap();
        // …and an observer correctly models the guard's belief.
        store
            .assert_belief(
                &mut trace,
                &observer_of_guard,
                &marble,
                1,
                BASKET,
                mass(8_000),
                vec![1],
            )
            .unwrap();

        // t=3 the player covertly moves it: a world event nothing links to
        // the guard. His belief must stay stale.
        let move_id = next_event_id(&trace);
        trace.events.push(TraceEvent {
            id: move_id,
            tick: 3,
            trope: TropeId(1),
            value_milli: BOX,
            confidence: None,
            revision: Revision::Assert,
            caused_by: vec![],
        });
        assert_eq!(
            store.belief(&trace, &guard, &marble).unwrap().value_milli,
            BASKET,
            "nothing reached the guard: divergence from the world is the deception"
        );

        // t=5 the guard perceives the empty basket — contradicting evidence —
        // and withdraws the belief.
        let contradiction = next_event_id(&trace);
        trace.events.push(TraceEvent {
            id: contradiction,
            tick: 5,
            trope: TropeId(1),
            value_milli: BOX,
            confidence: None,
            revision: Revision::Assert,
            caused_by: vec![move_id],
        });
        let retraction = store
            .retract(&mut trace, &guard, &marble, 5, vec![contradiction])
            .unwrap();

        // The whole trace still validates, under both contracts.
        assert_eq!(trace.validate(), Ok(()));
        assert_eq!(trace.validate_belief_contract(), Ok(()));

        // The discovery is an event, caused_by-linked to the evidence and,
        // transitively, to the deception itself.
        let found: Vec<u64> = trace.retractions().map(|e| e.id).collect();
        assert_eq!(found, vec![retraction], "exactly one discovered deception");
        let retract_event = trace.events.iter().find(|e| e.id == retraction).unwrap();
        assert_eq!(retract_event.caused_by, vec![contradiction]);

        // Withdrawn belief reads as absence; the stale content stays readable
        // as telemetry.
        let guards_trope = store.subject_tropes[&(guard.clone(), marble.clone())];
        assert_eq!(store.belief(&trace, &guard, &marble), None);
        assert_eq!(trace.held_value(guards_trope), None);
        assert_eq!(trace.latest_value(guards_trope), Some(BASKET));

        // ToM coda: no evidence reached the observer, so the observer still
        // holds a model of the guard's pre-retraction belief — a correct
        // model of a belief that no longer exists.
        assert_eq!(
            store
                .belief(&trace, &observer_of_guard, &marble)
                .unwrap()
                .value_milli,
            BASKET
        );

        // Re-learning is a fresh assertion with fresh provenance.
        assert_eq!(
            store.retract(&mut trace, &guard, &marble, 6, vec![contradiction]),
            Err(StoreError::NothingToRetract),
            "a retracted belief cannot be retracted again"
        );
        store
            .assert_belief(
                &mut trace,
                &guard,
                &marble,
                7,
                BOX,
                mass(9_500),
                vec![contradiction],
            )
            .unwrap();
        assert_eq!(
            store.belief(&trace, &guard, &marble).unwrap().value_milli,
            BOX
        );
        assert_eq!(trace.validate(), Ok(()));
    }

    #[test]
    fn rebuild_reproduces_the_store_from_a_round_tripped_trace() {
        let (mut trace, mut store, marble) = world();
        let guard = perspective(&["guard"]);
        let observer = perspective(&["observer", "guard"]);

        store
            .assert_belief(&mut trace, &guard, &marble, 2, 1_000, mass(9_000), vec![1])
            .unwrap();
        store
            .assert_belief(
                &mut trace,
                &observer,
                &marble,
                3,
                1_000,
                mass(7_000),
                vec![1],
            )
            .unwrap();
        let contradiction = next_event_id(&trace);
        trace.events.push(TraceEvent {
            id: contradiction,
            tick: 4,
            trope: TropeId(1),
            value_milli: 2_000,
            confidence: None,
            revision: Revision::Assert,
            caused_by: vec![],
        });
        store
            .retract(&mut trace, &guard, &marble, 5, vec![contradiction])
            .unwrap();

        let baseline: Vec<(Subject, Prior)> =
            store.baseline().map(|(s, p)| (s.clone(), *p)).collect();

        // Through serde and back: the fold reproduces the store exactly.
        let json = serde_json::to_string(&trace).unwrap();
        let restored: Trace = serde_json::from_str(&json).unwrap();
        assert_eq!(BeliefStore::rebuild(&restored, &baseline), store);

        // And independent of event order in the slice.
        let mut shuffled = restored.clone();
        shuffled.events.reverse();
        assert!(!shuffled.is_canonically_ordered());
        assert_eq!(BeliefStore::rebuild(&shuffled, &baseline), store);
    }

    #[test]
    fn rebuild_leaves_hand_built_epistemic_seams_alone() {
        // An Epistemic trope whose assertion carries no confidence was not
        // minted by the store (it always writes one), so the fold must not
        // absorb it.
        let trace = Trace {
            tropes: vec![Trope {
                id: TropeId(1),
                holders: vec![],
                bearer: "billy".into(),
                about: Some("note".into()),
                kind: "object_theory".into(),
                domain: Domain::Epistemic,
            }],
            events: vec![TraceEvent {
                id: 1,
                tick: 1,
                trope: TropeId(1),
                value_milli: 1_000,
                confidence: None,
                revision: Revision::Assert,
                caused_by: vec![],
            }],
        };
        assert_eq!(BeliefStore::rebuild(&trace, &[]), BeliefStore::new());
    }
}
