# 26 - Fidelity baseline (the "before")

Measured 2026-10-18 by `tools/oracle/fidelity.py --gameplay` on the `fx/oracle-harness`
branch, against **genuine captures of the original** (flatpak Wine 9.0, wined3d/llvmpipe,
1024x768, `local/oracle/original/`). Nothing here uses `tools/ui_compose.py`; those
numbers (0.0001-0.45) are self-consistency, not fidelity (see README "Validation").
Every later fix should be reported as a change to this table.

Reproduce: `cd engine && cargo build --release --offline && cargo build --release --offline -p ra2sim --example fidtrace`,
then `RA2_GAME=local/game python3 tools/oracle/fidelity.py --gameplay` (about 90 s). Outputs
go to `local/oracle/fidelity/latest/` (`summary.md/json`, `ours/`, `side/` = original | ours | |diff| x4).

## Metrics

* `MAE`: mean absolute RGB difference, 0..255, over the whole frame. `masked`: same with the OS
  pointer excluded (no capture of ours has one; kept for the old `mainmenu__000`). `>8 px`: fraction of
  pixels differing by more than 8 in any channel. `lowfreq`: MAE after a 16x16 box downscale (layout and
  colour, ignoring glyph detail).
* Renderers. **app** = `ra2-app --offscreen --size 1024x768`: the shipped path. The shell is laid out
  at its fixed 1280x1005 design size and resampled to 1024x768, so it can never match a game that lays
  out natively at 1024x768. **shell** = `ra2-shell --size 1024x768`: native layout at the reference size.
  The shell column is the fairer test of the layout/art code; the app column is what a player sees.
* Match fixtures compare a *live fight* with ours (different robots positions, scores, camera), so
  only the HUD/arena look is meaningful there, not a number near zero.

## Screens

| fixture | renderer | MAE | masked | >8 px | lowfreq | status |
|---|---|---:|---:|---:|---:|---|
| mainmenu | app | 23.96 | 23.96 | 48.5% | 17.64 | ok |
| mainmenu | shell | 1.45 | 1.44 | 2.0% | 0.75 | ok |
| exhibition-settings | app | 54.81 | 54.81 | 93.4% | 53.22 | ok |
| exhibition-settings | shell | 49.06 | 49.06 | 88.0% | 47.33 | ok |
| exhibition-arena | app | 60.22 | 60.22 | 89.3% | 58.37 | ok |
| exhibition-arena | shell | 56.88 | 56.88 | 86.7% | 54.95 | ok |
| exhibition-arena-box | app | 51.17 | 51.17 | 85.7% | 49.22 | ok |
| exhibition-arena-box | shell | - | - | - | - | unreachable: only ra2-app --play drives the arena list |
| exhibition-robots | app | 52.82 | 52.82 | 93.9% | 51.23 | ok |
| exhibition-robots | shell | 47.53 | 47.53 | 88.1% | 45.78 | ok |
| exhibition-deathmatch | app | 54.23 | 54.23 | 93.3% | 51.77 | ok |
| exhibition-deathmatch | shell | 48.72 | 48.72 | 88.2% | 46.06 | ok |
| exhibition-pick-ai | app | - | - | - | - | unreachable: the Select Computer Robot dialog has no named-click route in the shell |
| exhibition-pick-ai | shell | - | - | - | - | unreachable: the Select Computer Robot dialog has no named-click route in the shell |
| exhibition-robots-filled | app | - | - | - | - | unreachable: picker clicks need pixel points; ra2-app cannot mix pixel and named clicks |
| exhibition-robots-filled | shell | - | - | - | - | unreachable: picker clicks need pixel points; ra2-app cannot mix pixel and named clicks |
| teamhq-select | app | 18.60 | 18.60 | 50.2% | 15.89 | ok |
| teamhq-select | shell | 10.04 | 10.04 | 19.9% | 9.11 | ok |
| teamhq-inventory | app | 33.14 | 33.14 | 71.7% | 29.34 | ok |
| teamhq-inventory | shell | 26.31 | 26.31 | 50.4% | 23.68 | ok |
| teamhq-standings | app | 49.86 | 49.86 | 74.2% | 44.45 | ok |
| teamhq-standings | shell | 44.38 | 44.38 | 57.4% | 39.54 | ok |
| teamhq-events | app | 50.14 | 50.14 | 75.4% | 43.49 | ok |
| teamhq-events | shell | 44.70 | 44.70 | 58.4% | 38.87 | ok |
| teamhq-history | app | 30.28 | 30.28 | 69.5% | 26.49 | ok |
| teamhq-history | shell | 24.59 | 24.59 | 48.6% | 21.95 | ok |
| botlab-overview | app | 32.73 | 32.73 | 66.4% | 29.01 | ok |
| botlab-overview | shell | 28.59 | 28.59 | 51.8% | 26.05 | ok |
| botlab-chassis | app | 30.39 | 30.39 | 63.5% | 27.89 | ok |
| botlab-chassis | shell | 23.87 | 23.87 | 45.5% | 22.25 | ok |
| botlab-components | app | 32.90 | 32.90 | 68.2% | 29.08 | ok |
| botlab-components | shell | 26.01 | 26.01 | 50.7% | 23.35 | ok |
| botlab-wiring | app | 35.32 | 35.32 | 70.5% | 29.77 | ok |
| botlab-wiring | shell | 30.59 | 30.59 | 60.6% | 25.67 | ok |
| botlab-paint | app | 37.18 | 37.18 | 74.4% | 33.76 | ok |
| botlab-paint | shell | 30.10 | 30.10 | 62.9% | 27.33 | ok |
| match-countdown | app | 42.60 | 42.60 | 96.3% | 40.94 | ok |
| match-countdown | shell | - | - | - | - | unreachable: the match is only rendered by ra2-app |
| match-hud | app | 26.98 | 26.98 | 95.1% | 25.96 | ok |
| match-hud | shell | - | - | - | - | unreachable: the match is only rendered by ra2-app |
| match-results | app | 44.49 | 44.49 | 90.8% | 41.70 | ok |
| match-results | shell | - | - | - | - | unreachable: the match is only rendered by ra2-app |
| arena-* (56 references: select/intro/countdown/f2/f3/f3b/f4 x 8 arenas) | - | - | - | - | - | unreachable: no app route to a per-arena match camera still |
| **mean of 19** | app | 40.10 | 40.10 | 77.4% | 36.80 | |
| **mean of 15** | shell | 32.86 | 32.85 | 57.3% | 30.18 | |

