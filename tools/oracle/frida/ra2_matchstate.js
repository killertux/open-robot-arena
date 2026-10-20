// Frida agent: match-shell state hooks in the ORIGINAL (docs/34 section 5/9).
//
// Hook points recovered from the exe (match object = ecx, thiscall):
//   0x475540  start of a match
//   0x4758c0  pre-countdown state (intro running: hides countdown group, shows the Skip Intro group)
//   0x4759c0  starts the countdown (Skip Intro click or the intro's own end): +0x1dc = 4.0 s
//   0x416360  begin fight (+0x8e = 1)
//   0x477180  camera-name notification shown; +0x17c = 3.0 (decremented by dt in 0x477480)
//   0x477480  per-frame match update; [esp+4] -> frame info, dt = float at +4; this+0x1c0 sums dt
//   0x591030  texture manager get-by-name (cdecl, char* name) ; 0x591410 release ; 0x592300 clear list
// plus CreateFileA/W of texture files (to see which file a name resolves to, and whether it is
// opened again when an arena is loaded a second time).
'use strict';
let match = null;
let ic = 0;           // intro clock: summed dt since the intro state began (0x4758c0)
const t0 = Date.now();
const now = () => Date.now() - t0;
const f = (p, o) => p.add(o).readFloat();
function snap() {
  if (match === null) return null;
  return { t: now(), cd: f(match, 0x1dc), lbl: f(match, 0x17c), sum: f(match, 0x1c0),
           st: match.add(0x1d0).readS32(), ic: ic, st2: match.add(0x1d4).readS32(), fight: match.add(0x8e).readU8() };
}
function ev(name, extra) { send(Object.assign({ ev: name, t: now(), s: snap() }, extra || {})); }
for (const [a, n] of [['0x475540', 'match_start'], ['0x4758c0', 'intro_state'],
                      ['0x4759c0', 'countdown_start'], ['0x416360', 'fight_start'],
                      ['0x477180', 'camera_label']]) {
  Interceptor.attach(ptr(a), { onEnter() { match = this.context.ecx; ev(n); } });
}
let upd = 0;
Interceptor.attach(ptr('0x4758c0'), { onEnter() { ic = 0; } });
Interceptor.attach(ptr('0x475540'), { onEnter() { upd = 0; } });
Interceptor.attach(ptr('0x477480'), { onEnter() {
  match = this.context.ecx;
  const fr = this.context.esp.add(4).readPointer();
  upd++;
  const dt = f(fr, 4);
  ic += dt;
  // the first updates after a reset of the dt sum, and one in 30 afterwards
  if (upd <= 3) send({ ev: 'update', t: now(), dt: dt, s: snap(), n: upd });
} });
Interceptor.attach(ptr('0x591030'), {
  onEnter(args) { this.name = this.context.esp.add(4).readPointer().readCString(); },
  onLeave(r) { const p = ptr('0x71f574').readPointer(); send({ ev: 'tex_get', t: now(), name: this.name, tex: r.toString(), pfx: p.isNull() ? '' : cstr(p) }); } });
Interceptor.attach(ptr('0x591410'), { onEnter() {
  const tex = this.context.esp.add(4).readPointer();
  send({ ev: 'tex_release', t: now(), tex: tex.toString(), id: tex.isNull() ? null : tex.add(0x28).readS32() });
} });
Interceptor.attach(ptr('0x592300'), { onEnter() { send({ ev: 'tex_clear_all', t: now() }); } });
Interceptor.attach(ptr('0x5926d0'), { onEnter() { send({ ev: 'gfx_shutdown', t: now() }); } });
// every image file the game opens: Wine's PE DLLs are not Frida modules, so resolve the exe's own
// imports of CreateFileA/W from its import table (PE32 at 0x400000) and hook what they point at.
function importTarget(fn) {
  const base = ptr('0x400000');
  const nt = base.add(base.add(0x3c).readU32());
  let d = base.add(nt.add(24 + 104).readU32());
  for (; d.add(12).readU32() !== 0; d = d.add(20)) {
    let o = base.add(d.readU32() || d.add(16).readU32()), t = base.add(d.add(16).readU32());
    for (; o.readU32() !== 0; o = o.add(4), t = t.add(4)) {
      if (o.readU32() & 0x80000000) continue;
      if (base.add(o.readU32()).add(2).readCString() === fn) return t.readPointer();
    }
  }
  return null;
}
for (const [fn, wide] of [['CreateFileA', false], ['CreateFileW', true]]) {
  const a = importTarget(fn);
  if (a === null) continue;
  Interceptor.attach(a, { onEnter(args) {
    let p = null;
    try { p = wide ? args[0].readUtf16String() : args[0].readCString(); } catch (e) { return; }
    if (p && /\.(bmp|tga|jpg|jpeg|png)$/i.test(p)) send({ ev: 'open', t: now(), path: p });
  } });
}
// the image cache (by file name, list at 0x71f088): lookup/load 0x5914d0, erase 0x5918c0, clear 0x5924e0
function cstr(p) { try { return p.readCString(); } catch (e) { return null; } }
function strarg(p) {              // a char* or a pointer to an MSVC6 std::string (_Ptr at +4)
  let s = cstr(p);
  if (s !== null && /^[\x20-\x7e]+$/.test(s)) return s;
  try { return cstr(p.add(4).readPointer()); } catch (e) { return null; }
}
Interceptor.attach(ptr('0x5914d0'), {
  onEnter() { this.n = strarg(this.context.esp.add(4).readPointer()); },
  onLeave(r) { send({ ev: 'img_get', t: now(), name: this.n, img: r.toString() }); } });
Interceptor.attach(ptr('0x5918c0'), { onEnter() { send({ ev: 'img_erase', t: now() }); } });
Interceptor.attach(ptr('0x5924e0'), { onEnter() { send({ ev: 'img_clear_all', t: now() }); } });
// the global texture path std::string (0x71f570; _Ptr at 0x71f574): changed only by 0x593a40 (bot lab)
Interceptor.attach(ptr('0x593a40'), {
  onEnter() { const a = this.context.esp.add(4).readPointer(); send({ ev: 'texpath_set', t: now(), arg: strarg(a) }); },
  onLeave() { const p = ptr('0x71f574').readPointer(); send({ ev: 'texpath_now', t: now(), path: p.isNull() ? '' : cstr(p) }); } });
rpc.exports = { state() { return snap(); } };
