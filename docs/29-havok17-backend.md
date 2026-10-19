# 29 - The `havok17` physics backend and the game's drive model

`havok17` is a second implementation of `PhysicsBackend`
(`engine/crates/ra2sim/src/havok17/`) whose dynamics follow the Havok 1.7.1
subset *Robot Arena 2* ships, written from the recovered specification (the
map in the `havok-scope` study, the decompiles cited below and live
measurements of the original). Jolt is kept only as a geometry engine: it holds
every body's shape at its pose and answers closest-point queries and ray casts;
Jolt's solver never runs in this backend.

Selection: `RA2_PHYSICS=havok17` (default `jolt`). The game's drive model
(section 3) runs with `havok17` by default; `RA2_DRIVE=magic|interim`
overrides it for either backend. With the robot assembly fixed (section 8)
havok17 is the closest to the original on every robot measured, but it is not
the default yet: four workspace tests need backend features it lacks
(section 5). `RA2_H17_DEBUG=<body>` prints one body's state and
contacts every step.

## 1. The speed puzzle (resolved)

The Scout reaches 11.1 m/s while its drive hinge's target is 17 rad/s, and
17 rad/s on its hub would only be 3.1 m/s (hub radius 0.185 m to the flats)
or 5.5 m/s (the 0.325 m traction ring). Two findings resolve it:

1. **Grounded wheels are not driven by the hinge motor.**
   `SpinMotor::SetPercent` (`FUN_004f5800`) sets the hinge controller's
   percentage to 0 — motor off — while the motor's wheel touches something
   (`Wheel::IsTouching`, `FUN_00507ff0`: `wheel + 0x28c` and not detached), and
   only otherwise unlocks the hinge and runs its servo. Live
   (`havok-step__live__018/020`, hinge setup `0x535a50` hooked): the Scout's
   drive hinges have their motor flag **off for the whole full-throttle run**
   while the hubs spin at 30-40 rad/s; the flag comes on (servo at 17 rad/s)
   only after the robot hits a wall and its wheels leave the floor.
2. **The push is the game's `MagicAction`** (section 3), a Havok action that
   pushes the *chassis* with a thrust per touching wheel and damps it. The hubs
   are dragged along and spin from floor friction: live they turn at 30 rad/s
   at 9.9 m/s, i.e. they skid (`ω r = 5.5 m/s` at r = 0.185).

The earlier "backstep multiplier" suspicion (the servo running k times per
substep) was already refuted (k = 1.02); the hinge servo itself is correct and
matters only in the air.

## 2. Havok dynamics (recovered)

One `PhysicsBackend::step(1/60)` is one Havok substep: the game steps its world
at a fixed 1/30 s in two substeps (`FUN_004ac1d0`: `+0x4 = 1/30`, `+0x8 = 2`),
frame time clamped to 1/30 (`FUN_004accb0`). A substep is `World::stepTo`
(`FUN_0052aa60`), repeated until its time is used up:

| stage | what | evidence |
|---|---|---|
| events | collision events of the last detection, bodies as they are now | `0x549c30` |
| resolution | contact impulses (below) for those contacts | `0x52a520` → `0x53ed80` |
| integrate | explicit Euler `y += h f(y)`: position with the velocity the step started with, velocity with force/mass + gravity, angular velocity with `I⁻¹τ` (state carries ω, no gyroscopic term); accumulators cleared | `0x680ff0`, pack `0x520fb0`, derivative `0x521330`, unpack `0x521030` |
| detect | closest points within the tolerance | `0x52a4d0` |
| backstep | a pair that newly overlaps: restore the state, halve h (≤ 30 times, ≥ 1e-5 s), halve the tolerance | `0x52aa60`, "Backstep/Roll Back" |
| actions | constraint solver with motors, then the drag action | `0x52a990`, `0x523bd0`, `0x52d200` |

Constants: gravity `(0, -12, 0)` (`0x4ac377`, read live from the world);
tolerance 0.1 (live, the resolver sees contact distances up to 0.0999 and
resting bodies at 0.083-0.0999); drag `F -= 0.1 v`, `τ -= 0.3 ω` on every body
(`FUN_0052d140(0.1, 0.3)`); body defaults µ 0.5, e 0.2 (`0x51f8c0`).

