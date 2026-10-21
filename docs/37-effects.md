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
a whole. The `Flamethrower` component's flame is section 6.

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
(`batter01.tga`) on the same face; positions differ by the robots' pose drift while photographed. Slash decals' look: section 8.

Code: `ra2_render::decals` (parse, project, corners, material), `Duel::{mark_for, push_mark, mark_items}`,
`Component::decal`, `ra2sim --example decalshot`.

## 4. Other per-hit visuals found

* Hit sounds: docs/35. `FUN_0045f4c0` volume `> -1500` gates the sound, not a visual.
* Chassis crush: F9 (docs/06), the decals follow it.
* The hit does **not** call `plus.emitSmoke` (`FUN_004a3120`) or the flame; `emitSmoke` stays script-only.
* Hazard flames and lightning: docs/31, docs/34.

## 5. Gaps

The flame's hit rate against distance (section 6), the exact per-vertex normals and lighting of a decal (ours uses the face normal); the
original's subdivided chassis mesh (decal polygons are the same shape, fewer fans); live look of the slash
(additive) decals (section 8); `0x71abcc` (the camera-relative decal direction branch) is `(0, 0, 0)` and no stock
definition uses a zero direction, so that branch is not implemented.

## 6. The Flamethrower (`0x504080`, vtable `0x6ab664`, RTTI `Flamethrower`)

Live: `tools/oracle/frida/ra2_flame.js` via `tools/oracle/capture_flame.py` (captures `flame-open__live__000`,
`flame-wait__live__000/001`, side-by-sides kept outside the oracle directory), driven by a test robot made
with `ra2sim --example flamebot` (a Scout, a Flamethrower on its nose turned half a turn so it points forward, a
CO2 tank, an analog `Flame` wired to `Fire`, a button `FlameBurst` wired to `Fire burst`). **No stock robot,
AI or team has one** (`Flame Chopper` in `AI/Bindings.py` is only a name); the definition is
`Components\flamethrower.txt`: `base = Flamethrower`, `standard = 0`, `hidden = 2`,
`firesound = sounds\flame_thrower.wav`, `airmaxinoutrate = -100`, `elecmaxinoutrate = -100`.

**Controls.** The constructor registers two handlers: analog `Fire` (`0x504a70(on, level)`, scale 1) and
button `Fire burst` (`0x504b60`, scale 100). `Fire` switches the jet on while the control is non-zero, `level`
being its value; `Fire burst` is `Fire(1, 100)` plus `burst = 1` and a `0.1 s` timer, and a release does nothing.
The step (`0x504790`) counts the timer down and, when it has run out and the emitter is on, switches it off.
Live: a burst's emitter was on 0.10-0.20 s (the original steps at 1/30 s; ours at 1/60 s gives 0.10-0.12 s).
(`AI.Input` from the in-game Python reaches the wires, but a call made inside another Frida hook's callback is
invisible to the hooks, so the live handler calls were made from the agent's own thread.)

**Power.** The jet draws `airmaxinoutrate` and `elecmaxinoutrate` times `dt` (100 each a second, **whatever the
level**) while it is on or its burst timer runs (`0x504c50`). The emitter starts only while both published
power scales (`0x714a6c` air, `0x714a70` electric, docs/30 section 3) are above zero, and its speed scale is
`level * 0.01 * air scale`, **fixed when it starts**. Consequences, all live: with no air tank the emitter never
starts (the tankless Scout: ticks with `firing = 1`, `emitting = 0`); with a CO2 tank (800, 120 a second) the
first jet has scale 1.0 and, as the tank runs down, later starts had 0.44 (level 50), 0.30 (level 20) and 0.24
(a burst), so one tank is about eight seconds of full flame and then a weak one. A level change while on only
changes the sound.

**The emitter** (`Arenas\particles\flame.tga`, named "Fire burst"; config dump of the live emitter): 80 a second,
life 0.3 +- 0.075 s, velocity `(0, 0, -5) * scale` in the component's frame rotated to the world **plus the
chassis' velocity**, spread `(0.5, 0.5, 0.5)`, no gravity, colour blue `(0, 0, 1, 1)` to white (green spread
0.3) and size 0.2 to 0.4 **over the whole life** (`fade_start` 0, not the default 0.5: the live dump has 0 at
`+0x134`; `Config::jet`), additive, collides with the world without reflecting, elasticity 1. It sits at
`component * (0, 0.176, -0.9)` (live world matrix: component at `(0, 0.0037, 0.5491)` in the robot's frame,
rotation `diag(-1, 1, -1)`, nozzle `(0, 0.180, 1.449)`: our `ra2fmt::mount` placement agrees to 1e-3). With
`angle = 0` the jet points out of the back of the chassis; the test robot uses half a turn. Config `ra2_render::
effects::Config::{flame, JET_SPREAD}`, `Effects::{jet, step_with}`.

