// SPDX-License-Identifier: AGPL-3.0-or-later
//! Observer noninterference: GAME-DESIGN-SYSTEM invariant 2.
//!
//! > For two traces that agree on every event in perspective P's observation
//! > set and causal closure, every belief P reads is byte-identical.
//!
//! P's observation set, under the frozen-prior arm (ADR-0022, Proposed), is:
//!
//! * the source event of each registered subject's [`Prior`] — registration is
//!   the game declaring common knowledge, so every perspective has seen it;
//! * every world event P's own store mutations cite in `caused_by`;
//!
//! closed under `caused_by`. Nothing else in the trace — later world events,
//! other perspectives' beliefs, another agent's *model* of P — may reach P.
//!
//! Both tests also check that the perturbation is real: the world itself (the
//! empty perspective, which reads live ground truth) must see the difference,
//! so neither test can pass by perturbing nothing.
//!
//! These tests are the targets of the planted mutant
//! `tests/mutants/omniscience.patch` (run by `scripts/check-observer-mutant.sh`),
//! which restores the old live-truth fall-through and must make both fail.
//!
//! [`Prior`]: enaction_trace::Prior

use std::collections::BTreeSet;

use enaction_trace::{
    BeliefStore, BeliefView, Domain, Mass, Perspective, Revision, StoreError, Subject, Trace,
    TraceEvent, Trope, TropeId,
};
use proptest::prelude::*;
use proptest::sample::Index;

const BASKET: i32 = 1_000;

fn subject(k: usize) -> Subject {
    Subject {
        kind: "marble_location".into(),
        about: Some(format!("marble-{k}")),
    }
}

fn perspective(path: &[&str]) -> Perspective {
    Perspective(path.iter().map(|s| (*s).to_string()).collect())
}

fn world_event(id: u64, tick: u64, trope: u64, value_milli: i32, cause: Option<u64>) -> TraceEvent {
    TraceEvent {
        id,
        tick,
        trope: TropeId(trope),
        value_milli,
        confidence: None,
        revision: Revision::Assert,
        caused_by: cause.into_iter().collect(),
    }
}

/// The probe from ULTRACODE-COMPLETION-PLAN's appendix ("Observer leak"):
/// two runs that differ only in an event Sally never observed.
///
/// Before the fix this read `Some(1000)` and `Some(2000)`: Sally, with no
/// observations at all, read the world's hidden truth.
///
/// One deliberate difference from the appendix: the appendix registers the
/// baseline *after* pushing Anne's move. Under the frozen-prior arm,
/// registration is the moment the game declares common knowledge, so a world
/// event already in the trace at registration is part of every perspective's
/// prior by definition. The story says Sally saw the marble placed and not
/// moved, so the game registers between the two events.
#[test]
fn observer_sally_anne_two_run_probe() {
    fn run(hidden_value: i32) -> (Option<BeliefView>, Option<BeliefView>) {
        let marble = subject(0);
        let mut trace = Trace {
            tropes: vec![Trope {
                id: TropeId(1),
                holders: vec![],
                bearer: "world".into(),
                about: marble.about.clone(),
                kind: marble.kind.clone(),
                domain: Domain::Mechanical,
            }],
            events: vec![world_event(1, 1, 1, BASKET, None)],
        };
        let mut store = BeliefStore::new();
        // Sally watches the marble go into the basket.
        store
            .register_baseline(&trace, marble.clone(), TropeId(1))
            .unwrap();
        // Anne moves the marble while Sally is out: an event Sally never
        // observes, and no store mutation of Sally's cites.
        trace.events.push(world_event(2, 2, 1, hidden_value, None));
        (
            store.belief(&trace, &perspective(&["sally"]), &marble),
            store.belief(&trace, &perspective(&[]), &marble),
        )
    }

    let (sally_a, world_a) = run(1_000);
    let (sally_b, world_b) = run(2_000);

    assert_ne!(
        world_a, world_b,
        "non-vacuity: the hidden event must change the world's own truth"
    );
    assert_eq!(
        sally_a, sally_b,
        "Sally never observed Anne's move, so it cannot change her belief"
    );
    assert_eq!(
        sally_a,
        Some(BeliefView {
            value_milli: BASKET,
            confidence: None,
            source: None,
        }),
        "with no observations of her own, Sally reads the frozen prior"
    );
}

// ---------------------------------------------------------------------------
// The property.

/// Registered subjects. Subject 0 is the witness: P never acts on it, and the
/// script ends with a hidden world event on it, so every case contains at
/// least one unobserved change to a subject P reads through its prior.
const SUBJECTS: usize = 4;

/// P is index 0. The others include a model *of* Sally (`observer → sally`)
/// and Sally's model of Anne: neither is Sally's own perspective.
const PERSPECTIVES: [&[&str]; 4] = [
    &["sally"],
    &["anne"],
    &["observer", "sally"],
    &["sally", "anne"],
];

