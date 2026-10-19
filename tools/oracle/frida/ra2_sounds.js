// Frida agent: every SoundWizard call of the ORIGINAL (effects over DSOUND).
// Entry hooks only.  All rows carry ms since the agent loaded plus Date.now()
// so the host can align them with a parecord start.
//   C  create   [C,seq,ms,this,a1..a8(hex),name,retHandle,ret,pos]   0x4f0000 SoundWizard::Create (ret 0x20)
//      bt = fuzzy backtrace (return addresses, innermost first)
//      a3 = initial volume (hdb), a4 = 3D flag, a6 = &position, a7/a8 = max/min distance (floats)
//   (every h below is the SWHandle id, read through the pointer the call receives)
//   P  play     [P,seq,ms,h,once,x]                          0x4f03b0 Play(h, once, 0)
//   L  loop     [L,seq,ms,h]                                 0x4f0680 Loop(h)
//   S  stop     [S,seq,ms,h]                                 0x4f05c0 Stop(h)
//   V  volume   [V,seq,ms,h,vol_hdb,fadeMs]                  0x4f0bb0 SetVolume(h, vol, ms)
//   O  fadeOut  [O,seq,ms,h,ms]                              0x4f0e30
//   Q  fadeOutLoop [Q,seq,ms,h,ms]                           0x4f0ec0
//   I  fadeInToLoop [I,seq,ms,h,vol,ms]                      0x4f0f50
//   X  position [X,seq,ms,h,x,y,z]                           0x4f10a0 SetPosition(h, vec*)
//   N  mindist  [N,seq,ms,h,d]   0x4f10f0     M maxdist [M,seq,ms,h,d]  0x4f1130
//   B  buffer volume [B,seq,ms,buf,vol]   0x62c5a0 IDirectSoundBuffer::SetVolume wrapper
//   D  buffer play   [D,seq,ms,buf,pri,flags]  0x62c4a0
//   K  music folder [K,seq,ms,dir,shuffle?,fade]   0x4f1350 SetMusicFolder(dir, 1, 3.0) (+ the \*.mp3 playlist build)
//   T  music track  [T,seq,ms,name,vol,flag]       0x4f12d0 play one playlist entry (starts the stream)
//   R  music tick   [R,seq,ms]                     0x4f1620 end-of-track advance (once per track end)
//   G  music vol    [G,seq,ms,v]                   0x4f1600 set the global music volume (hdb)
//   U  ui load  [U,seq,ms,name,flags]   0x6287e0 DirectMusic loader (5e6070's UI one-shot: load, download, play, unload)
//   W  ui play  [W,seq,ms,handle]       0x629370 segment play
//   5  ui call  [5,seq,ms]              0x5e6070 UI play-once entry
'use strict';
let seq = 0, rows = [], t0 = Date.now();
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const now = () => Date.now();
const names = {};
function a(ctx, i) { return ctx.esp.add(4 * i); }
function cstr(p) {
  try {
    const s = p.readCString();
    if (s && s.length > 2 && s.length < 260 && /^[\x20-\x7e]+$/.test(s)) return s;
  } catch (e) {}
  return null;
}
// a std::string-like argument: the char* sits in the object, at +4 (MSVC6 layout) or +0
function findName(vals) {
  for (const v of vals) {
    let s = cstr(v);
    if (s) return s;
    try { s = cstr(v.readPointer()); if (s) return s; } catch (e) {}
    try { s = cstr(v.add(4).readPointer()); if (s) return s; } catch (e) {}
  }
  return null;
}
Interceptor.attach(ptr('0x4f0000'), {
  onEnter() {
    const vals = []; for (let i = 1; i <= 8; i++) vals.push(a(this.context, i).readPointer());
    this.vals = vals; this.ret0 = vals[0]; this.t = now();
    try { this.bt = Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 6).map(x => x.toString()); } catch (e) { this.bt = null; }
    this.name = findName([vals[1]]);
    this.pos = null;
    try { const v = vals[5]; this.pos = [f(v), f(v.add(4)), f(v.add(8))]; } catch (e) {}
  },
  onLeave(rv) {
    let h = null;
    try { h = [this.ret0.readU32(), this.ret0.add(4).readU32()]; } catch (e) {}
    push(['C', ++seq, this.t, this.context.ecx.toString(), this.vals.map(String), this.name, h, rv.toString(), this.pos, this.bt]);
  } });
function simple(addr, tag, nargs, fl) {
  Interceptor.attach(ptr(addr), { onEnter() {
    const r = [tag, ++seq, now()];
    for (let i = 1; i <= nargs; i++) r.push(fl && fl.indexOf(i) >= 0 ? f(a(this.context, i)) : a(this.context, i).readS32());
    // argument 1 is a pointer to the SWHandle {id, valid}: replace it by the id
    try { r[3] = ptr(r[3]).readS32(); } catch (e) { r[3] = null; }
    push(r); } });
}
simple('0x4f03b0', 'P', 3);
simple('0x4f0680', 'L', 1);
simple('0x4f05c0', 'S', 1);
simple('0x4f0bb0', 'V', 3);
simple('0x4f0e30', 'O', 2);
simple('0x4f0ec0', 'Q', 2);
simple('0x4f0f50', 'I', 3);
Interceptor.attach(ptr('0x4f10a0'), { onEnter() {
  try { const v = a(this.context, 2).readPointer(); push(['X', ++seq, now(), a(this.context, 1).readPointer().readS32(), f(v), f(v.add(4)), f(v.add(8))]); } catch (e) { push(['E', ++seq, String(e)]); } } });
simple('0x4f10f0', 'N', 2, [2]);
simple('0x4f1130', 'M', 2, [2]);
Interceptor.attach(ptr('0x62c5a0'), { onEnter() { push(['B', ++seq, now(), this.context.ecx.toString(), a(this.context, 1).readS32()]); } });
Interceptor.attach(ptr('0x62c4a0'), { onEnter() { push(['D', ++seq, now(), this.context.ecx.toString(), a(this.context, 1).readS32(), a(this.context, 2).readS32()]); } });
Interceptor.attach(ptr('0x6287e0'), { onEnter() {
  let n = null; try { n = a(this.context, 1).readPointer().readCString(); } catch (e) {}
  push(['U', ++seq, now(), n, a(this.context, 2).readS32()]); } });
Interceptor.attach(ptr('0x629370'), { onEnter() {
  let h = null; try { h = a(this.context, 1).readPointer().readS32(); } catch (e) {}
  push(['W', ++seq, now(), h, a(this.context, 2).readS32()]); } });
Interceptor.attach(ptr('0x5e6070'), { onEnter() { push(['5', ++seq, now()]); } });
function str(p) { return findName([p]); }
Interceptor.attach(ptr('0x4f1350'), { onEnter() {
  push(['K', ++seq, now(), str(a(this.context, 1).readPointer()) || str(a(this.context, 1).readPointer().readPointer()), a(this.context, 2).readS32(), f(a(this.context, 3))]); } });
Interceptor.attach(ptr('0x4f12d0'), { onEnter() {
  const p = a(this.context, 1).readPointer();
  push(['T', ++seq, now(), str(p) || str(p.readPointer()), a(this.context, 2).readS32(), a(this.context, 3).readS32()]); } });
Interceptor.attach(ptr('0x4f1620'), { onEnter() { push(['R', ++seq, now()]); } });
Interceptor.attach(ptr('0x4f1600'), { onEnter() { push(['G', ++seq, now(), a(this.context, 1).readS32()]); } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 200);