**Damage.** Each particle's step is cast against the world (`0x5bdc00`; `0x504b90` skips the jet's own robot);
a hit calls `0x504ca0`, which for a robot's part is `FUN_004148f0(robot, component, 2.0)` (the hazard path: no
attacker, no score, a floating `2` shown): **2.0 hitpoints a hit**, applied below the 10-point gate of a contact
because `0x4148f0` reaches `0x4175b0` directly. The particle then leaves the surface in a random direction at
its own speed (the non-reflect branch of `0x5bdc00`). Live (`flame-wait__live__000`, ALARM driven into the jet
3.3 m away, level 100): 16 damage events, all `amount 2.0`, `attacker -1`, from `0x4149b3`, in clusters of 2-3
a frame while ALARM was in reach; ALARM lost 22 hitpoints (component materials scale a hit). With ALARM at
2.1 m, so that the nozzle was inside it, only 1-2 hits (a cast that starts inside a body hits nothing; ours
likewise: `duel::ray_box_entry`). Ours (`Duel::update_flamethrowers`, `apply_hit_with`) gives 1.75 hits a 60 Hz
step at 3.3 m against about 2.6 a frame live: the right order, **the rate against distance is not
calibrated** (the live robots would not hold still at a chosen range).

**Sound.** The handle is created in `Init(0)`: `sounds\flame_thrower.wav` at volume 0, 3D, 100/20. Every `Fire`
call does `SetVolume(20 * level - 2000)` hundredths of a dB (0, -1000, -1600 at 100, 50, 20: live) and `Stop`
at off, but the `Loop` is gated on the power scales being above zero at the call, which they are not until the
jet's own draw has been asked for, so **a jet switched on from rest is silent** (no `Loop` in any live call).
`ra2sim::flame::Sound`, `SoundCue::Flame`.

**Look.** Live (`flame-cam__live__000-02/03.png`) against ours (`ra2-app --example flameshot`, same camera, HUD,
hazards off, AIs stopped); side-by-sides are kept in the scratchpad, not archived. The live camera was polled
(`ra2_camera.js`: pitch 0.2326, FOV 0.8, no target, eye `a + 1.3 f + 1.9 r + 0.6 up`); ours uses the same formula and
the same pitch. Pixels (1024x768, live / ours): cylinder tip `(148, 458)` / `(149, 454)`, cylinder rows 275-478 /
274-478; first pink puff x 635 / 629; bright flame x from 721 / 695, rows 421-610 / 373-606, centroid `(890, 512)`
/ `(890, 492)`; the Time box and the Scout/ALARM boxes are the same size. Remaining difference: ours spreads
about 20-40 px more upward at the far end. The first comparison was wrong because of the comparison (hazards on,
AIs running, HUD laid out for 1280x1005 and so drawn at 75%; `Session::set_layout` fixes that; the "Static 1" text
is the 3 s camera-name label every camera switch shows in normal play, now switched off before the shot). The
colour needed the `fade_start` 0 above. The red cylinder and fork are the Scout's own `BurstMotor` and its arm. The
original draws **no model** for the Flamethrower: with `RA2_FLAME_MODEL=1` ours draws the gun at the nose, covering
the cylinder, and live shows the bare cylinder, so ours hides it (`rig.rs`).

**Code.** `ra2sim::flame` (state machine, tests), `controls::Action::{Flame, FlameBurst}`,
`Robot::{flame_inputs, flame_update, flame_jets}`, `Duel::update_flamethrowers`; workshop wiring accepts
`Fire` on an analog and `Fire burst` on a button for a Flamethrower. Tests: `flame::tests::*`,
`duel::tests::{a_flamethrower_with_no_air_tank_never_lights, a_jet_burns_the_other_robot_for_two_a_hit_and_scores_nothing,
a_burst_is_a_tenth_of_a_second_of_flame, the_jet_flies_forward_at_five_metres_a_second_at_level_100}`,
`effects::tests::a_flamethrower_jet_follows_its_nozzle_and_scatters_on_a_hit`. `--example flameshot` photographs
ours with the live camera.

**Open.** Hit rate against distance the scatter direction law; the `Loop` when other air
draws keep the scales up; AI brains that fire it (none ship); the floating `2` is section 7.

