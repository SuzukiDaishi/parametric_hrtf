//! Export approximate left/right magnitude responses to CSV.
//!
//! This is intentionally simple: no external plotting dependencies. Open the CSV
//! in Python, Excel, or your favorite plotting tool.
//!
//! Run:
//! ```bash
//! cargo run --example export_response_csv
//! ```

use std::fs::File;
use std::io::{BufWriter, Write};

use phrtf_distance_proximity::{Direction3D, RendererConfig, SpatialPhrtfRenderer};

fn main() -> std::io::Result<()> {
    let config = RendererConfig::new(48_000.0);
    let mut renderer = SpatialPhrtfRenderer::new(config);

    let cases = [
        ("front_1m", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 1.0 }),
        ("up_1m", Direction3D { azimuth_deg: 0.0, elevation_deg: 90.0, distance_m: 1.0 }),
        ("right_3m", Direction3D { azimuth_deg: 90.0, elevation_deg: 0.0, distance_m: 3.0 }),
        ("front_50m", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 50.0 }),
        ("close_8cm", Direction3D { azimuth_deg: 0.0, elevation_deg: 0.0, distance_m: 0.08 }),
    ];

    for (name, dir) in cases {
        renderer.update(dir);
        let path = format!("response_{name}.csv");
        let mut w = BufWriter::new(File::create(&path)?);
        writeln!(w, "freq_hz,left_db,right_db")?;

        // Log-ish frequency grid.
        for i in 0..240 {
            let t = i as f32 / 239.0;
            let freq = 20.0_f32 * (20_000.0_f32 / 20.0_f32).powf(t);
            writeln!(
                w,
                "{:.3},{:.6},{:.6}",
                freq,
                renderer.left_filter_magnitude_db(freq),
                renderer.right_filter_magnitude_db(freq)
            )?;
        }
        println!("wrote {path}");
    }

    Ok(())
}