**Resolver type 2** (`resolver.rs`), per island (`0x5586a0`):

* once, before any impulse: `K = K_A(p) + K_B(p)` with
  `K = m⁻¹I - [r]× I⁻¹ [r]×` (`0x520930`), its inverse and `k_n = nᵀKn`
  (`0x540630`);
* round-robin over the contacts; the next one still approaching
  (`v_n < -0.001`, `DAT_006ac604`) gets an impulse; at most 4 × 100, inelastic
  after the first 100 (`0x559720`);
* the impulse (`0x540820`): `e = eA eB`, `µ = sqrt(µA µB)`, `a = -v_n/k_n`,
  `P0 = a n`, `w = K⁻¹(-u)`, `d = w - P0`, `P = (1+e)P0 + d`; inside the cone
  (`µ P·n ≥ |P_t|`) `J = P`, else `J = (1+e)P0 + s d` with
  `s = µ(1+e)a / (|w_t| - µ d·n)`;
* push-out after every impulse (`0x5598f0`): `f = (0.9 tol - d)/(0.9 tol)`; if
  `0 < f < 1` and the contact now separates, each body gets a velocity change
  `10 tol f²` along the normal plus the angular change of that mass-scaled
  impulse at the point.

**Constraints** (`joints.rs`): every joint is set up once per solver call
(`0x535a50`) — motor and limit impulse about the free axis, then its locked
rows solved as one dense block — then re-solved in four passes with
`(tau, damping) = (1,1), (1,1), (1,1), (0.8,0.8)` (`0x534480`); a block's
right-hand side is `tau · error / dt - damping · J v` (`0x5361e0`). The hinge
has three point rows and two angular rows perpendicular to the axis. The servo:
target angle `+= dt · targetVelocity` per call with `dt` the substep (never a
backstepped piece), `λ = m_eff (0.8 err/dt - ω_rel)` (`DAT_006ab658`), capped at
`maxTorque · dt` with the target slipping back by `(1 - s) err`; limits predict
`θ + (λk + ω_rel)dt` and clamp. Live check (`havok-step__live__018`, the Scout
spinning a wheel in the air): the target advances 0.2833 rad = 17/60 per call;
the capped call reads `λ = 11.25 = 675/60` and the target advances only
0.0554, i.e. `s = 0.65` — the slip rule; spinning free the hub settles at
17.13 rad/s, 0.7 % above the target, which our test reproduces (17.11) as the
first-order quaternion integration's under-rotation. The prismatic (three
angular rows, two perpendicular linear rows, servo along the axis) and
point-to-point (three rows) blocks follow the hinge's scheme; their own atoms
(`0x5318b0`, `0x534840`) were not decompiled.

**Narrowphase** (ours, on Jolt): every moving body's shape is queried with
`CollideShape` and `max_separation_distance = tol`; a pair of moving bodies is
kept once; a robot's own bodies never collide. Havok reports a point per pair
of features within the tolerance — live, two per mini-wheel hub on the floor,
one per hub triangle (`clist_rest`) — where Jolt reports one deepest point per
shape pair, on which a resting face rocks. So every corner of either body's
convex pieces within the tolerance of the other's surface plane through the
hit becomes a contact (a short ray confirms the surface under it), the point
halfway between corner and surface. Mesh triangle order (the original
`random_shuffle`s, `0x546f70`) is not reproduced.

**Phantoms**: a body may carry a phantom shape (a wheel's `wheel_traction`,
`DISABLE_COLLISIONS` in its `.gmf`), detected like any collidable within the
tolerance but never resolved; `phantom_touch` reports the surface it touched.

## 3. The drive model: `MagicAction` (game code, `src/drive.rs`)

