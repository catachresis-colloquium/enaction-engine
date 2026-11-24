// SPDX-License-Identifier: MPL-2.0
//! A deliberately small, game-neutral event seam.
//!
//! The crate makes two structural commitments and is otherwise nothing but
//! their consequences.
//!
//! # Typed by domain
//!
//! Domains are separate types in the trace: affect never silently becomes
//! knowledge, a belief never silently becomes a goal, and none of them chooses
//! behaviour without an explicit selection event.
//!
//! # Particularised by trope
//!
//! A [`Trope`] is an *abstract particular* — **this** bearer's **this**
//! property-instance, with its own identity. Trope theory's claim (Williams,
//! Campbell) is that properties are particulars rather than universals: there
//! is no single shared "suspicion" that several guards each instantiate. There
//! are several suspicions, which *resemble* one another.
//!
//! That distinction is not decoration here — it changes what the engine can
//! say:
//!
//! * **Separate causal histories.** Two guards suspecting the same object are
//!   two particulars with independent pasts. Keyed as `(bearer, state)` pairs
//!   into a shared universal, that independence is representable only by
//!   convention; as tropes it is structural.
//! * **A belief can be referred to.** A trope has an id, so an event can point
//!   at *the belief that caused this*, not merely at an earlier event.
//! * **Resemblance replaces identity.** [`Trope::resembles`] asks whether two
//!   particulars are of a kind, rather than testing equality — which is what
//!   [`Domain::SocialRelational`] needs in order to have anything to reason
//!   over.
//! * **Domain becomes structural.** `domain` lives on the trope, not on the
//!   event, so one property-instance cannot have one event call it affective
//!   and the next call it epistemic. Domain consistency stops being something
//!   a validator has to hope for.
//!
//! # Other minds are modelled, not shared
//!
//! [`Trope::holders`] carries an *ascription path*: whose model this
//! particular lives inside. Empty is ground truth; `["billy"]` is Billy's
//! model of someone's state; `["billy", "anya"]` is Billy's model of Anya's
//! model. Because it is a path and not a reference, an agent can model a
//! state that does not exist — which is what being wrong about someone is,
//! and the reason [`Trace::divergence`] and [`Trace::projections`] can
//! measure it.
//!
//! # What the engine does *not* supply
//!
//! A game supplies the vocabulary in [`Trope::kind`] and [`Trope::about`]; the
//! engine supplies identity, ordering, causality and domain separation. The
//! engine never interprets a game word.
//!
//! Magnitudes are integer milliunits ([`TraceEvent::value_milli`]), never
//! floats: a trace must compare bit-for-bit across machines and replays, and
//! float arithmetic near identity does not (see `enaction-time`'s `lerp`, which
//! meets the same constraint the same way).

#![forbid(unsafe_code)]

pub mod belief;
pub mod mass;
pub mod store;

pub use mass::Mass;
pub use store::{BeliefStore, BeliefView, Perspective, Prior, StoreError, Subject};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which kind of state a trope carries, and therefore which stage of the
/// cognitive chain any event about it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Mechanical,
    Epistemic,
    Affective,
    Conative,
    BehaviouralSelection,
    SocialRelational,
}

/// Identity of an abstract particular.
///
/// Distinct from a [`TraceEvent`] id: the trope is what persists, the events
/// are what happened to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TropeId(pub u64);

/// An abstract particular: *this* bearer's *this* property-instance.
///
/// Abstract because it is a property rather than a whole thing; particular
/// because it belongs to one bearer and is not shared. Billy's
/// interest-in-the-note is one trope; another guard's interest in the same note
/// is a different one that merely resembles it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trope {
    pub id: TropeId,
    /// The ascription path: whose *model* this particular lives inside,
    /// outermost first.
    ///
    /// Empty is ground truth — the bearer's own state. `["billy"]` is Billy's
    /// model of the bearer's state. `["billy", "anya"]` is Billy's model of
    /// Anya's model of the bearer's state. Length is the order of theory of
    /// mind (Dennett), and is bounded by the contract's declared maximum.
    ///
    /// **This is a path, not a reference, and that is the whole point.** An
    /// ascription must be able to describe a state that does not exist —
    /// Billy may model a suspicion Anya has never held. A reference to Anya's
    /// actual trope would make false ascription unrepresentable, and being
    /// wrong about other minds is the phenomenon this field exists to carry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holders: Vec<String>,
    /// Whose property-instance this is *about*. For ground truth this is also
    /// who holds it; for an ascription it is the modelled agent.
    pub bearer: String,
    /// Its intentional object, where it has one — what the state is *about*.
    /// `None` for non-relational states such as bare arousal.
    pub about: Option<String>,
    /// The game's word for the resemblance class. The engine never reads it.
    pub kind: String,
    /// Fixed for the life of the trope; every event about it inherits this.
    pub domain: Domain,
}

