// Frida agent: ra2_boxfight.js plus every hinge motor setting (M: the Scout's
// fork strokes) and the chassis' velocity and angular velocity in the X rows,
// every 4th piece, to set ALARM's hammer and the Scout's fork against the
// chassis' tips (docs/29 section 12ap).  Function-entry/leave hooks only.
//   M  [M,seq,motor,velocity,max torque]  0x4815c0 hinge motor set, on change
//   X  [X,seq,[[chassis,com(3),rot +0xcc (9),vel +0x54 (3),angvel +0x64 (3)],...]]
// Rows A/H/G/P/W/F/GP/GR as ra2_boxfight.js.
//   A  [A,seq,h]                  0x52a990: one per accepted step piece
//   H  [H,seq,attPlayer,attComp,vicPlayer,vicComp,amount,point(3),normal(3)]
//                                 0x417330 entry, the damage gate (as ra2_arenahits.js)
//   G  [G,seq,chassis]            0x4856f0 MagicAction apply, once per new chassis
//   X  [X,seq,[[chassis,com(3),rot +0xcc (9),vel +0x54,angvel +0x64],...]]  every 4th piece
//   P  [P,seq,leaving,[coll,tag,phantom],[coll,tag,phantom]]  0x4a4780, the
//      zone listener, every phantom-pair event (type DAT_006abf6c)
//   W  [W,seq,[direction,id,robot,chassis]]  0x4c9050, the arena's ZoneEvent
//      handed to Python (Arena_callback vtable slot +0x28)
//   F  [F,seq,controller,force]   0x4b51b0, a prismatic controller's push
//   GP [GP,seq,radius +0x118,[arg1..arg4 as 3 floats or null],[raw args]]
//      0x405990 GetPath entry, and on leave
//   GR [GR,seq,[[arg index, floats of the vector begin..end]]]
'use strict';
let seq = 0, rows = [], pieces = 0;
function push(r) { rows.push(r); if (rows.length >= 256) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
const chassis = new Map();
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  push(['A', ++seq, f(this.context.esp.add(4))]);
  if (++pieces % 4 === 0 && chassis.size) {
    const o = [];
    for (const [k, b] of chassis) {
      try {
        const rot = []; for (let i = 0; i < 9; i++) rot.push(f(b.add(0xcc + 4 * i)));
        o.push([k, vec(b.add(0x30)), rot, vec(b.add(0x54)), vec(b.add(0x64))]);
      } catch (e) {}
    }
    push(['X', seq, o]);
  }
} });
Interceptor.attach(ptr('0x4856f0'), { onEnter() {
  try {
    const wrap = this.context.ecx.add(0x30).readPointer();
    if (wrap.isNull()) return;
    let body = wrap.add(8).readPointer();
    if (body.isNull()) body = wrap.add(0x10).readPointer();
    const k = body.toString();
    if (!chassis.has(k)) { chassis.set(k, body); push(['G', ++seq, k]); }
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x417330'), { onEnter() {
  try {
    const sp = this.context.esp;
    let loc = null, nrm = null;
    try { loc = vec(sp.add(24).readPointer()); } catch (e) {}
    try { nrm = vec(sp.add(28).readPointer()); } catch (e) {}
    push(['H', ++seq, sp.add(4).readS32(), sp.add(8).readPointer().toString(), sp.add(12).readS32(),
          sp.add(16).readPointer().toString(), f(sp.add(20)), loc, nrm]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
const PAIR = ptr('0x6abf6c').readU32();
Interceptor.attach(ptr('0x4a4780'), { onEnter() {
  try {
    const ev = this.context.esp.add(4).readPointer();
    if (ev.add(8).readU32() !== PAIR) return;
    const side = c => c.isNull() ? null : [c.toString(), c.add(0x54).readU32(), c.add(0x58).readU8()];
    push(['P', ++seq, ev.add(0x14).readU32(), side(ev.add(0xc).readPointer()), side(ev.add(0x10).readPointer())]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x4c9050'), { onEnter() {
  try {
    const sp = this.context.esp, a = [];
    for (let i = 1; i <= 4; i++) a.push(sp.add(4 * i).readS32());
    push(['W', ++seq, a]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x4b51b0'), { onEnter() {
  try { push(['F', ++seq, this.context.ecx.toString(), f(this.context.esp.add(4))]); } catch (e) {}
} });
Interceptor.attach(ptr('0x405990'), {
  onEnter() {
    try {
      const sp = this.context.esp, raw = [], v = [];
      this.args4 = [];
      for (let i = 1; i <= 4; i++) {
        const a = sp.add(4 * i).readPointer();
        this.args4.push(a);
        raw.push(a.toString());
        let x = null;
        try { x = vec(a); } catch (e) {}
        v.push(x);
      }
      push(['GP', ++seq, f(this.context.ecx.add(0x118)), v, raw]);
    } catch (e) { push(['E', ++seq, 'GP ' + String(e)]); }
  },
  onLeave() {
    try {
      const out = [];
      (this.args4 || []).forEach((a, i) => {
        try {
          const b = a.add(4).readPointer(), e = a.add(8).readPointer();
          const n = e.sub(b).toInt32();
          if (!b.isNull() && n > 0 && n <= 256 && n % 4 === 0) {
            const fl = []; for (let k = 0; k < n; k += 4) fl.push(f(b.add(k)));
            out.push([i, fl]);
          }
        } catch (err) {}
      });
      push(['GR', seq, out]);
    } catch (e) { push(['E', ++seq, 'GR ' + String(e)]); }
  },
});
const lastMotor = {};
Interceptor.attach(ptr('0x4815c0'), { onEnter() {
  try {
    const m = this.context.ecx.toString(), sp = this.context.esp, v = f(sp.add(4)), t = f(sp.add(8));
    if (lastMotor[m] && lastMotor[m][0] === v && lastMotor[m][1] === t) return;
    lastMotor[m] = [v, t];
    push(['M', ++seq, m, v, t]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
