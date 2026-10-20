// One-off probe: which music-layer calls run at screen/match transitions?
//   rows [addr, ms, ecx, a1, a2, a3]
'use strict';
const A = ['0x4f12b0','0x4f0630','0x4f1330','0x4f15f0','0x4f1620','0x4f1600','0x629760','0x629910','0x629cf0','0x4f12d0','0x4f1350','0x4f1550'];
let rows = [];
for (const a of A) {
  Interceptor.attach(ptr(a), { onEnter() {
    const sp = this.context.esp;
    rows.push([a, Date.now(), this.context.ecx.toString(), sp.add(4).readU32(), sp.add(8).readU32(), sp.add(12).readU32()]);
    if (rows.length > 100) { send({ ev: 'batch', rows }); rows = []; }
  } });
}
rpc.exports = { flush() { send({ ev: 'batch', rows }); rows = []; } };
setInterval(() => { if (rows.length) { send({ ev: 'batch', rows }); rows = []; } }, 200);
