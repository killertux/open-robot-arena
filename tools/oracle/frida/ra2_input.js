// Frida agent: inject mouse state into the original Robot Arena 2.
//
// The game polls its DirectInput8 mouse in FUN_005d66c0 (cdecl, one arg: a
// 20-byte out struct {int dx, dy, dz; bool buttons[8]}; returns AL=1 on
// success).  Under Wine the exclusive dinput mouse never sees synthetic X
// input, so we overwrite that struct on return.  Each poll consumes one queued
// state; with an empty queue the real (always idle) state passes through but
// the held-button mask is still applied.
'use strict';
const POLL_MOUSE = ptr('0x5d66c0');
// FUN_005e4e30 (per-frame UI/input update, thiscall) gates all mouse handling
// on the byte at this+0x14+0x1a (FUN_005e5250 is `mov al,[ecx+0x1a]`).  Under
// Wine without a window manager the game never sees itself activated and the
// byte stays 0, so the dinput mouse is never even polled.  Force it on.
const UI_UPDATE = ptr('0x5e4e30');
// The UI cursor position is two ints at this+0x14 / this+0x18 (FUN_0049f000
// returns this+0x14 as a POINT*).  setpos() pins it there every frame.
// The mouse-state object at this+0x14 has the layout FUN_004e6470 writes:
//   +0x00 int x, +0x04 int y, +0x14+i button i down, +0x17+i button i changed,
//   +0x1a moved-this-frame, +0x1b button-changed-this-frame;
// FUN_004e64a0 clears the per-frame flags at the end of FUN_005e4e30.
// frameEvents is a queue of per-frame button edges [{button, down}] applied on
// entry, one entry per UI frame.
let forceMouse = true, uiThis = null, pinned = null, uiFrames = 0, keepBtn = -1;
const frameEvents = [];
Interceptor.attach(UI_UPDATE, { onEnter() {
  uiThis = this.context.ecx; uiFrames++;
  const m = uiThis.add(0x14);
  if (pinned) { m.writeS32(pinned[0]); m.add(4).writeS32(pinned[1]); }
  if (forceMouse) m.add(0x1a).writeU8(1);
  const ev = frameEvents.length ? frameEvents.shift() : null;
  if (ev && ev.pos) { pinned = ev.pos; m.writeS32(pinned[0]); m.add(4).writeS32(pinned[1]); }
  if (ev && ev.button >= 0) {
    m.add(0x14 + ev.button).writeU8(ev.down ? 1 : 0);
    m.add(0x17 + ev.button).writeU8(1);
    m.add(0x1b).writeU8(1);
    if (ev.keep) keepBtn = ev.down ? ev.button : -1;
  }
  // `dragh`: the button stays flagged (+0x17 is what FUN_0042b810 reads: the editing area's drag handler
  // moves a point only while it is set) on every frame between the press and the release.
  if (keepBtn >= 0) { m.add(0x14 + keepBtn).writeU8(1); m.add(0x17 + keepBtn).writeU8(1); }
} });
let camHook = null, camLast = null, camSeen = [];
const queue = [];          // [{dx, dy, dz, buttons}]
let held = 0;              // button bitmask held between queue entries
let polls = 0, lastReal = null;

Interceptor.attach(POLL_MOUSE, {
  onEnter(args) { this.out = this.context.esp.add(4).readPointer(); },
  onLeave(retval) {
    polls++;
    const out = this.out;
    const ok = (retval.toInt32() & 0xff) !== 0;
    if (ok) lastReal = [out.readS32(), out.add(4).readS32(), out.add(8).readS32()];
    const e = queue.length ? queue.shift() : null;
    if (!e && held === 0) return;
    const st = e || { dx: 0, dy: 0, dz: 0, buttons: held };
    out.writeS32(st.dx | 0); out.add(4).writeS32(st.dy | 0); out.add(8).writeS32(st.dz | 0);
    for (let i = 0; i < 8; i++) out.add(12 + i).writeU8((st.buttons >> i) & 1);
    held = st.buttons;
    retval.replace(ptr((retval.toInt32() & ~0xff) | 1));
  }
});

