// Frida agent: the collision-event registrations the ORIGINAL makes when a
// match loads (docs/29 section 12l).  Function-entry hooks only.
//   R  [R,seq,caller,body,partner,interval,threshold,type]  0x52d850 entry
//      (the event manager's add-registration, thiscall; one entry appended to
//      the body's list per call): the return address, then the five args
//   C  [C,seq,caller,body,arg2]  0x414cd0 entry: the robot set-up's per-body
//      registration, called from 0x414a60's loop over a robot's components
//      and, inside it, over each component's bodies (consecutive C rows with
//      the same arg2 are one component's bodies, in the robot's component order)
'use strict';
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 128) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
const f = p => p.readFloat();
Interceptor.attach(ptr('0x52d850'), { onEnter() {
  try {
    const sp = this.context.esp;
    push(['R', ++seq, sp.readPointer().toString(), sp.add(4).readPointer().toString(),
          sp.add(8).readPointer().toString(), f(sp.add(12)), f(sp.add(16)), sp.add(20).readU32()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
Interceptor.attach(ptr('0x414cd0'), { onEnter() {
  try {
    const sp = this.context.esp;
    push(['C', ++seq, sp.readPointer().toString(), sp.add(4).readPointer().toString(),
          sp.add(8).readPointer().toString()]);
  } catch (e) { push(['E', ++seq, String(e)]); }
} });
rpc.exports = { flush() { flush(); return seq; } };
setInterval(flush, 300);
