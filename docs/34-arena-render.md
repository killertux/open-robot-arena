# 34 — Arena render fidelity (the 48 arena stills)

Recovered from `Robot Arena 2.exe` (REA/Ghidra session, rizin disassembly) and measured against the
original's captures in `local/oracle/original/arena-*` with `tools/oracle/fidelity.py --renderer app
--only arena-,match- --out <dir>` (side images: reference | ours | |diff| x4). Implementation follows
the recovered behaviour; no decompiled code was transliterated.

## 1. Results

MAE (0-255, whole 1024x768 frame). *before* is this branch's base (fidelity 7178ea9), *after* this
round; *lowfreq* is the after MAE on a 16x16 box-downscaled pair (layout and colour without
pixel-level detail). The docs/28 §3j table was measured on an earlier build; the base run here
differs from it in a few dynamic shots (`metalskull-f2` 19.39 there, 27.37 here: fight state).

| fixture | before | after | after lowfreq |
|---|---:|---:|---:|
| match-countdown | 34.79 | 1.70 | 0.86 |
| match-hud | 0.49 | 0.30 | 0.12 |
| match-results | 0.88 | 0.72 | 0.47 |
| arena-box-countdown | 0.40 | 0.07 | 0.06 |
| arena-box-f2 | 4.13 | 2.62 | 1.96 |
| arena-box-f3 | 1.72 | 0.61 | 0.32 |
| arena-box-f3b | 2.90 | 1.20 | 0.79 |
| arena-box-f4 | 0.65 | 0.36 | 0.19 |
| arena-box-intro | 5.55 | 0.66 | 0.27 |
| arena-bridge-countdown | 7.87 | 3.78 | 2.18 |
| arena-bridge-f2 | 8.69 | 5.60 | 5.17 |
| arena-bridge-f3 | 7.13 | 1.28 | 0.94 |
| arena-bridge-f3b | 7.65 | 1.64 | 1.32 |
| arena-bridge-f4 | 0.53 | 0.41 | 0.21 |
| arena-bridge-intro | 16.90 | 4.12 | 1.86 |
| arena-compressor-countdown | 7.92 | 1.53 | 0.29 |
| arena-compressor-f2 | 2.38 | 0.38 | 0.16 |
| arena-compressor-f3 | 6.69 | 0.75 | 0.44 |
| arena-compressor-f3b | 8.53 | 1.06 | 0.67 |
| arena-compressor-f4 | 2.05 | 0.40 | 0.15 |
| arena-compressor-intro | 21.59 | 4.58 | 0.95 |
| arena-electric-countdown | 26.71 | 10.24 | 2.03 |
| arena-electric-f2 | 16.07 | 1.33 | 0.95 |
| arena-electric-f3 | 9.87 | 1.23 | 0.65 |
| arena-electric-f3b | 8.84 | 1.05 | 0.73 |
| arena-electric-f4 | 6.22 | 4.62 | 4.27 |
| arena-electric-intro | 38.77 | 12.01 | 2.58 |
| arena-halfcircle-countdown | 0.03 | 0.03 | 0.03 |
| arena-halfcircle-f2 | 10.37 | 8.55 | 7.85 |
| arena-halfcircle-f3 | 2.98 | 1.12 | 0.71 |
| arena-halfcircle-f3b | 2.82 | 1.04 | 0.55 |
| arena-halfcircle-f4 | 1.01 | 0.34 | 0.13 |
| arena-halfcircle-intro | 19.76 | 2.40 | 1.27 |
| arena-metalskull-countdown | 1.48 | 1.19 | 0.88 |
| arena-metalskull-f2 | 27.37 | 26.16 | 25.80 |
| arena-metalskull-f3 | 4.40 | 1.74 | 1.48 |
| arena-metalskull-f3b | 5.41 | 2.02 | 1.74 |
| arena-metalskull-f4 | 87.69 | 87.92 | 81.44 |
| arena-metalskull-intro | 18.84 | 1.37 | 0.91 |
| arena-octagon-countdown | 1.99 | 1.85 | 0.66 |
| arena-octagon-f2 | 7.71 | 6.24 | 5.34 |
| arena-octagon-f3 | 4.30 | 1.65 | 1.24 |
| arena-octagon-f3b | 2.68 | 0.97 | 0.64 |
| arena-octagon-f4 | 11.36 | 7.59 | 7.15 |
| arena-octagon-intro | 13.33 | 1.69 | 1.06 |
| arena-parkinglot-countdown | 11.94 | 4.65 | 0.96 |
| arena-parkinglot-f2 | 17.08 | 18.22 | 16.93 |
| arena-parkinglot-f3 | 11.93 | 1.93 | 1.57 |
| arena-parkinglot-f3b | 9.98 | 1.76 | 1.50 |
| arena-parkinglot-f4 | 4.61 | 1.87 | 1.75 |
| arena-parkinglot-intro | 27.00 | 3.52 | 0.85 |
| **mean of 51** | **11.02** | **4.90** | 3.79 |

Above MAE 8 before: 22 shots. After: 6 (`electric-countdown`, `electric-intro`, `halfcircle-f2`,
`metalskull-f2`, `metalskull-f4`, `parkinglot-f2`); the last four are robot pose / match state, §7.

Second round (section 9): the 51 old references are unchanged (mean 4.55 at this base, no render change;
the 4.90 above was measured at the previous base) and are marked unreliable where they were timed by
wall clock. The 18 new references measure at mean **2.32**: countdown2 0.03-2.80, match-countdown2
0.11, metalskull-f4live 0.33, intro2 1.52-8.60 (with the camera-clock rule; 12.3-58.3 with the game's
intro clock taken at face value). Full run: 69 shots, mean 3.97.

## 2. Triage of the shots above MAE 8 (before)

| shot | before | cause(s) seen in the side image |
|---|---:|---|
| match-countdown | 34.79 | **reference is not a countdown** (Skip Intro button, no COMPETITORS READY panel): timing (§6) |
| bridge-f2 | 8.69 | robot pose (Scout in view only in ours); missing specular on the tanks (§3.4) |
| bridge-intro | 16.90 | intro clock (§5); specular; half-pixel |
| compressor-f3b | 8.53 | fence bars drawn as opaque grey slabs (sub-material id, §3.1) |
| compressor-intro | 21.59 | the same slab under the foreground fence; intro clock |
| electric-countdown | 26.71 | bars walls dark with a triangle seam (normals, §3.2) and with Electric's own gradient bitmap where the capture shows Compressor's (§3.3); Skip-Intro clock |
| electric-f2/f3/f3b | 16.07/9.87/8.84 | same bars walls; generators dull (specular) |
| electric-intro | 38.77 | bars + intro clock |
| halfcircle-f2 | 10.37 | robot pose (orbit camera, no target: aims at the origin) |
| halfcircle-intro | 19.76 | intro clock; saw plates yellow where the capture's are Box's orange (§3.3) |
| metalskull-f2 | 27.37 | robot pose: the robots are in view only in ours |
| metalskull-f4 | 87.16-87.69 | **reference taken after the match ended**: the Match Results window is up (ALARM ELIMINATED, 490 points) — match state, not render |
| metalskull-intro | 18.84 | intro clock |
| octagon-f4 | 11.36 | a loose floor plate and a robot in other positions (physics state) |
| octagon-intro | 13.33 | intro clock |
| parkinglot-countdown | 11.94 | a dark diagonal band across the concrete wall (normals, §3.2); asphalt aliasing |
| parkinglot-f2 | 17.08 | robot pose |
| parkinglot-f3/f3b | 11.93/9.98 | normals; specular; half-pixel |
| parkinglot-intro | 27.00 | intro clock |

## 3. Recovered render rules

### 3.1 Face id -> sub-material (`FUN_005ae630`, `0x5ae8f0..0x5ae913`)

The batch builder walks the sub-materials `0..n-1` and, per pass, takes the faces whose id equals the
pass. A face id is first clamped: `cmp ebx, count; jle` keeps ids `<= n`, anything above becomes
`n - 1`. So an id **above** the count draws with the last sub-material and an id **equal** to the
count matches no pass and is never drawn (a material without sub-materials draws every face).
We used to fall back to the parent material, which drew the compressor's fence bars (face id 3 of
the 2-entry `Metal` material) as an untextured, opaque grey slab. Shipped faces affected: the
compressor `Fence` (24 faces, id 3 of 2), four prize photos (id > n) and `Spike_Trophy` (32 faces with
id == n, now not drawn). `gmi::Material::resolve_face` returns `Option`.

### 3.2 Stored normals are face, A, B, C

The GMF stores four normals per face. The exe's own GMA writer (`0x5c2568..0x5c265b`, which prints the
in-memory face record) emits `*MESH_FACENORMAL` from `+0` and `*MESH_VERTEXNORMAL` from `+0xc`,
`+0x18`, `+0x24`, i.e. the face normal first, then corners A, B, C; the shipped `*_txt.gmf` files
agree. We read `normals[4f + corner]`, so corner A took the face normal and B, C the wrong corners:
every smoothed quad showed a lighting seam along its diagonal (Electric's bar walls, Parking Lot's
concrete wall band). Now `normals[4f + 1 + corner]`, falling back to the face normal.

### 3.3 The texture manager keeps the first file per name for the whole session

`FUN_005ae630` asks for every stage bitmap as `DAT_0071f574 + <name as written in the GMF>`; the
global path is a std::string that only the bot lab changes (`FUN_00593a40`, from `0x42e650`,
`UI\BotLab\Maps`). **Correction (live, section 9):** the static initialiser `FUN_0058df20` mentions
`Graphics\`, but the string is empty at run time: all 573 arena-load requests had the prefix `""`, so
the manager's key is the bare name as written in the GMF (`sawhazard.bmp`). `session_bitmap` keys
on `texture_path() + name` with an empty default; `set_texture_path` is the bot lab's switch. The texture manager `FUN_00591030` first scans its live list
comparing each texture's name with that string (`FUN_0058e390`, exact byte compare) and returns the
hit; only a miss loads a file (`FUN_005a8940` -> `FUN_005a8bd0` -> `FUN_005914d0`, which has its own
by-name image cache). The directory a model came from is not part of the key, so the first file
loaded under a name is what every later model with that name gets.

Pixel evidence in the captures, which were taken in one session in the order bridge, box,
compressor, electric, halfcircle, metalskull, parkinglot, octagon (`capture_arenas.py`):

* Electric's `bars.tga` has a vertical brightness ramp (row peaks 11 -> 251); Compressor's is flat
  (251 on every row). Down the Electric countdown's bar wall the reference's bar peaks are
  238 178 214 225 226 232 213 223 218 222 224 219 (flat), ours with Electric's own file were
  238 28 45 66 81 108 122 151 172 189 205 214 (the ramp).
* Half Circle's saw plates (`sawhazard.bmp`) are yellow-striped, Box's orange-striped: the
  `arena-halfcircle-f3b` reference shows them orange.

Three shipped names differ between directories (`bars.tga`, `sawhazard.bmp`, `triple_spike.bmp`, the
last by a few counts in green); no component or robot texture collides. Implemented as
`ra2_render::material::session_bitmap` (process-wide, keyed by the GMF's bitmap string; a failed load
is not cached, as the manager deletes it). `ra2-app --match --earlier-arenas A.py,B.py` loads the
arenas played before in the same session, and `fidelity.py` passes the capture order
(`CAPTURE_ORDER`). Section 9 checks the scope live.

### 3.4 Specular (`FUN_005b24c0`, `FUN_005ba1e0`)

`FUN_005b24c0` sets `D3DRS_SPECULARENABLE` (`0x1d`) to 1 when the material power (`+0x74`, shine x 100)
is positive (and the global flag `DAT_0071fe89`, which also cuts the stage count to one, is off) and passes the material to
`SetMaterial` (`vtbl+0xa8`) with specular = shine strength x specular colour. The light builder
`FUN_005ba1e0` writes `D3DLIGHT8.Diffuse` = colour x intensity, **`Specular` = the colour itself**
(`in_ECX[0x3f..0x42]`, not scaled), `Ambient` = 0; docs/31 and the old `Light` doc said specular was
zero, which was wrong. The renderer now evaluates D3D's local-viewer Blinn term per vertex
(`H = normalize(L + V)`, `Ls x Ms x (N.H)^P x atten`, only for lights with `N.L > 0`) and adds it after
the texture stages, clamped. The bridge's orange tanks, the compressor's arms and Electric's
generators get their highlights back (`bridge-f3` 7.1 -> 1.3 with the other fixes).

### 3.5 Pixel centres (D3D8 rasterisation)

Direct3D 8 puts pixel centres on integer screen coordinates; wgpu at half-integers. The original's
3D image therefore sits half a pixel right and down of ours for the same projection.
`ra2_render::d3d_pixel_centers` post-translates the clip space by `(+1/w, -1/h)`; `Duel::render` and
`render_to` apply it (the particles keep the uncorrected matrix for their billboard axes). On the
pixel-aligned box static cameras: `f3` 1.70 -> 0.66, `f3b` 2.90 -> 1.62, `f4` 0.65 -> 0.36,
`countdown` 0.25 -> 0.22; one pixel the other way makes all four worse. The 2D HUD is not shifted.

### 3.6 Checked and already right

Opacity maps (alpha test only with `BITMAP_INVERT` 1, else blended and sorted), additive `XP_TYPE` 3,
lightmaps on UV set 2 switching lighting off, the sphere environment map, the absence of mipmapping
and the Pyramidal = point / SAT = bilinear filters (`material.rs`) all hold up in these frames. No
fog was found: none of the 29 call sites of the cached `SetRenderState` wrapper `FUN_00590f30` that
push an 8-bit state passes a fog state (0x1c, 0x22..0x26); direct `vtbl+0xc8` calls were not
surveyed, and no frame shows depth fog.

## 4. Electric's lightning and the Metal Skull f4 shot

The lightning (`LightningField`, docs/31) is switched on with the hazards and drawn only while a robot
is in a charge zone; no bolt is visible in any of the six Electric references, so nothing there
could be checked or changed. `arena-metalskull-f4` is not a camera
shot: by the time F4 was pressed ALARM had been eliminated and the Match Results window was up
(docs/30 §6 noted the same). It cannot be reproduced without the fight outcome.

## 5. Capture timing (fitted, not game behaviour)

`capture_arenas.py` takes the intro still and clicks Skip Intro a fixed wall time after Start Game, so
the intro clock at both moments depends on load time. `fidelity.py` now uses per-arena intro-clock
values fitted at the 1/60 s frame step with `tools/oracle/fit_capture_timing.py`
(`ARENA_INTRO`, `ARENA_SKIP`; table in the script). Every intro fits in 2.13..2.40 s (the script's
"~2.5 s after Start Game" less load), every skip near the old values; the fit is sharp (one frame
either side typically doubles the MAE), which is only possible because the camera flights are right.
The intro and countdown MAEs above depend on these fitted values.

The camera-name notification: see section 9.3 (rule recovered and verified live). The old intro/skip
references are now marked unreliable in the manifest; the fitted tables below are kept only so they
stay comparable, and the new state-hooked references do not use them (section 9).

## 6. `match-countdown`: the reference is an intro frame

The reference shows the Combat Arena with the `Skip Intro` button and no COMPETITORS READY panel, so
it was taken during the intro, not the countdown (the aligned countdown capture was lost with
`local/`, docs/12). Scanning our box intro: MAE 28 at 0.5 s, 5.2 at 6.5 s, **1.87 at 6.55 s**
(`fit_capture_timing.py box intro 6.3:6.7 --ref match-countdown__1024x768__000.png`). The fixture
now renders `ra2-app --camera-shots intro@6.55` (box) and says so in its note; ours-countdown against
it was 34.79. The real countdown reference is `match-countdown2` (section 9, MAE 0.11).

## 7. Remaining gaps

* **Robot pose / match state** (not render): `metalskull-f2` (robots out of view in the original),
  `parkinglot-f2`, `halfcircle-f2`, `octagon-f2/f4`, `bridge-f2`, `metalskull-f4` (results window).
* **Electric bars aliasing**: `electric-countdown` 10.2 and `-intro` 12.0 have lowfreq 2.0/2.6; the
  residual is the 1-2 px wide bars against a camera that is a fraction of a pixel off (the intro and
  skip clocks are quantised to frames; the half-pixel shift made Electric slightly worse, 7.7 -> 11.0
  at its best skip frame, while improving every other arena).
* The bot lab's own render path (no half-pixel shift there).
* Arena decals (`Arenas\\decals\\*.tga`, loader reference at `0x44f105`, class `DecalTrigger`
  `0x6ef5e4`), scorch marks, sparks and smoke were not recovered or drawn this round (section 9.5).
* The intro camera's clock lags the frame-dt sum by the arena-load stall plus 0.265 s; we render a
  deterministic intro and do not model the stall (section 9.2), so intro stills are only comparable
  when the stall is known.

## 8. Code

* `ra2fmt::gmi::Material::resolve_face` (Option; §3.1), `Mesh::normals` doc (§3.2).
* `ra2-render`: `lib.rs` corner normals, `d3d_pixel_centers`; `material.rs` `session_bitmap`,
  `Light::specular`; `renderer.rs` specular uniforms and shader term.
* `ra2sim::duel` applies the pixel-centre shift in `render`/`render_to`.
* `ra2-app --earlier-arenas`; `ra2 info` prints each texture's slot, invert, filter and channel.
* `tools/oracle/capture_arena_states.py`, `frida/ra2_matchstate.js`, `arena_states.json`;
  `fidelity.py` `LIVE_SHOTS`/`run_live_shots`/`INTRO_CAMERA_LAG`; `ra2-app --camera-shots cd@R`;
  `ra2_render::material::{texture_path, set_texture_path}`.
* `tools/oracle/fidelity.py`: `CAPTURE_ORDER`/`earlier_arenas`, `ARENA_INTRO`, refitted `ARENA_SKIP`,
  the `match-countdown` intro fixture; `tools/oracle/fit_capture_timing.py`.

Tests: `a_face_id_past_the_sub_materials_clamps_and_one_equal_to_the_count_is_dropped` (ra2fmt),
`corner_normals_follow_the_face_normal_in_the_stored_four`,
`d3d_pixel_centres_shift_half_a_pixel_right_and_down`,
`specular_is_added_after_the_texture_when_power_is_positive` (GPU; skipped without an adapter),
`the_first_bitmap_loaded_under_a_name_is_the_sessions`, and the specular colour assertion in
`omni_lights_become_d3d_point_lights` (ra2-render).

## 9. State-hooked live captures (second round)

The first round's references were taken at fixed wall times; this one waits on game state. Tool:
`tools/oracle/capture_arena_states.py` with the Frida agent `tools/oracle/frida/ra2_matchstate.js`
(session via `tools/capture/original_session.sh`, docs/12). Evidence (local, backed up):
`local/oracle/original/arena-*-intro2`, `-countdown2`, `match-countdown2`, `arena-metalskull-f4live`
(PNG), `arena-texture-session-forward` / `-reverse` (event logs), the unreliable-rows note
`note-unreliable-references` in `manifest.tsv`, clocks in `tools/oracle/arena_states.json`.

### 9.1 Hook points (match object = `ecx`)

| address | what | field read |
|---|---|---|
| `0x475540` | match start | |
| `0x4758c0` | pre-countdown state: the intro is running (Skip Intro shown) | |
| `0x4759c0` | countdown start (Skip click **or** the intro's own end) | `+0x1dc` = 4.0 |
| `0x416360` | fight begins | `+0x8e` |
| `0x477480` | per-frame update, `[esp+4]+4` = frame dt | `+0x17c` label timer, `+0x1c0` is a 5 s wrapping timer (not the intro clock) |
| `0x477180` | camera-name notification shown, `+0x17c = 3.0` | |
| `0x591030`, `0x5914d0` | texture-manager get, image load/lookup | |

The intro clock used here is the sum of `0x477480`'s dt since `0x4758c0` (equal to `3.0 - +0x17c`
while the label runs). Natural intro lengths (intro state to countdown start) were 22.9-25.8 s of
that clock; the countdown reads 4.0 s at its start and the fight begins when it reaches 1.0.

### 9.2 What the new references show

* `countdown2` (the intro runs to its end, no skip; shot at a countdown reading of 1.98 s, bracketed
  by two clock reads 20 ms apart): our natural end-of-intro pose matches at **0.03-2.8** (electric
  0.03, halfcircle 0.03, box 0.11, compressor 0.15, metalskull 0.78, octagon 1.03, parkinglot 1.74,
  bridge 2.80; residual is robot pose). The skip clocks `ARENA_SKIP` are not needed for these.
* `match-countdown2` is a real COMPETITORS READY frame (Combat Arena), MAE 0.11.
* `arena-metalskull-f4live` (F4 two seconds into the fight, results window not up, clock 2:56):
  **0.33**, against 87.6 for the old after-the-match shot.
* Electric's bars: with the exact natural pose `electric-countdown2` is **0.03**, so the old 10.2/12.0
  came from the quantised skip/intro clocks, not from the camera's sub-pixel pose or the bars
  rendering. `electric-intro2` is 5.88 (1/60 s step of the intro clock and the robot pose).
* `intro2` (shot at intro clock 2.31): taking the game's clock as our intro time gives 12-58; the
  camera's own clock lags it by **the first long frame after the arena load plus 0.265 s**:
  `camera T = intro clock - first_frame_dt - 0.265` (first_frame_dt = the second match update, 0.19 s
  bridge, 0.39 box, 0.44 compressor, 0.78 electric, 0.49 half, 0.39 skull, 1.28 parking lot, 0.95
  octagon). Six arenas were fitted at 1/60 s step first (offset minus stall 0.257-0.279); applied to all
  eight, `intro2` is box 1.67, bridge 8.60, compressor 2.37, electric 5.88, halfcircle 6.49, metalskull
  5.41, octagon 2.79, parkinglot 1.52 (parkinglot and metalskull were not in the fit). The constant
  0.265 s is fitted, its cause (a delay before the intro generator advances) was not found.

### 9.3 The camera-name label rule (`FUN_00477180`, `0x4774f1`)

`0x4758c0` (intro state) calls the notification, which sets `+0x17c = 3.0`; each `0x477480` update
subtracts the frame's dt (observed: `+0x17c = 3.0 - intro clock` to the millisecond) and the box is
hidden when it reaches 0. The first update after the arena load carries the whole load stall as its
dt (0.19-1.28 s above), so the label is visible while `intro clock < 3.0`, i.e. for a **shorter time
of intro flight the longer the arena took to load**. A new still at intro clock 2.31 shows the label in
all eight arenas (the new references), and the countdown re-triggers it (`0x4759c0` calls it again;
every `countdown2` shows it). The old references were shot at camera T 2.13-2.40, i.e. intro clock
`T + stall + 0.265` = 2.8-3.9 s with this session's stalls: Electric, Octagon and Parking Lot are
over 3 s (label off, as in their references), Box lands at 3.00 (label on in its reference), while the
other four predict "on" and show "off", which only a larger stall in the old session explains (the
stall is not reproducible and the old references are unreliable). Ours keeps the label for the
first 3 s of intro; no change.

### 9.4 The session texture cache, live

* The global texture path (`0x71f574`) is the empty string for all 573 arena requests; the key is the
  bare name (section 3.3 corrected).
* Forward session (bridge .. octagon, 8 arenas, two passes in one game): one image load for each of
  `bars.tga` (compressor), `sawhazard.bmp` and `triple_spike.bmp` (box); every later request returned
  the same texture. The clear-all functions (`0x592300` textures, `0x5924e0` images, both called only
  from the graphics shutdown `0x5926d0`) never ran, and `0x591410` (a stage's release, 1045 calls) did
  not remove a name from the lookup (`bars.tga` was released after Compressor's match and returned
  unchanged for Electric, no file opened).
* Reverse session (fresh game: half circle, box, electric, compressor), with file opens: Half Circle
  opens `Arenas/half/Maps\sawhazard.bmp` and `triple_spike.bmp` (its own, the B variants); **Box then opens
  neither** (it did open all its other maps) and so shows Half Circle's. Electric opens its own gradient
  `bars.tga`; Compressor opens no `bars.tga`. So the rule is **first loaded wins, not a directory
  search order** (the forward order had coincided with alphabetical), which is what `session_bitmap`
  does; a model's file is looked up in its own `Arenas/<dir>/Maps`.
* The bot lab's switch (`0x593a40`) never fired (no Team HQ visit); its key is separate by
  construction (`texture_path()` is part of the key). Test:
  `the_session_key_includes_the_texture_path`.

### 9.5 Not done

Arena decals/scorch marks, sparks and smoke were not attempted: the decal textures are loaded from
`Arenas\decals\` by code near `0x44f105` and placed by `DecalTrigger` on weapon damage, which needs
the match's hit events (ra2sim, another agent's area) before it can be drawn or measured. No frame in
the 69 references shows one.

Tests added: `the_session_key_includes_the_texture_path` (ra2-render), `a_countdown_left_shot_parses`
(ra2-app).

## 10. The Combat Arena floor "greyer than ours" (checked live: no render difference)

Report: in the `shape-flapjack-emergency` captures the original's floor is a grey-lavender (about
88,79,122) where ours is a saturated blue-purple (about 65,52,142), while the flame captures matched.
**Cause: the robots were not where ours were, so the camera looked at another part of a baked lightmap.**
The floor is `floor_tile.bmp` (stage 0) times `boxfloor_lm.bmp` (stage 1, UV set 2), lighting off; the
lightmap is a bright neutral disc (255,255,233) at the arena centre and saturated blue (R 0.7, B 1.0)
at the four corners. The old live run had FlapJack at (-5.6,-1.5) heading 1.01 (the capture was taken
after the robots had driven from their spawn points before the AIs were stopped) and looked at the
centre disc; `shapeshot` put ours at the spawn point (-9.9,10.1), in the blue corner.

Evidence:

* `tools/oracle/frida/ra2_d3dstate.js` dumps the D3D8 state at every `DrawIndexedPrimitive` (device
  `*[0x71f58c]`; render states, material, enabled lights, stage states, world matrix). 9285 draws over
  eight shots gave 65 distinct state signatures: `D3DRS_FOGENABLE` 0, one point light (colour 1,1,1,
  specular 1,1,1, range 10000, attenuation 1,0,0, at -9.383,11.262,-3.519 = the arena's `Omni01`),
  ambient 0xff262626, `SPECULARENABLE` 1. The three floor batches (8/12/20 vertices) have
  `LIGHTING` 0, stage 0 MODULATE (texture, current), stage 1 MODULATE (texture, current) on texcoord
  set 1, stage 2 off, mag/min filter linear (point on the rim). This is what `Material::from_gmi`
  builds; nothing about fog, a spotlight, per-pixel lighting, a second light or a gamma ramp differs.
* A fresh live run with the robots at their spawn points (`capture_shape.py`, now `SHAPE_HOOK=<file>`
  starts the dump during the shots, `SHAPE_BOT0=""` keeps the prefix's robot) is the blue-purple floor:
  live 57,48,140 / 64,52,145 / 67,53,141 against ours at the same poses 58,48,143 / 69,53,146 /
  67,53,141 (within 5 counts over 24 floor samples, eight shots, second run the same).
* `shapeshot` with `RA2_SHAPE_POSES` (the camera formula fed the old run's recorded location and
  heading) at the old live camera poses: floor samples live/ours 86,79,123 / 86,79,122; 88,78,126 /
  88,79,126; 90,83,127 / 90,83,127; 94,82,130 / 94,81,130; 68,61,95 / 67,61,95 (all 8 shots, at most
  1 count apart on every floor pixel not covered by a robot).

So the floor, walls and lightmap path need no change; compare robot shots only with the robots
at the same place (record `plus.getLocation`, which the jsonl has) or use the spawn poses on both
sides. Test: `the_box_floor_is_tile_times_a_lightmap_with_lighting_off` (ra2-render, needs the game
data). Scratchpad side-by-sides: `lt/sbs_pose_*_live-left-ours-right.png`, `lt/sbs_spawn_*`.


## 11. How robots are drawn: the stock-robot audit (shadows, chassis specular, viewer vector)

**The defect.** In `shape-flapjack-emergency` ours drew faint grey triangles on both sides of FlapJack's wedge,
from the chassis' bottom corners down to the plate; live shows floor there. They were **our shadow**, not a mesh
fault: `shadow_item` projected the convex hull of *every attached component's* hull points, and FlapJack's
bottom face and wheels put a big translucent polygon under the robot's front. Live (`ra2_shadow.js`, below) the
shadow is cast by the **chassis mesh only**, and from the spawn camera it falls behind the robot.

### 11.1 `ShadowWizard::Update` (`FUN_004ee700`), recovered from a live per-frame trace

`tools/oracle/frida/ra2_shadow.js` logs the plane builds, receiver tests, ray/plane tests and fan draws of one
frame (`shadowwiz-flapjack-emergency__live__000.jsonl`, FlapJack v EMERGENCY at the spawn points):

* **One caster per robot, its chassis mesh**: 28 fans for FlapJack (28 chassis triangles in the `.bot`), 52 for
  EMERGENCY (52). Each triangle is projected on its own; a projected triangle that lies inside a receiver is one
  fan triangle, so `draws` = triangle count.
* The light ray goes through **the first vertex of the first chassis triangle**: logged direction
  `(-0.5869,-0.5421,0.6014)` is `normalize(p0 - light)` with `p0 = (-9.0701,-2.0874,9.5467)`, hit parameter on
  that triangle's plane `t = 16.5681` (the ray's origin is the light `(0.653,6.894,-0.418)`).
* **Receiver test 1**: `dot(ray, receiver normal) >= 0.3` (`0x6fe914`), the normal as the declared triangle winds
  (`(p1-p0) x (p2-p0)`, `FUN_0061d9c0`). On `box.py` the floor `(0,-1,0)`, the `+z` wall and the `-x` wall passed for
  FlapJack, the `-z` and `+x` walls for EMERGENCY.
* **Receiver test 2**: the ray's parameter at the caster's first-triangle plane may exceed the receiver plane's
  by at most `0.3` (`0x6fe918`): `t_caster <= t_receiver + 0.3` (a caster plane behind the receiver rejects it).
* **Single layer.** The original draws all the triangles with `rgba (0,0,0,0.3)` and still ends with **one** layer:
  under the shadow the floor is `(43,36,109)` where unshadowed `(51,39,143)` is `0.7 x floor + 0.3 x 38`, not the two
  layers the overlapping top and bottom faces would give (`(39,34,84)` = ours before). We project only the
  triangles facing the light (for a closed mesh they tile the silhouette exactly once). After the change the same
  pixels read `(45,38,110)` vs live `(49,40,111)`.

Code: `ra2_render::effects::{ShadowCaster, shadow_item_triangles}`, `Duel::effect_items`; tests
`a_chassis_casts_one_projected_triangle_per_triangle`, `a_receiver_facing_away_from_the_ray_is_skipped`,
`a_receiver_well_before_the_casters_first_plane_is_skipped`. The old hull-based `shadow_item` stays for the unit
tests of the maths it shares.

### 11.2 The chassis is drawn with its armour row's specular colour and power

Live vertex-buffer/material dump (`ra2_vbdump.js`, `vbdump-*__live__000.jsonl`): the chassis' D3D material is
`dif 1, amb 1, spc = armour row r g b, power = armour row power`: Scout (titanium) `spc 0.4004 0.4998 0.6332`,
power 60; ALARM (steel) `0.3332 0.30 0.30`, power 10; FlapJack titanium. Ours built chassis with the default
material (power 0), so every lit chassis face read too dark next to live (ALARM's red-striped front plate 150 v 190;
Razor's plate; EMERGENCY's body). `chassis_items` now reads `ch.armour_material` (`r g b a power`, the row after the
armour line; the armour type's `material_row()` when the file has none). Test
`a_chassis_carries_its_armour_rows_specular_and_power`.

### 11.3 Other findings from the dumps

* **`D3DRS_LOCALVIEWER` is 0 in every draw** (`d3dstate-scout-alarm__live__000.jsonl`): the specular half vector
  uses the camera's `-forward` as the viewer direction for every vertex, not the direction to the eye. The vertex
  shader now does that (`renderer.rs`); the chassis frames improve slightly (ALARM shot 6 8.7 -> 8.2).
* The live chassis is a **subdivided mesh** (ALARM 12 triangles in the file, 70 drawn; Scout 20 -> 79; FlapJack 28 ->
  116) with **flat unit normals per cell**; the cells of a bent quad have slightly different normals (FlapJack's side
  quads are not planar), so the shading changes gently across the face. Our 2-triangle quads with flat per-triangle
  normals showed the quad's diagonal as a brightness seam (FlapJack, left front face). The chassis vertex normals are
  now the average at the shared vertex index (`smooth_vertex_normals`, test
  `chassis_vertex_normals_are_smooth_within_a_face_and_hard_between_faces`), which keeps the hard edges between faces
  (faces do not share indices) and removes the seam. **Not done:** the exact subdivision (the grid per face and the
  inset top plate of the `buckling`/`top_corners` data); it only matters for the Gouraud sampling of highlights.
* Components: vertices, normals and uv of ALARM's BurstPiston and Hammer Head are identical to the live buffers; the
  D3D materials of the component parts are the ones `Material::from_gmi` builds (amb 0.59 / spc 0.56 / pow 10, wheel
  amb 0.21, forklift arm 0.62 / 0.22, round extender 0.85 / 0.60, ...).

### 11.4 The audit

Method. `tools/oracle/capture_shape.py` with `SHAPE_SET=audit SHAPE_EARLY=1` copies two stock bots over the prefix's
`AI/Team0/Bot0/Bot1.bot` (restored and sha256-checked; a SIGTERM/watchdog now goes through the restore), starts the
Combat Arena exhibition (hazards off) and **neuters the AIs while the intro is still up**, so both robots stand at
their spawn points when the fight starts (the late neuter of section 10 let them drive a different distance every
run). Four cameras per robot (front-right corner 3.4 fwd / 2.0 right / 1.3 up, left side, back-left corner, front)
from docs/37's `CAM`; the shots are taken inside the first ~12 s before the immobility rule ends the match.
`shapeshot` (`RA2_SHAPE_SET=audit`) puts the same cameras on our robots at their own spawn points, hazards off, AIs
held (no `RA2_SHAPE_POSES`: the camera follows each side's own robot, so a small yaw or position difference of the
resting robot does not move the robot inside the frame). `tools/oracle/robot_audit.py` measures (HUD clock and the two
HUD cards masked) the whole-frame MAE and the MAE of the central 640 x 424 window the robot fills, writes the
side-by-sides and per-robot montages (live left, ours right), and reports the yaw difference of the resting robots.
`poses_from_jsonl.py` turns a capture's jsonl into `RA2_SHAPE_POSES` when the live poses are wanted instead.

All 45 stock AI robots (`AI/Team*/Bot*.bot`: Scout, ALARM, Sentinel, Roly Poly, SaberTooth, Grog, CatFish, Mud Runner,
BEAR, Ninja, Bushido, Ronin, BOT-204, Devil, Coal Miner, JACKPOT!, The Boxer, EMERGENCY, M.A.D., Dementia, Little Metal
Friend, MiniBerg, Iceberg, SnowJob, FlapJack, BackSlash, Wide Load, Flame Chopper, LugNut, Hanky Panky, Berserker,
Razor, Eye Poker, Lil' Dog, Da Dog, Big Dog, Civil Disobedience, REVENGE, Raptor, Stinger, Tornado, DEADBEAT, Arc
Pounder, Ripblade, Backyard Ripper) were photographed live in 23 matches (`robots-<pair>__live__000-NN.png`, plus the
reshoots), 4 shots each; frames in which the match-results screen already covers the arena (Big Dog shot 3, MiniBerg
5-7, SnowJob 3) are left out of the means. MAE per robot, mean of its valid shots, **before** (hull shadow, default
chassis material, local viewer) and **after**:

| robot | bot | shots | MAE before | MAE after | centre before | centre after | yaw ours-live (rad) |
|---|---|---|---|---|---|---|---|
| ALARM | AI/Team0/Bot1.bot | 4 | 6.88 | 5.46 | 10.51 | 6.60 | -0.03 |
| Arc Pounder | AI/Team14/Bot0.bot | 4 | 5.01 | 4.58 | 6.31 | 5.66 | +0.03 |
| BEAR | AI/Team2/Bot2.bot | 4 | 8.06 | 7.31 | 10.65 | 9.18 | +0.05 |
| BOT-204 | AI/Team4/Bot0.bot | 4 | 6.02 | 5.80 | 8.22 | 7.79 | +0.07 |
| BackSlash | AI/Team8/Bot1.bot | 4 | 5.62 | 5.50 | 6.46 | 6.16 | -0.10 |
| Backyard Ripper | AI/Team14/Bot2.bot | 4 | 12.22 | 11.82 | 19.47 | 18.89 | -0.04 |
| Berserker | AI/Team10/Bot0.bot | 4 | 12.20 | 11.97 | 18.82 | 18.47 | +0.21 |
| Big Dog | AI/Team11/Bot2.bot | 3 | 8.17 | 7.10 | 13.10 | 10.29 | -0.00 |
| Bushido | AI/Team3/Bot1.bot | 4 | 7.75 | 6.64 | 11.33 | 8.46 | -0.11 |
| CatFish | AI/Team2/Bot0.bot | 4 | 3.85 | 3.13 | 6.05 | 4.05 | +0.02 |
| Civil Disobedience | AI/Team12/Bot0.bot | 4 | 6.71 | 6.50 | 8.24 | 7.78 | -0.10 |
| Coal Miner | AI/Team4/Bot2.bot | 4 | 5.98 | 5.74 | 7.19 | 6.76 | +0.08 |
| DEADBEAT | AI/Team13/Bot2.bot | 4 | 7.65 | 6.66 | 11.34 | 9.28 | -0.10 |
| Da Dog | AI/Team11/Bot1.bot | 4 | 3.42 | 2.59 | 4.73 | 2.47 | +0.01 |
| Dementia | AI/Team6/Bot1.bot | 4 | 8.19 | 7.81 | 10.41 | 9.65 | -0.15 |
| Devil | AI/Team4/Bot1.bot | 4 | 4.35 | 2.86 | 7.27 | 4.22 | -0.00 |
| EMERGENCY | AI/Team5/Bot2.bot | 4 | 5.80 | 3.43 | 10.76 | 4.80 | -0.01 |
| Eye Poker | AI/Team10/Bot2.bot | 4 | 9.99 | 7.86 | 17.03 | 11.17 | +0.12 |
| Flame Chopper | AI/Team9/Bot0.bot | 4 | 5.40 | 5.18 | 5.87 | 5.40 | +0.10 |
| FlapJack | AI/Team8/Bot0.bot | 4 | 3.97 | 3.52 | 5.13 | 3.87 | +0.04 |
| Grog, The Warrior | AI/Team1/Bot2.bot | 4 | 8.58 | 7.98 | 10.57 | 9.82 | -0.14 |
| Hanky Panky | AI/Team9/Bot2.bot | 4 | 7.80 | 6.76 | 12.58 | 9.75 | +0.02 |
| Iceberg | AI/Team7/Bot1.bot | 5 | 15.17 | 15.12 | 30.19 | 30.17 | +0.03 |
| JACKPOT! | AI/Team5/Bot0.bot | 4 | 4.45 | 3.95 | 5.72 | 4.68 | -0.04 |
| Lil' Dog | AI/Team11/Bot0.bot | 4 | 4.60 | 4.53 | 4.82 | 4.62 | +0.00 |
| Little Metal Friend | AI/Team6/Bot2.bot | 4 | 8.70 | 7.06 | 10.53 | 6.85 | -0.07 |
| LugNut | AI/Team9/Bot1.bot | 4 | 8.22 | 8.38 | 15.15 | 15.64 | +0.03 |
| M.A.D. | AI/Team6/Bot0.bot | 4 | 11.57 | 10.92 | 15.52 | 13.77 | -0.36 |
| MiniBerg | AI/Team7/Bot0.bot | 7 | 5.49 | 5.34 | 6.10 | 5.82 | +0.05 |
| Mud Runner | AI/Team2/Bot1.bot | 4 | 9.67 | 8.85 | 13.51 | 11.72 | -0.01 |
| Ninja | AI/Team3/Bot0.bot | 4 | 7.21 | 7.01 | 9.18 | 8.61 | +0.11 |
| REVENGE | AI/Team12/Bot1.bot | 4 | 6.90 | 6.37 | 10.06 | 8.66 | -0.09 |
| Raptor | AI/Team12/Bot2.bot | 4 | 12.48 | 10.53 | 18.95 | 13.73 | +0.17 |
| Razor | AI/Team10/Bot1.bot | 4 | 5.72 | 4.07 | 9.21 | 4.66 | +0.04 |
| Ripblade | AI/Team14/Bot1.bot | 4 | 7.45 | 6.13 | 12.05 | 8.98 | +0.01 |
| Roly Poly | AI/Team1/Bot0.bot | 4 | 4.45 | 4.22 | 4.46 | 3.88 | -0.06 |
| Ronin | AI/Team3/Bot2.bot | 4 | 4.96 | 4.81 | 7.05 | 6.86 | -0.04 |
| SaberTooth | AI/Team1/Bot1.bot | 4 | 3.96 | 2.93 | 5.34 | 4.02 | -0.01 |
| Scout | AI/Team0/Bot0.bot | 8 | 7.84 | 7.60 | 9.49 | 8.80 | -0.17 |
| Sentinel | AI/Team0/Bot2.bot | 4 | 7.82 | 5.90 | 13.89 | 8.97 | -0.06 |
| SnowJob | AI/Team7/Bot2.bot | 3 | 6.38 | 5.73 | 7.19 | 6.14 | +0.08 |
| Stinger | AI/Team13/Bot0.bot | 4 | 5.41 | 4.88 | 7.87 | 6.52 | -0.03 |
| The Boxer | AI/Team5/Bot1.bot | 4 | 7.70 | 7.06 | 11.52 | 9.88 | +0.09 |
| Tornado | AI/Team13/Bot1.bot | 4 | 10.62 | 10.45 | 19.23 | 19.00 | +0.08 |
| Wide Load | AI/Team8/Bot2.bot | 4 | 5.76 | 5.44 | 8.13 | 7.65 | +0.01 |
| **mean (45 robots)** | | | **7.25** | **6.52** | **10.60** | **8.89** | |

(The central-window column is the robot's own area; the whole-frame column includes a background that shifts when the
resting robot's yaw differs.)

What the shots show, robot by robot:

* **No missing, extra, mis-oriented, mis-textured or mis-painted part** in the montages read in full (Scout, ALARM,
  Sentinel, Berserker, Razor, Eye Poker, Lil' Dog, Civil Disobedience, Coal Miner, Tornado, Backyard Ripper, Roly
  Poly, Mud Runner, M.A.D., Dementia, JACKPOT!, Iceberg, EMERGENCY, FlapJack); the other 26 were screened by the MAE
  table above (none above 9 after the changes apart from Iceberg, Berserker, Backyard Ripper, M.A.D., Raptor and Tornado) and sampled. Component geometry, textures, paints
  and transparency agree (side-by-sides in the scratchpad `robot-look/after/`, the same before the changes in
  `robot-look/before/`).
* **Shadow differences** (the reported defect and the doubled layer) and **chassis lighting/specular differences**
  (every chassis with a lit face looking at the light: ALARM, Sentinel, Razor, Eye Poker, EMERGENCY, Devil, Da Dog,
  SaberTooth ...) are what the changes above fix: MAE 7.25 -> 6.52 on the whole frame and 10.60 -> 8.89 in the
  robot's window; the robots with the biggest gains (ALARM, EMERGENCY, Razor, Eye Poker, Sentinel, Raptor, Devil)
  are those with large bright lit faces.
* What is left in the table is **not rendering**: (a) animated parts that are at another angle in live (the spinning
  blades of Berserker, Backyard Ripper, Ripblade, Arc Pounder, Coal Miner, the pickaxe of Iceberg, wheel hubs that
  rolled); (b) resting yaw/position differences of the physics (`yaw ours-live` column: M.A.D. -0.36 rad, Berserker
  +0.21, Raptor +0.17, Dementia -0.15, Grog -0.14; Tornado and Iceberg turn by a lot in live because of their weapons
  torque), which shift the background and the light on the robot; (c) the live **HUD** draws an `IMMOBILITY WARNING`
  counter over a robot that has not moved for ~10 s and an `IMMOBILE!` card, ours shows neither in these frames (not
  a robot-drawing item); (d) `Eye Poker`'s top domes and Dementia's wheel-hub nuts are brighter in ours; the
  materials are identical to the dump, the facets differ with the rolled/yawed part, so this is (a)/(b), not a
  material difference (but see 11.5: the Buzzard wheel's hub parts were also too bright for another reason).
* Open: the exact chassis subdivision. (Iceberg's shading, the largest residual of this table, was a heading, not a
  mesh: section 11.5.)

The `yaw ours-live` column compares ours 1.5 s after the bell (our robots had landed during the countdown) with live
shots taken 2-14 s after it, while the live robots creep, and in some of the live captures the spinners' weapons ran;
docs/29 section 12v measures the rest from the bell on both sides (and recovers how a match lowers the robots onto
their start points and holds them until the bell).

Per-draw state check (`ra2_d3dstate.js` on Scout v ALARM, 56 signatures): every robot draw is `LIGHTING` 1, one point
light, `SPECULARENABLE` 1, `LOCALVIEWER` 0, `CULL_CCW`, `COLORVERTEX` 1 with diffuse from the vertex colour (absent in
the vertex format, so the material), stage 0 MODULATE (texture, diffuse) with linear filters on the chassis and
components, point on a few small parts; no fog, no blending except the translucent extenders. Nothing else differs
from what `Material` builds.

Tools: `tools/oracle/robot_audit.py`, `tools/oracle/poses_from_jsonl.py`, `frida/ra2_shadow.js`,
`frida/ra2_vbdump.js`; `capture_shape.py` env `SHAPE_SET`, `SHAPE_EARLY`, `SHAPE_SETTLE`, `SHAPE_HOOK_JS`,
`SHAPE_WATCHDOG`; `RA2_DUMP_ITEMS=<file>` (with `ra2-render`) writes our part vertices as JSON for comparison with
`vbdump-*`. Scratchpad before/after of the defect: `robot-look/shadow/flapjack_{before,after}/`.

### 11.5 Iceberg: a heading; and the hub nuts: normals that are not unit length (flame-look)

**Iceberg (MAE 15.1).** Not the mesh. A live vertex-buffer dump of Iceberg v MiniBerg (`vbdump-iceberg__live__000`) shows
its chassis as a 145-vertex, 132-triangle record whose outline, wall normals and bottom face are exactly the file's 36
triangles (same 10 outline points, the walls cut into two rows of cells, the top plate with its inset ring of crush
points 0.1 m inside the outline), and every live triangle's winding agrees with its vertex normal (the geometric
normal and the stored one have a positive dot for all 132). Ours stores its triangles the other way round by
construction (`chassis_mesh_vertices` swaps two corners; the earlier "inward-wound" reading was that uniform
convention, our normals come from the winding): all consistent, no face is culled wrongly. What differs is **which
side of the mesh faces the camera**. The shots' camera is placed from `plus.getHeading`, and the robot object's
heading (`+0xac`) is `pi` for Iceberg, `-pi/2` for Tornado and LugNut (`heading - body yaw` from `bodyyaw-*__live__000`,
exact to 1e-3; `heading-old-bots__live__000`): the camera stood at the chassis' `-Z` end, where ours still put it at
`+Z`. The loader (`0x4de450`) reads a heading from the file only from version 1.1 (`1.1 <= version`); these three are
v1.07 and carry none, and the original gives them theirs through the AI callback: the vtable call at `0x415a6e` runs
`0x401150` (`0x401170`: `robot+0xac = callback+0x40`), the brain's `fNoseOffset`, the `nose` of `AI/Bindings.py`
(Iceberg `math.pi`, Tornado and LugNut `-math.pi / 2`). For the 42 robots whose files have a heading it equals the
`nose` (checked for all 45 stock robots: the three above are the only differences). Ours reads the heading from the
file only, so these three faced the wrong way round (and the spawn yaw `front - heading` was off by the same angle).

Fix: `scriptai::bindings_nose` reads the `nose` of the robot's `Bindings.py` entry and `Duel::new` sets the rig's
heading to it when the table lists the robot (a robot with a brain; our human-driven robots in a match share the rule).
Shots (`robot_audit.py` against the new live reshoot, 4 valid shots of Iceberg): MAE 14.67 / 8.57 / 14.93 / 19.90
-> **2.87 / 2.71 / 6.67 / 3.92** (mean 14.5 -> 4.0); Tornado (4 shots) 6.5 / 6.3 / 11.6 / 7.7, LugNut 3.8 / 6.7 / 11.8 /
4.9 (the table's 10.45 and 8.38 before). Test `the_old_robots_take_their_heading_from_the_brains_nose`. Side-by-sides:
scratchpad `fl/final/iceberg_after_sbs/`, `tornado_lugnut_after_sbs/`.

**Hub nuts and domes.** The Buzzard wheel's parts that are not the tyre (`wheel3.gmf` `wheel3_display`, and
`wheel_slipperbottom.gmf`) store **normals 0.586 long** and the live vertex buffers carry them as they are (Dementia,
FlapJack and Razor dumps: the 44-, 22- and 7-vertex records have length 0.586 exactly), while the state dump has
`D3DRS_NORMALIZENORMALS` 0 on every lit draw: the original lights them at 0.586 times the diffuse term (and a smaller
`N.H`). Our loader and shader normalised, so the hub nuts and the wheel's inner parts came out too bright. The loader now
keeps the stored length (`stored()`: direction from the node's transform, length from the file) and the vertex shader no
longer normalises (all lit draws, translucent ones included). Only these two parts and `rotatorbar1.gmf` (1.5) are not unit
in the shipped scenes. Dementia (4 shots): MAE 2.01 / 3.17 / 3.92 / 2.61 -> **1.59 / 2.91 / 3.83 / 2.37** (central window
3.01 / 4.67 / 4.74 / 3.16 -> 2.11 / 4.23 / 4.51 / 2.71); Eye Poker (it carries the same wheels) 5.74 / 8.20 / 11.96 / 4.86 ->
5.53 / 7.64 / 11.67 / 4.61. Test `the_buzzard_wheels_hub_normals_keep_their_file_length`. The Eye Poker domes themselves
are the file's meshes with the dump's materials; what is left of its shots is the resting yaw and the HUD card
(section 11.4 (b), (c)). Side-by-sides: `fl/final/dementia_eyepoker/`.

Tools: `frida/ra2_heading.js` (`robots(n)`: each robot's `+0xac` and `GetHeading`; hooks the loader, `0x401170` and
`0x401150` with backtraces), `capture_shape.py` + `SHAPE_HOOK_JS=ra2_vbdump.js` (Iceberg v MiniBerg, Tornado v LugNut).