Reading it: the only screen near the original is `mainmenu` on the shell path (1.45; this is the
historic 1.439 number). The app path is 24 on the same screen (the 1280x1005 resample). Every other
shell screen is 10-60: the exhibition setup is the worst (about 50-60; the shell at 1024x768 lays the
panels out wrongly: see `side/shell/exhibition-*.png`), Team HQ/bot lab 10-45. Not reachable by our app
today: the Select Computer Robot dialog, filled robot slots, per-arena static cameras (56 original
stills of the eight stock arenas exist as references), the match HUD with the original's camera.

## Gameplay (Scout vs ALARM, Combat Arena, DEATHMATCH, no human input)

Original fight length used: 114.8 s.  Ours: fight 114.81s, reason Some(Timeout), winners [], points [0, 0]

| robot | metric | original | ours |
|---|---|---:|---:|

| damage | original | ours |
|---|---:|---:|
| events | 8.00 | 0.00 |
| weapon_events | 3.00 | 0.00 |
| env_events | 5.00 | 0.00 |
| events_per_s | 0.07 | 0.00 |
| total | 165.69 | 0.00 |
| magnitude <5 | 0 | 0 |
| magnitude 5-10 | 0 | 0 |
| magnitude 10-20 | 7 | 0 |
| magnitude 20-50 | 0 | 0 |
| magnitude >=50 | 1 | 0 |

Drive test (drive-scout-box__live__000.tsv):

| metric | original | ours |
|---|---:|---:|
| turn_rate_rad_s | 10.66 | -1.33 |
| turn_rate_peak | 14.94 | 1.64 |
| top_speed | 11.12 | 2.26 |
| t_to_90pct_s | 1.58 | 1.70 |
| straight_distance_m | 15.42 | 2.77 |
| speed_at_release | 10.86 | 0.10 |
| coast_distance_m | 8.05 | 0.01 |

Notes on the numbers.

* Original side: `match-trace-box-scout-alarm__live__002.tsv` (one whole match, 114.8 s of fight) and
  `match-damage__live__000.jsonl` (8 calls into the damage sink; weapon hits are logged on both
  participants, hence pairs). Ours: `ra2sim --example fidtrace` (`Match` with the shipped AI, same bots,
  same length), the same heading/speed conventions (`atan2(fx,fz)`; forward velocity).
