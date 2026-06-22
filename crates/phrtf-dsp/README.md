# phrtf_distance_proximity

Rustで書いた、**パラメトリックHRTF + 距離 + 近接効果**の実装プロトタイプです。

目的は、実測HRTFの畳み込みではなく、ゲーム/リアルタイムDSP向けに、少数のパラメトリックEQだけで「それっぽい3D音像キュー」を作ることです。

## できること

- 飯田氏らのpHRTFに近い考え方で、P1/P2/N1/N2/P0を2次IIRピーキングEQとして合成
- `Direction3D { azimuth, elevation, distance }` に対応
- 水平方向: ITD / 簡易ILD / head-shadow high-shelf
- 垂直方向: 正中面pHRTFのβ角へ投影してP/N周波数を変化
- 距離: 1/rの音圧減衰
- 空気吸収: ISO 9613-1風の周波数依存減衰を計算し、少数のEQで近似
- 近接効果: 指向性マイク風の低域low-shelf boost
- 空気の揺らぎ: 低速ランダムゲイン変動として簡易実装

## 重要な制限

これは**完全な全天球HRTF**ではありません。

- pHRTF本体は上半球正中面モデルがベースです。
- 左右方向はITD/ILD/head-shadowで補っています。
- 下方向は実測モデルではなく、上半球へのミラーまたは水平面clampです。
- 近接効果は人間の耳そのものの近接HRTFではなく、主に指向性マイク的な「近い音」の演出モデルです。
- 厳密な近距離HRTF、肩・胴体、耳介個人差、部屋反射、距離に伴う直接音/残響比は別途必要です。

## 使い方

```bash
cargo run --example print_params
cargo run --example export_response_csv
cargo run --example process_sine
```

ライブラリとして使う場合:

```rust
use phrtf_distance_proximity::{Direction3D, RendererConfig, SpatialPhrtfRenderer};

let sample_rate = 48_000.0;
let config = RendererConfig::new(sample_rate);
let mut renderer = SpatialPhrtfRenderer::new(config);

renderer.update(Direction3D {
    azimuth_deg: 45.0,
    elevation_deg: 30.0,
    distance_m: 2.0,
});

let (left, right) = renderer.process_sample(input_mono);
```

## 処理チェーン

```text
mono input
  -> inverse-distance gain
  -> stochastic turbulence gain
  -> split L/R
  -> ITD fractional delay
  -> proximity low shelf
  -> pHRTF P1/P2/N1/N2/P0 cascade
  -> air absorption EQ
  -> far-ear head-shadow high shelf
  -> stereo output
```

## パラメータの中心

### pHRTF

`PhrtfProfile` を調整してください。

```rust
pub struct PhrtfProfile {
    pub f_p1_hz: f32,
    pub f_p2_hz: f32,
    pub f_n1_front_hz: f32,
    pub f_n2_front_hz: f32,
    pub p1_gain_db: f32,
    pub p2_gain_db: f32,
    pub n1_gain_db: f32,
    pub n2_gain_db: f32,
    pub p1_q: f32,
    pub p2_q: f32,
    pub n1_q: f32,
    pub n2_q: f32,
}
```

個人化するなら、最優先は以下です。

1. `f_n1_front_hz`
2. `f_n2_front_hz`
3. `f_p1_hz`
4. `f_p2_hz`
5. N1/N2のgain/Q

N1/N2は仰角に応じて大きく変化し、P1は比較的固定の参照ピークとして扱います。

### 距離

`RendererConfig` の以下を調整します。

```rust
reference_distance_m: 1.0,
min_distance_m: 0.05,
max_near_gain_db: 18.0,
```

音声サンプルが「1mで収録/設計された音」とみなせる場合、`reference_distance_m = 1.0` のままで良いです。

### 空気吸収

`Atmosphere` と `AirAbsorptionEqConfig` を調整します。

```rust
config.atmosphere.temperature_c = 20.0;
config.atmosphere.relative_humidity = 0.5;
config.air_absorption.perceptual_scale = 1.0;
```

ゲームでは、物理的な空気吸収は短距離だと非常に小さいため、聴感上わかりやすくするには `perceptual_scale = 2.0〜5.0` にしてもよいです。

### 近接効果

```rust
config.proximity.max_boost_db = 6.0;
config.proximity.shelf_frequency_hz = 180.0;
config.proximity.zero_boost_distance_m = 1.0;
config.proximity.full_boost_distance_m = 0.08;
```

`MicrophonePattern::Omni` では近接効果は0になります。`Cardioid` や `FigureEight` のほうが強く出ます。

## ファイル構成

```text
src/
  lib.rs
  biquad.rs          RBJ biquad / EQ / cascade
  phrtf.rs           P1/P2/N1/N2/P0 pHRTF model
  geometry.rs        Direction3D, inverse distance, ITD/ILD
  air_absorption.rs  ISO 9613-1 style attenuation + EQ approximation
  proximity.rs       microphone-style proximity low shelf
  delay.rs           fractional delay for ITD
  noise.rs           turbulence/scintillation approximation
  renderer.rs        full stereo renderer
examples/
  print_params.rs
  export_response_csv.rs
  process_sine.rs
docs/
  design.md
  math.md
  limitations.md
  integration.md
```

## 参考URL

- Iida et al., “Median plane localization using a parametric model of the head-related transfer function based on spectral cues”
- RBJ Audio EQ Cookbook: https://webaudio.github.io/Audio-EQ-Cookbook/audio-eq-cookbook.html
- ISO 9613-1 style implementation reference: https://github.com/python-acoustics/python-acoustics/blob/master/acoustics/standards/iso_9613_1_1993.py
- Air attenuation overview: https://www.mdpi.com/2076-3417/14/22/10139