RTTI `.?AVMagicAction@@`, vtable `0x6a80f4`, apply `FUN_004856f0` (slot 5),
run with the actions after every Havok step. Its records (0x50 bytes) are made
by `AddWheel` (`FUN_00485490` from `FUN_004dc180`) for every wheel on a
`SpinMotor`, `AxleMount` or `Steering`: `+0` id, `+4` thrust, `+8` touching,
`+0xc` surface normal, `+0x1c` resistance (`Wheel + 0x2ac`, key `resistance`,
string `0x6ff84c`), `+0x20` grip (`Wheel + 0x2b0`, key `grip`), `+0x2c` hub
centre minus chassis centre in the chassis frame, `+0x3c` axle. The robot
update (`0x4dc7a5..`) refreshes them every frame: thrust
`2 · trunc(0.7 · torque · percent)` (`FUN_004f5780`, `0x6aa990`, doubled in
`0x485580`) from the hinge controller's raw torque (`+0xb4`, the definition's,
before the ×40 of `+0xb8`) and the motor's integer percentage; touching
(phantom contact count, kept 0.2 s after the last contact, `0x508080`,
`0x6a6080`); the normal.

Decoded from the x87 code with the vector helpers (`0x404990` scale,
`0x45a730` cross, `0x535940` dot, `0x401ae0` length, `0x4b0a10`/`0x4812e0`
impulse at a point, `0x4b0950` force accumulator `+0x74`, `0x4b0860` torque
accumulator `+0x84`, `0x485ec0` `ω += I⁻¹L`). With `m` the chassis body's mass,
`N` the number of records, everything in the chassis frame, per touching
wheel:

* `d = normalize(-(axle × n))`; drive `d · thrust · grip`;
* drag `-0.5 m (d · v_p) d / N` (`0x6f3de8`), `v_p` the chassis velocity at the
  hub;
* an **impulse** at the hub point `-(|v_lat,body| / |v_lat,p|) v_lat,p ·
  resistance · m / N`: `resistance` is the wheel's sideways grip;

then: the summed force through the centre of mass; the wheels' yaw torque
`Σ p × drive` if it exceeds 145 N·m (`0x6f3df0`), else an angular impulse
`-up (ω·up) 0.05 m · touching / N`; `-1.0 · m v` (`0x6f3dec`; `0.1` with no
wheel down); down-force `(0, -2 |v| |g|, 0)`; and above 10 rad/s (`0x6a57a8`)
an angular impulse `-(ω - 10 ω̂) · 0.1 m` — the Scout's 10.7-11.5 rad/s
turn-in-place.

Live check (`tools/oracle/capture_magic.py`, `frida/ra2_magic.js`): the
records read thrust 1960 (= 2·trunc(0.7·14·100)), grip 1, resistance 0.1 for
the Scout; 1940 at a 99 % command; and the chassis masses 163.49 kg (Scout),
232.12 kg (ALARM). ALARM's thrust falls from 1940 to 1802 in 12 s at full
throttle: the power factor `g` of `SpinMotor`'s `SetPowerSettings` (robot
`+0x9c/+0xa0`, `0x45f130`) drops as its battery drains; we still take `g = 1`.

## 4. How the original builds a robot's bodies (`rig::HavokParts`)

* each component contributes one convex piece per colliding `.gmf` rigid body
  (its collision object, `GEOMETRY_PROXY_NAME` when set); phantoms collide
  with nothing — the mini wheel's hub is 0.2 m to its corners, its traction
  ring 0.33 m;
* every rigid body has mass, phantoms included, and a motor's rotor
  (`axle`, `bmotor_axle`, `shaft`, `slider`) belongs to the body it drives —
  live 27 kg per Scout/ALARM wheel body (9 + phantom 8 + axle 10), Scout fork
  arm 26 (2 + 12 + 12), chassis 163.49;
* a body's material is its top rigid body's (wheels 0.3/0.3); the chassis body
  is the game's own and keeps Havok's defaults (live 0.5/0.2);
* power cells do not collide (as with the Jolt bodies: the resting Scout
  touches the floor with its hubs and fork arm only);
* every part starts where the assembly (section 8) puts it. An earlier
  version spawned a burst piston "cocked", its rod retracted 0.73 m, to make up
  for ALARM's hammer hanging below the wheels; that was the misplaced
  assembly, not the piston. Live, the rod is built at its neutral point and
  ALARM's hammer body sits 0.55 m in front of the piston at chassis height
  (`assembly-alarm__live__000.json`); the cocking is gone.