impl Trope {
    /// Whether two tropes are of a kind — same domain, same game word, and the
    /// same intentional object.
    ///
    /// **Resemblance is not identity.** Two bearers' suspicions of the same
    /// object resemble each other and remain two separate particulars with
    /// separate histories. This is the relation [`Domain::SocialRelational`]
    /// reasoning is meant to be built over: *"does what he feels resemble what
    /// I feel?"* is answerable; *"is it the same feeling?"* is not a question
    /// tropes permit.
    #[must_use]
    pub fn resembles(&self, other: &Self) -> bool {
        self.domain == other.domain && self.kind == other.kind && self.about == other.about
    }

    /// Who actually holds this particular: the outermost ascriber, or the
    /// bearer themself when this is ground truth.
    #[must_use]
    pub fn holder(&self) -> &str {
        self.holders
            .first()
            .map_or(self.bearer.as_str(), String::as_str)
    }

    /// Whether this is a model of someone's state rather than the state.
    #[must_use]
    pub fn is_ascription(&self) -> bool {
        !self.holders.is_empty()
    }

    /// The order of theory of mind: 0 = ground truth, 1 = "he thinks she
    /// feels…", 2 = "he thinks she thinks he feels…".
    #[must_use]
    pub fn order(&self) -> usize {
        self.holders.len()
    }

    /// Whether `other` is what this ascription is *about* — the state one
    /// level closer to the ground.
    ///
    /// `["billy"]/anya/suspicion` has counterpart `[]/anya/suspicion`:
    /// Anya's actual suspicion, which Billy is modelling. Comparing their
    /// magnitudes is how wrong Billy is.
    ///
    /// Returns `false` for ground truth, which ascribes nothing.
    #[must_use]
    pub fn is_ascription_of(&self, other: &Self) -> bool {
        self.is_ascription()
            && self.holders[1..] == other.holders[..]
            && self.bearer == other.bearer
            && self.kind == other.kind
            && self.about == other.about
            && self.domain == other.domain
    }

    /// Whether this trope resembles `other` but is borne by someone else.
    ///
    /// The companion to [`resembles`](Self::resembles), which a trope trivially
    /// satisfies against itself.
    #[must_use]
    pub fn is_peer_of(&self, other: &Self) -> bool {
        self.bearer != other.bearer && self.resembles(other)
    }
}

/// Whether an event asserts its trope's state or withdraws a belief.
///
/// Retraction is first-class rather than "value zero" because believing
/// nothing and withdrawing a belief are different events, and *the trace must
/// be able to tell them apart*: contradiction is not an error state — it is
/// the moment an agent discovers it was deceived, and it must leave a trace
/// like everything else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Revision {
    #[default]
    Assert,
    Retract,
}

impl Revision {
    /// serde helper: the default (`Assert`) is omitted from serialization so
    /// pre-revision traces and their byte encodings are unchanged.
    fn is_assert(&self) -> bool {
        matches!(self, Revision::Assert)
    }
}

/// One change to, or observation of, a trope.
///
/// Carries no domain of its own: the domain is the trope's, so an event cannot
/// reclassify the particular it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub id: u64,
    pub tick: u64,
    /// The particular this event is about.
    pub trope: TropeId,
    /// Magnitude in integer milliunits. Never a float — see the crate docs.
    pub value_milli: i32,
    /// Bounded confidence (ADR-0016). Only meaningful on events about
    /// [`Domain::Epistemic`] tropes, and the validator enforces exactly that:
    /// affect intensities quantise to [`Mass`] only *at* the crossing — on the
    /// epistemic event they cause — and conative utilities never cross at all.
    ///
    /// Deliberately separate from `value_milli`: content magnitude and
    /// justification budget are different axes, and per-mille trace values
    /// are telemetry that must never become decision inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Mass>,
    /// Assert (the default) or Retract. See [`Revision`].
    #[serde(default, skip_serializing_if = "Revision::is_assert")]
    pub revision: Revision,
    /// Ids of the events that brought this one about.
    pub caused_by: Vec<u64>,
}

impl TraceEvent {
    /// The engine's total order over events: by tick, then by id.
    ///
    /// Deliberately independent of position in any slice, so causality can be
    /// judged without first requiring the caller to sort.
    fn stamp(&self) -> (u64, u64) {
        (self.tick, self.id)
    }
}

/// Default bound on theory-of-mind order.
///
/// The shared ESM contract's first implementation slice declares "one level of
/// nested belief, with a declared maximum depth"; this is that declaration's
/// default. Unbounded nesting is on the contract's own reject list, because
/// "he thinks she thinks he thinks…" has no natural floor and a malicious or
/// buggy producer can exhaust a consumer with it.
pub const DEFAULT_MAX_TOM_ORDER: usize = 1;

/// A trace: the particulars, and what happened to them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub tropes: Vec<Trope>,
    pub events: Vec<TraceEvent>,
}

