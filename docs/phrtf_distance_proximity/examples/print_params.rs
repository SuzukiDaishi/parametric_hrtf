//! Print generated pHRTF/distance parameters for a few directions.
//!
//! Run:
//! ```bash
//! cargo run --example print_params
//! ```

use phrtf_distance_proximity::{Direction3D, RendererConfig, SpatialPhrtfRenderer};

fn main() {
    let config = RendererConfig::new(48_000.0);
    let mut renderer = SpatialPhrtfRenderer::new(config);

    let cases = [
        ("front 1m", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 1.0 }),
        ("up 1m", Direction3D { azimuth_deg: 0.0, elevation_deg: 90.0, distance_m: 1.0 }),
        ("rear 1m", Direction3D { azimuth_deg: 180.0, elevation_deg: 0.0, distance_m: 1.0 }),
        ("right 3m", Direction3D { azimuth_deg: 90.0, elevation_deg: 0.0, distance_m: 3.0 }),
        ("front close 8cm", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 0.08 }),
        ("front far 50m", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 50.0 }),
    ];

    for (name, dir) in cases {
        renderer.update(dir);
        let dbg = renderer.debug_state();
        println!("=== {name} ===");
        println!("direction: az={:.1} el={:.1} d={:.2}m", dir.azimuth_deg, dir.elevation_deg, dir.distance_m);
        println!("distance_gain_amp: {:.4}", dbg.distance_gain_amp);
        println!("ITD delays: L={:.3} ms, R={:.3} ms, lateral={:.2}",
            dbg.ear_geometry.left_delay_s * 1000.0,
            dbg.ear_geometry.right_delay_s * 1000.0,
            dbg.ear_geometry.lateral);
        println!("pHRTF beta={:.1} deg, strength={:.2}", dbg.phrtf_bands.beta_deg, dbg.phrtf_bands.vertical_strength);
        println!("proximity low-shelf boost: {:.2} dB", dbg.proximity_gain_db);
        for b in &dbg.phrtf_bands.bands {
            println!("  {:>2}: f={:8.1} Hz, gain={:6.2} dB, Q={:4.2}", b.name, b.frequency_hz, b.gain_db, b.q);
        }
        println!();
    }
}
