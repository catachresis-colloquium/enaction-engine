// SPDX-License-Identifier: MPL-2.0
//! Double-buffered state and the blending applied to it.

/// A value that can be blended across a step boundary.
///
/// Implement this for whatever a host draws — a position, a rotation, a colour.
/// Implementations **must be exact at the endpoints**: `alpha <= 0.0` returns
/// `prev` bit-for-bit and `alpha >= 1.0` returns `curr` bit-for-bit. An
/// approximate endpoint pops once per step, which is the artefact interpolation
/// exists to remove.
///
/// Blend only *continuous* quantities. Booleans, enums and other discrete state
/// should be read live from the current step — blending them is meaningless at
/// best and wrong at worst. Sign-like values (a facing direction) are discrete
/// in this sense even though they are stored as numbers: lerping a facing
/// through zero draws the subject facing neither way.
pub trait Blend: Copy {
    /// Blend `prev` toward `curr` by `alpha`.
    fn blend(prev: Self, curr: Self, alpha: f64) -> Self;
}

/// Endpoint-exact, clamped linear interpolation.
///
/// Both halves of this were established by measurement, and both matter:
///
/// * **The endpoints are clamped, not computed.** `prev + (curr - prev) * 1.0`
///   is not exact in general — `(-5.5, 3.3)` yields `3.3000000000000007` and
///   `(1e16, 1.0)` yields `0.0`.
/// * **The interior uses the one-term form.** The symmetric-looking
///   `prev * (1 - alpha) + curr * alpha` is worse: with `prev == curr == 42.0`
///   and `alpha = 0.1` it returns `42.00000000000001`, so a *stationary* value
///   shimmers in the last ulp every frame. The one-term form collapses to
///   `prev + 0.0 * alpha` and is exactly stationary.
///
/// A `NaN` alpha resolves to `curr`: it means something upstream is broken, and
/// the current step is the safe thing to draw. Propagating `NaN` into a
/// transform typically makes the subject vanish instead.
#[inline]
#[must_use]
pub fn lerp(prev: f64, curr: f64, alpha: f64) -> f64 {
    if alpha.is_nan() || alpha >= 1.0 {
        curr
    } else if alpha <= 0.0 {
        prev
    } else {
        prev + (curr - prev) * alpha
    }
}

impl Blend for f64 {
    #[inline]
    fn blend(prev: Self, curr: Self, alpha: f64) -> Self {
        lerp(prev, curr, alpha)
    }
}

impl Blend for f32 {
    #[inline]
    fn blend(prev: Self, curr: Self, alpha: f64) -> Self {
        lerp(f64::from(prev), f64::from(curr), alpha) as Self
    }
}

/// Two steps of state, in fixed-size inline storage.
///
/// `prev` is step *N*, `curr` is step *N+1*, and [`sample`](Self::sample) draws
/// between them. `N` is the number of independently-tracked slots; the host
/// decides what a slot means and holds its own indices.
///
/// ```
/// use enaction_time::DoubleBuffer;
///
/// let mut buf: DoubleBuffer<f64, 2> = DoubleBuffer::new();
/// buf.prime(&[0.0, 0.0]);
/// buf.commit(&[10.0, -4.0]);
///
/// assert_eq!(buf.sample(0, 0.0), 0.0);   // exactly the previous step
/// assert_eq!(buf.sample(0, 0.5), 5.0);
/// assert_eq!(buf.sample(0, 1.0), 10.0);  // exactly the current step
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoubleBuffer<T: Blend, const N: usize> {
    prev: [T; N],
    curr: [T; N],
}

impl<T: Blend + Default, const N: usize> Default for DoubleBuffer<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Blend + Default, const N: usize> DoubleBuffer<T, N> {
    /// A buffer with both steps at `T::default()`.
    ///
    /// Follow with [`prime`](Self::prime) as soon as the real starting state is
    /// known, or the first frame draws a slide away from `default()`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prev: [T::default(); N],
            curr: [T::default(); N],
        }
    }

    /// Seed both steps with the same state, so the first rendered interval is
    /// stationary.
    pub fn prime(&mut self, state: &[T; N]) {
        self.curr = *state;
        self.prev = *state;
    }
}

impl<T: Blend, const N: usize> DoubleBuffer<T, N> {
    /// Advance one step: `prev` takes the old `curr`, `curr` takes `fresh`.
    pub fn commit(&mut self, fresh: &[T; N]) {
        self.prev = self.curr;
        self.curr = *fresh;
    }

    /// Collapse history so there is nothing left to interpolate across.
    ///
    /// Call this **after** [`commit`](Self::commit), never instead of it.
    ///
    /// On a discontinuity — a restart, a teleport, a level load, a network
    /// resync — the fresh state still has to enter the buffer; `snap` then
    /// discards the stale `prev` that would otherwise be drawn as a slide
    /// across the jump. Using `snap` alone leaves the new state out of the
    /// buffer entirely and keeps drawing the *old* position for a further
    /// interval before jumping. That mistake is easy to make and invisible
    /// except in motion, so it is worth a test of its own.
    ///
    /// ```
    /// use enaction_time::DoubleBuffer;
    ///
    /// let mut buf: DoubleBuffer<f64, 1> = DoubleBuffer::new();
    /// buf.commit(&[900.0]);        // far from spawn
    /// buf.commit(&[10.0]);         // the restart's fresh state
    /// buf.snap();                  // ...and only then collapse
    /// assert_eq!(buf.sample(0, 0.5), 10.0);
    /// ```
    pub fn snap(&mut self) {
        self.prev = self.curr;
    }

    /// The value to draw for `slot` at `alpha` through the current interval.
    ///
    /// `alpha` is clamped: `0.0` is exactly `prev`, `1.0` is exactly `curr`.
    ///
    /// # Panics
    ///
    /// If `slot >= N`.
    #[must_use]
    pub fn sample(&self, slot: usize, alpha: f64) -> T {
        T::blend(self.prev[slot], self.curr[slot], alpha)
    }

    /// The previous step's raw value, unblended.
    ///
    /// # Panics
    ///
    /// If `slot >= N`.
    #[must_use]
    pub fn prev(&self, slot: usize) -> T {
        self.prev[slot]
    }

    /// The current step's raw value, unblended. Discrete reads should use this.
    ///
    /// # Panics
    ///
    /// If `slot >= N`.
    #[must_use]
    pub fn curr(&self, slot: usize) -> T {
        self.curr[slot]
    }

    /// The number of slots this buffer tracks.
    #[must_use]
    pub const fn len(&self) -> usize {
        N
    }

    /// Whether this buffer tracks no slots at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        N == 0
    }
}
