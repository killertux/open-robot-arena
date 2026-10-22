// Frida agent: the ORIGINAL's ShadowWizard::Update (FUN_004ee700) decisions, one frame at a time (docs/34 section 11).
// An ordered event log per frame: plane builds from three points (FUN_0061d9c0: the caster's first triangle, then
// each receiver's), receiver updates (FUN_004eeb50), receivers that passed the facing test (FUN_0061d970: normal, d),
// ray/plane tests (FUN_0061c4b0: ray origin/dir, plane normal/d, hit parameter, return) and fan triangles drawn (FUN_005986a0).
'use strict';
const f = (p, o) => +p.add(o).readFloat().toFixed(4);
let frame = null, nframes = 0;
const maxFrames = 2;
Interceptor.attach(ptr('0x4ee700'), {
  onEnter() { frame = []; },
  onLeave() { if (frame && nframes < maxFrames) { nframes++; send({ frame: nframes, ev: frame }); } frame = null; }
});
Interceptor.attach(ptr('0x61d9c0'), { onEnter(a) { if (frame) frame.push(['plane3', [0, 1, 2].map(i => [0, 4, 8].map(o => f(a[i], o)))]); } });
Interceptor.attach(ptr('0x4eeb50'), { onEnter() { if (frame) frame.push(['recv', this.context.ecx.toString()]); } });
Interceptor.attach(ptr('0x61d970'), { onEnter(a) { if (frame) frame.push(['pass1', [0, 4, 8].map(o => f(a[0], o)), f(a[0], 12)]); } });
Interceptor.attach(ptr('0x61c4b0'), {
  onEnter(a) { this.ray = a[0]; this.pl = a[1]; this.out = a[2]; },
  onLeave(r) {
    if (!frame) return;
    frame.push(['ray', [4, 8, 12].map(o => f(this.ray, o)), [16, 20, 24].map(o => f(this.ray, o)), [4, 8, 12].map(o => f(this.pl, o)), f(this.pl, 16), f(this.out, 0), r.toInt32() & 0xffff]);
  }
});
Interceptor.attach(ptr('0x5986a0'), { onEnter() { if (frame) frame.push(['draw']); } });
