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
5. ~~Narrowphase~~: Havok's agents (section 10); the MOPP's triangle order
   and the arena's fixed bodies as collidables of their own (section 12v).
   Still not reproduced: Havok's tie-breaking in GJK (its support search
   hill-climbs from the last support vertex, `0x55b000`), the fixed bodies'
   own frictions, `SIMULATION_GEOMETRY 0` (not observed live; collided as
   convex).
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
every pair of triangles within the tolerance (two moving meshes' trees are
walked together with each box grown by half of it, a whole tolerance between
them: section 12s) gives the midpoint of their closest points
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

### 12h. The arena colliders, the rotors' mass, the heading drift and the tail-stand (fx/physics-drift)

New live captures (all in `local/oracle/original` with manifest rows):
`contacts-nohaz-alarm__live__003/004` and `contacts-nohaz-wideload__live__001`
(drive-at straights; `frida/ra2_contacts.js` now also writes a `P` row per
body, its raw floats once: `+0x30` centre of mass, `+0xcc` transform, `+0x190`
mass, `+0x19c` local inertia), `fightcontacts-scout-alarm-box__live__001..003`
with `aistate-scout-alarm-box__live__001..003` (three box fight starts, 31 s
each, fresh session: contacts agent through `capture_fight.py AGENT=
PREFIX=`, and `ra2_python.py --sample` of every brain's tactic, sensor table
`(robot, contacts, chassis)` and the Flipper's `upTrack`), and
`colliders-box/octagon/compressor__live__000` (every fixed collidable a
resolver contact names, with its type and bounding box). Diagnostics:
`RA2_H17_DUMP=<file>` makes havok17 write the same `R`/`B`/`A` rows as
`ra2_contacts.js` (and `G` rows from `MagicAction`), so one script reads
ours and the original's; `RA2_AI_TRACE` prints each sensor as
`(contacts, chassis)`; `gmfdump` takes `GMFDUMP_POINTS`, `GMFDUMP_TM` and
`GMFDUMP_MATERIALS`.

**The arena collider is the body's own mesh (recovered live).** A body with
`SIMULATION_GEOMETRY` 5 names a display proxy, and docs/06 F5 read that as
"collide as the display group". Live, every arena contact names one fixed
collidable whose bounds are the body's *own* mesh grown by the 0.05 radius:
the Combat Arena's 8-vertex `arena_collision` box ((-12.75, -2.315, -12.447)
to (12.75, 13.785, 13.053)), the Octagon's 65-vertex `main_collision` slab
(±13.91, y -4.084..1.719; the display group spans ±34 m) and the
Compressor's 88-vertex `collision box`; the collidable's header holds the
vertex count (8, 65, 88). The display group's children are not collided
with: in the box, about 780 wheel contacts in ten captures cross the spike
and saw plates (`hazard01/02`, `floor saw plate`, 3-6 cm above the floor)
at floor height, where ours climbed them (5-8 % of ALARM's wheel contacts
had normals tilted up to 42°; live none of about 3000). `collect_havok` now
takes the own mesh when there is one; the arena probe that guards against
a collider without a floor casts from above the arena instead of 5 m over
its lowest visible point (which put it under Hilltop's and Electric's own
floors). Colliders: box 396 → 24 triangles, Octagon → 332, Compressor →
448, Electric/Hilltop now their rigid bodies (were the visible meshes);
FlexTop unchanged (visible). The colliders are closed boxes with a roof, so
a robot placed at an arbitrary `(x, z)` (tests, `drive-at`) is now dropped
from just above the start points' height rather than from above the arena.
Tests `spawn::the_combat_arena_collides_as_its_own_box`,
`ra2_render::tests::arena_collider_is_the_bodys_own_mesh`.

**A motor rotor's mass sits where its open proxy puts it (recovered live).**
The angle motors' and the Z-Tek's rotor proxies (`axle_collision`) are boxes
with no inboard end face (8 points, 10 triangles). Live, every wheel body on
such a rotor has its centre of mass further out along the axle than ours:
9 mm on the Scout, ALARM, FlapJack and Razor (angle motor, live hubs |x|
0.4431/0.4892, 0.522-0.556, ...), 11.2 mm on Wide Load's Z-Tek wheels; the
AxleMount wheels (closed axle box) and every chassis already matched. The
tetrahedra from the hinge pivot to the proxy's triangles (the solid the
surface and the cone to the missing face bound) put the rotor's mass 3/8 of
the box from its closed end: 9.1 mm and 11.8 mm (the closed face itself
would give 11.4). The body's inertia stays the box's: live, ALARM's wheel
spins with 0.5569 kg·m² and Wide Load's with 1.2935, exactly the closed
box's, which the open solid would change (0.5601, 1.316). Transverse, live
0.4281/0.4297 (ALARM) and 1.5039 (Wide Load): ours were 0.4981 and 1.7160
and are now 0.4519/0.4502 and 1.5450 (+5 %, +3 %); the box inertia carried
from the new mass point is the closest rule that keeps the axle moments.
`robot::havok_part` (`MassPiece` carries the rotor's triangles), test
`mass_live::alarms_wheel_bodies_match_the_original`; the Scout's fork arm,
14 chassis and 18 wheel records are unchanged. The other open meshes
(`rotatorbar`, the bracket wedge, the snowplows) are not rotors and keep
their old rule; unverified.

**The heading drift.** Section 12g's "+0.30 rad, stable under
perturbations" came from starting the throttle during the spawn drop; the
live straights start after 1.5 s at rest. With a rest of 0.3-3.5 s
(`driftens`), ours turns +0.262 ± 0.047 (10 runs) against live +0.191 ±
0.033 (five runs, `contacts-nohaz-alarm__live__000..004`: 0.181, 0.190,
0.151, 0.192, 0.242). What is established:

* `MagicAction` is right, also its yaw branch: the 145 N·m test reads the
  first accumulator (`Σ p × drive`, stack `+0x124`; the asm also sums
  `p × drag` at `+0x1e4` and `p × impulse` at `+0x1d4`, which nothing reads),
  and with ALARM's live record points (-0.600, 0.564, 0.561, -0.614) the sum
  is 175 N·m, so the torque *is* applied every step, live and ours (ours
  173 in 94 of 96 throttle steps; the live G records' normals are flat, as
  ours). Section 12g's "under the threshold" was wrong.
* The drift scales with that torque (ours ×0 → -0.03, ×0.5 → +0.13, ×1 →
  +0.26); chassis inertia ×1.5 hardly moves it (0.24): it is set by the
  wheels' yaw resistance, which live is about 1.4 times ours.
* Per wheel, live and ours agree on speed, spin (17, 23, 28, 35 rad/s at
  0.25 s intervals), contacts per wheel (0.6-1.0 per step), contact
  distances, forward slip (+1 → +4.5 m/s), toe/camber flex (±0.3-0.7°,
  ±1-2°), friction 0.3 and restitution 0.09 (contact records `+0x58..0x64`
  read 0.3 per side), and the resistance impulse's yaw torque (-40..-70
  N·m in both, reconstructed from the chassis states).
* Neither the rotor rule nor the plates move it (0.283, 0.247 in single
  experiments; with both, 0.264). Not found.

**The tail-stand and the Scout's sensor.** The live fight starts show the
original doing what ours does: in run 2 ALARM stands on its tail from 15.1
to 23.4 s, leans back to up.y -0.55 and at 23.5 s goes over onto its back
(COM 0.47 m above the floor; ours 0.51), where the Scout pushes it around;
in run 1 it is on its tail for 1 s and comes back. While ALARM stands on its
tail the live Scout's `flip` zone reads its chassis (contacts 1-6, chassis 1)
as ours does, so `Engage` aims rather than drives; but live the reading
drops to `(0, 0)` in 25-45 % of the samples, and in those frames `Engage`
drives and the Scout shoves ALARM. Ours reads a steady `(2-4, True)` for as
long as 12 s (warmup 0, 24-36 s: no push, no fire) until the Flipper's
`CanDriveUpsideDown` latch trips (ALARM more than 3 m from where it was first
seen upside down within 10 s; live it tripped in run 2 too) and from then on
the Scout fires on ALARM whenever it is in the zone. Our zone test agrees
with an exact GJK distance on every reading in that fight, so the
difference is in what the phantom reports, not in our separating-axis
approximation; one candidate is that Havok's phantom loses a collidable that
lies wholly inside it (a penetrating pair reports no points, section 10).
Not verified. On frequency: live, ALARM ended on its back in one of seven
fights or fight starts; ours still in five of six box runs (warmups
0/7/13/19/26/31: back 71/51/54/0/86/94 % of the fight).

**ALARM's hits.** All of ALARM's counted hits are its Hammer Head's, live
(one attacking component: 62 of 63, 77 of 77, 37 of 37) and ours. Ours land
them in the first 20-40 s and then none, because ALARM is on its back; live
it stays upright and keeps landing 10-25 per 20 s. So the count follows the
tail-stand above.

**Validation.** Before = `fidelity` 597a227, after = this branch. Drive-at
table (section 11d's runs, hazards off; the Sentinel rows rebuilt as
`drive-at` from `magic-sentinel__live__002`'s pose (-10.036, 10.208, 2.427),
1.98 s straight, and `__001`'s (-10.154, 10.319, 2.474), full throttle and
`Turn(+100)` for 0.9 s):

| robot | metric | original | before | after |
|---|---|---:|---:|---:|
| Scout | top / t90 / dist / coast / turn | 11.01 / 1.47 / 14.29 / 7.89 / 10.66 | 10.83 / 1.43 / 14.25 / 7.30 / 10.77 | 10.84 / 1.35 / 14.38 / 7.88 / 10.78 |
| ALARM | top / t90 / dist / coast / turn | 13.85 / 1.22 / 14.15 / 11.15 / 12.1 | 14.00 / 1.27 / 14.11 / 9.20 / 11.30 | 14.05 / 1.25 / 14.09 / 9.62 / 11.28 |
| ALARM | drift 0.5-1.5 s after a 0.3-3.5 s rest (10 runs) | +0.191 ± 0.033 (5 runs) | +0.262 ± 0.047 | +0.264 ± 0.069 |
| FlapJack | top / t90 / dist / coast / turn | 10.31 / 1.45 / 12.41 / 6.78 / 10.16 | 10.25 / 1.48 / 11.67 / 6.61 / 10.01 | 10.47 / 1.48 / 11.93 / 6.94 / 10.11 |
| Razor | top / t90 / dist / coast / turn | 17.50 / 1.52 / 23.07 / 5.72 / 12.2 | 16.68 / 1.35 / 22.46 / 7.83 / 12.50 | 17.30 / 1.53 / 22.63 / 7.58 / 12.47 |
| Wide Load | top / t90 / dist / coast / turn | 13.90 / 1.55 / 19.49 / 8.61 / 12.37 | 13.36 / 1.65 / 17.79 / 9.53 / 13.19 | 13.21 / 1.58 / 17.91 / 9.14 / 13.19 |
| Sentinel | straight top / t90 / dist / coast | 14.98 / 1.45 / 19.79 / 7.88 | 14.94 / 1.52 / 19.08 / 9.92 | 14.73 / 1.50 / 18.74 / 9.75 |
| Sentinel | circle 0.9 s: heading / path / top | 1.43 / 4.54 / 8.46 | 1.45 / 4.61 / 8.57 | 1.42 / 4.58 / 8.59 |

Fights as ensembles (six runs each, `FIDTRACE_WARMUP` 0/7/13/19/26/31; mean
counted hits (mean total), mean length, runs that last 120 s):

| fight | original | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 63, 77, 37 / 9, 6, 7 | 11.8 (650) / 4.0 (53), 72 s, 1/6 | 14.2 (765) / 4.5 (59), 82 s, 0/6 |
| box Razor v FlapJack: Razor / FlapJack | 3 (40) / 1 (12) | 4.2 (91) / 0.7 (8), 102 s, 4/6 | 2.7 (58) / 1.0 (13), 107 s, 5/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 60 (1778) / 2 (23) | 33.0 (1057) / 1.2 (14), 99 s, 4/6 | 57.0 (1884) / 6.2 (84), 114 s, 5/6 |
| Octagon Scout v ALARM: ALARM / Scout | 24 (944), Scout out at 67 s | 6.0 (344) / 1.8 (35), 53 s, 0/6 | 6.7 (403) / 2.0 (29), 61 s, 0/6 |

AI suite (ten pairings, warmups 0/7/13): before 29/30 deal damage (warmup
0: 10/10), after 29/30 (warmup 0: 10/10; Tornado v Ripblade at warmup 13 in
both). Traction survey 45/45.

**Still open.** The heading drift (ours 0.26, live 0.19, the yaw resistance
of the wheel contacts); the Scout's sensor that never lets go of a
tail-standing ALARM, and with it ALARM ending on its back in five of six box
runs and its counted hits; the wheels' transverse inertia (+3-5 %); which
other `.gmf` children or open meshes the original drops or treats as here.

### 12i. What a SmartZone sees, the arena colliders elsewhere, the plows (fx/physics-zone)

New live captures (fresh sessions, all in `local/oracle/original` with
manifest rows, `manifest-additions-2026-10-21-physics-zone.tsv`):
`zonelist-scout-alarm-box__live__000..002` with
`aistate-scout-alarm-box__live__004..006` (three box fight starts, 45 s,
hazards on, shipped AIs; `frida/ra2_zone.js` through `capture_fight.py AGENT=
PREFIX=zonelist`), `colliders-{bridge,electric,halfcircle,metalskull,
parkinglot,hilltop,flextop,clawtop}__live__000` (`frida/ra2_colliders.js`,
40 s AI fights) and `assembly-{berserker,dadog,bigdog,snowjob}__live__000`.
Diagnostic: `examples/zoneprobe.rs` (places both chassis at live poses and
tests a robot's zones against the other's chassis as a hull and as its mesh,
and the two zones against each other); `arenadump` prints bounds to the mm.

**The phantom list (recovered, verified live).** `GetZoneContents`
(`0x4014f0`) copies the world's phantom pair list (`0x4ae1f0`: the collision
detector at `*(*(*0x714670 + 0x20) + 0x18)`, vector at `+0x100`) and, for every
pair holding one of the zone's collidables, reports the other collidable's tag
(`+0x54`, `robot << 4 | chassis`, `0x414430`), skipping tag 0 (the arena). The
list is cleared with the contacts before every detection (`0x542660`) and only
the collision agents fill it: when a pair with a phantom (`+0x58`) on either
side comes out of the closest-point query at a **negative** distance, the
convex agent (`0x544820`) and the convex-mesh agent (`0x545b50`) push it there
(`0x5444a0` for the implicit ones); a phantom pair within the tolerance
reports nothing at all. The convex-mesh agent runs that query per *triangle*
the MOPP returns near the convex piece and stops at the first that overlaps,
so a mesh collidable (every chassis, the `SIMULATION_GEOMETRY 6` parts) is in a
zone only while one of its triangles crosses it. A zone that sits wholly inside
a chassis' volume does not see it. That is the original's dropout: when the
Scout's nose is under a tail-standing ALARM its flip zone lies inside ALARM's
chassis.

Live, three fight starts, 261 `GetZoneContents` calls with both chassis
poses (`zoneprobe` on `zonelist-*`):

| rule | Scout's zone has ALARM's chassis | ALARM's zone has the Scout's chassis | zone v zone |
|---|---|---|---|
| hull (before) | 192/261 agree (ours reads it in 133, live 64) | 248/261 | - |
| mesh triangles (now) | **261/261** | **261/261** | 259/261 |

The list also holds the other robot's **phantoms**, which the zone counts:
its wheels' traction rings (tag `robot << 4 | 4`) and its SmartZone (each robot
has one convex phantom on the chassis body, tagged non-chassis). Ours now
counts per collision piece with the mesh rule, the traction rings (their
convex hulls) and the other robot's zone boxes, all but the chassis' own
pieces as `chassis = 0` (`duel::watch_brain_zones`, `zone_pieces`,
`hazards::box_touches_piece`; test
`hazards::a_zone_inside_a_mesh_body_sees_none_of_its_triangles`). The
arena's registered zones take the same per-piece mesh rule (they were the
union of a component's pieces as one hull); whether they also see the
traction rings was not checked.

Effect on the box fight (`fightprobe`, twelve warmups 0..71, ALARM's pose):
before, ALARM ends on its back in 11 of 12 runs (upright 25 % of the time on
average); now in 6 of 12 (upright 52 %), against live 1 in 7. With only the
mesh rule and without the phantoms: 9 of 12.

**The arena colliders elsewhere (verified live).** Every arena's fixed
collidables are the bodies' own meshes grown by 0.05, with the vertex count in
the header, as section 12h found for three: Bridge of Doom `h_floor` (24),
Electric `Collision Box` (16), Half Circle `Collision Box` (38) and its `Hammer
Stop Zone` box, Metal Skull `structure_collision` (50), Parking Lot
`main_collision` (12), Hill Top `Collision Box` (33; its bounds reach y -0.433
through a vertex no triangle uses), ClawTop `main_collision` (56): all match
ours to the millimetre. **FlexTop is the exception**: its only fixed
collidable is `Plane01`, an implicit plane (type 16) at y -9.813; the table is
the 400 kg dynamic `floor_collision`. Ours finds no floor among the fixed
bodies and falls back to the visible geometry (1436 triangles) as a static
collider; with the rigid bodies only (`RA2_COLLISION=rb`) our match ends at
0.02 s with both robots out, so the dynamic table needs work first. Left open.

**The plows and the rotator bar.** Da Dog, Big Dog (`sd_snowplow`) and
SnowJob (`snowplow`) chassis bodies match live to 0.1 mm in mass and centre
(`tests/mass_live.rs`, now 17 robots). Berserker's (`rotatorbar`) does not:
chassis 117.74 kg at (0.0016, 0.2404, 0.0351) against live (0.0032, 0.2465,
0.0379), and its 72 kg spinner body at y 0.5829 against 0.5975 (+14.6 mm
along the motor axis). The rotator bar is a convex (`SIMULATION_GEOMETRY 3`)
proxy of three closed bars; its mesh centroid instead of its hull's would
give half of that. Not resolved; Berserker is not in the table test.

**Validation.** Before = `fidelity` 29371d9 (c014140 plus a tools change),
after = this branch.

Drive-at table (section 11d's runs, Sentinel rows as 12h): identical before
and after (no zone is read): Scout 11.01 / 1.42 / 14.42 / 7.85 / 10.77,
ALARM 13.92 / 1.23 / 14.12 / 8.90 / 11.28, FlapJack 10.40 / 1.48 / 11.80 /
7.19 / 10.11, Razor 17.13 / 1.53 / 22.60 / 7.28 / 12.49, Wide Load 13.45 /
1.67 / 17.95 / 10.71 / 13.17, Sentinel straight 14.83 / 1.48 / 18.79 /
10.07, circle 1.424 / 4.58 / 8.56. (Section 12h's "after" column came from an
earlier build of that branch.)

Fights as ensembles (six runs each, `FIDTRACE_WARMUP` 0/7/13/19/26/31; mean
counted hits (mean total), mean length, runs that last 120 s):

| fight | original | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 63, 77, 37 / 9, 6, 7 | 14.2 (765) / 4.5 (59), 82 s, 0/6 | 17.3 (994) / 5.8 (80), 73 s, 1/6 |
| box Razor v FlapJack: Razor / FlapJack | 3 (40) / 1 (12) | 2.7 (58) / 1.0 (13), 107 s, 5/6 | 3.8 (108) / 1.0 (13), 106 s, 4/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 60 (1778) / 2 (23) | 57.0 (1884) / 6.2 (84), 114 s, 5/6 | 31.0 (1091) / 5.0 (74), 88 s, 2/6 |
| Octagon Scout v ALARM: ALARM / Scout | 24 (944), Scout out at 67 s | 6.7 (403) / 2.0 (29), 61 s, 0/6 | 9.0 (454) / 2.5 (32), 53 s, 0/6 |

Six runs of a fight scatter about as much as these differences (the mesh-only
and mesh-plus-rings variants gave the Compressor 46.7 and 37.5); the
Compressor's shorter fights are the one move away from live and were not
traced. AI suite (ten pairings, warmups 0/7/13): before 29/30 deal damage,
after 29/30 (warmup 0: 10/10; Tornado v Ripblade at warmup 13 in both).

**Still open.** ALARM still ends on its back in half the box runs (live 1 in
7); FlexTop's dynamic table; the rotator bar's mass; whether the arena's
zones see traction rings; the heading drift (ours 0.26-0.30, live 0.19: not
worked on beyond confirming that `MagicAction` runs once per step piece per
robot, live 730 G rows for 365 pieces with two robots); the wheels'
transverse inertia (+3-5 %).

### 12j. FlexTop's table, the tail-stand statistics, the Compressor ensemble (fx/physics-flex)

New live captures (fresh sessions, all in `local/oracle/original` with
manifest rows, `manifest-additions-2026-10-21-physics-flex.tsv`):
`flextable-scout-alarm-flextop__live__000` (`frida/ra2_dashpot.js`: every
angular-dashpot apply with its raw fields, the table body and both chassis per
step), `fight-scout-alarm-flextop__live__000..002` with
`flexpos-scout-alarm-flextop__live__000..002` (three FlexTop fights, hits and
10 Hz positions), `upside-scout-alarm-box__live__000..005` (six more box
fight starts, 52 s each: location, `isUpsideDown`, tactic),
`zonelist-sentinel-wideload-compressor__live__000` and
`fight-sentinel-wideload-compressor__live__002..004` (Sentinel and Wide Load
copied into the prefix `AI/Team0` slots and restored, sha256 checked).
Diagnostics: `examples/tableprobe.rs` (the table alone from a live state),
`examples/poseprobe.rs` (a robot placed at a live chassis pose: how far each
component reaches below its centre), `fightprobe` takes
`FIGHTPROBE_GAME=tabletop` and prints a suspended prop's tilt,
`RA2_ZONE_PHANTOMS=0` keeps the other robot's phantoms out of a SmartZone
(the rule before §12i), `gmfdump` prints hinges, dashpots and
point-to-points.

**How FlexTop holds its table (recovered, verified live).** The `.gmf` has a
point-to-point constraint (node 53, `Point-Point01`: `anchor03`'s
`(0, 0, 0)` and `floor_collision`'s `(0, 0.1196, 0)`, both the world point
`(0, 0.2025, 0)`) and two angular dashpots (node 49, `Dashpot01/02`, both
`anchor03` → `floor_collision`, `*STRENGTH` 1000, `*DAMPING` 0.08, rest
rotation identity). `anchor03` is a massless body that collides with nothing;
the table is the 400 kg `floor_collision`, a 12-sided ring 0.7 m thick with
the hole in the middle (its mesh has no bottom faces) plus four rim boxes; the
only fixed collidable is `Plane01`. No other arena or component `.gmf` has a
dashpot or a point-to-point (docs/04 §2.5 says otherwise). The dashpot's
reader is `0x5ca940` over the base dashpot's `0x5ca1b0` (`+0x80` strength,
`+0x84` damping, `+0x88` interpenetrations byte, then four floats at `+0x8c`);
its action's apply is `0x52cc00` (vtable `0x6ac0cc`; the linear dashpot's is
`0x52c190`, which the "Actions/Dashpots" timer brackets). Per call:
`f = 200 · (t2 - t1)` (`DAT_006ac0f0`; live the two times differ by 1/60 per
step), the error rotation between `q_A · rest` and `q_B` as a rotation vector
(`0x4f80b0`: `2 acos |w|`, the short way; under 0.001 none), and the angular
impulse `J = f (strength · θn + damping · (ω_A - ω_B))`, `-J` to A and `+J`
to B through the world inverse inertia (`0x485ec0`: `ω += I⁻¹ J`, body
`+0x10c`, `+0x64`), each only for a movable body (`0x4812c0`). Live, the
free table (robots fallen off) gains 0.056 rad/s per step at a 0.135 rad tilt;
the formula with the live inertia predicts it to 3 %, the rest being gravity
on a centre 0.88 m above the pivot. Strength 1000 makes it a stiff spring
(period 1.25 s); the robots on it tilt it 5-10°.

