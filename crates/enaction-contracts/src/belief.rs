// SPDX-License-Identifier: AGPL-3.0-or-later
//! The minimum epistemic data model (CAC-KERNEL §3) and the interval-derived
//! belief status (ADR-0016).

use enaction_trace::Mass;
use serde::{Deserialize, Serialize};

/// Identity of an epistemic event within one trace. Globally unique there —
/// CAC-KERNEL §3 invariant 1, enforced by the package validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

/// Identity of a belief. What persists; events are what happened to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BeliefId(pub u64);

/// What kind of thing brought evidence in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Observation,
    Communication,
    Inference,
    ActionOutcome,
    Correction,
}

/// What an event is about: another agent, or the world itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    Agent(String),
    World,
}

/// A proposition, opaque to the kernel.
///
/// The proposition language is the contract's hardest open problem
/// (CAC-KERNEL §8) and deliberately unsettled: **the kernel never reads
/// this**, exactly as it never reads `Trope::kind`. A game or profile supplies
/// the vocabulary; the kernel supplies identity, ordering and provenance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Proposition(pub String);

/// Where a piece of evidence came from. CAC-KERNEL §3 declares the field and
/// (deliberately) not its contents; this is the minimum that lets a reviewer
/// ask "who said so, over what channel" without inventing a provenance theory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The producing source — an agent name, a sensor, a scenario author.
    pub origin: String,
    /// The channel it arrived over, where the profile distinguishes channels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

/// An affect annotation: ADR-0014 crossing A, as a type.
///
/// A token from a profile-declared ordered vocabulary, plus an optional
/// `Mass`-typed modulation of *attention and priority*. There is deliberately
/// no field on which affect could assert a proposition or a truth — **affect
/// buys attention, never belief** — so the rule is enforced by construction
/// rather than by review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffectAnnotation {
    /// The argmax over the profile's declared affect vocabulary.
    pub token: String,
    /// Attention/priority modulation. Never truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention: Option<Mass>,
}

/// A conative annotation: only the argmax crosses (ADR-0016).
///
/// Utilities are internal rankings, invariant under monotone rescaling, and
/// never leave the conative layer — so there is no utility field here to leak
/// one through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConativeAnnotation {
    /// The selected option, by the profile's declared tie-break.
    pub selected: String,
}

/// One epistemic event: evidence arriving, with provenance (CAC-KERNEL §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpistemicEvent {
    pub event_id: EventId,
    /// Order within a tick. `(tick, sequence)` is the total reduction order.
    pub sequence: u64,
    pub tick: u64,
    /// Who received the evidence.
    pub observer: String,
    /// Who or what the evidence is about.
    pub subject: Subject,
    pub kind: EventKind,
    pub proposition: Proposition,
    pub confidence: Mass,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affect: Option<AffectAnnotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conation: Option<ConativeAnnotation>,
}

impl EpistemicEvent {
    /// The total order for reduction: by tick, then sequence (CAC-KERNEL §3
    /// invariant 2).
    #[must_use]
    pub fn stamp(&self) -> (u64, u64) {
        (self.tick, self.sequence)
    }
}

/// A belief: the durable output of reduction (CAC-KERNEL §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Belief {
    pub belief_id: BeliefId,
    /// The ascription path, outermost first — the SAME representation as
    /// `enaction_trace::Trope::holders`, so a belief and its trace tropes
    /// agree on what nesting *is*. Empty means the bearer's own belief;
    /// `["billy"]` is Billy's model of the bearer's belief. Length is the
    /// order of theory of mind, bounded by [`DepthPolicy`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holders: Vec<String>,
    /// Whose belief this is (or is modelled to be).
    pub bearer: String,
    pub proposition: Proposition,
    pub status: BeliefStatus,
    /// The belief–plausibility interval, not a point mass (ADR-0016):
    /// ignorance, disagreement and ambiguity are three different things.
    pub confidence: BeliefInterval,
    /// Provenance: the events this belief rests on. CAC-KERNEL §3 invariant 3
    /// — every non-derived belief names at least one — is enforced by the
    /// package validator.
    pub source_events: Vec<EventId>,
    /// The tick from which this belief holds.
    pub valid_from: u64,
    /// Contradiction, made explicit rather than silently overwritten
    /// (CAC-KERNEL §3 invariant 4). The link names its cause: see
    /// [`Supersession`] (ADR-0030).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Supersession>,
}

