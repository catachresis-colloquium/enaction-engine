// SPDX-License-Identifier: MPL-2.0
//! Interest and belief kinetics, extracted from the IDApTIK proving ground.
//!
//! Origin: `idaptik-core/src/scenario/actor/belief.rs` — the arithmetic behind
//! an actor's object theory, already factored out of one guard's behaviour so
//! every archetype shared it. That double service (one game, many archetypes)
//! is what qualified it for extraction under ADR-0017; this module is the
//! third home, and the semantics are preserved **bit for bit** (see the parity
//! tests, which pin IDApTIK's committed constants).
//!
//! # The model
//!
//! Each tracked object holds an interest meter. While the bearer observes the
//! object, the meter climbs at a per-channel rate, plus a carry bonus when the
//! object is being carried in view. While unobserved it decays — unless the
//! object is *pinned* (exposed, thrown, known-carried), in which case the
//! bearer does not forget. Belief forms over whichever meter is highest once
//! it crosses a threshold; **earlier entries win ties**, so callers list
//! objects in a deterministic order.
//!
//! # What was generalised on the way out
//!
//! * **Channels are game vocabulary.** IDApTIK's `Leakage {Still, Moving,
//!   Sprinting}` was baked into the mechanism as an enum; here the caller
//!   resolves its channel to a rate — via [`ChannelTable`], which makes the
//!   rates inspectable data rather than code — and hands the mechanism a
//!   number. The engine never learns what a channel means.
//! * **The meter advances by any monotone accumulator.** The parameter the
//!   origin called `dt` was already just a scalar; it is named `advance` here
//!   and documented as such. IDApTIK advances by seconds; Chronicles of
//!   Slavia's design advances instruction memory by *steps walked* ("memory
//!   erodes with motion") — one mechanism, both uses, zero code difference.
//! * **The ceiling is data.** The origin clamped to `0..=100`; the bound is a
//!   [`Kinetics`] field (IDApTIK sets `100.0`).
//!
//! # What deliberately did not change
//!
//! The arithmetic: operand order, the `max`-then-`min` clamp, the strict
//! inequality in the tie-break, the `>=` at the threshold. Determinism in the
//! host games is *replay-tested f64*; an "equivalent" formula that differs in
//! the last ulp is a different formula.
//!
//! # The seam
//!
//! Meters and beliefs map onto the trace as two tropes per (bearer, object)
//! pair: the meter is *salience* — [`Domain::Affective`] — and the formed
//! belief is [`Domain::Epistemic`], its events `caused_by` the meter events
//! that raised it. Appraisal precedes knowledge, structurally. Meter values
//! cross into the trace through [`to_milli`], the one explicit, documented
//! flattening (in the trope-particularity-workbench's sense) on this path.
//!
//! [`Domain::Affective`]: crate::Domain::Affective
//! [`Domain::Epistemic`]: crate::Domain::Epistemic

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Clamp preserved from the origin (`mathf::clamp`): `max` then `min`.
///
/// Not `f64::clamp`, whose NaN handling and edge semantics differ; parity
/// means reproducing the exact operations the host games replay-test.
// clippy::manual_clamp asks for f64::clamp — which is exactly the
// "equivalent" this function exists to avoid: f64::clamp propagates NaN and
// panics on inverted bounds, max-then-min resolves NaN to a bound. Parity
// with the replay-tested origin means keeping the origin's operations.
#[allow(clippy::manual_clamp)]
#[inline]
fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

/// How one bearer's attention couples to one object: the mechanism's rates,
/// with the game's meanings left outside.
///
/// This is the *shape* extracted from IDApTIK's `InterestProfile`; the fields
/// that stayed behind (`ObjectClass`, `value_signal`, the attention radius,
/// the guard timer) are game data and policy, not kinetics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Kinetics {
    /// Upper bound of the meter. The origin used `100.0`.
    pub ceiling: f64,
    /// Extra gain per unit of `advance` while the object is carried in view.
    pub carry: f64,
    /// Loss per unit of `advance` while unobserved and not pinned.
    pub decay: f64,
}