Ours: `ra2fmt::gmi` keeps the dashpots and point-to-points,
`ra2_render::HavokArena` carries them, `PhysicsBackend::create_point_to_point`
and `create_angular_dashpot` build them (havok17: the point-to-point's three
rows already in `joints.rs`, the dashpot in `havok17/dashpot.rs`, applied per
step piece before the constraint solver; the order of the two actions is not
recovered), `duel::suspend_props` hangs the prop from the static arena body
standing in for `anchor03` (both have the world's frame), the table collides
as its own mesh (`Prop::mesh`, a ring a robot falls through), and
`scene::load_arena` accepts the plane as the arena collider when the start
points stand on a suspended body (before: no fixed floor near the starts, so
the visible geometry, 1436 triangles, was a static collider). The table's
mass properties are taken **as measured** (`scene::LIVE_MASS`): centre
`(0, 1.0000, 0)` in its own frame (0.8805 m above the pivot), inertia
(15550.73, 31099.06, 15550.73) kg·m², xz -6.48. Its hull would give centre
0.974 and (15025, 29869, 14950); the live tensor is flat (`Iyy = Ixx + Izz`
to 0.01 %), which neither the hull, the open mesh's solid from any origin,
the faces' area nor the vertices reproduce. The rule is not recovered.

From the live state at step 1502 of the capture, the free table (`tableprobe`,
test `tests/flextop_live.rs`):

| step | live rotation vector | ours | live ω | ours |
|---:|---|---|---|---|
| 30 | (0.041, -0.024, -0.025) | (0.042, -0.025, -0.020) | (-0.29, 0.07, -0.67) | (-0.31, 0.08, -0.70) |
| 60 | (-0.070, 0.022, -0.060) | (-0.071, 0.022, -0.068) | (0.12, 0.09, 0.59) | (0.15, 0.09, 0.63) |
| 120 | (-0.045, -0.028, -0.133) | (-0.045, -0.029, -0.137) | (-0.31, -0.02, -0.15) | (-0.29, -0.01, -0.13) |

Ours swings 3-6 % wider, in phase after 2 s.

**The freeze (fx/loose-ends).** Live the table does stop, but **not when the
match ends**. `frida/ra2_flexend.js` (`flexend-*__live__*`) hooks the match
end (`0x4170e0`, `0x416450` which calls the arena's `Activate(0)` through the
Python shim at `0x4c9000`, `0x4172d0`), the arena's `SetPinned`/`SetActive`
wrappers (`0x405050`, `0x405010`), and per step the table body's first 0x200
bytes at the dashpot call. In six fights and drives the match ended at steps
281-1222 and the table kept swinging for 20+ s afterwards, while in three
drives where *both* robots had left the table (`flexend-off__live__002/003`,
the old `flextable-...__000`) it stopped at step ~1470 / 1503 / 915; the
`SetPinned` wrapper was never called (`flextop.py` pins `floor_collision` only
when the hazards are off). What stops is **the table body itself**: its
flag word at `+0x1c` goes from `0x22` to `0x20` (bit 1, the active bit; the
movable test `0x4812c0` reads bits 0 and 2 and stays true), the integrator skips it,
the two dashpots and the step actions keep being called and change nothing
(they apply only to movable bodies), and the velocity words keep their last
values. That is Havok's deactivator (`flextop.gmf`'s simulation object has
`include_deactivator`, `short_freq` 5, `long_freq` 0.1). The criterion is
**not recovered**: it needs no robot on the table (never with one touching it,
even an idle one), but the delay after the last robot left was ~10.5, ~3 and
~14.5 s in the three drives, the table was still swinging at 0.2-0.6 m/s and
5-10 cm per 0.2 s, and no window of its motion separates the frozen from the
earlier steps (the criterion is recovered in section 12t: the table is a
deactivation system of its own). Ours *at the time*: `Duel::update_deactivation` (`RestWatch`) puts the body
that the file's dashpots hold to rest when no robot part has touched it for 10 s
(`DEACTIVATION_SECONDS`, 1/`long_freq`) and wakes it on a touch; it matters in
a FlexTop match only after both robots are off the table (the match is over
by then). Test `tests/flextop_live.rs::the_flextop_table_freezes_once_no_robot_has_touched_it_for_ten_seconds`;
`fightprobe` prints `FROZEN`. Open: the real criterion (hook the writer of the flag:
the tests of bit 1 are at `0x52c1a9`, `0x52eb44`, `0x530341`, `0x531282`,
`0x53ccd6`, `0x548bb0..0x549429`, `0x55a9c6`, `0x55aa48`; no immediate `or`/`and`
writes it, and Stalker-following the physics thread crashes the game).

FlexTop Scout v ALARM (Deathmatch in ours, Tabletop live; six warmups; the
fight ends when the first robot is off the table, y < -6):

| | live (3 runs) | before | after |
|---|---|---|---|
| first robot off | 16.3, 10.4, ≤ 5.7 s | 6, 24, 5, 24, 5, 17 s | 10, 18, 6, 9, 10, 8 s |
| counted hits Scout / ALARM (per run) | 0 / 1.3 | 0 / 0.2 | 1.5 / 1.8 |
| table | swings 5-10°, robots slide off the low side | static | swings 2-10° |

**ALARM on its back: the live rate is 2 in 13, not 1 in 7.** With six more
box fight starts the live set is 13 (`aistate-*__live__000..006`,
`upside-*__live__000..005`). Counting from 5.7 s to 45 s with ALARM's
`getLocation` and `isUpsideDown` (on its back: upside down with the centre at
the upright height, y < -1.65; standing on its tail or nose: y ≥ -1.5):

| | live (13) | ours (12, warmups 0..71) |
|---|---|---|
| on its back at 45 s | 2 | 9 |
| mean time on its back | 11 % | 49 % |
| vertical episodes / ending on its back | 35 / 2 | 25 / 13 (per step) |
| mean time vertical | 15 % | 5 % |

The earlier "back" readings of `aistate-...__live__004` were tail-stands
(`zonelist-...__live__000`: up.y ±0.2, centre 0.4 m high). On its back ALARM
rests as live (centre 0.51 m above the floor in both; `poseprobe` at the live
pose of `zonelist-...__live__002`). The difference is on the tail. Live
(`fightcontacts-...__live__001`, 11.9-13.2 s) ALARM rises at 3-5 rad/s, stops
at up.y -0.30 resting on its chassis' rear edge alone (the only contact of its
bodies), and 0.3 s later its own `InvertHandler` fires the hammer: the centre
hops 0.11 m and ALARM rotates forward through vertical back onto its wheels.
Ours, alone on its tail (`invertprobe` pitched), rests at up.y -0.1 on its
rear wheels and the same hammer stroke hops it 0.06 m; in the fights it
reaches vertical with 1.4-2.5 rad/s and goes on over backwards within
0.3-0.7 s (e.g. warmup 19, 14.55-15.8 s: fork stroke, vertical at 15.1 s with
ω 2 rad/s, the Scout backing away, on its back at 15.8 s). So what differs is
how the tail stops the rotation (what ALARM touches past vertical: live the
rear edge of the chassis at -0.30, ours the rear wheels at -0.1) and the
hammer's kick; neither is recovered. The Scout's decisions are not the cause:
its zone readings agree with the live list (§12i) and it was not pushing in
the traced falls. `isUpsideDown` is the chassis' up.y < 0 (`0x4df4f0`), as
ours.

**Compressor Sentinel v Wide Load: the zone rule, and a gap 3× larger than
it.** The drop from 57 to 31 is the phantom rule of §12i: with
`RA2_ZONE_PHANTOMS=0` the same build gives Wide Load 41.8 (1335) counted in
103 s (4/6 full), with it 24.8 (848) in 88 s (2/6; Sentinel wins twice, at 30
and 65 s). The rule holds on this pairing too: the live phantom list
(`zonelist-sentinel-wideload-compressor__live__000`) holds zone-v-traction-ring
pairs (tags 32/20, both phantoms, 96 entries) and zone-v-zone pairs, as the
box captures. So it stays. But three new live fights give Wide Load **118,
158 and 136** counted hits (4390, 4516, 3835) with both robots running at 120
s, Sentinel 13, 7, 7; the earlier reference (60, `__001`) is the low one. Live
mean of four: 118 counted. Ours is a third of that with or without the rule;
not traced further.