/// A supersession link that names its cause (kernel contract 2.0, ADR-0030).
///
/// Both members are required, so a link with no cause has no wire form and
/// no Rust value: `"supersedes": 1` and `"supersedes": {"belief": 1}` are
/// refused by the decoder as `BadShape`. What the cause must be (an event of the
/// package, received by the revising mind, cited by the successor, new, and
/// in order) is checked by the six `supersession-cause-*` package rules; a
/// type cannot see the package's events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supersession {
    /// The belief this one replaces.
    pub belief: BeliefId,
    /// The event that licensed the overwrite: the contradiction,
    /// observation or correction the revising mind received. An
    /// [`EpistemicEvent::event_id`] of the same package.
    pub because: EventId,
}

impl Belief {
    /// The order of theory of mind: 0 = the bearer's own belief.
    #[must_use]
    pub fn order(&self) -> usize {
        self.holders.len()
    }
}

/// The declared bound on theory-of-mind nesting (CAC-KERNEL §3 invariant 5).
///
/// Counted in deterministic units — nesting order — never in milliseconds
/// (ADR-0015). The default is the trace crate's declaration, one authority
/// for the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthPolicy {
    pub max_order: usize,
}

impl Default for DepthPolicy {
    fn default() -> Self {
        DepthPolicy {
            max_order: enaction_trace::DEFAULT_MAX_TOM_ORDER,
        }
    }
}

/// A belief's standing, derived from its interval — never from a hardcoded
/// threshold (ADR-0016). `Unknown` is first-class: distinct from rejected and
/// from false (CAC-KERNEL §3 invariant 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefStatus {
    /// Full belief mass: `belief == FULL`. Certainty of *commitment*, not
    /// knowledge: mass certainty never reads as truth. Factive knowledge
    /// needs world truth, which a belief does not carry (rung 2, "Certain is
    /// not Known"; renamed from `Known`, completion-plan step 11).
    Certain,
    Believed,
    Suspected,
    Rejected,
    Unknown,
}

/// A belief–plausibility interval.
///
/// Invariant: `belief <= plausibility`, enforced by the constructor and at
/// the serde boundary, so an inverted interval is unrepresentable. The gap
/// between them is *ignorance* — uncommitted mass — which is exactly what a
/// single scalar cannot carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawInterval", into = "RawInterval")]
pub struct BeliefInterval {
    belief: Mass,
    plausibility: Mass,
}

/// The unvalidated wire shape of an interval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInterval {
    belief: Mass,
    plausibility: Mass,
}

/// An interval whose belief exceeded its plausibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvertedInterval {
    pub belief: Mass,
    pub plausibility: Mass,
}

impl std::fmt::Display for InvertedInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "belief {} exceeds plausibility {}",
            self.belief.get(),
            self.plausibility.get()
        )
    }
}

impl std::error::Error for InvertedInterval {}

impl TryFrom<RawInterval> for BeliefInterval {
    type Error = InvertedInterval;

    fn try_from(raw: RawInterval) -> Result<Self, Self::Error> {
        BeliefInterval::new(raw.belief, raw.plausibility).ok_or(InvertedInterval {
            belief: raw.belief,
            plausibility: raw.plausibility,
        })
    }
}

impl From<BeliefInterval> for RawInterval {
    fn from(interval: BeliefInterval) -> RawInterval {
        RawInterval {
            belief: interval.belief,
            plausibility: interval.plausibility,
        }
    }
}

impl BeliefInterval {
    /// An interval, if `belief <= plausibility`. A content gap must not
    /// panic, so the inverted case is a value, not an abort.
    #[must_use]
    pub fn new(belief: Mass, plausibility: Mass) -> Option<BeliefInterval> {
        (belief <= plausibility).then_some(BeliefInterval {
            belief,
            plausibility,
        })
    }

