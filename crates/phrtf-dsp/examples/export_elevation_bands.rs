//! Export the median-plane pHRTF band trajectory used by the default profile.
//!
//! Run:
//! ```bash
//! cargo run -p phrtf_distance_proximity --example export_elevation_bands
//! ```

use phrtf_distance_proximity::{design_phrtf_bands_beta, RendererConfig};

fn main() {
    let config = RendererConfig::new(48_000.0);
    println!("beta_deg,band,freq_hz,gain_db,q");
    for beta in [0.0_f32, 30.0, 60.0, 90.0, 120.0, 150.0, 180.0] {
        let bands = design_phrtf_bands_beta(config.phrtf_profile, config.phrtf_config, beta, 1.0);
        for band in bands.bands {
            println!(
                "{:.0},{},{:.3},{:.3},{:.3}",
                beta, band.name, band.frequency_hz, band.gain_db, band.q
            );
        }
    }
}
