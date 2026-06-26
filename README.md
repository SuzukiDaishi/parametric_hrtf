# Parametric HRTF — WebCLAP spatialiser

A dependency-free, real-time **parametric HRTF** binaural spatialiser written in
Rust, packaged as a real [WebCLAP](https://github.com/WebCLAP) plugin so you can
**try it in the browser with a GUI** — drag a sound source around your head and
hear it move.

The DSP grew out of the prototype in `crates/phrtf-dsp` (formerly
`docs/phrtf_distance_proximity`) and is now driven through the same
Rust → `wclap-plugin` → `.wclap` packaging pattern used by
[`SuzukiDaishi/z-audio-dsp-plugin`](https://github.com/SuzukiDaishi/z-audio-dsp-plugin).

## What makes it strong

A full processing chain per ear, all parametric and real-time friendly:

- **Per-ear pHRTF peak/notch cascade** (Iida-style P1/P2/N1/N2/P0). The
  median-plane model is evaluated *separately for each ear* with a `beta` bias —
  the contralateral (shadowed) ear is coloured more "rear", the ipsilateral ear
  more "frontal". This is the main horizontal-localization upgrade over the
  original shared-cascade prototype (see `Ear Spread` in the GUI).
- **ITD** (Woodworth spherical-head) + **broadband ILD**.
- **Head-shadow** high-shelf on the far ear.
- **Distance gain** (1/r pressure law) with optional near-field boost.
- **Air absorption** EQ (ISO 9613-1 approximation).
- **Proximity** low-shelf (microphone-style near-field boost).
- **Control-rate smoothing**: position glides toward its target every block, so
  dragging the GUI never zippers.

## Repository layout

```
crates/
  phrtf-dsp/       dependency-free DSP library (the renderer)
  wclap-plugin/    CLAP/WCLAP wasm runtime scaffold (vendored, MIT,
                   from z-audio-dsp-plugin)
  phrtf-webclap/   WebCLAP plugin adapter + GUI (compiles to wasm)
  xtask/           `cargo xtask bundle-webclap` packager
web/               self-contained WebCLAP browser host (vendored from
                   WebCLAP/browser-test-host) + the loadable bundle
dist/              build output of the packager (git-ignored)
```

## Try it (GUI)

```bash
make wasm-target     # one-time: add the wasm32 rustc target
make serve           # builds the .wclap bundle and serves the test host
```

Then open the printed URL, e.g.:

```
http://localhost:8000/?module=parametric-hrtf.wclap.tar.gz&audio=audio/loop.mp3
```

Click once to start audio, switch to the **UI** tab, and:

- **drag the dot** on the pad to move the source (azimuth + distance) — you
  should hear it pan and change colour around your head;
- **Elevation / Distance** sliders for fine control;
- **Engine** toggles (ITD, ILD, Head Shadow, Distance, Air, Proximity) — flip
  them to A/B each cue;
- **Ear Spread** widens the per-ear pHRTF divergence;
- **pHRTF Voicing** tunes the P/N feature frequencies (personalization).

You can also drag-and-drop your own audio file onto the host page.

> The host needs cross-origin isolation; `scripts/serve.py` uses the vendored
> `web/server.py`, which sends the required `COOP`/`COEP` headers. Any static
> server that sets those headers works too.

## Build & test

```bash
make test            # Rust DSP + adapter tests, and the JS protocol test
make test-rust       # just the Rust tests
make test-js         # just the webview wire-format test (Node)
make bundle          # build wasm + assemble dist/parametric-hrtf.wclap[.tar.gz]
```

Test coverage: per-ear pHRTF divergence + zero-offset identity, position
smoothing, lateral panning (near-ear louder), distance attenuation, finite
output across the full sphere, silence-in/silence-out, `beta` monotonicity,
elevation→notch movement, parameter round-trips/ranges, and the
webview CBOR `set`/`params` wire format.

The bundle is a standard `.wclap`: `module.wasm` + `plugin.json` + `ui/`. It
loads in any WebCLAP host (the bundled `web/` one, or e.g.
`WebCLAP/browser-test-host`) and, via `wclap-bridge`, in native CLAP/VST3 hosts.

## Parameters

| id | name | range | notes |
|----|------|-------|-------|
| 0  | Azimuth | -180…180° | 0 front, +90 right |
| 1  | Elevation | -90…90° | |
| 2  | Distance | 0.05…20 m | |
| 3  | Head Radius | 0.06…0.11 m | |
| 4  | Near Gain | 0…24 dB | optional near-field boost cap |
| 5–10 | ITD / ILD / Head Shadow / Distance Gain / Air / Proximity | on/off | |
| 11 | Ear Offset | 0…90° | per-ear pHRTF divergence |
| 12 | Spectral Strength | 0…2 | pHRTF depth |
| 13–16 | N1 / N2 / P1 / P2 | Hz | pHRTF voicing |

## Credits

- pHRTF model after Iida et al. (median-plane PNP).
- Runtime scaffold vendored from
  [`SuzukiDaishi/z-audio-dsp-plugin`](https://github.com/SuzukiDaishi/z-audio-dsp-plugin) (`crates/wclap-plugin`, MIT).
- Browser host vendored from [`WebCLAP/browser-test-host`](https://github.com/WebCLAP/browser-test-host).

Licensed MIT OR Apache-2.0 (see `crates/phrtf-dsp/LICENSE-*`).