    /// Full commitment: belief and plausibility both at scale.
    #[must_use]
    pub fn certain() -> BeliefInterval {
        BeliefInterval {
            belief: Mass::FULL,
            plausibility: Mass::FULL,
        }
    }

    /// Total ignorance: nothing committed either way.
    #[must_use]
    pub fn vacuous() -> BeliefInterval {
        BeliefInterval {
            belief: Mass::ZERO,
            plausibility: Mass::FULL,
        }
    }

    #[must_use]
    pub fn belief(self) -> Mass {
        self.belief
    }

    #[must_use]
    pub fn plausibility(self) -> Mass {
        self.plausibility
    }

    /// The uncommitted mass: how much is *ignorance* rather than disagreement.
    #[must_use]
    pub fn ignorance(self) -> Mass {
        self.plausibility.saturating_sub(self.belief)
    }
}

/// A versioned profile policy mapping an interval to a status (ADR-0016).
///
/// The policy is data, not code, and carries an identifier so a trace can
/// record *which* policy performed the mapping — the lossy step stays
/// auditable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusPolicy {
    /// Versioned policy identifier, recorded wherever the mapping is applied.
    pub id: String,
    /// At or below this plausibility, the proposition is `Rejected`.
    pub rejected_ceiling: Mass,
    /// An ignorance gap at or above this width reads `Unknown` — ignorance,
    /// not disagreement — *before* the belief floor is consulted.
    pub unknown_band: Mass,
    /// At or above this belief, `Believed`.
    pub believed_floor: Mass,
    /// At or above this plausibility, `Suspected`.
    pub suspected_floor: Mass,
}

