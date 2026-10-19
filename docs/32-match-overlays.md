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
