//! Process a synthetic sine sweep-ish signal and print the first few stereo samples.
//!
//! This avoids external WAV dependencies. In your project, feed your decoded mono
//! samples into `process_block()` and write/use the stereo result.

use phrtf_distance_proximity::{Direction3D, RendererConfig, SpatialPhrtfRenderer};

fn main() {
    let sample_rate = 48_000.0;
    let mut renderer = SpatialPhrtfRenderer::new(RendererConfig::new(sample_rate));
    renderer.update(Direction3D {
        azimuth_deg: 70.0,
        elevation_deg: 30.0,
        distance_m: 2.0,
    });

    let seconds = 0.02;
    let n = (sample_rate * seconds) as usize;
    let mut mono = vec![0.0_f32; n];
    for i in 0..n {
        let t = i as f32 / sample_rate;
        // A small multi-tone signal to excite several parts of the EQ.
        mono[i] = 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 4_000.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 9_000.0 * t).sin();
    }

    let mut left = vec![0.0_f32; n];
    let mut right = vec![0.0_f32; n];
    renderer.process_block(&mono, &mut left, &mut right);

    println!("first 32 stereo samples:");
    for i in 0..32.min(n) {
        println!("{:04}: L={:+.6}, R={:+.6}", i, left[i], right[i]);
    }
}
