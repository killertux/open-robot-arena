# 27 — The match cameras

Recovered from `Robot Arena 2.exe` (Ghidra session + linear disassembly) and
checked against the original captures in `local/oracle/original/arena-*`.

## 1. One camera, three configurations

The game object owns a single main camera (`+0x2a610`; constructor defaults at
`0x5a5c72`: FOV pi/4, aspect 4:3, near 1.0, far 2000.0). Every `plus` camera
function edits it. `0x4e3a90..0x4e3a9a` (the options code) then sets near 1.0 /
far 125.0 (`0x58fc90`).

The match's key handler `0x439c10` dispatches `0x71`..`0x73` (**F2, F3, F4**;
F1 is help) to `0x439cb0`, which re-configures that one camera:

| key | constructor | mode | what it sets | label |
|-----|-------------|------|--------------|-------|
| F2 | `0x439d60` | 2 (orbit) | FOV pi/4, aim offset `(0, 1)`, distance 4 (`+0x21c..0x224`), pitch -0.25, yaw = robot 0's heading, target = player 0 | literal `Orbit Cam` (`0x6f102c`) |
| F3 | `0x439f20` | 4 (scripted) | pos (-12.2, 5.478, 5.162), rot (0.2933, 1.575), FOV 0.9, targets cleared; then the arena's `NextStaticCamera()` | the name it returns (`Static 1`, `Overhead`, ...) |
| F4 | `0x43a020` | 4 + target | pos (-4.2, 2.5, 9.5), FOV range (2, 20, 0.8976, 0.3491), target = player 0 (`0x4014b0(0)`, `0x5a6650`); then `NextWatchCamera()` | the name it returns (`Watch 1`, `Pole 1`, ...) |

`NextStaticCamera` / `NextWatchCamera` are the Python `Arenas/__init__.py`
methods: they advance an iterator over `AddStaticCamera` / `AddWatchCamera`
records and call `plus.setCameraPosition/Rotation/FOV` (static) or
`setCameraPosition/FOVRange` (watch). The iterators restart when the intro ends
(`DoIntro(False)`), so the countdown's default `watch` camera takes `Watch 1`, a
later F4 gets the *next* one, and the first F3 is `Static 1`, the second
`Static 2` (the `f3` / `f3b` captures). The camera-type codes are stored at
`+0x214` of the manager (`0x71`/`0x72`/`0x73`).

## 2. The `plus` functions

| call | wrapper -> camera | meaning |
|------|-------------------|---------|
| `setCameraPosition(x, y, z)` | `0x4bb670 -> 0x58fbd0 -> vtbl+0x14 (0x5a6130)` | eye, world units, set immediately |
| `setCameraRotation(pitch, yaw)` | `0x4bb690 -> 0x5a6180` | radians, roll 0, set immediately (current, target and previous copies) |
| `setCameraFOV(f)` | `0x4bb6b0 -> 0x58fcc0` | **vertical** FOV in radians; `0x5a7460` rebuilds the projection |
| `setCameraFOVRange(min_d, max_d, fov_near, fov_far)` | `0x4bb6d0 -> 0x58fce0` | enables `+0x290`; with a target the FOV is `fov_near + (fov_far - fov_near) * clamp((d - min_d) / (max_d - min_d), 0, 1)` (`0x5a684f`), d = eye-to-target distance |
| `animateCamera(pos0, rot0, fov0, pos1, rot1, fov1, t0, t1)` | `0x4bb700` | three linear events queued on the camera: position `0x5a38d0`, rotation `0x5a3df0`, FOV `0x5a3ec0`. `t0`/`t1` are seconds **after the call**; nothing happens before `t0`, a lerp `(t - t0) / (t1 - t0)` between, one exact write of the end value when `t > t1` |

`animateCamera`'s events are separate objects owned by the scene, so they keep
running after the script that queued them is gone (the intro generator is
dropped on skip, the already-queued flight is not).

## 3. View and projection

Mode 4 (`0x5a66da`): forward = `D3DXMatrixRotationYawPitchRoll(yaw = rot.y,
pitch = rot.x, roll = rot.z)` applied to `(0, 0, 1)`, i.e. `(cos p sin y, -sin p,
cos p cos y)`; the view is `D3DXMatrixLookAtLH(eye, eye + forward, (0, 1, 0))`
(`0x5a6938`, the up vector is world +Y). With a target (`+0x1a8`) the aim point
is the target's position (`+0x8c`) instead. The projection is
`D3DXMatrixPerspectiveFovLH(fov, aspect, near, far)` (`0x50df36`: `_11 = cot(fov/2) / aspect`,
`_22 = cot(fov/2)`).

An overhead camera is a degenerate look-at; the sign of the tiny horizontal
component decides which way is up. `compressor.py`'s `Overhead` is pitch 1.5708
(a hair past pi/2): `cos p` is negative and the picture comes out with +Z up,
exactly like the capture. `ra2sim::camera::euler_forward` reproduces it.

Verified against the captures: `box` Static 2 (`pi/2.01`, `fmod*37`) lines up
with the original to the pixel (rim at x 170..855, y 45..730), which also pins
the FOV as vertical with an aspect of 4:3.

## 4. Phases

1. **Intro** (`ArenaIntros=1`): the match shell calls the arena's `DoIntro(True)`
   with the camera in its static configuration (the HUD label is `Static 1`),
   shows the `Skip Intro` button, and the arena's `Introduction` generator (run
   by the arena `Tick`, every 0.25 s) places the camera and queues
   `animateCamera` flights. `plus.introComplete()` (from `DoIntro(False)`) or the
   skip ends it. No HUD but the skip button is drawn.
2. **Countdown**: `ra2.cfg`'s `DefaultCamera` (`watch`), label `Watch 1`.
3. **Fight / results**: F2/F3/F4 as above.

## 5. This port

* `ra2script::camera` records the `plus` requests (`take_commands`) and calls
  the arena's `Next*Camera` (`next_camera`).
* `ra2sim::camera::SceneCamera` is the one camera: `select(kind)` = the key
  handler's constructor defaults, `apply(command)`, `advance(dt)` (the tracks),
  `pose()` / `view_proj(aspect)`. `Duel::select_scene_camera` is what the
  F-keys call; `Duel::advance` drains the script queue every step.
* `Match` gains `MatchState::Intro` (entered by `begin_intro`, left when the
  script calls `introComplete`).
* `ra2-app --match --out DIR --camera-shots intro@2.5,countdown@2,f2@5,f3@7,f3b@9,f4@11`
  writes `cam-<name>.png`; `tools/oracle/fidelity.py` runs it for the eight
  `arena-<id>-*` fixtures.

## 6. Not recovered / approximated

* **Orbit (F2)** follows robot 0 from 4 units behind and 1 above its aim point
  at pitch 0.25 and its heading (the values the constructor stores). The
  smoothing (`0x61bb70`), mouse look/zoom and the camera-blocker ray tests
  (`0x5a7680`, `AddCameraBlocker`) were not reproduced; the capture shows the
  robots smaller than this renders them, so the distance or lens is off.
* **Watch** looks straight at robot 0's position; nothing else about its
  target selection was found (`NextAutoCamera` is a stub returning `""`).
* Three arenas whose `extent` is not centred on the origin (`Bridge`, `half`,
  `Skull`) are framed with the right scale and orientation but sit about 5
  units / 0.2 rad off the capture; the other five line up exactly. Cause not
  established.
