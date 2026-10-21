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
`+0x9c/+0xa0`, `0x45f130`) drops as its battery drains. `g` is the power
budget's electric scale (docs/30) and it scales the motor's *percentage* only
(section 11).

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
3. ~~The power factor `g`~~: the power budget's scale, on the percentage
   only (section 11).
4. ~~Steering wheels~~: the `Steering` unit drives and steers (section 11).
   Still not modelled: the Havok wheel constraint's suspension (the hub is
   our hinge with its axle turned, not a type-5 constraint), deactivation
   (2 s / 2 s).
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
live captures (`tools/oracle/capture_contacts.py` + `frida/ra2_contacts.js`, captures `contacts-drive-*` and `contacts-nohaz-*`:
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
sampled every step (fresh captures `contacts-nohaz-<bot>__live__000`, the
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
Traction survey 45/45.

**From contact to damage** (settled against live fights). With the faithful
contacts, Scout v ALARM dealt almost nothing and
`the_shipped_ai_closes_sees_its_enemy_and_lands_blows` failed; a live fight
(`fight-scout-alarm-box__live__000`, `tools/oracle/capture_fight.py` +
`frida/ra2_fight_hits.js`: every `computeDamage` and every arena gate call)
showed ALARM's Hammer Head landing the first counted hit 5.3 s in and 63 in
120 s, where ours priced the hammer at zero. Three pricing rules were missing
and one event rule:

* a weapon that declares no `concussion`/`piercing` keeps the component
  constructor's 1 / 0 (`FUN_00489110` stores `+0x6c = 1.0`; the weapon
  constructor does not touch it) — ours had 0 / 0, so the Hammer Head, Sledge
  Hammer, Iron Fist, Saw Blade, Ninja Star and ChewBlade did nothing;
* a weapon's material float is 0.8 unless its definition names a material
  (`FUN_00502e10`, `0x502e5f`). Live the hammer prices a record at up to
  exactly 8x what the struck Scout part (0.1, concussion 1) prices it at;
* `Weapon::computeDamage` (`FUN_005032f0`) multiplies the base hit by
  `min(1, Σ max(n·f, 0))` over the definition's `normal = <abs> f...` faces
  (`|n·f|` when `abs`), `n` the contact normal toward the victim in the
  weapon's frame: live the hammer's ratio to the base is 0..1 (median 0.79,
  11 % zero);
* the world's collision-event filter (`FUN_0052d4f0`, ahead of the game's
  listener) lets a contact point through only if one of its bodies'
  registrations is due — every robot body at 1 Hz (`FUN_0045ab90` →
  `FUN_004ad500(body, 2, 1.0)`), a weapon with `frequency` at that rate
  (`0x502f7a`) — and the bodies close at the point (`FUN_0052d740`); the
  registration is then reset. Live
  (`fightcontacts-scout-alarm-box__live__000`) the robots touch at up to 13
  points a step, but only 16 % of those steps price a hit, one per component
  pair, and a pair in contact is priced again about 1.0 s later.
  `combat::EventGate`; before, every point of every step piece was priced.

| fight (Box, 120 s, shipped AIs) | original | before | now |
|---|---|---|---|
| Scout v ALARM: ALARM counted / total / first | 63 / 2470 / 5.3 s | 0 | 49 / 1161 / 2.6 s |
| Scout v ALARM: Scout counted / total | 9 / 121 (first 49 s) | 1 / 11 | 0 |
| Razor v FlapJack: Razor / FlapJack counted | 3 (40) / 1 (12), first 6.0 s | 0 / 0 | 8 (265), first 33 s / 0 |

