// Frida agent: the `.bot` forward heading the ORIGINAL holds for each robot (docs/34 section 11.5).
// plus.getHeading(p) is `FUN_004df320(FUN_00414780(manager 0x71466c, p), 0)` (0x4b9500): rpc `robots()` walks the players
// the same way and returns each robot's object, its +0xac float (the heading the loader reads for versions >= 1.1)
// and the heading the original reports.
'use strict';
const manager = () => ptr('0x71466c').readPointer();
const robotOf = new NativeFunction(ptr('0x414780'), 'pointer', ['pointer', 'int'], 'thiscall');
const getHeading = new NativeFunction(ptr('0x4df320'), 'float', ['pointer', 'int'], 'thiscall');
// the bot loader FUN_004de450 (ecx = the robot being loaded): +0xac before and after, and the version it reads
const loads = [];
Interceptor.attach(ptr('0x4de450'), {
  onEnter() { this.r = this.context.ecx; this.before = +this.r.add(0xac).readFloat().toFixed(5); },
  onLeave() { loads.push({ robot: this.r.toString(), before: this.before, after: +this.r.add(0xac).readFloat().toFixed(5), ver: +ptr('0x71f6a0').readFloat().toFixed(3) }); },
});
// the setter FUN_00401170 (`this+0xac = arg`): who sets a heading
const sets = [];
Interceptor.attach(ptr('0x401170'), { onEnter() {
  sets.push({ robot: this.context.ecx.toString(), v: +this.context.esp.add(4).readFloat().toFixed(5), from: this.returnAddress.toString(), bt: Thread.backtrace(this.context, Backtracer.FUZZY).slice(0, 8).map(x => x.toString()) });
} });
const wraps = [];
Interceptor.attach(ptr('0x401150'), { onEnter() {
  const c = this.context.ecx;
  wraps.push({ comp: c.toString(), vt: c.readPointer().toString(), p2c: c.add(0x2c).readPointer().toString(), a40: +c.add(0x40).readFloat().toFixed(5), a3c: +c.add(0x3c).readFloat().toFixed(5), a44: +c.add(0x44).readFloat().toFixed(5) });
} });
rpc.exports = {
  wraps() { return wraps; },
  sets() { return sets; },
  loads() { return loads; },
  robots(n) {
    const out = [];
    for (let p = 0; p < n; p++) {
      try {
        const r = robotOf(manager(), p);
        out.push({ player: p, robot: r.toString(), h0xac: r.isNull() ? null : +r.add(0xac).readFloat().toFixed(5), heading: r.isNull() ? null : +getHeading(r, 0).toFixed(5) });
      } catch (e) { out.push({ player: p, err: String(e) }); }
    }
    return out;
  },
};
