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
use crate::math::{clamp, deg_to_rad, lerp, rad_to_deg, smoothstep};

/// How to handle the N1/N2 notch-frequency trajectory.
#[derive(Clone, Copy, Debug)]
pub enum NotchTrajectoryMode {
    /// Median-plane trajectory calibrated against the local RIEC SOFA dummy-head
    /// references. This follows Iida's qualitative PNP behavior: P1 remains
    /// fixed while N1 rises quickly from the front toward the upper hemisphere
    /// and relaxes again toward the rear.
    SofaCalibratedMedianPlane,
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
            // Tuned against the local RIEC dummy-head SOFA subjects in
            // `hrtf_debug/sofa`: P1 sits around 3.8-4.5 kHz in front, while
            // side incidence is handled by the renderer's lateral pinna peak.
            f_p1_hz: 4_200.0,
            // P2 has large listener variance in the SOFA set; the two local
            // subjects repeatedly show useful energy around 6.5-7 kHz, so keep
            // this lower and broad rather than forcing one subject's 10 kHz peak.
            f_p2_hz: 6_800.0,
            // Front reference notch frequencies. These are strongly personal.
            f_n1_front_hz: 8_200.0,
            f_n2_front_hz: 11_300.0,

            p1_gain_db: 6.0,
            p2_gain_db: 2.0,
            n1_gain_db: -11.0,
            n2_gain_db: -6.0,

