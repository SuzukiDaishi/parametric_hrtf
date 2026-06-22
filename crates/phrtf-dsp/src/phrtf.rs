//! Iida-style parametric HRTF / PNP spectral model.
//!
//! Core idea:
//!
//! - Extract important spectral peaks and notches from a measured HRTF.
//! - Represent each component by center frequency, level, and sharpness/Q.
//! - Recompose only those components with parametric EQ sections.
//!
//! The original Iida et al. pHRTF work is about **upper median-plane vertical
//! localization**. This module keeps that spirit. Horizontal cues are handled in
//! `geometry.rs` and `renderer.rs`.

use crate::biquad::{peaking_eq, BiquadCoeffs};
use crate::geometry::Direction3D;
use crate::math::{clamp, deg_to_rad, lerp, rad_to_deg};

/// How to handle the N1/N2 notch-frequency trajectory.
#[derive(Clone, Copy, Debug)]
pub enum NotchTrajectoryMode {
    /// Polynomial using the Hz-scale coefficient interpretation used by this prototype.
    /// This gives kHz-scale notch movement over beta=0..180 degrees.
    IidaLikePolynomialHzScale,
    /// The smaller coefficient scale from the user's pasted notes. Kept for comparison
    /// because some copied documents lose exponent formatting. It usually moves the
    /// notches too little to be useful, so it is not the default.
    UserPastedSmallScalePolynomial,
}

/// How to deal with directions below the horizontal plane.
#[derive(Clone, Copy, Debug)]
pub enum LowerHemisphereMode {
    /// Project lower directions to the nearest horizon. Safest when you do not want
    /// fake lower-hemisphere cues.
    ClampToHorizon,
    /// Mirror lower directions into the upper hemisphere, but scale spectral cue depth.
    /// Useful for games where some cue is better than none, but it is not a measured
    /// lower-hemisphere HRTF.
    MirrorWithReducedStrength { strength: f32 },
}

/// Listener-specific pHRTF parameters.
///
/// In a strict implementation these should be measured or estimated per listener.
/// Defaults are intentionally conservative and should be treated as a starting point.
#[derive(Clone, Copy, Debug)]
pub struct PhrtfProfile {
    pub f_p1_hz: f32,
    pub f_p2_hz: f32,
    pub f_n1_front_hz: f32,
    pub f_n2_front_hz: f32,

    pub p1_gain_db: f32,
    pub p2_gain_db: f32,
    pub n1_gain_db: f32,
    pub n2_gain_db: f32,

    pub p1_q: f32,
    pub p2_q: f32,
    pub n1_q: f32,
    pub n2_q: f32,

    /// Rear-localization helper peak.
    pub p0_frequency_hz: f32,
    pub p0_q: f32,
}

impl Default for PhrtfProfile {
    fn default() -> Self {
        Self {
            // P1 is a broad concha-related peak around 4-6 kHz.
            f_p1_hz: 4_500.0,
            // P2 often appears around the 7-9 kHz region and helps upper localization.
            f_p2_hz: 8_500.0,
            // Front reference notch frequencies. These are strongly personal.
            f_n1_front_hz: 8_000.0,
            f_n2_front_hz: 11_500.0,

            p1_gain_db: 5.0,
            p2_gain_db: 3.0,
            n1_gain_db: -12.0,
            n2_gain_db: -10.0,

            p1_q: 1.2,
            p2_q: 1.5,
            n1_q: 3.5,
            n2_q: 3.0,

            p0_frequency_hz: 1_031.25,
            p0_q: 1.0,
        }
    }
}

/// Global pHRTF behavior settings.
#[derive(Clone, Copy, Debug)]
pub struct PhrtfConfig {
    pub trajectory_mode: NotchTrajectoryMode,
    pub lower_hemisphere_mode: LowerHemisphereMode,
    /// Overall scaling for the spectral pHRTF cues. 1.0 = default.
    pub spectral_strength: f32,
    /// Include P0 rear helper peak.
    pub enable_p0: bool,
    /// Clamp all generated P/N frequencies into a safe range.
    pub min_feature_hz: f32,
    pub max_feature_hz: f32,
}

impl Default for PhrtfConfig {
    fn default() -> Self {
        Self {
            trajectory_mode: NotchTrajectoryMode::IidaLikePolynomialHzScale,
            lower_hemisphere_mode: LowerHemisphereMode::MirrorWithReducedStrength { strength: 0.35 },
            spectral_strength: 1.0,
            enable_p0: true,
            min_feature_hz: 1_000.0,
            max_feature_hz: 18_000.0,
        }
    }
}