/// One step of a generated script. `Index` values pick a world event that
/// exists when the step runs; `shift`s are the run-B perturbation, applied
/// only where the step is outside P's closure.
#[derive(Debug, Clone)]
enum Op {
    /// A host-appended world event, optionally caused by an earlier one.
    World {
        subject: usize,
        tick: u64,
        value: i32,
        cause: Option<Index>,
        shift: i32,
        tick_shift: u64,
    },
    /// A perspective observes a world event and asserts its value.
    Observe {
        who: usize,
        subject: usize,
        of: Index,
        tick: u64,
        confidence: u16,
        shift: i32,
    },
    /// A perspective withdraws a belief, citing a world event.
    Retract {
        who: usize,
        subject: usize,
        of: Index,
        tick: u64,
    },
}

/// An `Op` with every index resolved against the world events that exist
/// when it runs (world events are numbered 0.. in creation order; the first
/// `SUBJECTS` are the setup events the priors are frozen from).
#[derive(Debug, Clone)]
enum Step {
    World {
        w: usize,
        subject: usize,
        tick: u64,
        value: i32,
        cause: Option<usize>,
        shift: i32,
        tick_shift: u64,
    },
    Observe {
        who: usize,
        subject: usize,
        of: usize,
        tick: u64,
        confidence: u16,
        shift: i32,
    },
    Retract {
        who: usize,
        subject: usize,
        of: usize,
        tick: u64,
    },
}

/// P never acts on the witness subject.
fn target(who: usize, subject: usize) -> usize {
    if who == 0 && subject == 0 { 1 } else { subject }
}

fn resolve(script: &[Op]) -> Vec<Step> {
    let mut worlds = SUBJECTS;
    script
        .iter()
        .map(|op| match op.clone() {
            Op::World {
                subject,
                tick,
                value,
                cause,
                shift,
                tick_shift,
            } => {
                let step = Step::World {
                    w: worlds,
                    subject,
                    tick,
                    value,
                    cause: cause.map(|c| c.index(worlds)),
                    shift,
                    tick_shift,
                };
                worlds += 1;
                step
            }
            Op::Observe {
                who,
                subject,
                of,
                tick,
                confidence,
                shift,
            } => Step::Observe {
                who,
                subject: target(who, subject),
                of: of.index(worlds),
                tick,
                confidence,
                shift,
            },
            Op::Retract {
                who,
                subject,
                of,
                tick,
            } => Step::Retract {
                who,
                subject: target(who, subject),
                of: of.index(worlds),
                tick,
            },
        })
        .collect()
}

/// P's observation set and causal closure, as world-event numbers.
fn closure_of_p(steps: &[Step]) -> BTreeSet<usize> {
    let mut causes: Vec<Option<usize>> = vec![None; SUBJECTS];
    let mut observed: Vec<usize> = (0..SUBJECTS).collect(); // the priors' sources
    for step in steps {
        match *step {
            Step::World { cause, .. } => causes.push(cause),
            Step::Observe { who: 0, of, .. } | Step::Retract { who: 0, of, .. } => {
                observed.push(of);
            }
            Step::Observe { .. } | Step::Retract { .. } => {}
        }
    }
    let mut closed = BTreeSet::new();
    while let Some(w) = observed.pop() {
        if closed.insert(w)
            && let Some(c) = causes[w]
        {
            observed.push(c);
        }
    }
    closed
}

/// Everything P can read, plus the world's view of the witness.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    p_mutations: Vec<Result<u64, StoreError>>,
    p_beliefs: Vec<Option<BeliefView>>,
    world_witness: Option<BeliefView>,
}

const WITNESS_TICK: u64 = 10_000;

