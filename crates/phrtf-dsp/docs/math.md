# 数式メモ

## 1. 距離減衰

点音源の音圧振幅は距離に反比例します。

```text
p(r) ∝ 1/r
```

基準距離 `r0` からの振幅ゲインは:

```text
G_geo(r) = r0 / max(r, r_min)
```

dBでは:

```text
L(r) = -20 log10(r / r0)
```

距離が2倍になるたびに約 -6 dB です。

## 2. 空気吸収

距離 `r`、周波数 `f` の追加損失:

```text
Loss_air(f, r) = alpha(f) * max(r - r0, 0)
```

`alpha(f)` は dB/m です。

実装では、標準的なISO 9613-1風の式を使っています。

```text
alpha(f) = 8.686 f^2 [ classical + molecular ]
```

ただし、ゲーム実装ではこの曲線を少数のIIR EQへ近似しています。

## 3. pHRTFのβ

```text
frontal_cos = cos(elevation) cos(azimuth)   // 正面軸 +x との内積
beta        = acos(frontal_cos)              // 正面軸からの角度
up          = sin(elevation)                 // 上下半球の判定にのみ使用
```

```text
beta = 0°    front
beta = 90°   above もしくは真横（±90°）
beta = 180°  rear
```

beta は「正面軸からの角度」で定義します。以前は `atan2(up, front)` で正中面へ
投影していましたが、これは横方向の情報を front の符号へ畳み込むため、水平面の
音源が ±90° を横切る瞬間に beta が 0°↔180° と不連続にジャンプしていました
（真横でのスペクトルの急変）。`acos(cos(el)cos(az))` は全球で連続（前後の極を
除いて滑らか）で、±90°・±180° のいずれの通過でも段差が出ません。正中面では
従来どおり前方で仰角、後方で `180° − 仰角` に一致します。

## 4. N1/N2の仰角依存

デフォルトでは、Hzスケールの動きになる多項式係数を使っています。

```text
f_N1(beta) = f_N1(0)
  + 1.001e-5 beta^4
  - 6.431e-3 beta^3
  + 8.686e-1 beta^2
  - 3.265e-1 beta

f_N2(beta) = f_N2(0)
  + 1.310e-5 beta^4
  - 5.154e-3 beta^3
  + 5.020e-1 beta^2
  + 2.563e1 beta
```

注意: この係数は、コピーされた資料で指数表記が崩れることがあります。本実装では `NotchTrajectoryMode` で小さい係数版にも切り替えられるようにしています。

## 5. P0

後方定位補助ピークです。

```text
f_P0 = 1031.25 Hz
Q_P0 = 1.0
```

ゲインはβで補間しています。

```text
beta < 120°       0 dB
beta = 120°       +2 dB
beta = 150°       +3 dB
beta = 180°       +5 dB
```

## 6. ITD

簡易球状頭部近似:

```text
theta = asin(abs(lateral))
ITD = a / c * (theta + sin(theta))
```

- `a`: head radius
- `c`: speed of sound
- `theta`: lateral angle

## 7. Head shadow

遠い耳だけ高域を落とします。

```text
G_shadow_db = -max_head_shadow_db * abs(lateral)
```

high-shelfで実装しています。

## 8. 近接効果

```text
t = smoothstep(full_boost_distance, zero_boost_distance, distance)
G_prox_db = max_boost_db * gradient_amount * (1 - t)
```

low-shelfで実装します。

## 9. RBJ biquad

全EQはRBJ Audio EQ Cookbook系の係数で実装しています。

```text
H(z) = (b0 + b1 z^-1 + b2 z^-2) / (1 + a1 z^-1 + a2 z^-2)
```

処理はTransposed Direct Form IIです。