* **The original's robots fight and move; ours barely do** (ALARM never moves, Scout drifts ~27 m, no
  damage in 115 s). The AI bring-up in our sim is the largest gameplay gap.
* **Hitpoints are on a different scale**: the original's `plus.getHitpoints(p,0)` is 1828 (Scout) / 2662
  (ALARM) at the start; ours reports the chassis health in 300 / 350.
* Our damage events are per physics step in which `damage_taken` grew (one number per step per
  victim); the original's are per call of `FUN_004175b0`. Compare counts and magnitudes loosely.
* Drive test: the original Scout reaches ~11 m/s in ~1.6 s and spins in place at ~10 rad/s; ours reaches
  ~2.3 m/s and turns at ~1.3 rad/s. The motor model is far too weak/slow (see
  `drive-scout-box__live__000.tsv`; the straight leg was released at ~2.2 s so 15 m is the original).
  The original was driven by `SuperAI.Throttle(100)/Turn(100)` (the commanded values are columns in
  the trace) with both AI brains neutered; ours uses `DriveInput {throttle 1, steer 1}`.

## Ground truth added for other workstreams

| what | where | produced by |
|---|---|---|
| 8 stock arenas x (picker select, intro fly-by, countdown, F2 orbit, F3 static 1, F3 again = static 2, F4 watch) 1024x768 | `original/arena-<id>-<shot>__1024x768__000.png` (id: bridge, box, compressor, electric, halfcircle, metalskull, parkinglot, octagon) | `tools/oracle/capture_arenas.py` |
| Deterministic-ish drive test (turn in place, coast, full throttle, stop; ~13 Hz; command columns) | `original/drive-scout-box__live__000.tsv/.json` | `tools/oracle/capture_drive.py` |
| Weapon test: inert Scout vs ALARM's Poker AI, per-component health, damage hook | `original/weapon-alarm-vs-inert-scout__live__000.tsv/.jsonl` | `tools/oracle/capture_weapon.py` |
| Audio: main menu music (13 s), match (18 s) from the game's own stream | `original/audio/*.wav` | `tools/capture/record_audio.sh` |
| Reproducibility runs | `original/repro-{none,seed}__live__00{0,1}.tsv` | `tools/oracle/capture_repro.py` |

Limits of the weapon test: only Red Zone robots can be picked (the team drop-down of the Select Computer
Robot dialog does not open under our mouse injection), so no spinner/hammer attacker; the AI attacker
landed only two weapon hits (20.0 and 25.2) before idling. Pre-fight damage already present at the first sample (Scout
at 0.83).

## Audio

`tools/capture/record_audio.sh OUT.wav [SECONDS]` records only the game's PipeWire stream:
`parec --monitor-stream=<sink-input index of "Robot Arena 2.exe">` (48 kHz stereo s16). It works; both
captures are non-silent (rms about 2600, peak up to full scale), and it does not pick up host audio.

## Reproducibility

Matches are not reproducible run to run. Findings (2026-10-18):

* The exe has no `srand`/`rand` symbols (static CRT). The VS CRT `_rand` is at `0x63dcf2` (found by the
  LCG constant `0x269ec3`; the seed lives in per-thread data `+0x14`); its only callers are a
  `std::random_shuffle` over collision triangles (`0x546f70`) and `0x620b50`, `0x62c430`. The shipped
  Python arenas use the `random` module (announcer choice, `Hazards.py` timing).
* `tools/oracle/frida/ra2_seed.js` replaces `_rand` with a fixed-seed LCG, and `random.seed(1234)` can be run
  in-game before Start Game (`capture_repro.py --pin seed`).
* Two identical matches (Scout vs ALARM, no input) drift apart immediately without pins (robot 1 up to
  9 m apart at 6 s of fight time; 2-3 m at 10-20 s) and still do with both pins (robot positions
  differ by 1-2 m at 6 s and 3-6 m at 20 s), though less early. The remaining nondeterminism is
  not the RNG: the game's time source (frame dt under llvmpipe, `plus.getFPS()` ~17-80) and AI timers are
  wall-clock driven. `plus.setTimestep`/`setSubsteps` exist and were **not** tried. Consequence: compare
  distributions, not trajectories; scripted-input tests (the drive test) are the reproducible kind.

## After wave 1 (fx/damage, fx/wiring, fx/render-core, fx/font merged)

