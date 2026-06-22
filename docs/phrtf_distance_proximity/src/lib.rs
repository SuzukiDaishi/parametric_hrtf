//! # phrtf_distance_proximity
//!
//! A small, dependency-free Rust prototype for a **parametric HRTF renderer** with:
//!
//! - Iida-style median-plane pHRTF spectral cues: P1, P2, N1, N2, P0.
//! - Stereo expansion using simple ITD / ILD / head-shadow approximations.
//! - Distance gain using inverse-distance pressure attenuation.
//! - Frequency-dependent air absorption using ISO 9613-1 style equations.
//! - A real-time friendly parametric-EQ approximation of air absorption.
//! - Optional microphone-style proximity effect using low-shelf EQ.
//! - Optional slow stochastic air turbulence / scintillation gain flutter.
//!
//! ## Important scope note
//!
//! This crate is designed as an implementation-oriented research prototype.
//! The pHRTF part is based on the median-plane peak/notch idea: measured HRTF
//! amplitude spectra are approximated by a small set of spectral peaks and notches
//! described by center frequency, level, and sharpness/Q.
//!
//! It is **not** a replacement for a measured full-sphere HRTF database. The side,
//! lower-hemisphere, and near-field HRTF behavior are approximations. The code is
//! intentionally written with many comments so that you can replace the defaults
//! with your own measured PNP/pHRTF parameters later.

pub mod air_absorption;
pub mod biquad;
pub mod delay;
pub mod geometry;
pub mod math;
pub mod noise;
pub mod phrtf;
pub mod proximity;
pub mod renderer;

pub use air_absorption::{Atmosphere, AirAbsorptionEqConfig};
pub use geometry::{Direction3D, EarGeometry};
pub use phrtf::{LowerHemisphereMode, NotchTrajectoryMode, PhrtfConfig, PhrtfProfile};
pub use proximity::{MicrophonePattern, ProximityConfig};
pub use renderer::{RendererConfig, SpatialPhrtfRenderer};
