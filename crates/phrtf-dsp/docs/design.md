# 設計メモ

## 1. 目的

この実装は、HRTFを長いFIR畳み込みで再現するのではなく、音像定位に効きやすいスペクトル成分だけを抽出して、少数のパラメトリックEQで再合成する方式です。

実用面では、以下のような用途を想定しています。

- ゲームランタイムで軽量に3D音像キューを付加する
- Wwise/UE/FMODで実装しやすいパラメトリックEQモデルに落とす
- 将来自作DSPライブラリやVST/CLAP/WebCLAPに移植する
- 実測HRTFを持っていない段階で、実装骨格だけ先に作る

## 2. 処理の大枠

```text
source mono signal
  -> distance gain
  -> turbulence/scintillation gain
  -> split to L/R
  -> ITD delay
  -> broadband ILD
  -> proximity EQ
  -> pHRTF spectral EQ
  -> air absorption EQ
  -> head shadow EQ
  -> stereo
```

### なぜこの順番か

- 距離ゲインは全帯域の音圧減衰なので最初に掛ける
- ITDは左右耳の到達時間差なのでL/R分岐直後に置く
- pHRTFは耳介/耳道由来のスペクトルキューとして適用する
- 空気吸収は伝播経路の高域減衰なのでpHRTFの前後どちらでも近似上は大差ない
- head-shadowは左右差の高域減衰なので最後に置くと挙動を確認しやすい

## 3. pHRTF部分

### β角

本実装では、正中面上の角度βを以下のように定義しています。

```text
β = 0°    正面
β = 90°   真上
β = 180°  後方
```

任意の3D方向は、正面軸（+x）からの角度として β を求めます。

```rust
frontal_cos = cos(elevation) * cos(azimuth) // 正面軸との内積
beta        = acos(frontal_cos)             // 正面軸からの角度（0..180°）
up          = sin(elevation)                // 上下半球の判定にのみ使用
```

以前の `atan2(up, front)` は正中面投影のため、水平面の音源が ±90° を横切る
瞬間に β が 0°↔180° と不連続にジャンプし、真横でスペクトルが急変していました。
`acos` 版は ±90°・±180° の通過を含め全球で連続です。

下方向は元論文の対象外なので、デフォルトでは上側にミラーしつつスペクトル強度を弱めています。

### P/N構成

- P1: 4〜6kHz付近の固定ピーク
- P2: 7〜9kHz付近の固定ピーク
- N1: 仰角で動く第1ノッチ
- N2: 仰角で動く第2ノッチ
- P0: 後方定位を補助する1kHz付近のピーク

すべてRBJ peaking EQで実装しています。

```text
P1: gain > 0
P2: gain > 0
N1: gain < 0
N2: gain < 0
P0: gain > 0, rear only
```

## 4. 水平方向

pHRTFだけでは左右定位ができません。そのため以下を追加しています。

### ITD

球状頭部近似です。

```text
theta = asin(abs(lateral))
itd = head_radius / c * (theta + sin(theta))
```

右に音源がある場合、左耳を遅らせます。

### ILD

広帯域ILDは控えめにしています。過度な左右音量差は不自然になりやすいためです。

### Head shadow

遠い耳に high-shelf cut を入れます。

```text
cut_db = -max_head_shadow_db * abs(lateral)
```

デフォルトでは最大 -8 dB、1.8 kHzから高域を落とします。

## 5. 距離

### 音圧減衰

音声サンプルは音圧波形に相当するため、振幅は `1/r` で減衰させています。

```text
gain = reference_distance / distance
```

これは音のエネルギー/強度が `1/r^2` で落ちることに対応します。

### 空気吸収

ISO 9613-1風の式で `alpha(f) [dB/m]` を求めます。

そのままでは任意周波数応答なので、リアルタイムでは以下のEQへ近似しています。

```text
4 kHz broad peak cut
8 kHz broad peak cut
12 kHz high shelf cut
```

高精度にするなら、距離ごとにFIRを設計するか、より多いパラメトリックEQバンドにフィットしてください。

## 6. 近接効果

この実装の近接効果は、厳密な人間の近距離HRTFではなく、指向性マイクの近接効果に近い演出モデルです。

```text
proximity_gain_db = max_boost_db * microphone_gradient_amount * (1 - smoothstep(...distance...))
```

low-shelfで低域をブーストします。

- Omni: 0
- Cardioid: 中程度
- FigureEight: 最大

## 7. 空気の揺らぎ

物理的には、大気乱流で振幅と位相が揺らぎます。本実装では軽量化のため、遠距離のみ低速ランダムゲイン変動を掛けます。

```text
gain_db = noise(-1..1) * gain_db_at_100m * strength * distance_factor
```

音楽用途では控えめ、屋外SE/ドローン/爆発音などでは強めでも面白いです。