## 7. Floating damage numbers (`ArenaManager::DamageDisplay`)

Live: `tools/oracle/frida/ra2_numbers.js` via `tools/oracle/capture_numbers.py` (human C.R.U.S.H. against a
stopped ALARM in the Combat Arena, hazards off; `plus.damage(0, 0, 25, point)` and wall hits; captures
`numbers-hazard__live__000`, `numbers-hazard123__live__000`; a fast burst grabber, `xgrab.py`, takes ~20 frames a
second). Code: `ra2sim::popup`, `Match::{route_popups, damage_numbers, set_damage_messages}`,
`UiAssets::centered_text`, `Session::add_damage_numbers`; `--example numbershot` photographs ours.

**Who sees one.** `FUN_004175b0` (the hit handler: contacts of 10 or more, `FUN_004148f0` for `plus.damage`, a flame
and hazards) calls `FUN_004177e0` -> `FUN_004178a0`, which asks the display (`FUN_00417940`) only when the player
object's `vtable[0x18]` is true: **a human player**. Live: a request for the AI (`id 1`) was logged and never accepted
(`0x417940` not called), the human's were. Outside King of the Hill (the whole block is the not-KOTH branch):

| case | player | colour | position |
|---|---|---|---|
| a hit with an attacker (the scoring hits: victim alive, not a team-mate in Team Match) | the attacker | green `(0, 0.8, 0)` | `point + 0.1 * normal` |
| a hit with no attacker (hazard, `plus.damage`, flame, a wall) | the robot struck | red `(0.8, 0, 0)` | `point - 0.1 * normal` |