/// A single spectral peak/notch component.
#[derive(Clone, Copy, Debug)]
pub struct PhrtfBand {
    pub name: &'static str,
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Debug/inspection result containing generated spectral components.
#[derive(Clone, Debug)]
pub struct PhrtfBandSet {
    pub beta_deg: f32,
    pub vertical_strength: f32,
    pub bands: Vec<PhrtfBand>,
}

impl PhrtfBandSet {
    pub fn to_biquad_coefficients(&self, sample_rate_hz: f32) -> Vec<BiquadCoeffs> {
        self.bands
            .iter()
            .map(|b| peaking_eq(sample_rate_hz, b.frequency_hz, b.q, b.gain_db))
            .collect()
    }
}

/// Converts arbitrary 3D direction to the pHRTF beta coordinate.
///
/// beta convention:
///
/// - 0 deg: front horizon
/// - 90 deg: up
/// - 180 deg: rear horizon
///
/// The original model is upper-median-plane. For non-median directions this is a
/// pragmatic projection, not a measured full-sphere PNP interpolation.
pub fn beta_from_direction(direction: Direction3D, mode: LowerHemisphereMode) -> (f32, f32) {
    let az = deg_to_rad(direction.azimuth_deg);
    let el = deg_to_rad(direction.elevation_deg);
    let front = el.cos() * az.cos();
    let up = el.sin();

    if up >= 0.0 {
        let mut beta = rad_to_deg(up.atan2(front));
        if beta < 0.0 {
            beta += 360.0;
        }
        (clamp(beta, 0.0, 180.0), 1.0)
    } else {
        match mode {
            LowerHemisphereMode::ClampToHorizon => {
                if front >= 0.0 {
                    (0.0, 0.15)
                } else {
                    (180.0, 0.15)
                }
            }
            LowerHemisphereMode::MirrorWithReducedStrength { strength } => {
                let mut beta = rad_to_deg((-up).atan2(front));
                if beta < 0.0 {
                    beta += 360.0;
                }
                (clamp(beta, 0.0, 180.0), clamp(strength, 0.0, 1.0))
            }
        }
    }
}

fn notch_delta_hz(beta_deg: f32, mode: NotchTrajectoryMode) -> (f32, f32) {
    let b = clamp(beta_deg, 0.0, 180.0);
    let b2 = b * b;
    let b3 = b2 * b;
    let b4 = b3 * b;

    match mode {
        NotchTrajectoryMode::IidaLikePolynomialHzScale => {
            // Coefficients chosen so that the polynomial produces kHz-scale movement.
            // This is the interpretation that makes the pHRTF perceptually meaningful.
            let dn1 = 1.001e-5 * b4 - 6.431e-3 * b3 + 8.686e-1 * b2 - 3.265e-1 * b;
            let dn2 = 1.310e-5 * b4 - 5.154e-3 * b3 + 5.020e-1 * b2 + 2.563e1 * b;
            (dn1, dn2)
        }
        NotchTrajectoryMode::UserPastedSmallScalePolynomial => {
            let dn1 = 1.001e-7 * b4 - 6.431e-5 * b3 + 8.686e-3 * b2 - 3.265e-1 * b;
            let dn2 = 1.310e-7 * b4 - 5.154e-5 * b3 + 5.020e-3 * b2 + 2.563e-1 * b;
            (dn1, dn2)
        }
    }
}

fn p0_gain_db(beta_deg: f32) -> f32 {
    if beta_deg < 120.0 {
        0.0
    } else if beta_deg < 150.0 {
        lerp(2.0, 3.0, (beta_deg - 120.0) / 30.0)
    } else {
        lerp(3.0, 5.0, (beta_deg - 150.0) / 30.0)
    }
}

/// Generate the pHRTF spectral bands for a direction.
pub fn design_phrtf_bands(
    profile: PhrtfProfile,
    config: PhrtfConfig,
    direction: Direction3D,
) -> PhrtfBandSet {
    let (beta, vertical_strength) = beta_from_direction(direction, config.lower_hemisphere_mode);
    design_phrtf_bands_beta(profile, config, beta, vertical_strength)
}

/// Generate the pHRTF spectral bands for an explicit `beta` coordinate.
///
/// This is the per-ear entry point used by the renderer: the left and right
/// ears are coloured with slightly different `beta` values so the
/// contralateral (shadowed) ear sounds more "rear" and the ipsilateral ear
/// more "frontal". That difference is the main horizontal-localization cue the
/// median-plane model cannot express on its own. Passing the `beta` computed
/// by [`beta_from_direction`] reproduces the plain [`design_phrtf_bands`]
/// result.
///
/// `beta_deg` is clamped to `0..=180`; `vertical_strength` is the
/// hemisphere-dependent depth scaler returned alongside `beta`.
pub fn design_phrtf_bands_beta(
    profile: PhrtfProfile,
    config: PhrtfConfig,
    beta_deg: f32,
    vertical_strength: f32,
) -> PhrtfBandSet {
    let beta = clamp(beta_deg, 0.0, 180.0);
    let vertical_strength = clamp(vertical_strength, 0.0, 1.0);
    let strength = clamp(config.spectral_strength, 0.0, 4.0) * vertical_strength;
    let (dn1, dn2) = notch_delta_hz(beta, config.trajectory_mode);

    let f_n1 = clamp(
        profile.f_n1_front_hz + dn1,
        config.min_feature_hz,
        config.max_feature_hz,
    );
    let f_n2 = clamp(
        profile.f_n2_front_hz + dn2,
        config.min_feature_hz,
        config.max_feature_hz,
    );

    let mut bands = Vec::with_capacity(5);
    bands.push(PhrtfBand {
        name: "P1",
        frequency_hz: profile.f_p1_hz,
        gain_db: profile.p1_gain_db * strength,
        q: profile.p1_q,
    });
    bands.push(PhrtfBand {
        name: "P2",
        frequency_hz: profile.f_p2_hz,
        gain_db: profile.p2_gain_db * strength,
        q: profile.p2_q,
    });
    bands.push(PhrtfBand {
        name: "N1",
        frequency_hz: f_n1,
        gain_db: profile.n1_gain_db * strength,
        q: profile.n1_q,
    });
    bands.push(PhrtfBand {
        name: "N2",
        frequency_hz: f_n2,
        gain_db: profile.n2_gain_db * strength,
        q: profile.n2_q,
    });

    if config.enable_p0 {
        let g = p0_gain_db(beta) * strength;
        if g.abs() > 0.001 {
            bands.push(PhrtfBand {
                name: "P0",
                frequency_hz: profile.p0_frequency_hz,
                gain_db: g,
                q: profile.p0_q,
            });
        }
    }

    PhrtfBandSet {
        beta_deg: beta,
        vertical_strength,
        bands,
    }
}
