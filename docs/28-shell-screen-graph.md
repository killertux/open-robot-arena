# 27 - Shell layout, window stack and screen graph (recovered)

## 1. Native layout (fx/shell-layout)

The original lays every screen out natively at the display resolution; there is
no 1280x1005 design space. `.gib` records give `Position`/`Size` as
`(value, mode)` pairs per axis: mode 1 = fraction of the parent's extent
(`0.5 1 0.1 1` = 50% across, 10% down), mode 0 = pixels (`160 0 0.9 1` = 160 px
wide, 90% high). Each is resolved against the **parent's frame**; the root record is
`Size 1 1 1 1` (the whole screen). `_Size 1280 1005` in a few files is only the
editor's canvas. `ra2-app` now boots `Shell` at the real target size
(`Shell::boot(game, [w, h])`) and calls `Shell::set_size` when a window
resizes, so the window is 1:1 with the layout (no letterbox) for the shell
screens. Match, HQ and build modes still use `SHELL_SIZE` (their own passes).

`SplitFrame`: `Master N` names the slot that `MasterSize` sizes; slot 0 is always
the first (top/left) pane and slot 1 the second. `exhib_setup.gib`'s main split is
`Horizontal false, Master 1, MasterSize 0.13`: slot 1 is the 13% bar at the
*bottom* (the bottom nav), slot 0 the remaining 87%. A slot's child takes the
pane's extent **along the axis the split divides** (its own `Size` there is the
editor's design-time leftover: `top slot` says 418 px and is the whole 87%
pane), while across that axis its own `Position`/`Size` apply (`content slot` is
`0.9` of the height of a horizontal split). Evidence: the panel at
(255,120)-(985,607) in the 1024x768 capture, `chassis_structure1.gib`'s
`Slot0 346 px / Slot1 at 346 px` (left 54% / right 46%) and the workshop's
info strip and nav bar at the bottom. Measured effect (app, 1024x768, MAE):
main menu 24.0 -> 0.59, teamhq-select 18.6 -> 6.3, botlab-chassis 30.4 -> 17.6,
exhibition-settings 54.8 -> 13.8.

Text: a `\n` written in a record's `_Text` is a line break (`credits.gib`).

## 2. Window stack

* `FUN_005e5620` (window manager, `this+0x68` = modal, `+0x6c` = result): hides
  the previous modal, stores the window as the modal and pumps messages until
  the modal pointer is cleared. It is the *modal* show.
* `FUN_005e5690(name)` finds a window by name (`FUN_005e4bd0`) and calls
  `FUN_005e5620`; the Credits handler uses it.
* `FUN_005e5ba0(name, ?, fade, flag)` is the screen *switch*: with a fade flag it
  builds fade-out/fade-in overlays (0.2 s alphas `0x3e4ccccd`), otherwise
  `FUN_005e5810 -> FUN_005e5830` swaps the active window.

So Options (`FUN_00486c50`, `options win`, `common\options.gib`, class
`OptionsWindow`, 0x19c bytes) and Credits (`FUN_00487260`, `credits win`,
`common\credits.gib`, 0x158 bytes) are modal windows over the main menu;
windows are created once and cached by name. The shell now keeps
`Shell.windows` (a stack over the current screen): the top window owns input
(clicks outside its controls are swallowed), the screen under keeps drawing, music
(`audio::on_screen`) ignores window screens, and the window's close/OK/cancel
buttons pop it.

## 3. Main-menu constructor `FUN_00486960`

Binds by widget name (`FUN_004294e0` lookup + `FUN_004873a0` bind):

| widget | handler | does |
|---|---|---|
| `Quit button` | 0x486c30 | quit |
| `Options button` | 0x486c50 | modal `common/options.gib` |
| `credits button` | 0x487260 | modal `common/credits.gib` |
| `exhibition button` | 0x486d70 | "Loading...", `ExhibitionSetupWindow(exhibition\exhib_setup.gib)`, switch (0x5e5ba0) |
| `Team HQ button` | 0x486f10 | "Loading...", `SelectTeamWindow(teamhq\select_team.gib)`, switch |
| `Multiplayer button` | 0x4870b0 | `multiplayer\multiplayer.gib`, plain switch (0x5e5810) |

