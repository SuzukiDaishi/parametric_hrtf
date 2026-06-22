//! Small math helpers used across the renderer.

/// Converts decibels to linear amplitude.
#[inline]
pub fn db_to_amp(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts linear amplitude to decibels.
#[inline]
pub fn amp_to_db(amp: f32) -> f32 {
    20.0 * amp.max(1.0e-20).log10()
}

/// Clamp helper for old Rust toolchains where `f32::clamp` may be unavailable.
#[inline]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn deg_to_rad(deg: f32) -> f32 {
    deg * std::f32::consts::PI / 180.0
}

#[inline]
pub fn rad_to_deg(rad: f32) -> f32 {
    rad * 180.0 / std::f32::consts::PI
}

/// Cubic smoothstep from 0 to 1.
/// Useful when changing gains with distance because it avoids abrupt slope changes.
#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear interpolation.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Wrap an angle in degrees into the range `(-180, 180]`.
///
/// Azimuth is circular: a source just past the rear at `+179°` and one at
/// `-179°` are nearly the same direction. Code that interpolates or differences
/// azimuth must do so along the shortest arc, otherwise crossing the back
/// (`±180°` wrap) makes the value swing all the way around the front. Use this
/// to fold a difference or a running position back onto the circle.
#[inline]
pub fn wrap_180(deg: f32) -> f32 {
    let wrapped = (deg + 180.0).rem_euclid(360.0) - 180.0;
    // `rem_euclid` maps an exact +180 onto -180; keep it at +180 so a source
    // held directly behind the listener has a stable representation.
    if wrapped <= -180.0 {
        180.0
    } else {
        wrapped
    }
}