Measured with the same command on branch `fidelity`. MAE, before (baseline above) -> after.

| fixture | renderer | before | after |
|---|---|---:|---:|
| mainmenu | app | 23.96 | 24.13 |
| mainmenu | shell | 1.45 | 0.59 |
| exhibition-settings | app | 54.81 | 21.26 |
| exhibition-settings | shell | 49.06 | 16.52 |
| exhibition-arena | app | 60.22 | 27.58 |
| exhibition-arena | shell | 56.88 | 24.25 |
| exhibition-arena-box | app | 51.17 | 25.50 |
| exhibition-robots | app | 52.82 | 22.29 |
| exhibition-robots | shell | 47.53 | 19.53 |
| exhibition-deathmatch | app | 54.23 | 21.42 |
| exhibition-deathmatch | shell | 48.72 | 17.25 |
| teamhq-select | app | 18.60 | 18.85 |
| teamhq-select | shell | 10.04 | 10.44 |
| teamhq-inventory | app | 33.14 | 33.52 |
| teamhq-inventory | shell | 26.31 | 26.70 |
| teamhq-standings | app | 49.86 | 50.15 |
| teamhq-standings | shell | 44.38 | 44.50 |
| teamhq-events | app | 50.14 | 50.27 |
| teamhq-events | shell | 44.70 | 44.79 |
| teamhq-history | app | 30.28 | 30.65 |
| teamhq-history | shell | 24.59 | 24.94 |
| botlab-overview | app | 32.73 | 32.85 |
| botlab-overview | shell | 28.59 | 28.46 |
| botlab-chassis | app | 30.39 | 30.43 |
| botlab-chassis | shell | 23.87 | 23.63 |
| botlab-components | app | 32.90 | 32.95 |
| botlab-components | shell | 26.01 | 25.83 |
| botlab-wiring | app | 35.32 | 35.30 |
| botlab-wiring | shell | 30.59 | 30.35 |
| botlab-paint | app | 37.18 | 37.22 |
| botlab-paint | shell | 30.10 | 29.88 |
| match-countdown | app | 42.60 | 39.18 |
| match-hud | app | 26.98 | 44.35 |
| match-results | app | 44.49 | 56.53 |
| **mean of 19** | app | 40.10 | 33.39 |
| **mean of 15** | shell | 32.86 | 24.51 |

Reading it: the shell path improved on every screen (font layout and borders: exhibition screens
~50 -> ~17-24, mainmenu 1.45 -> 0.59). The app path improved less since it still resamples the
1280x1005 layout. Regressions: match-hud 26.98 -> 44.35 and match-results 44.49 -> 56.53; the render-core
left-handed view with per-material culling now shows the arena brightly lit from a different camera,
where the original frame is nearly black, so the number moves with the camera/lighting, not the HUD.
Exhibition settings: the mode artwork panel is empty in ours.

### Gameplay after wave 1 (Scout vs ALARM)

Ours now ends at 21.0 s (LastRobot, winner robot 0) instead of timing out at 114.8 s.

| metric | before | after | original |
|---|---:|---:|---:|
| drive top_speed m/s | 2.26 | 2.31 | 11.12 |
| turn_rate rad/s | -1.33 | -2.25 | 10.66 |
| t_to_90pct s | 1.70 | 2.00 | 1.58 |
| straight_distance m | 2.77 | 2.99 | 15.42 |
| coast_distance m | 0.01 | 0.98 | 8.05 |
| robot 0 speed_max | - | 1.39 | 9.19 |
| robot 1 speed_max | - | 0.01 | 7.46 |
| damage events | 0 | 0 | 8 |
| hp_start (robots 0/1) | 300/350 | 300/350 | 1828/2662 |

Robots are still about 5x too slow, so no contact point reaches the original's >=10 gate and no damage
is applied. The turn sign is now negative (left/right mirroring through the wiring) while the original's is positive.


## After physics (fx/physics)

Same harness (`tools/oracle/fidelity.py --gameplay`), Scout drive test
(`drive-scout-box__live__000`). `fidtrace` now measures speed along the robot's own
front (`rig.forward()`), not a fixed axis.

| metric | original | baseline | after physics |
|---|---:|---:|---:|
| turn_rate_rad_s (in place) | 10.66 | -1.33 | 8.4 (-21 %) |
| top_speed (m/s) | 11.12 | 2.26 | 11.5 (+3 %) |
| t_to_90pct_s | 1.58 | 1.70 | 1.60 |
| straight_distance_m | 15.42 | 2.77 | 15.9 (+3 %) |
| speed_at_release | 10.86 | 0.10 | 11.5 |
| coast_distance_m | 8.05 | 0.01 | 5.9 (-26 %) |

