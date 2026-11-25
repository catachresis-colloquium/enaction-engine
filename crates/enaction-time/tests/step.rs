// SPDX-License-Identifier: MPL-2.0
//! The fixed-timestep accumulator.
//!
//! This is the half of the crate with no prior service in a real game, so it is
//! tested harder than the interpolation it feeds: the spiral-of-death guard,
//! hostile inputs, and the invariant that no simulation time is invented.

use enaction_time::{DoubleBuffer, FixedStep};

const HZ: f64 = 60.0;
const DT: f64 = 1.0 / 60.0;

#[test]
fn a_short_frame_owes_no_steps_but_accumulates() {
    let mut clock = FixedStep::from_hz(HZ);
    assert_eq!(clock.advance(DT / 2.0), 0, "half a step is not a step");
    assert!(
        (clock.alpha() - 0.5).abs() < 1e-9,
        "but it must show as half-way: {}",
        clock.alpha()
    );
}

#[test]
fn an_exact_frame_owes_exactly_one_step() {
    let mut clock = FixedStep::from_hz(HZ);
    assert_eq!(clock.advance(DT), 1);
    assert!(
        clock.alpha() < 1e-9,
        "nothing left over, got {}",
        clock.alpha()
    );
}

#[test]
fn a_long_frame_owes_several_steps_and_keeps_the_remainder() {
    let mut clock = FixedStep::from_hz(HZ);
    // 55 ms at 60 Hz = 3.3 steps: 3 whole ones and 0.3 of the next.
    assert_eq!(clock.advance(0.055), 3);
    assert!(
        (clock.alpha() - 0.3).abs() < 1e-9,
        "expected ~0.3 left over, got {}",
        clock.alpha()
    );
}

#[test]
fn an_exactly_divisible_frame_leaves_no_remainder() {
    let mut clock = FixedStep::from_hz(HZ);
    // 50 ms is exactly 3 steps at 60 Hz — a boundary worth pinning, since it
    // is the case where alpha must be 0 rather than "very nearly 1".
    assert_eq!(clock.advance(0.050), 3);
    assert!(clock.alpha() < 1e-9, "got {}", clock.alpha());
}

#[test]
fn no_simulation_time_is_invented_or_lost() {
    // The core accounting invariant: over a long, irregular run the steps
    // actually taken plus the leftover must equal the real time fed in.
    let mut clock = FixedStep::from_hz(HZ);
    let frames = [
        0.016, 0.017, 0.0083, 0.0331, 0.011, 0.0166, 0.0009, 0.021, 0.014,
    ];

    let mut fed = 0.0;
    let mut steps: u64 = 0;
    for _ in 0..50 {
        for f in frames {
            fed += f;
            steps += u64::from(clock.advance(f));
            assert!(!clock.took_shortcut(), "no frame here should hit the cap");
        }
    }

    let consumed = steps as f64 * DT + clock.alpha() * DT;
    assert!(
        (consumed - fed).abs() < 1e-9,
        "accounting drifted: fed {fed}, consumed {consumed}"
    );
}

#[test]
fn alpha_stays_in_range_across_a_long_irregular_run() {
    let mut clock = FixedStep::from_hz(HZ);
    let mut t = 0.001_f64;
    for _ in 0..2000 {
        // A cheap deterministic jitter — no RNG dependency.
        t = (t * 7.0).rem_euclid(0.033) + 0.0005;
        clock.advance(t);
        let a = clock.alpha();
        assert!((0.0..=1.0).contains(&a), "alpha escaped: {a}");
    }
}

// ── the spiral-of-death guard ───────────────────────────────────────────────

#[test]
fn a_huge_stall_is_capped_rather_than_replayed() {
    let mut clock = FixedStep::from_hz(HZ);
    // Ten minutes of stall: 36,000 steps owed if we tried to catch up.
    let steps = clock.advance(600.0);
    assert_eq!(steps, 8, "must cap at DEFAULT_MAX_STEPS, got {steps}");
    assert!(clock.took_shortcut(), "and must say that it dropped time");
    assert!(
        clock.alpha() < 1.0,
        "the backlog must be discarded, not carried: alpha {}",
        clock.alpha()
    );
}

