// WCLAP webview <-> plugin wire format, isolated from the DOM so it can be
// unit-tested under Node (see protocol.test.mjs). Loaded as a plain script in
// the browser (exposes `window.PhrtfProtocol`) and as a CommonJS module in
// tests.
//
//   ready    -> ascii "eready"
//   set      -> CBOR {"set":[<id u32>, <value f64>]}            (ui -> plugin)
//   snapshot -> CBOR {"params":{<id u32>: <value f64>, ...}}    (plugin -> ui)
(function (root) {
  "use strict";

  // Parameter ids — must match crates/phrtf-webclap/src/lib.rs exactly.
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

  /// Build a {"params": {...}} snapshot buffer. Only used by the tests (the
  /// plugin emits these from Rust), but keeping the encoder next to the decoder
  /// lets the round-trip be verified.
  function encodeParamsSnapshot(map) {
    const entries = [...map.entries()];
    const buf = new ArrayBuffer(9 + entries.length * 14);
    const view = new DataView(buf);
    let p = 0;
    view.setUint8(p++, 0xa1); // map(1)
    view.setUint8(p++, 0x66); // text(6)
    for (const ch of "params") view.setUint8(p++, ch.charCodeAt(0));
    // entries.length assumed < 24 (true for our 17 params).
    view.setUint8(p++, 0xa0 | entries.length); // map(n)
    for (const [id, value] of entries) {
      view.setUint8(p++, 0x1a);
      view.setUint32(p, id, false);
      p += 4;
      view.setUint8(p++, 0xfb);
      view.setFloat64(p, value, false);
      p += 8;
    }
    return buf;
  }

  const api = { P, encodeReady, encodeSet, decodeParamsSnapshot, encodeParamsSnapshot };
  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  } else {
    root.PhrtfProtocol = api;
  }
})(typeof self !== "undefined" ? self : this);
