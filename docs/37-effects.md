# 37 — Per-hit effects: sparks, debris, decals, smoke (fx3)

What the original does visually when a robot is hit, from `Robot Arena 2.exe` (file VAs), a live
Frida log of a 80 s AI-vs-AI fight (`tools/oracle/frida/ra2_fx.js`, driver `tools/oracle/capture_fx.py`)
and photographs of the result. Evidence (git-ignored, backed up, manifest rows
`fx-axe-v-spikehammer`): `local/oracle/original/fx-axe-v-spikehammer__live__000.jsonl` (every hook row),
`...-pose00..07.png` (scripted camera, 1.9 m from each robot, AI stopped), `....spec.txt` (the decals and
poses in the form `ra2sim --example decalshot` reads). Particle internals (emitters, spawn, draw) are in
docs/31; this adds what the log corrected and the decal system, which docs/31 did not have.

## 1. The hit chain

A priced contact runs the struck component's virtual `+0x6c` (`FUN_00417510`, before the `>= 10` damage gate
of docs/35): `FUN_0045f3a0` (sparks + sound) for every class, overridden by

| class | function | extra |
|---|---|---|
| Chassis | `FUN_0044ee70` | base, then debris (below), then the decal (section 3) |
| Bracket | `FUN_00503e40` | base, then `bracket_hit.wav` |

`FUN_004a2a50(count, other material, record, debris flag)` is the only caller of the spark (`0x4a2c10`) and
debris (`0x4a2e60`) effects. The log of 52 chassis hits (hit record `rec`, `rec+0x2c` = the other
component):

* **Count.** Sparks and debris are only thrown when `damage > 10` and `GraphicEffects == 1`. The spark
  count is `ftol(FUN_0045f9f0(damage))`: a linear map of `10..100` to `3..hi`, clamped, `hi = 20` at
  `GraphicDetail = 2` and `10` at 0 or 1 (settings object `0x7164f8`: `+0x54` detail, `+0x58` effects).
  Logged damage -> count: 12.34 -> 3.44, 16.70 -> 4.27, 38.39 -> 8.36, 71.06 -> 14.53, 115.1 and 133.6 -> 20.
  docs/31 said `int(damage)`; that was wrong.
* **Sparks** (`0x4a2c10`): only if a metal part and no rubber part is in the contact; velocity `2 * rec.normal`,
  spread `(1, 1, 1)`, colour white, texture `spark.tga`, additive (`FUN_004a3380`, entry type 0).