rpc.exports = {
  // queue n polls of motion (dx,dy split evenly) with the current button mask
  move(dx, dy, steps) {
    steps = steps || 1;
    let sx = 0, sy = 0;
    for (let i = 1; i <= steps; i++) {
      const nx = Math.round(dx * i / steps), ny = Math.round(dy * i / steps);
      queue.push({ dx: nx - sx, dy: ny - sy, dz: 0, buttons: held });
      sx = nx; sy = ny;
    }
    return queue.length;
  },
  // queue a button state for n polls (mask: 1=left, 2=right, 4=middle)
  buttons(mask, polls_) {
    for (let i = 0; i < (polls_ || 1); i++) queue.push({ dx: 0, dy: 0, dz: 0, buttons: mask });
    return queue.length;
  },
  wheel(dz) { queue.push({ dx: 0, dy: 0, dz: dz, buttons: held }); return queue.length; },
  pending() { return queue.length; },
  stats() { return { polls, held, pending: queue.length, frameEvents: frameEvents.length, uiFrames, lastReal, ui: uiThis && uiThis.toString() }; },
  // press/release button b (0 left, 1 right, 2 middle) at x,y over the next
  // UI frames: hold for `hold` frames between the edges.
  click(x, y, b, hold) {
    pinned = [x | 0, y | 0]; b = b | 0; hold = hold || 3;
    frameEvents.push({ button: -1 }, { button: -1 });
    frameEvents.push({ button: b, down: true });
    for (let i = 0; i < hold; i++) frameEvents.push({ button: -1 });
    frameEvents.push({ button: b, down: false });
    return frameEvents.length;
  },
  // press button b at (x0,y0), move to (x1,y1) over `steps` frames with it held, release
  drag(x0, y0, x1, y1, b, steps, keep, hold) {
    b = b | 0; steps = steps || 10; keep = !!keep; hold = hold | 0;
    frameEvents.push({ button: -1, pos: [x0 | 0, y0 | 0] }, { button: -1 });
    frameEvents.push({ button: b, down: true, keep });
    for (let i = 1; i <= steps; i++)
      frameEvents.push({ button: -1, pos: [Math.round(x0 + (x1 - x0) * i / steps), Math.round(y0 + (y1 - y0) * i / steps)] });
    for (let i = 0; i < 1 + hold; i++) frameEvents.push({ button: -1 });
    frameEvents.push({ button: b, down: false, keep });
    if (keep) {
      // the editing area's move handler reads the raw (dinput) button state, not the UI object's
      for (let i = 0; i < 2; i++) queue.push({ dx: 0, dy: 0, dz: 0, buttons: 0 });
      for (let i = 0; i < steps + 2 + hold; i++) queue.push({ dx: 0, dy: 0, dz: 0, buttons: 1 << b });
      queue.push({ dx: 0, dy: 0, dz: 0, buttons: 0 });
    }
    return frameEvents.length;
  },
  // The lab's 3D view camera (FUN_005a6260, thiscall, run every frame by the active tab's mouse read 0x48d130):
  // hook it for a moment, take the object it ran on last and write pitch (+0x1e0), yaw (+0x1e4) and, when given,
  // distance (+0x1d0), then detach again (a hook left behind wedges frida-server). The orbit by the middle button
  // does not reach the game through the injected state (tried: raw poll mask, UI state, both), so this is how
  // the scripted builds frame the robot; ours has the same step (labshot `cam`).
  camarm() {
    if (camHook) camHook.detach();
    camSeen = [];
    camHook = Interceptor.attach(ptr('0x5a6260'), { onEnter() { const c = this.context.ecx; if (!camSeen.some(x => x.equals(c))) camSeen.push(c); } });
    return true;
  },
  camset(pitch, yaw, dist) {
    if (camHook) { camHook.detach(); camHook = null; }
    // the lab's view is the object whose viewport (+0x190 / +0x194) is the 471 px square pane
    const view = camSeen.filter(c => Math.abs(c.add(0x190).readFloat() - 471) < 2);
    camLast = view.length ? view[view.length - 1] : null;
    if (!camLast) return camSeen.map(c => [c.toString(), c.add(0x190).readFloat(), c.add(0x194).readFloat()]);
    camLast.add(0x1e0).writeFloat(pitch); camLast.add(0x1e4).writeFloat(yaw);
    if (dist > 0) camLast.add(0x1d0).writeFloat(dist);
    return camLast.toString();
  },
  frames() { return uiFrames; },
  setpos(x, y) { pinned = [x | 0, y | 0]; return pinned; },
  release() { pinned = null; return true; },
  getpos() { return uiThis ? [uiThis.add(0x14).readS32(), uiThis.add(0x18).readS32()] : null; },
  forcemouse(on) { forceMouse = !!on; return forceMouse; },
  read(addr, n) { return Array.from(new Uint8Array(ptr(addr).readByteArray(n))); },
};
