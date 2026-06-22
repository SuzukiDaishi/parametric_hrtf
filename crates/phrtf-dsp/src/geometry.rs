//! Direction, geometric attenuation, ITD and ILD approximations.
//!
//! The spectral pHRTF model alone mainly covers median-plane elevation cues.
//! Horizontal localization needs binaural cues, so this module supplies simple:
//!
//! - ITD: interaural time difference using a Woodworth-like spherical-head model.
//! - ILD: small broadband far-ear gain reduction.
//! - Lateral factor used by the head-shadow high-shelf filter in the renderer.

use crate::math::{clamp, db_to_amp, deg_to_rad};

/// 3D source direction and distance relative to the listener.
///
/// Coordinate convention:
///
/// - azimuth_deg = 0: front
/// - azimuth_deg = +90: right
/// - azimuth_deg = -90: left
/// - azimuth_deg = ±180: rear
/// - elevation_deg = +90: up
/// - elevation_deg = -90: down
#[derive(Clone, Copy, Debug)]
pub struct Direction3D {
    pub azimuth_deg: f32,
    pub elevation_deg: f32,
    pub distance_m: f32,
}

impl Direction3D {
    pub fn front(distance_m: f32) -> Self {
        Self {
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
            distance_m,
        }
    }
}

/// Per-ear geometry values used by the renderer.
#[derive(Clone, Copy, Debug)]
pub struct EarGeometry {
    pub left_delay_s: f32,
    pub right_delay_s: f32,
    pub left_gain_amp: f32,
    pub right_gain_amp: f32,
    /// -1 means fully left, +1 means fully right.
    pub lateral: f32,
}

/// Inverse-distance pressure gain.
///
/// The acoustic intensity of a point source follows 1/r^2, while pressure
/// amplitude follows 1/r. Audio samples are pressure-like, so use r0/r.
pub fn inverse_distance_gain(
    distance_m: f32,
    reference_distance_m: f32,
    min_distance_m: f32,
    max_gain_db: f32,
) -> f32 {
    let d = distance_m.max(min_distance_m.max(0.001));
    let gain = reference_distance_m.max(0.001) / d;
    gain.min(db_to_amp(max_gain_db))
}

/// Computes simple stereo geometry cues.
///
/// This is not a full diffraction model. It gives enough horizontal information
/// for the parametric HRTF prototype to behave like a 3D panner.
pub fn compute_ear_geometry(
    direction: Direction3D,
    head_radius_m: f32,
    speed_of_sound_mps: f32,
    max_broadband_ild_db: f32,
) -> EarGeometry {
    let az = deg_to_rad(direction.azimuth_deg);
    let el = deg_to_rad(direction.elevation_deg);

    // Lateral component of a unit vector. +1 = right ear side.
    let lateral = clamp(az.sin() * el.cos(), -1.0, 1.0);
    let abs_lat = lateral.abs();

    // Woodworth-like model for total ITD at lateral angle theta.
    // At 90 degrees this gives roughly 0.6-0.7 ms for normal head radii.
    let theta = abs_lat.asin();
    let itd_s = (head_radius_m / speed_of_sound_mps) * (theta + theta.sin());

    let ild_db = max_broadband_ild_db.max(0.0) * abs_lat;
    if lateral > 0.0 {
        // Source is on the right: left ear is farther.
        EarGeometry {
            left_delay_s: itd_s,
            right_delay_s: 0.0,
            left_gain_amp: db_to_amp(-ild_db),
            right_gain_amp: 1.0,
            lateral,
        }
    } else {
        // Source is on the left: right ear is farther.
        EarGeometry {
            left_delay_s: 0.0,
            right_delay_s: itd_s,
            left_gain_amp: 1.0,
            right_gain_amp: db_to_amp(-ild_db),
            lateral,
        }
    }
}
