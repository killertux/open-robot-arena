// Frida agent: the ORIGINAL's D3D8 fixed-function state at every draw call (docs/34 section 10).
// The device is *[0x71f58c] (the cached SetRenderState wrapper 0x590f30 calls vtbl+0xc8 on it).  At each
// DrawPrimitive/DrawIndexedPrimitive (vtbl+0x118/0x11c) it queries the device (Get* are stdcall COM calls)
// for the render states, material, enabled lights and stage states, and sends one record per distinct
// signature with the number of draws that had it.
'use strict';
const dev = ptr('0x71f58c').readPointer();
const vt = dev.readPointer();
const fn = (idx, ret, args) => { const nf = new NativeFunction(vt.add(idx * 4).readPointer(), ret, ['pointer'].concat(args), 'stdcall'); return (...a) => nf(dev, ...a); };
const GetRS = fn(51, 'int', ['uint', 'pointer']);
const GetMat = fn(43, 'int', ['pointer']);
const GetLight = fn(45, 'int', ['uint', 'pointer']);
const GetLightEnable = fn(47, 'int', ['uint', 'pointer']);
const GetTex = fn(60, 'int', ['uint', 'pointer']);
const GetTSS = fn(62, 'int', ['uint', 'uint', 'pointer']);
const GetTransform = fn(38, 'int', ['uint', 'pointer']);
const buf = Memory.alloc(256);
const RS = { zen: 7, shade: 9, zwrite: 14, atest: 15, spec: 29, fog: 28, fogcol: 34, ablend: 27, src: 19, dst: 20, cull: 22,
  lighting: 137, ambient: 139, localv: 142, normz: 143, colorvtx: 141, dms: 145, sms: 146, ams: 147, ems: 148, tf: 60, fogtab: 35 };
const TSS = { cop: 1, ca1: 2, ca2: 3, aop: 4, aa1: 5, aa2: 6, tci: 11, mag: 16, min: 17, mip: 18, ca0: 26, res: 28, ttf: 24 };
function f(p, o) { return +p.add(o).readFloat().toFixed(4); }
function snap(prim, nv, pc) {
  const o = { prim: prim, nv: nv, pc: pc, rs: {}, lights: [], st: [] };
  for (const k in RS) { buf.writeU32(0xdeadbeef); GetRS(RS[k], buf); const v = buf.readU32(); o.rs[k] = (k === 'ambient' || k === 'fogcol' || k === 'tf') ? '0x' + v.toString(16) : v; }
  if (GetMat(buf) === 0) o.mat = { dif: [0, 4, 8, 12].map(i => f(buf, i)), amb: [16, 20, 24, 28].map(i => f(buf, i)),
    spc: [32, 36, 40, 44].map(i => f(buf, i)), emi: [48, 52, 56, 60].map(i => f(buf, i)), pow: f(buf, 64) };
  for (let i = 0; i < 8; i++) {
    const e = Memory.alloc(4);
    if (GetLightEnable(i, e) !== 0 || e.readU32() === 0) continue;
    if (GetLight(i, buf) !== 0) continue;
    o.lights.push({ i: i, type: buf.readU32(), dif: [4, 8, 12, 16].map(x => f(buf, x)), spc: [20, 24, 28, 32].map(x => f(buf, x)),
      amb: [36, 40, 44, 48].map(x => f(buf, x)), pos: [52, 56, 60].map(x => f(buf, x)), dir: [64, 68, 72].map(x => f(buf, x)),
      range: f(buf, 76), fall: f(buf, 80), a0: f(buf, 84), a1: f(buf, 88), a2: f(buf, 92) });
  }
  for (let s = 0; s < 4; s++) {
    const t = {}; GetTex(s, buf); const tp = buf.readPointer(); t.tex = tp.isNull() ? 0 : tp.toString();
    if (!tp.isNull()) new NativeFunction(tp.readPointer().add(8).readPointer(), 'uint', ['pointer'], 'stdcall')(tp);
    for (const k in TSS) { GetTSS(s, TSS[k], buf); t[k] = buf.readU32(); }
    o.st.push(t);
    if (t.cop === 1 && s > 0) break;
  }
  if (GetTransform(256, buf) === 0) o.world = [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44].map(i => f(buf, i));
  return o;
}
const seen = new Map();
let draws = 0, last = '';
function onDraw(args) {
  draws++;
  let o;
  try { o = snap(0, 0, 0); } catch (e) { send({ err: e.toString() }); return; }
  // geometry args of the DrawIndexedPrimitive (this, type, minIdx, numVerts, startIdx, primCount)
  const a = this.context.esp; // return addr at esp; args follow
  o.dip = [1, 2, 3, 4, 5].map(i => a.add(4 * (i + 1)).readU32());
  const sig = JSON.stringify([o.rs, o.mat, o.lights, o.st, o.dip]);
  const wsig = sig;
  const prev = seen.get(wsig);
  if (prev) { prev.n++; prev.world = o.world; return; }
  o.n = 1; seen.set(wsig, o); o.seq = draws;
  send(o);
}
Interceptor.attach(vt.add(0x11c).readPointer(), { onEnter: onDraw });
setInterval(() => { send({ ev: 'summary', draws: draws, sigs: seen.size }); }, 5000);