fn run(
    steps: &[Step],
    closure: &BTreeSet<usize>,
    setup: &[i32; SUBJECTS],
    witness: i32,
    perturb: bool,
) -> Outcome {
    let hidden = |w: usize| perturb && !closure.contains(&w);
    let mut trace = Trace {
        tropes: (0..SUBJECTS)
            .map(|k| Trope {
                id: TropeId(k as u64 + 1),
                holders: vec![],
                bearer: "world".into(),
                about: subject(k).about,
                kind: subject(k).kind,
                domain: Domain::Mechanical,
            })
            .collect(),
        events: (0..SUBJECTS)
            .map(|k| world_event(k as u64 + 1, 1, k as u64 + 1, setup[k], None))
            .collect(),
    };
    // World event number -> (trace event id, value in this run).
    let mut worlds: Vec<(u64, i32)> = (0..SUBJECTS).map(|k| (k as u64 + 1, setup[k])).collect();
    let next_id = |trace: &Trace| trace.events.iter().map(|e| e.id).max().unwrap_or(0) + 1;

    let mut store = BeliefStore::new();
    for k in 0..SUBJECTS {
        store
            .register_baseline(&trace, subject(k), TropeId(k as u64 + 1))
            .unwrap();
    }

    let mut p_mutations = Vec::new();
    for step in steps {
        match *step {
            Step::World {
                w,
                subject: k,
                tick,
                value,
                cause,
                shift,
                tick_shift,
            } => {
                let (value, tick) = if hidden(w) {
                    (value + shift, tick + tick_shift)
                } else {
                    (value, tick)
                };
                let id = next_id(&trace);
                let cause = cause.map(|c| worlds[c].0);
                trace
                    .events
                    .push(world_event(id, tick, k as u64 + 1, value, cause));
                worlds.push((id, value));
            }
            Step::Observe {
                who,
                subject: k,
                of,
                tick,
                confidence,
                shift,
            } => {
                let (cited, seen) = worlds[of];
                // Another perspective's belief is outside P's closure, so in
                // run B its content is perturbed too.
                let value = if perturb && who != 0 {
                    seen + shift
                } else {
                    seen
                };
                let result = store.assert_belief(
                    &mut trace,
                    &perspective(PERSPECTIVES[who]),
                    &subject(k),
                    tick,
                    value,
                    Mass::new(confidence).unwrap(),
                    vec![cited],
                );
                if who == 0 {
                    p_mutations.push(result);
                }
            }
            Step::Retract {
                who,
                subject: k,
                of,
                tick,
            } => {
                let result = store.retract(
                    &mut trace,
                    &perspective(PERSPECTIVES[who]),
                    &subject(k),
                    tick,
                    vec![worlds[of].0],
                );
                if who == 0 {
                    p_mutations.push(result);
                }
            }
        }
    }

    // The witness: a hidden world event on subject 0, after everything else.
    let id = next_id(&trace);
    trace
        .events
        .push(world_event(id, WITNESS_TICK, 1, witness, None));

    let p = perspective(PERSPECTIVES[0]);
    let unregistered = Subject {
        kind: "marble_location".into(),
        about: Some("never-registered".into()),
    };
    let mut p_beliefs: Vec<Option<BeliefView>> = (0..SUBJECTS)
        .map(|k| store.belief(&trace, &p, &subject(k)))
        .collect();
    p_beliefs.push(store.belief(&trace, &p, &unregistered));
    Outcome {
        p_mutations,
        p_beliefs,
        world_witness: store.belief(&trace, &perspective(&[]), &subject(0)),
    }
}

fn op() -> impl Strategy<Value = Op> {
    let tick = 1u64..1_000;
    let value = -1_000_000i32..1_000_000;
    let shift = -1_000i32..1_000;
    prop_oneof![
        (
            0..SUBJECTS,
            tick.clone(),
            value,
            proptest::option::of(any::<Index>()),
            shift.clone(),
            0u64..256,
        )
            .prop_map(
                |(subject, tick, value, cause, shift, tick_shift)| Op::World {
                    subject,
                    tick,
                    value,
                    cause,
                    shift,
                    tick_shift,
                }
            ),
        (
            0..PERSPECTIVES.len(),
            0..SUBJECTS,
            any::<Index>(),
            tick.clone(),
            0u16..=Mass::SCALE,
            shift,
        )
            .prop_map(|(who, subject, of, tick, confidence, shift)| Op::Observe {
                who,
                subject,
                of,
                tick,
                confidence,
                shift,
            }),
        (0..PERSPECTIVES.len(), 0..SUBJECTS, any::<Index>(), tick).prop_map(
            |(who, subject, of, tick)| Op::Retract {
                who,
                subject,
                of,
                tick,
            }
        ),
    ]
}

proptest! {
    // At least 1024 cases; PROPTEST_CASES may raise it, never lower it.
    // No regression file: a failure prints its shrunk input, and a file
    // written beside this test would land unreviewed in the crate (and in
    // the mutant script's copy).
    #![proptest_config(ProptestConfig {
        cases: ProptestConfig::default().cases.max(1024),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn observer_proptest_perturbs_outside_closure(
        script in proptest::collection::vec(op(), 0..48),
        setup in proptest::array::uniform4(-1_000_000i32..1_000_000),
        witness in -1_000_000i32..1_000_000,
        witness_shift in prop_oneof![-1_000i32..0, 1i32..1_000],
    ) {
        let steps = resolve(&script);
        let closure = closure_of_p(&steps);

        let a = run(&steps, &closure, &setup, witness, false);
        let b = run(&steps, &closure, &setup, witness + witness_shift, true);

        prop_assert_ne!(
            a.world_witness, b.world_witness,
            "non-vacuity: the hidden witness must change the world's own truth"
        );
        prop_assert_eq!(&a.p_mutations, &b.p_mutations);
        prop_assert_eq!(&a.p_beliefs, &b.p_beliefs);
        // P never touches the witness subject, so it reads that prior.
        prop_assert_eq!(
            a.p_beliefs[0],
            Some(BeliefView { value_milli: setup[0], confidence: None, source: None })
        );
    }
}
