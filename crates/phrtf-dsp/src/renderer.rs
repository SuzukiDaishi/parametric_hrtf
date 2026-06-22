//! Full stereo renderer that combines pHRTF, distance attenuation, proximity EQ,
//! air absorption, ITD/ILD, and head shadow.
//!
//! The processing chain per ear is:
//!
//! ```text
//! mono input
//!   -> inverse-distance gain
//!   -> stochastic turbulence gain
//!   -> ear split and ITD fractional delay
//!   -> proximity low shelf
//!   -> pHRTF P/N spectral cascade
//!   -> air absorption EQ
//!   -> head-shadow high shelf on far ear
//!   -> stereo output
//! ```
//!
//! The ordering is pragmatic. If you integrate with a larger engine you may want to
//! move distance gain outside this renderer, or split direct/reverb paths.

use crate::air_absorption::{design_air_absorption_eq, AirAbsorptionEqConfig, Atmosphere};
use crate::biquad::{high_shelf, BiquadChain, BiquadCoeffs};
use crate::delay::FractionalDelay;
use crate::geometry::{compute_ear_geometry, inverse_distance_gain, Direction3D, EarGeometry};
use crate::math::clamp;
use crate::noise::{TurbulenceConfig, TurbulenceModulator};
use crate::phrtf::{
    beta_from_direction, design_phrtf_bands, design_phrtf_bands_beta, PhrtfBandSet, PhrtfConfig,
    PhrtfProfile,
};
use crate::proximity::{design_proximity_eq, ProximityConfig};

/// Main renderer configuration.
#[derive(Clone, Copy, Debug)]
pub struct RendererConfig {
    pub sample_rate_hz: f32,

    /// Approximate human head radius. 8.75 cm is a common spherical-head value.
    pub head_radius_m: f32,

    /// Distance at which authored sound is considered neutral.
    pub reference_distance_m: f32,
    /// Prevents infinite gain when a source reaches the listener.
    pub min_distance_m: f32,
    /// Maximum distance-gain boost when distance < reference.
    pub max_near_gain_db: f32,

    pub phrtf_profile: PhrtfProfile,
    pub phrtf_config: PhrtfConfig,

    pub atmosphere: Atmosphere,
    pub air_absorption: AirAbsorptionEqConfig,
    pub proximity: ProximityConfig,
    pub turbulence: TurbulenceConfig,

    pub enable_distance_gain: bool,
    pub enable_itd: bool,
    pub enable_broadband_ild: bool,
    pub enable_head_shadow: bool,

    /// Broadband ILD is intentionally small; most horizontal coloration is handled
    /// by the head-shadow high shelf.
    pub max_broadband_ild_db: f32,
    /// Far-ear high-frequency attenuation at full lateral direction.
    pub max_head_shadow_db: f32,
    /// High-shelf transition frequency for head shadow.
    pub head_shadow_shelf_hz: f32,

    /// Per-ear pHRTF spectral divergence, in degrees of `beta` bias at full
    /// lateral direction. The ipsilateral ear is coloured toward the front
    /// (`beta - offset`) and the contralateral ear toward the rear
    /// (`beta + offset`), scaled by the lateral factor. This gives the two
    /// ears genuinely different peak/notch trajectories instead of a single
    /// shared cascade, which is the strongest horizontal-localization upgrade
    /// over the original prototype. `0.0` reproduces the legacy shared-cascade
    /// behaviour.
    pub phrtf_ear_offset_deg: f32,
}

impl RendererConfig {
    pub fn new(sample_rate_hz: f32) -> Self {
        Self {
            sample_rate_hz,
            head_radius_m: 0.0875,
            reference_distance_m: 1.0,
            min_distance_m: 0.05,
            max_near_gain_db: 18.0,

            phrtf_profile: PhrtfProfile::default(),
            phrtf_config: PhrtfConfig::default(),

            atmosphere: Atmosphere::default(),
            air_absorption: AirAbsorptionEqConfig::default(),
            proximity: ProximityConfig::default(),
            turbulence: TurbulenceConfig::default(),

            enable_distance_gain: true,
            enable_itd: true,
            enable_broadband_ild: true,
            enable_head_shadow: true,

            max_broadband_ild_db: 2.5,
            max_head_shadow_db: 8.0,
            head_shadow_shelf_hz: 1_800.0,

            phrtf_ear_offset_deg: 45.0,
        }
    }
}

