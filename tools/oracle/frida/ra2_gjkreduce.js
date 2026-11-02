// Frida agent: run the ORIGINAL's Johnson reduction (FUN_0055f100, docs/29 12an) natively on
// given simplexes, on the game's own thread with its own x87 control word, and send the results.
// CASES = JSON file drive_c/ra2oracle/reduce_cases.json (seen as /var/data/wine/... inside the flatpak sandbox): [[[x,y,z],...], ...] (2..4 points each).
//   C  [C, cw]                       the thread's x87 control word (fnstcw)
//   K  [K, i, ret, count, A[4], B[4], lambda[4], w[4][3]]   tags: A = 100+i, B = 200+i
// The cases run once, inside the first call of 0x55c390 (the distance query, the physics thread).
'use strict';
const CASES = JSON.parse(File.readAllText('/var/data/wine/drive_c/ra2oracle/reduce_cases.json'));
let done = false;
const reduce = new NativeFunction(ptr('0x55f100'), 'int', ['pointer', 'pointer', 'pointer']);
const cwCode = Memory.alloc(Process.pageSize);
Memory.protect(cwCode, Process.pageSize, 'rwx');
{
  const w = new X86Writer(cwCode, { pc: cwCode });
  w.putPushReg('eax');
  w.putBytes([0xd9, 0x3c, 0x24]);       // fnstcw [esp]
  w.putPopReg('eax');
  w.putRet();
  w.flush();
  w.dispose();
}
const getCw = new NativeFunction(cwCode, 'int', []);
Interceptor.attach(ptr('0x55c390'), { onEnter() {
  if (done) return;
  done = true;
  send(['C', getCw() & 0xffff]);
  const mem = Memory.alloc(0x400);
  const WA = mem, ST = mem.add(0x100), LM = mem.add(0x200);
  const out = [];
  CASES.forEach((c, i) => {
    mem.writeByteArray(new Array(0x400).fill(0));
    c.forEach((p, k) => { WA.add(16 * k).writeFloat(p[0]); WA.add(16 * k + 4).writeFloat(p[1]); WA.add(16 * k + 8).writeFloat(p[2]); });
    for (let k = 0; k < 4; k++) { ST.add(4 * k).writeU32(100 + k); ST.add(16 + 4 * k).writeU32(200 + k); }
    ST.add(0x20).writeU32(c.length);
    const ret = reduce(WA, ST, LM) & 0xff;
    const A = [], B = [], L = [], W = [];
    for (let k = 0; k < 4; k++) { A.push(ST.add(4 * k).readU32()); B.push(ST.add(16 + 4 * k).readU32()); L.push(LM.add(4 * k).readFloat()); W.push([WA.add(16 * k).readFloat(), WA.add(16 * k + 4).readFloat(), WA.add(16 * k + 8).readFloat()]); }
    out.push(['K', i, ret, ST.add(0x20).readU32(), A, B, L, W]);
    if (out.length >= 200) { send(['K*', out.splice(0)]); }
  });
  if (out.length) send(['K*', out]);
  send(['done', CASES.length]);
} });
