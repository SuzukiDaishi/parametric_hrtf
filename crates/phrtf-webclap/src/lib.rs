//! Parametric HRTF binaural spatialiser, packaged as a real WCLAP audio
//! effect with a GUI.
//!
//! A thin [`wclap_plugin`] front end around
//! [`phrtf_distance_proximity::SpatialPhrtfRenderer`]. The renderer is a
//! mono→stereo spatialiser (per-ear pHRTF peak/notch cascade, ITD/ILD, head
//! shadow, distance gain, air absorption, proximity EQ); this crate exposes
//! its controls as `clap.params` and drives it from the stereo effect bus.
//!
//! Stereo input is summed to mono (the source signal), positioned in space by
//! the renderer, and written back to the stereo bus. A control-rate one-pole
//! smoother glides the position toward the target every block so dragging the
//! GUI pad never zippers.
//!
//! Sibling design to `z-audio-webclap-eq` in SuzukiDaishi/z-audio-dsp-plugin.

use wclap_plugin::{
    init_plugin, silence, ParamDef, Plugin, PluginDef, ProcessCtx, ProcessStatus, StereoIo,
    PARAM_IS_AUTOMATABLE, PARAM_IS_STEPPED,
};

use phrtf_distance_proximity::{Direction3D, RendererConfig, SpatialPhrtfRenderer};

static PLUGIN_DEF: PluginDef = PluginDef {
    id: b"dev.phrtf.parametric-hrtf\0",
    name: b"Parametric HRTF\0",
    vendor: b"suzukidaishi\0",
    url: b"https://github.com/SuzukiDaishi/parametric_hrtf\0",
    version: b"0.1.0\0",
    description: b"Parametric HRTF binaural spatialiser with per-ear pHRTF, ITD/ILD, head shadow, distance and proximity cues\0",
    features: &[b"audio-effect\0", b"spatial\0", b"surround\0"],
    audio_inputs: 1,
    audio_outputs: 1,
    note_inputs: 0,
    ui_path: Some(b"/ui/index.html\0"),
};

// --- Parameter ids -------------------------------------------------------
// Stable, contiguous ids; the GUI in `ui/main.js` mirrors these exactly.
const P_AZIMUTH: u32 = 0;
const P_ELEVATION: u32 = 1;
const P_DISTANCE: u32 = 2;
const P_HEAD_RADIUS: u32 = 3;
const P_NEAR_GAIN: u32 = 4;
const P_EN_ITD: u32 = 5;
const P_EN_ILD: u32 = 6;
const P_EN_HEAD_SHADOW: u32 = 7;
const P_EN_DISTANCE_GAIN: u32 = 8;
const P_EN_AIR: u32 = 9;
const P_EN_PROXIMITY: u32 = 10;
const P_EAR_OFFSET: u32 = 11;
const P_SPECTRAL: u32 = 12;
const P_N1_FRONT: u32 = 13;
const P_N2_FRONT: u32 = 14;
const P_P1: u32 = 15;
const P_P2: u32 = 16;

const AUTO: u32 = PARAM_IS_AUTOMATABLE;
const TOGGLE: u32 = PARAM_IS_AUTOMATABLE | PARAM_IS_STEPPED;

