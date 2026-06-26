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

fn max_abs(x: &[f32]) -> f32 {
    x.iter().fold(0.0_f32, |peak, v| peak.max(v.abs()))
}

fn avg_magnitude_db(r: &SpatialPhrtfRenderer, freqs: &[f32], left: bool) -> f32 {
    freqs
        .iter()
        .map(|&f| {
            if left {
                r.left_filter_magnitude_db(f)
            } else {
                r.right_filter_magnitude_db(f)
            }
        })
        .sum::<f32>()
        / freqs.len() as f32
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
        Direction3D {
            azimuth_deg: 75.0,
            elevation_deg: 0.0,
            distance_m: 1.5,
        },
        cfg,
    );
    assert!(
        r > l,
        "source on the right should be louder in the right ear (L={l}, R={r})"
    );
}

#[test]
fn centred_source_is_symmetric() {
    let cfg = RendererConfig::new(SR);
    let (l, r) = settle(Direction3D::front(1.5), cfg);
    let diff = (l - r).abs() / (l + r).max(1e-6);
    assert!(
        diff < 0.02,
        "front source should be near-symmetric (L={l}, R={r})"
    );
}

#[test]
fn beta_is_monotonic_front_to_rear() {
    let mode = LowerHemisphereMode::ClampToHorizon;
    let front = beta_from_direction(Direction3D::front(1.0), mode).0;
    let up = beta_from_direction(
        Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: 90.0,
            distance_m: 1.0,
        },
        mode,
    )
    .0;
    let rear = beta_from_direction(
        Direction3D {
            azimuth_deg: 180.0,
            elevation_deg: 0.0,
            distance_m: 1.0,
        },
        mode,
    )
    .0;
    assert!(
        front < up && up < rear,
        "front={front}, up={up}, rear={rear}"
    );
    assert!((front - 0.0).abs() < 1.0 && (rear - 180.0).abs() < 1.0);
}

#[test]
fn lower_hemisphere_strength_is_continuous_at_zero_elevation() {
    let mode = LowerHemisphereMode::MirrorWithReducedStrength { strength: 0.35 };
    let below = beta_from_direction(
        Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: -0.1,
            distance_m: 1.0,
        },
        mode,
    )
    .1;
    let horizon = beta_from_direction(Direction3D::front(1.0), mode).1;
    let above = beta_from_direction(
        Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: 0.1,
            distance_m: 1.0,
        },
        mode,
    )
    .1;

    assert!(
        (below - horizon).abs() < 0.01,
        "lower-hemisphere strength should not jump just below 0 deg: below={below}, horizon={horizon}",
    );
    assert!(
        (above - horizon).abs() < 0.01,
        "upper-hemisphere strength should remain continuous just above 0 deg: above={above}, horizon={horizon}",
    );
}

#[test]
fn elevation_moves_the_n1_notch_upward() {
    let cfg = RendererConfig::new(SR);
    let front = design_phrtf_bands(cfg.phrtf_profile, cfg.phrtf_config, Direction3D::front(1.0));
    let up = design_phrtf_bands(
        cfg.phrtf_profile,
        cfg.phrtf_config,
        Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: 80.0,
            distance_m: 1.0,
        },
    );
    let n1 = |s: &phrtf_distance_proximity::PhrtfBandSet| {
        s.bands
            .iter()
            .find(|b| b.name == "N1")
            .unwrap()
            .frequency_hz
    };
    assert!(
        n1(&up) > n1(&front),
        "N1 should rise with elevation: {} -> {}",
        n1(&front),
        n1(&up)
    );
}

#[test]
fn zenith_notches_are_shallower_than_front() {
    let cfg = RendererConfig::new(SR);
    let front = design_phrtf_bands(cfg.phrtf_profile, cfg.phrtf_config, Direction3D::front(1.0));
    let up = design_phrtf_bands(
        cfg.phrtf_profile,
        cfg.phrtf_config,
        Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: 90.0,
            distance_m: 1.0,
        },
    );
    let gain = |s: &phrtf_distance_proximity::PhrtfBandSet, name: &str| {
        s.bands.iter().find(|b| b.name == name).unwrap().gain_db
    };

    assert!(
        gain(&up, "N1") > gain(&front, "N1") + 4.0,
        "SOFA-calibrated zenith N1 should be shallower than front: front={:.2} dB, up={:.2} dB",
        gain(&front, "N1"),
        gain(&up, "N1")
    );
    assert!(
        gain(&up, "N2") > gain(&front, "N2") + 1.0,
        "SOFA-calibrated zenith N2 should be shallower than front: front={:.2} dB, up={:.2} dB",
        gain(&front, "N2"),
        gain(&up, "N2")
    );
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
            r.update(Direction3D {
                azimuth_deg: az as f32,
                elevation_deg: -20.0,
                distance_m: d,
            });
            r.process_block(&input, &mut l, &mut rr);
            assert!(
                l.iter().chain(rr.iter()).all(|v| v.is_finite()),
                "non-finite at az={az}, d={d}",
            );
        }
    }
}

