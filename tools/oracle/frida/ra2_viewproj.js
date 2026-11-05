// Frida agent: the ORIGINAL's view, projection and viewport at every distinct draw (the Bot Lab's component preview
// and 3D view: docs/36 9.29). The D3D8 device is *[0x71f58c] (see ra2_d3dstate.js); vtable +0x98 GetTransform
// (VIEW = 2, PROJECTION = 3), +0x104 GetViewport; the draw is DrawIndexedPrimitive (+0x11c).
'use strict';
const dev = ptr('0x71f58c').readPointer();
const vt = dev.readPointer();
const fn = (idx, ret, args) => { const nf = new NativeFunction(vt.add(idx * 4).readPointer(), ret, ['pointer'].concat(args), 'stdcall'); return (...a) => nf(dev, ...a); };
const GetTransform = fn(38, 'int', ['uint', 'pointer']);
const GetViewport = fn(41, 'int', ['pointer']);
const buf = Memory.alloc(128);
const seen = new Map();
function m(state) { GetTransform(state, buf); const o = []; for (let i = 0; i < 16; i++) o.push(+buf.add(i * 4).readFloat().toFixed(5)); return o; }
Interceptor.attach(vt.add(0x11c).readPointer(), { onEnter() {
  const o = { view: m(2), proj: m(3) };
  GetViewport(buf); o.vp = [0, 4, 8, 12, 16, 20].map(i => buf.add(i).readU32()).slice(0, 4).concat([+buf.add(16).readFloat().toFixed(3), +buf.add(20).readFloat().toFixed(3)]);
  const sig = JSON.stringify(o);
  const prev = seen.get(sig);
  if (prev) { prev.n++; return; }
  o.n = 1; seen.set(sig, o); send(o);
} });