#[test]
fn the_cap_is_configurable() {
    let mut clock = FixedStep::from_hz(HZ).with_max_steps(2);
    assert_eq!(clock.advance(1.0), 2);
    assert!(clock.took_shortcut());
}

#[test]
fn took_shortcut_resets_on_the_next_healthy_frame() {
    let mut clock = FixedStep::from_hz(HZ);
    clock.advance(600.0);
    assert!(clock.took_shortcut());
    clock.advance(DT);
    assert!(
        !clock.took_shortcut(),
        "the flag must describe the last call only"
    );
}

#[test]
fn a_stall_does_not_starve_later_frames() {
    // The point of discarding: after a stall the clock must behave normally
    // again immediately, rather than owing a backlog for minutes.
    let mut clock = FixedStep::from_hz(HZ);
    clock.advance(600.0);
    for _ in 0..10 {
        assert_eq!(clock.advance(DT), 1, "should be back to one step per frame");
    }
}

// ── hostile input ───────────────────────────────────────────────────────────

#[test]
fn non_finite_and_negative_time_is_ignored() {
    let mut clock = FixedStep::from_hz(HZ);
    clock.advance(DT / 2.0);
    let before = clock.alpha();

    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
        assert_eq!(clock.advance(bad), 0, "{bad} must owe no steps");
    }
    assert_eq!(
        clock.alpha(),
        before,
        "and must not disturb the accumulator"
    );
}

#[test]
fn a_nan_frame_does_not_poison_the_clock_permanently() {
    // A NaN added to the accumulator would make every later alpha NaN.
    let mut clock = FixedStep::from_hz(HZ);
    clock.advance(f64::NAN);
    assert_eq!(clock.advance(DT), 1);
    assert!(
        clock.alpha().is_finite(),
        "alpha went non-finite after a NaN frame"
    );
}

#[test]
fn reset_discards_the_partial_step() {
    let mut clock = FixedStep::from_hz(HZ);
    clock.advance(DT * 0.75);
    assert!(clock.alpha() > 0.5);
    clock.reset();
    assert_eq!(clock.alpha(), 0.0);
}

#[test]
#[should_panic(expected = "finite and positive")]
fn a_zero_rate_is_rejected() {
    let _ = FixedStep::from_hz(0.0);
}

#[test]
#[should_panic(expected = "finite and positive")]
fn a_nan_step_length_is_rejected() {
    let _ = FixedStep::from_seconds(f64::NAN);
}

#[test]
#[should_panic(expected = "at least 1")]
fn a_zero_step_cap_is_rejected() {
    let _ = FixedStep::from_hz(HZ).with_max_steps(0);
}

#[test]
fn rate_and_step_length_round_trip() {
    let clock = FixedStep::from_hz(HZ);
    assert!((clock.dt() - DT).abs() < 1e-12);
    assert!((clock.hz() - HZ).abs() < 1e-9);
}

// ── the two halves together ─────────────────────────────────────────────────

#[test]
fn driving_a_buffer_from_the_clock_never_leaves_the_interval() {
    // The integration the crate exists for: step the sim on whole steps, draw
    // between them, and never draw outside the two committed states.
    let mut clock = FixedStep::from_hz(HZ);
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    let mut x = 0.0_f64;
    buf.prime(&[x]);

    let mut frame = 0.004_f64;
    for _ in 0..500 {
        frame = (frame * 3.7).rem_euclid(0.040) + 0.001;
        for _ in 0..clock.advance(frame) {
            x += 1.0; // one unit of travel per simulation step
            buf.commit(&[x]);
        }
        let drawn = buf.sample(0, clock.alpha());
        let (lo, hi) = (buf.prev(0), buf.curr(0));
        assert!(
            (lo..=hi).contains(&drawn),
            "drew {drawn} outside [{lo}, {hi}]"
        );
    }
    assert!(x > 0.0, "the run must actually have stepped");
}