impl Kinetics {
    /// One observed step: the meter climbs by `rate` per unit of `advance`,
    /// plus the carry bonus when the object is carried in view. Clamped to
    /// `0..=ceiling`.
    ///
    /// `advance` is any monotone accumulator — seconds, ticks, steps walked.
    /// The caller resolves its observation channel to `rate` (see
    /// [`ChannelTable`]).
    #[must_use]
    pub fn observed(&self, current: f64, rate: f64, carried: bool, advance: f64) -> f64 {
        let carry = if carried { self.carry * advance } else { 0.0 };
        clamp(current + rate * advance + carry, 0.0, self.ceiling)
    }

    /// One unobserved step: the meter decays toward zero — unless the object
    /// is pinned, in which case the bearer has no reason to forget it and the
    /// meter is returned unchanged.
    #[must_use]
    pub fn unobserved(&self, current: f64, pinned: bool, advance: f64) -> f64 {
        if pinned {
            current
        } else {
            (current - self.decay * advance).max(0.0)
        }
    }
}

/// Per-channel observation rates as inspectable data.
///
/// The game owns the channel type and its meaning; the engine owns only the
/// lookup. A missing channel reads as `0.0` — the origin's "inert profile"
/// fallback: a content gap must not panic mid-simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelTable<C: Ord>(pub BTreeMap<C, f64>);

impl<C: Ord> ChannelTable<C> {
    /// The observed gain rate for `channel`, `0.0` when undeclared.
    #[must_use]
    pub fn rate(&self, channel: &C) -> f64 {
        self.0.get(channel).copied().unwrap_or(0.0)
    }

    /// The origin's `from_value_signal` scaling law, generalised: every rate
    /// scales linearly with a `0..=1` signal of how valuable the object
    /// *reads*. Decay and ceiling are properties of the bearer's memory, not
    /// of the object's shine, so [`Kinetics`] is deliberately not scaled here.
    #[must_use]
    pub fn scaled(&self, value_signal: f64) -> Self
    where
        C: Clone,
    {
        let vs = value_signal.clamp(0.0, 1.0);
        Self(self.0.iter().map(|(c, r)| (c.clone(), r * vs)).collect())
    }
}

/// The bearer's object theory: the first entry holding the highest interest,
/// if that interest has crossed `threshold`; `None` while everything is below
/// it.
///
/// Later entries replace the front-runner only when **strictly** greater, so
/// ties resolve to the earlier entry — callers must list objects in a
/// deterministic order. (IDApTIK lists the note before the usb, which
/// reproduces its prototype's `note >= usb` tie-break exactly; the same rule
/// is what makes belief formation replay-stable everywhere else.)
#[must_use]
pub fn belief_over<K: Clone>(interests: &[(K, f64)], threshold: f64) -> Option<K> {
    let mut best: Option<(&K, f64)> = None;
    for (k, v) in interests {
        match best {
            Some((_, bv)) if *v <= bv => {}
            _ => best = Some((k, *v)),
        }
    }
    best.and_then(|(k, v)| (v >= threshold).then(|| k.clone()))
}

