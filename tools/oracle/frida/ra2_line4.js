// Frida agent: the Bot Lab's header line 4 (docs/36 9.32). `FUN_004df560` (called by the .bot text writer `FUN_004dd8c0` for the
// line after `Class:`) is `find_if(parts of the robot object, !part->flag)` over the vector at robot+0x1c / +0x20, the flag the byte
// `+0xf0` (read by `0x45d3b0`, the predicate at `0x4df5c0`): the line is 1 when no part has it 0. One `enter` (the robot object,
// the vector's begin / end), the `elem`s the search visited (vtable, +0xf0 / +0xf1 / +0xf2) and the `leave` (the line) per call.
'use strict';
Interceptor.attach(ptr('0x4df560'), {
  onEnter() { const th = this.context.ecx;
    const b = th.add(0x1c).readU32(), e = th.add(0x20).readU32();
    send({ ev: 'enter', t: Date.now(), th: th.toString(), b: b.toString(16), e: e.toString(16), g: ptr('0x716108').readU8() }); },
  onLeave(r) { send({ ev: 'leave', ret: r.toInt32() & 0xff }); } });
Interceptor.attach(ptr('0x45d3b0'), { onEnter() { const e = this.context.ecx;
  send({ ev: 'elem', e: e.toString(), vt: e.readU32().toString(16), f0: e.add(0xf0).readU8(), f1: e.add(0xf1).readU8(), f2: e.add(0xf2).readU8(), ec: e.add(0xec).readU32() }); } });