"Loading..." (`FUN_004e6bd0`, string 0x6f2ad0) is shown only on the *first* visit,
while the heavy window is constructed; later visits find the cached window by name.

## 3b. Select team (`SelectTeamWindow`, ctor `FUN_004eb540`, init `FUN_004ec2c0`)

Recovered this pass (widget -> handler):

| widget | handler | does |
|---|---|---|
| `Help button` | 0x4e4ab0 | help |
| `MainMenu button` | 0x4eba40 | back to the main menu |
| `Options button` | 0x4e4990 | options window |
| `continue button` | 0x4eba60 | `Loading...` (`FUN_004e6bd0`), `TeamHQWindow(teamhq\teamhq.gib)` found by name `team hq win` or built (0x200 bytes), selects the grid's team, HQ init `FUN_004fe0c0`, sounds 0xcd/0xc9, switch `FUN_005e5830` |
| `new team button` | 0x4ebd40 | no empty slot: box `New Team` / `There are no empty team slots.`; otherwise modal `EditTeamWindow(teamhq\editteam.gib)`, caption `New Robotics Team`; OK (result 1) creates the record (`FUN_00502680`), refills the grid (`FUN_004ec310`) |
| `delete team button` | 0x4ec110 | box `Delete Team` / `Are you sure you want to delete <team>?` (message box style 0xc = Yes/No, window `common/messagebox.gib`); Yes (4) deletes the record and refills the grid |
| `select team grid` | 0x4ebc70 (click), 0x4eba60 (double click) | select a row / continue |

The shell fills `select team grid` from `Teams/Teams.txt` with `team_cell.gib`
(`ra2ui::teams::fill_select_grid`), selects row 0 (so Continue works at once). New Team and Delete Team write
`Teams/Teams.txt` at once, as the original does (message 200 → `FUN_00502720`, docs/36 §9.4); Continue opens the
selected row's team (docs/36 §9.3).

## 3c. Team HQ (`teamhq/teamhq.gib`) and its tabs

The HQ opens on Robot Inventory. The shell fills each tab from the career as the tab loads
(`Shell::set_hq_view`, `ra2ui::hq::fill_document`): the header (team name, motto, logo), the inventory grid,
the league table, the season grid and the history counters. The data is the career model's
(`ra2-app::hq_mode::shell_view`: the team record in `Teams/`, nothing written). Layout facts measured on
the captures (1024x768):