impl StatusPolicy {
    /// Map an interval to a status. The evaluation order is **normative**
    /// (ADR-0016): Certain, Rejected, Unknown-by-ignorance, Believed,
    /// Suspected, otherwise Unknown. Reordering it changes meaning — in
    /// particular, a wide-ignorance interval must read `Unknown` even when
    /// its belief clears the believed floor.
    #[must_use]
    pub fn status(&self, interval: BeliefInterval) -> BeliefStatus {
        if interval.belief() == Mass::FULL {
            BeliefStatus::Certain
        } else if interval.plausibility() <= self.rejected_ceiling {
            BeliefStatus::Rejected
        } else if interval.ignorance() >= self.unknown_band {
            BeliefStatus::Unknown
        } else if interval.belief() >= self.believed_floor {
            BeliefStatus::Believed
        } else if interval.plausibility() >= self.suspected_floor {
            BeliefStatus::Suspected
        } else {
            BeliefStatus::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mass(v: u16) -> Mass {
        Mass::new(v).unwrap()
    }

    fn interval(belief: u16, plausibility: u16) -> BeliefInterval {
        BeliefInterval::new(mass(belief), mass(plausibility)).unwrap()
    }

    fn policy() -> StatusPolicy {
        StatusPolicy {
            id: "test/status/v1".into(),
            rejected_ceiling: mass(1_000),
            unknown_band: mass(6_000),
            believed_floor: mass(7_000),
            suspected_floor: mass(5_000),
        }
    }

    #[test]
    fn an_inverted_interval_is_unrepresentable() {
        assert!(BeliefInterval::new(mass(6_000), mass(4_000)).is_none());
        let err =
            serde_json::from_str::<BeliefInterval>(r#"{"belief": 6000, "plausibility": 4000}"#);
        assert!(
            err.is_err(),
            "the serde boundary enforces the invariant too"
        );
    }

    #[test]
    fn a_valid_interval_round_trips_through_serde() {
        let original = interval(3_000, 8_000);
        let json = serde_json::to_string(&original).unwrap();
        let back: BeliefInterval = serde_json::from_str(&json).unwrap();
        assert_eq!(original, back);
        assert_eq!(back.ignorance(), mass(5_000));
    }

    #[test]
    fn the_normative_evaluation_order_is_walked_in_order() {
        let p = policy();
        assert_eq!(p.status(BeliefInterval::certain()), BeliefStatus::Certain);
        assert_eq!(p.status(interval(0, 500)), BeliefStatus::Rejected);
        assert_eq!(p.status(interval(8_000, 9_000)), BeliefStatus::Believed);
        assert_eq!(p.status(interval(2_000, 6_000)), BeliefStatus::Suspected);
        assert_eq!(p.status(interval(2_000, 4_000)), BeliefStatus::Unknown);
    }

    #[test]
    fn ignorance_outranks_the_believed_floor() {
        // The order-matters case ADR-0016 exists to protect: belief 7000
        // clears the believed floor, but the 3000-wide gap... is under the
        // band here, so tighten: belief 7000 / plausibility 10000 has a
        // 3000 gap (< 6000 band) and reads Believed; belief 3000 /
        // plausibility 10000 has a 7000 gap and MUST read Unknown even
        // though its plausibility clears the suspected floor.
        let p = policy();
        assert_eq!(p.status(interval(7_000, 10_000)), BeliefStatus::Believed);
        assert_eq!(
            p.status(interval(3_000, 10_000)),
            BeliefStatus::Unknown,
            "wide ignorance is Unknown, not Suspected — order is normative"
        );
        assert_eq!(p.status(BeliefInterval::vacuous()), BeliefStatus::Unknown);
    }

    #[test]
    fn belief_nesting_uses_the_trace_crates_representation() {
        let own = Belief {
            belief_id: BeliefId(1),
            holders: vec![],
            bearer: "sally".into(),
            proposition: Proposition("marble in basket".into()),
            status: BeliefStatus::Believed,
            confidence: interval(8_000, 9_500),
            source_events: vec![EventId(1)],
            valid_from: 1,
            supersedes: None,
        };
        let ascribed = Belief {
            belief_id: BeliefId(2),
            holders: vec!["observer".into()],
            ..own.clone()
        };
        assert_eq!(own.order(), 0);
        assert_eq!(ascribed.order(), 1);
        assert_eq!(
            DepthPolicy::default().max_order,
            enaction_trace::DEFAULT_MAX_TOM_ORDER,
            "one authority for the depth default"
        );
    }

    #[test]
    fn a_supersession_link_without_a_cause_has_no_wire_form() {
        // Owner ruling D1. The 1.x form (a bare belief id) and a link with
        // either member missing are refused by the decoder; nothing reaches
        // the validator.
        let with = |supersedes: &str| {
            format!(
                r#"{{"belief_id":2,"bearer":"sally","proposition":"p","status":"believed",
                    "confidence":{{"belief":8000,"plausibility":9500}},"source_events":[1],
                    "valid_from":3,"supersedes":{supersedes}}}"#
            )
        };
        let parsed = serde_json::from_str::<Belief>(&with(r#"{"belief":1,"because":7}"#)).unwrap();
        assert_eq!(
            parsed.supersedes,
            Some(Supersession {
                belief: BeliefId(1),
                because: EventId(7)
            })
        );
        assert_eq!(
            serde_json::to_value(&parsed).unwrap()["supersedes"],
            serde_json::json!({"belief": 1, "because": 7}),
            "the link round-trips"
        );
        for refused in [
            "1",
            r#"{"belief":1}"#,
            r#"{"because":7}"#,
            r#"{"belief":1,"because":null}"#,
            r#"{"belief":1,"because":7,"note":"x"}"#,
        ] {
            assert!(
                serde_json::from_str::<Belief>(&with(refused)).is_err(),
                "{refused} must be refused"
            );
        }
    }

    #[test]
    fn affect_annotation_has_no_field_that_could_carry_truth() {
        // Enforced by construction: the only payload beyond the token is a
        // Mass-typed attention modulation. This test is documentation that
        // the shape is deliberate.
        let annotation = AffectAnnotation {
            token: "alarm".into(),
            attention: Some(mass(2_500)),
        };
        let json = serde_json::to_string(&annotation).unwrap();
        assert_eq!(json, r#"{"token":"alarm","attention":2500}"#);
    }
}