Traction survey (45 stock bots, 3 s at full throttle, >= 2 m): 35/45 -> 43/45.
Chassis hitpoints now read 1828 (Scout) and 2662 (ALARM), as in the original.

What is recovered and what is fitted:

* Recovered: chassis hitpoints = trunc(area x hardness) (`0x44d86d`); motor power
  degradation with damage (`FUN_0045fa90`); friction combine mu = sqrt(muA mu B)
  (`0x540820`, replaces the sum law); component masses from every rigid body of a
  `.gmf` (a mini wheel is 17 kg, not 8); centre of mass at the mass-weighted mean
  of the components.
* Inferred from the original's rest heights: a 0.045 m skin on every robot hull
  (0.9 x the Havok tolerance 0.1 halved once; Scout rests at y = -1.895 vs -1.884,
  ALARM without its hammer at -1.80 vs -1.811), round wheel hulls.
* Fitted (named constants in `robot.rs`): wheel drag rate 4.3 (per unit
  `resistance`, times robot mass), sideways grip rate/cap, wheel coupling rate.
  The thrust itself (motor torque / wheel radius, no friction limit, no speed
  limit) is read off the drive trace: 2 x 560 N.m on 0.33 m wheels is 15.2 m/s^2
  and the trace starts at 13.8 m/s^2.
* Not done: the original's `g` power factor (robot +0x9c/+0xa0, taken as 1); the
  hinge-servo `k` (refuted live, k = 1.02); gravity -12 and the 1/30 s step with
  two substeps (recovered statically, not applied here).

## After havok17 (fx/havok17)

`RA2_PHYSICS=havok17` (opt-in; see `docs/29-havok17-backend.md`): Havok 1.7
dynamics written from the recovered specification, with the game's own drive
model (`MagicAction`, `0x4856f0`) instead of the fitted wheel thrust. Drive tests
run from the original's own start poses (`fidtrace drive-at`).

| metric | original | Jolt + interim | havok17 |
|---|---:|---:|---:|
| Scout turn_rate_rad_s (in place) | 10.66 | 8.68 | 10.64 |
| Scout top_speed (m/s) | 11.12 | 12.4 | 11.04 |
| Scout t_to_90pct_s | 1.58 | 1.6 | 1.6 |
| Scout straight_distance_m | 15.42 | 17.44 | 15.71 |
| Scout coast_distance_m | 8.05 | 7.36 | 5.34 |
| Scout rest height | -1.884 | -1.896 | -1.932 |
| ALARM top speed (1.65 s, m/s) | 13.38 | 7.66 | 5.61 |
| ALARM turn in place (rad/s) | 12.1 | 3.98 | 8.94 |
| ALARM rest height | -1.811 | -1.807 | -1.832 |
| traction survey (45 bots) | - | 44/45 | 36/45 |

Nothing in havok17's drive is fitted. Both backends now spawn a burst piston
cocked (ALARM rested on its hammer: -1.63, now -1.807). Jolt stays the default:
havok17 loses the multi-robot comparison because our robot assembly puts some
wheels (ALARM's rear axles, 0.19 m) and parts (ALARM's tanks) where the
original does not — the live wheel records in docs/29 §6 are the reference.

## After assembly (fx/assembly)

Robot assembly now follows the original's own rule, measured live on 14 stock
robots (`docs/29-havok17-backend.md` section 8, `tools/oracle/capture_assembly.py`):
a chassis child's own mount point lands on its stored translation, a link plugs
the child's point into the parent's with a half turn about the point's Y, a
burst motor's rotor is built at its start angle, axle mounts and steering hubs
are bodies of their own. Every mesh of the 14 robots is within 8 mm of the
original as built (before: mean 0.13-1.2 m per robot, worst 2.3 m), pinned by
`cargo test -p ra2sim --test assembly`.

Drive tests from the original's start poses (`fidtrace drive-at`; sim time):

