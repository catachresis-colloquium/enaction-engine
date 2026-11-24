// SPDX-License-Identifier: MPL-2.0
//! Blending and buffer semantics.
//!
//! These guard properties that are invisible in a screenshot and obvious in
//! motion: an inexact endpoint pops once per step, a shimmering stationary
//! value never settles, a mishandled discontinuity slides across the jump.

use enaction_time::{Blend, DoubleBuffer, lerp};

// ── endpoint exactness ──────────────────────────────────────────────────────

#[test]
fn endpoints_are_bit_exact() {
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    buf.commit(&[-5.5]);
    buf.commit(&[3.3]);

    assert_eq!(buf.sample(0, 0.0), -5.5);
    assert_eq!(buf.sample(0, 1.0), 3.3);
}

#[test]
fn computing_the_endpoint_would_be_inexact() {
    // Why `lerp` clamps rather than computing. Real f64 results, measured
    // before being written down — if the clamp is removed, this is what ships.
    for (prev, curr) in [(-5.5_f64, 3.3_f64), (1e16, 1.0)] {
        let computed = prev + (curr - prev) * 1.0;
        assert_ne!(computed, curr, "{prev} → {curr} should be inexact computed");
        assert_eq!(lerp(prev, curr, 1.0), curr, "but lerp must be exact");
    }
}

#[test]
fn a_stationary_value_does_not_shimmer() {
    // Why the interior is `prev + (curr - prev) * alpha` and not the symmetric
    // `prev * (1 - alpha) + curr * alpha`, which returns 42.00000000000001.
    for step in 1..10 {
        let alpha = f64::from(step) / 10.0;
        assert_eq!(lerp(42.0, 42.0, alpha), 42.0, "shimmer at alpha {alpha}");
    }
}

#[test]
fn alpha_is_clamped_and_nan_safe() {
    assert_eq!(lerp(1.0, 2.0, -0.5), 1.0, "negative clamps to prev");
    assert_eq!(lerp(1.0, 2.0, 1.5), 2.0, "above one clamps to curr");
    assert_eq!(lerp(1.0, 2.0, f64::NAN), 2.0, "NaN resolves to curr");
    assert_eq!(lerp(1.0, 2.0, f64::INFINITY), 2.0);
    assert_eq!(lerp(1.0, 2.0, f64::NEG_INFINITY), 1.0);
}

#[test]
fn interpolation_is_monotonic_across_the_interval() {
    let mut last = lerp(0.0, 100.0, 0.0);
    for step in 1..=32 {
        let x = lerp(0.0, 100.0, f64::from(step) / 32.0);
        assert!(x >= last, "went backwards at step {step}: {last} → {x}");
        last = x;
    }
}

#[test]
fn f32_blends_through_f64_without_leaving_the_interval() {
    let x = f32::blend(0.1, 0.3, 0.5);
    assert!((0.1..=0.3).contains(&x), "{x} escaped [0.1, 0.3]");
    assert_eq!(f32::blend(0.1, 0.3, 0.0), 0.1);
    assert_eq!(f32::blend(0.1, 0.3, 1.0), 0.3);
}

// ── buffer semantics ────────────────────────────────────────────────────────

#[test]
fn commit_shifts_curr_into_prev() {
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    buf.commit(&[1.0]);
    buf.commit(&[2.0]);
    assert_eq!(buf.prev(0), 1.0);
    assert_eq!(buf.curr(0), 2.0);
}

#[test]
fn prime_seeds_both_steps() {
    let mut buf: DoubleBuffer<f64, 2> = DoubleBuffer::new();
    buf.prime(&[100.0, 50.0]);
    assert_eq!(buf.prev(0), 100.0);
    assert_eq!(buf.sample(0, 0.0), 100.0, "no slide from the origin");
    assert_eq!(buf.sample(1, 0.5), 50.0);
}

#[test]
fn a_stationary_slot_renders_stationary() {
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    buf.commit(&[42.0]);
    buf.commit(&[42.0]);
    for step in 0..=10 {
        let alpha = f64::from(step) / 10.0;
        assert_eq!(buf.sample(0, alpha), 42.0, "drift at alpha {alpha}");
    }
}

#[test]
fn slots_are_independent() {
    let mut buf: DoubleBuffer<f64, 3> = DoubleBuffer::new();
    buf.commit(&[0.0, 0.0, 0.0]);
    buf.commit(&[10.0, 0.0, -10.0]);

    assert_eq!(buf.sample(0, 0.5), 5.0);
    assert_eq!(buf.sample(1, 0.5), 0.0);
    assert_eq!(buf.sample(2, 0.5), -5.0);
}

// ── the discontinuity rule ──────────────────────────────────────────────────

#[test]
fn commit_then_snap_leaves_nothing_to_interpolate() {
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    buf.commit(&[900.0]);
    buf.commit(&[10.0]);
    buf.snap();

    for step in 0..=10 {
        let alpha = f64::from(step) / 10.0;
        assert_eq!(buf.sample(0, alpha), 10.0, "slide at alpha {alpha}");
    }
}

#[test]
fn snap_alone_would_lose_the_fresh_state() {
    // The mistake the doc comment warns about, pinned so a "simplification"
    // from `commit(); snap()` to bare `snap()` fails here rather than shipping
    // a visible jump.
    let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    buf.commit(&[900.0]);
    buf.snap();
    assert_eq!(
        buf.sample(0, 1.0),
        900.0,
        "snap alone keeps the pre-discontinuity value — hence commit-then-snap"
    );
}

// ── allocation ──────────────────────────────────────────────────────────────

#[test]
fn the_buffer_is_inline_fixed_size_storage() {
    use std::mem::size_of;
    assert_eq!(size_of::<DoubleBuffer<f64, 8>>(), 2 * 8 * size_of::<f64>());
    assert_eq!(size_of::<DoubleBuffer<f32, 3>>(), 2 * 3 * size_of::<f32>());
}

#[test]
fn len_reports_the_slot_count() {
    let buf: DoubleBuffer<f64, 4> = DoubleBuffer::new();
    assert_eq!(buf.len(), 4);
    assert!(!buf.is_empty());

    let empty: DoubleBuffer<f64, 0> = DoubleBuffer::new();
    assert!(empty.is_empty());
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn sampling_past_the_end_panics() {
    let buf: DoubleBuffer<f64, 2> = DoubleBuffer::new();
    let _ = buf.sample(2, 0.5);
}
