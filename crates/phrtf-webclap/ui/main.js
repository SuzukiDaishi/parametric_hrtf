"use strict";

// ---------------------------------------------------------------------------
// Parameter ids — must match crates/phrtf-webclap/src/lib.rs exactly.
// ---------------------------------------------------------------------------
const P = {
  AZIMUTH: 0,
  ELEVATION: 1,
  DISTANCE: 2,
  HEAD_RADIUS: 3,
  NEAR_GAIN: 4,
  EN_ITD: 5,
  EN_ILD: 6,
  EN_HEAD_SHADOW: 7,
  EN_DISTANCE_GAIN: 8,
  EN_AIR: 9,
  EN_PROXIMITY: 10,
  EAR_OFFSET: 11,
  SPECTRAL: 12,
  N1_FRONT: 13,
  N2_FRONT: 14,
  P1: 15,
  P2: 16,
};

// Visual radius of the pad maps to this many metres; further sources still
// work via the Distance slider (param range goes to 20 m).
const PAD_MAX_M = 8;

const state = {
  [P.AZIMUTH]: 0,
  [P.ELEVATION]: 0,
  [P.DISTANCE]: 1,
  [P.HEAD_RADIUS]: 0.0875,
  [P.NEAR_GAIN]: 18,
  [P.EN_ITD]: 1,
  [P.EN_ILD]: 1,
  [P.EN_HEAD_SHADOW]: 1,
  [P.EN_DISTANCE_GAIN]: 1,
  [P.EN_AIR]: 1,
  [P.EN_PROXIMITY]: 1,
  [P.EAR_OFFSET]: 45,
  [P.SPECTRAL]: 1,
  [P.N1_FRONT]: 8000,
  [P.N2_FRONT]: 11500,
  [P.P1]: 4500,
  [P.P2]: 8500,
};

// ---------------------------------------------------------------------------
// WCLAP webview binary protocol (same wire format as z-audio-webclap-eq):
//   ready    -> ascii "eready"
//   set      -> CBOR {"set":[<id u32>, <value f64>]}
//   snapshot -> CBOR {"params":{<id u32>: <value f64>, ...}}  (host -> ui)
// ---------------------------------------------------------------------------
function encodeReady() {
  return new Uint8Array([0x65, 0x72, 0x65, 0x61, 0x64, 0x79]).buffer;
}

function encodeSet(id, value) {
  const buf = new ArrayBuffer(20);
  const view = new DataView(buf);
  view.setUint8(0, 0xa1); // map(1)
  view.setUint8(1, 0x63); // text(3)
  view.setUint8(2, 0x73); // 's'
  view.setUint8(3, 0x65); // 'e'
  view.setUint8(4, 0x74); // 't'
  view.setUint8(5, 0x82); // array(2)
  view.setUint8(6, 0x1a); // uint32 follows
  view.setUint32(7, id, false);
  view.setUint8(11, 0xfb); // float64 follows
  view.setFloat64(12, value, false);
  return buf;
}

function decodeParamsSnapshot(ab) {
  const view = new DataView(ab);
  let p = 0;
  if (view.byteLength < 9 || view.getUint8(p++) !== 0xa1 || view.getUint8(p++) !== 0x66) {
    return null;
  }
  if (String.fromCharCode(...new Uint8Array(ab, p, 6)) !== "params") return null;
  p += 6;
  const head = view.getUint8(p++);
  if ((head & 0xe0) !== 0xa0) return null;
  let count = head & 0x1f;
  if (count === 24) count = view.getUint8(p++);
  const out = new Map();
  for (let i = 0; i < count; i++) {
    if (p + 13 > view.byteLength || view.getUint8(p++) !== 0x1a) return null;
    const key = view.getUint32(p, false);
    p += 4;
    if (view.getUint8(p++) !== 0xfb) return null;
    out.set(key, view.getFloat64(p, false));
    p += 8;
  }
  return out;
}

function sendSet(id, value) {
  state[id] = value;
  window.parent.postMessage(encodeSet(id, value), "*");
}

