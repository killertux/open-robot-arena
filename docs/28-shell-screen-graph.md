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
(`ra2ui::teams::fill_select_grid`), selects row 0 (so Continue works at once). A delete or a new team is kept in
the session, never written (the install is read-only data); the original rewrites `Teams/`.

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

## 4. Where the table lives

`engine/crates/ra2-shell/src/bindings.rs` holds the recovered (screen, widget
name) -> action table; `Shell` consults it before a `Notes` string. Screens
whose constructors were not recovered (team HQ tabs, exhibition setup content,
bot lab, results) still navigate by the `.gib` `Notes` (nav.rs); the main menu,
select team (above) and the options/credits/message box/edit team windows are
exe-derived.
