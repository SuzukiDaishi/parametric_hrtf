//! Atmospheric absorption and a real-time-friendly parametric EQ approximation.
//!
//! The exact ISO 9613-1 absorption curve is frequency dependent and is not itself a
//! small IIR filter. In this crate we do two things:
//!
//! 1. Compute the physical absorption coefficient `alpha(f)` in dB/m.
//! 2. Approximate the resulting loss curve with a few broad parametric filters so
//!    that it can run in a game/audio engine in real time.
//!
//! For short game distances, air absorption is usually subtle. At 10 m it may only
//! be fractions of a dB even at high frequencies under normal conditions. It becomes
//! more important at long outdoor distances, high frequencies, or dry/cold air.

use crate::biquad::{high_shelf, peaking_eq, BiquadCoeffs};
use crate::math::clamp;

/// Atmospheric state for ISO 9613-1 style absorption.
#[derive(Clone, Copy, Debug)]
pub struct Atmosphere {
    /// Temperature in Celsius.
    pub temperature_c: f32,
    /// Relative humidity in 0..1. 0.5 means 50% RH.
    pub relative_humidity: f32,
    /// Static pressure in Pascal. Standard atmosphere is 101325 Pa.
    pub pressure_pa: f32,
}

impl Atmosphere {
    pub fn standard() -> Self {
        Self {
            temperature_c: 20.0,
            relative_humidity: 0.50,
            pressure_pa: 101_325.0,
        }
    }

    /// Approximate speed of sound in m/s.
    pub fn speed_of_sound_mps(&self) -> f32 {
        331.3 + 0.606 * self.temperature_c
    }

    pub fn temperature_k(&self) -> f32 {
        self.temperature_c + 273.15
    }

    /// ISO 9613-1 style atmospheric absorption coefficient in dB/m.
    ///
    /// The expression follows the common implementation used in acoustics packages:
    /// oxygen and nitrogen relaxation frequencies plus classical absorption.
    /// Frequency is in Hz.
    pub fn attenuation_db_per_m(&self, frequency_hz: f32) -> f32 {
        let f = frequency_hz.max(1.0);
        let p_ref = 101_325.0;
        let t_ref = 293.15;
        let t_triple = 273.16;
        let t = self.temperature_k().max(1.0);
        let p = self.pressure_pa.max(1.0);
        let rh = clamp(self.relative_humidity, 0.0, 1.0);

        // Saturation vapor pressure relative to reference pressure.
        let psat_over_pref = 10.0_f32.powf(-6.8346 * (t_triple / t).powf(1.261) + 4.6151);
        let h = rh * psat_over_pref * (p_ref / p);

        let fr_o = (p / p_ref) * (24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h));
        let fr_n = (p / p_ref)
            * (t / t_ref).powf(-0.5)
            * (9.0 + 280.0 * h * (-4.170 * ((t / t_ref).powf(-1.0 / 3.0) - 1.0)).exp());

        let classical = 1.84e-11 * (p_ref / p) * (t / t_ref).sqrt();
        let oxygen = 0.01275 * (-2239.1 / t).exp() / (fr_o + f * f / fr_o);
        let nitrogen = 0.1068 * (-3352.0 / t).exp() / (fr_n + f * f / fr_n);
        let molecular = (t / t_ref).powf(-2.5) * (oxygen + nitrogen);

        8.686 * f * f * (classical + molecular)
    }

    /// Extra absorption loss in dB relative to a reference distance.
    ///
    /// The reference distance is important: if your source samples are authored as
    /// though they were recorded at 1 m, you normally want **zero** air loss at 1 m
    /// and increasing loss only beyond that.
    pub fn extra_loss_db(&self, distance_m: f32, reference_distance_m: f32, frequency_hz: f32) -> f32 {
        let path = (distance_m - reference_distance_m).max(0.0);
        self.attenuation_db_per_m(frequency_hz) * path
    }
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self::standard()
    }
}

/// Tuning for converting ISO absorption into a small parametric EQ cascade.
#[derive(Clone, Copy, Debug)]
pub struct AirAbsorptionEqConfig {
    /// Enable/disable air absorption filters.
    pub enabled: bool,
    /// Multiplier for artistic exaggeration. 1.0 is physical. 2-5 can be useful
    /// in games because real absorption can be too subtle at short distances.
    pub perceptual_scale: f32,
    /// Extra cap to prevent extreme losses from destroying the signal.
    pub max_loss_db: f32,
}

impl Default for AirAbsorptionEqConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            perceptual_scale: 1.0,
            max_loss_db: 36.0,
        }
    }
}

/// Build a compact EQ approximation of air absorption.
///
/// This does not exactly match the ISO curve. It approximates it with:
///
/// - a broad 4 kHz dip,
/// - a broad 8 kHz dip,
/// - a high shelf starting around 12 kHz.
///
/// The filters are intentionally broad and mild because the physical curve is smooth.
pub fn design_air_absorption_eq(
    sample_rate_hz: f32,
    atmosphere: Atmosphere,
    config: AirAbsorptionEqConfig,
    distance_m: f32,
    reference_distance_m: f32,
) -> Vec<BiquadCoeffs> {
    if !config.enabled {
        return Vec::new();
    }

    let scale = config.perceptual_scale.max(0.0);
    let max_loss = config.max_loss_db.max(0.0);

    let l1 = atmosphere.extra_loss_db(distance_m, reference_distance_m, 1_000.0) * scale;
    let l4 = atmosphere.extra_loss_db(distance_m, reference_distance_m, 4_000.0) * scale;
    let l8 = atmosphere.extra_loss_db(distance_m, reference_distance_m, 8_000.0) * scale;
    let l16 = atmosphere.extra_loss_db(distance_m, reference_distance_m, 16_000.0) * scale;

    // Make the EQ represent *relative* spectral coloration, not broadband gain.
    // Broadband distance gain is handled elsewhere.
    let rel4 = clamp(l4 - l1, 0.0, max_loss);
    let rel8 = clamp(l8 - l1, 0.0, max_loss);
    let rel16 = clamp(l16 - l1, 0.0, max_loss);

    let mut coeffs = Vec::new();
    if rel4 > 0.01 {
        coeffs.push(peaking_eq(sample_rate_hz, 4_000.0, 0.75, -0.55 * rel4));
    }
    if rel8 > 0.01 {
        coeffs.push(peaking_eq(sample_rate_hz, 8_000.0, 0.80, -0.45 * rel8));
    }
    if rel16 > 0.01 {
        coeffs.push(high_shelf(sample_rate_hz, 12_000.0, -rel16));
    }
    coeffs
}