impl Trace {
    /// Validate the structural contract without interpreting any game word.
    ///
    /// # Errors
    ///
    /// Returns every violation found, not only the first.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        self.validate_to_order(DEFAULT_MAX_TOM_ORDER)
    }

    /// Validate, bounding theory-of-mind nesting at `max_order`.
    ///
    /// # Errors
    ///
    /// Returns every violation found, not only the first.
    pub fn validate_to_order(&self, max_order: usize) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Pass 1 — index the particulars.
        let mut tropes = BTreeMap::new();
        for trope in &self.tropes {
            if tropes.insert(trope.id, trope).is_some() {
                errors.push(format!("duplicate trope id {}", trope.id.0));
            }
            if trope.bearer.is_empty() || trope.kind.is_empty() {
                errors.push(format!("trope {} has an empty bearer or kind", trope.id.0));
            }
            if trope.holders.iter().any(String::is_empty) {
                errors.push(format!(
                    "trope {} has an empty name in its ascription path",
                    trope.id.0
                ));
            }
            if trope.order() > max_order {
                errors.push(format!(
                    "trope {} nests theory of mind to order {}, beyond the declared \
                     maximum of {max_order}",
                    trope.id.0,
                    trope.order()
                ));
            }
        }

        // Pass 2 — index the events, so causality can be judged without
        // depending on whatever order the caller happened to supply.
        let mut stamps = BTreeMap::new();
        for event in &self.events {
            if stamps.insert(event.id, event.stamp()).is_some() {
                errors.push(format!("duplicate trace event id {}", event.id));
            }
        }

        // Pass 3 — the real checks.
        for event in &self.events {
            match tropes.get(&event.trope) {
                None => errors.push(format!(
                    "event {} is about trope {}, which is not declared",
                    event.id, event.trope.0
                )),
                Some(trope) => {
                    // Confidence is the epistemic crossing (ADR-0016): affect
                    // intensities quantise to Mass only at the crossing, and
                    // conative utilities never cross, so a confidence on any
                    // non-Epistemic trope's event is a scope breach.
                    if event.confidence.is_some() && trope.domain != Domain::Epistemic {
                        errors.push(format!(
                            "event {} carries a confidence but trope {} is {:?}, \
                             not Epistemic — confidence crosses only at the \
                             epistemic seam",
                            event.id, event.trope.0, trope.domain
                        ));
                    }
                    if event.revision == Revision::Retract {
                        // Revision is an epistemic act.
                        if trope.domain != Domain::Epistemic {
                            errors.push(format!(
                                "event {} retracts trope {}, which is {:?} — \
                                 only Epistemic tropes can be retracted",
                                event.id, event.trope.0, trope.domain
                            ));
                        }
                        // Retraction without a cause is exactly the untraced
                        // belief change this field exists to forbid.
                        if event.caused_by.is_empty() {
                            errors.push(format!(
                                "event {} is a retraction with no cause — the \
                                 contradicting evidence must be in the trace",
                                event.id
                            ));
                        }
                        // You cannot discover you were deceived about
                        // something you never believed.
                        let has_prior_assert = self.events.iter().any(|e| {
                            e.trope == event.trope
                                && e.revision == Revision::Assert
                                && e.stamp() < event.stamp()
                        });
                        if !has_prior_assert {
                            errors.push(format!(
                                "event {} retracts trope {} but no earlier \
                                 assertion on it exists",
                                event.id, event.trope.0
                            ));
                        }
                    }
                }
            }
            for cause in &event.caused_by {
                match stamps.get(cause) {
                    // Absent and out-of-order are different faults and are now
                    // reported as such. The previous single-pass check could
                    // only say "absent or not earlier", because it judged
                    // causality by position in the slice.
                    None => errors.push(format!(
                        "event {} names cause {}, which is not in the trace",
                        event.id, cause
                    )),
                    Some(&cause_stamp) if cause_stamp >= event.stamp() => {
                        errors.push(format!(
                            "event {} names cause {}, which is not strictly earlier \
                             (cause tick {} id {}, effect tick {} id {})",
                            event.id, cause, cause_stamp.0, cause_stamp.1, event.tick, event.id
                        ));
                    }
                    Some(_) => {}
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Whether the events are already in the engine's canonical order.
    ///
    /// Presentation only. An unordered trace is **not** invalid — causality is
    /// judged by `(tick, id)` rather than by position — so this is reported
    /// separately from [`validate`](Self::validate) rather than folded into it.
    #[must_use]
    pub fn is_canonically_ordered(&self) -> bool {
        self.events.windows(2).all(|w| w[0].stamp() < w[1].stamp())
    }

    /// Sort the events into the engine's canonical order, in place.
    pub fn canonicalise(&mut self) {
        self.events.sort_by_key(TraceEvent::stamp);
    }

    /// The most recent value recorded for `trope`, in milliunits.
    ///
    /// "Most recent" by the engine's canonical `(tick, id)` order, so the
    /// answer does not depend on how the caller happened to order the slice.
    ///
    /// Deliberately **retraction-blind**: this is raw telemetry — the last
    /// content that was on the wire — and [`Trace::divergence`] is built on
    /// it. For the belief-aware reading, use [`held_value`](Self::held_value).
    #[must_use]
    pub fn latest_value(&self, trope: TropeId) -> Option<i32> {
        self.events
            .iter()
            .filter(|e| e.trope == trope)
            .max_by_key(|e| e.stamp())
            .map(|e| e.value_milli)
    }

    /// The value currently *held* for `trope`: the latest value, unless the
    /// latest event is a retraction — then `None`.
    ///
    /// A withdrawn belief reads as absence of belief, reported as such.
    /// Falling back to any earlier value would be a belief change that leaves
    /// no trace, which is the store's own thesis violated; re-learning is a
    /// fresh assertion with fresh provenance.
    #[must_use]
    pub fn held_value(&self, trope: TropeId) -> Option<i32> {
        self.events
            .iter()
            .filter(|e| e.trope == trope)
            .max_by_key(|e| e.stamp())
            .and_then(|e| (e.revision == Revision::Assert).then_some(e.value_milli))
    }

    /// Every retraction in the trace, in whatever order the events are held.
    ///
    /// The watcher surface for discovered deception: each item is a moment an
    /// agent found out it was wrong, `caused_by`-linked to the evidence.
    pub fn retractions(&self) -> impl Iterator<Item = &TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.revision == Revision::Retract)
    }

    /// The opt-in belief-contract profile, over and above [`validate`]
    /// (Self::validate): every assertion on an Epistemic trope must carry at
    /// least one provenance event *and* a confidence.
    ///
    /// A profile rather than part of the base contract because existing valid
    /// traces contain epistemic events with neither — the base validator must
    /// keep accepting them. Under this profile, "every non-derived belief has
    /// at least one provenance event" (CAC-KERNEL §3) is a rejected trace,
    /// not a hope.
    ///
    /// # Errors
    ///
    /// Returns every violation found, not only the first.
    pub fn validate_belief_contract(&self) -> Result<(), Vec<String>> {
        let tropes: BTreeMap<TropeId, &Trope> = self.tropes.iter().map(|t| (t.id, t)).collect();
        let mut errors = Vec::new();
        for event in &self.events {
            let Some(trope) = tropes.get(&event.trope) else {
                continue; // the base validator reports undeclared tropes
            };
            if trope.domain != Domain::Epistemic || event.revision != Revision::Assert {
                continue;
            }
            if event.caused_by.is_empty() {
                errors.push(format!(
                    "belief contract: epistemic assertion {} has no provenance \
                     event",
                    event.id
                ));
            }
            if event.confidence.is_none() {
                errors.push(format!(
                    "belief contract: epistemic assertion {} carries no \
                     confidence",
                    event.id
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The actual state an ascription is about, if the trace records one.
    ///
    /// `None` means the ascriber is modelling something that is not there —
    /// see [`projections`](Self::projections).
    #[must_use]
    pub fn counterpart_of<'a>(&'a self, ascription: &Trope) -> Option<&'a Trope> {
        self.tropes.iter().find(|t| ascription.is_ascription_of(t))
    }

    /// How wrong an ascription is: ascribed magnitude minus actual, in
    /// milliunits.
    ///
    /// `Some(0)` is an accurate model. `None` means either side has no
    /// recorded value, or there is no counterpart at all — absence of
    /// evidence, reported as such rather than as agreement.
    ///
    /// This is the engine's thesis reduced to an integer: a model that can be
    /// wrong, and by how much.
    #[must_use]
    pub fn divergence(&self, ascription: &Trope) -> Option<i32> {
        let actual = self.counterpart_of(ascription)?;
        Some(self.latest_value(ascription.id)? - self.latest_value(actual.id)?)
    }

    /// Ascriptions with no counterpart: states an agent models in someone who
    /// does not have them.
    ///
    /// Pure projection. A guard who ascribes `intent: expose-secret` to every
    /// passer-by is reading his own preoccupation into other people, and this
    /// query is what makes that visible without anyone scripting it — his
    /// paranoia becomes data.
    pub fn projections(&self) -> impl Iterator<Item = &Trope> {
        self.tropes
            .iter()
            .filter(|t| t.is_ascription() && self.counterpart_of(t).is_none())
    }

    /// Every trope held by `holder`, ground truth and ascriptions alike.
    pub fn held_by<'a>(&'a self, holder: &'a str) -> impl Iterator<Item = &'a Trope> {
        self.tropes.iter().filter(move |t| t.holder() == holder)
    }

    /// Every trope that resembles `subject` but is borne by someone else.
    ///
    /// The primitive that [`Domain::SocialRelational`] reasoning is built from.
    pub fn peers_of<'a>(&'a self, subject: &'a Trope) -> impl Iterator<Item = &'a Trope> {
        self.tropes.iter().filter(move |t| subject.is_peer_of(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trope(id: u64, bearer: &str, kind: &str, domain: Domain) -> Trope {
        Trope {
            id: TropeId(id),
            holders: Vec::new(),
            bearer: bearer.into(),
            about: None,
            kind: kind.into(),
            domain,
        }
    }

    /// `holders` ascribing the state that `trope(..)` would build as ground.
    fn ascribed(id: u64, holders: &[&str], bearer: &str, kind: &str, domain: Domain) -> Trope {
        Trope {
            holders: holders.iter().map(|h| (*h).to_string()).collect(),
            ..trope(id, bearer, kind, domain)
        }
    }

    fn about(t: Trope, o: &str) -> Trope {
        Trope {
            about: Some(o.into()),
            ..t
        }
    }

    fn valued(id: u64, tick: u64, trope: u64, value_milli: i32) -> TraceEvent {
        TraceEvent {
            id,
            tick,
            trope: TropeId(trope),
            value_milli,
            confidence: None,
            revision: Revision::Assert,
            caused_by: vec![],
        }
    }

    fn event(id: u64, tick: u64, trope: u64, caused_by: Vec<u64>) -> TraceEvent {
        TraceEvent {
            id,
            tick,
            trope: TropeId(trope),
            value_milli: 500,
            confidence: None,
            revision: Revision::Assert,
            caused_by,
        }
    }

    /// The chain every domain must pass through, kept explicit end to end.
    fn appraisal_chain() -> Trace {
        let domains = [
            Domain::Mechanical,
            Domain::Epistemic,
            Domain::Affective,
            Domain::Conative,
            Domain::BehaviouralSelection,
            Domain::SocialRelational,
        ];
        let kinds = [
            "signal_changed",
            "possible_cause",
            "arousal_changed",
            "goal_priority_changed",
            "verification_selected",
            "protect_relation",
        ];
        Trace {
            tropes: domains
                .into_iter()
                .zip(kinds)
                .enumerate()
                .map(|(i, (domain, kind))| trope(i as u64 + 1, "agent", kind, domain))
                .collect(),
            events: (0..6)
                .map(|i| event(i + 1, 12, i + 1, if i == 0 { vec![] } else { vec![i] }))
                .collect(),
        }
    }

    #[test]
    fn deterministic_appraisal_chain_keeps_every_domain_explicit() {
        let trace = appraisal_chain();
        assert_eq!(trace.validate(), Ok(()));
        assert_eq!(
            trace.tropes.iter().map(|t| t.domain).collect::<Vec<_>>(),
            vec![
                Domain::Mechanical,
                Domain::Epistemic,
                Domain::Affective,
                Domain::Conative,
                Domain::BehaviouralSelection,
                Domain::SocialRelational,
            ]
        );
    }

    #[test]
    fn causality_must_point_backwards() {
        let trace = Trace {
            tropes: vec![trope(1, "agent", "anxiety", Domain::Affective)],
            events: vec![event(1, 0, 1, vec![2]), event(2, 0, 1, vec![])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("not strictly earlier")),
            "expected an ordering fault, got {errors:?}"
        );
    }

    #[test]
    fn an_absent_cause_is_distinguished_from_an_out_of_order_one() {
        // The whole point of the two-pass validator: these were previously one
        // message ("absent or not earlier") and could not be told apart.
        let trace = Trace {
            tropes: vec![trope(1, "agent", "anxiety", Domain::Affective)],
            events: vec![event(1, 5, 1, vec![99])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("not in the trace")),
            "an absent cause must say so: {errors:?}"
        );
        assert!(
            !errors.iter().any(|e| e.contains("not strictly earlier")),
            "an absent cause is not an ordering fault: {errors:?}"
        );
    }

    #[test]
    fn a_shuffled_trace_with_sound_causality_is_valid() {
        // Causality is judged by (tick, id), not by position, so a caller is
        // not required to sort before validating. Under the previous
        // single-pass check this trace failed purely for arriving unsorted.
        let mut trace = appraisal_chain();
        trace.events.reverse();
        assert_eq!(trace.validate(), Ok(()));
        assert!(!trace.is_canonically_ordered());

        trace.canonicalise();
        assert!(trace.is_canonically_ordered());
        assert_eq!(trace.validate(), Ok(()));
    }

    #[test]
    fn an_event_must_be_about_a_declared_trope() {
        let trace = Trace {
            tropes: vec![],
            events: vec![event(1, 0, 7, vec![])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("not declared")),
            "{errors:?}"
        );
    }

    #[test]
    fn two_bearers_of_one_kind_are_distinct_particulars_that_resemble() {
        let billy = Trope {
            about: Some("note".into()),
            ..trope(1, "billy", "suspicion", Domain::Epistemic)
        };
        let dave = Trope {
            about: Some("note".into()),
            ..trope(2, "dave", "suspicion", Domain::Epistemic)
        };

        assert_ne!(
            billy, dave,
            "same kind, different bearer: not the same trope"
        );
        assert!(billy.resembles(&dave));
        assert!(billy.is_peer_of(&dave));
        assert!(!billy.is_peer_of(&billy), "a trope is not its own peer");
    }

    #[test]
    fn resemblance_respects_domain_and_intentional_object() {
        let felt = trope(1, "billy", "alarm", Domain::Affective);
        let known = trope(2, "dave", "alarm", Domain::Epistemic);
        assert!(
            !felt.resembles(&known),
            "feeling alarm is not knowing alarm"
        );

        let about_note = Trope {
            about: Some("note".into()),
            ..trope(3, "billy", "suspicion", Domain::Epistemic)
        };
        let about_usb = Trope {
            about: Some("usb".into()),
            ..trope(4, "dave", "suspicion", Domain::Epistemic)
        };
        assert!(
            !about_note.resembles(&about_usb),
            "different object, different kind of state"
        );
    }

    #[test]
    fn peers_finds_every_other_bearer_of_a_resembling_trope() {
        let trace = Trace {
            tropes: vec![
                trope(1, "billy", "suspicion", Domain::Epistemic),
                trope(2, "dave", "suspicion", Domain::Epistemic),
                trope(3, "erin", "suspicion", Domain::Epistemic),
                trope(4, "dave", "calm", Domain::Affective),
            ],
            events: vec![],
        };
        let billy = &trace.tropes[0];
        let peers: Vec<_> = trace.peers_of(billy).map(|t| t.bearer.as_str()).collect();
        assert_eq!(peers, vec!["dave", "erin"]);
    }

    // ── Theory of mind ──────────────────────────────────────────────────────

    #[test]
    fn an_ascription_is_held_by_the_ascriber_not_the_bearer() {
        let ground = trope(1, "anya", "suspicion", Domain::Epistemic);
        let billys = ascribed(2, &["billy"], "anya", "suspicion", Domain::Epistemic);

        assert_eq!(ground.holder(), "anya");
        assert!(!ground.is_ascription());
        assert_eq!(ground.order(), 0);

        assert_eq!(billys.holder(), "billy", "Billy holds his model of Anya");
        assert!(billys.is_ascription());
        assert_eq!(billys.order(), 1);
        assert!(billys.is_ascription_of(&ground));
        assert!(
            !ground.is_ascription_of(&billys),
            "ground truth ascribes nothing"
        );
    }

    #[test]
    fn second_order_ascribes_the_first_order_not_the_ground() {
        let ground = trope(1, "dave", "fear", Domain::Affective);
        let anyas = ascribed(2, &["anya"], "dave", "fear", Domain::Affective);
        let billys = ascribed(3, &["billy", "anya"], "dave", "fear", Domain::Affective);

        assert!(billys.is_ascription_of(&anyas), "Billy models Anya's model");
        assert!(
            !billys.is_ascription_of(&ground),
            "second order is about the first order, not about Dave directly"
        );
        assert!(anyas.is_ascription_of(&ground));
    }

    #[test]
    fn a_self_model_is_a_legitimate_particular() {
        // ["anya"] over bearer "anya" is Anya's model of her own state, which
        // is NOT the same thing as the state: introspection can be wrong, and
        // an engine whose thesis is "models can be wrong" must not rule out
        // being wrong about oneself.
        let actual = trope(1, "anya", "calm", Domain::Affective);
        let self_model = ascribed(2, &["anya"], "anya", "calm", Domain::Affective);
        assert!(self_model.is_ascription_of(&actual));
        assert_ne!(actual, self_model);

        let trace = Trace {
            tropes: vec![actual, self_model],
            events: vec![],
        };
        assert_eq!(
            trace.validate(),
            Ok(()),
            "metacognition is not a contract breach"
        );
    }

    #[test]
    fn nesting_beyond_the_declared_order_is_rejected() {
        let deep = ascribed(1, &["a", "b", "c"], "d", "suspicion", Domain::Epistemic);
        let trace = Trace {
            tropes: vec![deep],
            events: vec![],
        };

        let errors = trace.validate().unwrap_err(); // default max order = 1
        assert!(
            errors
                .iter()
                .any(|e| e.contains("beyond the declared maximum")),
            "{errors:?}"
        );
        assert_eq!(
            trace.validate_to_order(3),
            Ok(()),
            "explicitly permitted depth"
        );
    }

    #[test]
    fn an_empty_name_in_the_path_is_rejected() {
        let bad = ascribed(1, &["billy", ""], "anya", "x", Domain::Epistemic);
        let trace = Trace {
            tropes: vec![bad],
            events: vec![],
        };
        let errors = trace.validate_to_order(2).unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("empty name")),
            "{errors:?}"
        );
    }

    #[test]
    fn divergence_measures_how_wrong_a_model_is() {
        let trace = Trace {
            tropes: vec![
                trope(1, "anya", "suspicion", Domain::Epistemic),
                ascribed(2, &["billy"], "anya", "suspicion", Domain::Epistemic),
                ascribed(3, &["erin"], "anya", "suspicion", Domain::Epistemic),
            ],
            events: vec![
                valued(1, 10, 1, 30_000), // Anya is mildly suspicious
                valued(2, 10, 2, 30_000), // Billy reads her exactly right
                valued(3, 10, 3, 90_000), // Erin thinks she is furious
            ],
        };
        let billys = &trace.tropes[1];
        let erins = &trace.tropes[2];

        assert_eq!(trace.divergence(billys), Some(0), "an accurate model");
        assert_eq!(trace.divergence(erins), Some(60_000), "wrong by 60 units");
        assert_eq!(
            trace.divergence(&trace.tropes[0]),
            None,
            "ground truth cannot diverge from itself"
        );
    }

    #[test]
    fn the_guilty_conscience_trap_shows_up_as_projection() {
        // A guard hiding something ascribes `expose-secret` intent to a
        // passer-by who has no such intent. Nobody scripted a tell; his own
        // belief state betrays him, and the query finds it.
        let trace = Trace {
            tropes: vec![
                about(
                    trope(1, "player", "walking", Domain::Mechanical),
                    "corridor",
                ),
                about(
                    ascribed(
                        2,
                        &["guard"],
                        "player",
                        "intent_expose_secret",
                        Domain::Conative,
                    ),
                    "secret",
                ),
                // A correctly-grounded ascription, to prove the query
                // discriminates rather than flagging every model.
                about(
                    trope(3, "player", "curiosity", Domain::Affective),
                    "corridor",
                ),
                about(
                    ascribed(4, &["guard"], "player", "curiosity", Domain::Affective),
                    "corridor",
                ),
            ],
            events: vec![],
        };

        let projected: Vec<_> = trace.projections().map(|t| t.kind.as_str()).collect();
        assert_eq!(
            projected,
            vec!["intent_expose_secret"],
            "only the ungrounded ascription is projection"
        );
    }

    #[test]
    fn sally_anne_the_ascription_tracks_sallys_evidence_not_the_world() {
        // The false-belief task, executable. The marble starts in the basket,
        // Sally sees it, Sally leaves, Anne moves it to the box. Sally's
        // belief must not update — nothing reached her — and an observer with
        // working theory of mind ascribes the STALE belief, not the world.
        const BASKET: i32 = 1_000;
        const BOX: i32 = 2_000;

        let trace = Trace {
            tropes: vec![
                about(
                    trope(1, "world", "marble_location", Domain::Mechanical),
                    "marble",
                ),
                about(
                    trope(2, "sally", "marble_location", Domain::Epistemic),
                    "marble",
                ),
                about(
                    trope(3, "anne", "marble_location", Domain::Epistemic),
                    "marble",
                ),
                about(
                    ascribed(
                        4,
                        &["observer"],
                        "sally",
                        "marble_location",
                        Domain::Epistemic,
                    ),
                    "marble",
                ),
            ],
            events: vec![
                // t=1 marble in basket; Sally and Anne both witness it.
                valued(1, 1, 1, BASKET),
                TraceEvent {
                    caused_by: vec![1],
                    ..valued(2, 1, 2, BASKET)
                },
                TraceEvent {
                    caused_by: vec![1],
                    ..valued(3, 1, 3, BASKET)
                },
                // t=3 Anne moves it. The world changes; Anne witnesses it.
                valued(4, 3, 1, BOX),
                TraceEvent {
                    caused_by: vec![4],
                    ..valued(5, 3, 3, BOX)
                },
                // Sally: NO event. Absence of provenance is the whole test —
                // a belief only moves when something reaches its holder.
                // t=4 the observer ascribes what Sally must still think.
                TraceEvent {
                    caused_by: vec![2],
                    ..valued(6, 4, 4, BASKET)
                },
            ],
        };
        assert_eq!(trace.validate(), Ok(()));

        assert_eq!(
            trace.latest_value(TropeId(1)),
            Some(BOX),
            "the world moved on"
        );
        assert_eq!(
            trace.latest_value(TropeId(3)),
            Some(BOX),
            "Anne saw it move"
        );
        assert_eq!(
            trace.latest_value(TropeId(2)),
            Some(BASKET),
            "Sally's belief is stale, because nothing updated it"
        );

        let observer = &trace.tropes[3];
        assert_eq!(
            trace.divergence(observer),
            Some(0),
            "PASSES SALLY-ANNE: the ascription tracks Sally's belief exactly"
        );
        assert_ne!(
            trace.latest_value(observer.id),
            trace.latest_value(TropeId(1)),
            "...and therefore differs from the world, which is the point"
        );

        // The failure mode, for contrast: an observer without theory of mind
        // reports the world and is wrong about Sally by exactly the distance
        // the marble travelled.
        let mut naive = trace.clone();
        naive.events.push(TraceEvent {
            caused_by: vec![4],
            ..valued(7, 5, 4, BOX)
        });
        assert_eq!(
            naive.divergence(&naive.tropes[3].clone()),
            Some(BOX - BASKET)
        );
    }

    #[test]
    fn held_by_separates_an_agents_world_from_the_world() {
        let trace = Trace {
            tropes: vec![
                trope(1, "anya", "suspicion", Domain::Epistemic),
                ascribed(2, &["billy"], "anya", "suspicion", Domain::Epistemic),
                ascribed(3, &["billy"], "dave", "fear", Domain::Affective),
                trope(4, "billy", "resolve", Domain::Conative),
            ],
            events: vec![],
        };
        let billys: Vec<_> = trace.held_by("billy").map(|t| t.id.0).collect();
        assert_eq!(billys, vec![2, 3, 4], "his models and his own states");
        assert_eq!(trace.held_by("anya").count(), 1, "her actual state only");
    }

    // ── Revision and confidence (C2) ────────────────────────────────────────

    fn retract(id: u64, tick: u64, trope: u64, caused_by: Vec<u64>) -> TraceEvent {
        TraceEvent {
            revision: Revision::Retract,
            ..event(id, tick, trope, caused_by)
        }
    }

    #[test]
    fn a_retraction_must_cite_its_contradicting_evidence() {
        let trace = Trace {
            tropes: vec![trope(1, "guard", "belief", Domain::Epistemic)],
            events: vec![event(1, 1, 1, vec![]), retract(2, 2, 1, vec![])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("retraction with no cause")),
            "{errors:?}"
        );
    }

    #[test]
    fn only_epistemic_tropes_can_be_retracted() {
        let trace = Trace {
            tropes: vec![trope(1, "guard", "arousal", Domain::Affective)],
            events: vec![event(1, 1, 1, vec![]), retract(2, 2, 1, vec![1])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("only Epistemic tropes can be retracted")),
            "revision is an epistemic act: {errors:?}"
        );
    }

    #[test]
    fn a_retraction_needs_a_prior_assertion() {
        // You cannot discover you were deceived about something you never
        // believed.
        let trace = Trace {
            tropes: vec![
                trope(1, "guard", "belief", Domain::Epistemic),
                trope(2, "world", "fact", Domain::Mechanical),
            ],
            events: vec![event(1, 1, 2, vec![]), retract(2, 2, 1, vec![1])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("no earlier assertion")),
            "{errors:?}"
        );
    }

    #[test]
    fn confidence_crosses_only_at_the_epistemic_seam() {
        // ADR-0016's scope rule as a validator: affect intensities quantise
        // to Mass at the crossing (the epistemic event), never on the
        // affective trope itself.
        let confident = |t: u64| TraceEvent {
            confidence: Some(Mass::new(8_000).unwrap()),
            ..event(1, 1, t, vec![])
        };
        let affective = Trace {
            tropes: vec![trope(1, "guard", "arousal", Domain::Affective)],
            events: vec![confident(1)],
        };
        let errors = affective.validate().unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("confidence crosses only at the epistemic seam")),
            "{errors:?}"
        );

        let epistemic = Trace {
            tropes: vec![trope(1, "guard", "belief", Domain::Epistemic)],
            events: vec![confident(1)],
        };
        assert_eq!(epistemic.validate(), Ok(()));
    }

    #[test]
    fn the_belief_contract_profile_demands_provenance_and_confidence() {
        // Base-valid but contract-bare: an epistemic assertion with neither.
        let trace = Trace {
            tropes: vec![trope(1, "guard", "belief", Domain::Epistemic)],
            events: vec![event(1, 1, 1, vec![])],
        };
        assert_eq!(trace.validate(), Ok(()), "the base contract still accepts");
        let errors = trace.validate_belief_contract().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("no provenance")));
        assert!(errors.iter().any(|e| e.contains("no confidence")));
    }

    #[test]
    fn held_value_reads_absence_after_retraction_while_latest_value_stays_telemetry() {
        let trace = Trace {
            tropes: vec![trope(1, "guard", "belief", Domain::Epistemic)],
            events: vec![
                valued(1, 1, 1, 42_000),
                TraceEvent {
                    value_milli: 42_000,
                    ..retract(2, 3, 1, vec![1])
                },
            ],
        };
        assert_eq!(trace.validate(), Ok(()));
        assert_eq!(trace.held_value(TropeId(1)), None, "withdrawn = absence");
        assert_eq!(
            trace.latest_value(TropeId(1)),
            Some(42_000),
            "telemetry still shows the withdrawn content"
        );
        assert_eq!(trace.retractions().count(), 1);
    }

    #[test]
    fn pre_revision_traces_parse_and_default_events_serialize_unchanged() {
        // Serde back-compat both ways: the new fields default on the way in
        // and are omitted on the way out, so pre-C2 traces and their byte
        // encodings are untouched.
        let old_shape = r#"{"id":1,"tick":5,"trope":7,"value_milli":250,"caused_by":[]}"#;
        let parsed: TraceEvent = serde_json::from_str(old_shape).unwrap();
        assert_eq!(parsed.confidence, None);
        assert_eq!(parsed.revision, Revision::Assert);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), old_shape);

        let retraction = TraceEvent {
            confidence: Some(Mass::new(1_000).unwrap()),
            revision: Revision::Retract,
            ..parsed
        };
        let json = serde_json::to_string(&retraction).unwrap();
        assert!(json.contains(r#""revision":"retract""#));
        assert!(json.contains(r#""confidence":1000"#));
        assert_eq!(
            serde_json::from_str::<TraceEvent>(&json).unwrap(),
            retraction
        );
    }

    #[test]
    fn duplicate_ids_are_caught_for_both_tropes_and_events() {
        let trace = Trace {
            tropes: vec![
                trope(1, "agent", "a", Domain::Affective),
                trope(1, "agent", "b", Domain::Affective),
            ],
            events: vec![event(1, 0, 1, vec![]), event(1, 1, 1, vec![])],
        };
        let errors = trace.validate().unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("duplicate trope id")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("duplicate trace event id")),
            "{errors:?}"
        );
    }
}