/// Debug information for the most recent `update()` call.
#[derive(Clone, Debug)]
pub struct RendererDebugState {
    pub direction: Direction3D,
    pub ear_geometry: EarGeometry,
    pub distance_gain_amp: f32,
    pub phrtf_bands: PhrtfBandSet,
    pub proximity_gain_db: f32,
}

/// Stereo pHRTF renderer.
#[derive(Clone, Debug)]
pub struct SpatialPhrtfRenderer {
    config: RendererConfig,

    left_delay: FractionalDelay,
    right_delay: FractionalDelay,

    left_proximity: BiquadChain,
    right_proximity: BiquadChain,
    left_phrtf: BiquadChain,
    right_phrtf: BiquadChain,
    left_air: BiquadChain,
    right_air: BiquadChain,
    left_head_shadow: BiquadChain,
    right_head_shadow: BiquadChain,

    distance_gain_amp: f32,
    left_gain_amp: f32,
    right_gain_amp: f32,

    turbulence: TurbulenceModulator,
    debug: RendererDebugState,
}

impl SpatialPhrtfRenderer {
    pub fn new(config: RendererConfig) -> Self {
        let max_itd_s = 0.003; // enough for human ITD plus safety margin
        let max_delay_samples = (config.sample_rate_hz * max_itd_s).ceil() as usize + 8;
        let direction = Direction3D::front(config.reference_distance_m);
        let speed = config.atmosphere.speed_of_sound_mps();
        let ear_geometry = compute_ear_geometry(
            direction,
            config.head_radius_m,
            speed,
            config.max_broadband_ild_db,
        );
        let phrtf_bands = design_phrtf_bands(config.phrtf_profile, config.phrtf_config, direction);

        let mut r = Self {
            config,
            left_delay: FractionalDelay::new(max_delay_samples),
            right_delay: FractionalDelay::new(max_delay_samples),
            left_proximity: BiquadChain::new(),
            right_proximity: BiquadChain::new(),
            left_phrtf: BiquadChain::new(),
            right_phrtf: BiquadChain::new(),
            left_air: BiquadChain::new(),
            right_air: BiquadChain::new(),
            left_head_shadow: BiquadChain::new(),
            right_head_shadow: BiquadChain::new(),
            distance_gain_amp: 1.0,
            left_gain_amp: 1.0,
            right_gain_amp: 1.0,
            turbulence: TurbulenceModulator::new(
                config.sample_rate_hz,
                config.turbulence,
                0x1234_abcd,
            ),
            debug: RendererDebugState {
                direction,
                ear_geometry,
                distance_gain_amp: 1.0,
                phrtf_bands,
                proximity_gain_db: 0.0,
            },
        };
        r.update(direction);
        r
    }

    pub fn config(&self) -> RendererConfig {
        self.config
    }

    /// Replaces the config and rebuilds filters for the current direction.
    pub fn set_config(&mut self, config: RendererConfig) {
        self.config = config;
        let direction = self.debug.direction;
        self.update(direction);
    }

    pub fn debug_state(&self) -> &RendererDebugState {
        &self.debug
    }

