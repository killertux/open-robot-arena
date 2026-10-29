// Frida agent: one row per call of the ORIGINAL's per-robot power step 0x4dc3f0 (docs/30).
// Row: [seq, robot, airNow, elecNow, airDem(+94), airSup(+98), elecDem(+9c), elecSup(+a0),
//       G6c, G70, G74, G78 (the scale globals the step publishes, 0x714a6c..0x714a78),
//       dt (stack arg), airFrac(+a4), batFrac(+a8)]
// The step's argument is dt (its [esp+0x30] after the prologue is [esp+4] on entry).
'use strict';
let seq = 0, rows = [];
const ids = new Map();
function id(p) { const k = p.toString(); if (!ids.has(k)) ids.set(k, ids.size); return ids.get(k); }
const f = (p, o) => p.add(o).readFloat();
Interceptor.attach(ptr('0x4dc3f0'), {
  onEnter(args) { this.r = this.context.ecx; this.dt = args[0]; this.dtf = this.context.esp.add(4).readFloat(); },
  onLeave() {
    const r = this.r;
    rows.push([++seq, id(r), f(r, 0x8c), f(r, 0x90), f(r, 0x94), f(r, 0x98), f(r, 0x9c), f(r, 0xa0),
               f(ptr(0x714a6c), 0), f(ptr(0x714a70), 0), f(ptr(0x714a74), 0), f(ptr(0x714a78), 0),
               this.dtf, f(r, 0xa4), f(r, 0xa8)]);
    if (rows.length >= 256) { send({ rows: rows }); rows = []; }
  } });
setInterval(() => { if (rows.length) { send({ rows: rows }); rows = []; } }, 500);
