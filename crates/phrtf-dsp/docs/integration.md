# Wwise / Unreal / FMOD / 自作エンジンへの統合メモ

## 1. 自作Rust DSPとして使う場合

一番簡単なのは、各音源ごとに `SpatialPhrtfRenderer` を1つ持つ方法です。

```rust
let mut renderer = SpatialPhrtfRenderer::new(RendererConfig::new(sample_rate));

// control-rate, e.g. once per audio block
renderer.update(Direction3D {
    azimuth_deg,
    elevation_deg,
    distance_m,
});

// audio-rate
for i in 0..num_samples {
    let (l, r) = renderer.process_sample(mono[i]);
    out_l[i] += l;
    out_r[i] += r;
}
```

多数音源を扱う場合は、近い音源だけpHRTF処理し、遠い音源は通常パン+LPFに落とすと軽くなります。

## 2. Wwiseで再現する場合

Wwiseで同じことをするなら、以下のRTPCを用意します。

- Distance
- Azimuth
- Elevation
- LateralAmount
- PhrtfBeta
- ProximityAmount
- AirAbsorptionAmount

実装候補:

- Distance Volume Curve: `1/r`
- Low-pass / High-shelf: Air absorption
- Parametric EQ bands:
  - P1
  - P2
  - N1
  - N2
  - P0
- Stereo delay: ITD
- Far-ear high-shelf: head shadow

Wwise標準機能だけでN1/N2の中心周波数を動かすのが難しい場合は、Source Plugin / Effect Pluginとして実装するほうがよいです。

## 3. Unreal Engineで再現する場合

Unrealなら、MetaSoundまたはSource Effectで実装できます。

- Source Effect Presetにbiquad cascadeを持たせる
- Actor位置からAzimuth/Elevation/Distanceを計算
- Audio threadへ平滑化済みパラメータを渡す
- 係数更新はブロック単位

Unreal標準のSpatialization Pluginと併用する場合は、二重HRTFにならないように注意してください。この実装を使うなら、標準HRTFを切るか、距離/近接/空気吸収部分だけ使う構成が安全です。

## 4. FMODで再現する場合

FMOD DSP pluginとして実装し、以下をパラメータにすると扱いやすいです。

- Azimuth
- Elevation
- Distance
- SpectralStrength
- ProximityStrength
- AirScale
- HeadShadowStrength

## 5. VST/CLAP/WebCLAP化

このcrateは依存なしなので、VST/CLAP/WebCLAP用DSPコアに移植しやすいです。

GUIに出すとよいパラメータ:

- Listener profile:
  - fN1 front
  - fN2 front
  - P1 freq/gain/Q
  - P2 freq/gain/Q
- Rendering:
  - azimuth
  - elevation
  - distance
  - HRTF strength
  - head shadow
  - proximity
  - air absorption scale
- Environment:
  - temperature
  - humidity
  - turbulence amount

## 6. 実用時の推奨デフォルト

```text
reference distance: 1.0 m
minimum distance:   0.05 m
max near gain:      +12 to +18 dB
head radius:        0.0875 m
head shadow:        6 to 10 dB
proximity:          0 to 6 dB
air scale:          1 physical / 2-5 perceptual
```