(Before = this branch's contacts without the pricing rules; f6cb227 had 7
Scout hits, from the corner manifold's extra points.) The test now also checks
that within 30 s the blows are ALARM's, as live. AI suite (ten pairings): all
ten deal damage (eight on f6cb227). Captures are in
`local/oracle/original` (`contacts-drive-*`, `contacts-nohaz-*`, `fight-*`,
`fightcontacts-*`, `hinge-octagon`, `agent-dispatch-scout-alarm`; manifest
rows of 2026-10-19).

**Mass**: every stock chassis body with a SmartZone weighs 2 kg more live
(163.49 against our 161.49 kg; Razor, without one, 212.52 on both): the
SmartZone's `.gmf` body (2 kg, `DISABLE_COLLISIONS`), which the renderer's
loader skips. `rig::smart_zone_mass` adds it to the body it is bolted to.

**Hinge direction rule** (`0x4814ca..`): the HingeController takes `+0x70/+0x74`
as min/max of the hinge's limits (`FUN_004801f0`), so the start > end flip
never fires from construction; `hinge + 0x60` is the axis in body B's frame
(`FUN_00525830`). Live (`hinge-octagon__live__000`: Octagon, `Arenas.currentArena.Hammer1.Smash()`,
hinge atom `0x535a50` hooked): the motor velocity is `+94` (the negated
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


## 11. Steering units, the power factor and hit pricing (fx/steering)

### 11a. The `Steering` unit (recovered)

RTTI `.?AVSteering@@`, vtable `0x6ab2dc`. A powered steering unit is a drive
motor *and* a steering rack:

* **wires** (`0x4f5fe0..0x4f6167`): `Forward` (sign `-1`) and `Backward`
  (`+1`) go to its drive handler (`0x4f6600` -> `0x4f6720`: the clamped sum is
  the command `+0x2cc`); `Left` (`+1`) and `Right` (`-1`) to its steer handler
  (`0x4f6680` -> `0x4f6790`). Sentinel wires `Forward`/`Backward` to the
  `Forward` control and `Left`/`Right` to `LeftRight`. (`Hovercraft`,
  vtable `0x6ab6f4`, registers the same names with other signs; no stock robot
  has one.)
* **drive**: its tick (`0x4f6850`) sets `+0x2d0 = ftol(f(ftol(command · g)))`
  (`g` the electric scale `0x714a70`, `f` the health factor `0x45fa90`); the
  thrust is `ftol(+0x2d0 · power · 0.7)` (`0x4f6700`, `power` = the
  definition's `power`, `+0x2e0`, default 1, all three stock units 14),
  doubled into the record like a spin motor's. It draws `|command| · dt`
  (`0x4f6cd0`, vtable slot `+0x5c`), as a spin motor does.
* **steer**: `0x4f6790` gives both hubs (`+0x294`, `+0x298`) the target
  `command · π · 1/6 · 0.01` (`0x6ab2d4`, `0x6ab350`: full lock `π/6`); each
  hub's action (`0x508920`, vtable `0x6ab958`) turns its angle `+0x68` toward
  the target `+0x64` at 4 rad/s (`0x5089d0`, `0x6a7384`) and hands it to the
  Havok wheel constraint (`0x52b610` -> `FUN_00553af0`), which rebuilds its
  axle as `cos θ a₀ + sin θ (a₀ × s)` from the unsteered axle `a₀` (`+0xb8`)
  and the steering axis `s` (`+0x98`).
* **records**: the robot update (`0x4dc912..0x4dca24`) writes, for each of the
  unit's wheels (`0x4f6d00` count, `0x4f6d10` wheel), the thrust (`0x486820`),
  touching (`0x486830`), the normal (`0x486850`), the hub's current axle in
  the chassis frame (`0x4f6d30` -> `0x508a50`, into the record's `+0x3c` by
  `0x486860`/`0x485660`) and its angle (`0x4f6d60` -> `+0x68`, into `+0x24`
  by `0x486890`/`0x4856b0`). `MagicAction` reads neither `+0x24` nor `+0x28`:
  steering acts only through the record's axle, i.e. the push direction
  `d = -(axle × n)` and the sideways impulse along the axle turn with the hubs.

Live (`magic-sentinel__live__000`, the earlier drive test): the two steering
records read thrust `-1960` at full `Forward` and `196` at 10 % (the spin
motors `∓2520`, raw torque 18), their axle `+X` for **both** hubs (the spin
motors' `+X`/`-X`), and at `Turn(+100)` the angle `-0.524` with the axle
`(0.864, ±0.02, -0.503)`; the angle reaches the lock within one sample.

Ours (`drive::Steer`, `Robot::drive_magic`): a steering hub's record carries
the unit, its `power`, the steering axis (the unit's mount point `-Z`, the
chassis' up for every stock mount) and its unsteered axle (the unit's hub
line, model `+Z` from `hub1` to `hub2`, which is `+X` on Sentinel as live);
every step the angle turns toward the steer command's target at 4 rad/s, the
record's axle is rebuilt as the constraint does, and the hub's hinge axis is
turned the same way (`PhysicsBackend::steer_hinge`, havok17; Jolt keeps the
hub unsteered and only the record turns).

### 11b. The power factor `g` (recovered, verified live)

`g` is the power budget's electric scale (docs/30: `clamp(S/D, 0.25, 1)`,
published at `0x714a70` by `0x4dc3f0`), and it enters a spin motor's thrust
**through the percentage only**: the tick (`0x4f59f0`) calls
`SetPercent(ftol(command · g))`, which stores `motor + 0x29c =
ftol(f(percent))` (`0x4f5800`); the thrust (`0x4f5780`) multiplies that by the
hinge controller's `+0xb4` (`0x4d33b0`), the raw torque `SetPowerSettings`
(`0x4810e0`) stored from the definition, which the spin motor's tick never
rescales. Ours multiplied the torque by `g · f` as well.

The robot with an undersized battery is the Razor (Nifty 24000 / 400 for four
spin motors: `S/D = 400 / 396` at full charge and a 99 % command, below 1 after
0.6 s), and its live records show it (`magic-razor__live__000`, `Turn(+100)`):
`1940, 1920, 1900, 1880, 1862` every 0.6 s, which is `2 · trunc(0.7 · 14 · p)`
for `p = 99, 98, 97, 96, 95`. Ours now reads `1960` (a full 100 command),
then `1940, 1920, 1900, 1880, 1862` at 0.1, 0.7, 1.3, 1.9 and 2.5 s; with the
torque scaled too it read 1900 at the second step. The same holds for ALARM,
MiniBerg and Roly Poly (also 24000 / 400 on four motors).
(`duel::tests::an_undersized_battery_lowers_the_percentage_and_not_the_torque`.)

### 11c. Hit pricing with the Havok bodies' own mass properties

`FUN_0045a5a0` (the magnitude `M`) takes its masses from the hit record
(`+0x2c`/`+0x30`, filled by `FUN_0045a430` from each rigid body's `+0x190`
while the flag `0x6f2210` is set, which it is), the lever arm from the
rigid body's centre of mass (`+0x30`) and the inverse inertia from the rigid
body's world tensor (`+0x10c`, `0x45a7f0`). Ours rebuilt all three from the
components as solid boxes (`combat::body_dynamics`, a Jolt-era stand-in), which
for the hammer and the Scout's parts gave a far smaller effective mass: live
the big hammer blows price `amount / rec0 ≈ 8` (128.6 at 17.3 m/s, 103.4 at
12.5, 72.4 at 9.5; `fight-scout-alarm-box__live__000`), ours `≈ 3` (60.6 at
17.5, 32.4 at 12.1). The pricing and the event gate now take the backend's
own mass, centre of mass and inverse inertia (`PhysicsBackend::mass_properties`,
`combat::mass_props`; Jolt keeps the rebuild): ours `≈ 9` (107.5 at 11.8).

The Scout's counted hits live are the reverse pricing of ALARM's hardest
blows (the struck Scout part prices the hammer at `0.1 · M`: 11.43 against
91.19, 10.85 against 86.81), so they need `M ≥ 100`, i.e. ALARM blows of
`≥ 80`; with the box inertia ours never priced one.

The event gate (`FUN_0052d4f0`) was read again: each body's registration list
holds entries `{+4 partner (0 = any), +8 interval, +0xc/+0x10 last time
(integer seconds + fraction), +0x14 closing threshold}`; one body's entries
are tried, then the other's, and the first due entry whose partner matches and
whose `FUN_0052d740` closing test passes lets the point through and is reset,
which is what `combat::EventGate` does. Which point of a step that is depends on the order of
the step's contact points, which follows Havok's agents and the MOPP's triangle
order (not reproduced, section 5); it stays approximate.

### 11d. Validation

**Sentinel, live** (new captures, Combat Arena, hazards off, Sentinel in the
`Team0/Bot0` slot; `capture_magic_set.sh`, which also takes `TURN_THROTTLE`
for a car-steered robot and `CAR_OFF` to clear the AI's `bCarSteering`, whose
`Throttle` adds 10 per call to a slow car and keeps it pushing). Ours is
`fidtrace drive-at` from the captured chassis pose:

| Sentinel | original | before | after |
|---|---:|---:|---:|
| straight 1.98 s from spawn (`magic-sentinel__live__002`): top / t90 | 14.98 / 1.45 | 7.52 / 1.40 | 14.12 / 1.42 |
| straight: distance / coast (m) | 19.79 / 7.88* | 9.76 / 5.39 | 18.44 / 10.56 |
| full throttle + `Turn(+100)`, first 0.9 s (`magic-sentinel__live__001`): heading change (rad) | +1.43 | +0.02 | +1.27 |
| same: path (m) / top speed (m/s) | 4.54 / 8.46 | 2.75 / 5.49 | 4.44 / 8.24 |
| steering records: thrust / angle / axle at `Turn(+100)` | -1960 / -0.524 / (0.866, 0, -0.5) | 0 / - / (∓1, 0, 0) | -1960 / -0.524 / (0.866, 0, -0.5) |

\* the original's coast ends against a wall 1.1 s after the stop.
Before, the steering hubs pushed nothing (the spin motors' 2520 alone) and
did not steer; the circle's 0.02 rad was the free hubs' drag.

**Drive-at table** (section 10's, hazards off, every step):

| robot | metric | original | before | after |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist / coast / turn | 11.01 / 1.47 / 14.29 / 7.89 / 10.66 | 10.92 / 1.47 / 14.25 / 7.24 / 10.68 | unchanged |
| ALARM | top / t90 / dist | 13.85 / 1.22 / 14.15 | 13.88 / 1.27 / 14.20 | 13.77 / 1.23 / 14.12 |
| ALARM | coast / turn | 11.15 / 12.1 | 10.92 / 10.99 | 10.82 / 11.02 |
| FlapJack | top / t90 / dist / coast / turn | 10.31 / 1.45 / 12.41 / 6.78 / 10.16 | 10.45 / 1.43 / 12.27 / 6.82 / 9.09 | unchanged |
| Razor | top / t90 / dist | 17.50 / 1.52 / 23.07 | 16.51 / 1.42 / 22.40 | **16.99 / 1.35 / 22.97** |
| Razor | coast* / turn | 5.72 / 12.2 | 7.84 / 12.18 | 6.11 / 12.27 |
| Wide Load | top / t90 / dist / coast* / turn | 13.90 / 1.55 / 19.49 / 8.61 / 12.37 | 13.47 / 1.62 / 18.09 / 8.39 / 12.83 | unchanged |

Only the two robots whose battery is undersized moved (the torque is no longer
scaled by `g`). Traction survey: 45/45.

**Fights** (Box, 120 s, shipped AIs; counted = `>= 10`; ours with
`FIDTRACE_HITS`, which dumps every priced hit in `fight_report.py`'s terms):

| fight | original | before | after |
|---|---|---|---|
| Scout v ALARM: ALARM counted / total / first | 63 / 2470 / 5.3 s | 49 / 1161 / 2.6 s | 69 / 1544 / 2.6 s |
| Scout v ALARM: Scout counted / total / first | 9 / 121 / 49.4 s | 0 | 1 / 14 / 51.7 s |
| Razor v FlapJack: Razor / FlapJack counted (total) | 3 (40) / 1 (12), first 6.0 s | 8 (265) / 0, first 33 s | 2 (39) / 0, first 27.8 s |

The Razor's over-pricing went with the torque fix (its spike strip now meets
FlapJack at other moments; with the torque fix alone, box inertia: 3 (90),
first 27.8 s) and the inertia fix (2 (39)). Its first counted hit stays late:
our first contact, 3.4 s in at 10.5 m/s, is FlapJack's light wedge body
against the Razor's chassis (7.35, under the gate), where the original's first,
6.0 s in at 6.4 m/s, is chassis against chassis (12.22 both ways). That is
the fight's approach, not the pricing.

**Sentinel v Wide Load on the Compressor** (hazards on, 120 s; live
`fight-sentinel-wideload-compressor__live__001`, its `.speed.tsv` the
positions):

| | original | before | after |
|---|---|---|---|
| robots in contact | first counted hit 2.0 s in | 36.6 s | 8.0 s |
| Wide Load counted / total / first | 60 / 1778 / 2.0 s | 48 / 2109 / 36.5 s † | 84 / 2736 / 8.0 s |
| Sentinel counted / total / first | 2 / 23 / 28.8 s | 5 / 78 / 58.2 s † | 4 / 45 / 25.9 s |
| path Sentinel / Wide Load (m) | 44 / 80 | 137 / 265 | 216 / 246 |

† applied damage events of the old binary (no hit dump), attributed by who
dealt damage that step. It is a fight now, as in the original. (`__live__000` is not one: the brains
were still neutered by an earlier capture in the same game session, since
`capture_drive.NEUTER` patches the brain classes' `Think`; a fight capture
needs a fresh session.)

**AI suite** (ten pairings, `fidtrace match`): before 10/10 deal damage, after
9/10. The one that stopped is Octagon Scout v ALARM, and it is not the
change's: in both trees ALARM is thrown out of the ring at 1.4 s, at 20 m/s
from (2.7, -0.1, -2.2) by the hellraiser it drives over (`HellRaiser` at
`(3.56, -0.15, -0.26)`, `SetPowerSettings(80, 1500)`), and lands outside at
y 2.5. The trees are identical to 7.6 s; before, ALARM happened to slide off
its perch at 9 s and fight, after it stays there and is immobilised at 24 s.
The live Octagon match (`fight-scout-alarm-octagon__live__000`) has nobody
launched (ALARM's highest point is y 1.6), ALARM landing 24 counted hits (944)
and the Scout 4 (48) before it drops into a deathpit at 67 s. It was not the hellraiser's violence
but its trigger: section 12.

**FlapJack's turn** (9.09 against 10.16, item 4 of the task): not cheap. Ours
reaches the same 11 rad/s plateau as the original but spins up more slowly
(4.7 rad/s at 0.5 s, 7.5 at 1.0 s; the original 6.9 and 11.2). Both the
original's chassis (its casters) and the 27 kg wedge body touch the floor on
about 90 % of the steps of the straight capture (`contacts-nohaz-flapjack`),
so the extra drag is not a contact that should be missing; not resolved.

## 12. Motor controllers, burst strokes and SmartZones (fx/load-limit)

New live captures (fresh sessions, `tools/oracle/frida/ra2_hinge_ctl.js` via
`ra2_hook.py`; `tools/oracle/capture_weapons_set.sh` + `capture_weapons.py` +
`frida/ra2_weapons.js` with the AIs neutered and hazards off):
`hingectl-scout-alarm-box__live__000`, `hingectl-scout-alarm-octagon__live__000`,
`weapons-tornado-ripblade__live__000..002`, `weapons-scout-alarm__live__000`.
The model is `ra2sim::motor`.

### 12a. The hinge controller (recovered, verified live)

`HingeController` (vtable `0x6a7e44`) and the prismatic one (`0x6a91b8`) are
Havok actions: their slot-5 updates run every substep (live, 302 world steps
and 11 x 302 hinge updates in 5 s, all called from `0x531030`).

* `SetPowerSettings(S, T)` (`0x4810e0`) stores `+0xb0 = S`, `+0xb4 = T`, the
  motor speed `+0xbc = S` and maximum torque `+0xb8 = 40 T`.
* **The load cap** is vtable slot 10, `0x480510`. A robot calls it for every
  controller of every component (`0x4dc390`) each time a component is added
  while it is built (`0x4dc287`, so the last call, with every body complete,
  holds) and again when one comes off (`0x4dbe3f`, `0x417c1d`; live the Scout's
  fork was re-capped from 462 to 296 N·m at 5.96 s in the Octagon when its
  attachments broke off). With `m` the mass of the hinge's body B (the child),
  `d = pivot_B · perp_B` (the pivot from B's centre of mass, in B's frame,
  along the constraint's reference perpendicular `+0x80`, which `0x525830`
  makes `normalize(a × Y)` for `|a.x| ≥ 0.57`, else `a × X`, in the robot
  frame, section 8):
  `+0xb8 = min(75 T, 25 m (1 - |d|))` (`75 T` when `m (1 - |d|) <= 0`) and
  `+0xbc = S · min(1, 2 T / (m (1 + |d|)))`. The arena's hinges are not a
  robot's and are never capped.
* **The update** (`0x481450`): with the integer percentage `p` (`+0x7c`) the
  motor runs at `trunc(p · 0.01f · +0xbc)` (x87, `__ftol` truncates; `0.01f` is
  just under a hundredth, so a full command is one short of an integer speed)
  and `+0xb8`, sign-corrected; `p = 0` turns the motor off, or locks the hinge
  when it auto-locks. A spin motor's `p` is `ftol(f(ftol(command · g)))`
  (section 11b): neither `f` nor `g` touches the speed or the torque.

Live against ours (`HingeSettings`, `RA2_MOTOR_DEBUG=1` prints them):

| controller | definition | live `+0xb8` / motor | ours |
|---|---|---|---|
| Scout, ALARM drive (27 kg wheels, `d` 0) | `18 14` | 675 / 17 rad/s at 100 % | 675 / 17 |
| Scout fork arm (26 kg, `d` 0.288) | `5 35` | 462.4 | 451.2 (`d` 0.306) |
| Tornado spinner (125 kg) | `36 48` | 3123.9 / 27 | 3123.9 / 27 |
| Ripblade disc (53 kg) | `36 24` | 1324.5 / -32 | 1324.5 / -32 |
| Octagon hammer (arena) | `95 1200`, `3 300` | 48000 / 94, 12000 / -2 | 48000 / 94, 12000 / -2 |

Before, ours ran the definition's `40 T` at the full speed and scaled both by
`f · g`: Tornado's spinner at 1920 N·m toward 36 rad/s. Live and ours now
plateau alike: Tornado's shell 14-16 rad/s (ours 15.0), Ripblade's disc ~17.5
(ours 17-18), both up within 0.5 s, and with the motor off a spinner coasts
(live 15.5 rad/s, nearly undamped, over 4 s).

### 12b. Burst motors and burst pistons (recovered, verified live)

A burst motor's definition carries two pairs: `burst = <n> <speed> <torque>`
and `power = <speed> <torque>` (`burst motor.txt`: `0 50 90`, `5 35`).
Firing (`0x446810`, while home and armed) unlocks, sets
`SetPowerSettings(f(g · 50), f(g · 90))` and `+100`, and starts a 0.6 s timer
(`+0x298`); the hinge locks itself on arriving within 0.05 rad of the end.
When the timer runs out (`0x446670`) it unlocks, sets
`SetPowerSettings(f(g · 5), f(g · 35))` and `-100`, and it may fire again once
locked within 0.05 of the start (`0x4810b0`). Live (Scout's fork): the motor
at -49 rad/s / 3600 N·m for exactly 36 substeps, then 4 rad/s / 1400 N·m; the
fork at its end 6-7 substeps after the trigger (peak 37 rad/s), home 22
substeps after the cock began. Ours fired at the *cocking* pair's 5 rad/s
and cocked at an invented 15 % (`COCK_FRACTION`, gone).

A burst piston (`0x448750`): unlock, a one-substep push `0x4b51b0(-1000 g)`,
auto-lock off, direction `ftol(f(100 g))` for 0.25 s, then auto-lock on and
direction `ftol(f(-100))`. Its prismatic controller (`0x4b5240`, skipped while
locked) is a **force**, not a motor: while the rod is short of the stop it is
driven to (`+0x70` for a positive direction, `+0x6c` for a negative one, 0.05)
it pushes `-(dir · F)` along the axis (`SetPowerSettings(S, F)`, `0x4b2af0`,
raw: no x40), unless the rod already moves that way at `S` or faster; at the
stop it locks if it auto-locks. Live (ALARM's `burstpiston2`, `60 100`): stops
`+0x6c = 0`, `+0x70 = -0.73`, forces -1000 then -10000 a substep, out to -0.73
in 6 substeps, held to 0.25 s, back to 0 in 5 and locked. So a burst piston
travels `[lower, neutral]` only; ours allowed `[lower, upper]` and its rod
slammed through home to `+0.2`. Ours now (`Robot::drive_burst`,
`drive_prismatic`): out in 5 substeps, back in 5. A servo piston is the same
controller (direction = its percentage, stops lower/upper) — not driven live.

### 12c. Switches take every AI press as a toggle (verified live)

`weapons-tornado-ripblade__live__002`: Tornado's `Spin` switch came on at
`Input("Spin", 0, 1)`, went off at a second `Input(.., 1)`, stayed off through
`Input(.., 0)`, came on at the next `1` and stayed on through the next `0`.
Ours set a switch to the last value's level; `ControlState::input` now
toggles on every non-zero call, as a key press does.

### 12d. SmartZones need an overlap (inferred from live behaviour)

The Octagon's hellraiser that threw ALARM out at 1.4 s was raised by
`hellzone2`: ours tested each component's *bounding sphere* against the zone.
Live, ALARM drives the same line past it (at 1.34 s at (2.56, -2.28), ours
(2.56, -2.30) at 1.28 s) and the hellraiser never rises (no
`SetPowerSettings(80, 1500)` in 150 s, `HellRaiser.timer` never reset). With
the component's collision pieces tested against the zone box (separating axes,
`Zone::touches_points`) ALARM's chassis still comes within 0.1 m (the world's
tolerance) but does not overlap; with zero tolerance nothing triggers, which
is what ours now does.

### 12e. Validation

**Drive-at table** (section 11d's, hazards off):

| robot | metric | original | before | after |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist / coast / turn | 11.01 / 1.47 / 14.29 / 7.89 / 10.66 | 10.92 / 1.47 / 14.25 / 7.24 / 10.68 | unchanged |
| ALARM | top / t90 / dist | 13.85 / 1.22 / 14.15 | 13.77 / 1.23 / 14.12 | 13.96 / 1.27 / 14.23 |
| ALARM | coast / turn | 11.15 / 12.1 | 10.82 / 11.02 | 11.07 / 11.02 |
| FlapJack | top / t90 / dist / coast / turn | 10.31 / 1.45 / 12.41 / 6.78 / 10.16 | 10.45 / 1.43 / 12.27 / 6.82 / 9.09 | 10.46 / 1.47 / 12.05 / 6.97 / 9.25 |
| Razor | top / t90 / dist / coast / turn | 17.50 / 1.52 / 23.07 / 5.72 / 12.2 | 16.99 / 1.35 / 22.97 / 6.11 / 12.27 | unchanged |
| Wide Load | top / t90 / dist / coast / turn | 13.90 / 1.55 / 19.49 / 8.61 / 12.37 | 13.47 / 1.62 / 18.09 / 8.39 / 12.83 | 13.13 / 1.60 / 17.91 / 9.17 / 12.85 |
| Sentinel (default spawn) | straight top / dist / coast; circle heading / path / top | - | 14.52 / 18.71 / 10.17; 1.20 / 4.40 / 8.06 | unchanged |

The changes are the wheels' in-air servo (17 rad/s at 675 N·m instead of 18 at
560 · f · g) and the pistons now locked at rest; Wide Load's top speed moves
within its run-to-run scatter (the speed traces agree to 0.2 m/s).

**AI suite** (ten pairings): before 9/10 deal damage, after **10/10**. The
Octagon's Scout v ALARM meets in the middle at 2 s as live, ALARM landing 6
counted hits (284) before the Scout is out at 64 s (live: 24 (944), the Scout
into a deathpit at 67 s); before, nobody dealt damage.

**Fights** (Box/Compressor 120 s, shipped AIs, counted = `>= 10`):

| fight | original | before | after |
|---|---|---|---|
| Scout v ALARM: ALARM counted / total / first | 63 / 2470 / 5.3 s | 69 / 1544 / 2.6 s | 12 / 595 / 4.6 s, fight over at 59 s |
| Scout v ALARM: Scout counted / total / first | 9 / 121 / 49.4 s | 1 / 14 / 51.7 s | 4 / 48 / 14.6 s |
| Razor v FlapJack: Razor / FlapJack counted (total), first | 3 (40) / 1 (12), 6.0 s | 2 (39) / 0, 27.8 s | 2 (32) / 1 (14), 10.5 s |
| Sentinel v Wide Load: Wide Load counted / total / first | 60 / 1778 / 2.0 s | 84 / 2736 / 8.0 s | 80 / 2230 / 6.9 s |
| Sentinel v Wide Load: Sentinel counted / total / first | 2 / 23 / 28.8 s | 4 / 45 / 25.9 s | 6 / 72 / 37.2 s |

Scout v ALARM in the box now ends early: the Scout's fork, at its live 49 rad/s
and 3600 N·m, flips ALARM onto its back (immobilised at 59 s), which the
original's does not. The cause is not found. Ruled out (`bodies-scout-alarm__live__000`,
`frida/ra2_body_dump.js`: raw dumps of the fork's hinge atom and bodies; body
`+0x190` mass, `+0x98` inverse mass, `+0x30` centre, `+0x10c` world inverse
inertia, three rows of stride 16, `+0x19c` the local inertia):

* the arm's mass (26 both) and inertia (principal moments live 2.57 / 2.51 /
  0.17 kg·m², ours 2.64 / 2.54 / 0.16), the chassis' (163.49 both);
* the motor's clamp: one `maxTorque · dt` per substep in both, and the
  per-substep acceleration agrees (live 18.5 then 37.3 rad/s, ours 17.8 then
  35.9; an earlier reading of "twice as fast" compared the wrong rows).

What differs: live, the motor runs two substeps with the arm barely moving
(3.8, 2.3 rad/s) before it accelerates, and over its last three substeps it
slows (25, 13, 9 rad/s) with a backstep in between (two solver calls in one
substep: a new contact), before locking at the end; ours locks at its end at
the peak speed. Our robots' chassis centres of mass also sit 1.5-4 cm lower
than live (in the robot frame, `y`: Scout 0.159 against 0.195, ALARM 0.238
against 0.263, FlapJack 0.183 against 0.208); taking the chassis piece's box
centre instead of its volume centroid closes half of `y` but moves `z` the
wrong way, so the rule is not that either. A lower centre makes ours harder
to flip, not easier. Traction survey: 45/45. (Section 12f: the two weak
substeps were a misreading of the capture; the centre of mass, inertia and
wheel-record rules are recovered; the flip remains.)

**FlapJack's spin-up** (task of section 11d): not the load cap — a grounded
wheel's hinge motor is off. Part of it is the wheel records' hub offsets
(`+0x2c`): live (0.465, -0.152, -0.405) and (-0.450, -0.145, -0.394), ours
(0.421, -0.109, -0.463) and (-0.386, -0.109, -0.466), 12 % less lever for the
yaw torque. Forcing the live offsets, ours reaches 4.6 rad/s at 0.4 s (was
4.0, live 5.8) and turns at 9.9 rad/s (was 9.25, live 10.16). The live offsets
are not hub minus final chassis centre of mass (live that is (0.436, -0.123,
-0.425)): `AddWheel` (`0x485490`) stores them while the robot is still being
assembled, and the chassis body's centre at that moment was not recovered.
Our chassis centre of mass also sits 2.6 cm lower and 3.5 cm further forward
than live ((0.003, 0.183, -0.053) against (-0.001, 0.209, -0.088) in the robot
frame, `assembly-flapjack__live__000`).

### 12f. Burst timing, the motor's target and the bodies' mass properties (fx/physics-flip)

New live captures (fresh sessions): `firetiming-scout-alarm__live__000`
(`frida/ra2_fire_timing.js` through `capture_weapons.py AGENT=`, hazards
off, AIs neutered, the Scout fires twice), a second and a third box fight
(`fight-scout-alarm-box__live__001` with `.pose.tsv` from `ra2_python.py
--sample`; `fightpose-scout-alarm-box__live__000`, `frida/ra2_magic.js`: both
chassis bodies every step). Diagnostics: `examples/comprobe.rs` (bodies' mass,
centre and inertia; `RA2_MASS_DEBUG=1` prints every component's mass point),
`forktrace.rs` (a burst motor's speed every substep), `fightprobe.rs` (both
chassis' height and up vector in an AI fight), `fidtrace match` with
`FIDTRACE_WARMUP=<n>` (n idle steps first: one fight as an ensemble).

**How a step is split (verified live).** Per step piece the actions run in
this order: the hinge controllers (`0x481450`), then the constraint solver
(`0x535a50`). A static reading suggested the opposite, but the live capture
shows the controllers first. A burst motor fires in the game update after a
world step (`0x446810`): it unlocks (`Lock(0)`), and in the next substep its
controller switches the motor on before the solver runs. A step that
backsteps is two pieces (live: `h = 1/120` twice), and each solver call still
uses `dt = 1/60` for the servo and the limit (live H rows); ours already
does both. The limit is the predictive clamp of section 2, in the motor's own
row. Factor 1, no softness, the motor not cut.

The "two near-zero substeps" of section 12e came from a misreading: the H
rows sample the bodies *before* each solver call. Live, the fork's speed
entering the solver calls after the fire reads 1.2, 3.5, 18.4, 37.1, (split
piece) 24.7, 13.1, 9.7, 0.2, then it locks at -1.18 rad. That is one weak
call, its target 0 against an angle of -0.03, then the cap. Ours, after each
substep: -1.9, 17.3, 35.3, 19.4, 9.3, then it locks: the same start, and the
end one substep earlier. Live, the 37 rad/s substep splits (a contact
backstep) and ours does not. Which contact that is was not identified.

**The target angle is never reset (recovered, verified live).** When a
motor comes on, the solver atom keeps its old target (`atom + 0x98`). It is 0
from construction and only shifted by whole turns to within π of the angle
(`0x5355a0`). Live: the fork locks with its target at -2.589, and the cocking
motor starts 29 substeps later from -2.589 at an angle of -1.161. Ours used to
reset the target to the current angle. It now keeps it
(`Havok17Backend::set_motor`; test
`a_motor_switched_back_on_keeps_its_old_target`).

**Mass properties (recovered live, `tests/mass_live.rs`).** Each body is the
sum of its components' rigid bodies, every one at its own mass:

* **Where each rigid body's mass sits.** At the centroid of the solid its
  geometry bounds: a mesh body's own triangles (`SIMULATION_GEOMETRY 6`, and
  the chassis mesh, which is not convex for FlapJack), a convex body's hull.
  The collision proxy counts when one is set.
* **Components with nothing to collide count as well.** Power cells, the
  control board and air tanks sit at their own geometry. A SmartZone's 2 kg
  sits at its zone box: about a metre in front of the robot.
* **How a component's mass is shared.** In proportion to its rigid bodies'
  `.gmf` masses, or by volume when they store none (the stock wheels).
  Phantoms keep their own masses.
* **Steering hubs.** A `Steering` unit's `hub1`/`hub2` (5 kg each) belong to
  the hub bodies, not the chassis. Live, Sentinel's hub bodies weigh 23 kg.
* **Inertia.** Each rigid body's own inertia (its hull at uniform density,
  scaled to its mass), carried to the body's centre of mass and summed
  (`BodyDesc::inertia`, which havok17 takes as given). Live, the Scout's
  chassis reads (23.2837, 39.0837, 21.2655) on the diagonal and (-0.2644,
  1.2629, 0.3957) off it (`bodies-scout-alarm__live__000`, body `+0x19c`).
  Ours now gives the same to four digits. The old one (the colliding hulls at
  one density) read (23.36, 44.34, 27.32) on the diagonal.

Chassis centre of mass in the robot frame, 14 robots
(`assembly-*__live__000`):

| | before | after |
|---|---:|---:|
| y error (all lower than live) | 1.6-3.6 cm | ≤ 5 mm |
| largest error | 5.9 cm (Grog, z) | 6.2 mm (Tornado) |
| robots within 0.5 mm | 1 (Coal Miner) | 10 |
| Sentinel / BEAR chassis mass | +10 kg (hubs) | exact |

Scout 0.159 → 0.195 m (live 0.195), ALARM 0.238 → 0.262 (0.263), FlapJack
0.183 → 0.208 (0.208).

**The wheel records (`AddWheel`, recovered live).** The record's offset
(`+0x2c`) is the wheel's own mass point (its rigid bodies, without the
motor's rotor it rides) less the chassis body's centre of mass at that moment
of the build. That centre is the one of the chassis members that come before
the wheel in the `.bot` (`0x4dc180` adds them in order). All 18 live records
of FlapJack, Sentinel, Razor, Wide Load and ALARM (`magic-*__live__*`) agree
to 0.5 mm; before, the error was up to 7 cm. FlapJack now turns at 10.09
rad/s (live 10.16, before 9.25).

**Still open.** The Scout's fork arm: live, its centre of mass is 1.8 cm
nearer the pivot (load lever 0.288, ours 0.306). Its principal inertia is
(0.171, 2.513, 2.573) live against (0.157, 2.719, 2.765) ours: the arm's mass
is less spread out than our three bodies (extender 2 kg, fork 12, the motor's
`bmotor_axle` 12) put it. Using the fork's display mesh or the proxies'
transforms does not explain it.

**Tipping in the original.** Ours flips ALARM, but the original tips robots
too:

* `fight-scout-alarm-box__live__001` (ALARM 77 counted hits, 2729):
  `plus.isUpsideDown` reads ALARM upside down when the sampling starts (21.6 s
  elapsed) and again from 22.9 to 24.7 s. Then it is upright again, and its
  centre of mass sits 0.36 m higher upside down than upright.
* `fightpose-scout-alarm-box__live__000`: the *Scout* goes onto its side at
  17.4 s and stays there. ALARM parks from 38 s.

So a fight that ends early is not a defect in itself. One run of a fight is
chaotic. The difference that remains is that the original's ALARM has been
seen to recover from upside down, and ours, once on its back or side, stays
there. Ours rests 8 cm lower upside down in the box (centre of mass 0.67 m
above the floor, live 0.75 m).

**Validation.** Before = `fidelity` beb1249, after = this branch.

Drive-at table (section 11d's runs, hazards off):

| robot | metric | original | before | after |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist / coast / turn | 11.01 / 1.47 / 14.29 / 7.89 / 10.66 | 10.92 / 1.47 / 14.25 / 7.24 / 10.68 | 10.82 / 1.45 / 14.25 / 7.21 / 10.77 |
| ALARM | top / t90 / dist / coast / turn | 13.85 / 1.22 / 14.15 / 11.15 / 12.1 | 13.96 / 1.27 / 14.23 / 11.07 / 11.02 | 14.00 / 1.27 / 14.11 / 9.20 / 11.30 |
| FlapJack | top / t90 / dist / coast / turn | 10.31 / 1.45 / 12.41 / 6.78 / 10.16 | 10.46 / 1.47 / 12.05 / 6.97 / 9.25 | 10.39 / 1.50 / 11.70 / 6.85 / **10.09** |
| Razor | top / t90 / dist / coast / turn | 17.50 / 1.52 / 23.07 / 5.72 / 12.2 | 16.99 / 1.35 / 22.97 / 6.11 / 12.27 | 16.68 / 1.35 / 22.46 / 7.83 / 12.50 |
| Wide Load | top / t90 / dist / coast / turn | 13.90 / 1.55 / 19.49 / 8.61 / 12.37 | 13.13 / 1.60 / 17.91 / 9.17 / 12.86 | 13.36 / 1.65 / 17.79 / 9.53 / 13.19 |
| Sentinel | straight top / t90 / dist / coast | 14.98 / 1.45 / 19.79 / 7.88 | 14.49 / 1.50 / 18.88 / 10.26 | 14.55 / 1.47 / 19.36 / 9.29 |
| Sentinel | circle 0.9 s: heading / path / top | 1.43 / 4.54 / 8.46 | 1.27 / 4.46 / 8.22 | 1.38 / 4.59 / 8.57 |

The results are mixed: FlapJack's turn and Sentinel are closer, and ALARM's
coast (-1.9 m) and Razor's coast are worse. Along the live capture's own
straight (`magic-wideload__live__000`, from (10.0, -9.55) at -1.636 for 2 s),
Wide Load now drifts 0.05 rad, against 0.55 rad with the old wheel records
(live 0.01).

Fights as ensembles (six runs each, `FIDTRACE_WARMUP` 0/7/13/19/26/31; mean
counted hits (mean total), mean length, runs that last 120 s):

| fight | original (one run each) | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM | 63 (2470); 77 (2729) | 17.3 (917), 67 s, 1/6 | 14.3 (715), 64 s, 1/6 |
| box Scout v ALARM: Scout | 9 (121); 6 (71) | 5.2 (66) | 5.2 (71) |
| box Razor v FlapJack: Razor / FlapJack | 3 (40) / 1 (12) | 6.2 (157) / 0.5 (6), 120 s, 6/6 | 2.8 (73) / 1.0 (16), 93 s, 2/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 60 (1778) / 2 (23) | 38.3 (1117) / 2.8 (40), 93 s, 4/6 | 46.2 (1476) / 2.8 (45), 113 s, 4/6 |
| Octagon Scout v ALARM: ALARM / Scout | 24 (944) / -, Scout out at 67 s | 6.0 (352) / 1.5 (20), 47 s | 2.0 (143) / 1.2 (16), 27 s |

The fork still flips ALARM: in the Octagon, after 6 s in every run. The
ensembles did not improve on the whole: the Compressor is better, Razor v
FlapJack and the Octagon are worse. AI suite (ten pairings, three warmups):
before 29/30 runs deal damage (warmup 0: 10/10), after 28/30 (warmup 0: 9/10).
Before, the failure was the Octagon's Scout v ALARM (warmup 7). After, the
hellraiser throws BEAR out of the Octagon at 4.5 s (BEAR v Tornado, warmup 0),
and Razor v FlapJack in the Parking Lot ends at 23 s (warmup 13). Traction
survey: 45/45.

### 12g. Recovering from upside down, the fork arm's body, the stroke's backstep and the coast (fx/physics-recover)

New live captures (fresh sessions, all in `local/oracle/original` with
manifest rows): `forkcontacts-scout-alarm__live__000` (`frida/ra2_contacts.js`
through `capture_weapons.py AGENT=`: the Scout fires twice, every resolver
contact, body and step piece), `contacts-nohaz-alarm__live__001/002` and
`contacts-nohaz-razor__live__001` (repeats of the drive-at straights),
`fight-scout-alarm-box__live__002` with `aistate-scout-alarm-box__live__000`
(`ra2_python.py --sample`: per player `getLocation`, `isUpsideDown`, the
brain's current tactic, its sensor table and `bImmobile`). Diagnostics:
`examples/invertprobe.rs` (settle a robot, roll or pitch it over, watch it
rest and optionally fire its weapons as an `InvertHandler` does),
`fightprobe` now takes `FIDTRACE_WARMUP` and prints the forward axis,
`forktrace` takes `FORKTRACE_SPAWN`/`FORKTRACE_SETTLE` and prints the chassis,
`RA2_H17_BACKSTEPS=1` prints every backstep with the pairs that caused it.

**How the original's ALARM "recovers".** The live poses (`fightpose-scout-
alarm-box__live__000`, its body rotation rows; `fight-scout-alarm-box__live__001`)
show ALARM *standing on its tail*, not on its back: the Scout's fork lifts its
nose, it rotates back onto its rear wheels and the rear edge of its chassis
(the rear wheels stand 0.215 m proud of the vertical rear face), leans 2-14°
past vertical (`isUpsideDown` reads true, up.y -0.02..-0.24) with its centre
of mass 0.9-1.0 m over the floor, and stays there. Both times it came back
because the Scout drove into it and pushed it over forwards (Scout and ALARM
1.5-1.7 m apart, moving together 2 m in 2 s). Its own `InvertHandler`
(`Poker.py`: fire every second) only hops it 0.15 m. Ours can stand on its tail
too (`invertprobe ... 90` with `INVERTPROBE_PITCH`: COM 0.86 m up, up.y -0.09,
stable, the piston hops it 6-8 cm), but our Scout never pushed: two of the
AI's answers were wrong.

* **A robot's SmartZone sees collision geometry** (`GetZoneContents`,
  `0x4014f0`): it walks the list of body pairs overlapping the zone's
  phantom (`0x406d10/0x406d20`) and gives one `(robot << 4 | chassis)` tag per
  entry (`0x414430`). Ours tested each component's *bounding sphere* against
  the zone box; ALARM's chassis sphere is 1.2 m across, so the Scout saw
  ALARM's chassis in its `flip` zone from 1.6 m, aimed (`AimToHeading`, no
  throttle) instead of driving (`Engage`: drive while the chassis is not in
  range), and did not fire because ALARM was upside down: a stalemate until
  ALARM was immobilised. The zones now use the arena zones' separating-axis
  overlap (section 12d, `hazards::box_touches_points`) on every collision
  piece, one reading per overlapping piece, `chassis` for the chassis body's
  pieces (a motor's rotor counts with the body it drives). Live, ALARM's sensor
  on the side-lying Scout counts up to 6 contacts and reads `chassis = 0` while
  only a non-chassis part is inside (`aistate-...__live__000`).
* **`plus.getLocation` / `AI.GetLocation` are the chassis body's centre of
  mass** (robot vtable slot 1 via `0x4cd4e0` / `0x4b8e20`; docs/26: the
  resting Scout and ALARM read -1.884 / -1.811 against bot origins -2.086 /
  -2.074). Ours gave the bot origin, which on a tail-standing ALARM is 0.26 m
  from its centre. Every brain's distances, headings and `DriveToLocation`
  targets read it.

With both, ours recovers as the original does: in the box ALARM goes onto its
tail and is pushed back onto its wheels (e.g. warmup 0: tail at 4 s, upright at
6 s; warmup 13: tail at 8 s, upright at 10 s). What still differs is that ours
also topples *backwards* off its tail onto its back (in 4 of 6 box runs, 1-2 s
after reaching the tail, usually under the next fork stroke while up.y is
still positive and the Scout's `Flipper` fires) and from there does not come
back. Live it has not been seen on its back in four fights; the live Scout
did end on its side against a wall once (`__002`) and was immobilised. On its back ours
lies on its chassis top, COM 0.51 m over the floor (the chassis section is a
trapezoid, 1.76 m wide at the bottom and 0.88 m at the top, so neither side is
a resting face): a pose the original's ALARM was not seen in, so section
12f's "8 cm lower upside down" compared two different poses.

**The fork arm's body (recovered, verified live).** The fork's collision mesh
is wound inside out: its signed volume is negative. `mesh_volume_centroid`
refused it and the fork's 12 kg sat at its *hull* centroid. Havok takes the
solid the triangles bound either way (the signs of the volume and of every
moment cancel). With the mesh's own centroid and its own inertia (a mesh body's
triangles, `massprops::mesh_inertia`, also used for the inertia of every
mesh piece) the arm body reads lever `d = 0.289` (live 0.288), load cap
462.18 N·m (live 462.4) and principal inertia (0.1712, 2.5127, 2.5735) kg·m²
(live 0.171, 2.513, 2.573; the live local tensor's diagonal 2.417 / 0.267 /
2.573 against ours 2.4216 / 0.2624 / 2.5734). Test
`mass_live::the_scout_fork_arm_matches_the_original`; the 14 chassis centres
and 18 wheel records are unchanged (worst 6.2 mm / 0.5 mm).

**The stroke's backstep (identified live).** The contact that splits the
37 rad/s substep is the Scout's **own chassis against the floor**: the fork's
reaction pitches the chassis nose-down at 7.3 rad/s and its two front corners
come within the tolerance (d 0.055-0.097, `forkcontacts-...__live__000`
piece 321). Whether that happens inside one piece depends on millimetres: in
the same capture the second stroke, with the chassis resting 6.5 mm lower,
does **not** split (relative fork speed 17.8, 36.4, 11.4, 15.1, 0.8, against
17.97, 36.71, [split] 32.2, 13.7, 9.6, 0.1 for the first). Ours does both too:
after `forktrace`'s 1 s settle the Scout is still bouncing from its spawn drop
and the stroke runs 19.0, 37.8, 9.5, 14.7, 0.55 (the corners already at 0.081
from the step before: an ordinary new contact, no TOI); settled for 10 s
(`FORKTRACE_SETTLE=600 FORKTRACE_SPAWN=arena`) it runs 18.27, 36.99,
[backstep, pair chassis-floor] 13.58, 9.67, 0.11, the live first stroke.
Nothing was changed for it.

**The coast (explained, not changed).** Both coasts end *against a wall*, live
and ours: ALARM's live straight ends at (-4.16, 11.88), ours at (-1.19, 11.26);
Razor's live at the side wall at 13 m/s, ours in the corner. The "coast" is
where the wall is along the drifted path, not rolling resistance: the path
lengths agree (ALARM live 25.5 / 24.5 m in two good runs, ours 24.9; Razor live
29.3 / 27.2, ours 30.7), the coast-down rate is the same (speed decays about
1.2/s in both; `MagicAction`'s `-0.5 m v` per wheel and `-m v`), all four
records touch throughout in both. The difference is the heading drift: from
0.5 to 1.5 s after the start ALARM turns +0.19, +0.16 rad live and +0.30 rad
ours (three perturbed start headings give 0.298-0.302); Razor turns -0.04 /
-0.02 live and -0.04 ours. Section 12f's wheel records moved ALARM's drift (its
coast fell from 11.07 to 9.20 m) although they now match the live records to
0.5 mm; the yaw that remains is not from `MagicAction` (its yaw sum,
4 · 1940 · 0.01 m, stays under the 145 N·m threshold) but from the contact
dynamics during the pitching, bouncing start, and was not found.

**Validation.** Before = `fidelity` d0dd0a3, after = this branch.

Drive-at table (section 11d's runs, hazards off; "coast" ends at a wall, see
above):

| robot | metric | original | before | after |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist / coast / turn | 11.01 / 1.47 / 14.29 / 7.89 / 10.66 | 10.82 / 1.45 / 14.25 / 7.21 / 10.77 | 10.83 / 1.43 / 14.25 / 7.30 / 10.77 |
| ALARM | top / t90 / dist / coast / turn | 13.85 / 1.22 / 14.15 / 11.15 / 12.1 | 14.00 / 1.27 / 14.11 / 9.20 / 11.30 | unchanged |
| ALARM | path to the wall / drift 0.5-1.5 s | 25.5, 24.5 / +0.19, +0.16 | 24.9 / +0.30 | unchanged |
| FlapJack | top / t90 / dist / coast / turn | 10.31 / 1.45 / 12.41 / 6.78 / 10.16 | 10.39 / 1.50 / 11.70 / 6.85 / 10.09 | 10.25 / 1.48 / 11.67 / 6.61 / 10.01 |
| Razor | top / t90 / dist / coast / turn | 17.50 / 1.52 / 23.07 / 5.72 / 12.2 | 16.68 / 1.35 / 22.46 / 7.83 / 12.50 | unchanged |
| Razor | path to the wall / drift | 29.3, 27.2 / -0.04, -0.02 | 30.7 / -0.04 | unchanged |
| Wide Load | top / t90 / dist / coast / turn | 13.90 / 1.55 / 19.49 / 8.61 / 12.37 | 13.36 / 1.65 / 17.79 / 9.53 / 13.19 | unchanged |

(The Scout and FlapJack change through the inside-out meshes' centroids.)

Fights as ensembles (six runs each, `FIDTRACE_WARMUP` 0/7/13/19/26/31; mean
counted hits (mean total), mean length, runs that last 120 s). The original's
box Scout v ALARM now has three runs: ALARM 63 (2470), 77 (2729), 37 (1810,
`__002`, the Scout immobilised on its side by about 52 s); Scout 9, 6, 7.

| fight | before | after |
|---|---|---|
| box Scout v ALARM: ALARM / Scout | 14.3 (715) / 5.2 (71), 64 s, 1/6 | 11.8 (650) / 4.0 (53), 72 s, 1/6 |
| box Razor v FlapJack: Razor / FlapJack (live 3 (40) / 1 (12)) | 2.8 (73) / 1.0 (16), 93 s, 2/6 | 4.2 (91) / 0.7 (8), 102 s, 4/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel (live 60 (1778) / 2 (23)) | 46.2 (1476) / 2.8 (45), 113 s, 4/6 | 33.0 (1057) / 1.2 (14), 99 s, 4/6 |
| Octagon Scout v ALARM: ALARM / Scout (live 24 (944), Scout out at 67 s) | 2.0 (143) / 1.2 (16), 27 s, 0/6 | 6.0 (344) / 1.8 (35), 53 s, 0/6 |

An intermediate build with one reading per *body* instead of per collidable
gave box Scout v ALARM 19.2 (942) / 4.2, 77 s, 2/6 and the Compressor 54.8
(2110) / 5.8, 107 s, 3/6: the two builds differ by about as much as six runs
scatter (single runs end on LastRobot anywhere from 24 to 120 s), so the
per-collidable reading, which the live sensor counts support, was kept. The
Octagon fights last twice as long and ALARM lands three times the hits.

AI suite (ten pairings, three warmups): before 28/30 runs deal damage (warmup
0: 9/10, BEAR v Tornado), after 29/30 (warmup 0: 10/10; the one without is
Tornado v Ripblade, warmup 13: Tornado parks against the north wall from 3 s
as it does before the change, Ripblade climbs onto it and both sit until one
is immobilised). Traction survey: 45/45.

**Still open.** ALARM topples backwards off its tail onto its back in ours,
which the original has not been seen to do; its straight drifts 0.30 rad
against the original's 0.16-0.19; the live fights' ALARM lands 37-77 counted
hits against our 12 on average.
