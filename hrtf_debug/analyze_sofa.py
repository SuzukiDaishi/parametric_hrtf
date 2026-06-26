#!/usr/bin/env python3
"""Inspect local SOFA HRIR files and print coarse tuning cues.

This is a debug helper for the parametric renderer; it is not used by the
runtime. It expects `h5py`, `numpy`, and the local `hrtf_debug/sofa/*.sofa`
files.
"""

from __future__ import annotations

from pathlib import Path

import h5py
import numpy as np


SAMPLE_RATE = 48_000
NFFT = 4096
FREQ = np.fft.rfftfreq(NFFT, 1.0 / SAMPLE_RATE)
FEATURE_MASK = (FREQ >= 1_000.0) & (FREQ <= 18_000.0)


def nearest_position_index(source_positions: np.ndarray, azimuth_deg: float, elevation_deg: float) -> int:
    azimuth_deg %= 360.0
    az_delta = np.abs(((source_positions[:, 0] - azimuth_deg + 180.0) % 360.0) - 180.0)
    el_delta = np.abs(source_positions[:, 1] - elevation_deg)
    return int(np.argmin(az_delta + el_delta))


def smoothed_magnitude_db(hrir: np.ndarray, normalize: bool = True) -> np.ndarray:
    window = np.hanning(len(hrir))
    spectrum = np.fft.rfft(hrir * window, NFFT)
    mag = 20.0 * np.log10(np.abs(spectrum) + 1.0e-12)
    if normalize:
        mag -= np.mean(mag[FEATURE_MASK])

    out = mag.copy()
    log_freq = np.log2(np.maximum(FREQ, 20.0))
    width_oct = 1.0 / 6.0
    for i, freq_hz in enumerate(FREQ):
        if freq_hz < 500.0:
            continue
        band = np.abs(log_freq - log_freq[i]) < width_oct * 0.5
        out[i] = float(np.mean(mag[band]))
    return out


def extrema(mag: np.ndarray, low_hz: float, high_hz: float, kind: str) -> tuple[float, float]:
    band = (FREQ >= low_hz) & (FREQ <= high_hz)
    values = mag[band]
    freqs = FREQ[band]
    index = int(np.argmax(values) if kind == "max" else np.argmin(values))
    return float(freqs[index]), float(values[index])


def avg_band(mag: np.ndarray, low_hz: float, high_hz: float) -> float:
    band = (FREQ >= low_hz) & (FREQ <= high_hz)
    return float(np.mean(mag[band]))


def print_direction(name: str, pos: np.ndarray, mag: np.ndarray) -> None:
    probes = [2_000, 4_000, 6_000, 8_000, 10_000, 12_000]
    values = []
    for freq_hz in probes:
        index = int(np.argmin(np.abs(FREQ - freq_hz)))
        values.append(mag[index])

    print(f"{name:>10} pos={pos}")
    print(
        " " * 11
        + f"P1={extrema(mag, 3_000, 6_500, 'max')} "
        + f"P2={extrema(mag, 6_500, 10_000, 'max')} "
        + f"N1={extrema(mag, 6_000, 11_000, 'min')} "
        + f"N2={extrema(mag, 10_000, 16_000, 'min')}"
    )
    print(" " * 11 + "probe dB=" + ", ".join(f"{v:+.1f}" for v in values))


def analyze(path: Path) -> None:
    with h5py.File(path, "r") as sofa:
        positions = sofa["SourcePosition"][:]
        ir = sofa["Data.IR"][:]
        sample_rate = float(sofa["Data.SamplingRate"][0])

        print(f"\n== {path.name} ==")
        print(f"sample_rate={sample_rate:.0f} Hz, measurements={len(positions)}, taps={ir.shape[-1]}")

        cases = [
            ("front", 0.0, 0.0, None),
            ("up", 0.0, 90.0, None),
            ("rear", 180.0, 0.0, None),
            # SOFA spherical convention here is +90 left, +270 right.
            ("left_near", 90.0, 0.0, 0),
            ("right_near", 270.0, 0.0, 1),
        ]
        for name, azimuth, elevation, ear in cases:
            i = nearest_position_index(positions, azimuth, elevation)
            if ear is None:
                mag = 0.5 * (smoothed_magnitude_db(ir[i, 0]) + smoothed_magnitude_db(ir[i, 1]))
            else:
                mag = smoothed_magnitude_db(ir[i, ear])
            print_direction(name, positions[i], mag)

        right_i = nearest_position_index(positions, 270.0, 0.0)
        right_far = smoothed_magnitude_db(ir[right_i, 0], normalize=False)
        right_near = smoothed_magnitude_db(ir[right_i, 1], normalize=False)
        print(" right far-near dB:")
        for low, high in [(1_500, 4_000), (4_000, 8_000), (8_000, 14_000)]:
            print(f"   {low:5.0f}-{high:5.0f} Hz: {avg_band(right_far - right_near, low, high):+.2f}")


def main() -> None:
    for path in sorted((Path(__file__).resolve().parent / "sofa").glob("*.sofa")):
        analyze(path)


if __name__ == "__main__":
    main()