#[test]
fn closest_default_distance_gain_is_unity() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);

    r.update(Direction3D {
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        distance_m: 0.05,
    });
    let close_gain = r.debug_state().distance_gain_amp;

    r.update(Direction3D {
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        distance_m: 2.0,
    });
    let far_gain = r.debug_state().distance_gain_amp;

    assert!(
        (close_gain - 1.0).abs() < 1e-6,
        "closest default gain should be 0 dB"
    );
    assert!(
        far_gain < close_gain,
        "farther sources should still attenuate"
    );
}

#[test]
fn near_field_full_scale_input_stays_below_output_ceiling() {
    let mut cfg = RendererConfig::new(SR);
    cfg.max_near_gain_db = 24.0;
    let mut r = SpatialPhrtfRenderer::new(cfg);
    r.update(Direction3D {
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        distance_m: 0.05,
    });

    let input = sine(512, 1_000.0);
    let mut l = vec![0.0; input.len()];
    let mut rr = vec![0.0; input.len()];
    for _ in 0..24 {
        r.process_block(&input, &mut l, &mut rr);
    }

    let peak = max_abs(&l).max(max_abs(&rr));
    assert!(
        peak <= 0.981,
        "near-field safety limiter should keep output below full scale; peak={peak}",
    );
}

#[test]
fn beta_is_continuous_across_the_horizon_sweep() {
    // Sweeping the azimuth around the full horizontal circle must not produce a
    // jump in the pHRTF `beta` coordinate. The previous median-plane projection
    // snapped `beta` from 0° to 180° exactly at ±90°; the angle-from-front
    // definition changes by ~1° per 1° of azimuth instead.
    let mode = LowerHemisphereMode::MirrorWithReducedStrength { strength: 0.35 };
    let mut prev = beta_from_direction(
        Direction3D {
            azimuth_deg: -180.0,
            elevation_deg: 0.0,
            distance_m: 1.0,
        },
        mode,
    )
    .0;
    let mut max_step = 0.0_f32;
    let mut az = -179;
    while az <= 180 {
        let beta = beta_from_direction(
            Direction3D {
                azimuth_deg: az as f32,
                elevation_deg: 0.0,
                distance_m: 1.0,
            },
            mode,
        )
        .0;
        max_step = max_step.max((beta - prev).abs());
        prev = beta;
        az += 1;
    }
    assert!(
        max_step < 2.0,
        "beta must vary smoothly with azimuth (no ±90° snap); max step was {max_step:.2}°",
    );
}

#[test]
fn per_ear_response_is_continuous_across_azimuth() {
    // End-to-end guard for the user-reported artefact: the per-ear magnitude
    // response must not jump at the ±90° / 180° crossings. We sweep the source
    // 1° at a time around the horizon and bound the change between neighbours.
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    let probe = [2_000.0_f32, 4_500.0, 8_000.0, 11_500.0];

    let response = |r: &SpatialPhrtfRenderer| -> Vec<f32> {
        probe
            .iter()
            .flat_map(|&f| {
                [
                    r.left_filter_magnitude_db(f),
                    r.right_filter_magnitude_db(f),
                ]
            })
            .collect()
    };

    r.update(Direction3D {
        azimuth_deg: -180.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    });
    let mut prev = response(&r);
    let mut worst = 0.0_f32;
    let mut worst_az = -180;
    let mut az = -179;
    while az <= 180 {
        r.update(Direction3D {
            azimuth_deg: az as f32,
            elevation_deg: 0.0,
            distance_m: 1.0,
        });
        let cur = response(&r);
        for (a, b) in prev.iter().zip(cur.iter()) {
            let d = (a - b).abs();
            if d > worst {
                worst = d;
                worst_az = az;
            }
        }
        prev = cur;
        az += 1;
    }
    // 1° of motion should never move any per-ear band by more than a fraction of
    // a dB. The pre-fix renderer jumped by tens of dB at ±90° as the notches
    // teleported between their front and rear frequencies.
    assert!(
        worst < 1.5,
        "per-ear response jumped {worst:.2} dB between adjacent azimuths near az={worst_az}°",
    );
}

