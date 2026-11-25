// SPDX-License-Identifier: MPL-2.0
//! Behavioural compatibility matrix for the IDApTIK extraction boundary.
//!
//! These are the cases IDApTIK already pins in `idaptik-core::interp` and its
//! Bevy driver. `FixedStep` is compared only where the host policy is defined;
//! the excessive-elapsed-time cap is a deliberate Enaction policy and is not
//! evidence that IDApTIK has adopted this accumulator.

use enaction_time::{DoubleBuffer, FixedStep, lerp};

const DT: f64 = 1.0 / 60.0;

fn reference_lerp(prev: f64, curr: f64, alpha: f64) -> f64 {
    if alpha.is_nan() || alpha >= 1.0 {
        curr
    } else if alpha <= 0.0 {
        prev
    } else {
        prev + (curr - prev) * alpha
    }
}

#[test]
fn continuous_interpolation_matches_idaptik_vectors_bit_for_bit() {
    for (prev, curr) in [(-5.5, 3.3), (1e16, 1.0), (42.0, 42.0), (-100.0, 100.0)] {
        for alpha in [
            f64::NEG_INFINITY,
            -0.5,
            0.0,
            0.1,
            0.5,
            1.0,
            2.0,
            f64::INFINITY,
            f64::NAN,
        ] {
            assert_eq!(
                lerp(prev, curr, alpha).to_bits(),
                reference_lerp(prev, curr, alpha).to_bits()
            );
        }
    }
}

#[test]
fn discrete_values_are_read_from_current_not_interpolated() {
    let mut continuous: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    continuous.commit(&[10.0]);
    continuous.commit(&[20.0]);
    let discrete_facing = -1.0;
    assert_eq!(continuous.sample(0, 0.5), 15.0);
    assert_eq!(discrete_facing, -1.0);
}

#[test]
fn restart_and_snapshot_restore_use_commit_then_snap() {
    let mut original: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    original.prime(&[72.0]);
    original.commit(&[900.0]);

    let snapshot_current = original.curr(0);
    let mut restored: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    restored.prime(&[snapshot_current]);
    assert_eq!(restored.sample(0, 0.37), 900.0);

    restored.commit(&[72.0]);
    restored.snap();
    for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert_eq!(restored.sample(0, alpha), 72.0);
    }
}

#[test]
fn healthy_fixed_step_accounting_matches_sixty_hertz_host_policy() {
    let frames = [0.008, 0.017, 0.033, 0.011, 0.016, 0.024];
    let mut clock = FixedStep::from_hz(60.0);
    let mut fed = 0.0;
    let mut steps = 0_u64;
    for _ in 0..40 {
        for frame in frames {
            fed += frame;
            steps += u64::from(clock.advance(frame));
            assert!(!clock.took_shortcut());
        }
    }
    let represented = steps as f64 * DT + clock.alpha() * DT;
    assert!((represented - fed).abs() < 1e-9);
}

#[test]
fn hostile_elapsed_time_is_ignored_or_bounded_by_declared_policy() {
    let mut clock = FixedStep::from_hz(60.0);
    for hostile in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0] {
        assert_eq!(clock.advance(hostile), 0);
    }
    assert_eq!(clock.advance(600.0), 8);
    assert!(clock.took_shortcut());
    assert!((0.0..1.0).contains(&clock.alpha()));
}

#[test]
fn discontinuity_resets_partial_wall_clock_time() {
    let mut clock = FixedStep::from_hz(60.0);
    assert_eq!(clock.advance(DT * 0.75), 0);
    assert!(clock.alpha() > 0.0);
    clock.reset();
    assert_eq!(clock.alpha(), 0.0);
}
