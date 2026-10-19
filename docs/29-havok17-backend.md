# 29 - The `havok17` physics backend and the game's drive model

`havok17` is a second implementation of `PhysicsBackend`
(`engine/crates/ra2sim/src/havok17/`) whose dynamics follow the Havok 1.7.1
subset *Robot Arena 2* ships, written from the recovered specification (the
map in the `havok-scope` study, the decompiles cited below and live
measurements of the original). Jolt is kept only as a geometry engine: it holds
every body's shape at its pose and answers closest-point queries and ray casts;
Jolt's solver never runs in this backend.

Selection: `RA2_PHYSICS=havok17` (default `jolt`). The game's drive model
(section 3) runs with `havok17` by default; `RA2_DRIVE=magic|interim` overrides
it for either backend. `RA2_H17_DEBUG=<body>` prints one body's state and
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
* a burst piston spawns cocked, its rod retracted to its lower limit. Live,
  ALARM's 33 kg hammer body sits at the chassis' height and the robot rests on
  its four hubs (origin y -1.811); at the model's own pose the hammer hangs
  0.74 m lower and held the robot up (ours -1.63). `burstpiston2`'s travel is
  `-0.73 .. 0.2`: the 0.73 m is that difference. This applies to both backends
  (ALARM now rests at -1.807 under Jolt).

## 5. Validation

Drive tests from the original's own start poses (`fidtrace drive-at`), against
`drive-scout-box__live__000` (Scout: 2.1 s straight, 2.5 s turn in place),
`magic-scout__live__003` and `magic-alarm__live__000` (sim time, from the
captured chassis positions):

| robot / metric | original | Jolt + interim | Jolt + MagicAction | havok17 |
|---|---:|---:|---:|---:|
| Scout top speed (m/s) | 11.12 | 12.4 | 10.67 | **11.04** |
| Scout t to 90 % (s) | 1.58 | 1.6 | 1.4 | **1.6** |
| Scout straight distance (m) | 15.42 | 17.44 | 16.38 | **15.71** |
| Scout coast (m, 3 s) | 8.05 | **7.36** | 4.87 | 5.34 |
| Scout turn in place (rad/s) | 10.66 | 8.68 | 10.61 | **10.64** |
| Scout 1.35 s straight: top / distance | 9.32 / 7.83 | 9.83 / 8.26 | 9.5 / 8.26 | **9.35 / 7.53** |
| Scout 1.35 s straight: coast (m) | 6.72 | **6.29** | 4.37 | 4.51 |
| Scout rest height (origin y) | -1.884 | -1.896 | -1.895 | -1.932 |
| ALARM 1.65 s straight: top (m/s) | 13.38 | 7.66 | 2.3 | 5.61 |
| ALARM distance / coast (m) | 14.11 / 11.91 | 6.52 / 4.21 | 1.28 / 0.0 | 5.09 / 1.77 |
| ALARM turn in place (rad/s) | 12.1 | 3.98 | 0.22 | 8.94 |
| ALARM rest height | -1.811 | **-1.807** | -1.807 | -1.832 |

Traction survey (45 stock bots, 3 s full throttle, ≥ 2 m): Jolt + interim
44/45, Jolt + MagicAction 39/45, havok17 36/45. The havok17 failures are robots
whose wheels or traction rings do not reach the floor in our assembly (they
rest on a motor, a tank or a weapon): the original's drive model only pushes
through touching wheels.

**Decision: Jolt + the interim wheels stay the default.** havok17 matches the
Scout's drive within a few percent with nothing fitted, but loses on the
multi-robot comparison, and the reason is outside the backend:

## 6. What is missing

1. **Robot assembly (F1/F4).** Live AddWheel records give the hub centres
   relative to the chassis centre (chassis frame). Scout:
   `(-0.490, -0.133, -0.202)`, `(0.528, -0.128, -0.178)`; ours
   `(-0.425, -0.231, -0.203)`, `(0.478, -0.231, -0.192)`. ALARM:
   `(-0.600, -0.208, 0.486)`, `(0.564, -0.201, 0.476)`, `(0.561, -0.195, -0.610)`,
   `(-0.614, -0.189, -0.592)` — all four level; ours put the rear axles 0.19 m
   above the front (`anglemotor1/2` mirrored models with `angle` ±π/2 move the
   axle's 0.097 m offset into ±y), the track is 0.115 m narrower on both robots,
   and ALARM's CO2 tanks and piston stand vertical below the wheels where its
   hammer body is 0.82 m in front of the chassis at chassis height
   (`bodies.json`). Every rest pose and wheel contact depends on this.
2. Coast: our hubs keep skidding after the thrust stops (they spin down from
   40 rad/s), the original's spin up to rolling (`ω ≈ v/0.185` within 0.3 s,
   `havok-step__live__023`), so we coast 5.3 m instead of 8.1; partly the skid
   points in item 1 (our Scout drags its burst-motor housing, the original its
   fork arm).
3. The power factor `g` (battery drain) is 1; ALARM's live thrust falls 7 % in
   12 s.
4. Steering wheels (`Steering` records, `0x486860`/`0x486890`) and the Havok
   wheel constraint (type 5) are not modelled; deactivation (2 s / 2 s) is off.
5. Narrowphase: Jolt's closest points plus our corner manifold stand in for
   Havok's agents (`0x543370`); a faithful one needs our own GJK convex/mesh
   agents (step 3 of the plan in the `havok-scope` study).
6. The weapon test (`weapon-alarm-vs-inert-scout__live__000`) is not compared:
   it needs ALARM's hammer where the original has it (item 1).

## 7. Tools

* `tools/oracle/capture_magic.py` (+ `frida/ra2_magic.js`): the live drive
  model's records and the chassis body every Havok step during a drive test
  (`PLAYER=0|1`, aims at the longest free line with the game's own
  `AimToHeading`). Frida slows the game below 30 fps, so use the recorded
  step times, not wall time.
* `fidtrace drive-at <arena> <bot> <x> <z> <heading> <script> <s> <out>`: a
  drive test from a given pose.
* `h17probe`: one robot, its bodies, wheel records and per-step state.
