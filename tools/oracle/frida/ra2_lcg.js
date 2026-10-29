// Frida agent: every draw from the engine LCG of the ORIGINAL (docs/36 section 9.23).
// The state is the dword at 0x726d58; the step is state = state * 0x41c64e6d + 0x3039. Every
// instruction that stores it is hooked: the shared step at 0x419230 (FUN_00419220, called by the
// range draw FUN_00419190(n) = floor(abs(state) / 2^31 * n)), the seeds (0x4e47b4 from
// GetTickCount, 0x61bb3a from time(0)) and the inline copies in the particle / effect code
// (0x59ec56 ... 0x5be167). send(['D', seq, site, state, caller, ms]) per draw; `caller` is the
// return address of the draw helper's caller (FUN_00419190's caller, normally).
'use strict';
const STATE = ptr('0x726d58');
const SITES = [0x419230, 0x4e47b4, 0x59ec56, 0x59ed84, 0x5bb77f, 0x5bc2ea, 0x5bc336, 0x5bc36c,
  0x5bc3a0, 0x5bc3c8, 0x5bc584, 0x5bc5d8, 0x5bc659, 0x5bc699, 0x5bc6b9, 0x5bc770, 0x5bc7bc,
  0x5bdf80, 0x5bdfad, 0x5be035, 0x5be05e, 0x5be0e1, 0x5be10a, 0x5be13e, 0x5be167, 0x61bb3a, 0x61d270];
let seq = 0, rows = [];
function push(r) { rows.push(r); if (rows.length >= 64) flush(); }
function flush() { if (rows.length) { send({ ev: 'batch', rows: rows }); rows = []; } }
// The draw helper's frame (hook at 0x419230, inside FUN_00419220): [esp] 0x4191db, then
// FUN_004191d0's frame; called from FUN_00419190 (return 0x4191bd) the caller of that is at
// [esp+0x1c]; called directly (0x483a33) the return address is at [esp+0x10].
function caller(ctx) {
  try {
    const sp = ctx.esp;
    const r = sp.add(0x10).readU32();
    const v = (r === 0x4191bd) ? sp.add(0x1c).readU32() : r;
    return '0x' + v.toString(16);
  } catch (e) { return 'err:' + e; }
}
for (const s of SITES) {
  Interceptor.attach(ptr(s), { onEnter() {
    const reg = (s === 0x5bb77f) ? this.context.esi : this.context.eax;
    const st = reg.toInt32() >>> 0;
    push(['D', ++seq, '0x' + s.toString(16), st, s === 0x419230 ? caller(this.context) : '-', Date.now()]);
  } });
}
rpc.exports = {
  flush() { flush(); return seq; },
  state() { return STATE.readU32(); },
  setstate(v) { STATE.writeU32(v >>> 0); return STATE.readU32(); },
  mark(label) { push(['M', ++seq, String(label), STATE.readU32(), '-', Date.now()]); flush(); return seq; },
};
push(['S', 0, 'attach', STATE.readU32(), '-', Date.now()]);
setInterval(flush, 300);
