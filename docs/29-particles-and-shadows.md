# Particles and shadows

What the original draws as particles and projected shadows, as recovered from
`Robot Arena 2.exe`, and where it lives here (`ra2-render/src/effects.rs`, hooks
in `ra2sim/src/duel.rs`, `ra2script/src/plus.rs`).

## ParticleManager (RTTI `0x6f53b0`)

An emitter is `FUN_005bab60` (`0x340` bytes, vtable `0x6af570`) built from a
config struct whose defaults are `FUN_005ba7b0`. Vtable: `+0x14` set position,
`+0x30` build vertex buffers (100 quads of `0x18`-byte vertices, FVF
`XYZ|DIFFUSE|TEX1`), `+0x1c`/`FUN_005bc8f0` update, `FUN_005bcf70` draw.
Config offsets (copied to emitter `+0xe8`): see the table in the module doc.

* **Spawn** `FUN_005bc250`: `life = life + spread * (r - 0.5)`; velocity per
  axis `v + spread * (r - 0.5)`; start colour per channel
  `c + spread * (r - 0.5)` clamped; size `s + spread * (r - 0.5)`. `r` is the
  engine LCG (`seed*0x41c64e6d + 0x3039`, 31 bits * `2^-31`, global seed at
  `0x726d58`). Default colour spread is `(0, 0.3, 0, 0)`, so green jitters.
* **Update** `FUN_005bc8f0`: spawn `rate * dt` accumulated, at most `max` alive;
  `pos += v dt + g dt^2 / 2`, `v += g dt`; progress `p = 1 - remaining / life`;
  past `fade_start` (default 0.5) colour and size blend linearly to the end
  values by `(p - fade) / (1 - fade)`; `p > 1` kills. Mode 2 textures animate
  every `0x88` seconds, mode 1 pick one per particle.
* **Draw** `FUN_005bcf70`: camera-facing quad, half-extent `size`, built from
  the first two columns of the view matrix; corners `+r+u, +r-u, -r-u, -r+u`,
  uv `(1,0) (1,1) (0,1) (0,0)`; diffuse = particle colour; lighting off, alpha
  blend, no alpha test (`FUN_005bb610`, `FUN_005b24c0`).
* **Blend** `FUN_004a3380` (`FUN_0048dfb0(type)` → material `+8`): type 0
  (`SRCALPHA, INVSRCALPHA`) for smoke, rubber and metal debris, type 1
  (`SRCALPHA, ONE`, additive) for sparks and flames.
* **One-shot effects** `FUN_004a3380`: the manager holds `< 20`; an effect
  emits for `0.1 s` (the entry's `+8` time) at `count / 0.1` per second.

| effect | function | count | life | speed | size | gravity | texture | blend |
|---|---|---|---|---|---|---|---|---|
| sparks | `FUN_004a2c10` via `FUN_004a2a50` | `int(damage)` | 0.7 | normal * 2, spread (1,1,1) | 0.05 | (0,-9.8,0) | `spark.tga` | additive, bounce |
| metal debris | `FUN_004a2e60` | `int(damage)` | 1.0 | normal * 3, spread (1,1,1) | 0.08 (+-0.01) | (0,-13,0) | `metal`, `metalslice`, `metal_nutbolt` | alpha, spin, bounce |
| smoke puff (`plus.emitSmoke`, `FUN_004b5bf0`) | `FUN_004a3120` | arg | 1.0 | arg | 0.3 -> 1.0 | 0 | `smoke`, `smoke2`, `smoke3` | alpha |
| motor smoke | `FUN_0048dc50` | 5/s standing | 0.7 | (0,2,0), spread (.5,0,.5) | 0.2 -> 0.6 | 0 | `smoke*` | alpha |
| flame (`plus.AddParticleEmitter`) | `FUN_004b5760` | 80/s standing | 0.3 (+-0.075) | args | 0.2 -> 0.4 | 0 | `flame.tga` | additive; colour blue -> white |

Sparks are thrown by `FUN_0045f3a0` only when `10 < damage` and `GraphicEffects`
is on (`FUN_0046e5e0() == 1`), and by `FUN_004a2a50` only when a `metal` part is
in the contact and no `rubber` one is. A motor under half its hitpoints switches
its emitter on (`FUN_0048e0d0` → `FUN_0048e0b0`).

Not recovered: the arena surface a particle bounces off (`FUN_005bdc00`
reflects `v - 2 (v.n) n`, times the elasticity at config `+0xf0` = 1, against a
callback this build does not name) and the flame's own collision branch. We
bounce sparks and debris off the arena's declared shadow receivers and let
flames pass through. The debris spin is an in-plane rotation (the original
builds a quaternion from an axis). `plus.emitSmoke`'s argument meaning is
unknown and no shipped script calls it.

## ShadowWizard (RTTI `0x6fe950`)

`SetShadowSource(x,y,z)` (`FUN_004ee460`) stores a point light, `AddShadowTriangle`
(`FUN_004ee490`) makes a `ShadowTrigger` receiver, `SetShadowPrimitive` stores an
object (`+0x30`, unused by the shipped boxes). `FUN_004ee700` runs every frame:
for each caster object it projects the caster's triangles from the light onto
each receiver plane (`ShadowTrigger +0x34` matrix), clips to the receiver
triangle (`FUN_0061cb10`) and draws a triangle fan (`FUN_005986a0`) with colour
`FUN_0061b4b0(0,0,0,0.3)` = black, alpha 0.3, lit material `0x71eff0`. Lit black
diffuse leaves only the ambient term, so the shadow is the ambient grey (38/255)
at 30 %: on `arena-box-f3` the floor (57,53,88) becomes (53,48,74), which is
`0.7 * floor + 0.3 * 38` to within a count. A receiver whose plane the caster
is behind is skipped; the original also rejects by a `0.3` test whose operand
was not identified (`FUN_0044d680`), which we replace with "faces the light".

We use each attached component's hull as a caster and lift the shadow `1 cm`
off the receiver.

## LightningField, DecalTrigger

Not recovered or implemented here; `CreateLightning`/`SetLightningVisible` stay
recorded as deferred.
