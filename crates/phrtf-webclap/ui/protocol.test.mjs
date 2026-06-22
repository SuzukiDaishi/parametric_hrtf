// Node test for the WCLAP webview wire format. Run with:
//   node --test crates/phrtf-webclap/ui/protocol.test.mjs
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const proto = require("./protocol.js");

test("encodeReady is the ascii bytes 'eready'", () => {
  const bytes = new Uint8Array(proto.encodeReady());
  assert.deepEqual([...bytes], [...Buffer.from("eready")]);
});

test("encodeSet lays out CBOR {set:[id,value]} big-endian", () => {
  const view = new DataView(proto.encodeSet(proto.P.AZIMUTH, -90.0));
  assert.equal(view.byteLength, 20);
  assert.equal(view.getUint8(0), 0xa1); // map(1)
  assert.equal(view.getUint8(1), 0x63); // text(3)
  assert.equal(String.fromCharCode(view.getUint8(2), view.getUint8(3), view.getUint8(4)), "set");
  assert.equal(view.getUint8(5), 0x82); // array(2)
  assert.equal(view.getUint8(6), 0x1a); // u32 tag
  assert.equal(view.getUint32(7, false), proto.P.AZIMUTH);
  assert.equal(view.getUint8(11), 0xfb); // f64 tag
  assert.equal(view.getFloat64(12, false), -90.0);
});

test("snapshot decode is the inverse of encode", () => {
  const input = new Map([
    [proto.P.AZIMUTH, 42.5],
    [proto.P.DISTANCE, 3.25],
    [proto.P.EN_PROXIMITY, 0],
    [proto.P.N1_FRONT, 8123.0],
  ]);
  const decoded = proto.decodeParamsSnapshot(proto.encodeParamsSnapshot(input));
  assert.ok(decoded, "decode returned null");
  assert.equal(decoded.size, input.size);
  for (const [k, v] of input) assert.equal(decoded.get(k), v);
});

test("decode rejects buffers that are not a params snapshot", () => {
  assert.equal(proto.decodeParamsSnapshot(proto.encodeReady()), null);
  assert.equal(proto.decodeParamsSnapshot(new ArrayBuffer(4)), null);
});

test("parameter ids are 0..16 contiguous and unique", () => {
  const ids = Object.values(proto.P).sort((a, b) => a - b);
  assert.deepEqual(ids, [...Array(17).keys()]);
});
