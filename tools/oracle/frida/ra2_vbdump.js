// Frida agent: the vertex and index data of every distinct small DrawIndexedPrimitive of the ORIGINAL
// (docs/34 section 11).  Per distinct (vertex buffer, index buffer, minIdx, numVerts, startIdx, primCount) one record:
// FVF, stride, the vertices' bytes and the indices as hex, the world matrix and the material, so a robot's drawn
// triangles, normals and texture coordinates can be compared with ours.  Needs the buffers lockable (wined3d: yes).
'use strict';
const dev = ptr('0x71f58c').readPointer();
const vt = dev.readPointer();
const fn = (idx, ret, args) => { const nf = new NativeFunction(vt.add(idx * 4).readPointer(), ret, ['pointer'].concat(args), 'stdcall'); return (...a) => nf(dev, ...a); };
const GetTransform = fn(38, 'int', ['uint', 'pointer']);
const GetMat = fn(43, 'int', ['pointer']);
const GetVS = fn(77, 'int', ['pointer']);
const GetStream = fn(84, 'int', ['uint', 'pointer', 'pointer']);
const GetIndices = fn(86, 'int', ['pointer', 'pointer']);
const maxVerts = 400;
const out = Memory.alloc(256);
const seen = new Set();
let draws = 0;
function lockRead(buf, off, size) {
  const vtb = buf.readPointer();
  const Lock = new NativeFunction(vtb.add(11 * 4).readPointer(), 'int', ['pointer', 'uint', 'uint', 'pointer', 'uint'], 'stdcall');
  const Unlock = new NativeFunction(vtb.add(12 * 4).readPointer(), 'int', ['pointer'], 'stdcall');
  const pp = Memory.alloc(8);
  if (Lock(buf, off, size, pp, 0x10) !== 0) return null;
  const bytes = pp.readPointer().readByteArray(size);
  Unlock(buf);
  return bytes;
}
function hex(ab) { return Array.from(new Uint8Array(ab)).map(b => ('0' + b.toString(16)).slice(-2)).join(''); }
Interceptor.attach(vt.add(0x11c).readPointer(), {
  onEnter() {
    draws++;
    const a = this.context.esp;
    const dip = [1, 2, 3, 4, 5].map(i => a.add(4 * (i + 1)).readU32()); // type, minIdx, numV, startIdx, prim
    if (dip[2] > maxVerts || dip[2] === 0) return;
    try {
      if (GetStream(0, out, out.add(8)) !== 0) return;
      const vb = out.readPointer(); const stride = out.add(8).readU32();
      if (GetIndices(out.add(16), out.add(24)) !== 0) return;
      const ib = out.add(16).readPointer(); const base = out.add(24).readU32();
      const key = [vb, ib, dip.join(',')].join('/');
      if (seen.has(key)) return;
      seen.add(key);
      GetVS(out.add(32)); const fvf = out.add(32).readU32();
      const vbytes = lockRead(vb, (dip[1] + base) * stride, dip[2] * stride);
      const ibytes = lockRead(ib, dip[3] * 2, dip[4] * 3 * 2);
      const o = { dip, stride, fvf, base, vb: hex(vbytes), ib: hex(ibytes), seq: draws };
      if (GetTransform(256, out) === 0) o.world = [0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60].map(i => +out.add(i).readFloat().toFixed(4));
      if (GetMat(out) === 0) o.mat = { dif: [0, 4, 8, 12].map(i => +out.add(i).readFloat().toFixed(4)), amb: [16, 20, 24, 28].map(i => +out.add(i).readFloat().toFixed(4)),
        spc: [32, 36, 40, 44].map(i => +out.add(i).readFloat().toFixed(4)), pow: +out.add(64).readFloat().toFixed(3) };
      send(o);
    } catch (e) { send({ err: e.toString() }); }
  }
});
