# 30 — The power budget (battery, air, dot) and the watch camera's target

Recovered from `Robot Arena 2.exe` (Ghidra decompiles plus a full `objdump` of the
`.text` section, because Ghidra does not make functions of the big robot methods)
and **measured on the live original** with Frida agents injected into the running
game (`tools/oracle/frida/ra2_energy.js`, `ra2_power_step.js`, `ra2_camera.js`; the
`ra2_python.py` oracle drove the robots: AI neutered, `Throttle`, `Turn`, `Input`).
Every number marked *measured* is a live read; everything else is code reading.

## 1. The HUD bars are not hitpoints

`statusmeters.gib`'s battery bar, air bar and dot read the **Robot** object
(`.?AVRobot@@`, vtable `0x6aa9ac`) fields `+0xa8`, `+0xa4`, `+0xbc` through
`FUN_0043e5f0`, `FUN_0043e5e0` and `FUN_00416940` (the update is `0x43e570`;
`0x4b3540`/`0x4b3570` multiply the float by 52 and `__ftol` it). The previous port
filled the bars from component hitpoints; that was wrong.

| field | meaning | writer |
|---|---|---|
| `+0xa8` | battery fraction `elec now / elec total` (`0` when the total is `0`) | `0x4dc5ae` (`fstp`), inside `0x4dc3f0` |
| `+0xa4` | air fraction `air now / air total` | `0x4dc57f`, inside `0x4dc3f0` |
| `+0x88`/`+0x84` | electric / air **total**, fixed when the robot is built (the writer was not located; they equal the sum of the sources' `+0x280`, 52000 / 1600 measured) | — |
| `+0x90`/`+0x8c` | electric / air **now**: the sum of the attached sources' `+0x284` | `0x4dc47e` / `0x4dc464` |
| `+0xb4`/`+0xb0` | chassis hitpoints / maximum (the armour bar) | `0x4dc558`, `0x4dd257` |
| `+0xbc` | the **dot**: the chassis' critical-hit count (`0` green dot, `1` yellow, `2`+ red; the third is DESTROYED) | `0x4df120` (`inc`), called from `Chassis::apply` `0x44f370` after every blow over 40 once the chassis is at 0 hp |

The three dots are the widgets `green dot` / `yellow dot` / `red dot`, bound to
`+0xe0` / `+0xe4` / `+0xe8` at `0x4b313c`, `0x4b3187`, `0x4b31ce`; `FUN_004b3640`
shows exactly one (`min(+0xbc, 2)`). `ra2sim::robot::Robot::criticals` already was
`+0xbc` (the earlier damage work found the chassis rule); the HUD now draws it.

## 2. The component data

`Components.cfz` definitions (cat with `cfzgrep`): `electotal` / `elecmaxinoutrate`
(Battery: SuperVolt 52000/800, Nifty 24000/400, battpack 7000/100), `airtotal` /
`airmaxinoutrate` (CO2 tank 800/120, big tank 2000/200). A **negative** rate is a
draw: burst motor `-500`, small burst `-400`, burst piston `-100` (air), servo
motor `-50`, angle motor `-80`, linear actuator / servo piston `-80` (air), carsteering
`-200`, cannon `-100` (air), magnet `-600`, …

`PowerSource` (vtable `0x6a9034`; `Battery` `0x6a90b4`, `AirTank` `0x6a9134`)
fields: `+0x280` total, `+0x284` current, `+0x288` drain rate, `+0x28c` =
`maxinoutrate / total` (measured `0.0153846` = 800/52000), `+0x290` = `current *
+0x28c` ("avail", the rate it can give now). `0x4b3d90` is the reset
(`current = total`, called at match start); **nothing else ever adds to
`+0x284`** (the only writers are that reset and the two ticks). So a battery and a
tank only run down: there is no refill rate. *Measured:* 12 s of an idle robot
leaves `+0x284` untouched, and `+0x288` stays 0 with no draw.

## 3. The per-robot power step `0x4dc3f0`

Called once per physics step per robot (`dt` is its argument; *measured* 1/30 s at
full speed, lower when the game runs slow). It

1. zeroes `+0x94/+0x98/+0x9c/+0xa0` (air draw/supply, electric draw/supply),
2. walks the component list (`+0x18`) and calls each component's vtable slot
   `+0x5c(dt, &air, &elec)`; the output is **negative for a draw, positive for a
   supply** and is summed into `D`/`S`; slots `+0x60`/`+0x64` give the air/electric
   current that is summed into `+0x8c`/`+0x90`,
3. computes the budget and publishes four globals, then the components run their
   own ticks.

Who answers `+0x5c`:

| class | vtable slot target | request |
|---|---|---|
| `Battery` / `AirTank` | `0x4b40a0` / `0x4b4450` | supply `dt * avail * f` (`f` = `FUN_0045fa90`: `0.5 + 0.5 * hp / maxhp`, `0.5` once hp < 0) |
| `SpinMotor` | `0x4f59c0` | draw `|+0x298| * dt`, `+0x298` = the wired command `-100..100` |
| `BurstMotor` | `0x446d70` | draw `elecmaxinoutrate * dt` while triggered; the trigger runs `0.6 s` (constant `0x3f19999a` stored at `0x446916`) |
| `BurstPiston` | `0x448980` | draw `airmaxinoutrate * dt`; the stroke timer is `0.25 s` (`0x3e800000` at `0x44880d`) |

The budget (`0x4dc5bc`..`0x4dc77b`), per channel with `q = S / D`:

```
D <= 0 or S <= 0  : scale = 0,                drain = 0
otherwise         : scale = clamp(q, 0.25, 1)
                    drain = D / S   if q >= 1       (sources pay exactly D)
                            scale   otherwise
```

Published globals: electric scale `0x714a70`, air scale `0x714a6c`, air drain
`0x714a74`, electric drain `0x714a78` (`0x45f130`, `0x45f150`). Consumers:

* **motor strength**: `scale` multiplies a motor's speed *and* torque (`0x4466fb`,
  `0x4468bc`: the `Motor` tick; `0x4f59fb`: `SpinMotor` speed =
  `trunc(command * scale)`), electric for motors, air (`0x4487bb`) for the burst
  piston's force;