#[test]
fn response_is_continuous_across_zero_elevation() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    let probe = [2_000.0_f32, 4_200.0, 8_200.0, 11_300.0, 14_700.0];

    let response = |r: &SpatialPhrtfRenderer| -> Vec<f32> {
        probe
            .iter()
            .flat_map(|&f| {
                [
                    r.left_filter_magnitude_db(f),
                    r.right_filter_magnitude_db(f),
                ]
            })
            .collect()
    };

    r.update(Direction3D {
        azimuth_deg: 0.0,
        elevation_deg: -1.0,
        distance_m: 1.0,
    });
    let mut prev = response(&r);
    let mut worst = 0.0_f32;
    let mut el = -0.9_f32;
    while el <= 1.0 {
        r.update(Direction3D {
            azimuth_deg: 0.0,
            elevation_deg: el,
            distance_m: 1.0,
        });
        let cur = response(&r);
        for (a, b) in prev.iter().zip(cur.iter()) {
            worst = worst.max((a - b).abs());
        }
        prev = cur;
        el += 0.1;
    }

    assert!(
        worst < 0.5,
        "response should not pop when elevation crosses 0 deg; worst adjacent step was {worst:.2} dB",
    );
}

#[test]
fn lateral_response_keeps_near_ear_clear_and_far_ear_shadowed() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    let freqs = [2_000.0, 4_000.0, 6_000.0, 8_000.0, 10_000.0, 12_000.0];

    r.update(Direction3D {
        azimuth_deg: 0.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    });
    let front_presence =
        0.5 * (avg_magnitude_db(&r, &freqs, true) + avg_magnitude_db(&r, &freqs, false));

    r.update(Direction3D {
        azimuth_deg: 90.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    });
    let near_ear_presence = avg_magnitude_db(&r, &freqs, false);
    let far_ear_presence = avg_magnitude_db(&r, &freqs, true);
    let shadow_drop = near_ear_presence - far_ear_presence;

    assert!(
        near_ear_presence > front_presence - 2.0,
        "side near-ear pHRTF should not sound muffled: front={front_presence:.2} dB, near={near_ear_presence:.2} dB",
    );
    assert!(
        r.right_filter_magnitude_db(6_000.0) > r.right_filter_magnitude_db(4_000.0) + 0.5,
        "side near-ear response should keep the SOFA-like 6 kHz pinna peak",
    );
    assert!(
        (6.0..=14.0).contains(&shadow_drop),
        "SOFA-calibrated side response should shadow the far ear without collapsing it: near={near_ear_presence:.2} dB, far={far_ear_presence:.2} dB",
    );
}

#[test]
fn rear_crossing_glides_through_the_back_not_the_front() {
    // A target that wraps from +170° to -170° is the same short move across the
    // back. The smoother must take the shortest arc (through ±180°), never swing
    // forward through 0°.
    let mut cur = Direction3D {
        azimuth_deg: 170.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    };
    let target = Direction3D {
        azimuth_deg: -170.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    };
    for _ in 0..256 {
        cur = SpatialPhrtfRenderer::smoothed_direction(cur, target, 0.2);
        // The frontal direction cosine must stay negative (rear hemisphere) the
        // whole way — i.e. |azimuth| stays > 90°.
        assert!(
            cur.azimuth_deg.abs() > 90.0,
            "rear crossing strayed to the front at az={}",
            cur.azimuth_deg,
        );
    }
    assert!(
        (cur.azimuth_deg - (-170.0)).abs() < 1.0,
        "should settle at the target"
    );
}

#[test]
fn silence_in_is_silence_out() {
    let cfg = RendererConfig::new(SR);
    let mut r = SpatialPhrtfRenderer::new(cfg);
    r.update(Direction3D {
        azimuth_deg: 30.0,
        elevation_deg: 0.0,
        distance_m: 1.0,
    });
    let input = vec![0.0f32; 256];
    let mut l = vec![9.9; input.len()];
    let mut rr = vec![9.9; input.len()];
    r.process_block(&input, &mut l, &mut rr);
    assert!(
        rms(&l) < 1e-6 && rms(&rr) < 1e-6,
        "zero input must yield zero output"
    );
}
