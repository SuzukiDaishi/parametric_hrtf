//! Microphone-style proximity effect approximation.
//!
//! Strictly speaking, microphone proximity effect is caused by pressure-gradient
//! microphone construction, not by human HRTF itself. However, it is often useful
//! in games/VR/voice rendering as a perceptual "very close" cue. This module adds
//! an optional low-shelf boost when the source is closer than a threshold.

use crate::biquad::{low_shelf, BiquadCoeffs};
use crate::math::{clamp, smoothstep};

/// Typical microphone directivity patterns expressed as pressure-gradient amount.
#[derive(Clone, Copy, Debug)]
pub enum MicrophonePattern {
    Omni,
    WideCardioid,
    Cardioid,
    SuperCardioid,
    HyperCardioid,
    FigureEight,
    /// Directly provide 0..1 pressure-gradient amount.
    Custom { gradient_amount: f32 },
}

impl MicrophonePattern {
    pub fn gradient_amount(self) -> f32 {
        match self {
            MicrophonePattern::Omni => 0.0,
            MicrophonePattern::WideCardioid => 0.35,
            MicrophonePattern::Cardioid => 0.55,
            MicrophonePattern::SuperCardioid => 0.75,
            MicrophonePattern::HyperCardioid => 0.85,
            MicrophonePattern::FigureEight => 1.0,
            MicrophonePattern::Custom { gradient_amount } => clamp(gradient_amount, 0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProximityConfig {
    pub enabled: bool,
    pub pattern: MicrophonePattern,
    /// Maximum low-frequency boost at or below `full_boost_distance_m`.
    pub max_boost_db: f32,
    /// Low-shelf transition frequency.
    pub shelf_frequency_hz: f32,
    /// At this distance and beyond, proximity boost becomes 0 dB.
    pub zero_boost_distance_m: f32,
    /// At this distance and closer, proximity boost reaches maximum.
    pub full_boost_distance_m: f32,
}

impl Default for ProximityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pattern: MicrophonePattern::Cardioid,
            max_boost_db: 6.0,
            shelf_frequency_hz: 180.0,
            zero_boost_distance_m: 1.0,
            full_boost_distance_m: 0.08,
        }
    }
}

impl ProximityConfig {
    pub fn gain_db_at_distance(&self, distance_m: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let amount = self.pattern.gradient_amount();
        if amount <= 0.0 {
            return 0.0;
        }
        let t = smoothstep(
            self.full_boost_distance_m.max(0.001),
            self.zero_boost_distance_m.max(self.full_boost_distance_m + 0.001),
            distance_m,
        );
        self.max_boost_db.max(0.0) * amount * (1.0 - t)
    }
}

pub fn design_proximity_eq(
    sample_rate_hz: f32,
    config: ProximityConfig,
    distance_m: f32,
) -> Vec<BiquadCoeffs> {
    let gain_db = config.gain_db_at_distance(distance_m);
    if gain_db.abs() < 0.001 {
        Vec::new()
    } else {
        vec![low_shelf(sample_rate_hz, config.shelf_frequency_hz, gain_db)]
    }
}
