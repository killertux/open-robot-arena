// Frida agent: Havok 1.x's MOPP compiler in the ORIGINAL (docs/29 section 12v).
// Function-entry/exit hooks only; nothing is written.
//   G  [G,seq,geom,nv,ntri,verts,tris]   0x536a10 entry (a fixed mesh's geometry
//      about to be compiled: +0x18 vertex count, +0x1c vertices 16 bytes apart,
//      +0x24 triangle count, +0x28 triangles 3 x u16)
//   I  [I,seq,n,[[id,sub,min,max],...]]  0x56a770 entry: the primitive list the
//      tree builder starts from (param_6[0], 16-byte entries)
//   T  [T,seq,root,nodes]  0x56a770 exit: the built tree, every node depth first
//      (child +0xec before +0xf0) as [addr,parent,leaf,count,children(2),
//      leafIds, raw 0xf8 bytes as u32]
//   B  [B,seq,len,bytes]  0x565fd0 exit: the first 4 KiB of the code object
//      (raw, as hex), for decoding the compiled program
//   Q  [Q,seq,[tri...]]  0x566a80 MOPP query: the triangles it returns, in order
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const QUERIES = (typeof MOPP_QUERIES === 'boolean') ? MOPP_QUERIES : false;

Interceptor.attach(ptr('0x536a10'), { onEnter() {
  try {
    const g = this.context.esp.add(4).readPointer();
    const nv = g.add(0x18).readS32(), nt = g.add(0x24).readS32();
    const vb = g.add(0x1c).readPointer(), tb = g.add(0x28).readPointer();
    const verts = [], tris = [];
    if (nv > 0 && nv < 50000) for (let i = 0; i < nv; i++) verts.push(vec(vb.add(16 * i)));
    if (nt > 0 && nt < 50000) for (let i = 0; i < nt; i++) {
      const t = tb.add(6 * i); tris.push([t.readU16(), t.add(2).readU16(), t.add(4).readU16()]);
    }
    push(['G', ++seq, g.toString(), nv, nt, verts, tris]);
  } catch (e) { push(['E', ++seq, 'G ' + String(e)]); }
} });

Interceptor.attach(ptr('0x56a770'), {
  onEnter() {
    try {
      const sp = this.context.esp;
      // thiscall: ecx = builder; stack: mediator, ?, planes, settings, param_6
      const mediator = sp.add(4).readPointer();
      const p6 = sp.add(0x14).readPointer();
      const list = p6.readPointer();
      const n = new NativeFunction(mediator.readPointer().add(4).readPointer(), 'int', ['pointer'], 'thiscall')(mediator);
      const out = [];
      for (let i = 0; i < n && i < 50000; i++) {
        const e = list.add(16 * i);
        out.push([e.readS32(), e.add(4).readS32(), f(e.add(8)), f(e.add(12))]);
      }
      push(['I', ++seq, n, out]);
    } catch (e) { push(['E', ++seq, 'I ' + String(e)]); }
  },
  onLeave(ret) {
    try {
      const nodes = [];
      const st = [ret];
      let guard = 0;
      while (st.length && guard++ < 200000) {
        const n = st.pop();
        if (n.isNull()) continue;
        const leaf = n.add(4).readU8();
        const count = n.add(8).readS32();
        const raw = [];
        const size = leaf ? 0xbc : 0xf8;
        for (let o = 0; o < size; o += 4) raw.push(n.add(o).readU32());
        let c0 = ptr(0), c1 = ptr(0), ids = [];
        if (leaf) {
          const ents = n.add(0xb8).readPointer();
          for (let i = 0; i < count && i < 64; i++) ids.push(ents.add(16 * i).readS32());
        } else {
          c0 = n.add(0xec).readPointer(); c1 = n.add(0xf0).readPointer();
        }
        nodes.push([n.toString(), n.readPointer().toString(), leaf, count, [c0.toString(), c1.toString()], ids, raw]);
        if (!leaf) { st.push(c1); st.push(c0); }
      }
      push(['T', ++seq, ret.toString(), nodes]);
    } catch (e) { push(['E', ++seq, 'T ' + String(e)]); }
  }
});

Interceptor.attach(ptr('0x565fd0'), { onLeave(ret) {
  try {
    const words = [];
    for (let o = 0; o < 0x40; o += 4) words.push(ret.add(o).readU32());
    // the code object: dump 4 KiB behind every pointer-looking word too
    const blobs = [];
    for (const w of words) {
      const p = ptr(w);
      try { blobs.push([w, Array.from(new Uint8Array(p.readByteArray(4096))).map(b => (b < 16 ? '0' : '') + b.toString(16)).join('')]); } catch (e) {}
    }
    push(['B', ++seq, ret.toString(), words, blobs]);
  } catch (e) { push(['E', ++seq, 'B ' + String(e)]); }
} });

if (QUERIES) Interceptor.attach(ptr('0x566a80'), {
  onEnter() { this.out = this.context.esp.add(0x14).readPointer(); },
  onLeave() {
    try {
      const b = this.out.readPointer(), n = this.out.add(4).readS32();
      const o = [];
      for (let i = 0; i < n && i < 512; i++) o.push(b.add(8 * i).readS32());
      push(['Q', ++seq, o]);
    } catch (e) { push(['E', ++seq, 'Q ' + String(e)]); }
  }
});
setInterval(flush, 200);
rpc.exports = { flush() { flush(); } };
