// Frida agent: the game's drive model ("MagicAction", vtable 0x6a80f4, apply 0x4856f0)
// of the ORIGINAL.  Function-entry hooks only (mid-function hooks corrupt the x87 state).
//   G  [G,seq,this,chassisBody,mass,nWheels,pos(3),vel(3),angvel(3),rot(9),wheels]
//      wheels = [[id,thrust,touching,normal(3),resistance,grip,f24,steer,point(3),axis(3)],...]
//      read at 0x4856f0 entry: thiscall(this=MagicAction, world, ?, ?)
//   A  [A,seq,h]  0x52a990 (one per accepted Havok step piece)
//   P  [P,seq,motor,pct,ret]  0x4f5780 SpinMotor drive value (ctl+0xb4 * motor+0x29c * 0.7)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const EVERY = (typeof SAMPLE_EVERY === 'number') ? SAMPLE_EVERY : 1;
let calls = 0;
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  if ((calls++ % EVERY) !== 0) return;
  try {
    const t = this.context.ecx;
    const wrap = t.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    const b = t.add(0x24).readPointer(), e = t.add(0x28).readPointer();
    const ws = [];
    for (let r = b; r.compare(e) < 0; r = r.add(0x50)) {
      ws.push([r.readU32(), r.add(4).readS32(), r.add(8).readU8(), vec(r.add(0xc)), f(r.add(0x1c)),
               f(r.add(0x20)), f(r.add(0x24)), r.add(0x28).readU8(), vec(r.add(0x2c)), vec(r.add(0x3c))]);
    }
    const xf = body.add(0xcc);
    const rot = []; for (let i = 0; i < 12; i++) rot.push(f(xf.add(4 * i)));
    push(['G', ++seq, t.toString(), body.toString(), f(body.add(0x190)), ws.length,
          vec(body.add(0x30)), vec(body.add(0x54)), vec(body.add(0x64)), rot, ws]);
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
Interceptor.attach(ptr('0x52a990'), { onEnter() { push(['A', ++seq, f(this.context.esp.add(4))]); } });
Interceptor.attach(ptr('0x4f5780'), {
  onEnter() { this.m = this.context.ecx; },
  onLeave(ret) { try { push(['P', ++seq, this.m.toString(), this.m.add(0x29c).readS32(), ret.toInt32()]); } catch (e) {} } });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