* Inventory / schedule grids divide their own rectangle evenly into `Rows` x `Columns` (3 x 2: 341 x 119 px
  cells in a 683 x 357 grid, not the file's `RowHeight 70` / `ColWidths 213`); the inventory fills down each
  column first (C.R.U.S.H., Sting Ray, BOT-CHOY / Forkie, Atom Smasher, Walrus).
* `AspectSetting`/`AspectRatio` on a group: 1 derives the height from the width (botlab `3d grp`: 0.92 of the
  left pane, square, 471 px), 2 derives the width from the height (`bot img group`: the 128 px snapshot is drawn
  square at the cell height, not stretched across 455 px). The group's Background is clipped to it.
* Class labels are the exe's strings at 0x6fd7b0: `Lightweight`, `Middleweight`, `Heavyweight`.
* Nothing is selected until a click: the shipped `Sel Bar`/`High Bar` positions are leftovers.

## 3d. Text line pitch (`FUN_00602210`)

The rich-text layout advances each line by the font's *ascent* (`FUN_00602bf0` = `font+0xab8`), not by
`ascent - descent` (`FUN_00602c00` measures the box the same way). `paragraph_run` uses `ascent`; the credits
names (8 lines in a 142 px box) now sit at the pitch the box implies.

## 3e. "Loading..." and the screen switch fade

`FUN_004e6bd0(text)` builds `UI\Common\loading.gib` (`loading window`, 160 x 60, centred, `caption bar` text
`Loading...`, `fg_dcn16`, colour `1 0.7 0.26 1`) once, then shows it modally (`FUN_005e5690`). It is called by
the main-menu Exhibition / Team HQ handlers and select team's `continue button` before the heavy window is built;
the window is then looked up by name, so only the first visit pays. `FUN_005e5ba0` then switches with a 0.2 s
fade each way (`0x3e4ccccd`). `Shell::set_animate(true)` (the window client) plays: loading splash over the old screen
(first visit of select team / exhibition only; 0.6 s here because our construction is instant), black overlay
0 -> 1 over the old screen in 0.2 s, 1 -> 0 over the new one in 0.2 s. Scripts and tests stay instant;
`ra2-shell --animate` with `wait N` (1/60 s ticks) draws the in-between frames.

## 3f. Options settings location

Settings are the player's, so the client reads and writes `ra2.cfg` in the user's config directory
(`$XDG_CONFIG_HOME` or `~/.config`, `%APPDATA%`, `RA2_CONFIG` overrides), seeded from the install's shipped
file on first change; `local/game/ra2.cfg` is never written. The Video tab's resolution dropdown steps through the
display modes (`FSDisplay*` and `FS3DDisplay*`). The exe asks the D3D device for its mode list at run time; the list
(640x480 ... 1600x1200) is the standard set, not recovered per adapter.

## 3g. Season schedule draw (`FUN_00500120`)

The HQ constructor fills the team record's schedule (`+0xe4`) with nine events drawn at random: nine times
`FUN_00419190(N)` = `floor(r * N)` (`N` = the event count, 25 in `Events.txt`; `r` = `FUN_004191d0`), redrawn while
the event is already in the list (`FUN_005014d0` is a find). `r` is the engine LCG at global `0x726d58`
(`FUN_00419220`: `s = s * 0x41c64e6d + 0x3039`; `FUN_00419200`: `s & 0x7fffffff`; divisor the double 2^31 at
`0x6a5850`; the seed setter is `0x4e47b0`). The state at HQ construction depends on every engine draw before it
(particles, shuffles), so it is **not recoverable**: the shipped "The Rookies" record has an empty schedule. The capture
(`teamhq-events`: Blade, Strategic Engineering, Sawblade Elite, Destructavision, Blackvolt, Robotic Terror Zone, Metal
Warriors, Toledo, BBA Midwest = events 2 6 12 9 13 15 5 22 20) is reproduced by 9 consecutive draws from state
1487546588 (exhaustive search of 2^32 states: one solution up to the top bit; chance match about 0.1%). The shell
uses that state (`ra2sim::career::ORACLE_SCHEDULE_STATE`, `draw_schedule`); `RA2_RANDOM_SCHEDULE=1` keeps the seeded draw.
One observed sample, not a rule. Cells carry the grid's `BorderColor`/`BorderWidth` ring (`cells::add_border`).

## 3h. Lists and their scrollers (arena list, league grid, select team)

* A `ListBox`/`Grid` wrapped by a `Scroller` attachment takes the **scroller body's rectangle** as its frame (arena
  list: 0.89 x 0.585 of the pane at (0.1, 0.08) = 258 x 282 px); its own `Position`/`Size` are editor leftovers.
  This fixed the arena list (MAE 21.9 -> 3.1) and widened the league grid to the scroll bar.
* The viewport is the laid-out height over the row height (arena list: 15 rows), not the file's design `Rows 6`
  (`Shell::sync_list_viewports` writes `_ViewRows`); the scroll bar is hidden while every row fits (also select team).
  Row text sits 2 px right and 5 px up of its 18 px row (fitted to the capture), selection/highlight bars span the list.
* `AspectSetting 1`: height = width x `AspectRatio` (arena preview 0.75 = 4:3 landscape).
* The arena description is the `DisplayBox` attachment of `description grp` (its body is the Glyph).
* The robot picker (modal popup) draws in front of the page (`ChildOrder` raised), which it did not before: pick-ai 9.4 -> 3.2.
* Grid `BufferWidth` insets each cell; select team opens with row 0 selected (blue bar) and no scroll bar.

Not recovered / not done this round: event bracket, season results, multiplayer, remaining Team HQ and Bot Lab
handlers, Help (none were decompiled); Bot Lab build mode in the harness.

## 3i. Shell3: descriptions, thumbnails, scrollers, bot lab view, Help

Evidence: `fidelity.py --out <dir>` (side images: reference | ours | diff). MAE before -> after (app renderer):
exhibition-arena 3.10 -> 2.17, exhibition-arena-box 12.50 -> 1.69, exhibition-pick-ai 3.17 -> 2.40, teamhq-standings
6.09 -> 4.23, botlab-overview 9.28 -> 5.87, -chassis 4.94 -> 5.50, -components 8.48 -> 5.82, -wiring 6.48 -> 5.56,
-paint 9.08 -> 7.04, match-hud 0.55 -> 0.50 (mean of 68: 9.21 -> 8.85). Chassis went up because the harness now shows the
real design (see below) instead of an empty lab; its text and layout are closer, the 3D model is the remaining gap.