/// Quantise a meter value to trace milliunits — the one explicit flattening
/// on the belief path.
///
/// The simulation's meters are f64 (replay-tested in the host games); the
/// trace is integer milliunits so that traces compare bit-for-bit across
/// machines. Precision below one milliunit is *deliberately discarded here
/// and nowhere else*: naming the loss is what keeps it inspectable, instead
/// of letting it happen silently at whatever seam noticed first.
///
/// Rounds half away from zero; saturates at the `i32` range.
#[must_use]
pub fn to_milli(value: f64) -> i32 {
    let scaled = (value * 1000.0).round();
    if scaled >= f64::from(i32::MAX) {
        i32::MAX
    } else if scaled <= f64::from(i32::MIN) {
        i32::MIN
    } else {
        scaled as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Domain, Trace, TraceEvent, Trope, TropeId};

    // ── IDApTIK's committed constants (scenario/constants.rs), pinned so the
    //    parity claim is against the real game, not convenient numbers. ──────
    const TICK: f64 = 1.0 / 60.0;
    const THRESHOLD: f64 = 48.0; // BELIEF_THRESHOLD
    const NOTE: Kinetics = Kinetics {
        ceiling: 100.0,
        carry: 13.0,
        decay: 0.25,
    };
    const USB: Kinetics = Kinetics {
        ceiling: 100.0,
        carry: 12.0,
        decay: 0.2,
    };
    const NOTE_URG: [f64; 3] = [4.0, 10.0, 20.0]; // still, move, sprint
    const USB_URG: [f64; 3] = [3.0, 8.0, 17.0];

    /// The origin's expression, verbatim, as an independent reference.
    #[allow(clippy::manual_clamp)] // quoting the origin — see clamp() above
    fn idaptik_observed(current: f64, urgency: f64, carry: f64, carried: bool, dt: f64) -> f64 {
        let c = if carried { carry * dt } else { 0.0 };
        (current + urgency * dt + c).max(0.0).min(100.0)
    }

    fn idaptik_unobserved(current: f64, decay: f64, pinned: bool, dt: f64) -> f64 {
        if pinned {
            current
        } else {
            (current - decay * dt).max(0.0)
        }
    }

    #[test]
    fn observed_matches_idaptik_bit_for_bit_across_a_long_run() {
        // Drive both implementations through 600 ticks of mixed channels and
        // carry states; every intermediate value must be identical to the bit.
        let mut ours = 0.0_f64;
        let mut theirs = 0.0_f64;
        for i in 0..600u32 {
            let ch = (i % 3) as usize;
            let carried = i % 7 == 0;
            ours = NOTE.observed(ours, NOTE_URG[ch], carried, TICK);
            theirs = idaptik_observed(theirs, NOTE_URG[ch], 13.0, carried, TICK);
            assert_eq!(
                ours.to_bits(),
                theirs.to_bits(),
                "diverged at tick {i}: {ours} vs {theirs}"
            );
        }
        assert!(ours > 0.0, "the run must actually have climbed");
    }

    #[test]
    fn unobserved_matches_idaptik_bit_for_bit() {
        let mut ours = 87.3_f64;
        let mut theirs = 87.3_f64;
        for i in 0..10_000u32 {
            let pinned = (i / 100) % 2 == 0;
            ours = USB.unobserved(ours, pinned, TICK);
            theirs = idaptik_unobserved(theirs, 0.2, pinned, TICK);
            assert_eq!(ours.to_bits(), theirs.to_bits(), "diverged at step {i}");
        }
    }

    #[test]
    fn the_ceiling_and_floor_are_exact() {
        let k = Kinetics {
            ceiling: 100.0,
            carry: 0.0,
            decay: 5.0,
        };
        assert_eq!(
            k.observed(99.999, 1000.0, false, 1.0),
            100.0,
            "exactly the ceiling"
        );
        assert_eq!(k.unobserved(0.001, false, 1.0), 0.0, "exactly the floor");
        assert_eq!(k.unobserved(0.0, false, 1.0), 0.0, "and stays there");
    }

    #[test]
    fn pinning_means_never_forgetting() {
        assert_eq!(NOTE.unobserved(42.5, true, 1e9), 42.5);
    }

    #[test]
    fn ties_resolve_to_the_earlier_entry() {
        // IDApTIK's note-before-usb ordering reproduces its prototype's
        // `note >= usb` tie-break; the rule itself is order-in, order-out.
        assert_eq!(
            belief_over(&[("note", 50.0), ("usb", 50.0)], THRESHOLD),
            Some("note")
        );
        assert_eq!(
            belief_over(&[("usb", 50.0), ("note", 50.0)], THRESHOLD),
            Some("usb")
        );
        assert_eq!(
            belief_over(&[("note", 50.0), ("usb", 60.0)], THRESHOLD),
            Some("usb")
        );
    }

    #[test]
    fn the_threshold_is_inclusive_and_none_below_it() {
        assert_eq!(belief_over(&[("note", 48.0)], THRESHOLD), Some("note"));
        assert_eq!(belief_over(&[("note", 47.999_999)], THRESHOLD), None);
        assert_eq!(belief_over::<&str>(&[], THRESHOLD), None);
    }

    #[test]
    fn channel_tables_are_data_and_missing_channels_are_inert() {
        let t = ChannelTable(BTreeMap::from([
            ("still", USB_URG[0]),
            ("move", USB_URG[1]),
            ("sprint", USB_URG[2]),
        ]));
        assert_eq!(t.rate(&"sprint"), 17.0);
        assert_eq!(t.rate(&"teleport"), 0.0, "a content gap must not panic");
    }

    #[test]
    fn scaling_reproduces_the_from_value_signal_law() {
        // from_value_signal: urg_still 5*vs, urg_move 11*vs, urg_sprint 21*vs.
        let reference = ChannelTable(BTreeMap::from([
            ("still", 5.0),
            ("move", 11.0),
            ("sprint", 21.0),
        ]));
        let half = reference.scaled(0.5);
        assert_eq!(half.rate(&"still"), 2.5);
        assert_eq!(half.rate(&"move"), 5.5);
        assert_eq!(half.rate(&"sprint"), 10.5);
        // The signal is clamped exactly as the origin clamps it.
        assert_eq!(reference.scaled(7.0).rate(&"still"), 5.0);
        assert_eq!(reference.scaled(-1.0).rate(&"still"), 0.0);
    }

    #[test]
    fn advance_is_any_monotone_accumulator() {
        // Chronicles' "memory erodes with motion": the same kinetics, advanced
        // by steps walked instead of seconds. Nothing in the mechanism knows
        // the difference — which is the point.
        let memory = Kinetics {
            ceiling: 100.0,
            carry: 0.0,
            decay: 1.5,
        };
        let after_12_steps = memory.unobserved(30.0, false, 12.0);
        assert_eq!(after_12_steps, 30.0 - 1.5 * 12.0);
    }

    #[test]
    fn quantisation_is_explicit_and_saturating() {
        assert_eq!(to_milli(48.0), 48_000);
        assert_eq!(to_milli(0.0), 0);
        assert_eq!(to_milli(12.345_678_9), 12_346, "rounds, not truncates");
        assert_eq!(to_milli(-0.0004), 0);
        assert_eq!(to_milli(f64::from(i32::MAX)), i32::MAX, "saturates high");
        assert_eq!(
            to_milli(f64::from(i32::MIN) - 5.0),
            i32::MIN,
            "saturates low"
        );
    }

    #[test]
    fn a_meter_and_its_belief_enter_the_trace_as_caused_tropes() {
        // The seam in one piece: Billy's interest in the note is an Affective
        // trope (salience); the belief formed over it is an Epistemic trope
        // whose event is caused by the meter's. Appraisal precedes knowledge,
        // structurally, and the trace validates.
        let mut meter = 0.0_f64;
        for _ in 0..200 {
            meter = NOTE.observed(meter, NOTE_URG[2], true, TICK);
        }
        let belief = belief_over(&[("note", meter)], THRESHOLD);
        assert_eq!(
            belief,
            Some("note"),
            "200 sprinting-carried ticks must convince"
        );

        let trace = Trace {
            tropes: vec![
                Trope {
                    id: TropeId(1),
                    holders: Vec::new(),
                    bearer: "billy".into(),
                    about: Some("note".into()),
                    kind: "interest".into(),
                    domain: Domain::Affective,
                },
                Trope {
                    id: TropeId(2),
                    holders: Vec::new(),
                    bearer: "billy".into(),
                    about: Some("note".into()),
                    kind: "object_theory".into(),
                    domain: Domain::Epistemic,
                },
            ],
            events: vec![
                TraceEvent {
                    id: 1,
                    tick: 200,
                    trope: TropeId(1),
                    value_milli: to_milli(meter),
                    confidence: None,
                    revision: crate::Revision::Assert,
                    caused_by: vec![],
                },
                TraceEvent {
                    id: 2,
                    tick: 200,
                    trope: TropeId(2),
                    value_milli: 1000, // the belief holds: 1.000
                    confidence: None,
                    revision: crate::Revision::Assert,
                    caused_by: vec![1],
                },
            ],
        };
        assert_eq!(trace.validate(), Ok(()));
    }
}