static PARAMS: &[ParamDef] = &[
    ParamDef { id: P_AZIMUTH, flags: AUTO, name: b"Azimuth\0", module: b"\0", min: -180.0, max: 180.0, default: 0.0 },
    ParamDef { id: P_ELEVATION, flags: AUTO, name: b"Elevation\0", module: b"\0", min: -90.0, max: 90.0, default: 0.0 },
    ParamDef { id: P_DISTANCE, flags: AUTO, name: b"Distance\0", module: b"\0", min: 0.05, max: 20.0, default: 1.0 },
    ParamDef { id: P_HEAD_RADIUS, flags: AUTO, name: b"Head Radius\0", module: b"\0", min: 0.06, max: 0.11, default: 0.0875 },
    ParamDef { id: P_NEAR_GAIN, flags: AUTO, name: b"Near Gain\0", module: b"\0", min: 0.0, max: 24.0, default: 18.0 },
    ParamDef { id: P_EN_ITD, flags: TOGGLE, name: b"ITD\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EN_ILD, flags: TOGGLE, name: b"ILD\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EN_HEAD_SHADOW, flags: TOGGLE, name: b"Head Shadow\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EN_DISTANCE_GAIN, flags: TOGGLE, name: b"Distance Gain\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EN_AIR, flags: TOGGLE, name: b"Air Absorption\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EN_PROXIMITY, flags: TOGGLE, name: b"Proximity\0", module: b"\0", min: 0.0, max: 1.0, default: 1.0 },
    ParamDef { id: P_EAR_OFFSET, flags: AUTO, name: b"Ear Offset\0", module: b"\0", min: 0.0, max: 90.0, default: 45.0 },
    ParamDef { id: P_SPECTRAL, flags: AUTO, name: b"Spectral Strength\0", module: b"\0", min: 0.0, max: 2.0, default: 1.0 },
    ParamDef { id: P_N1_FRONT, flags: AUTO, name: b"N1 Front\0", module: b"\0", min: 4000.0, max: 14000.0, default: 8000.0 },
    ParamDef { id: P_N2_FRONT, flags: AUTO, name: b"N2 Front\0", module: b"\0", min: 6000.0, max: 16000.0, default: 11500.0 },
    ParamDef { id: P_P1, flags: AUTO, name: b"P1 Peak\0", module: b"\0", min: 3000.0, max: 7000.0, default: 4500.0 },
    ParamDef { id: P_P2, flags: AUTO, name: b"P2 Peak\0", module: b"\0", min: 6000.0, max: 11000.0, default: 8500.0 },
];

/// Control-rate glide coefficient applied once per process block. At a 128
/// sample block / 48 kHz this is a ~13 ms time constant — fast enough to feel
/// instant, slow enough to kill zipper noise while dragging the GUI.
const SMOOTH_ALPHA: f32 = 0.15;

struct PhrtfPlugin {
    config: RendererConfig,
    renderer: SpatialPhrtfRenderer,
    /// Where the source is being asked to go (set from the GUI / automation).
    target: Direction3D,
    /// Where the source actually is right now (glides toward `target`).
    current: Direction3D,
    /// Pre-allocated mono scratch (the summed source signal).
    mono: Vec<f32>,
    /// Set when a non-positional param changed and the renderer needs a
    /// `set_config` before the next block.
    config_dirty: bool,
}

impl PhrtfPlugin {
    fn rebuild(&mut self) {
        self.renderer = SpatialPhrtfRenderer::new(self.config);
        self.renderer.update(self.current);
    }
}

impl Plugin for PhrtfPlugin {
    fn new() -> Self {
        let config = RendererConfig::new(48_000.0);
        let renderer = SpatialPhrtfRenderer::new(config);
        let start = Direction3D::front(config.reference_distance_m);
        Self {
            config,
            renderer,
            target: start,
            current: start,
            mono: Vec::new(),
            config_dirty: false,
        }
    }

    fn activate(&mut self, sample_rate: f64, max_frames: u32) {
        self.config.sample_rate_hz = sample_rate as f32;
        self.mono = vec![0.0; (max_frames as usize).max(1)];
        self.rebuild();
        self.config_dirty = false;
    }

    fn reset(&mut self) {
        self.mono.iter_mut().for_each(|s| *s = 0.0);
        self.current = self.target;
        self.rebuild();
    }

    fn params() -> &'static [ParamDef] {
        PARAMS
    }

    fn get_param(&self, id: u32) -> f64 {
        let c = &self.config;
        let v = match id {
            P_AZIMUTH => self.target.azimuth_deg,
            P_ELEVATION => self.target.elevation_deg,
            P_DISTANCE => self.target.distance_m,
            P_HEAD_RADIUS => c.head_radius_m,
            P_NEAR_GAIN => c.max_near_gain_db,
            P_EN_ITD => bool_f32(c.enable_itd),
            P_EN_ILD => bool_f32(c.enable_broadband_ild),
            P_EN_HEAD_SHADOW => bool_f32(c.enable_head_shadow),
            P_EN_DISTANCE_GAIN => bool_f32(c.enable_distance_gain),
            P_EN_AIR => bool_f32(c.air_absorption.enabled),
            P_EN_PROXIMITY => bool_f32(c.proximity.enabled),
            P_EAR_OFFSET => c.phrtf_ear_offset_deg,
            P_SPECTRAL => c.phrtf_config.spectral_strength,
            P_N1_FRONT => c.phrtf_profile.f_n1_front_hz,
            P_N2_FRONT => c.phrtf_profile.f_n2_front_hz,
            P_P1 => c.phrtf_profile.f_p1_hz,
            P_P2 => c.phrtf_profile.f_p2_hz,
            _ => 0.0,
        };
        v as f64
    }

    fn set_param(&mut self, id: u32, value: f64) {
        let v = value as f32;
        let on = value >= 0.5;
        match id {
            // Positional params only move the smoothing target — no rebuild.
            P_AZIMUTH => self.target.azimuth_deg = v.clamp(-180.0, 180.0),
            P_ELEVATION => self.target.elevation_deg = v.clamp(-90.0, 90.0),
            P_DISTANCE => self.target.distance_m = v.clamp(0.05, 20.0),
            // Everything else is config and needs a renderer rebuild next block.
            P_HEAD_RADIUS => self.set_cfg(|c| c.head_radius_m = v.clamp(0.06, 0.11)),
            P_NEAR_GAIN => self.set_cfg(|c| c.max_near_gain_db = v.clamp(0.0, 24.0)),
            P_EN_ITD => self.set_cfg(|c| c.enable_itd = on),
            P_EN_ILD => self.set_cfg(|c| c.enable_broadband_ild = on),
            P_EN_HEAD_SHADOW => self.set_cfg(|c| c.enable_head_shadow = on),
            P_EN_DISTANCE_GAIN => self.set_cfg(|c| c.enable_distance_gain = on),
            P_EN_AIR => self.set_cfg(|c| c.air_absorption.enabled = on),
            P_EN_PROXIMITY => self.set_cfg(|c| c.proximity.enabled = on),
            P_EAR_OFFSET => self.set_cfg(|c| c.phrtf_ear_offset_deg = v.clamp(0.0, 90.0)),
            P_SPECTRAL => self.set_cfg(|c| c.phrtf_config.spectral_strength = v.clamp(0.0, 2.0)),
            P_N1_FRONT => self.set_cfg(|c| c.phrtf_profile.f_n1_front_hz = v.clamp(4000.0, 14000.0)),
            P_N2_FRONT => self.set_cfg(|c| c.phrtf_profile.f_n2_front_hz = v.clamp(6000.0, 16000.0)),
            P_P1 => self.set_cfg(|c| c.phrtf_profile.f_p1_hz = v.clamp(3000.0, 7000.0)),
            P_P2 => self.set_cfg(|c| c.phrtf_profile.f_p2_hz = v.clamp(6000.0, 11000.0)),
            _ => {}
        }
    }

    fn process(&mut self, ctx: &mut ProcessCtx) -> ProcessStatus {
        let frames = ctx.frames();
        if frames == 0 {
            return ProcessStatus::Continue;
        }

        // Control-rate glide + (lazy) config rebuild, then refresh the filters.
        self.tick();

        match ctx.stereo_io() {
            Some(StereoIo { input_l, input_r, output_l, output_r }) => {
                let n = frames.min(output_l.len()).min(output_r.len());
                if self.mono.len() < n {
                    self.mono.resize(n, 0.0);
                }
                for i in 0..n {
                    self.mono[i] = 0.5 * (input_l[i] + input_r[i]);
                }
                self.renderer
                    .process_block(&self.mono[..n], &mut output_l[..n], &mut output_r[..n]);
            }
            None => silence(ctx),
        }

        ProcessStatus::Continue
    }
}