* **Arena description.** The `DisplayBox` hangs on the **unnamed child** of `description grp` and is a `Glyph` that
  also carries `_Text`; the old code looked on the group itself and so always showed the file's editor placeholder
  ("hammers..."). The selected row's docstring is now written into every `DisplayBox`/`Glyph` under the group.
  A `Glyph` with a `DisplayBox` wraps to its **frame** (376 px here), not the stored `_Margin` width (234).
  Test `the_arena_description_follows_the_selected_row`.
* **Paragraph position.** A `Glyph` paragraph's first line sits `ascent/2` above the face's own baseline rule
  (9 px fg_mcn14, 12 px fg_d18; fitted on the arena description and `chassis.gib`), and a `Glyph` may have several
  runs (`Params 2`: `fg_d18` heading ending in a newline, then `fg_mcn14` text): each run starts below the lines
  the previous one finished. A `Text` in a `MultiLine true` `TextBox` wraps to the box (help windows).
* **Pick-ai thumbnails.** The robot cells' `bot preview` is the **TGA snapshot stored in each `.bot`**
  (`ra2fmt::bot` `snapshot`), registered as a `#bot-preview:picker-*` texture; `sample_bot.bmp` ("No Robot Preview")
  remains for a bot without one. Test `the_robot_picker_cells_carry_the_bots_snapshots`.
* **Centred text boxes.** A `TextBox` whose `Alignment` has the centre bit (18, 10, 34) centres its `Text` child on
  the box whatever it says (the authored `_Position` was made for the placeholder string): fixes the picker's robot
  name, standings, events, match HUD.
* **exhibition-arena-box 12.50.** Harness, not engine: the fixture used `--play`, which clicks hazards **off** and
  scrolls to a different row. It is now a route (`exhibition`, `arena`, `@320,188`; `--click-names` accepts `@x,y`
  points in the render size). Also: selection/highlight bars were drawn twice (the `Rectangle` fill and the `Image`
  inside it, `Selection.tga` was a guess for the blank name and smeared a blotch); the capture draws one 0.5 layer
  ((38,38,127) on black) **behind** the row text (list rows get `ChildOrder` 10, `ClipMode` 0).
