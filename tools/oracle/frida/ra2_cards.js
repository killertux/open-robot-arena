// Frida agent: the ORIGINAL's "robot out" path (docs/32 section 7).
// 0x418040 FUN_00418040(robot, state): 1 destroyed, 2 immobilised, 3 eliminated; 0x4e2660 the popup-list entry (card, seconds);
// 0x4b3850 the status meter's dim (flag), 0x4df120 the critical counter, 0x417cc0 the destroy.
'use strict';
const t0 = Date.now();
const log = o => { o.t = Date.now() - t0; send(o); };
Interceptor.attach(ptr('0x418040'), { onEnter(a) {
  const s = this.context.esp;
  log({ ev: 'out', robot: s.add(4).readPointer().toString(), state: s.add(8).readS32(), ecx: this.context.ecx.toString() });
} });
const meters = {};
const dump = (m, off) => {
  const w = m.add(off).readPointer();
  return { off, w: w.toString(), f: Array.from({ length: 0x58 }, (_, i) => w.add(i * 4).readFloat()),
           u: Array.from({ length: 0x58 }, (_, i) => w.add(i * 4).readU32()) };
};
// the armour setter runs every frame per player slot: remember the meter
Interceptor.attach(ptr('0x4b35a0'), { onEnter(a) { meters[this.context.ecx.toString()] = this.context.ecx; } });
Interceptor.attach(ptr('0x4b3850'), {
  onEnter(a) { this.m = this.context.ecx; this.flag = this.context.esp.add(4).readU8(); },
  onLeave(r) {
    log({ ev: 'dim', flag: this.flag, meter: this.m.toString(), armour: this.m.add(0xf0).readFloat(),
          armor: dump(this.m, 0xd8), white: dump(this.m, 0xdc), air: dump(this.m, 0xd4) });
  } });
rpc.exports = {
  poll() {
    return Object.values(meters).map(m => ({ meter: m.toString(), flag: m.add(0xec).readU8(), armour: m.add(0xf0).readFloat(),
      armor: dump(m, 0xd8), white: dump(m, 0xdc), air: dump(m, 0xd4) }));
  } };
Interceptor.attach(ptr('0x4df120'), { onEnter(a) { log({ ev: 'critical', robot: this.context.ecx.toString() }); } });
Interceptor.attach(ptr('0x417cc0'), { onEnter(a) { log({ ev: 'destroy', id: this.context.esp.add(4).readS32() }); } });