## 5. Validation

Drive tests from the original's own start poses (`fidtrace drive-at`), in sim
time. Scout and ALARM against the numbers measured earlier
(`drive-scout-box__live__000`, `magic-scout__live__003`,
`magic-alarm__live__000`); FlapJack, Razor and Wide Load against new captures
(`magic-<bot>__live__000`, `tools/oracle/capture_magic.py` with the robot
copied into a Red Zone slot): 3 s turn in place, then about 2 s straight from
the pose the original started it, and 3 s of coast. Turn rates are given as
magnitudes (the two sides' heading conventions were not reconciled).

After the assembly fix (section 8):

| robot / metric | original | Jolt + interim | Jolt + MagicAction | havok17 |
|---|---:|---:|---:|---:|
| Scout top speed (m/s) | 11.12 | 12.92 | 11.1 | **11.11** |
| Scout t to 90 % (s) | 1.58 | 1.6 | 1.4 | **1.6** |
| Scout straight distance (m) | 15.42 | 17.97 | 16.85 | **15.72** |
| Scout coast (m, 3 s) | 8.05 | **8.12** | 5.2 | 5.83 |
| Scout turn in place (rad/s) | 10.66 | 10.27 | 10.77 | **10.59** |
| Scout 1.35 s straight: top / distance | 9.32 / 7.83 | 10.52 / 8.45 | 9.71 / 8.39 | **9.19 / 7.47** |
| Scout 1.35 s straight: coast (m) | 6.72 | 7.78 | 5.01 | 5.05 |
| Scout rest (bot origin y) | -2.086 | -2.009 | -2.009 | **-2.100** |
| ALARM 1.65 s straight: top (m/s) | 13.38 | 9.81 | 11.82 | **14.22** |
| ALARM distance / coast (m) | 14.11 / 11.91 | 11.41 / 4.19 | 13.39 / 3.05 | **14.4 / 9.05** |
| ALARM turn in place (rad/s) | 12.1 | 5.33 | 10.41 | **10.57** |
| ALARM rest (bot origin y) | -2.074 | -1.983 | -1.983 | **-2.075** |
| FlapJack top / t90 | 9.92 / 1.37 | 11.69 / 1.5 | 8.7 / 0.9 | **9.95 / 1.4** |
| FlapJack distance / coast (m) | 12.34 / 6.51 | 13.62 / 7.34 | 11.1 / 1.73 | 11.44 / 5.18 |
| FlapJack turn (rad/s) | 10.16 | 5.43 | 7.99 | **9.56** |
| Razor top / t90 | 16.07 / 1.38 | 12.03 / 1.2 | 15.94 / 1.2 | **16.52 / 1.3** |
| Razor distance / coast (m) | 20.56 / 7.82 | 17.2 / 5.19 | 22.04 / 2.58 | **21.3 / 5.31** |
| Razor turn (rad/s) | 12.2 | 8.48 | 12.23 | **12.25** |
| Wide Load top / t90 | 13.3 / 1.52 | 5.46 / 1.2 | 14.46 / 1.6 | **13.44 / 1.6** |
| Wide Load distance / coast (m) | 18.09 / 1.79 | 8.35 / 2.21 | 19.0 / 1.18 | **17.28 / 3.01** |
| Wide Load turn (rad/s) | 12.37 | 3.55 | 11.33 | **11.99** |

The rest heights are now compared like for like: the earlier table's
"original" (-1.884, -1.811) was `plus.getLocation`, the chassis body's centre
of mass, while ours is the bot origin; the live scene gives the bot origin of
the resting Scout and ALARM as -2.086 and -2.074 (`assembly-*__live__000.json`,
the `Chassis Display` node). Before the fix havok17 read Scout -1.932, ALARM
-1.832 (Jolt -1.896 / -1.807).

Before the assembly fix the same table read (original / Jolt + interim / Jolt +
MagicAction / havok17): Scout top 11.12 / 12.4 / 10.67 / 11.04, ALARM top
13.38 / 7.66 / 2.3 / 5.61, ALARM turn 12.1 / 3.98 / 0.22 / 8.94.

Traction survey (45 stock bots, 3 s full throttle, ≥ 2 m;
`cargo test -p ra2sim --test traction -- --nocapture`): Jolt + interim 38/45
(44/45 before), Jolt + MagicAction 45/45 (39/45), havok17 45/45 (36/45). The
interim wheel model was tuned on the misassembled robots; with the wheels
where the original has them six robots stop passing.

**Result: havok17 (with the MagicAction drive) clearly beats Jolt + interim.**
On all five robots it is the closest of the three on top speed,
acceleration, distance, turn rate and rest height, with nothing fitted, and it
passes the survey; the one row it loses is the coast (item 2 below). Jolt +
MagicAction beats the interim wheels on ALARM, FlapJack, Razor and Wide Load
and on the survey, but coasts far too short (Wide Load 1.2 m, FlapJack 1.7 m)
and is no clear win.

**Decision: Jolt + interim stays the default for now**, because switching to
havok17 (tried: `DEFAULT_BACKEND = "havok17"`, full workspace test run) fails
four tests that exercise features havok17 does not implement yet, none of
them drive or assembly:

* `wiring::a_burst_motor_fires_on_its_trigger_and_cocks_again`: a parked burst
  arm folds under the robot's weight (0.4 rad/s). Ours parks it with joint
  friction (the motor's torque); the original locks it by setting both hinge
  limits to the current angle (`FUN_00480e20(1)` → `FUN_00480880(θ, θ)`), which
  havok17 should do.
* `wiring::a_burst_piston_extends_when_its_key_is_pressed`: the parked rod
  creeps 7-11 cm (already so before the assembly change).
* `duel::the_arena_can_hide_a_body_and_pin_one`: an unpinned arena prop falls
  8 cm in 0.5 s instead of free-falling.
* `duel::the_models_own_hinges_are_driven_by_the_script`: the compressor's
  smash arms do not move under their script.

Once those pass under havok17, it should become the default
(`backend::DEFAULT_BACKEND`).

## 6. What is missing

1. ~~Robot assembly~~: fixed (section 8). The live AddWheel records of
   section 6 in the previous version of this note (Scout hubs at
   `(-0.490, -0.133, -0.202)`, ...) are hub centres relative to the chassis
   body's centre of mass; the scene capture of section 8 now pins every part.
2. Coast: our hubs still skid after the thrust stops. During thrust they turn
   at about 60 % of rolling, as live (30 rad/s at 9.9 m/s), but in the coast
   they only creep toward rolling where the original's reach it within 0.3 s
   (`havok-step__live__023`): Scout coasts 5.8 m instead of 8.05, FlapJack 5.2
   instead of 6.5, Razor 5.3 instead of 7.8, ALARM 9.1 instead of 11.9 (Wide
   Load, which drags its axle-mount skids, coasts 3.0 instead of 1.8). Turning
   the wheel hinges' parked friction (ours, 40 × `resistance`) off changes the
   Scout's coast by 0.1 m only, so the hinge is not it; the hub-floor contact
   (our corner manifold, item 5) is the suspect.
3. The power factor `g` (battery drain) is 1; ALARM's live thrust falls 7 % in
   12 s.
4. Steering wheels (`Steering` records, `0x486860`/`0x486890`) and the Havok
   wheel constraint (type 5) are not modelled: a `Steering` unit's hubs are
   now bodies of their own on a free hinge about the hub axle (live, `hub1` and
   `hub2` are bodies), and their wheels get records without thrust, like an
   `AxleMount`'s; deactivation (2 s / 2 s) is off.
5. Narrowphase: Jolt's closest points plus our corner manifold stand in for
   Havok's agents (`0x543370`); a faithful one needs our own GJK convex/mesh
   agents (step 3 of the plan in the `havok-scope` study).
6. The weapon test (`weapon-alarm-vs-inert-scout__live__000`) is not compared
   yet; ALARM's hammer is now where the original has it.

## 7. Tools

* `tools/oracle/capture_magic.py` (+ `frida/ra2_magic.js`): the live drive
  model's records and the chassis body every Havok step during a drive test
  (`PLAYER=0|1`, aims at the longest free line with the game's own
  `AimToHeading`). Frida slows the game below 30 fps, so use the recorded
  step times, not wall time.
* `fidtrace drive-at <arena> <bot> <x> <z> <heading> <script> <s> <out>`: a
  drive test from a given pose.
* `h17probe`: one robot, its bodies, wheel records and per-step state (each
  hub's spin about its axle against the rolling spin).
* `tools/oracle/capture_assembly.py` (+ `frida/ra2_assembly.js`,
  `capture_assembly_set.sh` for any stock robot): the original's scene as
  built and at rest; `tools/oracle/assembly_compare.py` against
  `asmdump`; `tools/oracle/assembly_fixture.py` writes the regression table
  `engine/crates/ra2sim/tests/data/assembly_live.tsv`.
* `gmfframes`: a `.gmf`'s object transforms, hinges and attachment frames.

## 8. Robot assembly (recovered)

How a `.bot` becomes bodies, measured on the live original and implemented in
`ra2fmt::mount` (the one rule the renderer, the simulator and the workshop
share).

**Ground truth.** `capture_assembly.py` hooks the per-robot power step
(`0x4dc3f0`, `ecx` = Robot; `+0x1c/+0x20` the component handles) and walks
each component's scene node (`component + 0x1c`, a `gi3DObject`, vtable
`0x6a5598`: `+0xc` local and `+0x4c` world 4x4, row-major with the
translation in row 3, `+0xbc/+0xc0` children, `+0xe0` parent, `+0xd0/+0xd4`
name). Under the robot's root there is one node per Havok rigid body
(`Chassis Display` for the chassis body, the driven body's top rigid body
name otherwise) whose world is the body's pose, and every component mesh
hangs below as an `<object> Instance` node. The first power step of each robot
gives the robot as built (`first`), a later one the robot at rest (`tree`).
Fourteen stock robots were captured: Scout, ALARM, Sentinel (steering, burst
motor), FlapJack (wedge on a snapper burst motor, casters), Tornado (spinner),
Ripblade (axle mounts, 27 components), Wide Load (axle mounts, burst piston),
Little Metal Friend (servo motors), Razor, Coal Miner (five motors), BEAR
(two snappers at oblique angles), Mud Runner (snappers on a side mount
point), Grog (axe on a burst motor), The Boxer (two burst pistons). The
robots outside the Red Zone team were copied into the Wine prefix's
`AI/Team0` slots for the capture and restored afterwards.

**The rule.** With `A_own` the component's own mount point (`own_mount`) and
`A_parent` the parent's (`parent_mount`), both `.gmf` attachment frames:

```text
chassis child:  T(t) · Q · T(0, 0, -height) · Rz(π - angle) · A_own⁻¹
component link: P · A_parent · Ry(π) · Rz(-angle) · A_own⁻¹
```

* A chassis child's **own point lands exactly on the stored translation**
  (`t.y = 0`, the baseplate), lifted by `height`; `Q` is the stored quaternion
  (every stock chassis child stores the quarter turn about X, whose Z is the
  chassis' down, hence `-height`). The old rule ignored the own point: that is
  what put ALARM's rear axles 0.19 m above its front ones (the angle motors'
  point is 0.115 m below and 0.097 m beside their origin, and the two mirrored
  models carry it to opposite sides), narrowed every track by 0.1 m and stood
  ALARM's air tanks and piston on end.
* At a link the child's point is turned half a turn about the parent point's
  **Y** (the old rule flipped about X, which differs by a half turn about the
  axle) and spun by `-angle` about the common Z.
* `P` is the parent's pose as whatever hosts the parent point carries it: the
  component itself, or a motor's rotor.
* A component's own mesh is drawn at the component frame; an object that is a
  body of its own (a rotor, a steering hub) at its `.gmf` transform. (A
  caster's `caster_display` declares a 0.022 m offset and an iron fist's
  0.11 m that the original does not apply; collision proxies keep their own
  transforms. Our renderer still draws a display mesh at its own transform.)
* **A burst motor's rotor is built at its start angle** by an absolute rule.
  `BurstMotor` (vtable `0x6a679c`) stores start/end at `+0x2a0/+0x2a4`
  (`FUN_00446960`, `FUN_004469a0`) and passes them to its hinge controller
  (`+0x280`): `FUN_00480c60(start)` calls `FUN_004808a0(start)`, which gives the
  rotor body an orientation rather than turning it, then re-creates the hinge
  there; `FUN_00480dd0(end)` sets the limits to `0` and `-end`. Measured on
  eight burst motors (both stock models, four mounting orientations, hooks on
  `0x480c60`/`0x4808a0`/`0x480dd0`): with `a` the housing's `Hinge01` spin
  axis in the robot frame and `p` the hinge's reference (`FUN_00525830`:
  `a × ŷ` when `|a.x| ≥ 0.57` (double `0x6abfc8`), else `a × x̂`), the rotor's
  Z is `-a` and its Y is `p` turned by the start angle about `a`, within 1°
  (the residue is one step of sag). In the robot frame, not the world: BEAR's
  oblique motor falls in the other branch in world axes and only the robot
  frame reproduces it. So `burstmotor.gmf`'s rotor (hinge along its +Z) is
  built flipped end for end against its housing; `snapper2.gmf`'s (hinge along
  -Z) is not.
