//! End-to-end tests against the public renderer API. These exercise the DSP
//! as a black box (the same surface the WebCLAP plugin drives), complementing
//! the in-module unit tests.

use phrtf_distance_proximity::{
    beta_from_direction, design_phrtf_bands, Direction3D, LowerHemisphereMode, RendererConfig,
    SpatialPhrtfRenderer,
};

const SR: f32 = 48_000.0;

fn sine(n: usize, freq: f32) -> Vec<f32> {
    (0..n)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Run a fixed direction to steady state and return (left_rms, right_rms).
fn settle(dir: Direction3D, cfg: RendererConfig) -> (f32, f32) {
    let mut r = SpatialPhrtfRenderer::new(cfg);
    r.update(dir);
    let input = sine(512, 1_000.0);
    let mut l = vec![0.0; input.len()];
    let mut rr = vec![0.0; input.len()];
    for _ in 0..40 {
        r.process_block(&input, &mut l, &mut rr);
    }
    (rms(&l), rms(&rr))
}

#[test]
fn lateral_panning_favours_the_near_ear() {
    let cfg = RendererConfig::new(SR);
    let (l, r) = settle(
        Direction3D { azimuth_deg: 75.0, elevation_deg: 0.0, distance_m: 1.5 },
        cfg,
    );
    assert!(r > l, "source on the right should be louder in the right ear (L={l}, R={r})");
}

#[test]
fn centred_source_is_symmetric() {
    let cfg = RendererConfig::new(SR);
    let (l, r) = settle(Direction3D::front(1.5), cfg);
    let diff = (l - r).abs() / (l + r).max(1e-6);
    assert!(diff < 0.02, "front source should be near-symmetric (L={l}, R={r})");
}

#[test]
fn beta_is_monotonic_front_to_rear() {
    let mode = LowerHemisphereMode::ClampToHorizon;
    let front = beta_from_direction(Direction3D::front(1.0), mode).0;
    let up = beta_from_direction(
        Direction3D { azimuth_deg: 0.0, elevation_deg: 90.0, distance_m: 1.0 },
        mode,
    )
    .0;
    let rear = beta_from_direction(
        Direction3D { azimuth_deg: 180.0, elevation_deg: 0.0, distance_m: 1.0 },
        mode,
    )
    .0;
    assert!(front < up && up < rear, "front={front}, up={up}, rear={rear}");
    assert!((front - 0.0).abs() < 1.0 && (rear - 180.0).abs() < 1.0);
}

#[test]
fn elevation_moves_the_n1_notch_upward() {
    let cfg = RendererConfig::new(SR);
    let front = design_phrtf_bands(
        cfg.phrtf_profile,
        cfg.phrtf_config,
        Direction3D::front(1.0),
    );
    let up = design_phrtf_bands(
        cfg.phrtf_profile,
        cfg.phrtf_config,
        Direction3D { azimuth_deg: 0.0, elevation_deg: 80.0, distance_m: 1.0 },
    );
    let n1 = |s: &phrtf_distance_proximity::PhrtfBandSet| {
        s.bands.iter().find(|b| b.name == "N1").unwrap().frequency_hz
    };
    assert!(n1(&up) > n1(&front), "N1 should rise with elevation: {} -> {}", n1(&front), n1(&up));
}

#[test]
fn renderer_output_is_always_finite() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    let input = sine(256, 3_000.0);
    let mut l = vec![0.0; input.len()];
    let mut rr = vec![0.0; input.len()];
    for az in (-180..=180).step_by(45) {
        for d in [0.05_f32, 0.5, 2.0, 20.0] {
            r.update(Direction3D { azimuth_deg: az as f32, elevation_deg: -20.0, distance_m: d });
            r.process_block(&input, &mut l, &mut rr);
            assert!(
                l.iter().chain(rr.iter()).all(|v| v.is_finite()),
                "non-finite at az={az}, d={d}",
            );
        }
    }
}

#[test]
fn silence_in_is_silence_out() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    r.update(Direction3D { azimuth_deg: 30.0, elevation_deg: 0.0, distance_m: 1.0 });
    let input = vec![0.0f32; 256];
    let mut l = vec![9.9; input.len()];
    let mut rr = vec![9.9; input.len()];
    r.process_block(&input, &mut l, &mut rr);
    assert!(rms(&l) < 1e-6 && rms(&rr) < 1e-6, "zero input must yield zero output");
}