// ---------------------------------------------------------------------------
// Spatial pad. Azimuth: 0 = front (up), +90 = right, -90 = left, ±180 = rear.
// Screen: x = cx + r*sin(az), y = cy - r*cos(az).
// ---------------------------------------------------------------------------
const pad = document.getElementById("pad");
const source = document.getElementById("source");
const ray = document.getElementById("ray");
const padReadout = document.getElementById("pad-readout");
const CX = 160;
const CY = 160;
const R_MAX = 150;
const R_HEAD = 22;

function clamp(v, lo, hi) {
  return Math.max(lo, Math.min(hi, v));
}

function distanceToRadius(d) {
  return R_HEAD + (clamp(d, 0.05, PAD_MAX_M) / PAD_MAX_M) * (R_MAX - R_HEAD);
}

function paintPad() {
  const az = (state[P.AZIMUTH] * Math.PI) / 180;
  const r = distanceToRadius(state[P.DISTANCE]);
  const x = CX + r * Math.sin(az);
  const y = CY - r * Math.cos(az);
  source.setAttribute("transform", `translate(${x.toFixed(1)} ${y.toFixed(1)})`);
  ray.setAttribute("x2", x.toFixed(1));
  ray.setAttribute("y2", y.toFixed(1));
  padReadout.textContent = `${Math.round(state[P.AZIMUTH])}° · ${state[P.DISTANCE].toFixed(2)} m`;
}

function pointerToPosition(evt) {
  const rect = pad.getBoundingClientRect();
  const scale = 320 / rect.width;
  const px = (evt.clientX - rect.left) * scale - CX;
  const py = (evt.clientY - rect.top) * scale - CY;
  let az = (Math.atan2(px, -py) * 180) / Math.PI; // 0 up, +cw
  az = clamp(az, -180, 180);
  const rpix = Math.hypot(px, py);
  const norm = clamp((rpix - R_HEAD) / (R_MAX - R_HEAD), 0, 1);
  const dist = clamp(0.05 + norm * (PAD_MAX_M - 0.05), 0.05, PAD_MAX_M);
  return { az, dist };
}

let dragging = false;
function applyPad(evt) {
  const { az, dist } = pointerToPosition(evt);
  sendSet(P.AZIMUTH, az);
  sendSet(P.DISTANCE, dist);
  distInput.value = dist;
  distOut.textContent = `${dist.toFixed(2)} m`;
  paintPad();
}
pad.addEventListener("pointerdown", (e) => {
  dragging = true;
  pad.setPointerCapture(e.pointerId);
  applyPad(e);
});
pad.addEventListener("pointermove", (e) => {
  if (dragging) applyPad(e);
});
pad.addEventListener("pointerup", () => {
  dragging = false;
});

// ---------------------------------------------------------------------------
// Sliders + toggles.
// ---------------------------------------------------------------------------
const elevInput = document.getElementById("elev");
const elevOut = document.getElementById("elev-out");
const distInput = document.getElementById("dist");
const distOut = document.getElementById("dist-out");

elevInput.addEventListener("input", () => {
  const v = Number(elevInput.value);
  elevOut.textContent = `${Math.round(v)}°`;
  sendSet(P.ELEVATION, v);
});
distInput.addEventListener("input", () => {
  const v = Number(distInput.value);
  distOut.textContent = `${v.toFixed(2)} m`;
  sendSet(P.DISTANCE, v);
  paintPad();
});

const GLOBALS = [
  { id: P.EAR_OFFSET, label: "Ear Spread", min: 0, max: 90, step: 1, fmt: (v) => `${Math.round(v)}°` },
  { id: P.SPECTRAL, label: "Spectral", min: 0, max: 2, step: 0.01, fmt: (v) => `${Math.round(v * 100)}%` },
  { id: P.HEAD_RADIUS, label: "Head Size", min: 0.06, max: 0.11, step: 0.0005, fmt: (v) => `${(v * 100).toFixed(1)} cm` },
  { id: P.NEAR_GAIN, label: "Near Gain", min: 0, max: 24, step: 0.5, fmt: (v) => `${v.toFixed(1)} dB` },
];

