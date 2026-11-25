// SPDX-License-Identifier: MPL-2.0
//! Fixed-timestep stepping and render interpolation.
//!
//! A deterministic simulation must advance in whole, equal steps: variable
//! `dt` makes a run depend on frame timing, and a run that depends on frame
//! timing cannot be replayed, snapshotted or lockstepped. But a display
//! refreshes at whatever rate it likes. This crate owns both halves of the gap
//! that opens between those two facts:
//!
//! * [`FixedStep`] — how many whole simulation steps to run this frame, and how
//!   far through the current one the frame falls.
//! * [`DoubleBuffer`] — the last two simulated states, sampled at that fraction
//!   so motion is smooth without the simulation ever seeing a partial step.
//!
//! # The invariant
//!
//! **Interpolation is render-only.** No value produced by [`DoubleBuffer`] may
//! flow back into simulation state. Everything here is a pure function of two
//! already-simulated states. Determinism depends on that separation, and it is
//! the first thing to check when integrating.
//!
//! # Allocation
//!
//! Nothing in this crate allocates. [`DoubleBuffer`] is two inline fixed-size
//! arrays and `commit` is a copy, so neither the per-frame nor the per-tick
//! path can stall on the allocator.
//!
//! # Provenance
//!
//! The interpolation half was built and proven in
//! [IDApTIK](https://github.com/metadatastician/IDApTIK) before being
//! generalised here; the float behaviour documented on [`lerp`] was measured
//! there, not reasoned about. [`FixedStep`] is the half IDApTIK never needed —
//! it borrows Bevy's accumulator — and is therefore the least battle-tested
//! code in the crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod blend;
mod step;

pub use blend::{Blend, DoubleBuffer, lerp};
pub use step::FixedStep;
