//! Tiny deterministic random and smoothed noise utilities.
//!
//! These support the optional air-turbulence / scintillation approximation.
//! The physical literature models this as stochastic amplitude and phase variation;
//! this implementation provides a lightweight block-free approximation suitable for
//! real-time use.

use crate::math::{clamp, db_to_amp};

#[derive(Clone, Debug)]
pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    pub fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    pub fn next_f32_0_1(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }

    pub fn next_f32_minus1_1(&mut self) -> f32 {
        self.next_f32_0_1() * 2.0 - 1.0
    }
}

/// Piecewise-linear random LFO.
#[derive(Clone, Debug)]
pub struct SmoothRandom {
    rng: XorShift32,
    sample_rate_hz: f32,
    rate_hz: f32,
    value: f32,
    step: f32,
    remaining: usize,
}

impl SmoothRandom {
    pub fn new(seed: u32, sample_rate_hz: f32, rate_hz: f32) -> Self {
        let mut s = Self {
            rng: XorShift32::new(seed),
            sample_rate_hz,
            rate_hz: rate_hz.max(0.01),
            value: 0.0,
            step: 0.0,
            remaining: 0,
        };
        s.choose_next_segment();
        s
    }

    pub fn set_rate_hz(&mut self, rate_hz: f32) {
        self.rate_hz = rate_hz.max(0.01);
    }

    fn choose_next_segment(&mut self) {
        let samples = (self.sample_rate_hz / self.rate_hz).round().max(1.0) as usize;
        let target = self.rng.next_f32_minus1_1();
        self.step = (target - self.value) / samples as f32;
        self.remaining = samples;
    }

    pub fn process(&mut self) -> f32 {
        if self.remaining == 0 {
            self.choose_next_segment();
        }
        self.value += self.step;
        self.remaining -= 1;
        self.value
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TurbulenceConfig {
    pub enabled: bool,
    /// 0..1 overall strength.
    pub strength: f32,
    /// Maximum stochastic gain variation at 100 m in dB.
    pub gain_db_at_100m: f32,
    /// Flutter rate. 0.1-2 Hz is usually enough.
    pub rate_hz: f32,
    /// Distance below which turbulence is not applied.
    pub start_distance_m: f32,
}

impl Default for TurbulenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.15,
            gain_db_at_100m: 2.0,
            rate_hz: 0.7,
            start_distance_m: 10.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TurbulenceModulator {
    config: TurbulenceConfig,
    distance_m: f32,
    noise: SmoothRandom,
}

impl TurbulenceModulator {
    pub fn new(sample_rate_hz: f32, config: TurbulenceConfig, seed: u32) -> Self {
        Self {
            config,
            distance_m: 1.0,
            noise: SmoothRandom::new(seed, sample_rate_hz, config.rate_hz),
        }
    }

    pub fn update(&mut self, config: TurbulenceConfig, distance_m: f32) {
        self.config = config;
        self.distance_m = distance_m.max(0.0);
        self.noise.set_rate_hz(config.rate_hz);
    }

    #[inline]
    pub fn process_gain(&mut self) -> f32 {
        if !self.config.enabled || self.distance_m <= self.config.start_distance_m {
            return 1.0;
        }
        let strength = clamp(self.config.strength, 0.0, 1.0);
        let distance_factor = clamp(
            ((self.distance_m - self.config.start_distance_m) / 100.0).sqrt(),
            0.0,
            1.5,
        );
        let db = self.noise.process()
            * self.config.gain_db_at_100m
            * strength
            * distance_factor;
        db_to_amp(db)
    }
}