`point` and `normal` are the hit record's (`param_6`, `param_7`); the normal points into the struck component (live:
wall at x = 12.67 gave `(-1, 0, 0)`, at z = -12.4 `(0, 0, 1)`); a hazard's is zero. The amount is the **effective**
damage (`FUN_00489620`) and the text its truncation (`__ftol`): `25` for 25, `34` for 34.297, `123` for 123.9.
Not shown: `GraphicMessages = 0` (settings `+0x60`, the options window's "damage" switch; checked at the flush),
King of the Hill, hits on or by AIs, damage of zero.

**Merging** (`FUN_00417940`). A request joins an entry of the same player and colour that is younger than 0.15 s
(`DAT_006ef568`) and whose point is closer than sqrt(2) m (`DAT_006ef56c = 2.0` on the squared distance), adding the
amount; the entry keeps its first point and time. `FUN_00413b20` (once a frame, the game's dt) turns every entry older
than 0.15 s into a number: live flush 0.17-0.2 s after the request.

**The number** (`FUN_00413bd0`). A text object (`0x198` bytes, size `0x4b`, scale `2 / width`) of the face `fg_d20`
(`s_fg_d20_006ef624`), drawn as screen text **1:1 in pixels** at the projection of its world point: the `25` is 21 x 13
px of ink at 0.8 m and at 3.9 m alike. A mover (`0x5a3780(a, b, 0.25, 0.75)`) with `a` = the start above, `b = a + dir`,
`dir = normalize((0, 6, 0) -/+ normal)` (red `-`, green `+`; `(0, 1, 0)` for a hazard): nothing moves until 0.25 s, then the
text goes along `dir` at 2 m/s (the interpolation keeps extrapolating after 0.75 s; live y: -1.307 at 0.25 s, -0.307 at
0.75 s, +0.276 at 1.05 s). A fader (`0x5a3c00((c, 1), (c, 0), 0.75, 1.0)`) keeps it opaque until 0.75 s and fades the alpha
linearly to 0 at 1.0 s; the effect's duration is 1.0 s (`FUN_004141a0(1.0)`), then it is removed. It is a scene object
without depth test: it shows over the robot it is on.

**Anchor.** The projection is the camera's own (`eye`, `target`, the live FOV; the human's chase camera looks at its
robot, which is why `setCameraRotation` alone did not predict it). Against the projected point the ink sits 1 px right and
5.5 px up of a line box centred on it (`DAMAGE_NUMBER_OFFSET`): live ink x 471-491, y 200-212 for the point projecting to
(480.0, 211.6); x 493-513, y 262-274 for (502.8, 272.9). `123` is centred the same way (ink x 465-495).
Ours, same camera and point (`numbershot`): ink 470-490 x 206-218 before the offset (122 px of ink in both, the same
glyph cells), 471-491 x 200-212 after. Side by side in the scratchpad (`ui-numbers-side.png`: live | ours, the robots
differ).

**Gap found on the way.** The original prices the robot's contacts with the arena (walls) too, as a hit with no attacker:
live 34-40 hp on a wall at about 10 m/s (`request id 0, amount 34.297, v2 (0, 0, 1)`), 12-19 on smaller bumps, each a red
number. `combat::resolve` only prices robot against robot, so ours takes no wall damage and shows no such number. Open.

**Open.** A green number was not captured (the human never landed a hit on ALARM in the live attempts; the attacker
branch is from the disassembly: `FUN_004178a0(attacker, amount, ..., 1)`, colour and position code at `0x413d1f`); the
original's multi-player (network) humans.

## 8. Slash decals: the live look (fx-audio)

The slashes (`single_slash.tga`, mode 1: additive `SRCALPHA, ONE`, alpha test `>= 16`) were the one decal look not seen
live. Four live runs of `capture_fx.py` (`fx-axe-v-scout__live__000`, `-full__live__000`, `-head__live__000/001`;
Backyard Ripper's axe against the stock Scout, hazards off, AIs held for the shots, the same `ra2_camera`-style scripted
cameras as docs/37 section 3 and 6, 1024x768) logged 47 + 50 + 41 + 4 slash decals and photographed the Scout from eight
sides at 1.9 m and head-on at the strongest decals. Findings:

* **Natural strength.** `alpha = damage / 100`: of the 25 decals of `fx-axe-v-scout__live__000` 20 are below 0.06 and one above 0.5 (the forced run's 25 have four above 0.5). With a texture alpha of
  at most 102/255 (0.4) and the vertex alpha multiplied in (`ALPHAOP = MODULATE`), a decal under about 0.16 never reaches
  the `>= 16` test: **a slash is drawn only for a big hit and then as a faint grey streak** (texture colour <= 210 grey,
  additive over a dark chassis). In no live frame at natural alpha is one visible (`fx-axe-v-scout__live__000-pose04..07`),
  and ours shows none either at the same marks (`decalshot`, same chassis matrices and cameras).
* **Forced to full strength.** `ra2_fx.js` `setalpha` writes 1.0 into the live `DecalTrigger` objects' alpha (`+0x1cec`,
  the last 25 per chassis; `ALPHA_FORCE=1.0` / `DECAL_SHOTS=n` of `capture_fx.py`). At 1.0 the same `decalshot` frames (new
  `DECALSHOT_ALPHA`) show a faint streak in three of eight poses (<= 41 / 255 against the same render with alpha 0); the
  live frames at the matching poses show none **on the faces the camera can see**, because the slashes of these fights sit
  on the Scout's low side rims and underside (decal normals `-y`, local `y` -0.1..-0.24, at the +x / +-z corners:
  the axe cuts from the floor) and the held Scout ends the fight on its side, pinned to a wall, or hidden by the results
  window (the match ends 40 to 120 s in). The head-on shots (`-head__live__000/001`, `dnat`/`dfull` pairs) put the
  camera 0.7..0.9 m off a decal's normal: all but one are inside the wall or the axe robot's body, the one in the open shows
  floor. **So the look is not established live beyond the draw state; no frame contradicts it.**
* **One difference found and fixed: the alpha test.** The state object has an alpha test for the additive mode, ours drew
  every slash fragment untested (`decals::material` had `no_alpha_test: true` for both blends). Now `no_alpha_test` is
  `!additive`: slashes are alpha-tested batches (`>= 16` after the stage's MODULATE), dents stay blended and untested. Effect
  on the eight poses of `fx-axe-v-scout` (natural alphas): at most 3 / 255 per channel over 667..11 486 pixels; at full strength
  at most 5 / 255 (test `decals::tests::slashes_are_tested_and_dents_are_not`). Both are below visibility, which is the
  point: the live game does not draw the faint ones either.

Evidence (git-ignored, manifest rows): `fx-axe-v-scout__live__000`, `fx-axe-v-scout-full__live__000` (forced alpha 1.0, poses),
`fx-axe-v-scout-head__live__000/001` (head-on). Side by side: scratchpad only. Tools: `capture_fx.py` `ALPHA_FORCE`,
`DECAL_SHOTS`, `DECAL_DIST`, `fx_decal_spec.py` (accepts `dshot` rows and a robot with no logged matrix),
`decalshot` `DECALSHOT_ALPHA`. Open: a live frame of a strong slash on a visible face (a held Scout hit by the axe from above, or
a decal placed with a scripted hit).