            p1_q: 1.0,
            p2_q: 1.4,
            n1_q: 3.0,
            n2_q: 2.0,

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
            trajectory_mode: NotchTrajectoryMode::SofaCalibratedMedianPlane,
            lower_hemisphere_mode: LowerHemisphereMode::MirrorWithReducedStrength {
                strength: 0.35,
            },
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
/// - 90 deg: up *or* directly to either side
/// - 180 deg: rear horizon
///
/// `beta` is defined as the angle between the source direction and the frontal
/// axis (`+x`), i.e. `acos` of the frontal direction cosine
/// `cos(el)·cos(az)`. This is a smooth, full-sphere "how far back is it"
/// coordinate.
///
/// The earlier implementation projected onto the median plane with
/// `atan2(up, front)`. That projection collapses the lateral dimension onto the
/// *sign* of `front`, so a horizontal source crossing `±90°` made `beta` jump
/// instantly between `0°` (front) and `180°` (rear) — an audible spectral
/// discontinuity at the sides. Measuring the angle from the frontal axis keeps
/// `beta` continuous (and differentiable away from the exact front/rear poles)
/// across the whole sphere, including the `±90°` and `±180°` crossings, while
/// preserving the median-plane behaviour: on the median plane
/// `acos(cos(el)·cos(az))` still equals the elevation angle in front and
/// `180° − elevation` behind.
///
/// The original model is upper-median-plane. For non-median directions this is a
/// pragmatic projection, not a measured full-sphere PNP interpolation.
pub fn beta_from_direction(direction: Direction3D, mode: LowerHemisphereMode) -> (f32, f32) {
    let az = deg_to_rad(direction.azimuth_deg);
    let el = deg_to_rad(direction.elevation_deg);
    let up = el.sin();

    // Angle from the frontal axis: 0 = front, 90 = up/side, 180 = rear.
    // Continuous across ±90° and ±180° because it never tests the sign of a
    // coordinate that is passing through zero.
    let frontal_cos = clamp(el.cos() * az.cos(), -1.0, 1.0);
    let beta = clamp(rad_to_deg(frontal_cos.acos()), 0.0, 180.0);

    if up >= 0.0 {
        return (beta, 1.0);
    }

    // Do not step the spectral cue depth at the horizon. A moving source that
    // crosses elevation=0 would otherwise jump from full upper-hemisphere cues
    // to reduced lower-hemisphere cues in one control update, which is audible
    // as a click/pop.
    let lower_amount = smoothstep(0.0, 1.0, clamp(-up, 0.0, 1.0));
    match mode {
        LowerHemisphereMode::ClampToHorizon => {
            // Project to the nearest horizon (elevation → 0) but keep a smooth
            // front/back angle so there is no step; only the cue depth is
            // reduced gradually as the source moves further below the listener.
            let horizon_cos = clamp(az.cos(), -1.0, 1.0);
            let strength = lerp(1.0, 0.15, lower_amount);
            (clamp(rad_to_deg(horizon_cos.acos()), 0.0, 180.0), strength)
        }
        LowerHemisphereMode::MirrorWithReducedStrength { strength } => {
            // `beta` is already even in elevation (cos is even), so the lower
            // hemisphere mirrors the upper one automatically; only cue depth
            // fades toward the configured lower-hemisphere strength.
            let strength = lerp(1.0, clamp(strength, 0.0, 1.0), lower_amount);
            (beta, strength)
        }
    }
}

fn notch_delta_hz(beta_deg: f32, mode: NotchTrajectoryMode) -> (f32, f32) {
    let b = clamp(beta_deg, 0.0, 180.0);

    match mode {
        NotchTrajectoryMode::SofaCalibratedMedianPlane => {
            let dn1 = interpolate_beta_anchors(
                b,
                &[
                    (0.0, 0.0),
                    (30.0, 2_200.0),
                    (60.0, 2_700.0),
                    (90.0, 3_000.0),
                    (120.0, 3_100.0),
                    (150.0, 2_600.0),
                    (180.0, 1_100.0),
                ],
            );
            let dn2 = interpolate_beta_anchors(
                b,
                &[
                    (0.0, 0.0),
                    (30.0, 900.0),
                    (60.0, 2_600.0),
                    (90.0, 3_400.0),
                    (120.0, 3_900.0),
                    (150.0, 3_400.0),
                    (180.0, 3_000.0),
                ],
            );
            (dn1, dn2)
        }
        NotchTrajectoryMode::IidaLikePolynomialHzScale => {
            let b2 = b * b;
            let b3 = b2 * b;
            let b4 = b3 * b;
            // Coefficients chosen so that the polynomial produces kHz-scale movement.
            // This is the interpretation that makes the pHRTF perceptually meaningful.
            let dn1 = 1.001e-5 * b4 - 6.431e-3 * b3 + 8.686e-1 * b2 - 3.265e-1 * b;
            let dn2 = 1.310e-5 * b4 - 5.154e-3 * b3 + 5.020e-1 * b2 + 2.563e1 * b;
            (dn1, dn2)
        }
        NotchTrajectoryMode::UserPastedSmallScalePolynomial => {
            let b2 = b * b;
            let b3 = b2 * b;
            let b4 = b3 * b;
            let dn1 = 1.001e-7 * b4 - 6.431e-5 * b3 + 8.686e-3 * b2 - 3.265e-1 * b;
            let dn2 = 1.310e-7 * b4 - 5.154e-5 * b3 + 5.020e-3 * b2 + 2.563e-1 * b;
            (dn1, dn2)
        }
    }
}

fn interpolate_beta_anchors(beta_deg: f32, anchors: &[(f32, f32)]) -> f32 {
    debug_assert!(anchors.len() >= 2);
    for pair in anchors.windows(2) {
        let (b0, v0) = pair[0];
        let (b1, v1) = pair[1];
        if beta_deg <= b1 {
            let t = smoothstep(b0, b1, beta_deg);
            return lerp(v0, v1, t);
        }
    }
    anchors[anchors.len() - 1].1
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

fn notch_elevation_shape(beta_deg: f32) -> (f32, f32, f32, f32) {
    // Iida's PNP model treats P1 as a fixed reference peak, while N1/N2 are the
    // main elevation-dependent cues. The local RIEC SOFA references follow that
    // pattern: N1 becomes much shallower and broader near the zenith, but N2
    // remains a useful high-frequency cue and should not disappear as much.
    let horizon_amount = clamp((beta_deg - 90.0).abs() / 90.0, 0.0, 1.0);
    let n1_gain_scale = lerp(0.32, 1.0, horizon_amount);
    let n2_gain_scale = lerp(0.72, 1.0, horizon_amount);
    let n1_q_scale = lerp(0.65, 1.0, horizon_amount);
    let n2_q_scale = lerp(0.85, 1.0, horizon_amount);
    (n1_gain_scale, n2_gain_scale, n1_q_scale, n2_q_scale)
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
    let (n1_gain_scale, n2_gain_scale, n1_q_scale, n2_q_scale) = notch_elevation_shape(beta);
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
        gain_db: profile.n1_gain_db * strength * n1_gain_scale,
        q: profile.n1_q * n1_q_scale,
    });
    bands.push(PhrtfBand {
        name: "N2",
        frequency_hz: f_n2,
        gain_db: profile.n2_gain_db * strength * n2_gain_scale,
        q: profile.n2_q * n2_q_scale,
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
