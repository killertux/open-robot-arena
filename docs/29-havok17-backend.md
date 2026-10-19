# 29 - The `havok17` physics backend and the game's drive model

`havok17` is a second implementation of `PhysicsBackend`
(`engine/crates/ra2sim/src/havok17/`) whose dynamics follow the Havok 1.7.1
subset *Robot Arena 2* ships, written from the recovered specification (the
map in the `havok-scope` study, the decompiles cited below and live
measurements of the original). Jolt is kept only as a geometry engine: it holds
every body's shape at its pose and answers closest-point queries and ray casts;
Jolt's solver never runs in this backend.

Selection: `havok17` is the default (`backend::DEFAULT_BACKEND`);
`RA2_PHYSICS=jolt` selects the Jolt fallback. The game's drive model
(section 3) runs with `havok17` by default; `RA2_DRIVE=magic|interim`
overrides it for either backend (Jolt defaults to the interim wheels).
`RA2_H17_DEBUG=<body>` prints one body's state and contacts every step.

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

**Narrowphase** (`narrow.rs`, recovered, section 10): Havok's own collision
agents — GJK closest points per convex piece against each nearby mesh
triangle or against another convex piece, each pair with its persistent
manifold, and triangle-pair closest points between two meshes. Jolt is only
asked for ray casts, phantoms and, when a step is accepted overlapping, the
depth of the overlap. Mesh triangle order (the MOPP's) is not reproduced.

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
(Reconciled since, docs/33 §5: the record axle is now `CLOCKWISE · axis` as
live, a robot's front is chassis `+Z` turned by its heading, and `Turn(+)`
turns at a positive rate about `+Y` as in the original.)

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

**havok17 is now the default** (section 9 has what it took and the numbers
on the default tree).

## 6. What is missing

1. ~~Robot assembly~~: fixed (section 8). The live AddWheel records of
   section 6 in the previous version of this note (Scout hubs at
   `(-0.490, -0.133, -0.202)`, ...) are hub centres relative to the chassis
   body's centre of mass; the scene capture of section 8 now pins every part.
2. ~~Coast~~: the hubs' contacts were the corner manifold's (item 5). With
   Havok's agents (section 10) the hub contacts per step match the live
   counts and the coasts the fresh hazard-free captures.
3. The power factor `g` (battery drain) is 1; ALARM's live thrust falls 7 % in
   12 s.
4. Steering wheels (`Steering` records) and the Havok wheel constraint
   (type 5) are not modelled. `0x486860` and `0x486890` (reached only
   indirectly) are thin wrappers that, if their wheel has a record, write
   into it: `0x486860` copies a vector into the matching record
   (`0x485660`; most likely the axle `+0x3c`, i.e. steering turns the
   record's axle rather than adding thrust) and `0x486890` sets the record's
   dword `+0x24` (`0x4856b0`); the steering unit's update that calls them
   was not traced, so the field meanings are unconfirmed: a `Steering` unit's hubs are
   now bodies of their own on a free hinge about the hub axle (live, `hub1` and
   `hub2` are bodies), and their wheels get records without thrust, like an
   `AxleMount`'s; deactivation (2 s / 2 s) is off.
5. ~~Narrowphase~~: Havok's agents (section 10). Still not reproduced: the
   MOPP's triangle order, Havok's tie-breaking in GJK (its support search
   hill-climbs from the last support vertex, `0x55b000`), the arena's fixed
   convex bodies (merged into one mesh here), `SIMULATION_GEOMETRY 0`
   (not observed live; collided as convex).
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

## 9. havok17 as the default (fx/havok-default)

With the default switched, four workspace tests failed; each was a game rule
the backend lacked, recovered and implemented as below.

**`Lock` (`PhysicsBackend::lock_joint`).** The hinge controller's `Lock(1)`
(`FUN_00480e20`) reads the current angle (`FUN_00481190`), sets *both* limits
to it (`FUN_00480880(θ, θ)`) and turns the motor off (`atom + 0x91 = 0`);
`Lock(0)` restores the stored limits (`+0x68/+0x6c`). The prismatic
controller's (`FUN_004b50c0`) sets both travel limits to the current offset
clamped to the travel. The controllers lock themselves in their update: the
hinge one (`FUN_00481450`) while the command (`+0x7c`) is zero if `+0x64`,
and on arriving within 0.05 of the end it is driven toward if `+0x65` — both
default on (`0x480234`/`0x480237`) and turned off only by `SpinMotor`
(`0x4f5571`/`0x4f557e`) — the prismatic one (`FUN_004b5240`) likewise with
`+0x7d` (on from construction, `0x4b47b0` ends with `Lock(1)`; a burst
piston's fire clears it, `0x448750`). Firing calls `Lock(0)` first (burst
motor `0x446810`, piston `0x448750`). So a cocked burst motor or piston is
held by its limits, not by friction: ours locks it while cocked
(`Robot::drive_burst`), and a hazard joint while it is held
(`Duel::drive_joints`). havok17 implements the lock as limits `[θ, θ]` (a
locked hinge measures its miss the short way round); Jolt keeps the friction
park (`lock_joint` returns `false`). Fixes
`a_burst_motor_fires_on_its_trigger_and_cocks_again` and
`a_burst_piston_extends_when_its_key_is_pressed`.

**Disabled collision pairs.** An arena `.gmf`'s rigid-body collection carries
`GMID_HAVOK_DIS_COLLISION_PAIRS` (`ra2fmt` read and discarded them): pairs of
bodies Havok never collides. They are now parsed (`SceneObject::disabled_pairs`,
`HavokArena::disabled_pairs`) and applied to havok17
(`PhysicsBackend::disable_collision`): between two movable bodies by name, and
between a movable body and the merged static collider when the file disables
it against *every* colliding fixed body. The Parking Lot's trap doors are
disabled against `main_collision`, the scaffolds and the barrels, and sit in
their frame with 5-16 mm to spare: without the pairs a door is wedged and
cannot open. Jolt ignores the pairs.

**`the_arena_can_hide_a_body_and_pin_one`'s premise.** The cinder block it
pins and unpins spawns resting on `trapdoor1`, which `Hazards.TrapDoor.__init__`
locks (`hinge.Lock(True)`). Under a faithful lock an unpinned block on a shut
door stays put; it fell before only because Jolt's friction park let the door
sag to -0.39 rad. The parking lot script activates the blocks from
`TrapDoor.Trigger()`, which unlocks the hinge, sets direction 100 and applies
a torque of 300; the test now does the same before unpinning, and the block
falls with the door (both backends).

**Hazard auto-lock arrival.** `FUN_00481450` checks the end for a positive
command and the start for a negative one. Our test used the sign of
`SetDirection` alone; the Compressor's `Smasher` gives its two arms opposite
speeds (`SetPowerSettings(12, ..)` / `(-12, ..)`) with the same direction, so
the arm driven toward its upper stop was taken as already home at the lower
one and locked. The sense is now that of the velocity the joint is driven at
(speed times direction). Fixes `the_models_own_hinges_are_driven_by_the_script`.
Not reproduced, and unverified: the original's velocity is
`trunc(-0.01 · percent · speed)` (`0x4814ca..`, `0x6a69cc` = 0.01) and is
negated when any component of a hinge atom vector (`+0x60..+0x68`, probably
the axis) is below -0.5 (`0x6a7e88`) and again when start > end
(`+0x70 > +0x74`). Applying the axis rule would reverse one of the Octagon's
two mirrored hammers and the Half arena's hammer relative to now, and without
a live check of which way they swing it is left out.

**Jolt fallback.** Seven robots did not move on Jolt + interim after the
assembly fix (Sentinel, Raptor, Civil Disobedience, Backyard Ripper,
BOT-204, Eye Poker, Dementia). Not the convex radius (traced on Sentinel;
the fix brings all seven back): the interim sampler
casts a ray up from below the floor to find ground, and with the wheels where
the original puts them a resting wheel reaches about a centimetre below the
floor's surface (Sentinel's to -2.278 on a floor at -2.267), so the ray met
the robot's own wheel first and read "no ground". The ray now skips the wheel
(`ray_cast_excluding`) and passes through the robot's other bodies. Jolt +
interim: 45/45 (38/45), and `a_scripted_drive_moves_robot_zero_and_reaches_the_results`
passes on Jolt again.

**Validation on the default tree** (same drive tests as section 5; Jolt is
`RA2_PHYSICS=jolt`, interim wheels):

| robot / metric | original | havok17 (default) | before (section 5) | Jolt + interim |
|---|---:|---:|---:|---:|
| Scout top / t90 | 11.12 / 1.58 | 11.31 / 1.7 | 11.11 / 1.6 | 12.92 / 1.6 |
| Scout distance / coast (m) | 15.42 / 8.05 | 15.86 / 5.85 | 15.72 / 5.83 | 17.97 / 8.12 |
| Scout turn (rad/s) | 10.66 | 10.71 | 10.59 | 10.27 |
| Scout 1.35 s: top / distance / coast | 9.32 / 7.83 / 6.72 | 9.34 / 7.51 / 4.89 | 9.19 / 7.47 / 5.05 | 10.52 / 8.45 / 7.78 |
| Scout rest (bot origin y) | -2.086 | -2.086 | -2.100 | -2.009 |
| ALARM top (1.65 s) | 13.38 | 12.57 | 14.22 | 9.81 |
| ALARM distance / coast | 14.11 / 11.91 | 13.91 / 7.44 | 14.4 / 9.05 | 11.41 / 4.13 |
| ALARM turn | 12.1 | 11.04 | 10.57 | 5.33 |
| ALARM rest | -2.074 | -2.075 | -2.075 | -1.983 |
| FlapJack top / t90 | 9.92 / 1.37 | 10.66 / 1.4 | 9.95 / 1.4 | 11.69 / 1.5 |
| FlapJack distance / coast | 12.34 / 6.51 | 12.14 / 6.03 | 11.44 / 5.18 | 13.62 / 7.34 |
| FlapJack turn | 10.16 | 9.42 | 9.56 | 5.43 |
| Razor top / t90 | 16.07 / 1.38 | 16.52 / 1.3 | 16.52 / 1.3 | 12.03 / 1.2 |
| Razor distance / coast | 20.56 / 7.82 | 21.3 / 5.31 | 21.3 / 5.31 | 17.2 / 5.18 |
| Razor turn | 12.2 | 12.25 | 12.25 | 8.48 |
| Wide Load top / t90 | 13.3 / 1.52 | 13.39 / 1.6 | 13.44 / 1.6 | 5.46 / 1.2 |
| Wide Load distance / coast | 18.09 / 1.79 | 17.19 / 3.29 | 17.28 / 3.01 | 8.35 / 2.22 |
| Wide Load turn | 12.37 | 13.25 | 11.99 | 3.55 |
| traction survey | - | 45/45 | 45/45 | 45/45 (38/45) |

The robots with a burst device changed (Scout's fork, ALARM's piston,
FlapJack's wedge, Wide Load's piston): their parked weapon is now held rigid
by the lock instead of flexing on friction. Razor has none and did not
change. The Scout's and ALARM's rest heights now match the original's to
1 mm. Better: Scout and ALARM rest, ALARM top speed and turn, FlapJack
distance and coast. Worse: Scout top (+0.2), ALARM coast (-1.6), FlapJack top
(+0.7), Wide Load turn (+0.9). havok17 is still the closest of the backends on
every robot except the coast.

(The Scout and ALARM originals are the numbers measured earlier; their raw
captures — `drive-scout-box__live__000`, `magic-scout__live__003`,
`magic-alarm__live__000` — were lost with `local/` on 2026-10-19, so only the
start poses kept in the validation scripts were reused. FlapJack, Razor and
Wide Load are against `magic-<bot>__live__000`.)

**AI-vs-AI matches** (`fidtrace match`, 120 s limit, shipped AI): the robots
do not fight. On the default backend: box Scout-ALARM 59 s (min distance
1.45 m, pushing, no damage), box Razor-FlapJack 40 s, box Grog-Boxer 51 s, box
Tornado-Ripblade 21 s (both stand still after 2 s), Octagon Razor-FlapJack
47 s (two hits >= 10, 37 damage to Razor, FlapJack 35 points), Octagon
Scout-ALARM 25 s, Compressor Sentinel-Wide Load 25 s (neither leaves its
start), Hilltop Grog-Boxer 21 s, Parking Lot Tornado-BEAR 26 s. Every one
ends early on `LastRobot` (a robot immobilised, 20 s inside a sqrt(2) m
sphere) with 0 points except the one Octagon match. Jolt is no better on the
same pairings (box Scout-ALARM 39 s, Compressor 68 s, Hilltop 41 s, box
Tornado-Ripblade 21 s, box Razor-FlapJack 75 s, box Grog-Boxer 21 s, Octagon
Razor-FlapJack 23 s; all 0 points). This is the AI bring-up gap docs/26
already records (the original's robots fight; ours barely do), not a backend
regression. That the robots standing still are not being driven by their AI
is an inference (the same robots drive under direct input in the traction
survey); the AI's commands were not traced.

## 10. Contacts as Havok 1.7 makes them (fx/havok-contact)

Recovered from the agents (`FUN_00543370` dispatches a pair of collidables on
the sum of their shape classes, collidable `+0x18`) and checked against new
live captures (`tools/oracle/capture_contacts.py` + `frida/ra2_contacts.js`:
every contact the resolver is handed, `0x53ed80` entry, with the bodies;
`contacts_report.py`; `capture_contacts_set.sh` for any stock robot with the
Combat Arena's hazards off).

**Shape classes in play** (live, `0x543370` hooked on the Scout and ALARM):
robot parts are convex (`CXGEOMETRIC` 0x400) except the chassis the game
builds and the parts whose `.gmf` body says `SIMULATION_GEOMETRY 6` (fork
arm, bracket wedge, snowplows), which are meshes (`GEOMETRIC` 4); the arena is
a mesh. `rig::collides_as_mesh`, `ShapeDesc::MeshHull` (a mesh collidable
massed as its hull; Jolt collides it as the hull). The chassis collidable is
the `.bot`'s chassis mesh with its own triangles.

**Convex x mesh** (`FUN_00545b50`): for every triangle the MOPP finds near the
convex piece (its box grown by the tolerance), GJK closest points
(`FUN_0055c390`: Johnson simplex, ends when `|v|²` stops falling; triangle
support `0x55b200`, convex support hill-climbing `0x55b000`). A distance below
zero is an overlap: the pair goes on the penetration list and reports nothing
more. Within the tolerance, the final simplex's vertices decide:
two points (an edge on either side), or three with one vertex of A, one of B,
or a repeated vertex on both sides (vertex-face, edge-edge) *persist*; the
rest (vertex-vertex, face against edge or face) are reported for this
detection only. A persisting point replaces a kept one within the tolerance
whose normal is within `acos 0.8`, else joins the pair's manifold (a hash
entry per collidable pair, `det + 0x2b4`). Every detection first re-measures
the kept points from their two body-fixed points (`+0x20` on A, `+0x30` on B):
dropped once they are a tolerance apart in any direction, else the midpoint,
and the distance along the normal they were found with. All kept points are
reported after the triangles'.

**Convex x convex** (`FUN_00544820`, `FUN_0055b880`): the GJK point is always
reported; kept points die as above, are reported with the *new* normal and
`|p_A - p_B|` if at least 4 tolerances from the new point, and one closer
keeps the new point out of the manifold.

**Mesh x mesh** (`FUN_0054a2a0` fixed mesh, `FUN_00549d00` two moving ones):
every pair of triangles within the tolerance (moving pairs' trees are queried
with half of it) gives the midpoint of their closest points
(`FUN_0055e950` → `FUN_00561410`; `< 1e-5` is an overlap), unless a point
already in the detection's list lies within `sqrt(0.001)` tolerances; no
manifold.

**Tolerances.** The agents and the resolver always use the world's tolerance
0.1 (`vt + 0x18`, `0x52e750`). The *current* tolerance (`setCurrentTolerance`
writes `det + 0x1c`) is halved before every detection attempt of a substep,
starting at 0.05 (`0x52aa60`), and only decides the bad step: a detection is
bad (`det + 0x130` non-empty, `0x52b320`) when a pair overlaps, unless a
previous bad step excused it (`det + 0xe4`, kept while it keeps overlapping),
or when any new point is closer than the current tolerance (`0x542db0`). A bad
detection rolls back and halves the step; after 30 the step is accepted and
its pairs excused (`0x542760`). Before this the detection itself used the
halved tolerance, the resolver too, and only a newly overlapping pair counted
as bad. Ours additionally gives an accepted overlapping pair Jolt's deepest
points so it separates (Havok reports none for it).

The arena soup carries each triangle in both windings for Jolt; havok17 keeps
each once (live contact lists hold no repeated point).

**Live check: contacts per step** (Scout from its spawn, 2 s full throttle, 2 s
coast, hazards off; points against the arena per body and step, median
distance):

| body | phase | original | before (corner manifold) | now |
|---|---|---|---|---|
| hub | drive | 0: 33 %, 1: 51 %, 2: 15 %; 0.095 | 0: 37 %, 2: 40 %, 3-5: 22 %; 0.088 | 0: 34 %, 1: 50 %, 2: 15 %; 0.082 |
| hub | coast | 0: 16 %, 1: 50 %, 2: 31 %; 0.094 | 0: 24 %, 2: 42 %, 3-7: 30 %; 0.089 | 0: 20 %, 1: 68 %, 2: 11 %; 0.093 |
| fork arm | drive | 0: 7 %, 1: 32 %, 2: 60 %; 0.095 | 0: 25 %, 2: 68 %; 0.088 | 0: 19 %, 1: 19 %, 2: 58 %; 0.093 |
| chassis | all | none | none | none (a handful) |

Step pieces: live 3 % of substeps are backstepped, ours 2.5 %.

**Drive tests** from the original's spawn poses, Combat Arena hazards off,
sampled every step (fresh captures `contacts-<bot>__live__000`, the
original's straight durations: Scout 1.967 s, ALARM 1.6, FlapJack 1.867, Razor
2.0, Wide Load 2.133; poses from the captured chassis). The earlier tables
sampled ours every 0.1 s and started the coast at the first sample after the
stop, which loses up to 0.1 s of the fastest motion (1.1 m on the Scout); and
the earlier ALARM pose ran it over the spike plate, whose firing launched ours.
Turn rates are magnitudes, against the earlier captures. "before" is fidelity
`f6cb227`, "now" this branch merged with it:

| robot | metric | original | before | now |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist | 11.01 / 1.47 / 14.29 | 11.18 / 1.47 / 14.43 | 10.92 / 1.47 / 14.25 |
| Scout | coast / turn | 7.89 / 10.66 | 7.00 / 10.68 | 7.24 / 10.68 |
| ALARM | top / t90 / dist | 13.85 / 1.22 / 14.15 | 13.88 / 1.28 / 14.07 | 13.88 / 1.27 / 14.20 |
| ALARM | coast / turn | 11.15 / 12.1 | 9.75 / 11.00 | 10.92 / 10.99 |
| FlapJack | top / t90 / dist | 10.31 / 1.45 / 12.41 | 10.55 / 1.45 / 12.24 | 10.45 / 1.43 / 12.27 |
| FlapJack | coast / turn | 6.78 / 10.16 | 6.45 / 9.44 | 6.82 / 9.09 |
| Razor | top / t90 / dist | 17.50 / 1.52 / 23.07 | 17.08 / 1.43 / 22.62 | 16.51 / 1.42 / 22.40 |
| Razor | coast* / turn | 5.72 / 12.2 | 7.53 / 12.24 | 7.84 / 12.18 |
| Wide Load | top / t90 / dist | 13.90 / 1.55 / 19.49 | 13.29 / 1.63 / 17.77 | 13.47 / 1.62 / 18.09 |
| Wide Load | coast* / turn | 8.61 / 12.37 | 8.13 / 13.21 | 8.39 / 12.83 |

\* both coast into a wall in the original and here; the distance is where
the wall is met, not the drive model.

Before the merge of `f6cb227` (old drive record axle) the same tree read
Scout 11.00 / 1.37 / 14.37, coast 7.86; ALARM coast 11.00; FlapJack 7.31.
Traction survey 45/45. AI suite (`fidtrace match`, ten pairings): nine deal
damage (eight before). Open: Scout v ALARM deals far less than before (box,
120 s: one 11-point hit against seven hits, 132 points), and
`duel::tests::the_shipped_ai_closes_sees_its_enemy_and_lands_blows` (damage
within 30 s) fails. The hits are there — ALARM's hammer head meets the Scout's
fork arm at 20-32 m/s — but priced at 4-8, under the arena's gate of 10; the
corner manifold used to give such a pair several points, each priced alone.
Collision events are fired per contact point (`FUN_00549c30` walks the whole
contact list), so the count now follows the agents; whether the original
prices these hits higher (its fork arm and hammer head are a mesh and a convex
piece) is not measured. Turning the bad-step rule's close-contact test off
makes that test pass by chance (the 120 s matches still deal 0-1 hits).

**Mass**: every stock chassis body with a SmartZone weighs 2 kg more live
(163.49 against our 161.49 kg; Razor, without one, 212.52 on both): the
SmartZone's `.gmf` body (2 kg, `DISABLE_COLLISIONS`), which the renderer's
loader skips. `rig::smart_zone_mass` adds it to the body it is bolted to.

**Hinge direction rule** (`0x4814ca..`): the HingeController takes `+0x70/+0x74`
as min/max of the hinge's limits (`FUN_004801f0`), so the start > end flip
never fires from construction; `hinge + 0x60` is the axis in body B's frame
(`FUN_00525830`). Live (Octagon, `Arenas.currentArena.Hammer1.Smash()`, hinge
atom `0x535a50` hooked): the motor velocity is `+94` (the negated
`trunc(-0.95·100)`) and the hammer turns `+x` (ω 11-16 rad/s) toward the
arena's centre; retracting, `-2` and `-x`. Ours drives the same hammer `+x`
toward the centre with the velocity's own sign, so applying the axis rule on
top would reverse it: left out, now with a live check. `Hammer2.Smash()` moved
nothing live (only one hammer atom ever ran its motor); ours swings the
mirrored hammer at +14.4 z, toward the centre. Not resolved.

**Locked hinge settling**: a hinge locked while swinging overshoots by about
0.08 rad and settles 0.03 rad past the lock angle in our test arm. The cause is
the order the recovered setup prescribes (`0x535a50`): the limit impulse about
the free axis uses the bodies' inertia about their centres (`k = aᵀ(I_A⁻¹ +
I_B⁻¹)a`), then the pivot rows' impulses turn part of it back. Ours follows
that order; whether the original settles the same was not measured.

