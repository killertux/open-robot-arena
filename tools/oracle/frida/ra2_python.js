// Frida agent: run Python 2.2 code inside the original Robot Arena 2.
//
// The game embeds python22.dll and exposes its engine to scripts through the
// `plus` module (plus.getPlayers, plus.getLocation, plus.getTimeElapsed, ...).
// This agent resolves PyRun_SimpleString from python22.dll's PE export table
// (Frida's module list only shows the ELF side of a Wine process, so the PE
// image is found through its file-backed mapping) and calls it on the game's
// main thread, from inside the per-frame UI update FUN_005e4e30, where the
// interpreter is idle and the GIL is held by that thread.
//
// rpc.exports.exec(src)     queue src for the next frame; returns a ticket
// rpc.exports.result(t)     PyRun_SimpleString's return (0 ok, -1 error) or null
// rpc.exports.everyframe(src, n)  run src on every n-th frame (null/'' stops)
//
// Python output: write to a file under C:\ (the prefix's drive_c is visible
// from the host at ~/.var/app/org.winehq.Wine/data/wine/drive_c/).
'use strict';

const FRAME = ptr('0x5e4e30');

function findPeBase(dllName) {
  const want = dllName.toLowerCase();
  for (const r of Process.enumerateRanges('r--')) {
    if (r.file && r.file.path.toLowerCase().endsWith('/' + want) && r.file.offset === 0) {
      if (r.base.readU16() === 0x5a4d) return r.base;
    }
  }
  return null;
}

function peExports(base) {
  const pe = base.add(base.add(0x3c).readU32());
  const rva = pe.add(0x78).readU32();
  const dir = base.add(rva);
  const n = dir.add(0x18).readU32();
  const funcs = base.add(dir.add(0x1c).readU32());
  const names = base.add(dir.add(0x20).readU32());
  const ords = base.add(dir.add(0x24).readU32());
  const out = {};
  for (let i = 0; i < n; i++) {
    const name = base.add(names.add(4 * i).readU32()).readCString();
    const ord = ords.add(2 * i).readU16();
    out[name] = base.add(funcs.add(4 * ord).readU32());
  }
  return out;
}

const py = findPeBase('python22.dll');
if (py === null) throw new Error('python22.dll is not mapped');
const X = peExports(py);
const PyRun_SimpleString = new NativeFunction(X['PyRun_SimpleString'], 'int', ['pointer']);

const pending = [];
const results = {};
let ticket = 0, frames = 0, repeat = null, repeatN = 1;

Interceptor.attach(FRAME, { onEnter() {
  frames++;
  while (pending.length) {
    const job = pending.shift();
    results[job.t] = PyRun_SimpleString(Memory.allocUtf8String(job.src));
  }
  if (repeat && frames % repeatN === 0) PyRun_SimpleString(repeat);
} });

rpc.exports = {
  exec(src) { const t = ++ticket; pending.push({ t, src }); return t; },
  result(t) { return t in results ? results[t] : null; },
  everyframe(src, n) {
    repeat = src ? Memory.allocUtf8String(src) : null; repeatN = n || 1; return !!repeat;
  },
  frames() { return frames; },
  info() { return { python22: py.toString(), PyRun_SimpleString: X['PyRun_SimpleString'].toString(), exports: Object.keys(X).length }; },
};