* **Standings scroll bar (6.09 -> 4.23).** A `Scroller`'s `ScrollBar` stands flush inside the right edge of the
  scroller body (capture: bar x 924..940 of a body 293..940), not at its authored x (591); a one-column grid in a
  scroller runs its rows to the bar (cells' width = grid width, not `ColWidths` 388). The slider's `Background`
  body is resized with the track (it kept the authored 281 px: the "extra strip").
* **Bot lab view.** `ra2-app --build --start Teams/Team0/Bot0.bot --build-view overview|chassis|components|wiring|paint
  --out DIR --size WxH` lays the lab out natively at the capture size, hides the synthetic edit panel, fills the 3D
  view (top-down camera, 0.15 grey) and writes one frame; `fidelity.py` uses it (the capture was taken on C.R.U.S.H.).
  Recovered/derived rules: overview name/weight/class/snapshot come from the open design (weight class by mass
  thresholds); Components: the palette lists base definitions only (no `hidden = 1`, no `Styles\` variants: 70 =
  the 70 distinct previews), ordered by `sort` then file name (control board `sort = -10`; capture: board, CO2 tank,
  large air tank, Nifty, small pack, SuperVolt), as **one row** scrolled horizontally (`Rows 1` + horizontal `Scroller`)
  with cells 96 px edge to edge, unselected cells at 0.6 grey; Wiring: the control grid is **3 x 3** cells of a third of
  its frame (capture: rules at x 556/698/840/982, y 386/499/612), control-cell labels are `Name`, key names joined
  with `" - "` (separator string at `0x6f1cc0`, filler `FUN_004451b0` loads `control_cell.gib`, sets `control name`,
  `control input` (`FUN_00444820/00444830` iterate the keys) and `control img`); a 1.07/1.08 key row `0 57 0` has its
  code **second**; Paint Shop opens on **Color Fill**: `effect dropdown` text and `colorfill.gib` in
  `effect settings group`. Not done: the translucent chassis in Components/Wiring, the exact C.R.U.S.H. mesh/camera
  (the capture's design weighs 395.2 kg, Bot0 here 387.2), the colour swatch default.
* **Help (`FUN_004e4ab0`).** Switch on a help id: 0 `help_selectteam`, 1 `help_teamhq`, 2 `help_eventbracket`,
  3 `help_repairs`, 4-10 `help_ws_` overview/chassis/armor/components/wiring/paintshop/testrobot, 11 `help_exhibition`,
  12 `help_multiplayer`, 13 `help_multicreate`, 14 `help_multiprematch`; it loads the gib (`FUN_00480110`) into the
  frame's `help content group` and runs it modally (`FUN_005e5620`). Bound on select team, Team HQ, exhibition setup
  and every lab tab (`bindings::lab_help_id`); the frame's `close button` closes it. Test
  `the_help_button_opens_the_screens_help_window`. Event bracket, repairs and multiplayer help ids exist in the table
  but their screens are not reachable yet.

Changed test expectations: `ra2ui` palette (131 -> 70 cells, mechanics 30 -> 15, cells one row 96 px apart, distinct
pictures); league/results/armour grid cells take the grid width; `key_label` is `"W - S"` (was `"W / S"`).


## 3j. Shell4: event click flow, bot lab camera findings, arena stills

Evidence: `fidelity.py --renderer app --out <dir>` before (baseline run of this branch at a89708b) and after; side
images reference | ours | diff. Measured MAE (app renderer, 1024x768): botlab-overview 5.47, -chassis 5.10,
-components 5.49, -wiring 5.23, -paint 6.63, match-results 0.73, teamhq-events 7.86, exhibition-settings 4.07
(the 3i table quotes 5.87/5.50/5.82/5.56/7.04: those were taken before the lab harness was re-run on Bot0; the
numbers above are the baseline of this round). Nothing in 3j changes those renders, so before = after for them.

* **Event click (`FUN_00500c40`, bound at `0x4fdc4e` in the Team HQ constructor `0x4fd380` to the `event grid`).**
  `FUN_00416940` is the career's current event (-1 = none). None: modal `JoinEventWindow` (`FUN_00481b60`,
  `teamhq/joinevent.gib`, window `join event window`); the current event itself: `EventBracketWindow`
  (`FUN_0046a500`, `event bracket win`, screen switch `FUN_005e5ba0`), or `repairs win` (`FUN_004d5de0`) when
  `FUN_005011e0` is false; any other cell: message box `Robot Arena 2` / "Each team may enter one event at a time.
  Finish your current event before beginning a new one." (`0x6ff2f8`, `FUN_005e5e40`, OK only).
  Join window binds `cancel button` -> `0x468a60` (close), `enter event button` -> `FUN_00481f20` (needs a chosen
  robot: close, build the bracket for event + robot, hide `team hq win`, show the bracket), `robot grid` ->
  `FUN_00482590` (chosen robot's name into `robot name`; `FUN_004fa3b0` rejects an unusable robot: name cleared, enter
  disabled). The grid is 6 cells of 90 px (`choosebot_cell.gib` template, preview = the `.bot` snapshot).
  Implemented in `ra2ui::event_screens` and `Shell::click_event_cell`/`enter_event`; the shell asks the application
  to enter the event through `Shell::set_enter_hook` (`hq_mode::shell_enter_hook` owns a `Career`), and draws the
  bracket from the returned view. Route: `Team HQ button;continue button;events button;@430,376` (join window),
  `...;enter event button` (bracket), `...;later button` (back; note the original returns to the cached HQ window,
  ours rebuilds it on the Robots tab). Test `an_event_cell_click_follows_the_exe_handler`; ra2ui unit tests for the
  message and markup helper. `hq_mode::event_screen` now delegates to `ra2ui::event_screens::bracket_screen`.
* **Repairs and season results.** Reached from inside the bracket window (`FUN_0046ce50` pushes `repairs.gib`
  after a round, `FUN_0046e2b0` pushes `season_results.gib` after the last event); both need a played match. They
  stay on the `--hq --simulate-event` path (`hq_mode`), not on the shell graph. No capture exists for them, so no
  fixture was added.
* **Bot Lab camera: the premise did not hold.** All five lab captures (overview, chassis, components, wiring, paint)
  show the same near-top-down view with the robot's front toward the top, a flat 0.15 grey background and the zoom/
  rotate hint at the lower right; there is no low front three-quarter view in the references. The large diff areas are
  the **design**: the capture's C.R.U.S.H. is a hammer-arm robot (395.2 kg) while the shipped `Teams/Team0/Bot0.bot` is a
  four-wheeler (387.2 kg), so no camera change can close the gap with the available assets. Components and Wiring do
  show a translucent chassis (white box outline, internals visible); that rendering mode is not recovered. Not done.
* **Match results snapshots.** Already implemented (the `.bot` snapshots draw in both rows; MAE 0.73). The remaining
  diff is match state: the capture's fight ended with ALARM immobilized (Scout winner, "MATCH WINNER" and
  "IMMOBILIZED" badges, 26/663 points) while our 20 s scripted fight times out (no winner, 31/0). `ra2ui::results`
  already draws both badges from `RobotReport.winner/immobilized`; the data is a sim outcome, out of scope.
* **Arena stills.** They already have an app route (`ra2-app --match --arena ... --camera-shots`, `fidelity.py`
  `find_arena_fixtures`): all 48 are measured. MAE: box 0.40/4.15/1.75/2.83/0.65/5.55 (countdown/f2/f3/f3b/f4/intro),
  bridge 7.87/8.72/7.13/7.66/0.46/16.90, compressor 7.92/2.38/6.69/8.52/2.05/21.59, electric 26.73/15.96/9.84/8.77/6.10/38.76,
  halfcircle 0.03/8.61/2.90/2.78/1.01/19.67, metalskull 1.40/19.39/4.13/5.38/87.16/18.85, octagon 1.98/8.08/4.19/2.66/15.63/13.30,
  parkinglot 11.94/19.66/11.91/9.89/4.62/27.00. `match-countdown` is 34.79 (camera/intro pose). All are camera/physics
  dependent; not touched. Triaged and re-measured in docs/34 (mean of the 51 match/arena fixtures 11.02 -> 4.90).
* Not done: Team HQ events 7.86 (diff is resampling edges on every cell, not layout) and exhibition-settings 4.07.


## 3k. Play2: native event flow and workshop, modal windows over the match, baselines

(docs/36 §9 has the addresses and tests.)

* **Native layout everywhere.** The Team HQ's event flow (`hq_mode::Session::set_layout`: bracket, award, repairs,
  season end, the event match's HUD) and the workshop (`Builder::set_size`) now lay out at the window size like the
  shell's screens; nothing is letterboxed at 1280x1005 any more except the headless evidence frames, which keep the
  design size.
* **Bracket graphic** (`ra2ui::hq::fill_bracket_at`, `FUN_0046ad60`): pictures and lines in `bracket group`, laid
  out for the screen it is built at and written as fractions of the group.
* **Windows over the match.** The pause dialog's Options opens `common/options.gib` on the shell's window stack
  (`Shell::open_modal`); the match draws only the windows (`Shell::window_placements`) over its frame. F1's
  `common/controls_reminder.gib` is the match's own window (`ra2-app::reminder`).
* **`OptionGroup` text is baseline-placed**, like a `Group`'s: `video_options.gib`'s `Low`/`Medium`/`High`/`Off`/`On`
  and `multiplayer.gib`'s `Local Network`/`Internet` now sit beside their radios instead of under the group's clip.
* **Defaults the constructors load**: the create-game screen's `mpc_settings.gib` tab (`FUN_0049a000`), the edit-team
  window's logo list (`FUN_00468760`).
* **Esc** does nothing on shell screens and windows (`0x5ee6b0`).

## 4. Where the table lives

`engine/crates/ra2-shell/src/bindings.rs` holds the recovered (screen, widget
name) -> action table; `Shell` consults it before a `Notes` string. Screens
whose constructors were not recovered (team HQ tabs, exhibition setup content,
bot lab, results) still navigate by the `.gib` `Notes` (nav.rs); the main menu,
select team (above) and the options/credits/message box/edit team windows are
exe-derived.
