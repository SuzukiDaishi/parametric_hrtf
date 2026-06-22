//! Biquad filters and RBJ Audio EQ Cookbook style coefficient design.
//!
//! The pHRTF model used here is fundamentally a **parametric equalizer**:
//! peaks and notches are represented as second-order filters, then cascaded.
//!
//! This file is deliberately dependency-free. In production code you may want to
//! replace this with your engine's optimized SIMD biquad implementation, but the
//! coefficient equations should remain portable.

use crate::math::{amp_to_db, clamp};

/// Normalized biquad coefficients for:
///
/// ```text
/// y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2]
///      - a1*y[n-1] - a2*y[n-2]
/// ```
///
/// `a0` is already normalized to 1.
#[derive(Clone, Copy, Debug)]
pub struct BiquadCoeffs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoeffs {
    pub const BYPASS: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    #[inline]
    fn normalized(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        let inv = 1.0 / a0;
        Self {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
        }
    }

    /// Magnitude response in dB at `freq_hz`.
    /// This is useful for offline inspection and for docs/examples.
    pub fn magnitude_db(&self, sample_rate_hz: f32, freq_hz: f32) -> f32 {
        let f = clamp(freq_hz, 0.0, sample_rate_hz * 0.5);
        let w = 2.0 * std::f32::consts::PI * f / sample_rate_hz;
        let c1 = w.cos();
        let s1 = w.sin();
        let c2 = (2.0 * w).cos();
        let s2 = (2.0 * w).sin();

        // z^-1 = cos(w) - j sin(w)
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -self.b1 * s1 - self.b2 * s2;
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -self.a1 * s1 - self.a2 * s2;

        let n2 = nr * nr + ni * ni;
        let d2 = (dr * dr + di * di).max(1.0e-20);
        amp_to_db((n2 / d2).sqrt())
    }
}

/// A stateful transposed-direct-form-II biquad.
#[derive(Clone, Debug)]
pub struct Biquad {
    coeffs: BiquadCoeffs,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn coeffs(&self) -> BiquadCoeffs {
        self.coeffs
    }

    /// Replaces coefficients while preserving delay state.
    /// For fast parameter modulation, smooth the parameters before calling this.
    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs) {
        self.coeffs = coeffs;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.coeffs.b0 * x + self.z1;
        self.z1 = self.coeffs.b1 * x - self.coeffs.a1 * y + self.z2;
        self.z2 = self.coeffs.b2 * x - self.coeffs.a2 * y;
        y
    }
}

/// A cascade of biquads. pHRTF, distance EQ, and proximity EQ are all implemented
/// as cascades because that matches the practical DAW/game-engine model.
#[derive(Clone, Debug)]
pub struct BiquadChain {
    filters: Vec<Biquad>,
}

impl BiquadChain {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.filters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Rebuilds the chain from coefficients. Existing states are preserved where
    /// filter indices still exist; new filters start from zero state.
    pub fn set_coefficients(&mut self, coeffs: &[BiquadCoeffs]) {
        if self.filters.len() > coeffs.len() {
            self.filters.truncate(coeffs.len());
        }
        for (i, c) in coeffs.iter().copied().enumerate() {
            if let Some(f) = self.filters.get_mut(i) {
                f.set_coeffs(c);
            } else {
                self.filters.push(Biquad::new(c));
            }
        }
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, mut x: f32) -> f32 {
        for f in &mut self.filters {
            x = f.process(x);
        }
        x
    }

    pub fn magnitude_db(&self, sample_rate_hz: f32, freq_hz: f32) -> f32 {
        self.filters
            .iter()
            .map(|f| f.coeffs().magnitude_db(sample_rate_hz, freq_hz))
            .sum()
    }
}

impl Default for BiquadChain {
    fn default() -> Self {
        Self::new()
    }
}

