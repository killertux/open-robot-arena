# 32 - Match overlays: layout, countdown panel, results, Esc

Recovered from `Robot Arena 2.exe` (Ghidra + rizin) and checked against the
genuine captures in `local/oracle/original/` (`arena-*-countdown`, `arena-*-f4`,
`match-results`, `match-hud`). Measured with `tools/oracle/fidelity.py --only arena,match`.

## 1. In-match windows lay out natively (the rule)

The original has no design size (docs/28 §1). The match HUD (`gamewindow.gib`),
the pause popup (`ingame_dialog.gib`) and `matchresults.gib` are resolved at the
display resolution, exactly like the shell. Our app used to resolve them at the
shell's 1280x1005 and let the renderer scale the frame to the target, so every
window was 0.8x of the original at 1024x768 (the countdown box was 170 px wide
in the capture and 129 px ours; the results window was smaller by the same
factor). `Session::set_layout` / `Presentation::layout` is now the target size:
`Session::new` takes `options.size`, the window sets it every frame, and clicks
are mapped with the window as the input space, so the renderer's `fit` is the
identity. At 1024x768 the countdown group is 210x96 at (407, 336), the camera
textbox is at x 140, the time group at (468, 731): all as captured.

## 2. The countdown panel and border

`countdown group` carries a `Background` (0.4-alpha black image, 212x98 at
214,191) and a `Border` (width 1, black). Their saved *bodies* say `Visible
false` because the group itself was hidden in the editor. In the game the
attachments follow the owner, so showing the group shows the panel and border;
we showed only the group (the capture had a translucent dark panel with a black
1 px frame, ours had two vertical black lines). `hud::set_visible` now sets the
bodies' `Visible` with the owner (also for `time group`, same pattern). Test:
`the_countdown_panel_and_border_show_with_the_group`.

## 3. Esc and the pause dialog (`0x475ee0`, the match key handler)

| state | Esc does |
|---|---|
| arena intro running (`+0x1d4`) | with the post-intro timer `+0x1dc == 0`: skip the intro (`0x477900`); else nothing |
| match over (`+0x1d5`) | nothing |
| otherwise | show `common\ingame_dialog.gib` (`in game popup win`, `0x475fdb`) modally; the match is paused while it is open |

Dialog (ctor `0x476510`) binds four buttons; the handler result decides:

* `resume button`: closes the dialog (`0x5e56d0`), play continues.
* `options button` (`0x476700`): closes the popup and opens `options_win`
  (`common\options.gib`, `OptionsWindow`) over the match; not wired in this
  port (the options screen is shell-owned).
* `end match button` (result 4, `0x476830`): game type (`+0x9c`) 1 (multiplayer)
  goes to `prematch win`; type 2 runs `0x418770` (the event/career scoring
  path); otherwise `exhibition_setup_win` is shown (`0x475540(0, ...)` ends the match).
* `quit button` (result 8, `0x476910`): `0x4122d0(1)` leaves the game.

The gib notes say "(show confirmation dialog)" for end match and quit; the two
handlers above call no message box (only `0x416450`/`0x468260`, which flag the
shutdown), so no confirmation was found in the exe. Not verified live.

Implemented: `Session::escape()` (`Escape::{SkippedIntro, Paused, Resumed,
Ignored}`); the window's `quit button` now exits the app, `end match button`
returns to the screen the match came from, Esc on the results does nothing (the
Continue button is the way out). Test: `escape_toggles_the_pause_dialog_and_is_ignored_in_the_results`.

## 4. Flow numbers (unchanged, re-confirmed against docs/24)

Countdown timer 4.0 s (`0x475a97`), digits `trunc(timer)`, fight at 1.0 left;
results delay 3.5 s (`+0xec`, constant `0x6a5788`). The results window ctor
(`0x487d50`) shows `continue button` and hides `Quit Session button` for a local
match; in a multiplayer client it is the reverse (`0x487e..`). Where Continue
navigates beyond "back to the originating screen" was not decompiled.

## 5. Numbers

Before/after `fidelity.py --renderer app` MAE (original vs ours):

| fixture | before | after |
|---|---:|---:|
| match-results | 21.97 | 1.03 |
| match-hud | 1.16 | 0.55 |
| arena-box-countdown | 1.37 | 0.40 |
| arena-halfcircle-countdown | 1.08 | 0.03 |
| arena-metalskull-countdown | 3.69 | 1.46 |
| arena-parkinglot-countdown | 13.46 | 11.93 |
| arena-box-f4 | 1.41 | 0.70 |
| mean of 51 (arena-* + match-*) | 11.65 | 10.57 |

The remaining results difference is content (live match had winner/points 26 vs
663 and robot snapshots; ours is a different fight). Remaining arena-* error is
the 3D scene, not overlays.

## 6. The immobility warning counter and the `IMMOBILE!` card

Seen in the live `robots-*` captures (Scout and ALARM left alone in the Combat Arena, shots 3 s apart,
e.g. `robots-team0bot0-team0bot1__live__000-04..07`): after 12 s a small `IMMOBILITY WARNING: n` label hangs over
the idle robot with n = 8, 5, 2 at 13.5, 16.5 and 19.5 s of fight, and at 21.5 s the first robot's status
group carries a red `IMMOBILE!` card over its meters, which are dimmed. Ours drew neither.