const VOICING = [
  { id: P.N1_FRONT, label: "N1 Notch", min: 4000, max: 14000, step: 50, fmt: fmtHz },
  { id: P.N2_FRONT, label: "N2 Notch", min: 6000, max: 16000, step: 50, fmt: fmtHz },
  { id: P.P1, label: "P1 Peak", min: 3000, max: 7000, step: 50, fmt: fmtHz },
  { id: P.P2, label: "P2 Peak", min: 6000, max: 11000, step: 50, fmt: fmtHz },
];

const TOGGLES = [
  { id: P.EN_ITD, label: "ITD" },
  { id: P.EN_ILD, label: "ILD" },
  { id: P.EN_HEAD_SHADOW, label: "Head Shadow" },
  { id: P.EN_DISTANCE_GAIN, label: "Distance" },
  { id: P.EN_AIR, label: "Air" },
  { id: P.EN_PROXIMITY, label: "Proximity" },
];

function fmtHz(v) {
  return v >= 1000 ? `${(v / 1000).toFixed(2)} kHz` : `${Math.round(v)} Hz`;
}

const controls = new Map();

function buildSliders(defs, container) {
  const root = document.getElementById(container);
  for (const def of defs) {
    const label = document.createElement("label");
    label.className = "slider";
    const lab = document.createElement("span");
    lab.className = "lab";
    lab.textContent = def.label;
    const input = document.createElement("input");
    input.type = "range";
    input.min = def.min;
    input.max = def.max;
    input.step = def.step;
    input.value = state[def.id];
    const out = document.createElement("output");
    out.textContent = def.fmt(state[def.id]);
    input.addEventListener("input", () => {
      const v = Number(input.value);
      out.textContent = def.fmt(v);
      sendSet(def.id, v);
    });
    controls.set(def.id, {
      paint: (v) => {
        input.value = v;
        out.textContent = def.fmt(v);
      },
    });
    label.append(lab, input, out);
    root.append(label);
  }
}

function buildToggles() {
  const root = document.getElementById("toggles");
  for (const def of TOGGLES) {
    const el = document.createElement("div");
    el.className = "toggle" + (state[def.id] >= 0.5 ? " on" : "");
    el.innerHTML = `<span>${def.label}</span><span class="dot"></span>`;
    el.addEventListener("click", () => {
      const next = state[def.id] >= 0.5 ? 0 : 1;
      el.classList.toggle("on", next >= 0.5);
      sendSet(def.id, next);
    });
    controls.set(def.id, {
      paint: (v) => el.classList.toggle("on", v >= 0.5),
    });
    root.append(el);
  }
}

// ---------------------------------------------------------------------------
// Host -> UI snapshots (keeps the GUI in sync with automation / preset loads).
// ---------------------------------------------------------------------------
function applySnapshot(snap) {
  for (const [id, value] of snap) {
    state[id] = value;
    if (id === P.AZIMUTH || id === P.DISTANCE) paintPad();
    if (id === P.ELEVATION) {
      elevInput.value = value;
      elevOut.textContent = `${Math.round(value)}°`;
    }
    if (id === P.DISTANCE) {
      distInput.value = value;
      distOut.textContent = `${value.toFixed(2)} m`;
    }
    controls.get(id)?.paint(value);
  }
  document.getElementById("status").textContent = "CONNECTED";
}

window.addEventListener("message", (event) => {
  if (!(event.data instanceof ArrayBuffer)) return;
  const snap = decodeParamsSnapshot(event.data);
  if (snap) applySnapshot(snap);
});

// ---------------------------------------------------------------------------
// Boot.
// ---------------------------------------------------------------------------
buildToggles();
buildSliders(GLOBALS, "globals");
buildSliders(VOICING, "voicing");
elevInput.value = state[P.ELEVATION];
distInput.value = state[P.DISTANCE];
distOut.textContent = `${state[P.DISTANCE].toFixed(2)} m`;
paintPad();
window.parent.postMessage(encodeReady(), "*");