impl PhrtfPlugin {
    /// Advance the control-rate state one block: glide the position toward its
    /// target, apply any pending config change, and refresh the renderer's
    /// filters. Shared by `process()` and the tests (which can't build a real
    /// `ProcessCtx` on the host).
    fn tick(&mut self) {
        self.current =
            SpatialPhrtfRenderer::smoothed_direction(self.current, self.target, SMOOTH_ALPHA);
        if self.config_dirty {
            self.renderer.set_config(self.config);
            self.config_dirty = false;
        }
        self.renderer.update(self.current);
    }

    /// Spatialise a mono block to stereo, exactly as `process()` does once the
    /// stereo bus has been summed to mono. Test-only.
    #[cfg(test)]
    fn render_block(&mut self, mono: &[f32], left: &mut [f32], right: &mut [f32]) {
        self.tick();
        self.renderer.process_block(mono, left, right);
    }

    /// Apply a mutation to the cached config and flag a rebuild for next block.
    #[inline]
    fn set_cfg(&mut self, f: impl FnOnce(&mut RendererConfig)) {
        f(&mut self.config);
        self.config_dirty = true;
    }
}

#[inline]
fn bool_f32(b: bool) -> f32 {
    if b {
        1.0
    } else {
        0.0
    }
}

#[no_mangle]
pub extern "C" fn _initialize() {
    init_plugin::<PhrtfPlugin>(&PLUGIN_DEF);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_contiguous_and_unique() {
        for (i, p) in PARAMS.iter().enumerate() {
            assert_eq!(p.id, i as u32, "param ids must be contiguous from 0");
        }
        assert_eq!(PARAMS.len(), 17);
    }

    #[test]
    fn positional_params_round_trip() {
        let mut p = PhrtfPlugin::new();
        p.set_param(P_AZIMUTH, 90.0);
        p.set_param(P_ELEVATION, -30.0);
        p.set_param(P_DISTANCE, 4.0);
        assert_eq!(p.get_param(P_AZIMUTH), 90.0);
        assert_eq!(p.get_param(P_ELEVATION), -30.0);
        assert_eq!(p.get_param(P_DISTANCE), 4.0);
    }

    #[test]
    fn config_params_round_trip_and_clamp() {
        let mut p = PhrtfPlugin::new();
        p.set_param(P_EN_PROXIMITY, 0.0);
        p.set_param(P_EAR_OFFSET, 999.0); // clamps to 90
        p.set_param(P_N1_FRONT, 9000.0);
        assert_eq!(p.get_param(P_EN_PROXIMITY), 0.0);
        assert_eq!(p.get_param(P_EAR_OFFSET), 90.0);
        assert_eq!(p.get_param(P_N1_FRONT), 9000.0);
        assert!(p.config_dirty);
    }

    #[test]
    fn every_param_default_is_in_range() {
        for d in PARAMS {
            assert!(
                d.default >= d.min && d.default <= d.max,
                "param {} default {} out of [{}, {}]",
                d.id,
                d.default,
                d.min,
                d.max,
            );
            assert!(d.min < d.max, "param {} has empty range", d.id);
        }
    }

    /// Drive the plugin to steady state at one target and return per-ear RMS.
    fn settle_rms(p: &mut PhrtfPlugin, input: &[f32]) -> (f32, f32) {
        let mut l = vec![0.0f32; input.len()];
        let mut r = vec![0.0f32; input.len()];
        // Let the position smoother and filters reach the target.
        for _ in 0..400 {
            p.render_block(input, &mut l, &mut r);
        }
        (rms(&l), rms(&r))
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn sine(n: usize, freq: f32, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
            .collect()
    }

    #[test]
    fn right_source_is_louder_in_right_ear() {
        let mut p = PhrtfPlugin::new();
        p.activate(48_000.0, 256);
        let input = sine(256, 1_000.0, 48_000.0);

        p.set_param(P_AZIMUTH, 90.0);
        let (left, right) = settle_rms(&mut p, &input);
        assert!(right > left * 1.05, "right ear should dominate: L={left}, R={right}");

        // Mirror to the left and the asymmetry must flip.
        p.set_param(P_AZIMUTH, -90.0);
        let (left2, right2) = settle_rms(&mut p, &input);
        assert!(left2 > right2 * 1.05, "left ear should dominate: L={left2}, R={right2}");
    }

    #[test]
    fn distance_gain_attenuates_far_sources() {
        let mut p = PhrtfPlugin::new();
        p.activate(48_000.0, 256);
        let input = sine(256, 500.0, 48_000.0);

        p.set_param(P_DISTANCE, 1.0);
        let near = settle_rms(&mut p, &input);
        p.set_param(P_DISTANCE, 8.0);
        let far = settle_rms(&mut p, &input);

        let near_e = near.0 + near.1;
        let far_e = far.0 + far.1;
        assert!(far_e < near_e * 0.6, "far source must be quieter: near={near_e}, far={far_e}");
    }

    #[test]
    fn output_is_finite_across_the_sphere() {
        let mut p = PhrtfPlugin::new();
        p.activate(48_000.0, 128);
        let input = sine(128, 2_000.0, 48_000.0);
        let mut l = vec![0.0f32; 128];
        let mut r = vec![0.0f32; 128];
        for az in (-180..=180).step_by(30) {
            for el in (-90..=90).step_by(45) {
                p.set_param(P_AZIMUTH, az as f64);
                p.set_param(P_ELEVATION, el as f64);
                p.set_param(P_DISTANCE, 0.1);
                for _ in 0..8 {
                    p.render_block(&input, &mut l, &mut r);
                }
                assert!(
                    l.iter().chain(r.iter()).all(|v| v.is_finite()),
                    "non-finite output at az={az}, el={el}",
                );
            }
        }
    }

    #[test]
    fn position_smoothing_is_not_instant() {
        let mut p = PhrtfPlugin::new();
        p.activate(48_000.0, 64);
        p.set_param(P_AZIMUTH, 90.0);
        // After a single block the smoothed position must be partway, not snapped.
        p.tick();
        assert!(
            p.current.azimuth_deg > 1.0 && p.current.azimuth_deg < 89.0,
            "one block should glide partway, got {}",
            p.current.azimuth_deg,
        );
    }
}