    /// Update all direction/distance dependent parameters.
    ///
    /// Call this at control rate, not necessarily every sample. If direction moves
    /// quickly, smooth `Direction3D` externally before calling to reduce zipper noise.
    pub fn update(&mut self, direction: Direction3D) {
        let sr = self.config.sample_rate_hz;
        let speed = self.config.atmosphere.speed_of_sound_mps();
        let distance = direction.distance_m.max(self.config.min_distance_m);

        let max_broadband_ild = if self.config.enable_broadband_ild {
            self.config.max_broadband_ild_db
        } else {
            0.0
        };
        let ear = compute_ear_geometry(
            direction,
            self.config.head_radius_m,
            speed,
            max_broadband_ild,
        );

        if self.config.enable_itd {
            self.left_delay.set_delay_samples(ear.left_delay_s * sr);
            self.right_delay.set_delay_samples(ear.right_delay_s * sr);
        } else {
            self.left_delay.set_delay_samples(0.0);
            self.right_delay.set_delay_samples(0.0);
        }

        self.left_gain_amp = ear.left_gain_amp;
        self.right_gain_amp = ear.right_gain_amp;

        self.distance_gain_amp = if self.config.enable_distance_gain {
            inverse_distance_gain(
                distance,
                self.config.reference_distance_m,
                self.config.min_distance_m,
                self.config.max_near_gain_db,
            )
        } else {
            1.0
        };

        // pHRTF spectral bands, designed independently per ear. The median-plane
        // model maps a direction to a single `beta` (front=0, up=90, rear=180);
        // we bias that `beta` toward the front for the ipsilateral ear and toward
        // the rear for the contralateral (shadowed) ear, scaled by how lateral the
        // source is. The two ears therefore get different peak/notch trajectories,
        // which is the main horizontal-localization cue the shared cascade lacked.
        let (beta, vertical_strength) =
            beta_from_direction(direction, self.config.phrtf_config.lower_hemisphere_mode);
        let ear_offset = self.config.phrtf_ear_offset_deg.max(0.0) * ear.lateral.abs();
        // `ear.lateral` is +1 when the source is fully to the right.
        let (beta_left, beta_right) = if ear.lateral >= 0.0 {
            // Source on the right: right ear ipsilateral (more frontal), left
            // ear contralateral (more rear).
            ((beta + ear_offset).min(180.0), (beta - ear_offset).max(0.0))
        } else {
            ((beta - ear_offset).max(0.0), (beta + ear_offset).min(180.0))
        };
        let left_bands = design_phrtf_bands_beta(
            self.config.phrtf_profile,
            self.config.phrtf_config,
            beta_left,
            vertical_strength,
        );
        let right_bands = design_phrtf_bands_beta(
            self.config.phrtf_profile,
            self.config.phrtf_config,
            beta_right,
            vertical_strength,
        );
        self.left_phrtf
            .set_coefficients(&left_bands.to_biquad_coefficients(sr));
        self.right_phrtf
            .set_coefficients(&right_bands.to_biquad_coefficients(sr));
        // Centre (un-biased) band set retained for inspection / debug export.
        let phrtf_bands = design_phrtf_bands_beta(
            self.config.phrtf_profile,
            self.config.phrtf_config,
            beta,
            vertical_strength,
        );

        // Air absorption: same for both ears in this simplified model.
        let air_coeffs = design_air_absorption_eq(
            sr,
            self.config.atmosphere,
            self.config.air_absorption,
            distance,
            self.config.reference_distance_m,
        );
        self.left_air.set_coefficients(&air_coeffs);
        self.right_air.set_coefficients(&air_coeffs);

        // Proximity effect: same for both ears. This is a creative/microphone-like cue.
        let prox_coeffs = design_proximity_eq(sr, self.config.proximity, distance);
        self.left_proximity.set_coefficients(&prox_coeffs);
        self.right_proximity.set_coefficients(&prox_coeffs);

        // Head shadow: far ear gets a high-shelf cut proportional to lateral angle.
        let mut left_shadow: Vec<BiquadCoeffs> = Vec::new();
        let mut right_shadow: Vec<BiquadCoeffs> = Vec::new();
        if self.config.enable_head_shadow {
            let amount = ear.lateral.abs();
            let cut_db = -self.config.max_head_shadow_db.max(0.0) * amount;
            if cut_db.abs() > 0.001 {
                let shelf = high_shelf(sr, self.config.head_shadow_shelf_hz, cut_db);
                if ear.lateral > 0.0 {
                    left_shadow.push(shelf);
                } else if ear.lateral < 0.0 {
                    right_shadow.push(shelf);
                }
            }
        }
        self.left_head_shadow.set_coefficients(&left_shadow);
        self.right_head_shadow.set_coefficients(&right_shadow);

        self.turbulence.update(self.config.turbulence, distance);

        self.debug = RendererDebugState {
            direction,
            ear_geometry: ear,
            distance_gain_amp: self.distance_gain_amp,
            phrtf_bands,
            proximity_gain_db: self.config.proximity.gain_db_at_distance(distance),
        };
    }

    /// Process one mono input sample into stereo output.
    #[inline]
    pub fn process_sample(&mut self, input: f32) -> (f32, f32) {
        let common = input * self.distance_gain_amp * self.turbulence.process_gain();

        let mut l = self.left_delay.process(common * self.left_gain_amp);
        let mut r = self.right_delay.process(common * self.right_gain_amp);

        l = self.left_proximity.process(l);
        r = self.right_proximity.process(r);

        l = self.left_phrtf.process(l);
        r = self.right_phrtf.process(r);

        l = self.left_air.process(l);
        r = self.right_air.process(r);

        l = self.left_head_shadow.process(l);
        r = self.right_head_shadow.process(r);

        (l, r)
    }

    /// Process a mono buffer into separate left/right slices.
    pub fn process_block(&mut self, input: &[f32], left: &mut [f32], right: &mut [f32]) {
        let n = input.len().min(left.len()).min(right.len());
        for i in 0..n {
            let (l, r) = self.process_sample(input[i]);
            left[i] = l;
            right[i] = r;
        }
    }