**The rule** (the immobility rule itself is docs/11/`ra2sim::immobility`, `FUN_00412fc0`, sampled once a second):

* `FUN_004749c0` (the HUD constructor, per player slot, objects at `+0x248` + 4*slot): `immobile.tga`
  (`0x6f3318`, 104 x 32) at `+0x268`, `UI\common\immobility_countdown.gib` (`0x6f32f4`, group of
  `immobility_warning.tga` 80 x 32 and the text box `immobile countdown`, 18 x 12 at (54,15), font `fg_dcn14`,
  white, alignment 18) at `+0x278`; the accessors are `0x477940` and `0x477950`.
* A per-robot record (`FUN_004122f0`: `+0x10` seconds, `+0x14` warned, `+0x18` world position = the popup's anchor,
  `+0x1c` the popup) lives in a map. `FUN_00413900` (once per sample) moves the anchor and zeroes the time when
  the robot is farther than sqrt 2 from it (dist squared > `2.0`, `0x6a55f8`); else the time grows by 1
  (`0x6a57a4`), and while warned `FUN_00412390` rewrites the text.
* **`FUN_00412390`**: n = `trunc(20 - seconds)` (`10 - (seconds - 10)`, `0x6a57a8` = 10), capped at 10; a value
  of 0 or less hides the group, otherwise the text box shows n and `immobile_countdown.wav` plays when the text
  changes. At 10 s `FUN_00412240(robot, 1)` flags the record and shows the group (on the host, in a networked match,
  `giBotImmobileMessage` carries the robot, where it sits, 1 and the seconds; the joiner's `FUN_004120e0` stores
  them and shows the group, frozen at the sent seconds because it does not run the rule); under 10 s
  `FUN_00412240(robot, 0)` hides it.
* The popup (`0x5dc4c0`, class `0x6af920`) is the world-anchored widget: each frame it projects the anchor with
  `D3DXVec3Project` (`0x50c95a`; viewport, projection, view), sets the group's position to
  `(projected x - width / 2, projected y - 60.0)` (`0x6aece8` = 60.0) and draws nothing when hidden.
* **The anchor is the robot's centre of mass** (measured: the live label sits 62 px higher than ours with the body
  origin, `Robot::position`, 0.26 m below the centre of mass; with `center_of_mass` it agrees within 1 px). The
  rule's positions are now centres of mass too (`Duel::robot_centre`).
* **The card** (`FUN_00418040(robot, 2)`, the IMMOBILIZED branch at `0x41822b`): `immobile.tga` is placed at the
  `playerN status` group's top-left minus `(card - group) / 2` (whole pixels, truncating: 104 x 32 on 84 x 64 gives
  x - 10, y + 16), shown, and registered in the popup list at `+0x100` with 3.0 s (`0x6a5784`), which
  `FUN_00416210` counts down with dt and hides at zero. The other out states use `controlboard_destroyed.tga` and
  `eliminated.tga` the same way (not drawn by this port yet).
* **The dimmed meters** (`FUN_00476bf0` -> `FUN_004b3850(1)`, called for every "out"): the colour `(0.3, 0.3, 0.3)`
  (`0x3e99999a`) on `main background` (the status group's `Background` image, which includes the `POINTS:` label), the
  three bars and the three dots; the name and the points number keep theirs. It stays for the rest of the match.
* **Last-robot rule**: `FUN_00418040` returns at once when the match is over (`+0x8f`), so when the first robot put
  out leaves one robot (or team) standing, a second robot reaching 20 s in the same sample is *not* put out: live,
  Scout got the card and ALARM stayed in and won. `Duel::update_immobility` now stops there; it used to put both out.

**Port**: `hud::immobile_*` (group, number, card geometry, dimming), `Session::add_immobility` (draws one counter per
warned robot at `view_proj * anchor` and the cards for 3.0 s), `Duel::immobility_warnings` (host/local from the
tracker, joiner from the host's message), `NetEvent::Immobile` now carries the position and seconds.

**Side by side** (live left, ours right; `RA2_SHAPE_TIMES=1.5,4.5,...,21.05 shapeshot Bot0 Bot1`, audit camera
set, scratchpad `imm/`): the counter agrees at 13.5, 16.5 and 19.5 s (8, 5, 2; same 491 black pixels in the number box;
label top at y 368 vs 367, x within 8 px, which is the 0.06-0.12 m the resting robots differ in), the card and the dim
status group in the top-left corner at 21 s agree to 0.65 MAE over the 200 x 100 corner. Test:
`an_idle_robot_shows_the_counter_then_the_card` (8/5/2, the card for 3 s, Scout out and ALARM the winner).

**Open**: the clock text shows `2:47` where live shows `2:46` at the same warning count (ours rounds the remaining
time up, `hud::clock_text`; the live capture's clock rounding was not re-measured); the armour bar of a dimmed
panel is pinker in live (the `whitebar.bmp` overlay, `+0xdc`, is not drawn by this port); the other two out cards.