* **source drain** (`0x4b40c0` / `0x4b4470`): `current -= dt * drain * avail`,
  floored at zero. At `q >= 1` that is exactly the demand `D` per step.

Consequences, all *measured* unless marked:

| situation | original |
|---|---|
| Scout, both spin motors at command 100 | `D = 200 * dt`, `scale 1.0`, `drain 0.25`, battery −200 /s |
| Scout, `Flip` (BurstMotor) | `D = 500 * dt` for 0.60-0.62 s: 301.6-310.2 asked, 291-299 paid (the last step is paid after the read) |
| ALARM, `Fire` (BurstPiston) | `D = 100 * dt` for 0.255-0.268 s: 25.5-26.8 asked, 22.8-24.4 paid, split over its two tanks |
| a robot with nothing to power | `D = 0`, scale and drain published as 0, battery constant |
| battery set to 150 (supply 0.05 < demand) | `scale = drain = 0.25`; the charge then decays exponentially (143.8 → 141.06 in 5.7 s) |
| battery set to 6500 (supply 100 dt vs demand 200 dt, `q` = 0.5) | `scale = drain = q`: 0.500, then 0.493 as it drains (27.9 /s paid vs `scale * avail` 26.5): **motor power falls linearly with the charge once `q < 1`** |
| battery set to 0 | `S = 0`, scale `0`: the motors get nothing (the robot's speed stayed ~0.1 m/s, though it was against a wall, so the *effect* is code reading) |
| air at 0 | by the same code the piston's force factor is 0 (**not** driven live) |
| battery hit points < max | supply `*= 0.5 + 0.5 hp/max` and the drain rises by the inverse (derived, not driven) |

## 4. This port

* `ra2sim::energy` is the step (`Energy::step`, `budget`, `health_factor`) with the
  sources built from `Component::power` (`PowerDef`, read from the definition in
  `rig.rs`). It knows nothing about the physics backend.
* `Robot::drive` computes this step's draws from the wired commands
  (`Robot::power_demand`), calls `Energy::step` with `physics.last_dt()`, and
  multiplies motor speed/torque by the scale (the old `motor_factor` doc said the
  budget factor was "taken as 1"; it is now this one). A burst device's draw is a
  fixed 0.6 s / 0.25 s timer from the trigger, like the original, whatever its
  stroke then takes.
* `RobotReport::{battery, air}` are the energy fractions (a robot without a tank has
  an empty air bar, like the Scout in the original) and `RobotReport::dot` is
  `min(criticals, 2)`; the HUD shows exactly one dot.
* A robot with **no source on a channel** (hand-built test rigs) is not throttled on
  it: the original would give it scale 0, but nothing stock lacks a battery.

Measured vs ours (`ra2sim --example energytrace`, `RA2_GAME=local/game`):

| quantity | original | ours |
|---|---|---|
| Scout battery cost, throttle 100 | 200 /s | 200.0 /s |
| Scout `Flip` | 301.6-310.2 asked (291-299 paid) | 300.0 |
| ALARM `Fire`, air | 25.5-26.8 asked (22.8-24.4 paid) | 26.7 |
| idle battery | constant | constant |
| Scout supply at full | 26.667 per 1/30 s (= 800 dt) | 26.667 |
| drain scale at command 100 | 0.250 | 0.250 |
| whole Scout vs ALARM match, end battery (Scout / ALARM) | ~85% / ~92% (HUD capture) | 94.6% / 86.3% |
| ALARM air at the end | ~45% | 100% (our AI never closes to fire the piston) |

A consequence worth knowing: a robot whose battery rate is below its motors' demand
(the 24000/400 Nifty with four motors: `q = charge / 24000`) loses motor power as it
drains, so `Roly Poly` (Team1 Bot0) runs at 0.95 after 10 % and a full-length fight
is noticeably weaker than the old constant-power model. That moved the shipped AIs
in the `a_deathmatch_reaches_over_with_a_winner` test by a few centimetres, enough
for the 20 s IMMOBILIZED rule to end its fight before the tabletop elimination it
exercises; the test now switches that rule off (`m.duel.immobility_active =
false`, with the reason in a comment) instead of changing any assertion. Its own
test (`a_robot_that_sits_still_is_immobilized_after_twenty_seconds`) is untouched.

The whole-match rows are *not* a validation of the model: they depend on how the
fight went (our robots do not trade blows like the original's), which is the
physics/AI work. The model itself reproduces every per-action number above.

## 5. Not recovered / not verified

* Servo motors, angle motors, linear actuators, magnets, cannons,
  flamethrowers have `*maxinoutrate` in their definitions but were not driven on
  the original; their draw here is `rate * |command| / 100` (**guess**). Weapons'
  draws (cannon air, flamethrower, magnet) are **not modelled**. A `Steering`
  unit draws `|drive command| * dt` like a spin motor (its `+0x5c`,
  `0x4f6cd0`; read from the code, docs/29 §11a), whatever its
  `elecmaxinoutrate`.
* The scale `g` reaches a spin motor's (and a steering unit's) thrust through
  its integer percentage only, not its torque (docs/29 §11b, checked on the
  Razor's live records).
* The effect of an empty tank on a piston (force factor 0) is code reading only.
* The original's `+0x80` flag (a robot with it clear gets scale 1 and no budget) is
  set for every robot seen.
* `+0xfc` (a tint multiplier the dot setter stores, `0`, `0.4`, `0.2`) is not
  applied.

## 6. The watch camera has no target (and keeps the intro's pose)

Live camera object (the game object's `+0x2a610`; fields: eye `+0x8c`/`+0x1c8`,
rotation `+0x19c`, FOV `+0x180`, **target pointer `+0x1a8`**, FOV range
`+0x290..+0x2a0`; `ra2_camera.js` polls them) in a Metal Skull exhibition,
Scout vs ALARM:

| moment | eye | rotation | FOV | target |
|---|---|---|---|---|
| intro flight | flies (-14.485,15.173,13.380) → … | (0.458,-3.486) → | 0.675 | null |
| Skip Intro → countdown `Watch Cam 1` | **(-1.26, 3.0, 4.56)** | **unchanged: (0.4377,-3.1628)** | **unchanged 0.675** | **null** |
| F4 (Watch Cam 2) | (4.87, 12.87, -0.94) | unchanged | unchanged 0.675 | null |
| F4 again (Watch Cam 3) | (4.4, 5.59, -10.47) | unchanged | unchanged | null |

* The F4 constructor `0x43a020` sets the target with `0x4014b0(0)` (decompiled:
  `index < count ? entry[index] : 0` over the 8-byte entries at `FUN_00406d10`).
  There is no human in an AI-vs-AI exhibition, so the target is **null** for the
  watch (F4) and orbit (F2) cameras. `Duel::human_robot` is that player;
  `follow_target()` is `None` without one. The earlier "look at robot 0" was
  wrong, and so was the FOV derivation: with no target the FOV range rule
  (`0x5a684f`) does not run, and the FOV stays what the arena's intro left, which
  is **0.675 rad** (`plus.setCameraFOV(0.675)` and the `animateCamera` end value
  in `Arenas/Skull.py`): the "0.65 fit" of the previous attempt.
* `Watch Cam 1` only moves the eye (`setCameraPosition`, `setCameraFOVRange`); the
  rotation is whatever the intro flight had reached when `Skip Intro` was clicked
  (our `skip@T` in `--camera-shots`). `SceneCamera::select(Watch)` already cleared
  the intro tracks, so the pose is frozen exactly there.
* The orbit camera (F2) with no target aims at the stored point `+0x1b0` = the
  origin, with no aim offset and heading 0 (`0x5a69a3` jumps to `0x5a6b74`). Live:
  eye `(0, 2.1277, -8.3326)` = `8.6 * (0, sin 0.25, -cos 0.25)`; `+0x224` reads
  **8.6**, not the constructor's 4 (`0x43aed0` stores `(0, 1, 4)`). Where the
  extra 4.6 comes from was not found (arena and robots do not change it); it is
  applied only to the no-target orbit until the user zooms
  (`ORBIT_DISTANCE_NO_TARGET`).
* The reference `arena-*-f4` still is the post-match camera (the results window is
  already up and the camera is elsewhere), so it is not comparable.

## 7. Arena stills: before / after (`tools/oracle/fidelity.py --only arena`)

MAE (0-255, whole frame) of our still against the original capture, `fidelity.py --only arena`; before = `fidelity` @ 32a55f7, after = this branch.

| arena | countdown | f2 (orbit) | f4 (watch) |
|---|---|---|---|
| box | 38.10 → 1.37 | 36.62 → 3.90 | 39.05 → 1.41 |
| bridge | 48.81 → 9.13 | 51.72 → 9.26 | 40.87 → 1.23 |
| compressor | 41.02 → 9.24 | 79.57 → 3.22 | 55.77 → 2.78 |
| electric | 72.53 → 28.56 | 63.71 → 16.70 | 72.46 → 5.95 |
| halfcircle | 30.59 → 1.08 | 61.00 → 6.71 | 37.88 → 1.87 |
| metalskull | 102.13 → 3.82 | 60.14 → 2.51 | 84.60 → 88.05 |
| octagon | 32.72 → 3.36 | 25.71 → 8.05 | 30.98 → 12.06 |
| parkinglot | 48.92 → 13.46 | 43.75 → 13.12 | 45.25 → 5.43 |
| **mean of 8** | **51.85 → 8.75** | **52.78 → 7.93** | **50.86 → 14.85** |

Unchanged (f3, f3b, intro): means 7.11 / 6.77 / 20.30.
Mean over all 48 stills: 31.61 → 10.95. Metal Skull (the target of this task): countdown 102.13 → 3.82, f2 60.14 → 2.51. Worse: Metal Skull f4 84.60 → 88.05 (its reference is the post-match camera, §6); Electric's countdown stays poor (28.56: its intro flight is not captured by a single skip-time fit).

The countdown frames need the original's `Skip Intro` moment, which is a capture
timing nuisance (each arena loads for a different time): `ARENA_SKIP` in
`fidelity.py` holds the fitted intro-clock second per arena. They are fitted
parameters, not game behaviour.

Evidence PNGs (original | ours | diff) are in
`/home/bruno/Projects/ora-wt/energy-watchcam-evidence/` (`countdown-metalskull-*`,
`f2-metalskull-*`).

## 8. Reproduce

```sh
tools/capture/original_session.sh start
# battery / air while driving (AI neutered, Throttle(100), Turn(100), Input("Flip",0,1)):
tools/oracle/ra2_hook.py tools/oracle/frida/ra2_power_step.js --seconds 25 > power.jsonl
# camera:
tools/oracle/ra2_hook.py tools/oracle/frida/ra2_camera.js --seconds 20 > camera.jsonl
# ours
cd engine && cargo run --release -p ra2sim --example energytrace -- fire $RA2_GAME/Arenas/box.py $RA2_GAME/AI/Team0/Bot0.bot Flip 3 3
```
`ra2_camera.js` reads the camera from the first call of `0x5a7460`/`0x5a6130`/
`0x5a6180`; start it before the match, or substitute the camera address (the heap
address was stable across matches in a session).