    /// Debug-only approximate magnitude response in dB for the left ear.
    /// Does not include ITD phase, scalar distance gain, or turbulence.
    pub fn left_filter_magnitude_db(&self, freq_hz: f32) -> f32 {
        let sr = self.config.sample_rate_hz;
        self.left_proximity.magnitude_db(sr, freq_hz)
            + self.left_phrtf.magnitude_db(sr, freq_hz)
            + self.left_air.magnitude_db(sr, freq_hz)
            + self.left_head_shadow.magnitude_db(sr, freq_hz)
    }

    /// Debug-only approximate magnitude response in dB for the right ear.
    pub fn right_filter_magnitude_db(&self, freq_hz: f32) -> f32 {
        let sr = self.config.sample_rate_hz;
        self.right_proximity.magnitude_db(sr, freq_hz)
            + self.right_phrtf.magnitude_db(sr, freq_hz)
            + self.right_air.magnitude_db(sr, freq_hz)
            + self.right_head_shadow.magnitude_db(sr, freq_hz)
    }

    /// Helper for UI smoothing: update direction gradually toward a target.
    pub fn smoothed_direction(current: Direction3D, target: Direction3D, alpha: f32) -> Direction3D {
        let a = clamp(alpha, 0.0, 1.0);
        Direction3D {
            azimuth_deg: current.azimuth_deg + (target.azimuth_deg - current.azimuth_deg) * a,
            elevation_deg: current.elevation_deg + (target.elevation_deg - current.elevation_deg) * a,
            distance_m: current.distance_m + (target.distance_m - current.distance_m) * a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of the per-ear filter magnitude over a few representative
    /// frequencies — a cheap proxy for "how differently are the two ears
    /// coloured?".
    fn ear_response_gap(r: &SpatialPhrtfRenderer) -> f32 {
        [2_000.0, 4_500.0, 8_000.0, 11_500.0]
            .iter()
            .map(|&f| (r.left_filter_magnitude_db(f) - r.right_filter_magnitude_db(f)).abs())
            .sum()
    }

    #[test]
    fn per_ear_phrtf_diverges_for_lateral_sources() {
        let mut r = SpatialPhrtfRenderer::new(RendererConfig::new(48_000.0));

        // Directly in front: the two ears should be coloured (almost) identically.
        r.update(Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 1.0 });
        let front_gap = ear_response_gap(&r);

        // Hard right: ipsilateral/contralateral pHRTF trajectories now differ,
        // so the spectral gap between the ears must be clearly larger.
        r.update(Direction3D { azimuth_deg: 90.0, elevation_deg: 0.0, distance_m: 1.0 });
        let side_gap = ear_response_gap(&r);

        assert!(
            side_gap > front_gap + 3.0,
            "expected lateral source to widen the inter-ear spectral gap: \
             front={front_gap:.2} dB, side={side_gap:.2} dB",
        );
    }

    #[test]
    fn zero_ear_offset_keeps_ears_matched() {
        let mut cfg = RendererConfig::new(48_000.0);
        cfg.phrtf_ear_offset_deg = 0.0;
        // Remove the broadband cues that are not part of the pHRTF cascade so
        // the two pHRTF chains are the only thing under test.
        cfg.enable_head_shadow = false;
        let mut r = SpatialPhrtfRenderer::new(cfg);
        r.update(Direction3D { azimuth_deg: 75.0, elevation_deg: 10.0, distance_m: 1.0 });
        // With no per-ear bias the pHRTF cascades are identical, so the only
        // surviving difference is the (disabled) head shadow → ~0 dB gap.
        assert!(
            ear_response_gap(&r) < 0.01,
            "ear_offset=0 should leave the pHRTF cascades identical",
        );
    }

    #[test]
    fn smoothing_converges_toward_target() {
        let start = Direction3D::front(1.0);
        let target = Direction3D { azimuth_deg: 90.0, elevation_deg: 30.0, distance_m: 3.0 };
        let mut cur = start;
        for _ in 0..256 {
            cur = SpatialPhrtfRenderer::smoothed_direction(cur, target, 0.2);
        }
        assert!((cur.azimuth_deg - target.azimuth_deg).abs() < 0.5);
        assert!((cur.elevation_deg - target.elevation_deg).abs() < 0.5);
        assert!((cur.distance_m - target.distance_m).abs() < 0.05);
    }
}