**Berserker's rotator bar (not recovered).** The chassis body is within the
test's 7 mm (6.9 mm) and is now a row of `tests/data/mass_live.tsv`. The
spinner body (rotator bar 14 kg, three angle connectors 4 kg, three axe heads
12 kg, the Z-Tek's rotor 10 kg) is 14.6 mm low along the motor axis: 1.05
kg·m. The rotator bar's mesh solid instead of its hull closes 0.54 of it
(y +0.016 against -0.023 in its frame); no single other piece's mesh, box or
vertex mean gives the rest, and the live assembly capture has no component
positions to tell a mass rule from a placement (its component nodes are
identity children of the root). The live collidable class says
`SIMULATION_GEOMETRY 3` is convex (§12g), so the mesh solid is not taken.
`mass_live::berserkers_spinner_body_matches_the_original` states the live
value and is ignored.

**Validation.** Before = `fidelity` 0ff4467, after = this branch. Nothing but
FlexTop changes: the dashpots and point-to-points exist only in
`flextop.gmf`, and the suspended-body paths only run for it; the traces of
box Scout v ALARM and Compressor Sentinel v Wide Load (warmup 7, 40 s) are
byte-identical before and after, so the drive-at table and the other fights
are unchanged. The ensembles of this build (six warmups, counted = `>= 10`
on the other robot; this counts a little differently from §12i's table, which
the numbers below should be read against only within the row):

| fight | live | this build (before = after) |
|---|---|---|
| box Scout v ALARM: ALARM / Scout | 63, 77, 37 / 9, 6, 7 | 11.2 (412) / 5.5 (78), 73 s, 1/6 |
| box Razor v FlapJack: Razor / FlapJack | 3 (40) / 1 (12) | 3.8 (108) / 0.0, 106 s, 4/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 60, 118, 158, 136 / 2, 13, 7, 7 | 24.8 (848) / 4.5 (74), 88 s, 2/6 |
| Octagon Scout v ALARM: ALARM / Scout | 24 (944), Scout out at 67 s | 6.0 (224) / 2.3 (32), 53 s, 0/6 |

AI suite (ten pairings, warmup 0): 10/10 deal damage. Traction survey 45/45;
workspace tests 644 passed, 0 failed, 1 ignored (the Berserker spinner).

**Still open.** ALARM's tail: what it rests on past vertical and the
hammer's kick (above); the Compressor's Wide Load at a third of the live
hits; the table's mass rule; the rotator bar; the heading drift and the
wheels' transverse inertia (not worked on).

### 12k. ALARM's tail and the piston's kick (fx/alarm-tail)

No new live captures: the existing `fightcontacts-scout-alarm-box__live__000..003`
(chassis and wheel bodies at every resolver entry, every contact) were enough.
The A/B runs below used a throwaway environment switch that was removed before
the commit.

**What ALARM rests on past vertical: the same as live.** In chassis
coordinates (centre of mass origin), live and ours rest past vertical on both
rear wheels (contacts at y -0.10..-0.25, z -0.79..-0.81) *and* the chassis'
top rear edge (±0.43..0.45, +0.38, -0.73) at up.y -0.10..-0.16
(`fightcontacts-...__live__002`, 16.3-19.7 s; ours `invertprobe` pitched 95°,
the same points to the centimetre). Alone on that edge live ALARM balances up
to up.y -0.48 (the edge lies 28° past vertical from the centre: live 21.4-22.1
s, ~1 s). Section 12j's "up.y -0.30 on the edge alone" in `__live__001` is
ALARM leaning on the Scout: from 11.92 to 12.12 s its underside rests on the
Scout's chassis and fork, which take its 3-5 rad/s down to 0.8. So the
geometry, the wheel shapes and the contact radii are not the cause.

**The piston pushes its rod only (recovered, verified against live data).**
`0x4b51b0` (the burst piston's one-substep push and the prismatic
controller's force, `0x4b5240`) scales the constraint's axis (`+0x58` → `+0x54`)
by the force, turns it into the world with the housing's frame (`+0x60`), and
applies it once, through `0x4b0950`, to one body: the rod's (`+0x5c`; it
wakes it and calls `0x4b09d0` → `0x45e940`). Nothing is applied to the
housing; the rest of the function only computes the rod's position in the
housing's frame into a local. So the robot does not recoil while the hammer
goes out and takes the hammer's momentum when the rod hits its stop. The live
chassis show exactly that (ALARM's speed along the stroke, step by step, at
the resolver's entry):

| | out stroke | back stroke |
|---|---|---|
| live, on its wheels (`__live__001/002`, about ten strokes) | 0 → +0.4..1.0 → +1.6..2.2 | Δ -2.0..-2.2 |
| live, on its tail (`__live__001`, 12.22 s) | vy -0.33 → +0.21 → +1.63 (hop 0.106 m) | |
| ours before, on its back | +0.1 → -0.4 → -0.8 → -0.05 → +0.9 | |
| ours before, on its tail (`invertprobe ... 95 4 fire`) | vy -0.13 → -0.83 → -0.91 → 0 → +1.16 (hop 0.06 m) | |
| ours now, on its back | 0 → +0.75 → +2.2 | Δ -2.3 |
| ours now, on its tail | vy -0.18 → +0.46 → +1.83 (hop 0.16 m) | |

With the reaction the hammer's push and its stop cancelled and only what the
floor took while the chassis recoiled was left; without it the robot gains
the rod's momentum, `F · t / m ≈ 21 m/s · 33 kg / 339 kg ≈ 2 m/s`, as live.
`Robot::push_slider`; test
`motors::a_burst_piston_kicks_the_robot_only_at_its_stop`. Every robot with a
piston changes: ALARM (`burstpiston2`) and Wide Load (`BurstPiston`).

**Box Scout v ALARM, ALARM's tail.** Twelve fights (`fightprobe`, warmups
0/7/13/19/26/31/39/45/52/58/64/71, 46 s, ALARM's up vector every 0.5 s from 5.7
to 45 s; on its back up.y < -0.8, vertical -0.8..0.5):

| | live (13, §12j) | before | after |
|---|---|---|---|
| on its back at 45 s | 2 | 6 | 3 |
| mean time on its back | 11 % | 42 % | 25 % |
| mean time vertical | 15 % | 6 % | 17 % |

(This counts a little differently from §12j's 9/12 and 49 %, which came from
the per-step trace; read the rows against each other.) ALARM now stands on its
tail and hops there, as live, and the Scout pushing under it is what tips it
either way. Two of the twelve stand on their tails for more than half the
window; live the longest stand is 8 s.

**Validation.** Before = `fidelity` 5bdd94a (§12j's table, the same build and
scripts), after = this branch. The drive-at table is unchanged (no piston
fires). Ensembles (six warmups 0..31, 120 s, counted = `>= 10` on the other
robot; mean counted (mean total), mean length, full-length runs):

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 63, 77, 37 / 9, 6, 7 | 11.2 (412) / 5.5 (78), 73 s, 1/6 | 21.7 (923) / 4.7 (60), 85 s, 2/6 |
| box Razor v FlapJack: Razor / FlapJack | 3 (40) / 1 (12) | 3.8 (108) / 0.0, 106 s, 4/6 | 3.8 (108) / 1.0 (13), 106 s, 4/6 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 60, 118, 158, 136 / 2, 13, 7, 7 | 24.8 (848) / 4.5 (74), 88 s, 2/6 | 60.8 (2242) / 8.7 (129), 100 s, 4/6 |
| Octagon Scout v ALARM: ALARM / Scout | 24 (944), Scout out at 67 s | 6.0 (224) / 2.3 (32), 53 s, 0/6 | 19.8 (925) / 5.0 (68), 87 s, 3/6 |

(Neither Razor nor FlapJack has a piston; FlapJack's 1.0 (13) is what §12i
read for the same build line.) AI suite (ten pairings, warmup 0): 10/10 deal damage.
Traction survey 45/45.

**Still open.** ALARM still ends on its back in 3 of 12 box runs (live 2 of 13)
and some tail-stands last longer than live; Wide Load's hits are now half the
live mean (60.8 against 118); the other items of §12j.

### 12l. The event filter's registrations, the burst motor's wrap, the Compressor's endings (fx/wideload)

New live captures (one fresh session, Sentinel and Wide Load copied into the
prefix `AI/Team0` Bot0/Bot1 slots and restored, sha256 checked; manifest rows
in `manifest-additions-2026-10-21-wideload.tsv`):
`fight-sentinel-wideload-compressor__live__005..007` with
`fightpos-sentinel-wideload-compressor__live__000..002` (three Compressor
fights: every priced hit, and every 6 frames both robots' location,
`isUpsideDown`, last tactic, commanded throttle and turn and sensor table;
`tools/oracle/capture_fightpos_set.sh`) and
`eventreg-sentinel-wideload-compressor__live__000` (`frida/ra2_eventreg.js`,
loaded at the main menu: every collision-event registration of the three
match loads). Diagnostics: `fightprobe`'s detail lines give each jointed
part's rotation and angular velocity relative to its parent.

**The event filter registers collidables, one per component (recovered,
verified live).** The robot set-up `FUN_00414a60` loops over the robot's
components and, inside, over each component's collidables, registering every
one (`FUN_00414cd0` → `FUN_0045ab90` → `FUN_004ad500(c, 2, 1.0)` →
`FUN_0052d850`, which appends an entry to that collidable's list and never
merges). Live, every match load makes 48 registrations: 9 arena bodies
(`FUN_00415140`), one per robot component collidable (31 for Sentinel's 13
and Wide Load's 23 components) and a second on each of the eight wheels
(`0x507ed6`), all with partner 0, interval 1 s, threshold 0, type 2. Sections
10-11's per-*body* reading was wrong: Wide Load's spike shaft is one body but
nine components, and live each of its four spikes is priced about 100 times
in a 120 s fight (`fight-...__live__003`: 107, 100, 100, 95), up to three
times within 0.12 s on one victim component and again a second later. The
gate's speed test (`FUN_0052d740`) compares `|Δv · n|` with the entry's
threshold, 0, so every point passes it; ours had required a closing point.
`combat::EventGate` now keys entries by (body, the component the point
touches), two for a wheel, `a`'s list first; test
`duel::tests::robot_contacts_reach_the_pricing_once_a_second_per_component`.
Replaying the live box contact stream (`fightcontacts-scout-alarm-box__live__000`,
robot-robot points only) through the models: per body 129 events, per
collidable 158, per collidable with the wheels doubled 181, against 153 live
events (306 H rows); the box capture holds the collidables of each point, and
this agrees with the registrations only to 18 %; not resolved further.

**A burst motor's target wraps past the stop (bug, fixed).** Sentinel fires
its wedge at 50 rad/s and 3600 N·m against 1.67 rad of travel; the wedge
overshoots the lock tolerance (1.77 rad), so the motor stays on and its target
runs on at 50 rad/s. Once the target was more than π ahead, the servo's error
was taken the short way round, and the same turn was subtracted from the
*angle* the limit is checked against: the hinge looked a turn below its lower
stop, the limit's correction spun the wedge at 160 rad/s, it went round
through 360° and launched Sentinel off the floor (FlapJack's box fights
change with the fix too, not traced; Scout's fork never leads by π). The limit now uses the measured angle
(`havok17/joints.rs`, test `a_hinge_driven_past_its_stop_stays_at_it`); the
stroke stops at 1.70 and cocks back. In the Compressor this was most of ours
flipping: before, a robot went over in 12 of 12 runs (Sentinel 8-21 s in),
after in 6 of 12.

**The live Compressor fights end early too.** In the three new fights a robot
goes onto its back 11, 14 and 21 s in (Sentinel twice, Wide Load once; from a
ledge or the other's flipper) and both AIs fall idle for good at 32, 58 and
55 s game time (the last tactic `None`, commands zero), while the positions
stand still and the priced hits stop with the robots still touching: the
match is over, the upside-down robot counted out. Wide Load lands 8, 32 and 23
counted hits. With `fight-...__001` (hits stop at 40 s, both robots still from
45 s) and `__002` (hits stop at about 90 s), four of seven live fights end
early. Section 12j's "both robots still run at 120 s" held for `__003` and
`__004` only. Live, seven fights: Wide Load 60, 118, 158, 136, 8, 32, 23
counted (mean 76) with 116-590 priced hits (mean 283); Sentinel 2, 13, 7, 7,
1, 3, 4 (mean 5.3).

**Which components are registered.** The registration ids of the capture
are handed out in component order (Sentinel's 13, then Wide Load's 23), and
the doubled entries land exactly on the eight wheels and Sentinel's Powered
Steering Unit on three collidables (the unit and its two hubs). The six ids
without a registration are both robots' batteries and control boards, Wide
Load's air tank and both SmartZones: those components have no collidable. The
gate therefore takes the nearest component *with* a registration
(`combat::registered_component`; no effect in the fights below, a contact
point is practically never nearest an internal part). ALARM and the Scout
were not captured; their Hammer Head and Forklift Arm are weapons like Wide
Load's spikes, which are registered.

**Validation.** Before = `fidelity` ce1aa67 (with alarm-tail's piston rule),
after = this branch; ensembles are `FIDTRACE_WARMUP` runs (12 runs: 0, 7, 13,
19, 26, 31, 37, 43, 50, 57, 64, 71; 24 runs add 2, 5, 9, 11, 16, 22, 28, 34,
40, 47, 53, 60), counted = `>= 10` on the other robot, total = every priced
hit on it (live counts zero-amount hits too, ours drops them). The two
changes were also run one at a time (temporary switches, since removed):

| fight | live | before | hinge fix only | gate only | after |
|---|---|---|---|---|---|
| Compressor Sentinel v Wide Load, 12 runs: Wide Load / Sentinel | 76 (283) / 5.3, 4 of 7 end early | 55.8 (114) / 7.3, 97 s, 7/12 full | | | 93.9 (336) / 5.2, 110 s, 10/12 full |
| box Scout v ALARM, 24 runs: ALARM / Scout | 63, 77, 37 / 9, 6, 7 | 19.2 (126) / 4.6, 88 s, 6/24 | 20.2 / 4.7, 87 s | 21.2 / 4.8, 89 s | 22.2 (140) / 5.1, 83 s, 4/24 |
| Octagon Scout v ALARM, 24 runs: ALARM / Scout | 24 (944), Scout out at 67 s | 12.2 (59) / 2.7, 74 s, 7/24 | 8.8 / 2.3, 67 s | 8.9 / 1.7, 67 s | 10.5 (55) / 2.7, 72 s, 4/24 |
| box Razor v FlapJack, six runs: Razor / FlapJack | 3 (40) / 1 (12) | 3.8 (109) / 1.0 (106), 106 s, 4/6 | | | 7.5 (165) / 1.3 (158), 105 s, 4/6 |

Across the Compressor's 12 runs a robot goes over in all 12 before and in 6
after. The Octagon's two halves of 12 scatter more than the variants differ
(ALARM 13.8 / 10.7 before, 9.4 / 11.6 after, 5.9 / 11.8 with the gate only).
A first version of this table showed the box ALARM falling by a third: its
"before" binary had been built with an experimental per-body gate left in the
working tree, not ce1aa67; with the switches reproducing ce1aa67 bit for bit
there is no drop.

**Still open.** The live Compressor's flips and early ends (four of seven live
fights; ours now 2 of 12 runs end early): how the robots go over (ledge falls,
the wedge's lift) was not traced; ours rarely flips without the hinge bug.
The box replay's 18 % surplus of events under the per-collidable rule (a live
event-registration capture of Scout v ALARM would settle which of their
components are registered). The arena's hazard bodies are registered too
(nine in the Compressor) and are not in ours.

### 12m. ALARM's hits, the rebuilt chassis, the piston rod's inertia, the event filter (fx/alarm-hits)

New live captures (fresh sessions on `:7`, prefix restored and sha256
checked; manifest rows in `manifest-additions-2026-10-21-alarm-hits.tsv`):
`fight-scout-alarm-box__live__003..005` with
`fightpos-scout-alarm-box__live__000..002` (three more 120 s box fights,
`capture_fightpos_set.sh`), `fightweapons-scout-alarm-box__live__000`
(`frida/ra2_weapons.js` over those three fights: the Scout's fork motor and
ALARM's piston every substep), `eventreg-scout-alarm-box__live__000..001`
(`frida/ra2_eventreg.js`, the registrations of six match loads) and
`fightcontacts-scout-alarm-box__live__004..006` (`frida/ra2_fightcontacts.js`,
new: `ra2_contacts.js` and the priced hits in one script; 40 s each, the
capture starts some seconds into the fight). Diagnostics: `fightprobe`'s
detail lines carry the chassis' x and z and print every component that comes
off.

**The live box fights scatter, and end early too.** The three new fights give
ALARM 10, 40 and 4 counted hits on the Scout (the Scout 0, 6, 1). In two of
them ALARM goes onto its back (from 37.7 s and 19.6 s; `fightpos`, y -1.81,
`isUpsideDown`) and the brains fall idle at 78 and 82 s; in the third the Scout
is on its side from 79 s. With `__000..002`: ALARM 63, 77, 37, 10, 40, 4
(mean 38.5), the Scout 9, 6, 7, 0, 6, 1 (mean 4.8); robot hits stop at 121,
120, 36, 63, 120 and 77 s, so three of six run the full length (§12l's live
column held the three high ones only). Ours before: 22.2 counted in 83 s, 5/24
full.

**What is short: the time ALARM spends on its wheels, not its blows.** The
priced hits of ALARM on the Scout per fight, by amount (zero-amount hits are
in live's first bin only):

| amount | 1-2 | 2-5 | 5-10 | 10-20 | 20-40 | 40-80 | ≥ 80 |
|---|---:|---:|---:|---:|---:|---:|---:|
| live (6 fights) | 25.5 | 13.7 | 5.2 | 10.3 | 12.5 | 12.2 | 3.5 |
| ours before (24) | 29.0 | 14.6 | 6.4 | 6.8 | 5.9 | 5.1 | 4.4 |

The blows below 10 are as many as live; the counted ones are fewer at every
size. Per second that ALARM stands upright (up.y > 0.5), ours lands 0.58
counted hits; live 0.64 in `__001` (upright for 120 s) and 0.33 in the new
`__004`. The hammer's rate agrees too: live ALARM's piston fires 10-20 times
per 20 s of fighting (every 0.5-1 s, `fightweapons`, O rows), ours 14-19; the
rod goes out in 6-7 substeps unloaded and 13-17 against the Scout, and home in
5 (F rows: the push is ±10000 per substep, `|direction| · F`, as ours). At
the first meeting the head leaves at 10-11 m/s relative to the chassis live
(`fightcontacts-...__live__003`, 4.03-4.12 s) and at 4-9 m/s in ours (warmup
19, before), from a rod pushed in 0.29 m live and 0.13 m in ours; one case
each, not followed further. The
difference is that ALARM is on its back far more: on its back (up.y < -0.8)
for at least 5 s by 45 s in 14 of our 24 runs, against about 5 in 19 live
starts (§12j's 13 plus the new six); ours mostly at its first meeting with
the Scout (10 of 24 within 6 s).

**At the first meeting ALARM is lifted twice as hard.** At the first contact
both games meet the same way (head on, the Scout 1.9-2.2 m in front of ALARM,
closing at 13-17 m/s, `fightcontacts` B rows in ALARM's frame; the fork goes
under ALARM's chassis at (-0.25, -0.30, 0.62) beside the hammer head and the
Scout's chassis meets the hammer head, to the centimetre in both), and the fork
runs 0 → 1.13 rad in 0.15 s in both. But within 0.7 s ALARM's chassis rises at
1.0 m/s live (1.07, 1.10, 0.28, 1.46; nose up at most 0.5, never below up.y
0.87) and at 1.9 m/s in ours (24 runs; nose up 0.38 on average, below up.y 0.5
in 10). Not traced to a cause; what was found on the way is below.

**A rebuilt body keeps its Havok mass, and the chassis collider does not
crush (bug, fixed).** When the chassis buckles (or a part comes off)
`Robot::rebuild_part` re-made the body by the Jolt rule:
the members' masses summed (a motor's rotor counted in the chassis as well as
in the body it drives), the convex hull of the members and its inertia. At its
first blow the Scout's chassis went from 163.49 to 193.49 kg, its centre of
mass 3 cm forward and 5 cm down. Live it stays 163.49 kg through a fight and
its wheels keep their place about its centre of mass to 5 mm
(`fightcontacts-scout-alarm-box__live__000`, 2.6 s against 60 s). On the
Havok backend the rebuild now uses the body the robot was built with
(`havok_part`, the members still attached) and sets its inertia
(`PhysicsBackend::set_body_inertia`); test
`mass_live::a_buckled_chassis_keeps_its_mass_and_centre`. The chassis'
Havok collidable is the `.bot` mesh, which the crush (F9, our own rule) does
not move, so on this backend the collider stays as built and only the drawn
chassis caves in. That is what the original does: in
`fightcontacts-scout-alarm-box__live__006` the Scout's chassis has lost 971 of
its 1828 hitpoints by 20 s and 1406 by 30 s, and contacts still land on its
top at y 0.359 above its centre of mass, the pristine height (`__000`, `__005`
the same), where our crush would have brought it 0.13 m down. Letting the
collider follow (tried: the crushed mesh as the shape with the built mass
properties) gave box ALARM 16.0 counted in 61 s, 2/24 full, ALARM on its back
by 45 s in 18/24. `duel::tests::a_crushed_chassis_collider_follows_and_the_robot_still_drives`
now asks for the collider to stay on havok17 and to follow on Jolt.

**A piston rod has its housing's inertia (measured live, code not located).**
The rod body's local inertia (`+0x19c`) is its chassis' with the diagonal
scaled by 0.2 and the products of inertia unchanged: ALARM's 33 kg hammer rod
(9.830, 14.541, 7.282 / -0.1215, 0.0264, -0.8657) against its chassis (49.149,
72.705, 36.411, the same products) in all three `fightcontacts-...__live__001..003`,
and Wide Load's 63 kg rod against its 226.67 kg chassis likewise
(`contacts-nohaz-wideload__live__001`); the world inverse inertia (`+0x10c`)
agrees, so the solver uses it. Ours gave the rod its own geometry's (0.31,
3.81, 3.78). `Robot::spawn` now builds every slider's body so
(`ROD_INERTIA`); test `mass_live::alarms_hammer_rod_takes_its_chassis_inertia`.
The 0.2 constant is not found in the code (none of the eight `0.2f` loads near
the robot and prismatic code is it).

The first meeting with both: ALARM rises at 1.76 m/s (median 1.42; live 1.0),
nose up 0.28, below up.y 0.5 in 6 of 24. But later in the fight ours goes onto
its back more, not less (below).

**Which components the event filter registers (verified live for Scout v
ALARM).** Every box match load makes 29 registrations: four arena bodies, the
Scout's chassis, burst motor, both angle motors, both wheels, Round Extender
and Forklift Arm (ids 1-6, 9, 10), ALARM's chassis, four angle motors, four
wheels, Burst Piston and Hammer Head (ids 12-20, 24, 25), and a second entry
on each of the six wheels; none for the batteries, the CO2 tanks, the control
boards and the SmartZones. That is §12l's rule (`combat::registered_component`).

**How the filter decides (recovered) and what calls it.** The filter is
`0x52d4f0` (vtable `0x6ac110` slot 2, behind Havok's filter chain `0x554c70`,
which calls it and passes the event on when it returns nonzero). For an event
between collidables A and B it walks A's list (`0x52e0c0`): an entry whose
partner is 0 or B, whose interval has run out since its last firing and whose
speed test passes (`0x52d740`, `|Δv · n| >= threshold`) takes it: the time is
stored, the event's type set and 0 returned. If none of A's does, B's list is
walked the same way, and an entry there also swaps A and B and flips the
normal. At most one entry per event, A's first: ours. What differs is how
often it is asked. Replaying the live contact stream of
`fightcontacts-...__live__004..006` through the live registrations: one event
per resolver point per step (ours) gives 345 events against 268 live (+29 %),
one per collidable pair when it starts touching 284 (-6..+36 % per run). Live
the fork lies against ALARM's chassis for a second at a time with no event at
all (`__004`, 0.1-1.1 s), so the events are not the resolver's points. Which
Havok callback calls the chain (a new contact point, a new pair) was not
captured: a hook on `0x52d4f0` was prepared but the live session was held by
another agent. Ours still asks once per resolver point and step.

**Validation.** Before = `fidelity` 59cfdbc, built from a clean tree (its
ensembles reproduce §12l to the digit); after = this branch. `FIDTRACE_WARMUP`
ensembles (12 runs: 0, 7, 13, 19, 26, 31, 37, 43, 50, 57, 64, 71; 24 add 2, 5,
9, 11, 16, 22, 28, 34, 40, 47, 53, 60), counted = `>= 10` on the other robot
(mean, total in brackets), mean length, full-length runs:

| fight | live | before | rebuild only | after |
|---|---|---|---|---|
| box Scout v ALARM, 24: ALARM / Scout | 63, 77, 37, 10, 40, 4 / 9, 6, 7, 0, 6, 1; 3/6 full | 22.2 (140) / 5.1, 83 s, 5/24 | 21.2 (116) / 4.2, 75 s, 7/24 | 17.4 (137) / 3.2, 77 s, 7/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 14/24 | 14/24 | 19/24 |
| Compressor Sentinel v Wide Load, 12: Wide Load / Sentinel | 76 (283) / 5.3 | 93.9 (336) / 5.2, 110 s, 10/12 | 82.3 (369) / 4.2, 109 s, 9/12 | 82.3 (349) / 4.8, 106 s, 9/12 |
| Octagon Scout v ALARM, 24: ALARM / Scout | 24 (944) / 4 | 10.5 (55) / 2.7, 72 s, 4/24 | 10.6 (72) / 2.0, 65 s, 5/24 | 11.0 (72) / 2.3, 73 s, 6/24 |
| box Razor v FlapJack, 24: Razor / FlapJack | 3 (40) / 1 (12) | 8.4 (156) / 1.7, 100 s, 17/24 | | 12.7 (202) / 1.4, 110 s, 21/24 |

(Razor v FlapJack over §12l's six warmups: 7.5 / 1.3 before, 12.0 / 3.8
after.) AI suite (ten pairings, warmup 0): 10/10 deal damage before and after.

So the two body fixes do not bring ALARM's hits up: the box ALARM is a little
lower (within one standard deviation of the 24-run mean, 18-22), the
Compressor's Wide Load moves towards live, Razor away from it. They are kept
because both are what the original's bodies measure.

**Still open.** Why ALARM rises twice as fast at the first meeting and goes
onto its back three times as often (the rod's inertia lowers the first and
raises the second); the 0.2 of the rod's inertia in the code; what Havok calls the event filter for (a hook on `0x52d4f0` with the contacts settles it); the
live piston's out stroke ends after 6-7 or 15 substeps, ours after a fixed
0.25 s; the items of §12l.

### 12n. The first meeting: what ALARM's lift comes from, the zones' events, the brains' clocks, the rod's inertia factor (fx/first-meet)

New live captures (fresh sessions on `:7`, prefix sha256 checked before and
after; manifest rows in `manifest-additions-2026-10-21-first-meet.tsv`):
`zoneevents-scout-alarm-box__live__000..005` (`frida/ra2_zoneevents.js`
through `capture_fight.py`, about 12 s of box fight each: every
`SmartZoneEvent` the C++ side hands a brain with both chassis, every call of
the event filter `0x52d4f0` with its callers) and `__006..007` (20 s each, the
same plus every phantom-pair event the zone listener receives). Diagnostic:
`examples/meetprobe.rs` puts both robots at the bodies' states of a live
`fightcontacts` snapshot (each body moved rigidly onto the live chassis, the
live wheels' and fork's velocities) and steps them with the live drive
commands (the MagicAction thrusts of the G rows turned back into throttle and
steer) and each weapon fired at a given time; `RA2_H17_DUMP` writes the
capture's rows, so the live scripts read both.

**From the same state the first meeting lifts ALARM as live.** Replayed from
`fightcontacts-scout-alarm-box__live__003` at 3.90 s (0.03 s before the first
contact, the hammer fired 0.0-0.03 s and the fork 0.05 s later, as live):
ALARM's chassis rises at 1.39-1.63 m/s within 0.7 s (live 1.46), nose up 0.53
(0.50), up.y down to 0.83-0.85 (0.87); the Scout's fork shoves the rod 0.24 m
in past its home (live 0.29) and the limit sends it back out at 13 m/s (live
17.4 = 0.29 · 60, the joint's stop at the neutral point, §12b). From
`__000` at 5.05 s the bodies follow live for 0.2 s step by step (hammer speed
13.2 against 13.8 m/s, ALARM 0.84 against 0.69); then the fork's stroke
catches the extended hammer head from below where live it meets the Scout's
chassis, and ALARM rises at 1.47 m/s (live 1.07). Fired a few substeps
earlier or later the same states give 0.3-2.9 m/s: the lift is set by *when*
each weapon fires relative to the contact (a fork that rises 0.03-0.07 s
before it meets ALARM lifts it at 2.5-2.9 m/s), not by the contact solver.
The resolver, its restitution (`eA eB`) and friction (`sqrt(µA µB)`, chassis
0.5/0.2, components 0.3/0.3 on both sides, the live contact records carry them)
and the push-out (`0x5598f0`: `10 tol f²` per body after each impulse,
recovered again) are as §2 has them.

**What the ensembles did differently: both weapons fired before the robots
met.** In 15 of our 24 first meetings both brains got a `SmartZoneEvent` with
the robots' centres 3.0-3.2 m apart, where the two SmartZones overlap and
nothing else does, and in six of them both weapons fired there (2.3-2.8 m
apart, before any contact; those six include the four biggest lifts, 2.6-3.4
m/s). Live (`aistate-*`, `fightpos-*`, `zoneevents-*`) ALARM's zone first
sees one of the Scout's parts (its fork, not the chassis) 2.6-3.0 m apart and
the Scout's sees ALARM at 1.9-2.4 m, never both at once, and in the four
captured first meetings both weapons fire 0-0.11 s after the contact.

**A SmartZone's events skip phantoms (recovered, verified live).** The zone's
events are not its readings. `GetZoneContents` lists every pair of the
detector's phantom list (§12i), the other robot's SmartZone and traction rings
included. The events come from that list's *changes*: `0x5499..0x549b9f`
compares the list with the last detection's (`det + 0x100` against `+ 0x110`),
and sends each new pair as an entering and each vanished one as a leaving event
(type `DAT_006abf6c`) down the world's listener chain. The zone listener
(`0x4a4780`, the registry `RegisterSmartZone`, `0x401340`, fills per zone
collidable through `0x4a45c0`) calls the callbacks registered on the pair's
first collidable with the second as the other, then those on the second with
the first, and each only when the *other* collidable is not a phantom
(`0x4a49b0` returns collidable `+0x58`). So the other robot's zone and its
wheels' traction rings never raise a `SmartZoneEvent`. Live
(`zoneevents-...__live__006`, 20 s): 842 pair events, 94 of them between two
phantoms, and not one of those is followed by a callback; 406 callbacks, all
for a pair with a solid other side. `duel::watch_brain_zones` keeps the
phantoms in the published contents and no longer turns them into events
(test `duel::tests::another_robots_zone_is_listed_but_raises_no_smart_zone_event`).
At the first meeting ours now fires both weapons 0-0.15 s after the contact in
all 24 runs, as live.

**Each brain keeps its own clock, a quarter interval apart (recovered).** The
AI's update (`FUN_004011f0`) adds the frame's time to `+0x44` and, while it
exceeds `tickInterval` (`+0x3c`, 0.125), calls `Tick` and takes the interval
off, keeping the remainder; `Activate` (`FUN_00401270`) starts the clock at
`tickInterval · 0.25 · +0x1c`. Live the brains' commands change every 0.11-0.14
s (four frames, sometimes three) and ALARM's one frame before the Scout's
(`fightcontacts-scout-alarm-box__live__000..003`, G rows). Ours ticked every
brain together every 8 substeps (0.133 s, the remainder dropped); now each
robot's clock starts at a quarter interval times its index and keeps the
remainder (`duel::advance_brain_clocks`, test
`duel::tests::brains_tick_every_eighth_of_a_second_a_quarter_apart`). That the
`+0x1c` id is the robot's index is inferred from the live order; whether the
clock runs per frame or per substep was not traced (ours: per substep).

**The piston rod's inertia factor is the `prismatic` line's last number
(recovered).** The piston component's set-up (`0x4b25f0`) reads the line's
eight tokens in turn (`body, shaft, ax, ay, az, neutral, lower, upper`) and
hands the last to its prismatic controller (`0x4b29e7` → `0x4b2b10`,
controller `+0x84`); the controller's initialisation (`0x4b4d4f..0x4b4de7`)
then, when it is not zero, gives the rod (`+0x5c`) the housing's (`+0x60`)
inertia scaled by it (`FUN_00404990`, `setInertiaTensor` `0x520200`). Every
burst piston style but the 60 cm one says `0.2` (§12m's measured factor); the
60 cm `burstpiston1` says `.8`, the linear actuators `0.3`. The travel stays
`[lower, neutral]`: the joint's stop is at the neutral point (the live
correction above). `rig::PistonStats::rod_inertia`, `JointInfo::rod_inertia`
replace the constant.

**Who calls the event filter (recovered).** `0x549c30`, the events stage at the
head of `World::stepTo` (`0x52aba8`), walks the detector's contact list
(`det + 0x144..0x148`, the very list `0x52e7b0` hands the resolver) and sends
every point as a type-2 event to the listener chain `0x554c70`, whose first
link is the filter `0x52d4f0` (it passes the event on when the filter returns
0). Live, every one of the filter's first 400 calls in each capture came from
there. So the events are the resolver's points, once per step piece, as ours;
§12m's surplus of 77 events (345 against 268) is not explained by the caller.

**Validation.** Before = `fidelity` 6262ad8 built from a clean tree (it
reproduces §12m's after column to the digit); "zones" = this branch's zone and
rod rules without the brains' clocks; after = this branch. First meeting, 24
runs (`fightprobe` dumps, ALARM's chassis within 0.7 s of the first contact):

| | live (4) | before | zones | after |
|---|---|---|---|---|
| ALARM's rise, mean (median) | 1.07, 1.10, 0.28, 1.46 | 1.76 (1.42) m/s | 1.38 (1.29) | 1.61 (1.59) |
| nose below up.y 0.5 | 0 | 6 | 1 | 3 |
| weapons fired before the contact | 0 | 6 | 0 | 0 |

`FIDTRACE_WARMUP` ensembles as §12m (counted = `>= 10` on the other robot, mean
(total), mean length, full-length runs):

| fight | live | before | zones | after |
|---|---|---|---|---|
| box Scout v ALARM, 24: ALARM / Scout | 38.5 / 4.8; 3/6 full | 17.4 (137) / 3.2, 77 s, 7/24 | 19.6 (135) / 5.5, 71 s, 4/24 | 16.0 (132) / 3.7, 71 s, 3/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 19/24 | 14/24 | 15/24 |
| Compressor Sentinel v Wide Load, 12: Wide Load / Sentinel | 76 (283) / 5.3 | 82.3 (349) / 4.8, 106 s, 9/12 | 76.0 (366) / 3.2, 108 s, 8/12 | 53.3 (316) / 2.6, 91 s, 7/12 |
| Octagon Scout v ALARM, 24: ALARM / Scout | 24 (944) / 4 | 11.0 (72) / 2.3, 73 s, 6/24 | 8.5 (70) / 2.0, 72 s, 4/24 | 8.3 (68) / 1.7, 70 s, 6/24 |
| box Razor v FlapJack, 24: Razor / FlapJack | 3 (40) / 1 (12) | 12.7 (202) / 1.4, 110 s, 21/24 | 12.5 (198) / 1.7, 107 s, 20/24 | 14.7 (230) / 0.6, 120 s, 23/24 |

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after. The
box ALARM's falls move later (by 9 s in 8 of 24 runs after, 10 before) but it
still ends on its back three times as often as live. The 24-run means scatter
by about 3 counted hits (box, Octagon) and the Compressor's 12 by about 10; the
Compressor's drop with the brains' clocks is twice that and not traced.

**Still open.** ALARM still goes over three times as often as live, the falls
now mostly after the first meeting; the Razor's surplus; the live piston's
firing phase lasts 15 substeps (13 or 17 at times, from the one-shot push to
the first return push, `fightweapons-scout-alarm-box__live__000`), ours 16:
both the piston (`0x448650`) and the burst motor (`0x446670`) set their timer in
the fire call and take the first `dt` off in the same call, expiring below
zero, which with 1/60 steps gives 16 and 36 (the motor's 36 is live); whether
the components update per frame (1/30, which would explain the odd counts) was
not traced. The event filter's surplus (§12m). The `+0x1c` id and the clock's
time base (above).

### 12o. The arena's hits on the robots, and the wheels' rubber (fx/wall-damage)

New live captures (one fresh session on `:7`, prefix sha256 checked before
and after; manifest rows in `manifest-additions-2026-10-21-wall-damage.tsv`):
`arenahits-scout-alarm-box__live__000..001` and
`arenahits-scout-alarm-octagon__live__000..001` (`frida/ra2_arenahits.js`
through `capture_fight.py`, four 120 s Scout v ALARM fights: every
`computeDamage` call with the attacking component, its piercing, concussion
and material, the whole hit record and both sides' rigid bodies; every
`0x417330` gate call with its point and normal). Diagnostic: `fidtrace`'s hit
rows print `att -1`, `by "arena"` for an arena hit.

**Who prices a contact with the arena (recovered, verified live).** The robot
listener (`0x414d80`, registered per collidable by `FUN_00414cd0`) looks up
the damageable object of both collidables. When the *other* one is not a
robot component (type test `0x414e7d` against `0x415000`) it only lets this
side price it and applies nothing (`0x414fb2`). When this side is not a robot
component but the other is (`0x414ef3` → `0x414f62`), the struck robot
component is the victim and `0x417330` is called with the player `-1` and no
attacking component: a hit with no attacker. The arena manager registers the
arena under one such object (`FUN_00415140`, called from the match set-up at
`0x41597a` with the arena, `+0x48`): a `0xc`-byte wrapper (vtable `0x6a581c`)
holding the arena's component, whose collidables it loops over. Live that
component is the arena's `giPython::Arena_callback` (vtable `0x6aa52c`) with
an **Arenium** material (vtable `0x6a5534`, float 0.5), piercing 0 and
concussion 1, in the box and the Octagon (4492 priced calls). `AddXtra` props
get a component of their own with the script's material (`0x403aa0`, from the
Python binding `0x4b9e80`; the KOTH prize `0x46e610` is a metal one); not
exercised by the stock arenas.

**Which bodies.** All colliding bodies of the arena `.gmf`, fixed and movable:
the box's four registrations (§12l) are `arena_collision`, `spikes1`,
`spikes2` and `floorsaw`, the Compressor's nine its `Collision Box`, four
spike zones and four smash arms. The floor is one of them (the box's floor is
`arena_collision`): live most of the priced arena contacts have normals
(0, ±1, 0), and floor hits of 11-17 hp land on the Scout's chassis when it
comes down. The Octagon's start blocks (200 kg) and hazards price with their
own mass.

**The formula (recovered, verified live to 1e-7).** It is the robot one,
`(piercing · rec0 · 10 + concussion · M) · scale`, with the arena's
component striking: `damage = 0.5 · M`. In `M` (`FUN_0045a5a0`) the fixed
body's mass (`+0x190`) is 0, which counts as an inverse mass of `0.08`, and its
world inverse inertia (`+0x10c`) is zero, so `M = 0.5 · rec0 / (1/m_robot
(at least 0.01) + 0.08 + (r × n)·I⁻¹(r × n))`: a fixed arena body weighs
12.5 kg to the formula, not infinitely much. Recomputing every priced arena
hit of the four captures from its record and the robot body's live mass,
centre and inertia gives the gate's amount to a relative 1.2e-7 (726 non-zero
hits). The hit then goes the robot way: the `>= 10` gate, the victim
component's own damage (`FUN_00489620`), the crowd's excitement on the raw
amount, no score and no "last damage done" (`FUN_004175b0` only fills that
for a player `>= 0`). Arenium does nothing to Rubber, and every wheel is
Rubber (below), so wheels take nothing from the floor.

**The event filter.** Each arena collidable has one 1 s, threshold-0 entry
(§12l), tried like a robot component's: a robot-arena point gets through on
whichever side's list has a due entry. Ours merges the fixed bodies into one
static collider; the gate tells them apart by the contact point (nearest
triangle of each fixed body), and each movable arena body is one entry.

**A wheel is Rubber (recovered, verified live).** The `Wheel` constructor
(`0x507a65`, vtable `0x6ab8cc`) replaces the component's Metal with a new
`Rubber` (`0x507aea` → `FUN_004899e0`, float 0); live all 4012 calls of a
`Wheel` (ALARM's mini wheels and the Scout's) price with Rubber 0. Ours read
the material from the definition only and gave wheels Metal 0.1: they priced
the other robot at 0.1 (82 hits per box fight, 166 in the Compressor, none
reaching 10) and took the floor's Arenium as damage. `rig` now gives a wheel
without a `material` line Rubber 0.

**Implementation.** `combat::ArenaBodies` (built by `duel::arena_bodies`:
the static body with one triangle set per fixed `.gmf` body, every `.gmf`
prop, `AddXtra` props with their own material), `EventGate::admit_with`,
`combat::resolve_with(.., arena)` → `arena_hit`, `Impact::attacker =
combat::NO_ATTACKER`, which `Duel::resolve_combat` applies with no attacker,
so `Match::route_popups` shows the red number (docs/37 §7). Tests
`duel::tests::a_robot_striking_the_arena_is_hit_by_its_arenium`,
`duel::tests::an_arena_collidable_lets_one_point_through_a_second`.

**Validation.** Arena hits on each robot per fight, by amount (non-zero only),
and the applied ones (`>= 10`) per fight with their hitpoints; live = the four
new captures and the older `fight-*` captures, whose `H` rows have them too:

| | live: < 1, 1-2, 2-5, 5-10 / applied (hp) | ours after: same |
|---|---|---|
| box Scout (8 live, 24 ours) | 87, 29, 18, 4.7 / 4.2 (99) | 76, 31, 21, 5.5 / 4.7 (73) |
| box ALARM | 33, 1.8, 2.9, 2.6 / 0.6 (15) | 26, 6.6, 4.7, 3.8 / 2.2 (47) |
| Octagon Scout (3 live) | 121, 8.3, 11, 3.0 / 2.0 (31) | 49, 34, 17, 6.8 / 4.6 (65) |
| Octagon ALARM | 62, 6.7, 5.3, 1.0 / 3.3 (100) | 27, 9.5, 13, 9.8 / 3.3 (47) |
| Compressor Sentinel (8 live, 12 ours) | 95, 23, 14, 8.8 / 5.9 (178) | 78, 33, 26, 9.4 / 9.2 (180) |
| Compressor Wide Load | 74, 42, 40, 14 / 4.8 (146) | 73, 49, 42, 11 / 1.8 (28) |
| box Razor (1 live, 24 ours) | 187, 10, 3, 0 / 0 | 20, 10, 9, 3.4 / 4.0 (128) |
| box FlapJack | 145, 1, 2, 0 / 1 (24) | 17, 32, 23, 2.6 / 2.0 (57) |

Before = `fidelity` 8cfa6ae built from a clean tree (it reproduces §12n's
after column to the digit), after = this branch; `FIDTRACE_WARMUP` ensembles
as §12n (counted = `>= 10` on the other robot, robot hits only, mean (total),
mean length, full-length runs):

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM, 24: ALARM / Scout | 38.5 / 4.8; 3/6 full | 16.0 (132) / 3.7, 71 s, 3/24 | 18.3 (80) / 3.8, 75 s, 6/24 |
| Compressor Sentinel v Wide Load, 12: Wide Load / Sentinel | 76 (283) / 5.3 | 53.3 (316) / 2.6, 91 s, 7/12 | 52.8 (306) / 2.4, 92 s, 7/12 |
| Octagon Scout v ALARM, 24: ALARM / Scout | 24 (944) / 4 | 8.3 (68) / 1.7, 70 s, 6/24 | 6.3 (40) / 1.6, 67 s, 4/24 |
| box Razor v FlapJack, 24: Razor / FlapJack | 3 (40) / 1 (12) | 14.7 (230) / 0.6, 120 s, 23/24 | 13.8 (173) / 0.8, 116 s, 22/24 |

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after; three
fights change (box Scout v ALARM 1/6 → 3/18 counted, Sentinel v Wide Load 2/17
→ 0/21, Bushido v Devil 35/15 → 25/5), the others are the same to the hit.
The totals fall because the wheels no longer price anything (none of their
hits reached 10). The counted means move by less than one standard error
(box ALARM's sd is 15, the Octagon's 7-14): the arena's hits take 50-180 hp a
fight off a robot's components, which changes trajectories and the brains'
damage readings from the first one on, so the runs decorrelate, but the hits
the robots land on each other are not systematically more or fewer. The
arena's sub-10 hits on the box Scout match live bin for bin; ALARM, the
Octagon's robots and the Razor take more applied arena hits than live, the
Compressor's Wide Load fewer: those follow how the robots move and land (the
Razor's are its tumbles, ALARM's its falls onto its back, §12n), not the
pricing, which reproduces live record by record.

**Still open.** Which collidable is the event's A (the arena's list or the
robot's first) is not traced; ours takes the physics' `a`. The Octagon's
registrations were not captured (its colliding bodies are assumed registered
as in the box and the Compressor). `AddXtra` and KOTH prize components are
implemented from the code, not seen live. The crowd now also hears arena hits
over 50 (as `FUN_004175b0` does), not checked against live audio.

### 12p. FlapJack's wedge lies flat, and the Razor goes over (fx/razor)

New live captures (two fresh sessions on `:7`, prefix sha256 checked before
and after; manifest rows in `manifest-additions-2026-10-21-razor.tsv`):
`arenahits-razor-flapjack-box__live__000..004` with
`fightpos-razor-flapjack-box__live__000..004` (five 120 s box fights,
`capture_fightpos_set.sh` with `AGENT=frida/ra2_arenahits.js`: every priced
hit with both bodies, and every 6 frames both robots' location,
`isUpsideDown`, last tactic, commands and sensor table) and
`fightweapons-razor-flapjack-box__live__000` (`frida/ra2_weapons.js` over the
same session); `flipfight-razor-flapjack-box__live__000..002` with
`fightpos-razor-flapjack-box__live__005..007` (three more fights with the new
`frida/ra2_flipfight.js`: `ra2_fightcontacts.js` plus every hinge motor
setting, `M`, in one seq, so a stroke lines up with the contacts and the
bodies). Diagnostic: with `RA2_H17_DUMP` ours now writes the `M` row at
every burst phase change, so the live scripts read our strokes too.

**The live fights end with the Razor on its back.** Razor (`Rammer`, the
`Charge` tactic: no weapon to fire; its spike strips, piercing 0.8, Metal
0.8, price `≈ 6.6 · rec0` per strike, as ours) lands 5, 2, 5, 1, 0, 0, 5, 6
counted hits in the eight new fights (with `fight-...__live__000`'s 3: mean
3.0, sd 2.2); FlapJack 0 in all eight (1 in `__000`). In every one of the
eight FlapJack's flipper puts the Razor on its back, 5-35 s in (median 17 s,
`fightpos`, `isUpsideDown`; lifted 0.3-0.7 m by 1-6 strokes first), the Razor's `Invert` tactic cannot right it, it creeps 0.3-6 m
in the next 10 s and is counted out by the immobility rule (`immobility.rs`)
20-38 s later: the brains fall idle at 27-64 s (mean 42). Ours before
(`fidelity` 868d9ed): 116 s, 22 of 24 full length, and in the 23 strokes of a
probed fight (`fightprobe`, warmup 0, first 60 s) the Razor never rose more
than 0.15 m. So the Razor's surplus was the time it got to ram, not its
strikes: per second before the flip live lands about 0.17 counted hits, ours
0.12 per second of fight.

**At rest FlapJack's nose was 4° up, its wedge stood on edge.** Live the
resting FlapJack's chassis has its centre of mass at y -1.865 with its
forward axis 0.016 below horizontal (`flipfight-...__live__001`, 458
samples; the assembly capture's rest -1.866 / -0.011); ours sat at -1.835,
0.061 nose up. Its flipper's floor contacts live lie at the two corners of
the wedge's leading edge, x -0.30 and +0.24 in the chassis frame (the
wedge's 0.54 m width across the robot), 0.33 m below the chassis' centre of
mass and 1.40 m ahead; ours touched at one point on the centre line 0.50 m
below. The flipper body's local inertia settles it (live `+0x19c`, the same
in all three `flipfight` captures): 1.4788 about the vertical, 0.3827 along
the arm, 1.1849 about the hinge, products 0.0531, -0.0055, 0.1686. Ours,
with the wedge's object `TM` (a quarter turn about x) applied to its points,
had the principal moments 1.434 about the hinge and 1.231 about the
vertical: a plate 0.54 m tall. With the points unrotated it is
1.1849 / 0.3827 / 1.4788 with the same products to four digits.

**Own geometry ignores the object's rotation (verified live for the Small
Wedge).** A rigid body that simulates its own object
(`SIMULATION_GEOMETRY` 2, no proxy) collides with that object's points
placed by the `TM`'s translation only; a proxy (`SIMULATION_GEOMETRY` 3)
keeps its `TM`: the angle motors' `axle_collision` is also a quarter turn
and the wheel bodies' spin moment 0.5569 (live, every wheel) is the rotated
box's; unrotated it would be 0.5813. `rig::own_geometry_unrotated`,
`HavokParts::from_scene`; test `mass_live::flapjacks_flipper_lies_flat`.
Stock objects it turns: the Small Wedge, `flipper1.gmf`'s flipper (both
`emergency` and `opponentgear`; EMERGENCY's) and `spinmotor1.gmf`'s `axle`
and `motor` (no shipped AI robot uses it); the others have rotations below
1e-6. **The original draws it flat too (live, fx/loose-ends).** `capture_shape.py`
(`shape-flapjack-emergency__live__000-0{0..7}.png` and `.jsonl`: FlapJack and
EMERGENCY standing in the Combat Arena, hazards off, both AIs neutered, the
docs/37 §6 camera at eight robot-relative positions) against `ra2-app
--example shapeshot` (same cameras): before, ours stood FlapJack's Small Wedge
and EMERGENCY's two flipper plates on edge like vertical boards; live they are
the low ramps lying on the floor (the wedge's thin edge at the floor, the
striped plates sloping down to it). The drawn object is now placed like the
collider: an object that is a rigid body's own mesh (`SIMULATION_GEOMETRY` 2,
no proxy) is drawn by its `TM`'s translation alone
(`GmiScene::object_is_own_geometry`, `RigidBody::simulates_own_mesh`, used by
`ra2_render::scene_items` and the bot loader, not by arena scenery, whose own-mesh
bodies with a rotation (barrels, the Silver Cup, the hilltop spike) are
drawn as before). After the change the side-by-sides agree in
shape and place. Test
`ra2_render::tests::a_component_that_is_its_own_collision_mesh_is_drawn_unrotated`.
Which code drops the rotation was not located (the GMF reader's
`*SIMULATION_GEOMETRY` table at `0x7077ac` was the start point).

With it ours rests as live (-1.864, -0.018; the tip contacts at x -0.30 and
+0.24, 0.33 m down), and FlapJack's strokes lift the Razor: in the same
probed fight three of the first four strokes raise it 0.7-1.2 m and the
fourth puts it on its back at 17.5 s.

**Validation.** Before = `fidelity` 868d9ed built from a clean tree (it
reproduces §12o's after column to the digit), after = this branch.
`FIDTRACE_WARMUP` ensembles as §12n (counted = `>= 10` on the other robot,
robot hits only, mean (total), mean length, full-length runs):

| fight | live | before | after |
|---|---|---|---|
| box Razor v FlapJack, 24: Razor / FlapJack | 3.0 / 0.1 (9 fights, sd 2.2), match over at 27-64 s, 0/8 full | 13.8 (173) / 0.8, 116 s, 22/24 | 4.4 (133) / 0.5, 69 s, 3/24 |
| ... Razor on its back | 8 of 8, 5-35 s (median 17) | 0 of 4 probed | 24 of 24, 4-40 s (median 10) |
| box Scout v ALARM, 24: ALARM / Scout | 38.5 / 4.8 | 18.3 (80) / 3.8, 75 s, 6/24 | the same to the digit |
| Compressor Sentinel v Wide Load, 12: Wide Load / Sentinel | 76 (283) / 5.3 | 52.8 (306) / 2.4, 92 s, 7/12 | the same |
| Octagon Scout v ALARM, 24: ALARM / Scout | 24 (944) / 4 | 6.3 (40) / 1.6, 67 s, 4/24 | the same |

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after;
Razor v FlapJack (box 16/1 → 4/0, FlapJack wins at 108 s; Parking Lot 18/1
→ 2/1) changes as above, and Grog v Boxer (box and hill) and Tornado v
Ripblade change by float noise (their own-geometry objects carry rotations of
1e-7, now dropped); the other five are the same to the hit.

**Still open.** After the flip ours ends later: 11 of 24 runs are over 20-38
s after it, as live (20-38 s), but in the rest the Razor keeps being shoved
(median 3.1 m in the 10 s after the flip, live 1.4 m) and FlapJack keeps
firing at it (strokes at 25-35 s in the probed fight; probably the `Flipper`
brain's `CanDriveUpsideDown`, which allows firing at an upside-down robot
once it has moved 3 m within 10 s), so the immobility count restarts: 3 run
the full 120 s, 10 others end 45-110 s after the flip. Ours also flips the
Razor at the first meeting more often (9 of 24 within 5.4 s, live 2 of 8
within 6.5 s; the two time bases may differ by about a second). Whether the original draws the Small Wedge flat;
the code behind the own-geometry rule.

### 12q. Every body of the four robots against live; the piston rod's frame (fx/alarm-back)

Diagnostic: `examples/bodycmp.rs` prints every rigid body of each robot as
built (mass; local inertia, its products and principal moments; centre of
mass, inertia and rotation in the chassis body's frame; the collision points'
extent per body and component), and with `BODYCMP_SETTLE=<s>` the chassis' rest
pose after standing that long with no input. The live side is the `P` rows
(`+0x190` mass, `+0x19c` local inertia, rows of four floats) and `B` rows
(`+0x30` centre, `+0xcc` rotation, stored by columns) of the existing
`fightcontacts-scout-alarm-box__live__000..006` and
`contacts-nohaz-wideload__live__001`, the `assembly-*__live__000` bodies and
the `contacts-nohaz-*` rest poses. New agent `frida/ra2_alarmfight.js`:
`ra2_flipfight.js` plus every prismatic `SetPowerSettings` (`0x4b2af0`, `K`
rows), so a hammer stroke lines up with the contacts and the bodies.

**What matches (Scout, ALARM, Wide Load, Sentinel).** Every chassis body's
mass and full local inertia to four digits, products included (Scout 23.284 /
39.084 / 21.266, -0.264 / 1.263 / 0.396; ALARM 49.149 / 72.705 / 36.411;
Wide Load 44.308 / 108.785 / 86.579); every wheel body's mass, centre of mass
in the chassis frame (Scout and ALARM to 0.1 mm as built, all four robots to
1-5 mm after settling, where live and ours sag the same 10-20 mm on the
hinges; Sentinel's wheels sag 6-11 mm less in ours) and spin moment; the
Scout's fork arm in the chassis frame (2.573 / 2.327 / 0.357, product 0.633,
live 2.573 / 2.327 / 0.357, 0.633); the chassis' contact extents (§12k, the
0.05 tolerance included); and the rest poses on the box floor:

| chassis centre y, forward.y | live | ours (3 s, no input) |
|---|---|---|
| Scout (`contacts-nohaz-scout__live__000`) | -1.886, -0.045 | -1.885, -0.035 |
| ALARM (`contacts-nohaz-alarm__live__000/003`) | -1.810, -0.004 | -1.810, -0.003 |
| Wide Load (`contacts-nohaz-wideload__live__000`) | -1.766, +0.055 | -1.761, +0.053 |
| Sentinel (`assembly-sentinel__live__000`) | -1.800, -0.017 | -1.796, -0.012..-0.018 |

The geometry types of every component these robots use agree with §12g/§12p:
the motors' and pistons' bodies and rotors, the extenders, connectors, tips,
hammer, tanks and the control board are proxies (`SIMULATION_GEOMETRY` 3),
the batteries, wheels' groups, the axle mount's display and the SmartZone
their own geometry (2, none of them rotated), the fork arm and the bracket
wedge meshes (6), the steering hubs 0 (convex). No other body field of the
chassis, wheels or fork (`+0x0..+0x1fc`) carries a damping or limit ours lacks
(`+0x194/+0x198` are restitution and friction, 0.2 / 0.5 on the chassis).

**A piston rod's inertia is in the piston's frame (bug, fixed).** Section 12n
gives the rod its housing's local inertia with the diagonal scaled
(`0x4b4d4f..0x4b4de7`), and our rod had it, in its *body* frame. But live the
rod body's frame is the piston's: ALARM's rod is its chassis turned half a
turn about y (`B` rows, every sample of `__live__001`), as its Burst Piston,
and Wide Load's rod has its chassis' frame, as its piston
(`contacts-nohaz-wideload__live__001`). Ours took the frame of the part bolted
on (the Hammer Head, a quarter turn about y; Wide Load's T connector, a
quarter turn about two axes), so the same numbers lay on the wrong axes: in
the chassis frame ALARM's rod had 7.28 about the pitch axis and 9.83 about the
roll axis (live 9.83 and 7.28), Wide Load's 17.32 / 8.86 / 21.76 (live 8.86 /
21.76 / 17.32): its 63 kg spike assembly 2× too stiff in pitch and 2.5× too
loose in yaw. `Robot::spawn` now turns the scaled tensor from the piston's
frame into the rod body's; test
`mass_live::piston_rods_take_their_chassis_inertia_in_the_pistons_frame`
(both robots, in the chassis frame, to 0.01).

**Not matched: the wheels' transverse moments.** Live a Mini wheel on a
Reversible Angle Motor (27 kg) has 0.4281 / 0.4297 across the axle, ours
0.4502 / 0.4519 (+0.022, the spin moment 0.5569 and the centre exact); on an
Axle Mount (26 kg) live 0.4619, ours 0.4631; the N-12 wheel on a Z-Tek (32 kg)
live 1.5039, ours 1.545 / 1.548 (+0.042). The surplus follows the rotors whose
open proxies put their mass off their box's centre (angle motor 2.5 cm, Z-Tek
3.8 cm; the axle mount's is centred). The open solid's own inertia from the
pivot (tried) gives 0.444 and moves the spin moment to 0.560; not recovered.
Against a robot's 70-130 kg·m² it is below 0.1 %.

**Sentinel (new capture).** `alarmfight-sentinel-wideload-compressor__live__000`
gives Sentinel's bodies for the first time: the chassis 296.394 kg,
78.859 / 122.523 / 59.195, products 0.2417 / 1.521 / -0.1238 (ours the same to
four digits); the rear SlipperBottom wheels 28 kg, 0.9129 about the axle and
1.2766 across, the steered ones 23 kg, 0.9181 / 0.6103 / 0.6111 (ours the
same); the Bracket Wedge's principal moments 1.0498 / 2.2472 / 3.1564 (ours
1.0507 / 2.2464 / 3.1565). Wide Load's bodies in the same capture repeat
`contacts-nohaz-wideload__live__001` to the digit.

New live captures (three fresh sessions on `:7`, prefix sha256 checked before
and after each; manifest rows in `manifest-additions-2026-10-21-alarm-back.tsv`):
`alarmfight-scout-alarm-octagon__live__000..001` with
`fightpos-scout-alarm-octagon__live__000..001` (two Octagon fights; a second
fight of the first session did not start and was not retried there) and
`alarmfight-sentinel-wideload-compressor__live__000` with
`fightpos-sentinel-wideload-compressor__live__003`. The live Octagon set is
now five fights (`fight-...__live__000`, `arenahits-...__live__000..001`, the
two new): ALARM 24, 13, 20, 7, 4 counted (mean 13.6), the Scout 4, 5, 4, 3, 0.
In the first new one an Octagon hazard throws ALARM 3.6 m up at 13.3 s and it
lands on its back for good; in the second ALARM lands 4. The Compressor set is
eight: Wide Load 60, 118, 158, 136, 8, 32, 23, 54 (mean 73.6), Sentinel 2, 13,
7, 7, 1, 3, 4, 5.

**ALARM's falls, again.** With every body as live, our box ALARM still goes
onto its back ≥ 5 s by 45 s in 14 of 24 runs. Classified from `fightprobe`
(24 runs, 46 s, up vector every 0.5 s): 35 tips from upright (up.y < 0.5), 16
back onto the wheels and 19 onto the back, 11 of those within 1 s; by the
forward axis at the tip, 20 pitch over the tail (10 to the back) and 15 roll
over a side (9 to the back); before the fix 23 / 9, 13 and 5 to the back.
Live the box captures with bodies (`fightcontacts-...__live__000..006`) hold
seven tips, two to the back: one pitch, a 9 s tail-stand with the hammer
hopping ALARM (`__002`, 14.5-24 s, up.y between +0.2 and -0.48 resting on its
rear edge, 0.73 m behind the centre), one roll at 3.4 rad/s about the forward
axis that rests 0.5 s on the side edge (x -0.92) and goes on (`__005`, 28.4 s).
In the recovered tips ALARM had climbed onto the Scout and stood on its tail
on the Scout's chassis. What tips ours over so much more often is not found;
the bodies, contact extents and the hammer's kick (§12k) are not it.

**Validation.** Before = `fidelity` a1d2abd built from a clean tree (it
reproduces §12o/§12p's box and Octagon rows to the digit), after = this
branch. `FIDTRACE_WARMUP` ensembles, 24 runs each (the Compressor's 24 now
too), counted = `>= 10` on the other robot, robot hits only, mean (total), mean
length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 18.3 (80) sd 15 / 3.8, 75 s, 6/24 | 21.4 (71) sd 22 / 4.1, 67 s, 2/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 15/24 | 14/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 55.8 (268) sd 39 / 2.7, 93 s, 15/24 | 67.9 (278) sd 41 / 3.9, 98 s, 14/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 6.3 (40) sd 7 / 1.6, 67 s, 4/24 | 12.5 (56) sd 15 / 2.2, 68 s, 4/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 4.4 (133) / 0.5, 69 s, 3/24 | the same to the digit |

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after; the
pairings with a piston change (box Scout v ALARM 3/18 → 5/13, Octagon 0/5 →
1/13, Sentinel v Wide Load 0/21 → 5/75, Grog v Boxer box 62/98 → 71/202 and
hill 32/42 → 25/41), the other five are the same to the hit. The Octagon and
the Compressor move to within a standard error of live; the box ALARM's
counted hits stay at about half of live, because it is on its back as often
as before.

**Still open.** Why our ALARM tips over in half its tips where live recovers
from five in seven (above); the wheels' transverse moments; Sentinel's wheels
sag 6-11 mm less on their hinges than live under its 296 kg chassis.

### 12r. Controlled tips against live; the piston's limit, locks and ticks, the wheels' traction phantoms, the spawn height (fx/tip-ab)

Fight ensembles are too chaotic to say why our ALARM ends on its back so
often, so this round compares the two engines from identical states, one
mechanism at a time.

**The experiment.** New agent `frida/ra2_tipab.js` with
`tools/oracle/capture_tipab.py` (a box match, both AIs neutered, hazards off;
the immobility rule's 20 s raised in the process's memory only, `0x6a57f0`, so
the match is not ended by a robot standing still): at the head of every
`World::stepTo` (`0x52aa60`) it holds every body of a robot 1 m up and still
for 30 substeps (so no contact of the old pose is left), then turns the rest
pose rigidly about the chassis' centre of mass, sets the centre, and gives each
body `v + w × r` and `w`, writing the body's centre (`+0x30`), quaternion
(`+0x40` w, `+0x44` x y z), velocity (`+0x54`) and angular velocity (`+0x64`)
and calling the body's own sync (`0x520750`: the rotation matrix `+0xcc` from
the quaternion, `+0xf0` from the centre, the world inverse inertia `+0x10c`,
the version count `+0x25c`). The integrator state is exactly these
(`0x520fb0` packs centre, quaternion, `m v`, `ω`; `0x521160`, the
derivative: `ω` integrates as `0.5 ω ⊗ q`, `dω = I_w⁻¹ τ`, no gyroscopic term,
as §2). The weapon is fired, and the robot driven, through its AI
(`Input("Fire")`, `Throttle`, `Turn`); when that landed shows in the rows.
`examples/tipprobe.rs` places ours from the live placed state (`X` row) and
steps it the same way (`TIPPROBE_DRIVE`, fire time, `TIPPROBE_PHASE`). The
robot rests and is placed at its rest heights from our solid geometry (the
wheels' hull points include the traction rings, 0.0975 m below the solid
wheel).

New live captures (three fresh sessions on `:7`, prefix sha256 checked before
and after each; manifest rows in `manifest-additions-2026-10-21-tip-ab.tsv`):
`tipab-alarm-box__live__000..002`, `tipab-alarm-box-hammer__live__000`,
`tipab-alarm-box-drive__live__000`, `tipab-scout-box-fork__live__000`,
`spawn-scout-alarm-box__live__000`, `spawn-scout-alarm-compressor__live__000`,
`spawn-razor-flapjack-box__live__000`, `burstticks-scout-alarm-box__live__000..001`.

**Tips, no weapon, no drive: the same.** 46 trials (pitch over the tail from
-60, -90, -110 and -120 degrees, roll to either side from 45, 60, 70 and 75,
`w` 0-4 rad/s about the same axis, drops of 0.3 m at 10-20 degrees): every
outcome agrees (on the wheels, on the tail, on the back), and so does `up.y`
at 0.25-4 s to 0.01-0.05, the contact points in the chassis frame to the
centimetre and `w` step by step until the robot lands. The thresholds: from
-90 (on the tail) ALARM goes over at 4 rad/s and recovers at 3 in both; from
-110 it goes over with 2 rad/s and recovers with 1; a roll from 60 degrees
recovers at 2 rad/s and falls at 3-4. Near the threshold our roll loses
0.2-0.4 rad/s more on the side edge, so ours is, if anything, the steadier. In
the first set (`__000`) the placement put the chassis' side edge 2 mm into the
floor: live that pair produced no contact at all and the robot sank 11 cm
into the floor until its wheels caught it, ours held it at the overlap; not
followed (a placement artefact; a fight reaches it only after a bad step).

**The hammer (`tipab-alarm-box-hammer__live__000`): three differences.**
Fired on its tail, on its back and on its side, step by step (ALARM on its
tail, hopping):

| step | live rod (chassis frame) | ours before | ours after |
|---|---|---|---|
| out | 5.4, 10.2, 15.0, 12.8 m/s, then 0.000 at the stop | 13.5, then +0.45 past the stop, back in at -0.7 m/s | as live |
| held out | at the stop to the millimetre for 10 substeps | drifts 6 cm in; pushed out again at +3.8 (+0.55 m/s to the robot) | as live |
| back | 14 substeps after it started out | 16 | 14 |
| home | +3.30 then -3.32 m/s, the robot falling at -3.30 | stops dead | as live |

* *A prismatic's limit is re-solved in every solver pass (recovered).* The
  prismatic atom (vtable `0x6ac524`: set-up `+0x24` `0x555080`, pass `+0x28`
  `0x555960`) re-runs its motor and limit (`0x555cc0`) in each of the four
  passes before its block, with the pass's `tau`: an impulse that brings the
  predicted position `x + v_rel · dt` back onto the stop it would pass,
  `-(predicted - bound) / dt · tau · m` (the atom's limit factor `+0x30` is
  1). The hinge's pass (`0x5361e0`) does not; its limit acts at the set-up
  only, as ours. Ours set the slider's limit once, and the hinges' passes on
  the wheels then moved the chassis under it: the rod overshot by 5-12 mm and
  the correction threw it back in. `joints::Joint::limit_pass`. The atom's
  pass also re-runs its motor, which advances the motor's target in every pass
  (`0x555ea1..0x555eb9`); our sliders do not use the motor (a piston's rod and
  the hazards' sliders are driven by forces in the game), so it is not built.
* *A lock stops the driven body in the world (recovered).* `Lock(1)` of the
  hinge (`0x480e20`) and the prismatic controller (`0x4b50c0`) collapse the
  limits onto the joint's position and set the driven body's velocity and
  angular velocity to the zero vector `0x71abcc` (`0x480f00`, `0x480ef0`):
  absolute, not relative to the parent. So the rod, locked home while ALARM
  falls at 3.3 m/s, leaves its stop at +3.30 m/s. A lock holds until the
  device's own `Lock(0)` (the fire, `0x448750`/`0x446810`, and the end of the
  stroke, `0x4486c4`/`0x4466f4`); arriving at the stop it is driven toward
  locks it (`0x4b5240`, `0x481450`) and nothing in between unlocks it. Ours
  unlocked whenever the joint was 0.05 off the stop. `lock_joint` zeroes the
  child; `Robot::burst_locked`.
* *A burst device ticks once a frame (verified live).* `burstticks-*`: the
  piston's and the burst motor's ticks (`0x448650`, `0x446670`) come between
  the frame's two substeps with the frame's time (1/30 s, less when a frame
  was short: 0.009-0.033 live), the fire is taken at the tick (the press flags
  `+0x284/+0x285`) which takes its frame off the new timer at once, and the
  prismatic controller pushes every substep. From the one-shot push to the
  return push the piston takes 15 substeps (17 after a short frame, six
  firings), the fork from its firing setting to its cocking one 34 (36, six
  firings): 0.25 and 0.6 s in whole frames. Ours ticked every substep, 17 and
  36. `Robot::frame_clock`, `FRAME`; the fork's stroke then matches live step
  for step (`tipab-scout-box-fork__live__000`: -18.7, -10.5 rad/s, at rest
  from the fourth substep, cocking 34 substeps after the fire).

**Driving while tipped: the wheels' traction phantoms touch only when they
overlap (verified live).** `tipab-alarm-box-drive__live__000` drives ALARM at
rest, on its tail and while it tips (`G` rows: each wheel's thrust and
touching flag). Live, ALARM on its tail at `up.y` -0.39 with full throttle has
no wheel touching and so no thrust, until it settles to `up.y` -0.19; ours had
both rear wheels touching from the start, so the thrust drove it 3 m off and
back onto its wheels where live stayed on its tail. Our rear rings were
0.07-0.09 m off the floor there: ours listed a phantom's pair within the 0.1 m
tolerance, as a solid pair. A SmartZone's pair is listed only at a negative
distance (§12d, §12i), and so is a wheel's: rolling back onto its wheels live
each wheel turns touching within a frame of its ring reaching the floor, ours
up to 0.1 m before. `Havok17Backend::detect_phantoms` now asks for overlap.
After it the drive trials follow live (on the tail at 1.5 s, `up.y` -0.10 /
-0.06; the wheels' touching flags within a few substeps of live); ours then
wheelies about 10 % faster (live's battery had run down: thrust 1802-1822,
ours 1940) and comes down at 2.5-3 s where live keeps the wheelie to 4 s.

**The spawn height (verified live; coordinator's lead).** `spawn-*__live__000`
(the agent loaded before Start Game): each robot is put at its start point,
lowered, held still through the intro and let go at the bell. Its origin (the
`.bot`'s, our chassis body's) is then at the floor under the start point plus
0.3975 m, for Scout, ALARM, Razor and FlapJack in the Combat Arena and Scout
and ALARM in the Compressor alike (the start points stand 1.75 and 1.30 m over
those floors, so it is the floor, not the start point); each falls 0.20-0.21 m
(-2.2 m/s at the floor), and ours fell 0.12 (FlapJack 0.16). `SPAWN_ABOVE_FLOOR`
now puts all six to 0.2 mm. The code that places them was not located (a
`0.4` near a `0.03` and a `5` at `0x403d83` was looked at, not followed). The
award scene's extra lift (`prize_scene::SPAWN_LIFT`, docs/36 §9.23) is reduced
to keep its measured 0.226 m drop.

**The mirror lead (coordinator's).** No sign or handedness error shows in the
tips: both roll directions, both pitch directions and the drops agree to
0.03 in `up.y`, and the post-landing yaw drift differs by a few degrees either
way. A 10 s rest creep of ALARM (`tipab-alarm-box-hammer__live__000` trial 0)
wanders +4 degrees live and ±1 ours, too short to call. The award scene's
Atom Smasher still turns +0.0157 rad/s (live -0.0145) after this round; not
followed.

**Fights.** With all of it, our ALARM still tips more often than live and goes
over more often when it tips. Live box fights with bodies
(`fightcontacts-...__live__000..006`, 277 s): 7 tips (`up.y` through 0.5), 1 to
the back, all with the Scout in contact; ours (12 runs, 46 s, 552 s): 18 tips,
8 to the back, all with the Scout in contact, 10 of them rolls, several at 4-6
rad/s after the Scout's fork has gone under ALARM's side (live one roll, at 2
rad/s).
In the tips we read, ALARM's brain keeps its `Engage` tactic for up to a
second after it is past vertical (tactics are re-evaluated once a second,
`SuperAI.Tick`), turning or driving on its tail: live the same mechanism, with
the evaluation landing earlier in the live cases. The fork's own stroke now
matches live; what the fork does to ALARM's side was not isolated.

**Validation.** Before = `fidelity` bed66ad built from a clean tree (it
reproduces §12q's after column to the digit), after = this branch.
`FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10` on the other
robot, robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 21.4 (71) sd 22 / 4.1, 67 s, 2/24 | 20.0 (139) sd 15 / 3.6, 94 s, 10/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 14/24 | 15/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 4.4 (133) / 0.5, 69 s, 3/24 | 5.7 (99) / 0.9, 58 s, 2/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 67.9 (278) sd 41 / 3.9, 98 s, 14/24 | 71.2 (295) sd 36 / 3.9, 97 s, 16/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 12.5 (56) sd 15 / 2.2, 68 s, 4/24 | 16.5 (64) sd 17 / 2.6, 78 s, 5/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 19/24 | 18/24 |

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after; every
pairing changes (the spawn moves every opening). Within the ensembles' scatter
(a 24-run mean moves by about one standard error, 3-4 counted hits, and the
on-its-back count by 2-3) nothing moved beyond it except the box fights'
length (67 → 94 s, 2 → 10 full): ALARM's falls are not what these mechanisms
were, though each of them is what the original does.

**Still open.** What the Scout's fork does to ALARM's side (a two-robot
placement, the next controlled experiment); the prismatic atom's motor
re-run per pass (unused by our sliders); the hazards' sliders still use our
velocity motor and our own arrival lock (not the sticky lock above); the
placing code of the spawn; the initially overlapping pair; the award scene's
Atom Smasher creep.

### 12s. Two robots placed against live; the mesh x mesh trees' tolerance (fx/fork-side)

Section 12r's single-robot tips agree with live, yet in fights our ALARM tipped
about twice as often and went over more (box, 12 runs of 46 s: 21 tips, 7 to
the back, 12 of them rolls, seven at 4.6-5.6 rad/s; live 7 tips in 277 s, one
to the back, one roll at 2 rad/s). This round places *both* robots.

**The experiment.** New agent `frida/ra2_twobody.js` with
`tools/oracle/capture_twobody.py` (after `capture_tipab.py setup`: box,
hazards off, AIs neutered): at the head of `World::stepTo` both robots' bodies
are held 1 m above their rest poses for 30 substeps, then written at poses the
script computes (ALARM's rest pose moved to the arena's centre; the Scout
turned about the vertical to face one of ALARM's faces, its fork arm's lowest
front edge `gap` m from it, `lat` along it, `yaw` off square, every body of it
at `speed`), with the throttles set through the AIs before the hold. Every
substep the rows give every body as the resolver gets it (`B`, with the
`+0x1c` flags in the second set) and as the resolver leaves it (`V`: with `B`,
each body's contact impulse of the piece), the contacts (`R`), the pieces
(`A`), the wheel records (`G`), the weapons' settings (`M` hinge
`SetPowerSettings`, `F` prismatic pushes) and the components' ticks with their
frame time (`TM`/`TP`). `examples/twoprobe.rs` replays a trial in ours: the
live placed state (`X` row; each chassis at its live pose, the other bodies
carried rigidly with it, every body at the live velocities), the throttles
from the start of the hold, the live ticks with their times
(`Robot::set_tick_schedule`, a diagnostic hook: live frames are irregular,
0.006-0.033 s, and two ticks may fall between two substeps) and every fire the
live rows show. The neutered AIs still fire on their weapon zones (in the first set
the Scout's fork fired in all 20 trials, in 9 with no press from the script,
and ALARM's hammer in 5, twice unasked), so the replay takes the fires from
the `M`/`F` rows, not the grid.
The scratch scripts that build the replay and compare are
`tools/oracle/twobody/` (`mkours.py`, `cmp.py`, `detail.py`).

New live captures (one session on `:7`, prefix sha256 checked before and
after; manifest rows in `manifest-additions-2026-10-21-fork-side.tsv`):
`twobody-scout-alarm-box__live__000` (20 trials: the Scout driving into
ALARM's side at 1-3 m/s at 0.05 m, at three offsets and both sides, already
under it, coasting, head-on with both driving, the hammer fired onto the
Scout at three gaps, from behind) and `__001` (17 trials: gap 0.1, approaches
20-30 degrees off square, ALARM driving or reversing across the fork, a lift
of 0.05, a 10 s rest).

**The first divergence: the fork loses ALARM's side edge.** In the first set
live ALARM is lifted on the fork and comes back onto its wheels in 19 of 20
trials (lowest `up.y` 0.62-0.85 in the side trials; the one exception is the
head-on trial with both driving, -0.16 in both engines); ours rolled it onto
its back in 15 (11 of them stayed there), at up to 6.7 rad/s where live
peaked at 2-4. Step by step (trial 0, the Scout at 1 m/s into ALARM's +x
side): the fork tip reaches the side edge at substep 5. Live the resolver gets
5-9 points between the fork arm and ALARM's chassis every substep from there
on, at 0.055-0.098 m, and pushes 20-110 N·s a substep; ours got two, then at
substeps 7-11 none, so the fork swung in unloaded and struck the chassis at
0.013-0.035 m with 180-220 N·s in one piece, and ALARM's chassis rose at
4.7 m/s where live rose at 1.5. Counting every pair of triangles within the
tolerance, ours had 12-20 such pairs at those substeps and its tree query let
none of them through.

**Two moving meshes' trees are walked with each box grown by half the
tolerance (recovered).** The fork arm and ALARM's chassis are both meshes on
moving bodies, so the pair goes to `FUN_00549d00`, which walks the two
bounding-box trees together (`FUN_00550100`) and passes `0.5 · tol` to the
oriented-box overlap test `FUN_00557870`; that test adds the margin to the
half extents of *both* boxes before every separating axis, so two boxes are
dropped only when more than a whole tolerance separates them. Ours grew one
side's box by half the tolerance in all, so any pair of triangles more than
0.05 m apart along an axis was never measured. Since the separation along an
axis never exceeds the distance, the original's walk keeps every pair of
triangles within the tolerance, and so does ours now (each triangle's box
grown by the whole tolerance against the other mesh's tree;
`Havok17Backend::mesh_mesh`); test
`havok17::tests::two_moving_meshes_meet_within_a_whole_tolerance` (two slabs
whose edges are 0.08 m apart on a diagonal, their boxes 0.057 m apart on two
axes). Section 10's "half of it" read the call's argument, not what the test
does with it. A fixed mesh (the arena) was already queried with the whole
tolerance.

| replays of the live placements (ALARM's chassis) | live | ours before | ours after |
|---|---|---|---|
| set 1: on its back at some point / at the end (20) | 1 / 1 | 15 / 11 | 3 / 2 |
| set 1: mean of the trials' peak \|ω\| | 2.6 rad/s | 5.1 | 3.4 |
| set 1: both chassis' end centres from live, median (40) | | 2.42 m | 1.39 m |
| set 2: lowest `up.y` (17) | ≥ 0.99 | ≥ 0.93 | ≥ 0.90 |
| set 2: both chassis' end centres from live, median (34) | | 1.81 m | 0.27 m |

In set 2 the Scout only pushes ALARM (the fork does not fire at 0.1 m), and
the pushes now end where live's do: the pair's points slid off and came back
before, so the Scout drove 1-2 m further. In set 1 the first 20 substeps of
trial 0 follow live to 2-6 cm with the same points and normals; the trials
still part later where the fork's stroke meets ALARM a substep earlier or
later (ours fires within a substep of live's tick, and a fork that starts
loaded or not then differs).

**The robots deactivate (verified live; not modelled).** The coordinator's
lead: in `__001` every body of both robots shows `+0x1c` = 2 (active) except
for one substep (the chassis) or two (wheels, fork, rod) about every 24
substeps (0.4 s) whenever a robot is slow, at rest or creeping at up to
0.4 m/s and 0.6 rad/s on the chassis (wheels spinning at 1-4 rad/s): flag 0,
the body's centre unchanged from the substep before, its velocity kept, and
active again at the next substep (the robot's own actions or contacts wake
it). During the hold (written still) it happens too. So the original's
deactivator freezes a slow robot for a substep in every 24; ours never does.
It acts only on slow robots, so it cannot be what tipped ALARM (4-6 rad/s); a
10 s rest of both robots (`__001` trial 16) yaws the Scout +0.066 rad live and
+0.034 ours, ALARM -0.017 live and +0.029 ours (the rest creep of the award
scene's Atom Smasher has its sign flipped the same way). The criterion (and
whether the freeze is what turns the creep) is not recovered.

**Validation.** Before = `fidelity` 1b5ac27 built from a clean tree (it
reproduces §12r's after column to the digit), after = this branch.
`FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10` on the other
robot, robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 20.0 (139) sd 15 / 3.6, 94 s, 10/24 | 31.8 (111) sd 25 / 5.6, 85 s, 7/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 (2 of 13, §12j) | 15/24 | 8/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 5.7 (99) / 0.9, 58 s, 2/24 | 4.2 (75) / 0.9, 53 s, 1/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 71.2 (295) sd 36 / 3.9, 97 s, 16/24 | 73.0 (291) sd 33 / 4.0, 99 s, 14/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 16.5 (64) sd 17 / 2.6, 78 s, 5/24 | 18.7 (88) sd 16 / 3.5, 84 s, 8/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 18/24 | 12/24 |

ALARM's tips in box fights (`fightprobe`, 12 runs of 46 s, `up.y` through
0.5; live `fightcontacts-...__live__000..006`, 277 s): live 7, one to the
back, one roll; before 21, 7 to the back, 12 rolls (seven at 4.6-5.6 rad/s);
after 16 (8.0 per 277 s), 3 to the back, 4 rolls at 1.0-3.1 rad/s, all with
the Scout in contact. AI suite (ten pairings, warmup 0): 10/10 deal damage
before and after; the pairings with two robots' meshes meeting change
(box Scout v ALARM 13/45 → 4/12, Octagon 5/21 → 1/19, Bushido v Devil 19/7 →
31/18, Bear v Tornado 54/54 → 49/61, Grog v Boxer hill 12/18 → 13/23, Razor v
FlapJack 2/1 → 4/1, Sentinel v Wide Load 7/102 → 5/100), the other three are
the same to the hit. The box fight's ALARM hits move by about one standard
error toward live and its falls halve; the other ensembles stay within their
scatter.

**Still open.** The deactivator's criterion for robots (above); why ours
fires the fork's stroke a substep apart from live when the tick schedules
agree (the replay aligns the presses by the fork's onset); the award scene's
creep.

### 12t. The deactivators, and the contacts' order (fx/deactivate)

Live, every robot body goes inactive (`+0x1c` bit 1 clear) for a substep or two
about every 0.4 s whenever the robot is slow, and FlexTop's table freezes 3-15 s
after the robots leave it while it still swings (sections 12j, 12s). This round
recovers what does it, from the binary first and then with Frida, and puts it in
havok17 (`havok17/deactivation.rs`); and finds why a free robot at rest creeps
the other way round from live (the award scene's Atom Smasher): the order of the
contacts.

New live captures (two sessions on `:7`, the prefix's `AI/`, `Teams/`, `Robot
Designs/` and `ra2.cfg` sha256-identical before and after each; manifest rows in
`manifest-additions-2026-10-21-deactivate.tsv`): `deact-scout-alarm-box-rest__live__000`
(new agent `frida/ra2_deact.js` with `tools/oracle/capture_deact.py`; box Scout v
ALARM after `capture_tipab.py setup`: 15 s at rest, ALARM at throttle 20, rest,
the Scout at 30, rest, ALARM turning, rest; every substep's phase markers and
every robot body, every system step, every `CheckFreeze` with its references,
every `setActive` with its caller, every detector wake, every world-deactivator
call), `deact-scout-alarm-flextop-off__live__000..001` (the same agent, `NO_DRIVE=1`,
alongside `capture_flexfreeze.py` in a Tabletop FlexTop match: Game Settings
`TABLETOP` at (492, 390), FlexTop the arena list's second row, y 188) with their
drives `flexend-scout-alarm-flextop-off-deact__live__000..001`.

**Who deactivates (recovered).** The `include_deactivator`/`short_freq`/`long_freq`
strings belong to the `.gmf` writer only (`0x5c5584`); the game does not read
them. Two deactivators run, both once a frame (after the frame's second
substep; live the calls come between a frame's last `Q` and the next `T`):

* *The world's.* The world's constructor (`0x529700`) builds a deactivator
  (`0x53cad0(5, 0.1)`, vtable `0x6ac39c`) and adds it to its listeners
  (`0x529c70`); the game's world setup (`0x4ac2e0`) then sets both of its
  frequencies to 0.5 Hz (`0x4acab0`/`0x4acb00` with 0.5), so it checks every
  2 s (live `D` rows: `+0x20 = +0x24 = 2`, the timers stepping 102, 104, ...).
  Its step (`0x53cc00`): every entity of the subspace that is active and in no
  system is checked with the default thresholds and, when the check passes,
  `setActive(0, 1)`.
* *The systems'.* Every robot is a system (`0x5236e0` builds one through
  `0x533c80`, vtable `0x6ac2a8`, and scales its thresholds by 10, `0x533db0`),
  holding all its bodies (live: the Scout's four, ALARM's six; `+0x20` of each
  body counts its systems, `0x531e90`). FlexTop's table is one too, alone
  (live `G` rows of `deact-...-flextop-off__live__001`: thresholds 0.1, 0.05, 1,
  2). The system's step (`0x533fd0` → `0x534050`): while active, a short check
  every 0.2 s and a long one every 10 s (`+0x50`, `+0x5c`; due when strictly
  more has passed, so 7 frames, sometimes 6 or 8); when every body passes it
  sets all of them `setActive(0, 1)` and itself inactive, and skips its actions
  (a robot's list is empty live). While inactive it does nothing until one of
  its bodies is active again, then sets all of them `setActive(1, 1)`.

**The check** (`CheckFreeze`, `0x522cb0`): the first one only stores the
references (centre `+0x30` and quaternion `+0x40` into `+0x1fc` long and
`+0x220` short) and `r²`, the squared half diagonal of the body's collidables'
bounding box as it is then (`+0x244`; live 1.27 for the Scout's chassis, 1.47
ALARM's, 0.019 a wheel, 310.5 FlexTop's table). A short check passes when
`|Δc|² < t0²` and `|Δq|² r² < t1²` (`Δq` the four components' difference),
a long one with `t2`, `t3`; a passing short check returns at once, a failing
one takes the pose as its reference, then the long check likewise. Defaults
(`0x6abdf4`, `0x7021b4`): 0.01 m, 0.005, 0.1 m, 0.2; a robot's and the table's
0.1, 0.05, 1, 2. So a robot freezes when its chassis moved less than 0.1 m
and turned less than about 0.1 / r rad in the last 0.2 s.

**Who wakes.** `setActive(1, 1)` on a rigid body (`0x523230`) also writes
`FLT_MAX` into both references' centres, so the next checks fail; the
detector's wake (`0x548da0` → `0x548ef0`: an inactive movable body in a
detected pair) is `setActive(1, 0)` and keeps them. The game's force, torque
and impulse wrappers (`0x4b0860`, `0x4b0950`, `0x4b0a10`) wake the body first
(`0x45ebc0` reads bit 1): live (`A` rows, caller `0x4b0a38`, from the
MagicAction at `0x485b9f`) a robot at rest is frozen at a frame's end, its
traction push wakes the chassis in the next substep's actions, and its system
wakes the rest at the next frame's end (`0x5341cb`). A dashpot with one active
end wakes the other (`0x52cc3e`, `0x52cc65`); a hinge controller's settings
(`0x4815c0`) do not.

**What inactive means (live).** Not integrated: the centre stays to the bit,
the velocity is kept (the integrator's active list `+0x38` is rebuilt from
bit 1, `0x531240`), gravity is added only to listed bodies (`0x52eb30`). The
contacts detected before the freeze are still resolved in the next substep
and reach the inactive body (the wheels' velocities change by 0.8 m/s in that
substep, the floor holding them up); after that no pair between two inactive
bodies, or an inactive one and a fixed one, is detected (the broadphase keeps
active, inactive and fixed collidables in three lists and only pairs the
active ones, `0x559d30`). The constraint solver leaves a constraint alone while
one of its moving bodies is inactive: a frozen wheel's velocity does not
change while its chassis is active again, and the chassis falls freely for
that substep (`v_y` -0.2 m/s, one substep of gravity). So a robot at rest
loses one integration of its chassis and two of its other bodies every 12-14
frames (live intervals 0.400, 0.433, 0.467 s).

**Ours.** `Body::active`, `Freeze` (the references), one `System` per robot
(its collision group) and one for the arena's held bodies
(`PhysicsBackend::deactivation_system`, FlexTop's table; `Duel::suspended`
replaces `RestWatch`), the world deactivator for every other dynamic body.
The deactivators run at the start of every second substep (after the game's
pushes of the frame's last step, as live), on Havok's clock arithmetic
(`Clock`). Integration, the drag and the joints skip inactive bodies, the
detector pairs only active bodies and wakes an inactive one it meets (its
pairs then detected in the same detection, as `0x542db0`'s second run), a
dashpot wakes its other end, `add_force`/`add_torque`/the impulses/`set_velocity`
wake with the references forgotten. `RA2_H17_DEACTIVATION=0` turns it off,
`RA2_H17_DEACT_DEBUG` prints every check. `examples/deactprobe.rs`: robots left
alone in an arena, their freezes and drift.

| at rest, box Scout v ALARM, 15 s | live | ours |
|---|---|---|
| bodies per system | Scout 4, ALARM 6 | 4, 6 |
| chassis inactive, substeps per freeze | 1 | 1 |
| other bodies inactive | 2 | 2 |
| interval between freezes | 24-28 substeps (12-14 frames) | 24-28, mean 27 |

FlexTop's table (`deact-...-flextop-off__live__001`): frozen many times while
the robots stood still on it (short checks, |Δc| 0.003-0.005 m, |Δq| r
0.02-0.05) and woken by them each time; frozen for good 8.3 s after the last
robot fell off, by a long check (|Δq| r 0.35 against 2). Ours from the live
swinging state (`tests/flextop_live.rs`): frozen at the first long check, 10.0 s
(live delays 10.5, 3, 14.5 and 8.3 s).

**The creep's sign: the contacts' order (recovered, verified live).** With the
deactivator in, the award scene's Atom Smasher still turned +0.0155 rad/s
(live -0.0145): the deactivator takes about a tenth off the creep and does not
turn it. What does is the order the resolver meets the contacts in (one
impulse at a time, round-robin from the first, `0x559720`): reversing our list
turned the creep to -0.0167 rad/s with the robot drifting +x, as live. The
order is the broadphase's (`FUN_00559d30`, every detection): collidables are
kept in three lists, active, inactive and fixed, each sorted by its box's lower
bound along the sweep axis (`0x5411f0`: `+0x1d4 = (1, 0, 0)`, the world's x;
`0x559c20` projects, `0x559ec0` sorts); for every active collidable in that
order come its pairs with the active ones after it, then the inactive ones,
then the fixed ones (`0x55a750` keeps the overlapping ones and appends them to
`+0x58`), and the narrowphase appends each pair's points in that order
(`0x542db0`). Live at rest (`twobody-...__live__001`) the floor's contacts come
wheel by wheel in ascending x; in the fork trials (`twobody-...__live__000`
trial 0, substeps 3-8) the pair sequence is ALARM's chassis-fork, then ALARM's
wheels 1, 4, 2, the fork on the floor, ALARM's wheel 3, the Scout's wheels 2,
1, and ours now gives exactly that sequence (it was body creation order).
`Havok17Backend::broadphase_pairs`. Atom Smasher now turns **-0.0145 rad/s**
and drifts +0.30 m in x in 50 s (live -0.0145, +0.3 m in 56 s); C.R.U.S.H.
-0.017 rad in 25 s (live ±0.1 in 50 s). No sign or handedness error was
needed. The pairs' A/B orientation (`0x55a750`: `+0x18`, then `+0x50`) is not
modelled: it flips a normal, which the resolver's impulse does not see.

The rest creeps of the box robots stay chaotic: live (15 s from spawn) Scout
+0.044, ALARM -0.029 rad; ours -0.014, -0.037; the twobody rest (trial 16)
ends within 0.05 and 0.07 m of live for the two chassis.

**Validation.** Before = `fidelity` 97bef36 built from a clean tree (it
reproduces section 12s's after column to the digit), after = this branch.
`FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10` on the other
robot, robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 31.8 (111) sd 25 / 5.6, 85 s, 7/24 | 30.8 (130) sd 23 / 5.5, 76 s, 7/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 8/24 | 8/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 4.2 (75) / 0.9, 53 s, 1/24 | 4.5 (146) / 0.6, 63 s, 5/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 73.0 (291) sd 33 / 4.0, 99 s, 14/24 | 79.0 (275) sd 53 / 4.6, 88 s, 12/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 18.7 (88) sd 16 / 3.5, 84 s, 8/24 | 23.5 (123) sd 23 / 3.5, 93 s, 9/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 12/24 | 12/24 |

Section 12s's replays of the live placements: set 1 (20 trials) ALARM on its
back at some point / at the end 3 / 2 before, 6 / 6 after (live 1 / 1; with
the deactivator off as well, 6 on its back: the contacts' order, not the
freezes; the trials that go over differ), mean peak |ω| 3.8 → 3.6 rad/s (live
3.0), both chassis' end centres from live, median 1.39 → 1.46 m; set 2 (17)
never on the back in either, median 0.27 → 0.24 m.

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after. Every
pairing changes (the contacts' order changes every fight); two end early:
Tornado v RipBlade at 22 s (Tornado stuck against the wall near its start and
taken by the immobility rule, as in 2 of 9 warmups after, 0 of 9 before; with
the deactivator off too, so it is the order) and Sentinel v Wide Load at 58 s.
The ensembles move within about one standard error (Octagon's ALARM 18.7 →
23.5 with sd 16-23, away from live's 13.6; the Compressor's Wide Load 73 → 79,
live 73.6).

**Still open.** The narrowphase's point order within a pair (the convex x mesh
manifold's, live keeps its old points first), the pairs' A/B orientation, the
broadphase's insertion order of the woken pairs (ours re-sorts them), which
other arena objects get systems (the box's arena has an empty system with
thresholds times 100); whether the original's hazards ever freeze with their
motors on (a hinge controller's settings do not wake a body).

### 12u. The contacts' order, the rest of it (fx/contact-order)

Section 12t put the broadphase's pair order in and three ensembles moved away
from live (Octagon ALARM 18.7 → 23.5, live 13.6; the §12s set-1 replays 3/2 →
6/6 on the back, live 1/1; Tornado v RipBlade stuck on a wall in 2 of 9
warmups). This round recovers the rest of the order from the binary, checks
each part live with Frida, and compares the contact list substep by substep
over whole fights with every body where live had it.

New live captures (three sessions on `:7`, the prefix's `AI/`, `Teams/`,
`Robot Designs/` and `ra2.cfg` sha256-identical before and after each; the
first, 40 s, lost its agent at the first broadphase call and is not kept;
manifest rows in `manifest-additions-2026-10-21-contact-order.tsv`):
`ord-scout-alarm-box__live__000` and `__001` (new agent `frida/ra2_order.js`,
run with `ra2_hook.py`; box Scout v ALARM with the shipped AIs, hazards off,
40 s and 25 s): every collidable in the broadphase (`C`: shape type `+0x18`,
`+0x50`, body, geometry, the OBB tree's leaves depth first), the projected lists
(`L`), every pair offered (`P`, `0x55a750`), every MOPP query's result (`Q`,
`0x566a80`), every resolver entry (`R`, `0x53ed80`) and island (`I`, `0x559720`),
and in `__001` every 50th call each listed collidable's box with its body (`X`).

**The tool.** `examples/orderprobe.rs` (`PhysicsBackend::detect_now`, a
diagnostic: one detection where the bodies are) places every body of both
robots at its live pose at every resolver entry of a capture (the
`fightcontacts-...` `R`/`B` rows: 2000-3800 entries a fight) and runs our
detection, so our manifolds age with live's poses; `tools/oracle/twobody/
ordercmp.py` compares the two lists entry by entry (pair set, pair order, A/B
orientation, point count, point order). The original's body frames are not
ours for the wheels, the fork and the rod (a fixed turn, up to 2.1 rad); the
probe takes each body's turn relative to its chassis from the live rest poses
(`twobody-...__live__000` `REST`), which leaves a wheel's spin about its axle
a few hundredths off. `seqcmp.py` does the same for the §12s replays (their
first divergence per trial).

**(c) The broadphase's lists (recovered, verified live).** There is no
insertion order: every detection the active list is projected afresh from the
active collidables (`0x559c20`) and sorted (`0x559ec0`: insertion sort up to 16
entries, quicksort above), the inactive list likewise whenever a body changed
state (flag `+0x1d1`), and the fixed list is projected when it is built. So a
woken or new pair falls where its box puts it. What §12t missed is the box:
live (`X` rows) a moving collidable's box is a cube, the same size whatever the
body's attitude, of half side `r + tol/2` about a point fixed in its body, `r`
the smallest sphere around the collidable's vertices (Welzl; every one of 41
moving collidables of both robots to 0.0002 m, the Scout's chassis 1.0377 m,
ALARM's 1.1072 m, more than half its longest span 1.100 m); a fixed one's is
its box plus `tol/2`. Ours sorted moving collidables by their turning box, so
the Scout's two wheels swapped places whenever it turned. Now
`narrow::bounding_sphere`, `Piece::sphere`, `WorldPiece::sweep`.

**(b) A and B (recovered, verified live).** `0x55a750` puts first the
collidable with the greater shape type (`+0x18`: convex `0x400` before mesh `4`)
and between equal types the greater `+0x50`, which is the collidables' creation
count (live 0 for the arena, 1-6 its props, then the robots' pieces in the
order the game builds them: ALARM's after the Scout's). The convex x mesh agent
(`0x545b50`) keeps the convex piece as A; a fixed mesh against a moving one goes
to `0x54a2a0` with the fixed (MOPP) mesh as A whatever the pair said; two moving
meshes keep the pair's order (`0x549d00`). Ours put the moving body first and
the lower handle first (ALARM's chassis was B against the Scout's in 573 of 611
live entries); now A is the greater (body handle, piece), which is the creation
order for the arena and the two robots, and the agents swap as the original's.
The resolver does not see it: flipping A, B and the normal negates `u`, `n` and
the impulse exactly, `K_A + K_B` is the same sum, so the velocities come out
bit for bit the same. It matters to the mesh agents, whose walks start from A.

**(a) The order within a pair (recovered, verified live).** The convex x mesh
manifold (`0x545b50`) is ours already: the kept points are re-measured from the
last, a dropped one replaced by the last and the list shortened; each triangle's
point that persists replaces the first kept point within the tolerance whose
normal agrees to `0.8`, else is appended; one that does not persist is reported
at once; the kept points follow, in their order. Live keeps old points first
because new ones are appended. What decides the order is the triangles': the
agent meets them in the order its query returns them, and ours were ascending
by index. A moving mesh's query walks Havok's OBB tree (`+0x1c8`, a struct whose
first word is the root) depth first, first child first (`0x550560`; the same in
`0x54a080` and `0x550100`). The tree (`0x53a700`, built over the triangles in
index order, `0x5371c0`) splits a set of more than one triangle by its
triangles' centroids: their mean and variance per axis of the geometry's frame
(`0x53ac00`, single precision), the axis of the largest variance (x only when
strictly the largest, then y, else z), centroids below the mean to the first
child, the rest to the second, each in order, and when a side is empty the
other's last triangle moves over. Every node also gets a box from the
triangles' area-weighted covariance (`0x557320` → `0x680650`, its eigenvectors
`0x556e70`, the extents `0x556fc0`). Ours (`havok17/obbtree.rs`) gives the live
leaf order of all three moving meshes exactly (the Scout's chassis, 20
triangles, its fork arm, 20, ALARM's chassis, 12). The geometry's frame is the
component's own, not the body's: the fork arm's vertices live are ours turned
(x = -y, y = -z, z = x) and moved, so `ShapeDesc::MeshHull` now carries `raw`,
the vertices before the component is placed (`robot.rs`), and the tree splits
there.

Two moving meshes are walked together (`0x550100`): at each pair of nodes the
one with the bigger box (`8 hx hy hz`) is split, A's when B's is a leaf and B's
when A's is, first child first (`ObbTree::pairs`). A fixed mesh against a moving
one (`0x54a2a0` → `0x54a080`): B's tree depth first, and for each of its
triangles A's MOPP's triangles near it. The MOPP is compiled at load
(`0x536a10`, Havok's MOPP compiler, `0x565xxx`), and its order is not
recovered: live (`Q` rows of `__000`, 184 000 queries of the box arena's 12 triangles) every
query returns its triangles in one fixed order (10, 0, 8, 9, 1, 6, 4, 7, with
11 somewhere after 0), which is neither the index order nor ours; ours keeps the
ascending order for a fixed mesh.

**(d) The resolver's walk (verified live).** `0x559720` as section 10 has it:
the cursor starts at the island's first contact, moves past the contact it
pushes, a lap without an approaching contact ends the island, 400 impulses at
most, inelastic after 100; ours matches. The islands keep the detection's order
(`0x5589f0` copies an island's contacts by ascending index): live 13 784 of 13 791
islands are a subsequence of their resolver entry's list (the 7 others hold the
same point twice).

**Measured with every body at its live pose** (`orderprobe`, fight
`fightcontacts-scout-alarm-box__live__000`, 3780 resolver entries; before = this
round's start; entries where both lists hold the same pairs):

| | before | after |
|---|---|---|
| pair order the same / different | 1243 / 75 | 1628 / 2 |
| A/B orientation different (pairs) | 1625 | 3 |
| Scout chassis x floor: point order same / different | 127 / 464 | 815 / 0 |
| chassis x chassis: point order same / different | 55 / 34 | 129 / 0 |
| fork x floor: point order same / different | 33 / 50 | 51 / 0 |
| wheels x floor: point order same / different | 451 / 122 | 514 / 124 |

The other five fights with the robots mapped (`__001`-`__004`, `__006`; in
`__005` the probe cannot place ALARM) give 0-6 different pair orders each (of 1919-2498 entries). What
is left differs in the wheels (the MOPP's order and, in this probe, the
wheels' spin): the wheels' point counts differ in a third of the entries, live
with two points where ours has one twice as often as the other way round
(2383 against 1245). In the §12s replays the first substep where the pair
sequence differs over the same pairs moves from 3-161 in 9 of 20 trials to 3-85
in 6, every one after the robots' states have parted (trial 13, substep 3: ALARM's
wheel 0.1 m from live's), and the orientation no longer differs anywhere (it
did in all 20 from substep 2-24).

**Validation.** Before = `fidelity` 10e7ab4 (the pre-built binary; it
reproduces section 12t's after column to the digit), after = this branch.
`FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10` on the other robot,
robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 30.8 (130) sd 23 / 5.5, 76 s, 7/24 | 35.3 (154) sd 30 / 5.2, 86 s, 5/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 8/24 | 12/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 4.5 (146) / 0.6, 63 s, 5/24 | 3.9 (112) / 0.9, 54 s, 2/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 79.0 (275) sd 53 / 4.6, 88 s, 12/24 | 81.2 (258) sd 43 / 4.8, 95 s, 12/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 23.5 (123) sd 23 / 3.5, 93 s, 9/24 | 22.5 (112) sd 22 / 3.3, 86 s, 8/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 12/24 | 10/24 |
| box Tornado v RipBlade, 9 warmups (0, 7, ... 50): stuck at 22 s | | 2 of 9 (0, 26) | 1 of 9 (0) |

Section 12s's replays of the live placements: set 1 (20 trials) ALARM on its
back at some point / at the end 6 / 6 before, 5 / 4 after (live 1 / 1), mean
peak |ω| 3.6 → 3.2 rad/s (live 3.0), both chassis' end centres from live,
median 1.46 → 1.32 m; set 2 (17) never on the back, median 0.24 → 0.35 m
(worse).

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after; every
pairing changes, Tornado v RipBlade still ends at 22 s (Tornado stuck against
the wall near its start; the walls are a fixed mesh, whose MOPP order is
still ours). Every ensemble moved within about one standard error; the
contact order, now matching live in the probe almost everywhere it can be
compared, is not what keeps the Octagon's ALARM count (22.5, live 13.6) and
the box's falls (12/24, live about 5 of 19) away from live. The box falls went
from 8 to 12 of 24 with the order now right.

**Still open.** The MOPP's order (a fixed mesh's triangles: the arena's floor
and walls; the compiler is Havok's own, run at load); the wheels' point
counts against the floor (live has two points where ours has one about twice as
often as the reverse; the probe's wheel spin is approximate, so this needs a
controlled check); the original's body frames for hinged parts (the fork, the
rod, the wheels are turned against ours by a fixed rotation: harmless to the
physics, which is frame-free, but the probe has to correct for it).

### 12v. The MOPP's order, the arena's bodies one by one, the spawn and the hold (fx/wall-mopp)

Section 12u left a fixed mesh's triangles in ascending order (the arena's floor
and walls: Havok compiles them into a MOPP at load and the queries return them
in the MOPP's order) and the wheels' point counts against the floor open. A
pose-matched visual audit (docs/34 §11.4) had the robots resting at other yaws
than live before any fight (ours minus live: M.A.D. -0.36 rad, Berserker +0.21,
Raptor +0.17, Dementia -0.15). This round recovers the MOPP compiler, splits the
arena collider into the original's bodies, and recovers how a match places and
holds the robots.

New live captures (four sessions on `:7`, the prefix's `AI/`, `Teams/`, `Robot
Designs/` and `ra2.cfg` sha256-identical before and after each; one more session
crashed the game 24 s into a match and is not kept; manifest rows in
`manifest-additions-2026-10-21-wall-mopp.tsv`):
`mopp-scout-alarm-arenas__live__000` (new agent `frida/ra2_mopp.js` through the
new `tools/oracle/capture_mopp.py`: the eight arenas of the Arena list loaded in
turn; every fixed mesh's geometry as it is compiled (`G`, `0x536a10`), the
primitive list the tree builder starts from (`I`), the built tree node by node
with the raw node memory (`T`), and every MOPP query's triangles (`Q`, 553 000
queries)), `fightcontacts-scout-alarm-octagon__live__000` (`ra2_fightcontacts.js`,
40.8 s of a Scout v ALARM AI fight in the Octagon), `contacts-{scout,alarm}-straightturn__live__000`
(`capture_contacts.py PLAN=straightturn`: from the spawn pose, 2 s straight at
full throttle, 3 s turning in place, 2 s arcing), and for seven pairs of stock
robots (EMERGENCY v M.A.D., Sentinel v Berserker, REVENGE v Raptor, Dementia v
Little Metal Friend, Stinger v Tornado, MiniBerg v Iceberg, Scout v ALARM;
Combat Arena, hazards off, AIs neutered while the intro is up; new
`tools/oracle/capture_rest.py`) `rest-<pair>-box__live__000..002` (`ra2_deact.js`:
every body of both robots every substep from the hold through the bell to 15 s
of rest), `spawnwatch-<pair>-box__live__000..001` (new `frida/ra2_spawnwatch.js`:
every body moved outside the integrator, with its callers) and
`spawnq-<pair>-box__live__000` (new `frida/ra2_spawnq.js`: the start-point
lowering's contact test, every query and its results).

**The MOPP compiler (recovered, verified live).** `0x536a10` compiles a fixed
mesh's geometry with Havok's MOPP compiler (`0x565fd0`): a mediator serves the
triangles in index order (`0x565a00`, `0x536d60`), the tree builder
(`0x56a770` → `0x56a150`) builds a binary tree with one triangle per leaf, and the
assembler flattens it into byte code. The query (`0x566a80` → `0x5661a0`, the
same VM as `0x566ad0`) walks the code depth first and visits the lower side of
every split first, so whatever box is asked for the triangles come back in the
tree's leaf order, the lower child's leaves before the upper child's. What the
builder does (`havok17::mopp`, written from this account):

* There are 13 splitting directions with costs (`0x5673e0`, table `0x71cef8`):
  the axes (cost 0), the six face diagonals (0.2, then 0.25 for (0,1,-1),
  (1,0,-1), (1,-1,0)), (1,1,1) at 0.3, (1,1,-1) and (1,-1,-1) at 0.32 and
  (1,-1,-1) again at 0.34 (the table repeats it where (1,-1,1) would be).
* A node first tries the three axes, the longest of its parent's extents first
  (an extent the parent was split along counts 0.66 of its length; the other two
  by length), each needing room for `0.8 (1 - len/longest)^3` (`0x569fe0`); once
  the best cost so far is below an axis's cost plus that room the remaining axes
  only get their extents measured. Then the diagonals in table order until the
  best cost is below a direction's own cost.
* A direction is tried by projecting every triangle on it (its corners'
  `[min, max]`, `(x nx + y ny) + z nz`, `0x565680`), sorting by `min` with
  Havok's quicksort (`0x56a8a0`: middle pivot, Hoare scan, not stable) and
  sweeping (`0x569840`): before triangle `i` is weighed the open triangles (those
  before it, kept by decreasing `max`, at most 51) whose `max` lies below its
  `min` are closed. Triangle `i`'s `min` is the upper child's lower bound; the
  lower child's upper bound is the `max` of open triangle `j` (so `j` triangles
  would straddle) or of the last closed one when none is open; `j` is tried in a
  window of two either way of the last best (`0x569670`). The candidate's cost is
  the direction's cost (less 0.03 while the triangle numbers swept so far span
  less than 16) plus penalties for a split off the node's centre on either side
  (`2.9 x^2 + 3 x^4 + 1500 x^8`, half a node allowed off centre near the root,
  `0x56aa40`), for unbalanced counts (`((|i - n_upper|/n - 0.75) 6)^4` near the
  root), for straddlers (`5 j / n`) and for the overlap of the two children
  relative to the node's extent (`0x56ab10`). The cheapest candidate wins, the
  first of equals.
* The triangles are dealt out (`0x5699f0`): below the upper child's bound and
  within the lower child's go lower, the reverse upper, those that fit both are
  sorted along the node's longest axis other than the split's and dealt to keep
  a 4:1 balance; a triangle fitting neither would be cut in two (no stock
  arena's tree has one; ours sends it upper). The original moves the entries
  in its buffer in a particular way, and that decides which of two triangles
  with equal `min` the unstable sort meets first further down: ours follows the
  moves. The upper child is built first, the lower one is walked first.
* The settings are the game's (`0x536a10`: 50 straddlers weighed, a window of
  two, room for 1000 cut pieces, cost weights 1 and 0.5), and the arithmetic is
  single precision: the game runs with the x87 unit at 24 bits. With 53- or
  64-bit intermediates the Metal Skull's root split falls one ulp the other way
  (two candidates whose costs round to the same float in single precision) and
  its tree differs; in single precision every tree is the original's.

Checked against every tree the original built for the eight arenas (21 fixed
meshes, 12 to 168 triangles): every one is the original's leaf for leaf, and
every one of the 553 000 live queries returns its triangles in that order (the
252 distinct multi-triangle answers are all subsequences of it). The geometry
is the body's own (its `.gmf` points in the body's frame, in face order); the
original's triangles are ours to 2 mm. Tests
`havok17::tests::the_combat_arenas_mopp_returns_its_triangles_in_the_live_order`
and `tests/mopp_live.rs` (all 21, against `tests/data/mopp_live.tsv`).

**The arena's fixed bodies are collidables of their own (recovered, verified
live).** The original makes each fixed rigid body of the arena its own
collidable, and compiles a MOPP for exactly those with `SIMULATION_GEOMETRY` 5
(live it compiled the Octagon's `main_collision` and `outerfloor_collision` and
not its `hammerplate`s, the Half Circle's `Collision Box` and `Hammer Stop Zone`
and not its six spike plates, the Parking Lot's scaffolds and not its barrels;
those are `SIMULATION_GEOMETRY` 2, collided as convex hulls like a robot part's,
§12p). Ours merged every fixed body into one mesh. The arena is now one static
body with a child per fixed body (`ArenaScene::colliders`, `collider_shape`): a
mesh for 5 (and 6), the convex hull of its points for 2 and 3, the two large
triangles of a plane; each mesh orders its triangles by its MOPP
(`narrow::Mesh::mopp`, used by the convex x mesh agent and by the fixed mesh x
moving mesh walk). `RA2_ARENA_MERGED=1` gives the merged mesh back and
`RA2_H17_MOPP=0` the ascending order (diagnostics). The bodies still share the
strongest friction of them (open).

Measured with every body at its live pose (`orderprobe`; `ordercmp.py` has a new
`-r` that leaves out the pairs of two unmapped bodies, the arena with a hazard,
which the probe does not place; `orderprobe` now takes a part only within
0.5 m of where its chassis carries it, since the Scout's and ALARM's wheels both
weigh 27 kg):

| fight | before: point order same / different | after |
|---|---|---|
| Octagon (`fightcontacts-scout-alarm-octagon__live__000`, 2511 entries) | 1033 / 83 | 1114 / 3 |
| ... Scout chassis x floor | 257 / 75 | 332 / 0 |
| box `__000`, `__004`, `__006` | 1554 / 126, 3649 / 82, 833 / 39 | the same |

In the box the MOPP's order (10, 0, 11, 8, 9, 3, 1, 6, 5, 2, 4, 7: the floor is
0 and 1, the -x wall 10 and 11, the +z wall 8 and 9) differs from the ascending
one only where a piece is within the tolerance of the floor and the -x or +z
wall at once, or of two walls; the three fights give the same lists either way.

**The wheels' point counts against the floor (the controlled check).** In
`contacts-{scout,alarm}-straightturn__live__000` the counts differ as in the
fights, and as much at rest as when driving: ALARM, standing still while the
Scout drives, has two points on a wheel where ours has one in 1050 of about
2400 wheel entries, three where ours has one in 293 more, and ours two where
live has one in 61. The points are on the wheel
hubs 0.09-0.10 m from the floor (the robot stands on other pieces), at the
tolerance's edge, so a few millimetres decide whether a second point is kept;
and for some wheels live's points lie up to 0.14 m from ours along the floor
(ALARM's second wheel) where the first wheel's agree to 1 cm. The original turns
some wheels about their axles otherwise than ours (the hub is an octagon prism):
the same shows at the spawn, where Sentinel's four wheels stopped the lowering
at 0.051, 0.073, 0.079 and 0.088 m live and all four sit alike in ours. Which
turn the original gives each wheel is not recovered.

**Where a match puts the robots (recovered, verified live).** The robots are
made at the arena's origin and, when the countdown begins (`0x415bd3`,
`0x416126`), each is set at its start point (`0x4dcff0`: the start point's turn,
then its position, `0x4dced0` moving every body by the difference, `0x4a6f70`)
and lowered (`FUN_00418b90`): every body moves down 0.05 m (`0x4dce70`), and for
each body of the robot a collision query (`0x4adfd0` → `0x542a70`: the
broadphase, then the agents of every pair with the body) is asked whether it
finds anything (`0x542a70`'s pair and contact lists, the detector's list at
`+0xe0`); at the first yes the robot moves back up 0.05 m and the loop ends (at
most 100 moves). Live (`spawnq-*`) only the contact records ever answer, and the
record that stops it is a wheel's at the distance its traction phantom is from
the floor (M.A.D. 0.0859 m live and 0.0847 ours, the Scout 0.0962 and 0.0955,
ALARM 0.0978 and 0.0956, Berserker 0.0982 and 0.0956, Iceberg 0.079-0.081 and
0.0788): the phantoms count in this test though they hold nothing up in a step.
The robot then stays where it is to the bit, with no velocity, the chassis
flagged `+0x1c` = 4 and its other bodies inactive, until the bell, and falls the
last 0.15-0.30 m onto its wheels as the fight begins (`rest-*`). Ours
(`duel::lower_onto_floor`, `PhysicsBackend::spawn_touch`, the phantoms tested
within the tolerance as well; `Duel::hold_robots`, held as fixed bodies by the
match until the bell and let go with no velocity) put the robots at a constant
0.3975 m over the floor and let them fall at once, so they had landed and
settled during the countdown. `RA2_SPAWN_FLOOR=1` gives the old placement back.

| chassis centre held through the countdown (box) | live | before | after |
|---|---|---|---|
| Scout, ALARM, Razor, FlapJack, M.A.D., Berserker, REVENGE | | the same as live | the same |
| Little Metal Friend / Stinger / Iceberg | -1.6962 / -1.6939 / -1.4942 | +0.050 | the same |
| EMERGENCY / Sentinel / Raptor / Dementia | -1.5614 / -1.5779 / -1.5672 / -1.6459 | +0.050 / -0.050 / -0.050 / +0.050 | -0.050 each |
| Tornado / MiniBerg | -1.6018 / -1.6253 | +0.050 | +0.050 |

Ten of the sixteen robots now start where live has them (seven before); the
six others are one step off, the wheels' turn above.

**The resting yaws.** The audit's column compares ours 1.5 s after the bell,
having landed during the countdown, with live's shots taken 2 to 14 s after the
bell, while the robots creep: live they creep reproducibly (M.A.D. +0.165 and
+0.170 rad 9 s after the bell in two runs, at about +0.018 rad/s; EMERGENCY
-0.017 and -0.027), so most of M.A.D.'s -0.36 is the time of the shots. With
the AIs neutered before the bell no weapon turns live (Berserker's, Tornado's
and Iceberg's weapon bodies at most 0.4-0.7 rad/s), so the large turns of
Tornado and Iceberg in the audit came from their weapons running there, not
from the rest. Measured from the release, live against ours (`deactprobe
TRACE=0.5`, the chassis' yaw change, rad), the creep is of the same size and
mostly the same sense, but not the same robot by robot, and the new spawn
heights do not bring it closer:

| yaw change at 1 / 4 / 9 s | live | before (old spawn) | after |
|---|---|---|---|
| M.A.D. | +0.051 / +0.082 / +0.165 | +0.038 / +0.065 / +0.119 | +0.030 / +0.053 / +0.106 |
| Raptor | -0.195 / -0.253 / -0.284 | -0.016 / -0.006 / +0.005 | -0.088 / -0.035 / -0.043 |
| REVENGE | +0.112 / +0.146 / +0.169 | -0.005 / +0.039 / +0.074 | -0.022 / +0.029 / +0.067 |
| Dementia | -0.015 / -0.034 / -0.118 | -0.090 / -0.103 / -0.140 | -0.001 / +0.024 / +0.028 |
| Berserker | -0.023 / -0.125 / +0.025 | +0.022 / +0.010 / +0.010 | +0.054 / +0.101 / +0.094 |
| mean of \|ours - live\| over the 14 robots | | 0.055 / 0.062 / 0.063 | 0.056 / 0.070 / 0.071 |

Live, the robots with the largest landing yaw (Raptor -0.14 rad within 0.5 s,
REVENGE +0.10, Stinger +0.11) bounce: Raptor's chassis falls 0.30 m, comes back
up 0.15 m and lands again, with its wheels spun to 4-10 rad/s at the impact;
ours (0.05 m lower then, 0.21 m drop) came back up 0.08 m. The wheels' turn about
their axles (above) changes how a polygonal wheel meets the floor, and is the
first suspect for the rest of it.

**Ensembles.** `fidtrace`'s and `fightprobe`'s `FIDTRACE_WARMUP` ran idle steps
before the match; with the robots held until the bell they would not move and
every run would be the same fight (all 24 box runs ended with the Scout out at
28-32 s). The warmup now lets the robots go for its steps and holds them where
they come to rest; the warmup-0 run falls at the bell as live.

**Validation.** Before = `fidelity` f1b84b9 built from a clean tree (it
reproduces section 12u's after column to the digit); MOPP = this branch with the
MOPP's order and the arena's bodies only (no spawn or hold change); after = this
branch. `FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10` on the
other robot, robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | MOPP | after |
|---|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 35.3 (154) sd 30 / 5.2, 86 s, 5/24 | 38.2 (142) sd 28 / 6.0, 82 s, 2/24 | 32.2 (118) sd 26 / 5.5, 80 s, 5/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 12/24 | 11/24 | 10/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 3.9 (112) / 0.9, 54 s, 2/24 | 4.2 (107) / 1.0, 52 s, 0/24 | 4.2 (112) / 0.8, 52 s, 2/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 81.2 (258) sd 43 / 4.8, 95 s, 12/24 | 82.3 (320) sd 45 / 4.7, 102 s, 16/24 | 72.5 (309) sd 33 / 4.1, 103 s, 13/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 22.5 (112) sd 22 / 3.3, 86 s, 8/24 | 19.0 (93) sd 20 / 3.3, 78 s, 4/24 | 20.5 (109) sd 21 / 3.7, 93 s, 10/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 10/24 | 13/24 | 10/24 |
| box Tornado v RipBlade, 9 warmups: stuck at 22 s / full length | | 1 of 9 (0) / 7 | 1 of 9 (0) / 7 | 0 of 9 / 9 |

Section 12s's replays of the live placements (`twoprobe`): set 1 (20 trials)
ALARM on its back at some point / at the end 5 / 4 before and with the MOPP's
order (the same to the digit: the replays stay clear of the walls), 4 / 3 after
(live 1 / 1), mean peak |ω| 3.2 → 3.9 rad/s (live 3.1), both chassis' end
centres from live, median 1.32 → 1.45 m; set 2 (17) never on the back, median
0.35 → 0.33 m. The after column moves because `twoprobe` settles the robots
from their spawn for 60 substeps before it learns their rest frames, and they
now start at other heights; the trials are chaotic within a few substeps.

AI suite (ten pairings, warmup 0): 10/10 deal damage before and after; every
pairing changes (the robots now fall at the bell). Tornado v RipBlade no longer
sticks on the wall near its start: it ran the full 120 s (124 / 38 counted; it
ended at 22 s before and with the MOPP's order alone). The box fight's falls go
12 → 10 of 24 (live about 5 of 19) and the Compressor's Wide Load 81.2 → 72.5
(live 73.6); the Octagon's ALARM count stays high (22.5 → 20.5, live 13.6). All
of these are within about one standard error.

**Still open.** Which turn about its axle the original gives each wheel (the
spawn heights of six robots, the wheels' point counts, and likely the landing
yaws); the fixed bodies' own frictions (ours still takes the strongest);
whether the phantoms count in the spawn test because of the query or because
the wheels' traction collidables are not flagged as phantoms there (`+0x58` read
0 on the stopping record); a primitive the MOPP compiler would cut in two (none
in the stock arenas).

### 12w. How a match turns a robot onto its start point; the hinge's rows and anchor (fx/wheel-axle)

Section 12v left open which turn about its axle the original gives each wheel:
the spawn heights of six robots, the wheels' point counts against the floor and
probably the landing bounces and yaws hang on it. This round finds that the
turn is not built in: the match makes it, when it puts the robot on its start
point, and the original's hinge constraint decides it.

New live captures (three sessions on `:7`, the prefix's `AI/`, `Teams/`,
`Robot Designs/` and `ra2.cfg` sha256-identical before and after each; manifest
rows in `manifest-additions-2026-10-21-wheel-axle.tsv`): new agent
`frida/ra2_spinup.js` loaded before Start Game through `capture_rest.py`
(`WATCH_AGENT=ra2_spinup.js`): from the match load every robot body every
substep (`B`), each robot system (`Y`, in the later sessions with each body's
local inertia and first `0x200` bytes), the start-point placement (`P`,
`0x4dcff0`) and the lowering (`L`, `0x418b90`), and every hinge set-up's
motor/limit state (`H`) and atom (`J`, the first `0xa0` bytes, every call while
a robot is being placed); each followed by the usual `ra2_deact.js` rows from
the bell (`spinup-deact-*`). `spinup-scout-alarm-box__live__000..001`, and
`__000` of `sentinel-berserker`, `revenge-raptor` (and `__001`),
`stinger-tornado`, `emergency-mad`, `dementia-lmf`, `miniberg-iceberg` and
`razor-flapjack` (and `__001`). `tools/oracle/spincmp.py` takes each robot's
bodies from such a capture (as built, the substep before the placement, and as
held, the substep after the lowering) and prints every body's turn in the
robot's frame; `examples/spinprobe.rs` prints ours the same way (and with
`LAND=` every body's height through the landing).

**The placement (recovered, verified live).** A robot is built at the arena's
origin, frozen (chassis `+0x1c` = 4, the others 0). When the countdown begins
`0x4dcff0` turns the *chassis body* to the start point's heading about its own
centre and moves it there (`0x4dcfb0` → `0x4b0af0`, then `0x4dced0`), and moves
every other body of the robot by the chassis' displacement only: each keeps its
built orientation and its unturned offset (`spawnwatch-*` rows 29-34: the
Scout's chassis turned 135°, its wheels moved 9.8 m with their quaternions
unchanged). Then `0x4dee20(1.0, 1/30)` steps the world to one second in thirtieths
(58 substeps live in every capture), and only then is the robot lowered
(`0x418b90`, §12v) and held. In those steps the chassis stays fixed and, for the
first two substeps, inactive (`+0x1c` 4 → 6 at its system's end-of-frame wake),
so the other bodies fall freely for two substeps; then the joints pull them
round onto their hinges in two or three substeps (the Scout's wheel at 8.5 m/s,
40-60 rad/s about the vertical), and a wheel comes out of it spinning about its
axle (0.6 rad/s on the Scout's left wheel, 6.9 on ALARM's front left, 21 on two
of REVENGE's), slowing by under 1 % a substep, until the robot freezes or the
lowering zeroes it. The angle it has turned by then is the turn §12v was after:
live the Scout's wheels 14.5° and 2.1°, ALARM's 66.8°, 10.6°, 85.7°, 10.5° —
the angles `assembly-*__live__000`'s `first` scene showed in section 8 ("the
rotors have already turned by the first physics step"). Nothing holds a hinge
meanwhile: the spin motors' hinges have their motor off and no limit (`H` rows),
the burst motor's hinge is limited to `[0, 0]` from its first set-up and ALARM's
burst piston rod snaps back to its rest point.

Ours (`duel::turn_into_place`): the robot is spawned turned, its other bodies
are put back unturned at the chassis' displacement, the chassis is made fixed and
inactive (`PhysicsBackend::set_active`, new) for two substeps and active for the
remaining 56, with every other body of the world kept as it was, and the robot
is then lowered as before. The parked joints' friction (ours) is off for these
steps (`Robot::joint_friction`); a burst device's joint is locked where it is
built (`Robot::spawn`, as its first tick already did). `RA2_SPAWN_RIGID=1` gives
the old rigid placement back; `RA2_SPAWN_SETTLE0/1` the substep counts.

**The hinge's angular rows (recovered from `0x535a50`, verified live).** The
block's rows 3 and 4 turn about A's own perpendiculars, its reference `u1`
(atom `+0x58` in A's frame) and `u2 = u1 × a`, and their errors are B's axis
along them: `u2 · b` for `u1`, `-(u1 · b)` for `u2` (`0x535940` dot products of
`0x535b8c`'s vectors, stored `·1/dt` at `+0xc`, `+0x10`). To first order this is
what ours had (the gradient of `a · b_k` over B's perpendiculars), but far from
aligned the two part: from 135° out ours settled on the reversed axis, the
original comes back the short way (`joints::tests::a_hinge_turned_most_of_a_half_turn_comes_back_the_short_way`).

**The hinge's anchor (recovered, verified live).** A spin motor's constraint
holds the bodies at its `.gmf` `Hinge01` pivot, which lies on the mount point's
line but not at it: the Reversible Angle Motor's 0.156 m in from its `axle_m`
point, inside the motor, the mirrored model's 0.019 m out. Along a free axle the
anchor does not matter, but it sets the lever of every pull across it, and so
the spin a wheel takes from the placement: with the mount point the kick that
aligns ALARM's front left wheel gave it 0.1 rad/s about its axle, with the
hinge's pivot 3.0 (live 3.1), and its velocities in the next substeps follow live
to 1-4 % (`(-15.46, 0.12, -32.44)` m/s and `(3.0, -42.4, 3.0)` rad/s against
live `(-15.46, 0.08, -32.73)`, `(3.1, -42.4, 3.1)`). `JointInfo::anchor` (only
the constraint's anchors use it; the rotor's mass, which section 12h puts where
its proxy's triangles from the mount point put it, and the load cap keep the
mount point: with the anchor in the mass too, the wheels' transverse moments
went from 0.450 to 0.485, live 0.428). `RA2_HINGE_MOUNT_PIVOT=1` for the old
anchor.

**The turns, ours against live** (degrees about the axle, robot frame; the
bodies whose first substep after the placement was a whole one, see below):

| robot | wheels: live / ours |
|---|---|
| Scout | 14.52 / 14.15, 2.14 / 2.54 (fork 0.81 / 0.55) |
| ALARM | 66.75 / 68.51, 10.57 / 10.47, 85.71 / 87.24, 10.50 / 10.40 (rod at its rest point in both) |
| Raptor | 173.64 / 173.62, 4.22 / 4.22, 39.59 / 39.57, 137.99 / 137.98 |
| Sentinel (rear) | 29.23 / 29.22, 72.53 / 72.54 |
| Dementia | 150.82 / 143.88, 41.73 / 41.40, 107.94 / 107.20, 7.26 / 1.35 |
| MiniBerg | 10.76 / 10.13, 117.58 / 127.25, 159.25 / 165.78, 10.97 / 10.32 |
| Razor, FlapJack | 12.47 / 6.48, 10.38 / 10.19; 97.96 / 101.57 |
| Berserker | 23.20 / 20.30 (spinner 45.00 / 45.00) |

Before this round every one was 0 in ours. Test `tests/spin_live.rs` (Scout,
ALARM, Raptor to 2.5°).

**Not reproduced.** (1) Live, some bodies take only part of the first substep
after the placement: their vertical speed after it is -0.024 to -0.114 m/s
instead of -0.2 (gravity 12 m/s², one substep), reproducibly per robot (REVENGE's
four wheels -0.074 in two sessions, its blade -0.2; the Scout's fork -0.092 in one
session, -0.2 in the other), so they fall less before the joints take them and
spin otherwise: REVENGE's wheels 161.8° and 175.9° live against 81.9° and -18.5°
ours (its other two, also partial, within 1.5°), M.A.D.'s two, Razor's and
FlapJack's front pair. What splits that substep is not recovered (a piece of a
backstepped substep, `0x52aa60`, is the suspect). (2) The steered hubs (Sentinel's
two 23 kg bodies, live 109° and 113°, ours -172° and 55°): the steering unit's
wheel constraint (`FUN_00553af0`) is not the hinge above. (3) **Tornado and Iceberg
face the wrong way**: live their chassis are turned +45° and 134° at their start
point, ours -45° (90° and 180° off; Stinger, the same start point, -45° in both).
Both are 1.07 `.bot`s, which store no forward heading (`RobotRig::heading` falls back to 0); what the
original takes as their front is not recovered. This is what keeps Tornado one
lowering step high (section 12v) and makes its turns incomparable.

**The spawn heights** (chassis centre at the bell, box): 13 of 16 stock robots now
where live holds them, to 0.1 mm (§12v: 10): EMERGENCY, Sentinel, Raptor,
Dementia, MiniBerg come down to live's step now; Scout, ALARM, Razor, M.A.D.,
REVENGE, Little Metal Friend, Stinger, Iceberg stay right. Wrong: Tornado (+0.05,
its heading), and two the turns moved: Berserker and FlapJack one step low
(-0.05; FlapJack's lowest traction phantom ends 0.1002 m above the floor, 0.2 mm
past the tolerance; both have partial-substep wheels, (1)). Compressor Scout and
ALARM unchanged. `spawn_live` lets FlapJack be a step low.

**The wheels' points against the floor.** `orderprobe` now also takes `FRAME`
lines (`spincmp.py frames <spinup capture>`): every live body as built, so each
body's turn from the original's frame to ours comes from the build, where no
wheel has turned yet, instead of from a rest pose of another session (whose
wheels had turned otherwise). Over `contacts-{alarm,scout}-straightturn__live__000`
the robots' wheel × floor entries whose point count differs: with the rest
poses 891 and 1237, with the build frames 354 and 207 (ALARM's wheels 19-31 of
about 460 entries each, the Scout's 13-28 of 510-545; the §12v note had ALARM
with two points where ours had one in 1050 of 2400). One wheel stays apart: the
Scout's right wheel in both captures (113-256 of 480-520 differ, 46-80 with points far
apart), whose live build frame alone carries a 0.9° tilt; not resolved.

**The landing** (box, from the hold: lowest drop / rebound after it within 0.5 s /
yaw at 0.5 s; live from `rest-*__live__000`; before = `fidelity` cc6fe23):

| robot | live | before | after |
|---|---|---|---|
| Raptor | 0.296 / 0.153 / -0.142 | 0.217 / 0.072 / +0.041 | 0.274 / 0.089 / -0.065 |
| M.A.D. | 0.228 / 0.024 / +0.056 | 0.227 / 0.025 / -0.035 | 0.227 / 0.021 / +0.006 |
| Scout | 0.249 / 0.048 / -0.007 | 0.251 / 0.073 / -0.007 | 0.245 / 0.033 / -0.032 |
| ALARM | 0.236 / 0.040 / -0.039 | 0.240 / 0.077 / +0.003 | 0.234 / 0.045 / +0.028 |

Raptor's bounce and yaw move most of the way and its yaw takes live's sense; the
Scout's and ALARM's rebounds come to live's, their yaws move away. Live, Raptor's
chassis stops first and its wheels, pulled down through their hinges at up to
4.6 m/s (ours 3.7), then throw it back up at 1.9 m/s (ours 1.2): the rest of the
bounce is in that exchange. The resolver is not it: `0x559720` turns inelastic
once more than 100 impulses are spent in an island (`N < count`), as ours
(`spent > 100`), every wheel and the chassis being islands of their own against
the floor, and the restitution is `eA eB` (§12m).

**Validation.** Before = `fidelity` cc6fe23 (section 12v's after column),
after = this branch. `FIDTRACE_WARMUP` ensembles, 24 runs each, counted = `>= 10`
on the other robot, robot hits only, mean (total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 32.2 (118) sd 26 / 5.5, 80 s, 5/24 | 39.9 (161) sd 31 / 5.1, 89 s, 9/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 10/24 | 7/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 4.2 (112) / 0.8, 52 s, 2/24 | 4.5 (146) / 0.9, 65 s, 3/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 72.5 (309) sd 33 / 4.1, 103 s, 13/24 | 83.3 (301) sd 52 / 3.9, 98 s, 16/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 20.5 (109) sd 21 / 3.7, 93 s, 10/24 | 17.7 (101) sd 15 / 3.6, 85 s, 7/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 10/24 | 11/24 |
| box Tornado v RipBlade, 9 warmups: full length | | 9 of 9 | 8 of 9 (warmup 0 ends at 44 s, RipBlade last) |

Section 12s's replays of the live placements (`twoprobe`): set 1 (20 trials)
ALARM on its back at some point / at the end 4 / 3 before, 3 / 2 after (live
1 / 1), mean peak |ω| 3.9 → 3.4 rad/s (live 3.1), both chassis' end centres
from live, median 1.45 → 1.62 m (worse); set 2 (17) never on the back, median
0.33 → 0.31 m.

AI suite (ten pairings, warmup 0), counted hits each way before → after: Scout v
ALARM 2/24 → 6/32, Tornado v RipBlade 124/38 (120 s) → 18/1 (Tornado out at 44 s),
Grog v Boxer 62/119 → 66/171, Razor v FlapJack 6/0 → 9/2 (120 s), Sentinel v Wide
Load 3/118 → 0/116, Bear v Tornado 45/75 → 44/58, Grog v Boxer hill 17/22 → 10/12,
Octagon Scout v ALARM 9/31 → 8/53, Razor v FlapJack park 4/0 → 3/0, Bushido v
Devil 38/8 → 21/9; every pairing still has damage dealt (Sentinel and the park's
FlapJack deal none of their own, as the park's FlapJack before).
The box fight's ALARM count comes to live's (32.2 → 39.9, live 38.5) and its
falls drop (10 → 7 of 24, live about 5 of 19); the Octagon's ALARM moves toward
live (20.5 → 17.7, live 13.6); the Compressor's Wide Load moves away (72.5 →
83.3, live 73.6, sd 52), within about one standard error.

**Still open.** The partial first substep after a placement (above, (1)); the
steering unit's hubs; the 1.07 robots' heading (Tornado, Iceberg; also LugNut and
the other 1.07 `.bot`s); the Scout's right wheel in the point-count probe; the
rest of the landing bounce (the wheels' pull-down through their hinges).

### 12x. The weight a body carries out of the build, the bell's empty substep, the steering unit's wheel constraints (fx/landing)

Section 12w left five things open: some bodies taking only part of the first
substep after the placement, the steering unit's hubs, the rest of the landing
bounce, the spawn heights of Berserker and FlapJack (and Tornado's, with the
1.07 nose now merged), and the Scout's right wheel in the point-count probe.

New live captures (three sessions on `:7`, the prefix's `AI/`, `Teams/`,
`Robot Designs/` and `ra2.cfg` sha256-identical before and after each; manifest
rows in `manifest-additions-2026-10-21-landing.tsv`): new agent
`frida/ra2_pieces.js` loaded before Start Game through `capture_rest.py`
(`WATCH_AGENT=ra2_pieces.js`): from the match load every change of a robot
body's mass or `+0x1c` flags (`M`), and for six `World::stepTo` calls
(`0x52aa60`) from every start-point placement (`0x4dcff0`) and forty from the
bell every stage of the substep with every robot body after it (`S`: the
resolution, vtable `+0x24`; the integration `0x52a740` with its `dt`;
`0x52a810`; the detection, vtable `+0x20`; the actions `0x52a990`); in the
later sessions also every body's first `0x200` bytes at the placement (`PB`),
the wheel atoms (`W`, `0x553c30`), the hinge atoms (`J`), the other constraint
set-ups (`C`) and every `setActive` (`0x523230`) with its caller (`A`).
`pieces-{revenge-raptor,scout-alarm,miniberg-iceberg,sentinel-berserker,stinger-tornado}-box__live__000`
(the first session: its agent failed to hook `0x52ae83`, which is mid-function,
and lost the `P`/`PB`/`W`/`J`/`A` rows), `__001` of `miniberg-iceberg`,
`stinger-tornado`, `sentinel-berserker` and `__002` of `sentinel-berserker`;
each with the `ra2_deact.js` rows of its session (`pieces-deact-*`). New
`tools/oracle/landcmp.py` measures the landing (below) from such rows or a
`spinup-deact-*` capture against `examples/spinprobe` (`LAND=`, which now
holds the robots and lets them go as a match does, and prints the chassis'
heading).

**The partial first substep is a stale weight (recovered, verified live).**
The original adds gravity to a body's force accumulator, with the body's mass
of that moment, in the actions after its integration; the next integration
turns the accumulator into velocity with the body's current inverse mass. A
robot is built one component per frame (about 116 substeps apart), the world
stepping in between; a component that brings a body of its own (a motor's
rotor, a burst motor's, a piston's rod, a steering unit's hubs) wakes the robot
(`A` rows: `0x531ef7` on the chassis, then the system's `0x5341cb` on every
body) and it freezes again 14 substeps later (`0x534171`). The components that
only add to an existing body come later and change its mass while it sleeps
(`M` rows: REVENGE's wheels 10 → 27 kg at substeps 1221-1569, its last wake at
1107), so the robot is frozen with each accumulator holding `m_then · g`. The
placement wakes the bodies, and their first integration moves them by
`-0.2 · m_then / m` m/s (`S` rows, `int` stage: REVENGE's wheels -0.07406,
exactly `-0.2 · 10/27`; its blade, 30 kg since a wake, -0.2). Every live
`vy1` of section 12w fits: the Scout's fork arm 12/26, ALARM's piston rod 8/33,
Raptor's top 12/45, Sentinel's top 12/28, Berserker's wheel 10/27, Stinger's
8/15, Tornado's spinner 15/125. Ours (`Robot::build_share`, `duel::turn_into_place`):
a part's share is the mass of its members that come no later in the `.bot`
than the last body-bringing component, over its mass; the placement's first
substep gets `(share - 1) m g` added to its force. `RA2_SPAWN_FULL_WEIGHT=1`
turns it off. Test `tests/landing_live.rs`.

Not reproduced: in some robots a body is also woken during the build by the
detector (`A` rows, caller `0x548f09`, the detector's wake of an inactive body
in a pair, 56-58 substeps after certain components are added: MiniBerg's
second motor's rotor at 500, 616, 1312 and 1776, Iceberg's at 3458), after which
the system wakes the rest; what it is paired with is not recovered. These late
wakes give MiniBerg's last wheel and top, Iceberg's top (22 of 38 kg, ours 12),
Tornado's second wheel and the Scout's right wheel (live woken at 521 and 637,
57 substeps after its wheel and its battery were added) their full (or a
larger) weight, which ours does not model; they come 57-58 substeps after some
component additions and not others (MiniBerg: after its 2nd, 3rd, 9th and 13th
component; REVENGE and Raptor: never).

| turn about the axle at the hold, ours - live, deg | before | after |
|---|---|---|
| REVENGE's wheels (live 161.75, 7.00, 8.88, 175.85) | -79.8, +0.8, +1.5, +165.7 | -8.9, +0.9, +0.9, -5.4 |
| M.A.D.'s wheels (141.14, 160.57) | -95.7, +69.0 | -0.0, -0.0 |
| Razor's 10 kg wheels (110.53, 7.12) | -101.7, +3.1 | +3.9, -0.1 |
| FlapJack's 10 kg wheel (7.47) / Berserker's (7.39) | +3.2 / +3.2 | -0.1 / -0.1 |
| Dementia's 10 kg body (129.06, live share 0.32, ours 0.44) | +144.2 | +53.8 |
| MiniBerg's 10 kg wheel (10.97) / Tornado's (10.75) / the Scout's right (2.14), late wakes | -0.1 / +1.0 / +0.4 | -3.5 / -2.4 / +0.7 |

**The spawn heights.** Berserker and FlapJack now stop their lowering where
live does (exact); Tornado, with its nose merged, was at live's height before
this round and MiniBerg too, and both are now one step (0.05 m) high: their
wheels turn with weights the late build wakes would have changed. Dementia
holds 0.05 m lower than `spinup-dementia-lmf-box__live__000` and exactly at
`rest-team6bot1-team6bot2-box__live__000`: live it differs between sessions.
14 of 16 stock robots at live's height, the same count as before with two
others. `spawn_live` no longer lets FlapJack be a step low.

**The bell's empty substep (recovered, verified live).** Held through the
countdown (the chassis fixed, the rest inactive) a body gathers no weight, so
the first substep after the bell integrates empty accumulators: live every
body of both robots keeps velocity 0 through that substep's integration and
starts falling at -0.2 m/s in the next (`S` rows at the `BELL`, both robots in
every capture). Ours added gravity in the integration, so its robots fell a
substep early relative to everything else (the components' ticks, the
contacts' timing). `Duel::hold_robots(false)` now adds `-m g` to every body
for that substep; `RA2_BELL_GRAVITY=1` for the old release.

**The landing** (`landcmp.py`: from the first substep the chassis moves, its
lowest drop and the rebound after it within 0.5 s, the chassis' heading
change at 0.5 s, and the fastest downward speed of its other bodies; live
from `spinup-deact-*__live__000`, the sessions whose turns section 12w used —
`rest-*` sessions differ by up to 0.04 m and 0.07 rad; before = `fidelity`
2f3cefc, after = this branch):

| robot | live | before | after |
|---|---|---|---|
| Raptor | 0.288 / 0.116 / -0.179, 4.71 m/s | 0.274 / 0.089 / +0.065, 3.81 | 0.289 / 0.113 / -0.141, 4.55 |
| REVENGE | 0.255 / 0.084 / +0.098, 4.98 | 0.232 / 0.041 / +0.040, 3.28 | 0.258 / 0.063 / +0.129, 4.34 |
| Scout | 0.258 / 0.072 / +0.004, 3.48 | 0.245 / 0.034 / +0.032, 2.91 | 0.260 / 0.063 / +0.024, 3.81 |
| ALARM | 0.253 / 0.056 / -0.024, 4.34 | 0.234 / 0.045 / -0.028, 3.44 | 0.251 / 0.066 / -0.028, 4.24 |
| M.A.D. | 0.228 / 0.022 / +0.088, 2.76 | 0.227 / 0.021 / -0.006, 2.97 | 0.228 / 0.019 / +0.080, 2.77 |
| EMERGENCY | 0.227 / 0.029 / +0.013, 4.95 | 0.221 / 0.017 / +0.002, 4.96 | 0.221 / 0.021 / -0.000, 5.03 |
| Razor | 0.241 / 0.037 / -0.031, 3.88 | 0.233 / 0.038 / -0.060, 2.68 | 0.226 / 0.026 / -0.019, 3.47 |
| FlapJack | 0.232 / 0.037 / -0.043, 2.95 | 0.189 / 0.032 / -0.008, 2.64 | 0.226 / 0.028 / -0.036, 2.96 |

(The headings here are `landcmp.py`'s for both columns; section 12w's table
took ours in the other sense.) The "4.6 m/s pull-down" is not a pull through
the hinges alone: live (`S` rows at the landing) the four wheels are stopped by
the floor in the resolution of one substep, and the actions of the same substep
(the joints) then hand them the chassis' momentum, so the velocity a capture
reads between substeps is that pull and the next resolution stops it again.
Ours had one wheel of Raptor's detect the floor a substep early (0.0996 m, 0.4
mm inside the tolerance), which started the exchange a substep early and
softer; with the bell's empty substep the four reach it together, as live.
Raptor's drop, rebound and yaw now follow live; REVENGE's drop too (its yaw
now overshoots).

**The steering unit's hubs are wheel constraints (recovered, verified live).**
A `Steering` unit's hubs are held by Havok's wheel constraint (vtable
`0x6ac4b8`; set-up `0x553c30`; its pass `0x5547d0` re-solves only the block;
`0x553af0` rebuilds the steered axle), declared in the unit's `.gmf` as
constraint node 46 (`Carwheel01` on `hub1`, `Carwheel02` on `hub2`: a name, a
frame, the two bodies, a pivot, an axle and eight floats; `gmi::WheelJoint`,
skipped before). During the build the unit's hubs are constrained to a body of
its own left at the arena's origin (`W` rows at substep 101: 40 kg, then 4);
the atoms are re-targeted to the chassis before the placement (`W` rows from
3799). The set-up (atom fields, chassis frame for A): a limit row along the
suspension axis `s` (`+0xc8`, `(0, 1, 0)`, limit flag `+0x20` set, `[0, 0]`,
strength `+0x2c` = 1, motor `+0x21` off; applied once per solver call like the
hinge's limit), then one block of four rows: linear along `n2 = n1 × s` and
`n1 = normalize(s × a)` (errors the anchors' gap along them), angular about
the steering axis `st` (`+0x98`, `(0, 1, 0)`) and `st × a` (errors `b · (st ×
a)` and `-(b · st)`, the hinge's form of section 12w); no steering servo
(`+0x3c`/`+0x3d` off); block factors `+0x18`, `+0x1c` = 1. The anchors are the
constraint's pivot: `+0x58` (-0.8281, -0.1023, 0.2937) and (0.8362, -0.1023,
0.2942) from the chassis' centre of mass, `+0x68` 0.1404 and 0.1386 m inboard
of the hubs' centres along the axle; that is `carsteering2.gmf`'s `Carwheel`
pivot (and `wheelpoint` attachment), 0.058 m inboard of the attachment the
wheel mounts on (`Point04`), which is where ours had put it. The other
constraint set-ups (`0x534850`, `0x552bb0`, `0x555080`) never touch the hubs
(`C` rows). Ours: `JointInfo::anchor` is the unit's wheel pivot on the hub line
(`Component::wheel_pivots`; `RA2_STEER_MOUNT_PIVOT=1` for the mount point), and
the hub's hinge carries the suspension axis (`HingeDesc::suspension`,
`joints::Joint::suspension_limit`; `RA2_STEER_HINGE=1` for the plain hinge).
The anchors and the hubs' inertia (0.611 across, 0.918 along the axle, live
and ours) now agree; the turns at the hold come closer for one hub and not the
other (live 109.2 and 113.2 deg; before -171.7 and 55.0; after 117.4 and 57.9).
Test `tests/landing_live.rs`.

**The Scout's right wheel in the point-count probe.** Not resolved. Its live
build frame is turned 1.27 deg about its axle (not tilted): it was awake twice
more during the build (substeps 521 and 637 of `spinup-scout-alarm-box__live__000`),
the late wakes above; the probe maps each live body through its own build
frame, so that turn is accounted for and is not what parts the point counts.

**Validation.** Before = `fidelity` 2f3cefc (built from a clean tree; it
reproduces section 12w's after column for the box and the Octagon, while the
Compressor reads 72.5 against 12w's 83.3: the flame-look merge's hub normals
moved it back), after = this branch. `FIDTRACE_WARMUP` ensembles, 24 runs each,
counted = `>= 10` on the other robot, robot hits only, mean (total), mean
length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 39.9 (161) sd 31 / 5.1, 89 s, 9/24 | 36.2 (160) sd 28 / 4.8, 90 s, 12/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 7/24 | 10/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 5.2 (147) / 0.9, 70 s, 4/24 | 3.8 (96) / 1.0, 58 s, 1/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 72.5 (302) sd 49 / 4.1, 96 s, 13/24 | 89.2 (328) sd 41 / 5.0, 104 s, 16/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 17.7 (101) sd 15 / 3.6, 85 s, 7/24 | 23.4 (98) sd 23 / 4.1, 75 s, 3/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 11/24 | 10/24 |

The weight and bell changes alone (no steering change) give the same box,
Razor and Octagon numbers and Wide Load 76.0; with the steering anchors alone
72.9, with the wheel constraint's rows alone 90.1. So the Compressor's rise is
the steering hubs' suspension rows, a recovered and live-checked change that
moves this number away from live (by about two standard errors); the
Octagon's ALARM count moves away by about one (17.7 → 23.4, live 13.6) and the
box's falls rise (7 → 10 of 24, live about 5 of 19); Razor's count comes to
live's (5.2 → 3.8, live 3.0).

Section 12s's replays of the live placements (`twoprobe`): set 1 (20 trials)
ALARM on its back at some point / at the end 3 / 2 before, 2 / 2 after (live
1 / 1), mean peak |ω| 3.43 → 3.15 rad/s (live 3.05), both chassis' end centres
from live, median 1.61 → 1.31 m; set 2 (17) never on the back, median 0.31 →
0.23 m, mean peak |ω| 1.52 → 1.62 (live 1.40).

AI suite (ten pairings, warmup 0), counted hits each way and length, before →
after: Scout v ALARM 6/32 120 s → 5/68 100 s, Tornado v RipBlade 116/65 84 s →
119/40 116 s, Grog v Boxer 66/171 → 53/176 (120 s both), Razor v FlapJack 5/2
106 s → 5/1 43 s, Sentinel v Wide Load 2/26 61 s → 4/57 120 s, Bear v Tornado
59/64 52 s → 60/82 82 s, Grog v Boxer hill 10/12 46 s → 8/28 39 s, Octagon
Scout v ALARM 8/53 81 s → 8/68 112 s, Razor v FlapJack park 3/1 63 s → 3/1
73 s, Bushido v Devil 21/9 → 32/6 (120 s both); every pairing still has damage
dealt each way.

**Still open.** What pairs MiniBerg's, Iceberg's and Tornado's bodies with the
detector during the build (their late wakes, hence two spawn heights); the
steering hubs' remaining turn on one side; the Scout's right wheel in the
point-count probe; the Compressor's Wide Load count with the wheel
constraint's rows; Raptor's weapon motor takes hold in the third substep after
the bell live (ours: the first).

### 12y. The steering hubs in motion, the arenas' sliders, the build's late wakes (fx/compressor)

Section 12x left the Compressor's Wide Load count above live (89.2, live 73.6),
the Octagon's ALARM count above live since section 12t (23.4, live 13.6), the
build's late wakes (caller `0x548f09`) unrecovered and the steering hubs' turn
at the hold open on one side.

New live captures (three sessions on `:7`, the prefix's `AI/`, `Teams/`,
`Robot Designs/` and `ra2.cfg` copied before each and sha256-identical after;
manifest rows in `manifest-additions-2026-10-21-compressor.tsv`):
`wheel-sentinel-miniberg-box__live__000` and `wheel-iceberg-tornado-box__live__000`
with their `deact-wheel-*` (new agent `frida/ra2_wheel.js` through
`tools/oracle/capture_wheel.py`: box, hazards off, AIs neutered, robot 0 driven
by `capture_deact.py` phases, straight at 60, driving and turning, coasting,
reversing and turning, steering at rest; from the bell every wheel-constraint
set-up `0x553c30` and pass `0x5547d0` with both bodies at entry and exit, every
`0x553af0`; from the load every `0x548ef0` with the detector's held-back pairs),
`slider-octagon__live__000`, `slider-metalskull__live__000`,
`slider-box__live__000` (new agent `frida/ra2_slider.js` through
`tools/oracle/capture_slider.py`: every prismatic controller update `0x4b5240`,
push `0x4b51b0` and `Lock` `0x4b50c0`, with the moving body; the box's saw and
spike driven through `ra2_python.py`).

**The wheel constraint in motion matches ours (verified live).**
`tools/oracle/wheelcmp.py` turns a `wheel-*` capture into one block per solver
call (a `stepTo` may run more than one) and `examples/wheelreplay` runs ours on
the live bodies and atom (`havok17::joints::replay_wheel`): the set-up from
its entry, each pass from its own entry. Over Sentinel's 4808 calls of both
hubs (driving, steering to ±30°, a wall hit), the four passes agree to the
live digits (mean miss 7e-5 rad/s against a 0.17 rad/s mean change in the
first), the set-up to a median 2.4 % of its change (90th percentile 12 %; the
largest misses come in the substeps of the wall hit). Every alternative tried
is worse: no suspension limit (3.8× the miss), the limit after the block, the
limit and the block from the same entry, the linear rows' lever arms both at
A's or B's pivot or the midpoint, the limit's effective mass without the
angular terms, the bodies' frames a substep stale. The steering itself is as
ours: `0x553af0` (called from `0x508a45`) rebuilds the axle as
`cos θ b + sin θ (b × st)` from the unsteered axle `+0xb8` and the steering
axis `+0x98`, `θ` moving 4 rad/s to at most `π/6` (live steps of 0.0667 a
substep, half steps when a frame ran two calls).

**No friction about the axle (recovered, verified live).** The atom's motor
flag is off throughout and nothing else in the set-up acts about the axle: the
live change of a hub's angular velocity along its axle is ours *without*
friction to 0.002 rad/s (mean over the 2032 calls with the hub turning faster
than 1 rad/s relative to the chassis, signed by its spin), where the 8 N·m
ours gave a passive axle (the wheel's `resistance` × 40) would take 0.145 a
substep. A `Steering` unit's hubs now have none (`rig.rs`, the axle branch;
`RA2_STEER_FRICTION=1` puts it back). Other passive axles keep theirs: their
constraint is not this atom and was not captured. Test
`havok17::joints::tests::a_steered_wheel_constraint_answers_as_the_live_atom`.

**The arenas' sliders are pushed, not motored (recovered, verified live).**
`AddPrismatic(host, child, ax, ay, az, s1, s2, offset)` (`0x404800` →
`0x4b47b0`) keeps `s1` as the stop a negative direction drives to (`+0x6c`),
`s2` as the positive one's (`+0x70`), the travel `[min, max]` of the two
(`+0x64`/`+0x68`), power `(1, 60)` and starts locked; `Lock(1)` (`0x4b50c0`)
collapses the travel onto the position, stops the body and clears the
direction. Its update (`0x4b5240`, every substep while unlocked) is the robot
piston's (section 12b): with no direction or within 0.05 of its stop it locks
if it auto-locks; otherwise it pushes `-(dir · F)` along the axis for one
substep (`0x4b51b0`) unless the body already moves the commanded way at `S`
or faster. Ours drove arena sliders as a speed motor at `speed · sign(dir)`
(the pit floors' `+0.2` pointed at the stop they sat on, so they never
opened), took the travel relative to the first number (the box's floor saw
`[0, 0.56]` for `[-0.1, 0.46]`), and kept `Lock`'s direction.
`Duel::drive_arena_slider` now runs the controller (`motor::PrismaticSettings`,
as `Robot::drive_prismatic`); `hazards::slider_limits(s1, s2)`,
`LiveJoint::stops`; the controller's default power and `Lock`'s cleared
direction in `ra2script`.

| slider | live | ours |
|---|---|---|
| Octagon pit floor, free (200 kg, `(.2, 2000)`) | 2 pushes of 2000 N, locked at -3.8 679 substeps after the unlock | 2 pushes, 679 substeps |
| Octagon pit floor, a robot on it | 1031-1032 substeps | (a robot leaves it in a fight) |
| Metal Skull grate (100 kg, `(.3, -400)`) | pushed +400 every substep (moving with a negative force is never capped), at +5 after 95 substeps, locked | 97 substeps |
| box floor saw (100 kg, `(2, -2000)`) | rises in about 20 substeps and bobs under its stop, pushed 155 times in 7 s; down in 10 pushes | pushed, as live (not compared step by step) |
| box spike (40 kg) | defaults `(1, 60)`; `FireTeeth` 40000 N for a substep, 16.5 m/s | as live |

Test `duel::tests::a_pit_floor_slides_open_once_its_script_lets_it_go`.

**What made the Octagon's ALARM count high.** Live, the Octagon's hazards end
ALARM's attack: robots fall into the opened pits (two of the five live fights:
`fight-scout-alarm-octagon__live__000` at 65 s, `alarmfight-...__live__001` at
27 s) or a hellraiser throws ALARM (arena hits 27, 68, 62 at (-4.8, 0.3, -0.4)
in `arenahits-...__live__001`; 107 in `alarmfight-...__live__000`). Per hit and
in the first 30 s ours matched live (amount 42, live 47; 7.9 and 3.4 counted
hits in 0-15 and 15-30 s, live 6.8 and 3.6); the surplus was every hit after
that, in fights that ran on because no pit ever opened (0 of 24 runs below
y -1.5). With the pits pushed open, 6 of 24 runs end with the Scout in a pit
and 6 with ALARM. Our hellraisers do fire (`hellzone1` entered by ALARM's
hammer head: `Hinge01` up to its 1.2 rad stop at 79 rad/s and back at -3),
but no arena hit on ALARM reaches 50 in 24 runs (live 61-170 in four of five
fights): not resolved.

**The build's late wakes (recovered in part).** The detector wakes a body
(`0x548ed0` → `0x548ef0`, `setActive(1, 0)`, caller `0x548f09`) and then
goes through the pairs its broadphase held back for it (the detector's list
`+0x78..+0x7c`, two collidables each), creating their agents and waking the
partner if it is inactive (`0x524180`, the recursive call returns to
`0x54918b`). During the build every component first appears as a body of its
own at the arena's origin, in its own frame (live: the Iceberg's battery at
(0.091, 0.183, 0.001), its Ice Pick at (0, 0.066, 0)), active, and is moved
onto the robot 58 substeps later (the chassis' `0x531ef7` wake). The late
wakes are the held-back pairs of that first activation: Iceberg's at
substep 1734, its Ice Pick (component 13) with the wheel on its second motor
(two pairs); Tornado's at 3416, its Nifty battery (component 10) with the
spinner (one pair). So a component wakes the sleeping robot bodies its
collidables overlap where it first appears, just before it is added. Our
per-piece bounding boxes (`examples/buildwakes`) give Iceberg's (13 only,
though against the other wheel) and Tornado's 10, but also Tornado's 9, 12
and 13, MiniBerg's 8 and 12 (live 3, 4, 10 and 14) and the Scout's 6 only
(live 6 and 7): what the broadphase's boxes are for a body built while
asleep is not recovered, so ours still does not model these wakes.

**The steering hubs' turn at the hold.** Unchanged (live 109.2 / 113.2 deg,
ours 117.4 / 57.9). Live the hubs ride the unit's build body at the origin
until they are re-targeted to the chassis (section 12x), and that body turns
154-155 deg about a skew axis during the build
(`spinup-sentinel-berserker-box__live__000`, robot 0); ours builds the hubs on
the chassis, so their turn before the placement is not modelled.

**Validation.** Before = `fidelity` a5810c0 (reproduces section 12x's after
column to the digit), after = this branch. `FIDTRACE_WARMUP` ensembles, 24
runs each, counted = `>= 10` on the other robot, robot hits only, mean
(total), mean length, full-length runs:

| fight | live | before | after |
|---|---|---|---|
| box Scout v ALARM: ALARM / Scout | 38.5 / 4.8 (6) | 36.2 (160) sd 28 / 4.8, 90 s, 12/24 | 34.2 (151) sd 28 / 4.8, 86 s, 11/24 |
| ... ALARM on its back ≥ 5 s by 45 s | about 5 of 19 | 10/24 | 10/24 |
| box Razor v FlapJack: Razor / FlapJack | 3.0 / 0.1 | 3.8 (96) / 1.0, 58 s, 1/24 | 3.5 (81) / 1.0, 52 s, 2/24 |
| Compressor Sentinel v Wide Load: Wide Load / Sentinel | 73.6 / 5.3 (8) | 89.2 (328) sd 41 / 5.0, 104 s, 16/24 | 70.5 (262) sd 53 / 3.9, 86 s, 9/24 |
| Octagon Scout v ALARM: ALARM / Scout | 13.6 / 3.2 (5) | 23.4 (98) sd 23 / 4.1, 75 s, 3/24 | 17.8 (85) sd 17 / 3.4, 59 s, 1/24 |
| ... ALARM on its back ≥ 5 s by 45 s | | 10/24 | 8/24 |

The Compressor's change is the hubs' friction alone (with
`RA2_STEER_FRICTION=1` the after build gives 89.2 again); the box's and
Razor's are the box's saws and spikes under the new slider drive; the
Octagon's are its pits.

Section 12s's replays of the live placements (`twoprobe`): set 1 (20 trials)
ALARM on its back at some point / at the end 2 / 2 both (live 1 / 1), mean
peak |ω| 3.15 both (live 3.05), end-centre median 1.31 m both; set 2 (17)
never on its back, median 0.23 m both, mean peak |ω| 1.62 → 1.64 (live 1.40).

AI suite (ten pairings, warmup 0), counted hits each way and length, before →
after: Scout v ALARM 5/68 100 s → 5/40 55 s, Tornado v RipBlade 119/40 116 s
(both), Grog v Boxer 53/176 120 s (both), Razor v FlapJack 5/1 43 s (both),
Sentinel v Wide Load 4/57 120 s → 6/40 55 s, Bear v Tornado 60/82 82 s →
49/69 71 s, Grog v Boxer hill 8/28 39 s (both), Octagon Scout v ALARM 8/68
112 s → 5/59 69 s, Razor v FlapJack park 3/1 73 s (both), Bushido v Devil
32/6 120 s (both); every pairing still has damage dealt each way.

**Still open.** The bounding boxes behind the build's late wakes (hence
MiniBerg's and Tornado's spawn heights); the hellraisers' hits on ALARM; the
steering hubs' turn before the placement (the unit's build body); the
set-up's residual 2-12 % against live in a wall hit.