| metric | original | Jolt + interim | Jolt + MagicAction | havok17 |
|---|---:|---:|---:|---:|
| Scout top speed (m/s) | 11.12 | 12.92 | 11.1 | 11.11 |
| Scout straight distance (m) | 15.42 | 17.97 | 16.85 | 15.72 |
| Scout coast (m) | 8.05 | 8.12 | 5.2 | 5.83 |
| Scout turn in place (rad/s) | 10.66 | 10.27 | 10.77 | 10.59 |
| ALARM top speed (1.65 s, m/s) | 13.38 | 9.81 | 11.82 | 14.22 |
| ALARM distance / coast (m) | 14.11 / 11.91 | 11.41 / 4.19 | 13.39 / 3.05 | 14.4 / 9.05 |
| ALARM turn in place (rad/s) | 12.1 | 5.33 | 10.41 | 10.57 |
| FlapJack top / turn | 9.92 / 10.16 | 11.69 / 5.43 | 8.7 / 7.99 | 9.95 / 9.56 |
| Razor top / turn | 16.07 / 12.2 | 12.03 / 8.48 | 15.94 / 12.23 | 16.52 / 12.25 |
| Wide Load top / turn | 13.3 / 12.37 | 5.46 / 3.55 | 14.46 / 11.33 | 13.44 / 11.99 |
| rest, bot origin y (Scout / ALARM) | -2.086 / -2.074 | -2.009 / -1.983 | -2.009 / -1.983 | -2.100 / -2.075 |
| traction survey (45 bots) | - | 38/45 | 45/45 | 45/45 |

(The rest heights quoted before, -1.884 / -1.811, were `plus.getLocation`, the
chassis body's centre of mass, not the bot origin ours reports.)

havok17 is now the closest on every robot except the coast (our hubs skid,
docs/29 section 6 item 2). It is not the default yet: four tests that need
havok17 features unrelated to driving (parked burst lock, piston creep, arena
prop pin/unpin, script-driven arena hinges) fail with it; see docs/29 section 5.
The interim Jolt wheels lose six survey robots now that the wheels sit where
the original puts them (44/45 before).

## After havok-default (fx/havok-default)

`havok17` is the default backend; `RA2_PHYSICS=jolt` is the fallback.
What it took (`docs/29-havok17-backend.md` section 9): the game's `Lock` as
the original does it (both limits at the current angle, `FUN_00480e20`; the
prismatic `FUN_004b50c0`; self-locking while the command is zero,
`FUN_00481450`/`FUN_004b5240`), the arenas' disabled collision pairs
(`GMID_HAVOK_DIS_COLLISION_PAIRS`, which free the Parking Lot's trap doors),
and the hazard auto-lock judged by the sense the joint is driven in. On Jolt,
the interim wheel sampler no longer mistakes the robot's own wheel for "no
ground" (seven robots stood still).

Drive tests from the original's start poses (`fidtrace drive-at`; sim time):

| metric | original | havok17 (default) | Jolt + interim |
|---|---:|---:|---:|
| Scout top speed (m/s) | 11.12 | 11.31 | 12.92 |
| Scout straight distance / coast (m) | 15.42 / 8.05 | 15.86 / 5.85 | 17.97 / 8.12 |
| Scout turn in place (rad/s) | 10.66 | 10.71 | 10.27 |
| ALARM top speed (1.65 s, m/s) | 13.38 | 12.57 | 9.81 |
| ALARM distance / coast (m) | 14.11 / 11.91 | 13.91 / 7.44 | 11.41 / 4.13 |
| ALARM turn in place (rad/s) | 12.1 | 11.04 | 5.33 |
| FlapJack top / turn | 9.92 / 10.16 | 10.66 / 9.42 | 11.69 / 5.43 |
| Razor top / turn | 16.07 / 12.2 | 16.52 / 12.25 | 12.03 / 8.48 |
| Wide Load top / turn | 13.3 / 12.37 | 13.39 / 13.25 | 5.46 / 3.55 |
| rest, bot origin y (Scout / ALARM) | -2.086 / -2.074 | -2.086 / -2.075 | -2.009 / -1.983 |
| traction survey (45 bots) | - | 45/45 | 45/45 |

Open: the coast (hubs skid; docs/29 section 6 item 2 has the measurement
pointing at the corner manifold), the battery power factor, steering
records, and the AI: in nine AI-vs-AI matches on five arenas the robots
mostly do not engage — every match ends early on an immobilised robot and
only one (Octagon, Razor vs FlapJack) scores (two hits >= 10, 35 points). Jolt
does no better on the same pairings, so this is the AI bring-up gap recorded
above, not the backend.
