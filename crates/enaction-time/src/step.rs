// SPDX-License-Identifier: MPL-2.0
//! The fixed-timestep accumulator.

/// How many whole simulation steps to run, and how far through the next one the
/// current frame falls.
///
/// Feed it real elapsed time; it tells you how many equal-sized steps are owed.
/// The simulation never sees a partial step, so a run stays a pure function of
/// its inputs and remains replayable, snapshottable and lockstep-safe.
///
/// ```
/// use enaction_time::FixedStep;
///
/// let mut clock = FixedStep::from_hz(60.0);
///
/// // 50 ms is exactly three 16.67 ms steps, so nothing is left over.
/// assert_eq!(clock.advance(0.050), 3);
/// assert!(clock.alpha() < 1e-9);
///
/// // 55 ms is three steps and about a third of a fourth.
/// assert_eq!(clock.advance(0.055), 3);
/// assert!((clock.alpha() - 0.3).abs() < 1e-9);
/// ```
///
/// # The spiral of death
///
/// If a frame stalls — a debugger breakpoint, a backgrounded window, a slow
/// disk — the accumulator fills with more time than the simulation can work off
/// before the next frame. Running every owed step then takes *longer* than real
/// time, so the next frame is later still, and the gap grows without bound
/// until the program stops responding.
///
/// [`advance`](Self::advance) caps the steps it will return in one call and
/// **discards** the excess. The simulation therefore runs slower than wall-clock
/// during a stall rather than trying to catch up forever. Dropping time is the
/// lesser evil: the alternative is a freeze. Tune with
/// [`with_max_steps`](Self::with_max_steps) and detect it with
/// [`took_shortcut`](Self::took_shortcut).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedStep {
    dt: f64,
    accumulator: f64,
    max_steps: u32,
    took_shortcut: bool,
}

/// Steps allowed per `advance` before the excess is discarded.
///
/// At 60 Hz this lets a frame absorb a ~133 ms stall in full before time starts
/// being dropped — comfortably more than any healthy frame, comfortably less
/// than a stall the simulation could never work off.
pub const DEFAULT_MAX_STEPS: u32 = 8;

impl FixedStep {
    /// A clock running at `hz` steps per second.
    ///
    /// # Panics
    ///
    /// If `hz` is not finite and greater than zero.
    #[must_use]
    pub fn from_hz(hz: f64) -> Self {
        assert!(
            hz.is_finite() && hz > 0.0,
            "step rate must be finite and positive, got {hz}"
        );
        Self::from_seconds(1.0 / hz)
    }

    /// A clock whose step is `dt` seconds.
    ///
    /// # Panics
    ///
    /// If `dt` is not finite and greater than zero. A zero step would make
    /// every frame owe infinitely many steps.
    #[must_use]
    pub fn from_seconds(dt: f64) -> Self {
        assert!(
            dt.is_finite() && dt > 0.0,
            "step length must be finite and positive, got {dt}"
        );
        Self {
            dt,
            accumulator: 0.0,
            max_steps: DEFAULT_MAX_STEPS,
            took_shortcut: false,
        }
    }

    /// Override how many steps one `advance` may return.
    ///
    /// # Panics
    ///
    /// If `max_steps` is zero — the simulation would never advance.
    #[must_use]
    pub fn with_max_steps(mut self, max_steps: u32) -> Self {
        assert!(max_steps > 0, "max_steps must be at least 1");
        self.max_steps = max_steps;
        self
    }

    /// Add `real_dt` seconds of elapsed time and return the number of whole
    /// steps now owed.
    ///
    /// Run the simulation exactly that many times, then draw using
    /// [`alpha`](Self::alpha).
    ///
    /// Non-finite or negative input is ignored rather than trusted: a `NaN`
    /// would poison the accumulator permanently, and time does not run
    /// backwards. Both leave the clock untouched and return `0`.
    pub fn advance(&mut self, real_dt: f64) -> u32 {
        self.took_shortcut = false;
        if !real_dt.is_finite() || real_dt <= 0.0 {
            return 0;
        }
        self.accumulator += real_dt;

        let owed = (self.accumulator / self.dt).floor();
        // `owed` is finite and non-negative here, and the cap is applied before
        // the cast, so this cannot wrap or saturate surprisingly.
        let owed = if owed >= f64::from(self.max_steps) {
            self.took_shortcut = true;
            self.max_steps
        } else {
            owed as u32
        };

        if self.took_shortcut {
            // Discard the backlog: keep only a sub-step remainder so `alpha`
            // stays meaningful and the next frame starts clean.
            self.accumulator %= self.dt;
        } else {
            self.accumulator -= f64::from(owed) * self.dt;
        }
        owed
    }

    /// How far through the next step the current frame falls, in `[0, 1)`.
    ///
    /// Pass this to `DoubleBuffer::sample`.
    #[must_use]
    pub fn alpha(&self) -> f64 {
        let a = self.accumulator / self.dt;
        a.clamp(0.0, 1.0)
    }

    /// Whether the last [`advance`](Self::advance) hit the cap and dropped time.
    ///
    /// Worth surfacing: a clock that shortcuts every frame means the simulation
    /// cannot keep up, and silently running slow is exactly the kind of thing
    /// that goes unnoticed until it is expensive.
    #[must_use]
    pub const fn took_shortcut(&self) -> bool {
        self.took_shortcut
    }

    /// The step length in seconds.
    #[must_use]
    pub const fn dt(&self) -> f64 {
        self.dt
    }

    /// The step rate in steps per second.
    #[must_use]
    pub fn hz(&self) -> f64 {
        1.0 / self.dt
    }

    /// Discard any partial step.
    ///
    /// Use after a deliberate discontinuity — a level load, a resync — so the
    /// first frame of the new state is not drawn part-way through an interval
    /// that belonged to the old one.
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.took_shortcut = false;
    }
}
