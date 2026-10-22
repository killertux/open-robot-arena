// Frida agent: what freezes FlexTop's table when the match ends (docs/29 section 12j).
// Rows (all through send({ev:'batch', rows})):
//   A  [A,seq,h]                       0x52a990, one per accepted step piece
//   H  [H,seq,name,step,args...,bt]     entry of the match-end functions and the arena's
//      pinning wrappers: 0x4170e0 match end, 0x416450 deactivate robots, 0x4172d0,
//      0x405050 (Arena SetPinned, name, on), 0x405010 (SetActive), 0x4c9000 (Arena Activate)
//   X  [X,seq,step,[[off,old,new]..]]   the table body (angular dashpot's body B, first dashpot):
//      words of its first 0x200 bytes that changed since the previous step, when 1..12 did
//   F  [F,seq,step,old,new,[[off,old,new]..]]  the body's +0x1c word (movable test 0x4812c0) changed
//   T  [T,seq,step,com(3),vel(3)]       its centre and +0x54 every 30th step
'use strict';
let seq = 0, step = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
const vec = p => [f(p), f(p.add(4)), f(p.add(8))];
function bt(ctx) {
  try { return Thread.backtrace(ctx, Backtracer.FUZZY).slice(0, 6).map(a => a.toString()); } catch (e) { return []; }
}
function hook(addr, name, nargs) {
  Interceptor.attach(ptr(addr), { onEnter() {
    const a = [];
    for (let i = 0; i < nargs; i++) { try { a.push(this.context.esp.add(4 + 4 * i).readU32()); } catch (e) { a.push(null); } }
    let s = null;
    try { s = this.context.esp.add(4).readPointer().readCString(); } catch (e) {}
    push(['H', ++seq, name, step, this.context.ecx.toString(), a, s, bt(this.context)]);
    flush();
  } });
}
// FLEXEND_CALLS: follow the physics thread with Stalker; every call target not seen in the
// first WARM steps is reported once with the step it first ran in (an N row)
const seen = new Set();
let following = false;
const WARM = (typeof WARM_STEPS === 'number') ? WARM_STEPS : 240;
Interceptor.attach(ptr('0x52a990'), { onEnter() {
  step++; push(['A', ++seq, f(this.context.esp.add(4))]);
  if (typeof FLEXEND_CALLS !== 'undefined' && FLEXEND_CALLS) {
    if (!following) {
      following = true;
      Stalker.follow(this.threadId, { events: { call: true },
        onCallSummary(sum) {
          const add = [];
          for (const k in sum) if (!seen.has(k)) { seen.add(k); if (step > WARM) add.push([k, sum[k]]); }
          if (add.length) push(['N', ++seq, step, add]);
        } });
    } else Stalker.flush();
  }
} });
hook('0x4170e0', 'matchEnd', 2);
hook('0x416450', 'deactivateRobots', 0);
hook('0x4172d0', 'results', 2);
hook('0x405050', 'SetPinned', 3);
hook('0x405010', 'SetActive', 3);
hook('0x4c9000', 'ArenaActivate', 1);
let first = null, prev = null;
Interceptor.attach(ptr('0x52cc00'), { onEnter() {
  try {
    const t = this.context.ecx;
    if (first === null) first = t.toString();
    if (t.toString() !== first) return;
    const bb = t.add(0x24).readPointer();
    if (bb.isNull()) return;
    const cur = new Uint32Array(bb.readByteArray(0x200));
    if (prev !== null && cur[7] !== prev[7]) {
      // the movable test 0x4812c0 reads +0x1c: bits 0 and 2 clear = movable
      const all = [];
      for (let i = 0; i < 128; i++) if (cur[i] !== prev[i]) all.push([i * 4, prev[i], cur[i]]);
      push(['F', ++seq, step, prev[7], cur[7], all]);
    }
    if (prev !== null) {
      const d = [];
      for (let i = 0; i < 128; i++) if (cur[i] !== prev[i]) d.push([i * 4, prev[i], cur[i]]);
      if (d.length >= 1 && d.length <= 12) push(['X', ++seq, step, d]);
    }
    prev = cur;
    if (step % 30 === 0) push(['T', ++seq, step, vec(bb.add(0x30)), vec(bb.add(0x54))]);
  } catch (err) { push(['E', ++seq, String(err)]); }
} });
rpc.exports = { flush() { flush(); } };