* **Debris** (`0x4a2e60`, entry type 4): thrown by the **chassis** handler only (the second `4a2a50` call has
  the debris flag), metal or not, `int(0.25 * count)` pieces (0 to 5; `0x6a54c0`), velocity **`6 * rec.normal`**
  (the chooser scales its vector in place by 2 and then 3; docs/31's `3` was wrong), spread `(1, 1, 1)`,
  gravity `(0, -13, 0)`, random texture of `metal`, `metalslice`, `metal_nutbolt`. Nothing else calls
  the debris effect: the "debris when a part comes off" that `ra2sim` had is removed.
* **Direction.** `rec.normal` points *into* the struck component: `normal . (point - chassis origin) < 0` in
  52 of 52 chassis hits. `combat::Impact::normal` points out of it, so the sparks and debris take `-normal`.

Code: `ra2_render::effects::hit_count`, `Effects::{sparks, debris}` (`Effects::detail` from `GraphicDetail`),
`Duel::sparks_for`. Tests `the_hit_count_follows_the_live_log`, `a_chassis_hit_throws_debris`.

## 2. Smoke

Only two classes own a smoke emitter, both created by their `Init(2)` and switched on the first hit that
leaves them at or under half their hitpoints (`hp / max <= 0.5`; `FUN_0048e0d0` for motors, `FUN_004f6dd0`
for car steering, then `FUN_0048e0b0` = `SetEmitting(1)`; the emitter follows the component):

| class | creator | texture | velocity | spread | rate | life | size |
|---|---|---|---|---|---|---|---|
| Motor (spin, servo, burst) | `0x48dc50` | `smoke`, `smoke2`, `smoke3` (random) | `(0, 2, 0)` | `(.5, 0, .5)` | 5/s | 0.7 | 0.2 -> 0.6 |
| Steering (`carsteering*`) | `0x4f6a10` | `smoke` | `(0, 0.7, 0)` | `(.3, 0, .3)` | 5/s | 0.7 | 0.2 -> 0.6 |

Both gate on `GraphicEffects == 1`, colour white fading to alpha 0, alpha blended. New: the steering smoke
(`Config::steering_smoke`) and `<=` (it was `<`). No smoke comes from a hurt chassis, a weapon or the robot as
a whole. `Flamethrower` (`0x504080`) owns a flame emitter of its own (the hazard flame's configuration,
80/s, 0.3 s, blue -> white, `flame.tga`, additive) positioned at its nozzle: **not implemented**, the weapon's
gameplay is not in the sim (re-checked in the polish round, docs/36 9.21: still
not implemented; there is no firing state for an emitter to follow).

## 3. Decals (`DecalTrigger`, RTTI `0x6ef5e4`, constructor `0x5a0e00`, object `0x1cf4` bytes)

The loader reference `0x44f105` is `FUN_0044ee70`'s `Arenas\decals\` + file name. The decals are **battle
marks on the struck chassis**, not scorch marks on the arena. A weapon definition's
`decal = <file> <dx dy dz> <w> <h> <mode>` (the weapon at `+0x80` flag, `+0x84` file, `+0x94` direction,
`+0xa4/+0xa8` size, `+0xac` mode; parsed by `0x503190`) selects it: 33 stock definitions (axes and blades
`single_slash`/`double_slash` mode 1 or 0, spikes `pierce01/02`, hammers `batter01/02`, ...; `fire.tga` ships
but nothing references it). Files `Arenas/decals/*.tga`, 32x32 RGBA.

**Placement** (chain captured live: `0x51eb00` rigid inverse, `0x51e650` point, `0x51e6d0` / `0x51e600`
rotations, constructor arguments):

* no damage gate and no spark gate; only `GraphicEffects != 0` and a weapon with the flag;
* `c = inv(chassis object matrix) * contact point (rec + 0)`; `N = rotate(inv, -rec.normal)` (out of the
  chassis); `A = rotate(inv, weapon body orientation * direction)` (the width axis; not made
  perpendicular to `N`); half sizes `w/2`, `h/2`, depth `0.5` (`0x6f20c0`); height axis `V = N x A`;
* alpha (`+0x1cec`) `= clamp(damage / 100, 0, 1)` (`FUN_004194f0` over `0..100 -> 0..1`);
  logged 0.1234 for damage 12.34.

**Cut** (`FUN_005a1700` over the chassis' mesh object 0): each triangle whose unit face normal `n` (index
order `(p1-p0) x (p2-p0)`) has `n . N > 0.25` (`0x705f48`) is pushed `0.0005` (`0x6f20c4`) along `n`, clipped
by Sutherland-Hodgman against `A, -A, V, -V, N, -N` (`FUN_005a1970`/`FUN_005a1a80`) and appended as a fan
(`0, i-1, i`); a polygon that would pass 89 points ends the walk. Each point keeps its source triangle's three
indices and barycentric weights (sub-areas over the triangle's area, so pushed points sum a little over 1);
UV `= ((p-c).A / w + .5, (p-c).V / h + .5)`. `FUN_005a1ee0` re-evaluates positions from the mesh every frame,
so a decal rides a buckling chassis. An empty decal (box off the mesh) still counts. At most **25** per
chassis (`0x6f20c8`), the oldest removed first.

**Draw** (`FUN_005a2620`/`FUN_005a2240`, state object `0x71fd78`): alpha blending on, depth write off,
lighting on, vertex colour `(1, 1, 1, alpha)`, linear filter, CCW cull. `mode == 1` (slashes): `SRCALPHA, ONE`
(additive) with alpha test `>= 16`; any other mode: `SRCALPHA, INVSRCALPHA`, no alpha test (the dark dents).

**Evidence.** The original's chassis mesh is subdivided for the crush (84 / 206 triangles for chassis that are
12 / 52 in the `.bot`), so polygon counts differ; what matches is the footprint: for five logged boxes cut
from our meshes the texture-coordinate extents agree to 0.003 and the area to within 8 % (test
`decals_cut_from_the_chassis_match_the_live_footprints`). Photographs: `pose01..03` of the capture against
`decalshot` frames (`DECALSHOT_CHASSIS_ONLY=1`) show the same jagged grey dent with the green cross-hatch
(`batter01.tga`) on the same face; positions differ by the robots' pose drift while photographed. Not
verified live: the slash decals' additive look (they are faint, `single_slash.tga` alpha <= 102) and the
lighting factor.

Code: `ra2_render::decals` (parse, project, corners, material), `Duel::{mark_for, push_mark, mark_items}`,
`Component::decal`, `ra2sim --example decalshot`.

## 4. Other per-hit visuals found

* Hit sounds: docs/35. `FUN_0045f4c0` volume `> -1500` gates the sound, not a visual.
* Chassis crush: F9 (docs/06), the decals follow it.
* The hit does **not** call `plus.emitSmoke` (`FUN_004a3120`) or the flame; `emitSmoke` stays script-only.
* Hazard flames and lightning: docs/31, docs/34.

## 5. Gaps

Flamethrower flame; the exact per-vertex normals and lighting of a decal (ours uses the face normal); the
original's subdivided chassis mesh (decal polygons are the same shape, fewer fans); live look of the slash
(additive) decals; `0x71abcc` (the camera-relative decal direction branch) is `(0, 0, 0)` and no stock
definition uses a zero direction, so that branch is not implemented.