* The burst travel: the game's hinge angle is the opposite sense of the
  start rule's turn, so from the start the rotor swings by `+end` about `a`
  (Scout's fork 66° up, Grog's axe 119° down). Our joint limits are
  `[min(0, end), max(0, end)]` about the `.gmf` hinge (`rig::assemble`; the
  old code used `end - start` about the arm's mount point Z).
* A burst piston's rod is built at its neutral point (shaft = body), no
  cocking.
* An `AxleMount` (its `.gmf` hinge) and a `Steering` unit's hubs (their mount
  point's axis; the file declares no hinge) are bodies of their own on a free
  hinge (`rig::Role::Axle`). Before, their wheels were welded to the chassis.
* Spin motors' rotors (and free axles) have already turned by the first
  physics step (deterministically: the same angles in two captures, Scout's
  hubs 15.6° and 3.6°, Tornado's spinner 45.4°) — one step of the hinge servo
  before the AI is neutered. Those bodies are pinned by their distance along
  and from the hinge axis only.

**Result** (`assembly_compare.py --first`, every mesh of the robot, chassis
frame; before = the previous rule, `RA2_MOUNT=frames-flip`):

| robot | before mean / max (m) | after mean / max (m) |
|---|---:|---:|
| Scout | 0.164 / 0.215 | 0.001 / 0.006 |
| ALARM | 0.189 / 0.812 | 0.000 / 0.000 |
| Sentinel | 0.128 / 0.215 | 0.001 / 0.006 |
| FlapJack | 0.133 / 0.479 | 0.001 / 0.008 |
| Tornado | 0.383 / 0.941 | 0.000 / 0.000 * |
| Ripblade | 1.209 / 2.325 | 0.000 / 0.000 * |
| Wide Load | 0.442 / 1.397 | 0.000 / 0.001 |
| Little Metal Friend | 0.369 / 1.113 | 0.000 / 0.002 |
| Razor | 0.143 / 0.358 | 0.000 / 0.000 |
| Coal Miner | 0.289 / 0.784 | 0.000 / 0.000 * |
| BEAR | 0.301 / 1.840 | 0.001 / 0.004 |
| Mud Runner | 0.368 / 2.130 | 0.001 / 0.003 |
| Grog | 0.352 / 2.202 | 0.000 / 0.002 |
| The Boxer | 0.319 / 1.174 | 0.000 / 0.000 |

\* along/from the spinner's axis (its rotor had turned 45°, 42° and 66° by the
first step); in plain positions 0.24 / 0.81, 0.03 / 0.47, 0.02 / 0.28.

`cargo test -p ra2sim --test assembly` pins all 310 meshes to 1 cm
(worst 8 mm, a burst rotor's one-step sag).
