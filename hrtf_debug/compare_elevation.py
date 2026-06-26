#!/usr/bin/env python3
"""Compare local SOFA median-plane cues with the Rust pHRTF trajectory."""

from __future__ import annotations

import csv
import subprocess
from io import StringIO
from pathlib import Path

import h5py
import numpy as np


ROOT = Path(__file__).resolve().parents[1]
SOFA_DIR = Path(__file__).resolve().parent / "sofa"
SAMPLE_RATE = 48_000
NFFT = 4096
FREQ = np.fft.rfftfreq(NFFT, 1.0 / SAMPLE_RATE)
FEATURE_MASK = (FREQ >= 1_000.0) & (FREQ <= 18_000.0)
BETA_DEG = [0, 30, 60, 90, 120, 150, 180]


def nearest_position_index(source_positions: np.ndarray, azimuth_deg: float, elevation_deg: float) -> int:
    azimuth_deg %= 360.0
    az_delta = np.abs(((source_positions[:, 0] - azimuth_deg + 180.0) % 360.0) - 180.0)
    el_delta = np.abs(source_positions[:, 1] - elevation_deg)
    return int(np.argmin(az_delta + el_delta))


def smoothed_magnitude_db(hrir: np.ndarray) -> np.ndarray:
    window = np.hanning(len(hrir))
    spectrum = np.fft.rfft(hrir * window, NFFT)
    mag = 20.0 * np.log10(np.abs(spectrum) + 1.0e-12)
    mag -= np.mean(mag[FEATURE_MASK])

    out = mag.copy()
    log_freq = np.log2(np.maximum(FREQ, 20.0))
    width_oct = 1.0 / 6.0
    for i, freq_hz in enumerate(FREQ):
        if freq_hz >= 500.0:
            band = np.abs(log_freq - log_freq[i]) < width_oct * 0.5
            out[i] = float(np.mean(mag[band]))
    return out


def extrema(mag: np.ndarray, low_hz: float, high_hz: float, kind: str) -> tuple[float, float]:
    band = (FREQ >= low_hz) & (FREQ <= high_hz)
    values = mag[band]
    freqs = FREQ[band]
    index = int(np.argmax(values) if kind == "max" else np.argmin(values))
    return float(freqs[index]), float(values[index])


def beta_to_sofa_direction(beta_deg: int) -> tuple[float, float]:
    if beta_deg <= 90:
        return 0.0, float(beta_deg)
    return 180.0, float(180 - beta_deg)


def sofa_features(path: Path) -> dict[int, dict[str, tuple[float, float]]]:
    out: dict[int, dict[str, tuple[float, float]]] = {}
    with h5py.File(path, "r") as sofa:
        positions = sofa["SourcePosition"][:]
        ir = sofa["Data.IR"][:]
        for beta in BETA_DEG:
            azimuth, elevation = beta_to_sofa_direction(beta)
            i = nearest_position_index(positions, azimuth, elevation)
            mag = 0.5 * (smoothed_magnitude_db(ir[i, 0]) + smoothed_magnitude_db(ir[i, 1]))
            out[beta] = {
                "P1": extrema(mag, 3_000.0, 6_500.0, "max"),
                "N1": extrema(mag, 6_000.0, 11_500.0, "min"),
                "N2": extrema(mag, 10_000.0, 17_000.0, "min"),
            }
    return out


def model_bands() -> dict[int, dict[str, tuple[float, float, float]]]:
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "phrtf_distance_proximity",
        "--example",
        "export_elevation_bands",
    ]
    result = subprocess.run(cmd, cwd=ROOT, check=True, capture_output=True, text=True)
    rows = csv.DictReader(StringIO(result.stdout))
    out: dict[int, dict[str, tuple[float, float, float]]] = {}
    for row in rows:
        beta = int(float(row["beta_deg"]))
        out.setdefault(beta, {})[row["band"]] = (
            float(row["freq_hz"]),
            float(row["gain_db"]),
            float(row["q"]),
        )
    return out


def main() -> None:
    model = model_bands()
    for path in sorted(SOFA_DIR.glob("*.sofa")):
        print(f"\n== {path.name} ==")
        sofa = sofa_features(path)
        print("beta | sofa N1 Hz/dB | model N1 Hz/dB/Q | sofa N2 Hz/dB | model N2 Hz/dB/Q")
        for beta in BETA_DEG:
            s = sofa[beta]
            m = model[beta]
            print(
                f"{beta:>4} | "
                f"{s['N1'][0]:>7.0f}/{s['N1'][1]:>5.1f} | "
                f"{m['N1'][0]:>7.0f}/{m['N1'][1]:>5.1f}/{m['N1'][2]:>3.1f} | "
                f"{s['N2'][0]:>7.0f}/{s['N2'][1]:>5.1f} | "
                f"{m['N2'][0]:>7.0f}/{m['N2'][1]:>5.1f}/{m['N2'][2]:>3.1f}"
            )


if __name__ == "__main__":
    main()