fn sanitize_f0_q(sample_rate_hz: f32, f0_hz: f32, q: f32) -> (f32, f32) {
    let nyq = sample_rate_hz * 0.5;
    let f0 = clamp(f0_hz, 10.0, nyq * 0.95);
    let q = clamp(q, 0.05, 100.0);
    (f0, q)
}

/// RBJ peaking EQ.
///
/// Positive `gain_db` creates a peak. Negative `gain_db` creates a notch-like dip.
pub fn peaking_eq(sample_rate_hz: f32, f0_hz: f32, q: f32, gain_db: f32) -> BiquadCoeffs {
    if gain_db.abs() < 0.0001 {
        return BiquadCoeffs::BYPASS;
    }
    let (f0, q) = sanitize_f0_q(sample_rate_hz, f0_hz, q);
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate_hz;
    let cw = w0.cos();
    let sw = w0.sin();
    let alpha = sw / (2.0 * q);

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cw;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cw;
    let a2 = 1.0 - alpha / a;
    BiquadCoeffs::normalized(b0, b1, b2, a0, a1, a2)
}

/// RBJ low-shelf EQ with shelf slope S=1.
pub fn low_shelf(sample_rate_hz: f32, f0_hz: f32, gain_db: f32) -> BiquadCoeffs {
    if gain_db.abs() < 0.0001 {
        return BiquadCoeffs::BYPASS;
    }
    let (f0, _) = sanitize_f0_q(sample_rate_hz, f0_hz, 0.707);
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate_hz;
    let cw = w0.cos();
    let sw = w0.sin();
    // For S=1 the cookbook term simplifies to alpha = sin(w0)/2 * sqrt(2).
    let alpha = sw * std::f32::consts::FRAC_1_SQRT_2;
    let sqrt_a = a.sqrt();

    let b0 = a * ((a + 1.0) - (a - 1.0) * cw + 2.0 * sqrt_a * alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cw);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cw - 2.0 * sqrt_a * alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cw + 2.0 * sqrt_a * alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cw);
    let a2 = (a + 1.0) + (a - 1.0) * cw - 2.0 * sqrt_a * alpha;
    BiquadCoeffs::normalized(b0, b1, b2, a0, a1, a2)
}

/// RBJ high-shelf EQ with shelf slope S=1.
pub fn high_shelf(sample_rate_hz: f32, f0_hz: f32, gain_db: f32) -> BiquadCoeffs {
    if gain_db.abs() < 0.0001 {
        return BiquadCoeffs::BYPASS;
    }
    let (f0, _) = sanitize_f0_q(sample_rate_hz, f0_hz, 0.707);
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate_hz;
    let cw = w0.cos();
    let sw = w0.sin();
    let alpha = sw * std::f32::consts::FRAC_1_SQRT_2;
    let sqrt_a = a.sqrt();

    let b0 = a * ((a + 1.0) + (a - 1.0) * cw + 2.0 * sqrt_a * alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cw);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cw - 2.0 * sqrt_a * alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cw + 2.0 * sqrt_a * alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cw);
    let a2 = (a + 1.0) - (a - 1.0) * cw - 2.0 * sqrt_a * alpha;
    BiquadCoeffs::normalized(b0, b1, b2, a0, a1, a2)
}

/// RBJ second-order low-pass filter.
pub fn lowpass(sample_rate_hz: f32, f0_hz: f32, q: f32) -> BiquadCoeffs {
    let (f0, q) = sanitize_f0_q(sample_rate_hz, f0_hz, q);
    let w0 = 2.0 * std::f32::consts::PI * f0 / sample_rate_hz;
    let cw = w0.cos();
    let sw = w0.sin();
    let alpha = sw / (2.0 * q);

    let b0 = (1.0 - cw) * 0.5;
    let b1 = 1.0 - cw;
    let b2 = (1.0 - cw) * 0.5;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cw;
    let a2 = 1.0 - alpha;
    BiquadCoeffs::normalized(b0, b1, b2, a0, a1, a2)
}
