# 36 — Playthrough: the rewrite as a game, flow by flow

**Measured 2026-10-19 on `fx/playthrough` (based on `fidelity` 50a779f);
round 2 on `fx/play2` (based on `fidelity` beb1249), 2026-10-20, see §9;
round 3 on `fx/play3` (based on `fidelity` f55f0be), 2026-10-20, see §9.11.**
This is the master list of what a player can and cannot do in the windowed
client today. Each flow was played through the real player path: the windowed
`ra2-app` (no mode flags) on a private Xephyr display (`:7`, 1280×1024, no
window manager, the shipped `FullScreen=1` config), driven with XTEST mouse and
key events (`tools/capture/xtest.py`), captured with `xwd` +
`tools/capture/xwd2png.py`. The headless `--play` route covered the exhibition
game types. Where it helped, the exe was read with REA/Ghidra; the live original
was **not** run this round.

Legend: **works** · **fixed** (broken at the start of this round, fixed with a
test) · **broken** (still blocks or misleads a player) · **differs** (works, but
not as the original does).

## 0. How it was run

```sh
Xephyr :7 -screen 1280x1024 -ac -noreset &
# a scratch install: Teams/, AI/, Robot Designs/, Custom Textures/ and ra2.cfg
# copied, everything else symlinked to local/game (the career and the workshop
# write the game's own Teams/ and Robot Designs/)
RA2_CONFIG=$SCRATCH/user-ra2.cfg DISPLAY=:7 engine/target/release/ra2-app --game $SCRATCH/game
python3 tools/capture/xtest.py --display :7 focus --title "Open Robot Arena"   # keys need focus
python3 tools/capture/xtest.py --display :7 click X Y
python3 tools/capture/xtest.py --display :7 key w down / up
DISPLAY=:7 xwd -root -silent -out shot.xwd && python3 tools/capture/xwd2png.py shot.xwd shot.png
```

Two harness facts worth keeping: without a window manager, XTEST key events
only reach the client after `xtest.py focus` (mouse clicks work without it);
and `xtest.py key T` sends an unshifted `t`.

**Never point a playthrough at `local/game` itself**: `Career::save` writes
`Teams/Teams.txt`, `AI/Teams.txt` and `Teams/career.dat`, the workshop writes
the robot it edits and `Robot Designs/`. The client has no separate save
folder yet (§7).

Screenshots referenced below are in the session scratchpad
(`.../scratchpad/shots/NN-*.png`); they contain game art and are not committed.

## 1. Summary

| flow | state at start of round | now |
|---|---|---|
| Main menu, Options, Credits | works | works |
| Quit (main menu) | **dead button** | fixed |
| Exhibition setup → match → results → setup | works (keys only after focus) | works |
| Exhibition: human player drives | works | works |
| Exhibition: `Skip Intro` button | **dead** (Esc worked) | fixed |
| Exhibition results: player's team | **wrong** (AI team of the same number, "Red Zone") | fixed |
| Pause: Resume / End Match / Quit | works | works |
| Pause: Options | dead | fixed (round 2) |
| Pause: F1 controls reminder | missing | fixed (round 2) |
| Team HQ: select team, tabs, standings, history | works | works |
| Team HQ: robot inventory click | **crash** (client exits: `robot_cell.gib is not in UI`) | fixed |
| Team HQ: event cell click at 1280×1024 | **wrong event / dead-end screen** (`event_cell.gib` full screen) | fixed |
| Team HQ: join event → bracket | works | works (chosen robot now recorded) |
| Team HQ: `Play Next Round!` | **dead** | fixed |
| Event match driven by the player | **AI-only** (and Esc quit the game) | fixed |
| Round award, repairs, next round, back to HQ | unreachable from the menu | fixed |
| `Forfeit Event` | went to the HQ, event stayed open | fixed |
| Season end (after the ninth event) | unreachable | fixed (`season_results`/`golden_sawblade`, next season) |
| New Team: typing a name | **impossible** (no text input) | fixed; saved to `Teams/` (round 2) |
| Selecting a team other than the first | opens the first team's HQ | fixed (round 2) |
| Workshop from the HQ | **inert** screen (no robot, no editing) | fixed |
| Workshop edits saved to the team robot | not possible | fixed (on `Back to HQ`) |
| Edited robot in an exhibition match | not possible | fixed |
| Workshop `Test Robot` | AI drives the player's robot | fixed (player drives) |
| Multiplayer: Create Game, IP Address, Help | **dead** | fixed (navigation only) |
| Multiplayer: Join, Refresh, Start Game | dead | fixed (netplay, docs/38; our transport) |
| Workshop from an empty cell: robot saved into that cell (§9.26) | first free file, nothing saved when untouched | fixed (round 9) |
| Inventory grid deselect (message 2) | unknown | recovered, matches (§9.26) |
| Career end to end: new team, import, event, forfeit, restart (§9.26) | untested as one flow | works (`a_whole_career_new_team_to_forfeit_survives_a_restart`) |
| Shell message boxes: blank body, Yes left (§9.26) | template text, No first | fixed |
| Bot Lab: a new robot starts with no chassis; Components / Wiring / Paint Shop / Test Robot and Armor dim until it has a shape / a control board (§9.27) | started from a stock chassis, every tab live | fixed |
| Bot Lab chassis: freeform outline closed on its first point, step 2 and Finished extrude it (mesh, atlas, height slider 0.436364..1.32) (§9.27) | outline stored, mesh not rebuilt, stock 1.1 m scale | fixed (new `workshop::extrude`, byte-checked on 8 live files) |
| Bot Lab wiring: drag SWITCH / BUTTON / ANALOG onto the controller, properties window, click a part in the 3D view for the connection window (§9.27) | only the debug panel could wire a robot (player-facing lab had no way) | fixed (`lab_dialog`) |
| Weight class from the weight: light < 249, heavy >= 399 (`FUN_004df080`); a shapeless robot reads `None` (§9.27) | 250 / 400, `Lightweight` | fixed |
| Whole build from nothing, wired, named, saved, driven (§9.27) | untested | works (`a_robot_built_from_nothing_is_wired_saved_and_fights`) |
| Bot Lab 3D view: level 45 degree orbit camera per tab (middle button turns, wheel zooms 2..11 / 4..9), camera headlight, Components / Wiring baseplate + additive ghost + outline lines (§9.27b) | flat top-down view, 0.3 alpha ghost, one light | fixed (within 1 - 4 grey levels of the live frames) |
| Bot Lab wiring graphics: floating control icon, blue stem, green wires; double click edits a control (§9.27b) | none | fixed (wire geometry from the live strips) |
| Test Robot: practice arena, one robot, Obstacles list, RESET, static camera at (-4.2, 2.5, 9.5) (§9.27b) | the shipped AI in the box arena | fixed |
| Bot Lab name box takes keys after a click and adds to the text; Paint Shop labels, swatch and picker; the tab strip's cables (§9.27b) | first key replaced the name, labels / cables clipped away | fixed |

Counts: 26 rows; **16 fixed this round** (2 of them crashes/dead ends that
closed or stranded the client), 6 worked already, 4 still broken (pause
Options, F1 reminder, select-another-team, network multiplayer), plus the
"differs" items in §2–§6.

## 2. Main menu, Options, Credits, Help, Quit

* **works** Boot to `mainmenu.gib`, music `Enter the Battlefield`
  (`01-menu.png`).
* **works** Options is the modal `options win`: Video and Audio tabs, radios,
  sliders and checkboxes edit the user `ra2.cfg`; Cancel restores the file
  byte-for-byte (checked with `diff`). (`50-options.png`, `52-audio.png`)
* **fixed (round 3)** The resolution and camera dropdowns open a list and a
  row sets the value (§9.12); before, a click stepped to the next mode.
* **fixed (round 2)** Options' radio rows: the `Low`/`Medium`/`High` and
  `Off`/`On` labels were clipped away, which made the Graphic Detail row's three
  radios look misplaced; they now sit on their baselines (§9.9).
* **works** Credits modal and its Close (`53-credits.png`).
* **works** Help on select team, Team HQ, exhibition, the lab tabs; **fixed**
  multiplayer and create-game help (ids 12/13 of `FUN_004e4ab0`).
* **fixed** `Quit button`: the shell binds it to `Action::Quit` (`0x486c30`)
  but the window ignored it. The window now exits on any click the binding
  table maps to `Quit` (`flow::quits`). Test
  `playthrough::quit_and_the_multiplayer_buttons_are_bound`.
* **fixed (round 2)** Escape on a shell screen exited the whole client. The
  original ignores it there (§9.7); so does this client now.

## 3. Exhibition

Played: Exhibition → Robots → slot 1 human picker (Sting Ray) → slot 3 computer
picker (Red Zone / Scout) → Arena (Combat Arena) → Start Game → intro → fight
→ pause → results → Continue (`02-exhib.png` … `14-aftercontinue.png`); again
with the default robots and the keyboard (`m21.png`, `m23.png`).

* **works** Game Settings, Arena (list, preview, description, hazards
  checkbox), Robots (four slots, the two shipped pickers, Eject).
* **works** The player drives robot 0 with the robot's own wiring (`W/A/S/D`
  and arrows map to its scan codes); chase camera; `C` cycles chase → watch →
  orbit; F2/F3/F4; Esc pauses. Measured headlessly with the same `Session`
  the window uses: 3 s of `W` moves C.R.U.S.H. 22.7 m and Sting Ray 17.4 m.
* **fixed** `Skip Intro` was not a click target in the window (only Esc
  skipped). `Session::skip_button_at` hit-tests the HUD's `skip button` while
  the intro plays. Test `playthrough::the_skip_intro_button_skips_the_arena_intro`.
* **fixed** The results row of a career robot showed the AI table's record of
  the same number (Sting Ray of The Rookies was shown as "Red Zone", logo
  included). `MatchReport` now reads `Teams/Teams.txt` for robots under
  `Teams/Team<N>/` (`ra2sim::match`, unit test
  `a_result_rows_team_comes_from_the_table_the_bot_lives_in`; `72-results.png`
  shows The Rookies).
* **works** All five game types through `--play` (deathmatch, battle royal,
  team match, tabletop, king of the hill): each reaches `Results` with the
  player robot moving 22–26 m.
* **fixed (round 2)** Pause `Options` opens the options window over the
  paused match; F1 shows the robot controls reminder (§9.1, §9.2).
* **differs** `Space` is a drive key when the robot is wired to it, so it
  cannot skip the intro in a player match (Esc and the button do).
* **fixed (round 3)** The time dropdown opens its list (`No limit`, 1, 2, 3, 4,
  5, 10, 15, 20) and a row is the match length; the weight-class filter empties
  the slots of other classes and dims the pickers' robots outside it (§9.12,
  §9.13).
* **fixed (round 2)** A match the player plays that times out goes to the
  judges' decision (`FUN_00418770`), not the evidence rule (§9.6).
* **fixed (round 3)** A duel's unused slots 2 and 4 are reported `OPEN`, the
  word of the lobby's `noplayer.gib` cell (§9.14).
* **differs** In the first run the player robot was pinned by the AI and
  declared IMMOBILIZED within ~25 s of not being driven; that is the
  immobility rule, not a fault.

## 4. Team HQ career

Played: Team HQ → select team → Continue → the four tabs → Event Schedule →
Blade Tournament → join window (robot chosen) → Enter Event! → bracket →
Play Next Round! → intro → fight (driving) → results → event award → Continue
→ back on the HQ with the league and schedule updated (`34-hq.png`,
`m35.png`, `41-join1.png`, `42-bracket.png`, `70`–`76`).

* **works** Select team, Delete Team box (session only), the HQ header,
  inventory with snapshots, League Standings, Event Schedule, Team History,
  Help, Select Different Team, Main Menu.
* **fixed — crash** A click on a robot in the inventory exited the client:
  the grid's editor note `fill grid with team robots (robot_cell.gib)` was
  read as a link and `robot_cell.gib` is not a screen. Notes that describe a
  widget's *contents* (`fill …`, `load … into this slot`, `load '…' here`,
  `can have …`) are no longer navigation (`nav::describes_contents`, unit test
  `content_notes_are_not_navigation`), and a click the shell cannot follow is
  now reported instead of closing the client.
* **fixed — wrong event / dead end** At the window's native 1280×1024 layout
  the schedule's hit test used the file's design-time `RowHeight`/`ColWidths`
  while the cells are drawn evenly over the grid: the first cell opened the
  second event, the third fell through to the grid's note and replaced the HQ
  with a full-screen `event_cell.gib` (`m39.png`). The shell and `ra2ui::hq`
  now divide the grid evenly, as drawn. Test
  `playthrough::the_hq_grids_hit_the_cells_they_draw_at_the_window_size`.
* **fixed — dead core flow** `Play Next Round!` did nothing from the menu
  route; the match/award/repairs flow existed only behind `--hq`. The bracket's
  `play button` now hands the saved career to `hq_mode::Session`
  (`flow::start_event_match`), which plays the round, awards and saves it,
  shows the award / repair bench / next bracket, and returns the player to the
  shell's own HQ (`flow::return_to_shell_hq`) when the event flow reaches the
  HQ. The shell's event hook reloads the career from disk and records the
  join window's chosen robot as `Robot In Event`; it writes nothing. The shell
  remembers the entered event and robot (`Shell::entered_event`), the `play
  button` enters (or resumes a saved) event in the event flow, and the round's
  award is what saves the career. (An earlier version of this round saved on
  entry; a test then wrote into `local/game/Teams/` — see the report.)
* **fixed — human control** Event matches were AI-versus-AI and Esc in them
  quit the game. They are now player-driven like an exhibition
  (`Session::with_match_player`), with the pause dialog (Resume, End Match =
  leave the round unplayed, Quit) and the camera keys. Test
  `playthrough::an_event_entered_in_the_shell_is_played_by_the_player_and_saved`
  (holds `W`, wins round 1 in the run measured, repairs, Finish Later, back on
  the HQ).
* **fixed** `Forfeit Event` closes the event as a loss with the rounds won so
  far (`Career::finish_event(false)`), saves, and returns to the HQ.
* **fixed** After the ninth event's award, `continue button` shows
  `golden_sawblade.gib` (rank 1) or `season_results.gib` (`FUN_0046e2b0`'s
  branches) and its `continue button` starts the next season. Test: the tail of
  `hq_mode::a_headless_season_run_completes_and_writes_the_season_frames`.
* **fixed** New Team: the shell had no text input. A click on a text box whose
  record says `Editable true` focuses it; typed printable characters and
  Backspace edit it (labels such as a robot's weight class are not editable).
  Test `playthrough::a_new_team_is_typed_into_the_edit_team_window`.
* **fixed (round 2)** The selected team is the one the HQ, the event flow
  and the workshop open; New Team and Delete Team write `Teams/Teams.txt` at
  once, a new team gets its `Teams/Team<N>/` folder (§9.3, §9.4).
* **fixed (round 2)** Event matches take the event's robots per match, game
  type and the bracket's opponents from `Events.txt`; the match length is the
  session's match time (§9.5).
* **fixed (round 2)** The event flow screens and the event match are laid out
  natively at the window size; the bracket is the original's graphic
  (`FUN_0046ad60`, §9.8). **fixed (round 3)** The award is drawn over the
  event-results 3D scene with the prize model on its spot (§9.11).
* **not reproduced (round 2)** The join window's notebook shift: at the
  native 1280×1024 layout the schedule's frames are identical with and without
  the window (measured headlessly), and the round-2 capture shows no shift.
* **fixed (round 3)** An event entered and left with `Finish Later` before any
  round is played is committed to the team record, as the original does
  (§9.15).
* **fixed (round 3)** `End Match` in an event hands the match to the judges
  (`FUN_00418770(1)`) and the round goes on to its results and award (§9.16).
* **fixed (round 3)** The AI teams bring the robot of the player's weight
  class (§9.17).

## 5. Bot Lab

Played: HQ → click Sting Ray → WORKSHOP → Components → Weapons → a palette
cell (Axe Head attached, 212.3 → 224.3 kg) → Back to HQ → the team file
changed (`md5sum`) → an exhibition `--play --team 0 --bot 1` spawns the
224 kg, 16-part Sting Ray (`81-lab.png`, `82-comp.png`, `83-attach.png`,
`85-backhq.png`).

* **fixed — inert screen** From the menu route the lab was the shell's static
  `botlabwin.gib`: empty 3D view, empty palette, no robot (`m45.png`). The
  `workshop button` now opens the editable workshop
  (`build_mode::Builder`) on the inventory robot the player clicked
  (`flow::inventory_cell_at`, `flow::open_workshop`), or on a new robot from
  the stock chassis when none is selected.
* **fixed** `Back to HQ` (`back button`, `0x43b6d0` → the `team hq win`)
  writes the design back to its team slot (`Builder::save_to_team`), refreshes
  the HQ data and takes the app's shell to the HQ; `Main Menu` does the same
  to the menu. Test
  `playthrough::a_robot_edited_in_the_workshop_is_saved_to_its_slot_and_fights`
  (attach a motor, Back to HQ, the `.bot` on disk has one more component, and
  the exhibition match built from the player picker spawns it).
* **fixed** `Test Robot` lets the player drive the robot (was AI-driven).
* **works** (as documented in docs/17) tabs, chassis steps, palette attach,
  undo/redo, wiring add/bind, paint fill, Save As (Enter) to
  `Robot Designs/`.
* **fixed (round 2)** The workshop is laid out natively at the window size;
  its synthetic panel's rows read in full; a robot built from an empty
  inventory selection joins the team's roster on `Back to HQ`; the robot in
  the current event is refused with the original's message (§9.9).
* **differs** New parts on the baseplate are placed at a fixed spread (no drag
  placement); the snapshot button, the shift-mouse rotation and outline
  extrusion are open (docs/17).
* **differs** The original's test arena is the `Practice Arena` with an
  obstacles option group (`FUN_0043a6f0` case 9); ours is the box arena
  against a fixed AI opponent.

## 6. Multiplayer (navigation only)

* **fixed** `Create Game` → `mp_create.gib` (`FUN_0049a000`, plain switch),
  its Game Type/Arena tabs and `Cancel` back to the browser; `IP Address` →
  the modal `ip connect window` (`FUN_00497cd0`, `FUN_005e5690`) and its
  Cancel; Help (`FUN_004e4ab0(0xc)`, `0xd` on create). Bindings in
  `ra2-shell/src/bindings.rs`; test
  `playthrough::quit_and_the_multiplayer_buttons_are_bound`. (`90-ip.png`,
  `92-help.png`, `91-create.png`)
* **fixed (netplay, docs/38)** Refresh List enumerates LAN hosts (DirectPlay
  `EnumHosts` frames, `RA2_NET_HOSTS` adds direct addresses) into `server
  listbox`; Join Game / the IP window's Connect join the host; Create Game's
  Start Game hosts and opens `prematch.gib`, whose panels, ready box,
  disconnect and Begin Match work; the match is host-authoritative (inputs
  up, snapshots down). The transport is ours (not DirectPlay byte for byte),
  so the original cannot join this port's games. Test
  `ra2-app/tests/netplay.rs`.
* **fixed (round 2)** The LAN/Internet radio labels sit beside their radios
  (option-group labels are baseline-placed, §9.9); the create screen opens on
  its `mpc_settings.gib` tab.

## 7. What remains for a working game (ordered, after round 3)

1. **A save location**: the career and the workshop write into the install
   (`Teams/`, `Robot Designs/`), as the original does; a read-only or shared
   install needs a per-user copy.
2. **Workshop drag placement**: done (§9.18); open: the original's physics
   overlap query and a live trace of one drag.
3. **A capture of the original's bracket, options, award and dropdown
   windows** to confirm the recovered layouts (§9.8, §9.11, §9.12). Round 3
   read everything from the exe; the original was not run.
4. Multiplayer: done over our own transport (docs/38); open are DirectPlay
   wire compatibility, spectators, voice and the scoring messages.

Done in round 3 (§9.11 to §9.17): the award's 3D prize scene, the resolution,
camera and match-time dropdown lists, the weight-class filter, the duel's open
slots, `Finish Later` before a round, the event `End Match` path and the AI
teams' robot choice. Done in round 2 (§9): team selection and persistent teams,
the event's own field and match length, pause Options and F1, the judges at
time-out, Esc on shell screens, native event flow and workshop with the bracket
graphic, option-group labels, the workshop panel rows, the create-game tab, the
new workshop robot on the roster.

## 8. Tests added this round

* `ra2-app/tests/playthrough.rs` (6): HQ grid hits at 1280×1024, the shell →
  event → player-driven match → saved round → HQ route, workshop edit saved to
  its slot and spawned in an exhibition, New Team typing, Quit and the
  multiplayer bindings, the Skip Intro button. The career tests run on a
  scratch install (copied `Teams/`, `AI/Teams.txt`, `Robot Designs/`) and hold
  `ra2script::lock()`.
* `ra2-app::hq_mode` season test: the last award → season end → next season.
* `ra2-shell::nav::content_notes_are_not_navigation`.
* `ra2sim::match::a_result_rows_team_comes_from_the_table_the_bot_lives_in`.

Gate on this branch: `cargo fmt --all --check` and `cargo clippy ... -D
warnings` clean; `cargo test --release --workspace --no-fail-fast` 529 passed,
0 failed with `RA2_GAME` pointing at a pristine copy of the install. Against
`local/game` as it stands after this round (its `Teams/Teams.txt` and a
`Teams/career.dat` were written by a test during development, see the report)
the career tests that copy `Teams/` fail until that file is restored.

## 9. Round 2 (`fx/play2`, 2026-10-20)

Played on a scratch install in a windowed `ra2-app` on Xephyr `:9` (1280×1024,
`FullScreen=1`), driven with `tools/capture/xtest.py`, plus headless routes
and tests. The live original was not run this round; every behaviour below is
read from `Robot Arena 2.exe` (addresses given).

| # | item | status |
|---|---|---|
| 1 | Pause → Options | **fixed** |
| 2 | F1 controls reminder | **fixed** |
| 3 | Selecting a team other than the first | **fixed** |
| 4 | New/deleted teams persist (`Teams/Teams.txt`, `Teams/Team<N>/`) | **fixed** |
| 5 | Event format: robots per match, game type, match length, bracket | **fixed** (match length: see 9.5) |
| 6 | Time-out decided by the judges | **fixed** |
| 7 | Esc on shell screens | **fixed** (ignored, as in the original) |
| 8 | Event screens and workshop native; bracket graphic | **fixed** (the award's 3D prize scene: round 3, 9.11) |
| 9 | Smaller items | 7 fixed, 1 not reproduced; the rest were closed in round 3 (9.11-9.17) and the lab's drag placement (9.18) |

### 9.1 Pause → Options (`FUN_00476700`)

The in-game popup's constructor (`FUN_00476510`) binds `resume button`
(`0x5e56d0`), `options button` (`FUN_00476700`), `end match button` and
`quit button`. `FUN_00476700` closes the popup with result 1 and runs `options
win` (`common/options.gib`, `OptionsWindow`, `FUN_0049f010`/`FUN_0049f5f0`)
modally. The Esc handler (`0x475ee0`) paused the match before the popup and
unpauses when its modal loop returns, so the match stays paused under the
options window and resumes when it closes. Here: `match_mode::Session::
open_options`/`close_options` (frozen while open), the window lives on the
shell's window stack (`Shell::open_modal`, drawn alone over the match with
`Shell::window_placements`), and edits the same staged `ra2.cfg`. Tests
`match_mode::pause_options_holds_the_match_until_the_window_closes`,
`playthrough::the_match_options_window_is_a_shell_modal`.

### 9.2 F1 controls reminder (`0x475ee0` key `0x70`, `KeyControlRemindWindow`)

F1 is ignored during the arena intro (`+0x1d4`); otherwise it builds `controls
reminder` from `common/controls_reminder.gib` (`FUN_004828b0`), finds the first
local robot, fills it (`FUN_00482b20` → `FUN_00482b60`: robot name into `bot
name`, `control grid` set to 3×3 (`FUN_005fd510(3,3)`), one
`botlab/control_cell.gib` per control at its own cell, the wiring tab's filler
`FUN_00482bd0`), pauses outside network games (`FUN_004190f0(1)`) and shows it
modally. The window's key handler `0x482a10` (vtable `0x6a7f74`): Esc closes,
F1 moves to the next local robot or closes. Module `ra2-app::reminder`; tests
`reminder::the_reminder_lists_the_robots_controls`,
`match_mode::f1_shows_the_controls_reminder_over_a_paused_match`.

### 9.3 Team selection (`FUN_004eba60`)

`continue button` hands the grid's selected team to the HQ. The shell reads
the selected row and calls its team hook (`Shell::set_team_hook`,
`hq_mode::shell_team_hook`), which records the install's current team
(`hq_mode::select_team`) and returns that team's HQ view; the career
(`Career::load_team`/`new_team`), the event flow and the workshop all open it.
Each team keeps its own sidecar (`Teams/career.dat` for slot 0,
`Teams/career<N>.dat` otherwise). Test
`playthrough::the_selected_team_is_the_one_the_hq_opens`.

### 9.4 New Team / Delete Team write `Teams/`

New Team's OK (`FUN_004ebd40`, result 1) marks the first empty slot used
(`FUN_00502680(slot, 1)`) and broadcasts message 200 (`FUN_005882f0`); the
player table's constructor (`0x502150`) registered `FUN_00502720` for it,
which writes `Teams\Teams.txt` (`FUN_005024e0`). Delete's Yes (`FUN_004ec110`,
result 4) clears the record (`FUN_00502680(slot, 0)`, `FUN_004fa230`) and
broadcasts the same message. No empty slot: `New Team` / `There are no empty
team slots.`. The new row is selected after the refill (`FUN_004ec8d0`). The
logo list is every `Teams\*.bmp` (`FUN_00468760`), the preview `Teams\` + the
selected row (`FUN_00468a90`). `ra2ui::teams::{create_team, delete_team}`;
test `playthrough::a_new_team_is_typed_saved_selected_and_deleted` (scratch
install: record, folder, logo, reboot, delete).

### 9.5 Event format (`FUN_00469350`, `FUN_0046c720`)

`Events.txt` records are `id, name, description, tiers, tiers+1 model/message
pairs, trophy art, event art, arena, preview` and **three** numbers:
`+0x10c` robots per match (getter `0x469990`), `+0x110` robots advancing per
match (`0x4699a0`; `FUN_004698f0` sizes the next round), `+0x118` game type
(`0x46bfe0`; jump table `0x46bfa4`: 0 BATTLE ROYAL, 1 DEATHMATCH, 2 KING OF
THE HILL, 3 TABLETOP). The "fourth setting" of the earlier reader was the next
record's id. The bracket's play (`FUN_0046c720`) copies arena, game type,
robots per match and advancing robots into the match settings: the player's
match holds every opponent the bracket drew (4-bot events are 4-robot
matches). A round's field has no repeated team (`FUN_004695a0` redraws a
team already drawn). **Match length:** the bracket never sets one; the only
writer of the match time is the exhibition dropdown (`FUN_00474330` →
`FUN_00414230`) on the global game object, so an event match lasts the
constructor's 3 minutes (`0x41276d`) or the exhibition time last chosen this
session (`flow::event_match_seconds`). Tests
`ra2fmt::event::the_shipped_event_list_parses_to_the_end`,
`hq_mode::the_event_match_takes_its_game_type_and_field_from_the_data`,
`career::a_rounds_field_has_no_repeated_team`,
`playthrough::an_event_match_lasts_the_sessions_match_time`.

### 9.6 Judges at time-out

`ra2sim::r#match` already settles a time-out the original's way
(`FUN_00418770`: the unique best score among robots still alive, a shared or
zero best is a draw). The evidence script (`plus.eliminatePlayer` on the
weaker robot) now runs only for AI-versus-AI evidence sessions
(`Session::evidence_decision`), not for any match the player plays. Test
`match_mode::a_player_match_that_times_out_goes_to_the_judges`.

### 9.7 Esc on shell screens

The base window's key handler (`0x5ee6b0`) only forwards a key to the focused
control; the only handlers in the exe that test for Esc (`0x1b`) are the match
window's (`0x475f13`), the in-game popup's (`0x476a1b`) and the reminder's
(`0x482a23`). Esc on menus, the HQ and the lab now does nothing
(`flow::shell_escape`); test `playthrough::escape_on_the_shell_does_nothing`.

### 9.8 Native layout of the event flow and workshop; the bracket graphic

`hq_mode::Session::set_layout` and `Builder::set_size` lay the bracket,
award, repairs, season screens, the event match's HUD and the workshop out at
the window size (no letterbox); clicks are mapped into the same space. The
bracket (`ra2ui::hq::fill_bracket_at`, recovered from `FUN_0046ad60`): rows =
rounds + 1, 16 team pictures in the first row, picture size `min((w-192)/16,
(h-15·rows)/rows)` of the group less 6 px, later rows centred under their
match, 4 px black bracket lines with 13 px drops (`FUN_0046b4d0`), team logos
up to the current round and `questionmark.bmp` beyond it, a 2 px blue frame
3 px out round the player's picture (`FUN_0046b720`). Tests
`hq::the_bracket_group_is_filled_from_the_view` (now the graphic's cells and
lines), `hq::the_bracket_cells_follow_the_recovered_layout`.
Not verified against a capture of the original (none exists).

### 9.9 Smaller items

* **fixed** Options radio labels and multiplayer LAN/Internet labels: an
  `OptionGroup`'s `Text` children are baseline-placed like a group's
  (`ra2ui::tests::option_group_labels_sit_on_their_baseline`).
* **fixed** The create-game screen opens on `mpc_settings.gib`.
* **fixed** The workshop panel's rows were drawn above their own clip.
* **fixed** A robot built from an empty selection joins the roster
  (`playthrough::a_new_workshop_robot_joins_the_roster`; joining only when a
  part was attached or the name edited is this client's rule).
* **fixed** The robot in the current event is not editable (`FUN_004fee90`,
  `This robot is currently entered in an event and is not editable.`;
  `playthrough::the_robot_in_an_event_is_not_editable`).
* **fixed** Edit-team logo list (9.4).
* **fixed** A round's field without repeats (9.5).
* **not reproduced** The join-window notebook shift (§4).
* **closed in round 3** Resolution dropdown list, time dropdown list,
  weight-class filter, deathmatch slot 2 label, `Finish Later` before a round,
  event `End Match` result-4 path, the award's 3D prize scene, the original's
  AI robot choice per team in an event (§9.11-§9.17); the lab's drag placement
  (§9.18, `fx/workshop2`).

### 9.10 Gate (round 2)

`cargo fmt --all --check` and `cargo clippy --release --offline --workspace
--all-targets -- -D warnings` clean; `cargo test --release --offline
--workspace --no-fail-fast` **551 passed, 0 failed** (538 at the start of the
round); `find ../local/game -newer <stamp>` printed nothing (every test that
writes a team, a career or a robot runs on a scratch install).

## 9.11 Round 3 (`fx/play3`, 2026-10-20): the award's 3D prize scene

Everything in 9.11-9.17 is read from `Robot Arena 2.exe`; the original was not
run this round (no capture was added to `local/oracle`).

The award is the event-winner window opened by `FUN_00416960`, called from the
bracket's round hook `FUN_004164a0` when the event is over (`FUN_0046ce50`).
It builds `teamhq/event_winner.gib` as `EventResultsWindow` (`FUN_0046def0`,
0x184 bytes; its vtable `0x6a73ac` has only the destructor `0x46e110` of its
own, so nothing runs per frame) and calls `FUN_0046e140(1, ...)`, which starts
the **`Event Results` arena** under the window and fills it in:

* `FUN_0046e610`: `AddXtra("prize", <model>, "metal")` on the running arena.
  `<model>` is the event record's tier string: the record keeps its `tiers + 1`
  model/message pairs from `+0x6c` in 0x10-byte slots (`FUN_0046e6e0`), the
  window's `+0x178` picks the slot (`FUN_004164a0`: the rounds the team won,
  one more, the trophy, when it won the event).
* `FUN_0046eb70`: finds the arena object `banner Instance` and its `banner`
  batch, empties its texture stages (`FUN_005b2270`) and adds one whose texture
  is the team's logo (`FUN_00494a10` is the team's `+0x28` string,
  `FUN_005b1e60` adds the stage). The window's `+0x17c` is that team.
* `FUN_0046e8f0`, `FUN_0046e9c0`, `FUN_0046ea70`, `FUN_0046eb10`: the window's
  own controls (`team name`, `bot name`, `rounds won`, `team points`, `event
  img`, `prize description`); these were already in `hq_mode::award_screen`.

The scene is `Arenas/event_results.py`: `event_results.gmf`, black back colour,
DEATHMATCH only, `AddStaticCamera("fixed cam", (0, 3.95, -11.88), (0.206, 0),
45 * .015)`. Its two omni lights come with the model. The prize `.gmf` files are
authored in place (`Gold_Cup_Trophy.gmf`'s node sits at 2.31, 0.27, -1.89, on
the arena's winner spot), so `AddXtra` needs no placement, and no per-frame code
exists: the prize does **not** turn; the scene is a still. Implemented as
`ra2-app::prize_scene::PrizeScene` (arena items + prize items drawn with the
match's own `SceneCamera`: near 1, far 125, `d3d_pixel_centers`), held by
`hq_mode::Screen::Award`, drawn under the UI (`Session::render_3d`, the
window's `three_d` path). Without the install's scene the old arena-preview
backdrop remains as a fallback. Test
`hq_mode::the_award_scene_shows_the_prize_and_the_teams_logo` (the prize, the
banner logo and the scene each change the picture); a frame is
`--hq --simulate-event 0 --out DIR` (`award.png`: a six-pack of Fruity-Punch on
its plinth, the team's skull banner, the results window over it).
**Not verified against the original:** the prize's `Trophy_Collision` body may
settle after the arena starts (a static still is drawn), and the banner's team
is the player's team (the window's `+0x17c`, `FUN_00416940`).

## 9.12 Dropdown lists (`ra2ui::dropdown`)

A shipped `DropDown` is a 20-pixel field with an empty `ListBox`; the exe fills
the list when the owning window is built and opens it from the field or its
arrow. `ra2ui::dropdown` keeps the open state in the document itself (fields on
the dropdown and on the widgets it lifts: no clip, drawn above its siblings),
`Shell::click_dropdown` drives it for any registered dropdown (the rows are
`Shell::set_dropdown_items`; picks come back from `take_dropdown_picks`); the
scroll bar is the list's own `+Scroller`.

* **Match time** (`FUN_004733f0`): `No limit` (value 0) then 1, 2, 3, 4, 5, 10,
  15, 20 (the value of an item is its number, minutes), `3` selected
  (`FUN_005f1160`); the selection handler `FUN_00474330` stores the value in
  the global game object (`FUN_00414230`: `+0xdc`, flag `+0xd8`). The choice
  survives a trip to the arena tab. Test
  `exhibition::the_time_dropdown_opens_its_list_and_a_row_sets_the_match_length`.
* **Resolution** (`FUN_004a00b0`, called by the video tab's loader
  `FUN_0049f5f0`): one row per display mode the D3D device lists, text `WxH`
  (`0x6f5274` is `x`; with several colour depths for one size the row would also
  carry the depth, not reproduced). The device is the window's monitor here:
  `window::display_modes` reads winit's video modes at start (each size once,
  smallest first) and falls back to the standard 4:3/5:4 set when the platform
  lists none. A row writes `FSDisplayWidth/Height` and the `FS3D` pair.
* **Camera**: `watch`, `static`, `orbit` (the exe's three strings).
  Test `playthrough::the_options_dropdowns_list_their_rows_and_a_row_sets_the_value`;
  frames `dd-res.png`, `dd-time.png` (offscreen, see §0 for the route).

## 9.13 Weight-class filter

`weight class optiongroup` has `lightweight cb`, `mediumweight cb`,
`heavyweight cb` and `all classes cb` (the constructor's `ALLWEIGHT`).
`Shell::weight_choice` follows it (and survives the tab reload). Two effects
are in the exe:

* `FUN_0047b320` (run when the weight set changes, `FUN_0047ae70`): every slot
  whose robot's class differs from a chosen class is emptied
  (`FUN_0047b4b0(slot, -1, 0, 4)`). `Shell::eject_mismatched_robots`.
* `FUN_004ea040` (the picker's robot cells): a robot outside the filter is drawn
  at alpha 0.3 (`FUN_0061b4b0(1, 1, 1, 0.3)`, two images) and cannot be selected
  (`FUN_005ff960(0, cell, 0)`); the class is the `.bot`'s `Class:` (0 light, 1
  medium, 2 heavy). The picker opens on its first allowed robot.
Test `exhibition::the_weight_class_filter_empties_slots_and_dims_the_pickers_robots`.

## 9.14 A duel's open slots

Only BATTLE ROYAL and TEAM MATCH use all four slots; a duel shows slots 1 and 3.
The setup trace now says `slot 2: OPEN` / `slot 4: OPEN` for the others (the
lobby's `noplayer.gib` word `OPEN`) instead of repeating slot 1's robot
(`Choice::uses_slot`, `slot_line`). Test
`exhibition::a_duel_reports_slots_two_and_four_as_open`.

## 9.15 Finish Later before any round

The bracket's `later button` is `FUN_0046b9b0`: it calls `FUN_0046c720`, which
(when the team has a current event, `FUN_00416940 != -1`) copies the bracket's
settings (robots per match, advancing, game type, round data) into the team's
event record (`FUN_0046c3f0`, `FUN_004f9df0`) and broadcasts message 200 (the
same message as New Team, 9.4), which writes `Teams/Teams.txt`. So an event
entered and left at once is the team's current event next time. `Shell` calls
a later hook on `later button` with an entered event
(`hq_mode::shell_later_hook`): it enters the event in a fresh career and saves
(`Career::save`, `Teams.txt` plus the sidecar); a career already in that event,
or an event already played this season, is left as it is. Entering still writes
nothing. Test `playthrough::finish_later_keeps_the_entered_event_in_the_team_record`
(scratch install: the saved phase is `InEvent`, a fresh view shows it in
progress).

## 9.16 Event End Match (result 4)

The pause dialog's `end match button` (`0x476830`) closes the dialog with result
4; the Esc handler `0x475ee0` then switches on the game type (`FUN_004145f0`):
1 (multiplayer) goes to `prematch win`, **2 (event) runs `FUN_00418770(1)`**,
anything else shows `exhibition_setup_win` (`FUN_00475540`). `FUN_00418770` is
the judges' decision (9.6; the parameter only requires the robots' `+0x1c`
ids to be set, which every robot's is), so End Match in an event ends the match
on points at once and it goes on to its results and the award; it is no longer
an abandoned round. `Match::end_by_judges`, `match_mode::Session::end_for_judges`,
the window's `end match button`. Quit (result 8) runs the same function and then
leaves the game. Test
`match_mode::end_match_in_an_event_hands_the_match_to_the_judges`.

## 9.17 Which robot an AI team brings

The bracket's play handler `FUN_0046bb70` loops over the opponents and passes
each AI team (`FUN_004699b0`: entrant `i` of the bracket, a team of the AI
table) and the number `Team+0x118` of the player's team to `FUN_0046c090` ->
`FUN_004fa890`, which builds `AI\Team<N>\Bot<number>.bot`. `Team+0x118`
(`FUN_004faad0`; set by `FUN_004faac0`) is stored by `FUN_0046aa50` when the
bracket opens, from the `.bot` of the robot entered (`Teams\Team<N>\Bot<in
event>.bot` opened and parsed there), and the play handler clamps it to 0..=2
(`0x46be9e`-`0x46beb5`). Every shipped AI team's `Bot0`, `Bot1` and `Bot2` are
its light, medium and heavy robot (`Class: 0/1/2`), so the AI brings the robot
of **the player's weight class**. `hq_mode::match_config` now reads the entered
robot's class (`ai_bot_for_class`); it was `Bot0`. Test
`hq_mode::the_ai_teams_bring_the_robot_of_the_players_weight_class`.

## 9.18 Workshop drag placement (`fx/workshop2`)

The Components tab's flow as the exe runs it, traced from `BotLabComponents`
(`FUN_0042ac20` builds it; its handlers are `0x42b460` mouse button,
`0x42b590` mouse move, `FUN_0042c3a0` Attach, `FUN_0042c850` drag follow,
`FUN_0042d360` SHIFT, `FUN_0042d3e0` CTRL, `FUN_0042d4b0` drop,
`FUN_0042d280` pick, `FUN_0042f270` remove). Model: `ra2ui::workshop::placement`;
app: `ra2-app` `build_mode` (`Builder::mouse_moved`, `attach_pressed`,
`drop_click`). Everything below is derived from the exe except where it says
*ours*.

**Pick (`FUN_0042e650`).** A palette click only selects the part. It loads
`botlab\comp_preview.gib` into the screen's `component content group` (the
note on that group: *"contains either component preview, adjustments, or
appearance"*) and fills `component name textbox`, `component weight textbox`,
`component description` (a `<text font fg_mcn14 color 0 0 0 1>` box), the
`style dropdown group` (`model label` = `Component options (N)`, N = styles + 1;
hidden when the part has none) and the `component 3d preview`. Styles are the
definition's `styles = Label, file, Label, file, ...` line: the first label is
the palette's own definition, then a `Components\Styles\<file>` and its label
per style; choosing one swaps the previewed (and later placed) definition
(`FUN_0042f050`, `FUN_00462ec0`; stock bots use `Components\Styles\ztek2.txt`,
`anglemotor2.txt`, `smartzone2.txt`, ...). The preview draws
`UI\BotLab\AP_box_selected.gmf` on the chosen attachment point and
`AP_box_unselected.gmf` on the others, each at the point's own frame. A point
gets a box unless its type mask is exactly `axle_m` (0x10) or `generic_m` (4)
(`FUN_0045de80` maps the `Attach =` names to the bits `base_m` 1, `base_f` 2,
`generic_m` 4, `generic_f` 8, `axle_m` 0x10, `axle_f` 0x20): the plugs other
parts push onto are not offered as a part's own mount. The tip below the
preview is `0x6f07f4` for a part with fewer than two points, else `0x6f0828`.
Clicking a box chooses the part's own mount.

**Attach (`FUN_0042c3a0`, the button's note: *"click to begin attachment
dragging for placement"*).** The mass check first (`FUN_0042f790`: robot mass
+ the part's weight must not pass `DAT_006aa9a4`; the box says *"Total mass of
robot must not exceed 800 kg."*); then the drag starts (`this+0x15c = 1`), the
tip becomes `0x6f04f8` (*"Drag the component where you want it to go, then
click. Hold shift to rotate the component. Hold control to raise/lower the
component."*) and the part is placed where the mouse already is
(`FUN_0042c6b0`).

**Drag (`FUN_0042c850`).** On every mouse move the part follows the mouse ray.
It lands on a free, compatible attachment point of a placed part (the chassis'
baseplate is the chassis' own point), or, over nothing, floats 3.0 m along the
ray (`0x40400000`) and is not droppable. Each position is judged in this order
and the part is painted red (`FUN_0042c780`, overlay `1 0 0 0.3`) while it is
refused, the tip being the refusal:
collision with the robot (`FUN_004a8660`, a physics query) gives *"Components
should not intersect the chassis or other components."* (code 3), or *"Robots
cannot exceed a maximum size."* when it still collides with the arena's size
box (code 2); a part origin farther than 3.0 m (`0x6a6078`) or lower than
-1.5 m (`0x6a6074`) gives the size text (code 4); and for a chassis child the
chassis' own point test (`vtable+0x1c`, two points) gives *"Baseplate
components must fit inside the chassis."* (code 5).

**SHIFT and CTRL (`0x42b6b4` -> `FUN_0042d360`, `FUN_0042d3e0`).** The mouse
movement is reduced to its dominant axis (the larger of |dx| and |dy|, signed,
in whole pixels). With SHIFT held, `delta * 0.2` (`0x6a6080`) is added to a
counter (`this+0x11c`) that wraps at 36 (`0x6a5a68`; below zero it adds 36),
and the part's turn is `trunc(counter) * 10 deg` (`0x6a5df4 * 0x6a607c`). With
CTRL held the same product goes to a second counter (`this+0x120`), clamped at
0 below and at the chassis height (`+0x2ec` of the chassis object) above, and
the lift is `trunc(counter) * 0.05 m` (`0x6f035c`); the stored chassis-child
`angle` and `height` are exactly these. While a modifier is held the part stays
where it is and is re-judged.

**Drop (`FUN_0042d4b0`, on the button release).** Over something and not
refused: the part is added to the robot (`FUN_004dbf40`), the attach sound
plays, the mass readout updates (`FUN_0046c710`), and the drag ends. Over
something but refused: nothing happens, the part stays on the drag. Over
nothing: the drag is cancelled (`FUN_0042c280`).

**Select, remove, move.** A click with no drag picks the placed part under the
mouse (`FUN_0042d280`, each part's own pick), or nothing. The remove button
(`FUN_0042f270`) acts on that selection: a part with parts attached to it is
refused with a message box titled `Remove Component` (*"You must delete the
components attached to this first."*, `FUN_004db920` is the leaf test);
otherwise it plays `Sounds\cmp_removal.wav`, removes the part
(`FUN_004dbc80`) and updates the mass. **There is no move**: to put a part
elsewhere the player removes it and attaches it again. `heading_arrow.gmf` is
not part of the drag: `FUN_00437770` (the lab window's constructor) loads it
once as a model of the window (`this+0x1c4`); it belongs to the chassis'
heading slider, not to placement.

**In the client.**

* A palette click selects (the tip `TIP_BOXES`/`TIP_ATTACH`), the shipped
  `comp_preview` group is loaded into `component content group` and filled, the
  preview renders the part and its two green boxes (`AP_box_*.gmf`), a click on
  a box (nearest within 30 px) chooses the own mount, and the shipped `attach
  button` (redrawn over the preview, as it is the placeholder's child) starts
  the drag. The `Component option` dropdown is `ra2ui::dropdown` driven through
  `Shell::set_dropdown_items`; the lab's document is now reachable by the
  shell's dropdown click (`click_dropdown`).
* The drag: `Builder::mouse_moved` (the window forwards the cursor and the
  SHIFT/CTRL state) re-picks, re-plans and re-judges; the part is drawn in the
  lab preview at its candidate transform (the model's own mount rule on a copy
  of the component list), red when refused, floating when over nothing; the
  framing ignores it. A click in the preview drops it. *Ours:* the drop is on
  the button press, not the release (the Attach press is on another widget, so
  the order does not matter); the pick radius around a socket (9 cm), the
  overlap test (oriented boxes on the parts' mesh bounds with 5 cm of slack,
  parent skipped, instead of the physics query; on the 57 stock bots 1 in 100
  non-adjacent pairs still exceed it, and every chassis child fits), the
  chassis fit (the part origin and its mesh-box centre inside the bottom
  outline; the exe tests two points) and the pick of a placed part (mesh
  boxes) are ours. The size cap's second form (code 2, the arena box) has no
  counterpart.
* The old behaviour is gone: the palette click no longer attaches, the preview
  click no longer moves a part, and the panel's *Attach to* mount rows and
  `rotate`/`lift` rows are removed. Remove takes the selected part, refuses one
  with children, and no longer falls back to the last part.
* Saving is unchanged: the dropped part is a `Component` with the stored
  `parent`, `parent_mount`, `own_mount`, `angle` and `height` the game writes
  (`ra2fmt::bot::write`); a test reads the saved bytes back to the same
  components and the same bytes.

Evidence: `ra2ui::workshop::placement` tests (SHIFT/CTRL arithmetic and wrap,
the mount bits, box overlap, polygon fit, drop/refuse/snap/remove/pick/mass
rules, and the 57-stock-bot check), `ra2-app` `build_mode` tests
(`a_palette_click_selects_and_only_a_drag_attaches_one_component`,
`shift_and_control_turn_and_raise_the_dragged_part`,
`a_refused_spot_keeps_the_drag_and_a_click_over_nothing_ends_it`,
`attach_refuses_a_part_that_would_pass_the_mass_limit`,
`a_click_selects_a_placed_part_and_remove_refuses_one_with_children`,
`a_click_on_a_green_box_chooses_the_parts_own_mount`,
`the_style_list_swaps_the_definition_the_part_is_placed_with`) and the scripted
build (`--build --out`), whose frames `component-selected.png`,
`component-drag.png`, `component-refused.png` and `placement.png` show the
flow. Gate: `cargo fmt --all --check` and `cargo clippy --release --offline
--workspace --all-targets -- -D warnings` clean; `cargo test --release
--offline --workspace --no-fail-fast` **589 passed, 0 failed** (561 at the end
of round 3); `find ../local/game -newer <stamp>` printed nothing. The original
was not run for this item: addresses and constants are from
the exe only; a live trace of one drag would confirm the SHIFT/CTRL input axis
and the exact judge order.

## 9.19 Gate (round 3)

`cargo fmt --all --check` and `cargo clippy --release --offline --workspace
--all-targets -- -D warnings` clean; `cargo test --release --offline --workspace
--no-fail-fast` **561 passed, 0 failed** (551 at the start of the round);
`find ../local/game -newer <stamp>` printed nothing (every test that writes a
team, a career or a robot runs on a scratch install, and the options tests write
a temporary `.cfg`). New tests: `hq_mode::the_award_scene_shows_the_prize_and_the_teams_logo`,
`hq_mode::the_ai_teams_bring_the_robot_of_the_players_weight_class`,
`match_mode::end_match_in_an_event_hands_the_match_to_the_judges`,
`exhibition::{the_time_dropdown_opens_its_list_and_a_row_sets_the_match_length,
the_weight_class_filter_empties_slots_and_dims_the_pickers_robots,
a_duel_reports_slots_two_and_four_as_open}`,
`playthrough::{the_options_dropdowns_list_their_rows_and_a_row_sets_the_value,
finish_later_keeps_the_entered_event_in_the_team_record}`.


## 9.20 Round 4 (`fx/shellcap`, 2026-10-21): the screens of rounds 2-3 against the original

Rounds 2 and 3 implemented the award scene, the options dropdowns, the pause
menu, the F1 reminder, the event screens and Edit Team from code only. This round
ran the original (flatpak Wine 9.0, Xephyr `:5`, 1024x768, `ra2_drive.py`) through
every one of them, added the captures to `local/oracle/original` (33 rows,
`manifest-additions-2026-10-21-shellcap.tsv` in `~/ra2-oracle-backup/oracle`) and
compared ours with `tools/oracle/fidelity.py` (new rows in `FIXTURES`:
`routes` for the click-driven states, `hqsim=(row, frame)` for the event flow,
`matchquit=frame` + `box=` for the pause dialog and the reminder over a match).

### Captures (`local/oracle/original/<id>__1024x768__000.png`)

Options: `options-video`, `options-audio`, `options-resolution-open`,
`options-resolution-end` (scrolled by ten arrow clicks), `options-camera-open`.
Exhibition: `exhibition-time-open`, `exhibition-weight-light`,
`exhibition-pick-ai-light` (the picker with LIGHTWEIGHT set), `exhibition-robots-filled`.
Match: `match-pause`, `match-pause-options` (the Options window over the paused
match), `match-f1`, `match-results-event`. Team: `teamhq-newteam`,
`teamhq-editteam`, `teamhq-event-join`, `teamhq-event-join-robot`,
`teamhq-events-in-progress`, `teamhq-events-one-event`, `teamhq-events-completed`,
`teamhq-events-season2`, `teamhq-league-season2`, `teamhq-history-season1`.
Event flow: `event-bracket-tabletop`, `event-bracket-tabletop-final`,
`event-bracket-duel-round1`, `event-bracket-duel-round3`, `event-repairs`,
`event-award-photo`, `event-award-plaque`, `event-award-plaque-2of2`,
`event-award-trophy`, `season-golden-sawblade`. The whole season was played
(nine events, 25 wins, eight titles) by eliminating every non-human player with
`plus.eliminatePlayer` (`plus.isHuman` picks the human) and clicking through; the
prefix's `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` were restored to their
prior bytes afterwards (hash-checked).

### What the original does that the code reading missed (and what was done)

* **The award scene has the team's robot standing on the pad.** `FUN_0046e140`
  also calls `FUN_0046e710`, which adds the entered robot as a real player of the
  running `Event Results` arena (`FUN_00439be0`); it stands at start point 0 on
  the orange rectangle and its yaw keeps changing from frame to frame (every
  capture shows a different yaw), so it is not a still. Ours now places the
  robot on start point 0 of `event_results.gmf` (`prize_scene::place_robot`,
  floor found by a ray on the arena triangles); the yaw drift is **not**
  reproduced (gap; see 9.23: the robot is a free physics body).
* **The award window is laid out on the real screen.** The headless event run laid
  it on 1280x1005 and resampled; the original's window is 250 px wide at 1024.
  `hq_mode::run` now sets `set_layout(size)` (award MAE 8.57 -> 5.68 with the robot).
* **Resolution list** (`FUN_004a00b0`): rows are `WxHxDepth` (`FUN_004192a0(w, 'x',
  h, 'x', depth)`), orange (`FUN_0061b4b0(1.0, 0.5, 0, 1.0)` into `FUN_005f0f90`
  before the rows are added), five 32-bit sizes (640x480, 800x600, 1024x768,
  1280x1024, 1600x1200) then the same five at 16 bits, ten rows, five visible,
  the bar's arrows and thumb drawn, the field `1024x768x32`. Camera list order is
  **watch, orbit, static** (ours had static before orbit); rows orange.
  `options.rs` (`mode_text`, `DEPTHS`, `CAMERAS`), `Shell::set_dropdown_row_color`,
  `dropdown::set_row_color`.
* **A dropdown's scroll bar** lives in a hidden wrapper (`Visible false`,
  `ClipMode 2`) that the list's `+Scroller` carries; `input::set_scroller_visible`
  showed the bar but not the wrapper, so no list ever had one (exhibition time,
  resolution, logo list). Fixed.
* **`TextBox Buffer N`** insets left-aligned text (the resolution field says
  `Buffer 8`): `ra2ui::place_core`.
* **Edit Team was unbound**: the HQ's `edit team button` fell through to a screen
  `Goto` of `editteam.gib` (a black page with the placeholder window). It is now
  the same modal as New Team, captioned `Edit Robotics Team`, filled from the HQ's
  team (name, motto, logo selected and previewed), and OK rewrites the record
  (`teams::update_team`; `playthrough::edit_team_shows_the_teams_own_record_and_ok_rewrites_it`).
  The logo list shows its bar. MAE 56.6 -> 1.9.
* **The bracket's background** is the event's arena preview
  (`Arenas\flextop\tts_preview.bmp`, filtered); `background img` sits in a
  `Background` attachment body, which the child-only search never entered, so the
  placeholder `sample_arena.bmp` was always shown (`event_screens::find_mut_attached`).
  MAE 26.8 -> 7.1.
* **F1 reminder**: the 3x3 grid draws its cell lines (`BorderColor 0.4`,
  `BorderWidth 1`, `BufferWidth 1`) around all nine cells, empty ones too
  (`reminder::empty_cell`). F1 toggles as coded (shown, hidden, shown).
* **`Teams.txt` written by the original could not be read.** After the schedule is
  drawn the original writes `+0xf4` = true and the nine ints on **one line** with a
  trailing space (`2 6 12 9 13 15 5 22 20 `), not nine lines; `team::parse_table`
  rejected it (`bad history value`) so every career screen of a played-in install
  failed to open. It now accepts both (`team::tests::the_originals_own_save_with_a_schedule_parses_and_round_trips`).
* **Pixel snapping.** Comparing the exhibition panel's edges (original 256..986 x
  120..607, ours 256..987 x 120..608) showed the original truncates both edges of
  every box to whole pixels; our fractional frames put the lower and right parts
  of fractional layouts one to two pixels out. `ra2_render::ui::scene` now snaps
  every placed frame and clip to `floor(x), floor(x + w)` for drawing (hit tests keep
  the exact frames). Mean over the 19 screens of the existing fixtures 3.66 -> 2.75
  (main menu 0.59 -> 0.09, Team HQ history 2.45 -> 0.37, inventory 2.80 -> 1.85,
  exhibition settings 4.07 -> 2.65, standings 6.02 -> 3.32, events 8.42 -> 6.96,
  bot lab 5.1-6.6 -> 4.8-6.0). Tried and rejected: flooring the fractional lengths
  (better exhibition, worse Team HQ), and both together (3.16).

### MAE per screen (ours vs original, 1024x768; the "before" binary is `fidelity` 597a227)

| screen | before | after |
|---|---:|---:|
| options-video / audio | 1.24 / 1.30 | 0.66 / 0.79 |
| options-resolution-open / end | 1.53 / 1.75 | 0.72 / 0.75 |
| options-camera-open | 1.51 | 0.89 |
| exhibition-time-open | 4.28 | 2.78 |
| exhibition-weight-light | 4.05 | 2.62 |
| exhibition-pick-ai-light | 2.32 | 2.07 |
| teamhq-newteam | 7.84 | 1.33 |
| teamhq-editteam | 56.56 | 1.89 |
| teamhq-event-join / -robot | 36.6 / 36.5 | 4.76 / 4.63 |
| event-bracket-tabletop / duel-round1 | n/a (screen could not open) | 7.15 / 7.01 |
| event-repairs | n/a | 10.04 |
| event-award-photo | 8.57 (round 3 build, 1280 layout) | 5.29 |
| match-pause (dialog box) | 5.64 | 5.64 (the dialog is pixel-identical; the box includes the live match) |
| match-f1 (reminder box) | no frame | 9.31 (different robot, ALARM v Scout) |

The "n/a"/36-56 rows of the "before" column are the Teams.txt reading bug above
(the career screens did not open on the original's own file).

### Gaps

* The award robot's yaw drift; the bracket's entrant order (the original draws
  its opponents from its own random state; ours is the oracle schedule); both are
  still open after 9.21 (both done in 9.23) (the black bands turned out to be already equal).
* **Repairs** (see 9.21): the robot view, the border and the undamaged state are done.
* **One event at a time**: clicking another event while one is in progress opens a
  message box (caption `Robot Arena 2`, OK) whose body was empty in the capture,
  and the original then stopped responding (CPU busy, no repaint) twice in a row
  under Wine. Whether that is the Wine/Frida setup or the exe was not settled;
  ours shows the recovered text (`ONE_EVENT_AT_A_TIME`).
* Tool notes: after any `frida` timeout the next `ra2_drive.py` call works only
  once `frida-server` is restarted; never `pkill -f frida` (it matches the shell
  and kills the sandbox); a `flatpak enter` helper left from a killed attach keeps
  the game ptrace-stopped.

### 9.20 Gate

`cargo fmt --all --check` and `cargo clippy --release --offline --workspace
--all-targets -- -D warnings` clean; `cargo test --release --offline --workspace
--no-fail-fast` **573 passed, 0 failed** (561 at the start of the round); `find
../local/game -newer <stamp>` printed nothing. New tests: `team::tests::the_originals_own_save_with_a_schedule_parses_and_round_trips`,
`playthrough::edit_team_shows_the_teams_own_record_and_ok_rewrites_it`; the options dropdown test
now expects the `WxHxDepth` rows and the watch/orbit/static order.

## 9.21 Round 5 (`fx/polish`, 2026-10-21): repairs, borders, centred groups, F1 grid, workshop panel

Numbers are `tools/oracle/fidelity.py --renderer app` (ours vs the original, 1024x768), before = `fidelity`
c014140 + the harness fix, after = this branch. Nothing new was captured from the original this round.

* **Repairs (`0x46ce50`, `teamhq/repairs.gib`).** The `3d view group` (`repairs 3d placeholder`, "show bot needing
  repairs here") now shows the entered robot straight down, nose up, on a (25, 25, 25) backing (measured in the
  reference), drawn over the placeholder by `Session::prepare_repairs` / `build_mode::robot_top_view` (the lab's
  own view). The original's robot is a different design (the capture's team), so the picture differs inside the
  box. An empty bench opens with the 30:00 clock (`FUN_004d62a0`; the reference reads `30:00` with no rows).
  The harness state is undamaged: `ra2-app --hq --simulate-event N --bench-undamaged` opens the bench with no
  pending repairs (fidelity.py passes it). MAE 9.97 -> 5.63 (with the offsets below; 10.05 with only the robot).
* **`Border` with `Width >= 2`.** Every ring is `Outer` (the repair bench's wood frame, `Width 2`, `Inner 1 1 1 1`,
  `Outer 0.55 0.45 0.3`, is two pixels of (140, 114, 76) in the reference, no white one), and its `Background`
  stretches over the frame grown by the border (`ra2ui::background_fill`): the wood tile phase then matches
  (shift search 1.0 MAE at (+1, +2) became 0.9 at (-1, 0), i.e. only the next item). Inventory 1.85 -> 1.75,
  event join 4.76 -> 4.50, events 6.01 -> 5.91.
* **Centred groups anchor in whole pixels** (`ra2ui::sized`): a `Group`/`Window` with an anchor places itself at
  `parent + position - trunc(anchor * trunc(size))`. Measured, not read from the exe: `main group` of the bench
  (0.9 of 1024 = 921.6 wide at `Position 0.5`) starts at 52 (512 - 460), not 51.2, and every widget inside it was
  one pixel left of the reference; the F1 window had the same +1. Over the 104 app fixtures (mean 3.68 -> 3.58):
  exhibition settings 2.65 -> 2.45, arena list 1.57 -> 1.45, robots 0.95 -> 0.75, deathmatch 2.57 -> 2.36,
  pick-ai 2.15 -> 1.95, team select 1.29 -> 0.92, new team 1.33 -> 0.96, bot lab 4.8-6.0 -> 4.5-5.8, bracket 7.15 /
  7.01 -> 6.21 / 6.00, repairs 8.66 (with the border) -> 5.63, F1 9.31 -> 6.83. Worse: `teamhq-events` 5.91 -> 6.36
  and the event join window 4.50 -> 4.84 (the cells of those grids sit on other rules). **This is the cause of the
  exhibition screens' remaining 2-3 MAE layout offset** (item 6): the box's width is fractional, the original
  halves a truncated width. Tried and rejected: truncating sizes and rounding every position (3.77), applying the
  rule to every widget kind (the standings background lost 2.67 -> 4.47), the vertical axis (events 7.0).
* **F1 reminder grid.** The nine cell lines are one pixel each (lines at x 388, 471, 554, 637 and y 395, 461 in the
  capture: the cell is `floor(grid / 3)`, 83 px), not a double line; a control's cell sits one pixel inside its
  line (`BufferWidth 1`), so the key bars are 80 px in the 83 px pitch (ours were 82-83). Lines are pixel-exact.
* **Workshop (Bot Lab).** The synthetic text panel (parts list, `Save as:` row, tab text rows) under the robot view
  is gone from the player's lab: `flow::open_workshop` and `window::run_build` call
  `Builder::apply_debug_panel_default`, which leaves the robot view filling the screen's own 3D placeholder (the
  harness's `view_only`). `--debug-panel` brings the panel back; the scripted `--build --out` demo keeps it because
  it drives the model through its rows. Test `build_mode::the_player_route_hides_the_synthetic_panel_unless_asked`.
  The bot-lab MAE rows are unchanged by this (the harness already ran view-only).
* **Award robot yaw: not done.** `FUN_0046e710` builds the robot with `FUN_00439be0` -> `FUN_004a5c40`, `FUN_004fa890`
  (the AI-robot builder of 9.17), so the pad robot is an AI player of the running arena; `event_results.py` has
  no per-tick code that turns it (`Tick` only counts to 3.5). The four award captures show different yaws and
  positions (sampled at unknown times), so a rate cannot be read from them. The likely driver is the AI idling
  without a target; verifying it needs timed captures of the award scene (not taken).
* **Bracket: not done.** Measured the bands: the top black band is already opaque (0 in both), the bottom band
  differs by 3-6 levels at y 676-692 only, below any visible difference. The entrant order is drawn by
  `FUN_004695a0` from the engine LCG at `0x726d58`, whose state at that moment depends on every earlier draw
  (particles, menus, the schedule); it is not determinable from the code, only fittable per capture.
* **Flamethrower flame: not done** (docs/37: the weapon's gameplay is not in the sim, so an emitter would have
  nothing to follow).
* **Process note.** Career-saving harness modes (`--hq --simulate-event`) must run on a scratch install; before the
  harness fix two runs of this round wrote the real install's career (restored by the coordinator).

### 9.21 Gate

`cargo fmt --all --check` and clippy (`-D warnings`) clean; `cargo test --release --offline --workspace
--no-fail-fast` **596 passed, 0 failed**; `find ../local/game -newer <stamp>` printed nothing. The anchor rule
is limited to centred (0 < anchor < 1) groups after two `ra2ui` tests (the off-screen count and the bottom-right
step layer) failed with it on every anchor.

## 9.22 Round 6 (`fx/ui-numbers`, 2026-10-21): the schedule grid and whole-pixel groups

Numbers are `tools/oracle/fidelity.py --renderer app` (ours vs the original, 1024x768, 104 fixtures), before = `fidelity`
ffcae47, after = this branch. Nothing new was captured for the shell.

* **The schedule grid was drawn with per-cell rings.** Measured in `teamhq-events` (1024x768): the grid is **552 x 333** px
  with ONE black pixel line shared between neighbours: vertical lines at x 341, 525, 709 (the left of each column, none at
  the right edge, 334 rows long) and horizontal ones at y 318, 429, 540, 651 (the top of each row and one under the last);
  the art starts `1 + BufferWidth 2` = 3 px inside a line and stops 2 px short of the next (344..522, 321..426). Ours gave
  every cell its own ring, so neighbours had two lines three pixels apart and the art 1 px off. `Hq::fill_events` now puts
  the cells at `3/184 x 3/111` of their slot with `179/184 x 106/111` of its size and adds `grid line column N` /
  `grid line row N` rectangles (`grid_line`). Test `hq::tests::the_schedule_grid_has_shared_lines_and_art_inside_them`.
* **A split's children keep their authored extent across the split.** `fit_splits` made every child of a split
  `Size 1 1 1 1`; the calendar's `Slot1` is `Size 0.95 1 198 0` in a top/bottom split, i.e. **0.95 of the pane wide** (552
  of 581.. px, not the pane's 583). Only the axis along the split fills the pane now (`hq::fit_splits`).
* **A `Group`/`Window` is whole pixels wide and tall** (`ra2ui::sized`): `frac * parent` is truncated. With that the grid is
  552 wide at 1024x768 (it was 553.9) and the exhibition panels land on the capture's edges. The Team HQ's own chrome
  groups (`EXACT_GROUPS`: `hq content group`, the four slots, `team heading group`, `kills group`, `logo grp`, the
  inventory's `robot cell group`, `bot img group`, `grid grp`) keep the exact fractions: the captures of the inventory,
  history, standings and edit-team screens sit on them (truncating them cost those four 0.1-0.4 MAE). Not read from the
  exe; fitted per fixture (variants tried: all groups, width only 5.21, height only 5.35 for events; rounding 6.31).
* Numbers (MAE, before -> after): teamhq-events **6.35 -> 3.20**, event join 4.84 -> **3.25**, join robot 4.71 -> **3.12**,
  exhibition robots 0.75 -> 0.06, settings 2.45 -> 1.68, deathmatch 2.36 -> 1.60, pick-ai 1.95 -> 1.48, time list open 2.58
  -> 1.83, weight light 2.42 -> 1.65, pick-ai light 1.87 -> 1.40, bot lab wiring 4.96 -> 4.36, paint 5.72 -> 5.40, bracket
  6.21 / 6.00 -> 5.82 / 5.59, repairs 4.96 -> 4.74, F1 6.83 -> 6.69; teamhq inventory / standings / history / edit team
  unchanged (1.77 / 2.67 / 0.24 / 1.80 against 1.75 / 2.71 / 0.24 / 1.79); **arena box 1.07 -> 1.28** is the one regression.
  Mean over the 104: **3.663 -> 3.537**.
* **Open.** The schedule's rows are one pixel short (lines at 318, 429, 541: the pane is 334 px tall where the capture's
  is 333); the capture's `Right TB split` / content group heights sit about 3 px under ours (420 against 422.9) and the
  vertical chain is not recovered. The join window's robot row still differs by a selected robot (ours has C.R.U.S.H.
  selected, the capture none).

## 9.23 Round 7 (`fx/shell-order`, 2026-10-21): the award robot is a free body; the bracket is the engine LCG's draw

Live runs on display :9 (flatpak Wine 9.0, `ra2_drive.py` plus the new agents `tools/oracle/frida/ra2_lcg.js` /
`tools/oracle/capture_lcg.py` and `tools/oracle/capture_award.py`; the prefix's `Teams/`, `AI/`, `Robot Designs/` and
`ra2.cfg` hash-checked identical afterwards). Captures added to `local/oracle/original` (20 rows,
`manifest-additions-2026-10-21-shell-order.tsv` in `~/ra2-oracle-backup/oracle`): `event-bracket-lcg-blade`,
`event-bracket-lcg-final`, `event-results-lcg-final`, `event-results-lcg-sawblade-r1/-r2`, `event-award-atom-t4/-t20/-t40/-t58`,
`lcg-bracket-blade`, `lcg-advance-blade`, `lcg-bracket-sawblade`, `lcg-advance-sawblade`, `lcg-match-start`, `lcg-idle-menus`,
`awardpose-crush-pad` (000-002), `awardpose-crush-drop`, `awardpose-atom-pad`.

### The award robot (task 1)

* **It is not turned by anything.** `plus.getHeading/getLocation/getDirection/getSpeed` of the award arena's player 0, every UI frame
  for 50-60 s in six award scenes: the heading starts at **2.7925 rad = 160 degrees exactly** (start point 0's facing, the
  pad's `+Z` turned by the point's transform) and the body starts at the start point's xz, **0.226 m above where it rests**
  (C.R.U.S.H.: y 0.675 -> 0.449 in 0.35 s, a bounce to 0.394 and back), then creeps. No hinge motor is ever set in the scene
  (`ra2_hinge_motor.js`: no `0x4815c0` call, no motor-on atom) and `Arenas/event_results.py` only counts a timer: the robot
  is a player of the running arena that nothing drives, standing on free wheels, and the arena's physics runs on. The camera
  does not move. The speed column jitters by +-0.2 m/s (solver noise on the wheels) in every scene.
* **What it does depends on the robot.** C.R.U.S.H. (four wheels, flat): heading 2.78 +- 0.02 for 50 s in one scene, +0.1 rad
  in 50 s in two others (0.002 rad/s), x drifting 0.1 m. Atom Smasher (nose heavy, rests tilted on its bumpers): a steady
  **-0.0145 rad/s** (-0.83 degrees/s; 2.79 -> 1.98 in 56 s, 47 degrees a minute), x drifting +0.3 m in 56 s. The earlier
  captures' "different yaw every time" are scenes sampled at unknown times: the 2-of-2 plaque frame (robot turned to the
  camera) is simply several minutes in.
* **Implemented:** `prize_scene::PrizeScene` now holds a `ra2sim::Duel` of the Event Results arena with the entered robot on
  start point 0 (the match spawn), lifted by `SPAWN_LIFT` 0.122 m so it falls the 0.226 m live shows (the duel's own spawn
  is 0.06 m above the floor: ours falls 0.104 m; **matches spawn at 0.19 m live** (four robots, flextop: y 0.381 -> 0.19), so the
  duel's spawn height is too low for every match; left alone here, the reference fights depend on it), and steps it with the
  window's 50 ms tick (`HqSession::advance` -> `PrizeScene::advance`, 60 Hz steps, no driver, free wheels). The still render
  (`--hq ... --out`) advances 3 s first so the robot has landed. `Duel::robot_groups` is the new shared draw call. Test
  `prize_scene::tests::the_award_robot_drops_settles_and_creeps` (start heading, a fall of more than 0.15 m, creep below
  0.03 rad/s, stays on the pad).
* **What does not match.** Our free body creeps at the same order (C.R.U.S.H. random +-0.1 rad a minute, Atom Smasher
  **+0.0145 rad/s**, steady, in all four lifts tried) but the Atom Smasher turns the **other way** and drifts to -x where
  the original drifts to +x: a mirror image of the live motion, which says a sign or a handedness is wrong in the
  free-wheel/contact physics of an asymmetric robot (not in the award code). It was not chased (the Havok side belongs to
  the physics rounds). The creep is chaotic in both (the original's three C.R.U.S.H. runs differ), so a frame match at a
  given time is only possible to a few hundredths of a radian at the start.
  *Resolved (docs/29 section 12t):* the sign came from the order our resolver met the contacts in; with the
  broadphase's order (sorted along x) the Atom Smasher turns -0.0145 rad/s and drifts +x, as live. Timed frames: live `event-award-atom-t4..t58`
  against ours is a robot turning one way against the other; `event-award-photo` (C.R.U.S.H., ours 3 s in) **4.11 -> 3.99**
  (the robot is now on the floor the physics finds, not the visual floor).

### The bracket's random draw (task 2)

* **The generator.** `FUN_00419220` (`0x419220`): `s = s * 0x41c64e6d + 0x3039` on the 32-bit global at `0x726d58`;
  `FUN_00419190(n)` = `floor((s & 0x7fffffff) / 2^31 * n)` (`0x419200`; `2^31` is the double at `0x6a5850`), clamped to
  `n-1`. `ra2sim::lcg::Lcg` (`step`, `range`; tests against the live trace). `FUN_00500120`'s schedule draw
  (`draw_schedule`) is the same.
* **Seeding and consumers (Frida: every store to `0x726d58`, 27 sites, in `ra2_lcg.js`).** Seeds: `GetTickCount()`, twice, when the
  main menu is built (`0x4e456a`/`0x4e4575`) and `time(0)` at start-up (`0x61bb3a`). Draws: the shared step (called only from
  `FUN_00419190`'s 12 callers, among them the bracket `0x4696d1`, the winners `0x46ce18/37`, the schedule `0x50016c`), and
  inline copies of the step in the particle/effect code (`0x59ec56`, `0x5bb77f`, `0x5bc2ea`..`0x5be167`). **The menus, the Team
  HQ, the schedule, the event picker and the bracket draw nothing while idle** (75 s logged: no draw), but a match draws
  thousands (**6700** in the ~15 s from Play Next Round to the countdown: 5600 from `0x5bb77f`, the rest from the particle
  sites; per frame, so it scales with the time spent). So the state at a
  bracket is the clock's seed advanced by whatever the effects drew before: **not reproducible across sessions** (the
  first bracket after a fresh team is not the schedule's stream either: the state that gave the captured tabletop field
  is 1.87e9 draws from the schedule's). Within a session it is a function of the state only: setting the state to the
  same value (`setstate`) gave a bit-identical 46-draw sequence twice.
* **The bracket (`FUN_004695a0`), verified live.** One call draws every round. Round `r` has `16 / bots^(r+1)` matches of
  `bots` slots (the `16.0` / `+0x110/+0x10c` loop at `0x4695c9`); every slot but match 0's first is `FUN_00419190(15)`, redrawn
  while the team is in **that round's** list (`FUN_005014d0` on a list that starts empty each round). From state 4037340146
  the Blade Tournament (4 bots, 2 rounds) took exactly **46** draws = 15 + 3 accepted + 28 repeats, giving
  `[P 4 13 3 | 9 2 8 10 | 6 1 7 12 | 0 5 11 14]`, the 16 icons of `event-bracket-lcg-blade`; Sawblade Elite (2 bots, 4 rounds)
  **59** = 15 + 7 + 3 + 1 accepted. Only round 1's field survives.
* **Advancing (`FUN_0046c830`, `FUN_0046cde0`).** The next round has one slot per match of this one; slot 0 is the
  player (no draw), slot `k` is entrant `FUN_00419190(bots)` of match `k`: 3 draws after the Blade's round 1 (picks
  2, 1, 3 of 4 -> High Voltage, Prehistoric, The Scrappers = the final's opponents on `event-results-lcg-final`), 7 after
  Sawblade's (picks `1 0 1 0 1 0 0` -> round 2's opponent Team Dragon, `event-results-lcg-sawblade-r2`). A lost player
  makes slot 0 a winner other than entrant 0 (`FUN_0046cde0`'s second mode, redrawn while 0); that branch is from the code,
  not seen live. Between the creation and the advance a match draws thousands of numbers, so a later round's opponents
  depend on how long the match ran: **timing-dependent in the original**; ours continues the stream without that gap.
* **Implemented (`ra2sim::career`).** `EventBracket` draws and advances with `Lcg` exactly so (`draw_field`, `advance_field`,
  `replay_to` for a reload); `Career::enter_event` hands the bracket the session's state (`Career::seed`, now the engine
  state) and keeps the state after its draws, so the next event's bracket differs and the same session actions give the
  same brackets. The state at the start is `hq_mode::SEED` (7) unless **`RA2_LCG_STATE`** names one (the live state read by
  Frida reproduces the live bracket). The old per-round hash RNG is gone. Tests
  `career::tests::{the_engine_lcg_draws_the_bracket_the_original_drew_live, the_two_bot_bracket_and_its_winners_match_the_live_draws,
  a_session_state_gives_the_same_brackets_again}`, `lcg::tests`. Fidelity (`--renderer app`): `event-bracket-lcg-blade` **5.97 -> 2.67**
  (state 4037340146); the older `event-bracket-tabletop` **5.82 -> 2.68** with state 1361071406, the (only, up to the top bit)
  state whose 15 draws give its captured order, found by search over all 2^32 states: a fit, but an algorithm that were wrong
  would have had no solution (about 0.3 % chance for a random 15-team order). `duel-round1` is unchanged (unknown state, ~5.7).
* **Tools.** `tools/oracle/capture_lcg.py --out F.jsonl <drive steps with mark LABEL / setstate N>` logs the draws with one frida
  client (two concurrent clients wedged frida-server and once killed the game); `capture_award.py` samples the pad robot every UI
  frame while taking screenshots. `fidelity.py` fixtures take `env=`; `RA2_BIN` picks the binaries.

## 9.24 Round 8 (`fx/shell-polish`, 2026-10-22): the schedule's third row, the duel bracket, Scrap Bot

Live on display :9 (flatpak Wine 9.0, `ra2_drive.py`, `capture_lcg.py`); the prefix's `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` were
hash-checked afterwards: all identical except `Teams/Team0/Bot0.bot` (see "Process note"). New captures (manifest rows and backup
`manifest-additions-2026-10-22-shell-polish.tsv`): `event-bracket-lcg-duel`, `lcg-bracket-duel` (jsonl), `teamhq-scrap-selected`,
`teamhq-scrap-box`, `teamhq-scrap-done`. Numbers are `fidelity.py --renderer app`, MAE.

### The schedule grid (1 px short)

The live grid is 333 px tall (lines at 318, 429, 540, 651; a fresh capture of the Events tab gave the same four rows); ours was 334 (541). The
chain: the calendar group is 0.95, its split 0.9 and the title row 26 px of the Events pane, so a 420 px pane (the live content group,
against ours 422.9) gives 399 -> 359 -> 333, and 422 gives 334. The chain of splits above the content group is still not recovered, so the
fit is on the events pane only: `Hq::fill_events` sets `events group` to 0.99325 of its slot (420.0 of 422.9). Lines now 318/429/540/651, grid 333.1.
`teamhq-events` **3.20 -> 1.39**, event join 3.25 -> **2.62**, join robot 3.12 -> **2.49** (all pixel lines now agree). Test
`hq::tests::the_schedule_grid_has_shared_lines_and_art_inside_them` pins the rows.

### The duel bracket (`duel-round1`)

The state was unknown; it was read live. Enter Event! on Strategic Engineering (16 entrants, 2 bots, 4 rounds) with Frida logging every engine-LCG
draw: the state before the click was **4124306675**, the bracket (`FUN_004695a0`) took **53** draws from it. Ours with `RA2_LCG_STATE=4124306675`:
`event-bracket-lcg-duel` **2.69** (the same as the Blade/tabletop fits 2.67/2.68); the old `event-bracket-duel-round1` fixture (state 7) stays 5.74. The
draw is the `Lcg` of 9.23, so this also confirms the algorithm for a 4-round duel (field `[P ... ]` equal to the capture's 16 icons).

### Scrap Bot (the original's only robot deletion)

There is no way to delete a design in the *Robot Designs* list (no `delete` control in any botlab screen; the only `.gib`s with a delete are
`select_team` (Delete Team), `hq_robots` (`scrap button`) and the wiring editor's input control). The deletion the original has is the Team HQ
inventory's **Scrap Bot** (`hq_robots.gib`, note "click to delete selected bot"), handler `FUN_004ff0c0` (`0x4ff0c0`):

* with the robot entered in the current event (`FUN_004d63b0` = `+0xc0`, compared with `DAT_007184a8`) it only shows the OK box `Robot Arena 2` /
  "This robot is currently entered in an event. In order to scrap it, you must either complete or forfeit the current event.";
* otherwise the box `Team HQ` / "Are you sure you want to scrap <name>?" (`FUN_005e5e40`, style `0xc`, Yes/No). Yes: `FUN_0043b180(column * 3 + row, 0)`
  clears the slot's flag in the window's map (`+0x1ec`), message 200 (the table write, as the bracket's Later) and the grid refills (`FUN_004ff560` ->
  `FUN_004ff570`).
* **Nothing is removed from disk.** Live: after Yes, `Teams.txt` read `Robots: 0 1 2 3 5` (was `0 1 2 3 4 5`) and `Teams/Team0/Bot4.bot` was still
  there with its old date. The only callers of the CRT `remove` (`0x63c2d8`, over `DeleteFileA`) are `0x4e5610`/`0x592fb0` (table/temp-file writes),
  `0x5d5570` and `0x622a90` (`ra2.cfg`): no code removes a `.bot` or a design. So the port does not tombstone anything (docs/39): the roster entry is
  removed and the file stays, as with Delete Team.
* **The grid is slot-indexed.** Cell `i` (down each column first) is slot `i`: the scrapped cell becomes the `nobot_cell.gib` cell (`New Robot`,
  silhouette) and the other robots keep their cells (`teamhq-scrap-done`). Ours filled only as many cells as robots and shifted; now six cells, robot or
  `New Robot` (`RobotRow::slot`), a selection frame that is a whole cell (live: the blue frame of `teamhq-scrap-selected`; `_EvenCells`), picks are slots
  (`flow::inventory_robot_path`, `robot_in_event`).
* The message box's body draws **blank** in the original (as `teamhq-events-one-event`); ours does too (the body is handed to the box and not drawn).
* Port: `Career::scrap_robot(slot)`, `Shell::ask_scrap` / `set_scrap_hook`, `flow::scrap_prompt` / `inventory_click` (shared by the window and the
  `--offscreen --click-names` route), `hq_mode::shell_scrap_hook` (writes through the save dir). Test `playthrough::scrapping_a_robot_unlists_it_and_keeps_the_file`.
  Fidelity: `teamhq-scrap-selected` 3.17 -> **1.96**, `teamhq-scrap-box` 3.02 -> **2.23**, `teamhq-scrap-done` 2.49 -> **1.78** (the first numbers are before the
  selection frame and the slot-indexed refill). **Open (closed in 9.25):** the four buttons' enabled state and Export Bot / Import Bot.

### Tests and the save directory

`ra2fmt::saves::isolate_for_tests()` gives every test that opens the real install an empty save directory of its own process (never created unless something
writes; a binding or `--save-dir` still wins; it ignores `ORA_SAVE_DIR` and the platform default). All `game_dir()` / `ui_root()` helpers of the test code call it,
so results do not depend on `~/.local/share/open-robot-arena` (docs/39). Checked by running the workspace tests with `ORA_SAVE_DIR` and `XDG_DATA_HOME`
pointing at poisoned directories (a garbage `Teams/Teams.txt` and a `deleted.txt` hiding `Teams` and `AI`).

### Process note

`Teams/Team0/Bot0.bot` of the Wine prefix was re-saved by the game (Finish Later saves the entered robot) before I had backed it up (only hashes were taken);
it was rebuilt by running the game's own workshop open/close on the install's Bot0 (same size, 333003 bytes, but a different sha256: `9c711db4...` against
`c8763932...`). Everything else was restored byte for byte (`Teams.txt` by undoing its one line; sha equal).

## 9.25 Team HQ inventory: button states, Export Bot, Import Bot (`fx/fx-scatter`, 2026-10-23)

Live on display :9 (flatpak Wine 9.0, `ra2_drive.py`; the prefix's `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` were copied before the first session and
rsync'ed back after each one, sha256 equal to the pre-session list every time). New captures (manifest rows, backup `manifest-additions-2026-10-21-fx-scatter.tsv`):
`teamhq-buttons-none`, `teamhq-buttons-empty`, `teamhq-export-dialog`, `teamhq-import-dialog`, `teamhq-import-row`, `teamhq-import-done` (PNG), and the files the original wrote:
`teamhq-export-atom-smasher__live__000.bot`, `teamhq-export-forkie__live__000.bot`, `teamhq-import-barbershop__live__000.bot`.

### Which buttons are lit (`FUN_004ff2e0`, the robot grid's select handler; `0x4fd380` builds the window)

The window keeps `workshop` (`+0x170`), `scrap` (`+0x174`), `export` (`+0x178`) and `import` (`+0x17c`) buttons (`FUN_005ec4e0(enabled)`; a disabled button shows its
fourth state image, `export_button_n.bmp` tinted 0.5 0.5 0.5). Message 2 (no selection) disables all four; message 1 (a cell selected: `FUN_00501420` fills the cell's
info, byte 0 = the cell is empty) enables Workshop; Scrap = robot present; Export = robot present; Import = cell empty. Measured on the four labels (brightness max 127 against 217 lit):

| selection | Workshop | Scrap | Export | Import |
|---|---|---|---|---|
| none (fresh window, after a scrap or an import) | dim | dim | dim | **dim** |
| a robot | lit | lit | lit | dim |
| a `New Robot` cell | lit | dim | dim | lit |

(9.24 had Import lit with nothing selected: wrong, the capture `teamhq-scrap-done` has it dim.) A dim button takes no click (live: Import with nothing selected: nothing happens).
Ours: `ra2ui::hq::sync_inventory_buttons` / `InventoryPick`, `input::{set_button_enabled, button_dimmed}`, the shell ignores a click on a dimmed button, `Shell::inventory_selection`
is the pick (an empty cell is a selection now). The selection frame's `Lines` were absolute 213 x 70 px; they are fractions of the bar now, so the frame is the whole cell
(`teamhq-scrap-selected` 1.86 -> 1.65, `teamhq-scrap-box` 2.13 -> 1.93).

### Export Bot (`FUN_00500690`, window `ExportBotWindow` `0x4711e0`, `botlab\exportbot.gib`, OK `FUN_00471620`)

Opens `Export Robot Design`: the `file name` box holds the selected robot's name (`Atom Smasher`, focused), the robot's snapshot beside it, the text "Exported bot designs will be
saved in the 'Robot Designs' folder in your game directory." OK writes **`Robot Designs/<file name>.bot`** (`.bot` appended whatever the name, `C.R.U.S.H..bot`), **silently overwriting** a
file of that name (live: a second export of the same robot raised no box); the window closes; the HQ is unchanged (selection kept). Cancel closes it. The box refuses `\/:?"<>|`
(`0x6f2fec`). **The file is not a copy but the robot loaded and saved again in the current format**: 1.12, the default-texture line `1` after `Class:` added to a 1.07-1.11 file, and for a
1.07/1.08 remote control the key codes `a b c` become `a 0 b c`. Checked **byte for byte** against three live files (source `Bot4.bot` 1.11 -> 333 295 B; `Bot3.bot` 1.08; the 1.12 `Bot0.bot`;
the 1.11 `Bot1.bot`): `ra2fmt::bot::Bot::upgraded` (`write()` of it equals every live file; example `exportcmp`, test `an_upgraded_bot_is_what_the_original_exports`).
Port: `Shell::ask_export`, `hq_mode::shell_export_hook` (writes through the save directory). The `.bot` goes to `Robot Designs/`, never into `Teams/`.

### Import Bot (`FUN_00500860`, `ImportBotWindow` `0x470730`, `botlab\importbot.gib`, OK `FUN_00470b40`)

Needs an **empty cell selected** (the handler returns for a cell with a robot; with nothing selected the button is dim). Opens `Import Robot Design`: `file list` = `Robot Designs\*.bot`
(the live rows draw **no text**: `_Font System` is not available under Wine, as the other `System.giff` screens; eight blank 18 px rows, no scroll bar for a short list), a preview of the selected
design's snapshot (it kept the previous dialog's picture at first; ours shows none until a row is chosen). OK with a row selected: `Robot Designs\<row>` is loaded and saved as the cell's
**`Teams/Team<n>/Bot<slot>.bot`** (the same re-save: 1.07 `XTRA_BarberShop Chop.bot` -> `Bot4.bot`, 334 725 B, equal to `Bot::upgraded().write()`), the slot's flag is set (`FUN_0043b180(slot, 1)`:
`Robots: 0 1 2 3 4 5`, ascending), the table is written (messages 200, `0xcd`, `0xc9`) and the grid refills (`FUN_004fe0c0`); the new robot shows in the cell and nothing is selected. No row: nothing,
the window closes. The list is in **byte order** (the third of the six `XTRA_` designs was `XTRA_BarberShop Chop`, after `XTRA_BOT-CHOY`: `O` before `a`), ours sorts the same. Port: `Shell::ask_import`, `hq_mode::shell_import_hook`, `Career::import_robot`. Test `playthrough::export_and_import_bot_resave_the_design_and_the_buttons_follow_the_selection`.

Fidelity (`--renderer app`, MAE): `teamhq-buttons-none` 1.49, `teamhq-buttons-empty` 1.53, `teamhq-export-dialog` 1.90, `teamhq-import-dialog` 1.96 (4.15 before the scroll bar and the blank preview were fixed),
`teamhq-import-row` 1.87, `teamhq-import-done` 1.82.

## 9.26 Career run end to end, the workshop's new robot, the grid's deselect (`fx/career`, 2026-10-23)

Live on display :9 (flatpak Wine 9.0, `ra2_drive.py`; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before the session, rsync'ed back after it, sha256 list equal to
the one taken before). New captures (manifest rows, backup `manifest-additions-2026-10-21-career.tsv`): `teamhq-newbot-empty` (PNG and the `.bot` the game wrote), `workshop-newbot`.

### Workshop into a selected cell (`FUN_004fee90`, the lab's init `FUN_00439650`)

The Workshop button (after the entered-robot guard, `Loading...`, `botlab\botlabwin.gib`) calls `FUN_00439650(cellEmpty = +0x16c, info = +0x15c, DAT_007184a4, DAT_007184a8)`: the lab keeps
**the team and the selected cell number** (`+0x1f4`, `+0x1f8`; `DAT_007184a8` is the slot, the same global the guard compares with `Robot In Event`). For an empty cell it builds a blank lab:
the four tabs `6..9` (Components, Wiring, Paint Shop, Test Robot) are disabled, the name box reads `Enter Bot Name` (`0x6f101c`, `FUN_004db260`), no robot in the view (`workshop-newbot`:
weight 0.0, Classification `None`, `No Robot Preview`). There is **no search for a free file**: the design belongs to the selected cell. Live, with Atom Smasher scrapped (cell 4) and its
`Bot4.bot` still on disk: Workshop on the `New Robot` cell, nothing touched, `Back to HQ` -> **`Teams/Team0/Bot4.bot` was overwritten** (127 bytes, `1.12`, `Name: Enter Bot Name`, no parts, a garbage
`Class:` value from uninitialised memory), `Robots: 0 1 2 3 4 5` again, the cell reads `No Robot Preview` / `Enter Bot Name` / `None` and nothing is selected (`teamhq-newbot-empty`). So a new robot
is saved **even when nothing was built**, and opening an existing robot and leaving re-saves it (`Bot0.bot` took a new mtime from a view-only visit).

Ours took the first slot whose file did not exist and only when something had been built, so a scrapped slot's file stayed and the robot landed beyond the six cells. Now `ra2ui::teams::add_robot(game,
team, Some(cell))` puts the robot in the selected cell (roster kept ascending), `Builder::set_team_new(team, slot)`, `flow::open_workshop` passes the pick, the name box starts as `Enter Bot Name` and an
untouched new robot is saved under that name (as live). Without a cell (`--build` from the menu) the first number not on the roster is used and only a built/named robot joins. The main-menu Bot Lab has no
team: it writes `Robot Designs/<name>.bot` only (Save As), unchanged. Test `playthrough::a_new_workshop_robot_joins_the_roster`.
**Differs (open):** the original's new robot starts with *no chassis* (a blank lab, the Chassis tab picks one); ours starts from the stock chassis, so an untouched new robot is a 1-part lightweight where
the original's is an empty `None` class design, and the HQ cell shows `Lightweight` not `None`.

### Message 2 of the robot grid (`FUN_004ff2e0`)

The window constructor (`0x4fd380`) registers `FUN_004ff2e0` on `robot grid` (`+0x190`) twice with `FUN_005ea170(grid, id)` (`0x4fd8f9`: message 1, `0x4fd925`: message 2). Message 1 is the grid's own
selection event. **Message 2 is only sent by the grid's programmatic clear**: the refill `FUN_004ff570` starts with `grid->vtable[0xe8](0)` (clear the selection), disables Scrap and Workshop itself
(`+0x174`, `+0x170`) and then fills the six cells. The refill runs when the window is built, after a scrap's Yes and after an import. Live: clicking the selected cell again, a click beside the grid, a right
click and Escape all leave the selection (and the lit buttons) as they were. Ours already behaved this way (the selection is dropped by the same three events, and a fresh page after `Back to HQ`);
no change except the doc.

### The career, scripted (`playthrough::a_whole_career_new_team_to_forfeit_survives_a_restart`, 7 s)

New team `Career Crew` (typed name, OK, Continue) -> empty inventory -> Import Bot into cell 0 (a shipped design) -> Workshop on cell 1 (new robot, saved empty) -> restart -> Event Schedule, join event
A with the imported robot, `Play Next Round!` (the player holds W, 4 s match) -> loss, award, saved (history 1, 80 points) -> restart (a boot lists the first team selected; the player picks theirs) ->
event A is `Completed` -> Workshop on the robot (a part added, written back to `Bot0.bot`) -> join event B, **forced wins** (`Session::set_forced_win`, a harness knob like `--report-damage`) through
its rounds with `Finish Later` and a restart in the middle (the open event shows `In Progress`, every other cell is disabled, the entered robot cannot be edited, `Play` resumes the saved bracket) -> final
win: trophy in `Teams.txt`, history 2, points 240 -> join event C and `Forfeit Event` -> closes as a loss, history 3, the robot is free again. Saves go to the test's scratch install only.
There is **no money** in the original (no cash field, `ra2fmt::team`; `Career::money` is ours and stays 0): "earning" is event points, trophies and the rank.

### Found broken or different, in the order fixed

1. The workshop's new robot went to the first free file instead of the selected cell, and an untouched one was dropped (above). **fixed**
2. `ra2ui::input::select_cell` printed `DBG bar [...]` to stderr on every list click (debug left in 9.25). **fixed**
3. The message boxes of the shell (scrap, delete team, OK boxes) drew the template's `This is a message.` and listed `No` before `Yes`; the live box has an empty body and `Yes` on the left
   (`teamhq-scrap-box`, `teamhq-events-one-event`; 9.24 had claimed ours drew blank). Now `Shell::open_window` blanks the template text and trades the two buttons' names and labels (`yes_left`, `blank_message_body`); `teamhq-scrap-box` 1.93 -> **1.84**. **fixed**
4. A team with no robot: the join window opens and `Enter Event` does nothing (checked live and in the exe, §9.28). **fixed** (ours closed the window)
5. New-robot lab starts from a stock chassis, `Class` shown as `Lightweight` for an empty design (above). **fixed** in 9.27
6. A fresh boot's Select Team: the original selects the LAST team, whichever was played (§9.28). **fixed** (ours selected the first)
6. A fresh boot's Select Team lists the first team selected (not the one last played): matches the earlier live notes for the select screen, not re-checked for a restart. **unverified**


## 9.27 Building a robot from nothing in the Bot Lab (`fx/botlab`, 2026-10-23..28)

Live on display :9 (flatpak Wine 9.0, `ra2_drive.py`; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before the sessions, rsync'ed back after each, sha256 list equal; Team 0 had its
cell 5 scrapped and a new robot built there, the `Bot5.bot` the game wrote copied out). Captures (manifest rows, backup `manifest-additions-2026-10-21-botlab.tsv`): `botlab-flow-*` (13 frames) and the
`botlab-newbot-*` `.bot` files (8). Side-by-sides, ours against live, are in the scratchpad (`sbs_w0.png`, `sbs_b2.png`, `sbs_c.png`). New driver steps in `ra2_drive.py`: `drag X0 Y0 X1 Y1 [BUTTON] [STEPS]`.
`labshot` (ra2-app example) replays the same clicks on our lab; `tools/labgame.sh` makes the scratch install.

### The flow as the original runs it

* **Blank robot** (`FUN_00439650` with the empty-cell flag): Components, Wiring, Paint Shop, Test Robot (`FUN_0043ef60(6..9)`, `FUN_005ec4e0(0)`) are dim and take no click; Overview shows `Enter Bot Name`, weight
  `0.0`, Classification `None` (`FUN_004deff0` prints `None` (`0x6f1f10`) for a class outside 0..2) and `No Robot Preview`. The Chassis tab's `Armor` button is dim too. Tip: `Click the Chassis button to start your robot`.
* **Chassis, step 1** (`chassis_structure1.gib`): a 10x10 blueprint, `Draw Freeform` / `Circle` / `Rectangle`. Points snap to the 100-unit grid and the grid is 450 px of the 466 px `Editing Area` at 1024x768
  (lines at x 39..489, y 148..598); clicking the first point of an outline of 3 or more closes it. A square drawn from (200,200) is `4 / 200 200 / 800 200 / 800 800 / 200 800`.
* **Step 2** (`chassis_structure2.gib`, `Adjust Top Outline`): going to step 2 **extrudes** the baseplate (`274 24 36 12`, height `0.825`, top outline = bottom outline); the Components, Paint Shop and Test Robot tabs light
  (Wiring stays dim) and the height slider moves the whole mesh: thumb at the top 1.32 m, at the bottom 0.436364 (the file's `height` and every vertex `y`). **Finished** returns to the Chassis page (`Structure Design`, `Armor`).
* **Armor**: Plastic 8/80, Aluminum 12/100 (default, selected), Titanium 16/150, Steel 20/175; the new chassis is aluminium: `1 12 0 200 0` (hardness stored doubled, as stock files).
* **Overview after Finished**: the square at aluminium weighs 94.1 (area 7.84 m2 x 12), `Lightweight`; a triangle (200,200) (800,200) (500,800) is 3 + 3 faces and 6 + 3 skin groups.
* **Components**: the first palette part (Robot Control Board 4.0 kg) is selected; weight 98.1 after the board. The board lights **Wiring** (`FUN_004f7b30`). The 3D view draws the chassis as a ghost.
* **Wiring**: a 3x3 controller (1 px rules, (102,102,102)), `SWITCH` / `BUTTON` / `ANALOG` above it. Dragging one onto a cell opens **`Analog Control`** (`ctrl_analog_props.gib`, `Control Name New Analog`, `Positive Axis` /
  `Negative Axis` boxes that take the next key) or `Switch Control` / `Button Control` (`New Switch`, `New Button`, one box); OK adds the control (`W - S` after pressing W then S), Cancel does not. With a control
  selected the 3D view shows it as a floating icon with green wires to the parts it drives; clicking a part opens its **connection window** (`wiring_connection_*.gib`: the part's name and picture, the control's keys,
  one dropdown per input listing `<none>` then the actions: `Spin Clockwise`, `Spin Counter-clockwise` for a motor). The windows are modal.
* **Paint Shop**: `Effect: Color Fill`, `Paint Entire Chassis`, `Undo`, `Export Paint Template`, `Import Custom Paint` (as before). **Test Robot**: the practice arena with `Obstacles` None/Crates/Blocks/Barrels/Cones/Ramps and RESET.
* **Saving**: **Back to HQ** writes the robot into the selected cell. The file: `Class:` is the class its weight gives (`FUN_0046c710` after each mass change, `FUN_004df080`: < 249 light, >= 399 heavy; the constants
  are 249.0 and 399.0, not 250 / 400); chassis `bottom` / `top` outlines, the mesh with a bottom ring (normal -y), a top ring (+y) and one quad per edge, the atlas (`face groups`), 12-px-wide skin groups, a white
  256x256 skin, one crush-corner row per top vertex (`index 3 a b c`), no buckle points. Ear-clipped plate triangles `(v, prev, next)` with the last reversed; the top plate reversed on the top ring; side `(v1 v2 v0)(v2 v3 v0)`.
  The atlas packs the cells from v = 246 downwards, 10 px in from the left, 9 px apart at 0.15 px per outline unit, and again at 0.9 of the scale until it fits (800-unit plates: 0.135; 1.19 m: 0.1215).

### What ours did and now does

1. Opened every new robot on a 1.1 m stock chassis with every tab live. Now `Build::blank` (no outline, no mesh, height 1, aluminium), `Builder::open_blank`, dim tabs (`set_checkbox_enabled`, `refresh_tabs`) and `0.0` / `None`. **fixed**
2. The outline was stored but never extruded, and the mesh a player drew was never made. Now `workshop::extrude` (ear clipping, quads, atlas packing, crush corners) reproduces four of the live `.bot` files' (square, big square, triangle, tall) point lists, indices,
   corners and atlas rows (unit tests on the square, the small and the big square, the triangle, the tall one); step 2 and Finished extrude, the height slider rebuilds. The writer and reader handle the shapeless chassis' extra `0` line
   (`teamhq-newbot-empty`). **fixed**; open: a 100-unit-wide group and a concave outline's ear order (the L's triangle order, the 100 px groups' wider gap) are not byte-equal.
3. The blueprint grid, the outline strokes and its points were not drawn, and the click mapping used the whole 466 px area instead of the 450 px grid. Now `blueprint_grid`, `chassis_overlay`. **fixed**
4. **Blocker**: the player-facing lab could not wire a robot at all (the SWITCH / BUTTON / ANALOG icons sat under the controller's picture, so a click never reached them; binding lived in the debug panel only). Now the icons take
   the click (`hit_control`), a drag or click-then-click opens the properties window, a click on a part opens the connection window (`lab_dialog`, `Builder::mouse_released`, `wire_at_preview`); keys come from the keyboard
   (`dik_for_char`, arrows via `key_named`). **fixed**
5. The palette's last, half-visible cell counted as shown (`ceil`), so a click on it missed at 1024 px. Now whole cells only. **fixed**
6. The class came from the file and never changed; the lab now writes the class its weight gives and the HQ prints `None` for a shapeless robot. The class cut-offs were 250 / 400; now 249 / 399. **fixed**
7. The name box showed the design's stored name; typing on Overview edited it, the first key replacing it, with no click needed. Live (`botlab-name-box-*`): a key goes nowhere until the box is clicked, and then it is
   added to the text that is there (`Enter Bot Name` + `x` reads `Enter Bot Namex`; four BackSpaces leave `Enter Bot Nam`; the saved name of a robot left that way is `Enter Bot Nam`, as the HQ cell showed). Now
   `name_focus` and `type_char` / `backspace` in the player-facing lab. **fixed**
8. The 3D view of the lab was a top-down flat shaded mesh. It is the original's orbit camera (below). **fixed** (the lab's tip line with its `tip.bmp` icon: 9.27c)
9. Wire lines in the 3D view (the floating control icon and its green / blue lines), the control's own edit window (double click) and the connection window's rows for a new control. **fixed** (below)
10. Test Robot fought the shipped AI in the box arena. It is the practice arena with one robot (below). **fixed**

### 9.27b The lab's 3D view, Test Robot and wiring graphics (`fx/lab-view`, 2026-10-28)

Live on display :7 (flatpak Wine 9.0, `ra2_drive.py`, Frida; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before and restored after, sha256 list equal; Team 0's cell 5 was scrapped and a blank
robot saved for the name-box test; the original crashed once (the Wine "serious problem" box) when a workshop was opened right after leaving that blank lab, nothing else). Captures (manifest rows, backup
`manifest-additions-2026-10-21-lab-view.tsv`): `botlab-view-*` (the lab camera on every tab, the wires), `botlab-wiring-*` (edit windows, the new control's connection window), `botlab-test-*` (the practice arena, its
obstacles, state and trajectory dumps), `botlab-name-box-*`, `botlab-paint-default`. The camera states were written through Frida into the tab's camera object; the angles are in each row.
Side-by-sides, ours against live, are in the scratchpad (`cmp4.png` Overview at three camera states, `cmp7.png` wiring, `cmp5.png` components, `cmp8.png` rotated wiring / components, `obst2.png` / `obst3.png` obstacles,
`cmp10.png` Test Robot, `paint-cmp.png`, `strip.png`).

* **The camera is not top-down.** Every tab has its own camera object (`FUN_005a6260`, `+0xf8 = 2`, the orbit mode; five distinct objects were seen, one per tab). It is a 45 degree (`+0x180`) perspective on the robot's
  **origin**, `+0x224` = 5 m behind it, level: the robot's up is the screen's up, the hammer arm stands, the wheels are seen end on; pitch is `+0x1e0`, yaw `+0x1e4`. The matrix `+0xfc..0x124` is the rotation about x
  by pitch then about y by yaw (live at pitch 0.5, yaw 0.7: rows `(0.765, -0.309, 0.565) (0, 0.878, 0.479) (-0.644, -0.367, 0.671)`), the forward axis is column 3, the eye is `-distance * forward` (`+0x1c8..0x1d0`;
  `z = -d cos(pitch)`), up is column 2. Positive pitch lowers the eye (the underside shows). The viewport is 471 x 471 (`+0x190/0x194`), far plane 2000 (`+0x18c`). Controls (`0x48d130`, the lab's per-frame mouse
  read; `FUN_00476b00(this+0x398)` is button 2): the **middle button** drags (yaw += dx * 0.01, pitch -= dy * 0.01, pitch held to +-pi/2; `0x6a69cc` = 0.01, `0x6af2b8` = pi/2), the **wheel** zooms (distance -= wheel * 0.01,
  120 per notch = 1.2 m), within `+0x230..+0x23c` = **2..11** on Overview / Chassis and **4..9** on Components / Wiring / Paint Shop. `0x48d1a0` also zooms on Page Up / Page Down (`0xc9` / `0xd1`, the dinput codes): the
  earlier note that those rotate was wrong. Ours: `LabCam`, `Builder::lab_wheel` / `lab_orbit`, the middle button and wheel in `window.rs`; one camera per tab, a new lab starts at pitch 0, yaw 0, 5 m.
* **Light and ground state** (`ra2_d3dstate.js`): one white point light on the camera (type 1, diffuse and specular 1, range 1000, attenuation 1 / 0 / 0, at the eye), ambient `0xff262626`, background 0.15 grey. The
  Overview / Chassis / Paint Shop draw the robot as is. **Components and Wiring** draw first the **baseplate**: the chassis' bottom polygon opaque, textured with `botlab/baseplate_grid.bmp` (a teal 16 px tile, uv = x, z
  * 8), diffuse 0.5, no culling, normal up (so it is nearly black from below); then the chassis as a **ghost**: the whole mesh unlit-grey, `SRCALPHA / ONE` blend, no z write, no texture, diffuse (0.625, 0.625, 0.625, 0.14),
  drawn before the parts (which cover it); then the parts; then 2 px grey (`0xff828282`) ribbon lines along the chassis' face outlines (the bottom and top rings and each side face's loop), z-tested. Constant on-screen
  line widths: 2 px (outline), 4 px (wires). The legend (`mouse instructions grp`), the SHIFT / CTRL tips and REMOVE (Components only) are children of the 3D group, drawn over the view. Ours: `preview_pixels`
  (`set_lights`, `Material::early` for the baseplate and ghost, which the renderer draws before the opaque pass), `lab_view::{ribbon, outline_edges, baseplate}`; against live frames the Overview is within 1 grey level
  (mean abs difference over the view 1.0), Components 0.8 - 2.2, Wiring 1.9 - 4.1 (the larger ones are 1 px of line placement and the fence of tips).
* **Wiring graphics** (`DrawPrimitive` triangle strips with `XYZ | DIFFUSE`, world space, lighting off): with a control selected the view shows its **icon** (the 32 x 28 `ctrl_icon_<kind>.bmp`, 0.262 x 0.23 m, centred
  1.6 m above the robot's **control board** part, facing the camera at the start), a blue stem (`0xff0000ff`) from the floor to 1.0 m, and for each part the control is bound to a green (`0xff00ff00`) wire: down from the
  icon's centre to y = 1.0, across at y = 1.0 to the part's `(x, z)`, down to the floor (bends are filled by the strip's joint pairs). Forward (4 wheel motors), LeftRight (2) and Fire (the burst motor) were dumped; a
  control with no part shows icon and stem only; the anchor is the same for every control. Ours: `lab_view::wire_scene`, `Builder::wire_anchor`.
* **Double click on a control** opens its own window: `Analog Control` / `Switch Control` / `Button Control` (the properties windows) with the control's name and keys in it; typing adds to the name (`Forwardz`); OK
  renames and re-keys it and keeps its wires. Ours: `DialogKind::Props { editing }`, `open_edit_dialog`. A **new control's connection window** starts with both rows at `<none>` (`botlab-wiring-new-connection`),
  as ours did; a bound one shows the stored rows. Delete Control opens a modal `Wiring` window with `Yes` / `No` whose text did not draw live (`botlab-wiring-delete-confirm`): 9.27c.
* **Test Robot** = `Scripts/Practice.py`: the `Practice Arena` (`Arenas/practice arena/practice_arena.gmf`, gravity -12.753, friction 0.5) with **one** robot, a DEATHMATCH to the scripts (`plus.getGameType`), no countdown,
  no end, the robot let go on the model's start point `(0.306, 2.163, -0.948)` (live: it drops, settles at `(0.26, 0.45, -1.0)` and keeps rocking by a centimetre). The camera is the scene camera's mode 0 (our `Watch`): eye
  fixed at **(-4.2, 2.5, 9.5)** looking at the robot, its FOV following the distance (`+0x290..0x2a0`: distance 2..20, FOV 0.8976 .. 0.3491; 0.6047 at 11.6 m), the lights the model's two `Omni` points plus the white
  camera headlight. `Obstacles` (`setObstacle`, `AddXtra` meshes `crates`, `cinderblocks`, `barrels`, `cones`, `ramps`; the first four are dynamic Havok bodies of the files' masses, the ramps mass 0 = fixed) replaces the
  props; RESET puts the robot and the obstacles back; leaving the tab drops the arena (the clock stops) and entering it builds a new one with `None` picked. The robot is driven by its own wiring (W held 1.6 s: z -1.1 to
  16.4, the far wall; ours reaches the same wall in about the same time, `botlab-test-trajectory-drive`). The status box shows the robot's name, the three meters and `POINTS: N/A`. Ours: `Match` with `MatchConfig::practice`
  (one robot, no countdown or hold, never ends), `lab_arena::LabArena` (the practice `Duel`, the camera, the headlight, frames read back into the `test arena placeholder`'s place between its black background and its
  children), `Builder::{advance, set_drive, arena_quad, bind_arena_hud}`, `hud::bind_status`. Mass-0 xtras are static bodies (`spawn_prop`) and the movable obstacles rest as one deactivation system until hit
  (`new_with_humans`), which keeps the block pyramid standing as live. Open: how the live pile falls when hit was not checked; our pile settles a few centimetres on its first frames; the fence's alpha is a
  shade lighter than live; the robot's idle rocking is not reproduced.
* **Paint Shop** against `botlab-paint-default`: the panel was missing the `Selected Color` label under the swatch, the `Tint` label, the swatch colour and the slider thumb, and drew the SHIFT / CTRL tips. The labels were clipped away:
  `ClipMode 2` was read as "clip to the parent's frame", but live the five bot-lab `selected label`s hang below their 64 px group and are drawn (now: clipped by what the parent was clipped by); the `tint label` is right
  aligned in a box narrower than its authored position (`text_box_right`); `wires grp` / `wires grp 2` (the cables left and right of the tab strip, outside their group's 630 px frame) are drawn out to the window's edge
  (they were missing from every lab frame). The Color Fill state is the live one: swatch = HSL(x * 360, 1 - y, thumb) of the picker target and the thumb (target 0.184 / 0.253, thumb 0.4 give `(161, 176, 25)` as live);
  the first captures (`botlab-paint`) showed `(185, 150, 18)` with the same target, so the start colour is not a function of the target (not traced; ours starts at the first captures' value). Clicks on the picker and
  slider move them, clicks on the chassis and `Paint Entire Chassis` fill with the swatch, `Undo` undoes (`Texture::solid`). The weight in the Overview of C.R.U.S.H. read **395.2** live against 387.209 from
  `plus.getWeight` and 387.2 in ours: explained and fixed in 9.27c (the wheels' phantom 2 kg), as are the lab's 1 px low content, the tip line, the Delete Control box and the practice arena's sounds.

### 9.27c The lab's tip line, Delete Control, the weight, the 1 px, the practice sounds (`fx/visual-polish`, 2026-10-30)

Against `botlab-overview`, `botlab-flow-wiring-stock`, `botlab-wiring-delete-confirm` of the original (no new live session: every number below is read from the existing captures and the exe).

* **The weight (395.2 against 387.2).** `FUN_0042f7d0` (weight / weightclass boxes) -> `FUN_004df0b0` -> `FUN_004e23b0` / `FUN_004e24b0` (two nested `accumulate` loops over the robot's component list) ->
  `FUN_004e24f0` -> `0x45a4f0`, which is `fld [ecx + 0x128]`: the sum of every body's stored mass (`0x524ab0`'s constructor argument, the `.gmf` rigid body's own mass). It is also what `getWeight` and the
  800 kg check call (docs/05). The sum includes the **no-collision phantom bodies**: `wheel_slipperbottom/slipperbottom.gmf` has `white_wheel` (16 kg) and `whitewheel_traction` (2 kg, `nocoll true`), so a
  SlipperBottom wheel is 18 and C.R.U.S.H.'s four wheels are the 8.0 kg we lacked (387.209 + 8 = 395.209, the label's 395.2). `ComponentInfo::mass` (`ra2ui::workshop::library`) now sums every body (miniwheel 9 + 8 = 17);
  the simulator's own mass is unchanged (it already adds `phantom_mass` to a part's body, `robot.rs`). Not changed: `plus.getWeight` in a match still reports `rig.mass` (the collidable bodies, 387.2), the original's
  is this same sum (395.2); it is used by `Hazards.Electricity` only and feeds `robot.mass`'s wheel drag, so it stays with the physics owner. Tests `a_component_loads_with_its_mounts_and_mass` (miniwheel 17, SlipperBottom 18),
  the `--build` self-check compares the rig's mass plus its phantoms. Example `ra2ui --example botmass` prints a design's parts.
* **The 1 px low content.** The shell's two vertical splits (`botlab splt`, `lowersplit`) take a **whole number of pixels** for the sized pane (`PaneFloorV`, `ra2ui::split_children`; the tab strip is
  `floor(0.14 x 768) = 107`, not 107.52). Name box / weight row / 3D frame are then exact: the Overview's name and weight rows differ 0.00 from the capture (they were 7.05 and 12.07 at the 1 px offset), the 3D frame
  top border is at y 147. MAE against the captures (1024x768, master -> final, the tip's random pick included: another pick than the capture's costs about 0.05): overview 1.118 -> 0.51, chassis 0.911 -> 0.82,
  components 2.058 -> 1.21, wiring (`botlab-flow-wiring-stock`) 3.93 -> 3.81, paint 1.229 -> 1.02 (the first three on the weight and the whole-pixel panes only: 0.66 / 0.77 / 1.16 / 3.79 / 0.97).
  The snapshot box under it was 1 px low: fixed in docs/34 11.8.
* **The tip line** (`FUN_0043e900`, text boxes at `0x6f1500..0x6f1844`): the lab's `Info Display` (80 % wide, 18 px, 2 px under the pane) shows `tip.bmp` (19 x 17) and one line of `fg_mcn14` text in
  `.5 .6 1` (markup `<image filename "UI\botlab\tip.bmp"><text font fg_mcn14 color .5 .6 1 1 align 2>` at `0x6f1500`; `0x43e6e0` sets it, `0x43e7c0` is the same in `alert.tga` / `1 .8 .3`), icon and text
  **centred as a pair** (icon at x 349 live, text from 372). The text follows the state (`+0x1d8` is the lab's screen: 1 Overview, 2 Chassis, 3 / 4 the two structure screens, 5 Armor, 6 Components, 7 Wiring, 8
  Appearance, 9 Test Robot): no chassis yet -> `Click the Structure Design button to build a chassis` (screen 2), `Click on the grid to add points to your chassis` (3), `Click the Chassis button to start your
  robot` (others); screen 4 -> `Click and drag points to adjust the sides of your chassis`; only the chassis -> `Click the Components button to add parts to your robot`; no control board on Components ->
  `Click the Control Board (under Power) to add it first`; no controls -> `Click the Wiring button to set up keyboard or joystick controls for your robot`; else a random one of four (`0x419190(4)`, again until it is
  not about the screen shown): Paint Shop / Components / Chassis (not on 2 or 5) / Test Robot. The live Overview shows the Paint Shop one, the live Wiring (`botlab-wiring-delete-confirm`) the Test Robot one.
  Ours: `Builder::{tip_text, refresh_tip}` (the roll is redone on every screen change); the widgets hang on the `Glyph` of `info_display.gib` whose `DisplayBox` body is the `Info Display` box (the frame is the pane's,
  so the positions are the box's x / y minus the pane's). Pixel check: the tip row of the Overview is now **0.02** MAE (icon at 349, text 372..674, rows 691..707). The alert tips and the random pick: done in 9.27d.
* **Delete Control** (`0x6f19fc` *Delete this input control?*): the shipped `common/messagebox.gib` titled **`Wiring`**, `Yes` left of `No`, the body **does not draw** (live `botlab-wiring-delete-confirm`: an empty
  box). `ra2_shell::yes_no_box` dresses the template (shared with the HQ's boxes), `DialogKind::ConfirmDelete`; No keeps the control, Yes removes it. Test `delete_control_asks_before_it_deletes` (`ORA_TEST_SCRATCH=<dir>` writes the frame): the box, title and buttons sit exactly where `botlab-wiring-delete-confirm`'s do (box MAE 7.05, all of it the nail texture's phase).
* **Practice-arena sounds.** `LabArena` observes its match with the same `MatchSounds` the matches use (motors and their pitch, weapon fire, impacts, the obstacles' `AddXtraSound` materials, parts coming off)
  and the window plays them from the practice camera (`Builder::take_arena_cues`, `AudioSystem::play_cues`); the ceremony cues (countdown beeps, announcer, crowd, immobility tick, chat) are dropped because
  `Practice.py` starts none of them. **Not verified live**: no practice-arena audio capture exists; the cue rules are the matches' (docs/35).

Tests: `playthrough::the_test_robot_tab_is_a_practice_arena_with_obstacles_and_reset` (arena up on the tab, start point, the frame, W reaches the far wall, obstacle and RESET, leaving drops it),
`the_lab_camera_matches_the_live_matrices`, `the_paint_swatch_is_the_pickers_hue_and_saturation_at_the_sliders_lightness`, `lab_view` (outline edges, constant-pixel ribbons), the playthrough's name step (a key
before the click does nothing, the first key adds). Addresses verified: `0x5a6260` (camera input / orbit), `0x48d130` (the lab's mouse read; button index `+0x398` = 2), `0x48d1a0` (Page Up / Down), `0x6a69cc`
(0.01), `0x6af2b8` (pi/2), `0x6a84e0` / `0x6a84e4` (-1000 / 1000), `0x42662c` / `FUN_00426590` (Color Fill's constructor: `lum slider`, `color picker img`, `picker target`, `selected color img`, `paint entire`),
`0x71f58c` (the D3D device the dumps read).

Test: `playthrough::a_robot_built_from_nothing_is_wired_saved_and_fights` (blank lab on a scrapped cell, dim tab ignored, square drawn and closed, step 2, Finished: 94.1 kg, board, two motors with wheels, analog control W-S dragged on,
both motors wired through the connection window, named `Bolt`, Back to HQ, `Bot5.bot` read back, then a match in which the new robot drives more than 0.5 m). Unit tests: `workshop::extrude` (6), `stats::weight_classes_use_the_games_thresholds`.
Addresses: `FUN_00439650` (blank lab), `FUN_004deff0` / `FUN_004df080` / `FUN_0046c710` (class text, class from weight, the lab's class field), `FUN_0042f7d0` (weight / weightclass boxes), `FUN_004f7b30` (control board test enabling Wiring),
`FUN_0042c850` (the drag judge, verified live on the new chassis), `FUN_0048d130` / `FUN_0048d1c3` (the 3D view's input: see 9.27b, the middle button rotates and the wheel zooms; 0xc9 / 0xd1 are the Page Up / Down keys), `0x6aa9e8` / `0x6aa9ec` / `0x6aa9a4` (399, 249, 800).

### 9.27d The alert tips, the lab's tip line as the exe drives it, the practice sounds (`fx/polish2`, 2026-11-01)

Live on display :9 (flatpak Wine 9.0; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before and restored after, sha256 list equal; the first session's frida-server wedged after a hook
was left on `0x5a6260` and the session was restarted: the camera hook now detaches before the client does). Captures (manifest rows, backup `manifest-additions-2026-10-21-polish2.tsv`):
`botlab-alert-intersect`, `botlab-alert-expired`, `botlab-tip-drag`, `botlab-tip-attach-select`, `botlab-tip-cancel-blank`, `botlab-components-top-camera`, `audio-practice-test` (wav + `ra2_sounds.js` log).

* **The four `0x43e7c0` callers.** `FUN_0043e7c0(text)` is the alert: `<image filename "UI\botlab\alert.tga" alpha 1><text font fg_mcn14 color 1 .8 .3 1 align 2>` + text into the lab's `Info Display`
  (`0x71488c`), `0x714890` := 2.0 s; the lab's per-frame update (`0x43e660..0x43e6c4`) counts it down by the frame's dt and, at <= 0, writes the string saved at `0x71487c` (the last tip) back. An **empty** text takes the
  early path (`0x43e832`: `FUN_0042f980` is `length == 0`): it only restores the tip. The callers:
  `0x42cf46` / `0x42d11a` in `FUN_0042c850` (the drag judge, **every frame of a drag**, only the baseplate branch: a part over another's `SmartZone` raises none): *Components should not intersect the chassis or other
  components.* (`0x6f05f8`, code 3) when the physics test hits and the part is not too big, *Robots cannot exceed a maximum size.* (`0x6f063c`, codes 2 and 4: height over 3.0 or origin below -1.5, or the retest),
  *Baseplate components must fit inside the chassis.* (`0x6f05c0`, code 5); `0x44d5fb` in `FUN_0044d1b0` (the **chassis side-adjust**, screen 4: after a point is moved, for the two adjacent sides the dot product
  `FUN_0044d680` of their face normals is < 0.75) *Can't move that point because it will bend one side too much* (`0x6f2118`), then the point goes back; `0x478d1d` is the **clear** (empty text) after a chassis
  click was accepted. **Not** `0x44d5fb` = the mass limit as 9.27c guessed: *Total mass of robot must not exceed N kg.* (`0x6f0590`, `0x42c425`, `0x43cb6f`) is a `Robot Arena 2` message box, as ours has it.
* **Live** (`botlab-alert-intersect`): the Small Battery Pack dragged over the motors, camera at pitch -1.2 (the baseplate seen from above; the 3D camera is set through the camera object after a Page Up / Page Down):
  the tip line becomes the purple `!` icon (alert.tga 17 x 17) and the orange text, centred as a pair; it stays for the 5 s the mouse sat still (the drag judge refreshes it each frame); moved off the chassis it was
  gone within the next sample (`botlab-alert-expired`: the drag tip is back, `0x71487c`). Ours: `Builder::{show_alert, alert}`, `tip_set`, the alert is raised in `drag_to` and kept at 2 s by `advance` while the
  part stays refused over the baseplate (`parent == 0`), cleared by the countdown; `refresh_tip` swaps `tip.bmp` / `.5 .6 1` for `alert.tga` / `1 .8 .3`. **Tip-line row against the captures: 0.009 MAE** (alert),
  0.013 / 0.015 (select, drag tips), 0.000 (cancel), whole width 685..715.
* **The other tip-line texts, found in the same frames.** The attachment tips (*Click a green box ... Attach*, *Click the Attach button*) and the drag tip (*Drag the component where you want it to go ...*) are tip-line
  texts (blue `tip` icon), not the synthetic panel's status: set on the palette click and on Attach, the drag tip is also what comes back after an alert; a click on nothing (drag cancelled) leaves the line **empty**
  (no icon: `botlab-tip-cancel-blank`); a screen change recomputes the default (`FUN_0043e900`). Ours: `Builder::tip_set` (the status string still feeds the synthetic panel, which stays for the editing the port
  has not moved to the shell).
* **The chassis side-adjust's bend alert** (`0x44d5fb`) was not reproduced when this was written; it is in 9.27e (the port had only the click-click outline editor, `chassis::move_outline_point`).
* **Practice-arena sounds, against `audio-practice-test`** (Test Robot, C.R.U.S.H.; W 2.5 s, Space, W 3 s into the far wall). Live: four `motorloop` voices (one per wheel) are created when W goes down, their pitch
  (`F`, Hz of the 22 050 Hz sample) ramps 11 536 -> 16 223 -> 21 128 -> 24 409 -> 28 591 over 1.1 s (0.52 -> 1.30 of the rate) and they stop (`S`) when W is released; no crowd, announcer or ceremony sounds; the
  workshop music keeps playing; at the wall `sounds\steel_collision.wav` (-7.95 dB, -13.41 dB) and `sounds\hammer_hit.wav` (-13.45, -14.51 dB) play. Ours (`labshot ... keys down 17 tick ... cues`): four
  `motorloop` voices on W, off on release, pitch 0.685 / 0.955 / 1.33 at 0.2 / 0.5 / 1.0 s (live, interpolated: 0.69 / 0.9 / 1.25-1.3: **equal to the sampling error**), the weapon key gives `burst_trigger.wav`
  (live: a sound of id 5 played with no call row, not named). **Differs:** ours plays **no impact** at the wall; the practice arena's `Impact` cues come from the match's priced contacts (`duel.impacts`), and a
  robot against the wall is not priced there. Live plays them for chassis and hammer against the arena. Owner: the combat / physics side (`fx/bell-landing`); the cue and its volume rule (`hundredths of dB` of
  the contact magnitude, docs/35) are the match's. New `labshot` step `cues`.
* **Tip RNG and the 1 px items** are in docs/34 11.8 (snapshot box, DisplayBox rows, the chat line).

### 9.27e The chassis side-adjust, screen 4: dragging the top points (`fx/lab-adjust`, 2026-11-01)

Live on display :9 (flatpak Wine 9.0; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before and restored after, sha256 lists equal, three sessions). Team 0's cell 5 scrapped, a new robot,
Chassis > Structure Design, a baseplate clicked in, step 2, then drags on the top points (`tools/oracle/ra2_drive.py dragh`, new: see Harness). Captures (manifest rows, backups
`manifest-additions-2026-10-21-lab-adjust.tsv` and `-lab-adjust2.tsv`): `botlab-adjust-hover`, `-drag-ok`, `-alert-bend`, `-alert-close`, `-alert-cross`, `-alert-sharp`, `-taper`, `-nosnap`, `-step2-reextruded`,
`-overview` (PNG); `botlab-adjust-triangle-ok`, `-triangle-bent`, `-square-taper`, `-base-vertical`, `-base-ccw` (the `.bot` Back to HQ wrote); `botlab-adjust-trace` (`tools/oracle/frida/ra2_chassis_adjust.js`, every call of
`FUN_0044d1b0` with the two normals it dots and the alert text).

* **How the original gets here.** Screen 4 is `chassis_structure2.gib` (*Adjust Top Outline*, the `Editing Area`, `height slider`, `3d view placeholder`) loaded by the step layer's `step 2 button`; screen 3 is
  `chassis_structure1.gib` (`FUN_00437770` builds both). Going to step 2 **extrudes afresh** every time: after step 1 and step 2 again the top outline is the baseplate again, the height the default 0.825
  (thumb at 302 px) and every drag is gone (`botlab-adjust-step2-reextruded`). Finished keeps what was dragged. The tip line is *Click and drag points to adjust the sides of your chassis* (`+0x1d8` = 4).
  A baseplate clicked **counter-clockwise** is stored reversed (`botlab-adjust-base-ccw`: (200,200) (500,800) (800,200) is written `800 200 / 500 800 / 200 200`).
* **What can be dragged.** The **top outline's points** only (the baseplate's stay; there is no per-vertex height: the height is the slider's), in the two-dimensional blueprint, and the 3D view follows. The widget
  (class vtable `0x6a7c6c`, ctor `FUN_004781f0`, `Point_Selection.tga` image per point) takes a press (`0x478f00`) on the 16 px selection image of a point (a point at (129,238) is taken by cursors x 121..136,
  y 232..245; the first point in list order wins) and holds it until the release (`0x4791e0`); its move handler (`0x478c20`) then moves the point to the cursor each mouse move: `FUN_00479260` converts the
  pixel to blueprint units (`round((px - 39) / 450 * 1000)`, snapped to the 100-unit grid when `Snap to grid` is on: live, snap off follows the pixel exactly), `FUN_004a10d0` judges the new polygon, `FUN_00479ca0`
  sets it and raises the callback `FUN_0043d820` -> **`FUN_0044d1b0`(index, &point, 1)**. The ring of the point under the cursor (and of the one held) is white, the others grey (`191`); the ring stays
  where the cursor left the area (the widget gets no move outside its frame).
* **The 3D mapping.** `FUN_0044a570`: the vertex of an outline point is `((u - 500) * 0.0022, h, -(v - 500) * 0.0022)` with `h` = `lab+0x2ec` (the height). `FUN_0044d1b0` moves every mesh vertex of that top
  corner (the top ring vertex and the two side-quad copies: a map point -> vertices at `lab+0x2f0`), then recomputes **every face normal** (`FUN_004260c0`, normalised: `(p1 - p0) x (p2 - p0)` of the
  triangle as indexed) and writes it to **each of its three vertices in index order**: the last triangle wins. A side is the quad `b_i b_i+1 t_i+1 t_i` as `(v1 v2 v0)(v2 v3 v0)`, so `v1` keeps the first
  triangle's normal and `v0 v2 v3` the second's: equal on a flat side, different once a corner is dragged (`botlab-adjust-triangle-ok` has `-0.226108 0.361772 0.904431` three times and `0 0 1` once on the
  slanted side). `FUN_0044d710` then sums the triangle areas (`lab+0x300`) for the weight and the hitpoints (live: the triangle with its first point at (400,300) weighs **55.8** in the Overview; ours 55.8).
* **The bend test** (`0x44d5fb`). For the two sides next to the point (side ids `index + 2` and `(index == 0 ? n : index) + 1` in the 12-byte records at `lab+0x2c8`, two records per side: `u16` first
  vertex of each triangle, `int` side id) it takes the two vertex normals (`FUN_005a9d50`) and dots them (`FUN_0044d680`); **dot < 0.75** (`0x6a69d8`) sets the verdict false. Then the orange alert
  *Can't move that point because it will bend one side too much* (`0x6f2118`, always shown, `FUN_0043e7c0`) and the lab calls itself again with the **old** point and flag 0 (no alert): the point goes back, in the
  lab and in the widget, and the drag goes on (every further mouse move over a bad place repeats it). Live trace of the triangle (200,200) (800,200) (500,800): the second top point accepts (800,300) (dots 0.988 / 0.953), (800,400)
  (0.937 / 0.837), and refuses (800,500): dots **0.825** (the side towards the third point) and **0.698** (the first side). Our `adjust::face_normal` reproduces both (`the_face_normals_dot_as_the_original_traced`).
* **The editing area's three alerts** (`FUN_004a10d0`, before the lab sees the point; a refusal moves nothing and the lab is not called): `FUN_004a1290` *Points should not be too close to other points or lines*
  (`0x6f533c`: another point under 100 units away, or the point within 99 units across a non-adjacent edge), `FUN_004a1660` *Corners of outline should not be too sharp* (`0x6f5310`: two consecutive edge
  directions with dot < -0.95), `FUN_004a1a50` *Lines should not cross each other* (`0x6f52ec`: two edges meeting away from their ends, or a point within 99 units across one of the two edges of the moved
  point). They show through `FUN_004a0df0`, which only calls `FUN_0043e7c0` when **`DAT_006f52e0` is set**: every accepted move sets it and `FUN_004a0e00` (a point added in step 1) clears it, so on a fresh screen 4
  the **first refused move is silent** (live: the first run of the close / cross / sharp shots showed no alert until one valid drag had gone before). Ours: `Builder::adjust_armed` (the original's flag lives
  across labs; ours per lab).
* **What is saved.** The `.bot` after Finished and Back to HQ carries the dragged top outline, the mesh with the moved corners, the **per-vertex last-triangle normals** and a skin layout laid out again from the
  final shape: the top plate is the polygon unfolded flat from its first edge (`a = unit(p1 - p0)`, `w = unit(n x a)`, `u = max - along`, `v = top - across`, so a plate whose first edge is not along x is
  rotated: `botlab-adjust-base-vertical`), a side is its quad projected on the plane of its **second triangle's normal** (a trapezoid after a taper), each cell the bounding box of its polygon, and the cells
  are packed by the **skyline**: each at the lowest spot among the left margin and the right end (+9, rounded up to a pixel) of every cell placed, the leftmost on a tie (a triangle's top plate beside the
  bottom one, its first side under the top plate). The face-group rows, the crush corners (`index 3 a b c`), the triangles and the outlines are the extrusion's. Our `extrude` reproduces
  `triangle-ok`, `square-taper`, `base-vertical`, `base-ccw` to the file's six digits (points, normals, uvs, corners, group rectangles) and `triangle-bent` but for the **one** group below.
* **Ours.** `ra2ui::workshop::adjust` (the three rules, the bend test, `TopMove`), `Build::{begin_top_adjust, adjust_top_point}` (the move is judged on the extrusion of the new outline, so mesh, normals,
  weight and skin are the saved ones), `extrude` (per-vertex normals, unfolded plates and sides, skyline pack, counter-clockwise baseplates reversed), `Builder::{adjust_press, adjust_move, tip_line}` (press on a point,
  moves while held, release, hover ring, the alerts through `show_alert`, screen 4's tip, the pane's 25/255 backdrop, step button selection kept after the screen swap, blueprint 1 px lower as live), the
  overlay (baseplate outline 2 px (153,153,191), top outline 1 px white at the pixel the point names, rings 16 px grey / white). `labshot` gets `drag X0 Y0 X1 Y1 STEPS [HOLD]` and `alert`.
* **Against the captures** (ours through `labshot`, the same clicks; MAE over the 1024x768 frame / the blueprint panel / the 3D pane / the tip row): `drag-ok` 1.63 / 0.86 / 1.75 / 0.008, `hover` 1.61 / 0.90 / 1.50 / 0.008,
  `alert-bend` 1.67 / 0.92 / 1.86 / **0.010** (the alert row), `alert-close` 2.03 / 1.72 / 1.73 / 3.56, `alert-cross` 1.91 / 1.72 / 1.73 / **0.006**, `alert-sharp` 2.00 / 1.72 / 1.73 / 2.90, `taper` 2.04 / 2.19 / 1.50 / 0.008,
  `nosnap` 1.71 / 1.04 / 1.76 / 0.008, `step2-reextruded` 1.61 / 0.86 / 1.50 / 0.008; the Overview's snapshot box 0.000. Before: the blueprint panel was 2.9 (grid one pixel high, white 2 px outline). What is left in the
  frame is not the adjust screen's: the title and the logo sit 2 px higher, the height slider's track is a 26 px black bar where the original draws 4 px, the `Info Display` of the alert is 1 px off for two
  of the texts (close, sharp: the pair's centring). The saved file of a triangle with its first top point at (400,300) equals `botlab-adjust-triangle-ok` line for line but the number format (the exe writes
  six digits) and the header's fourth line (`1` for ours, `0` live: not this feature's).
* **Harness.** `ra2_drive.py dragh X0 Y0 X1 Y1 [BUTTON] [STEPS] [HOLD]`: the editing area's move handler reads the **raw button state** (`0x42b810(0)` on the mouse object at `0x726878`, not the UI state at
  `0x71643c`), so a drag has to hold the raw button as well (`ra2_input.js`: `drag(.., keep, hold)` queues the dinput poll for the whole drag). `drag` (UI state only) moves nothing here.
* **Tests.** `adjust::tests` (6: the rules and the traced dots), `build::tests` (4: the corner moves in the mesh and the weight falls, the bend refusal at the traced place, the extrusion starts over, a
  counter-clockwise baseplate is stored reversed), `extrude::tests::a_dragged_top_point_changes_the_normals_and_the_layout_as_live` (5 live files), `playthrough::chassis_step_two_drags_top_points_and_refuses_what_bends`
  (press / move / release through the `Builder`, the alerts, the weight 55.8, the tip, step 1 / step 2 again, Finished).
* **Open** (9.27f settles (2) and (5), narrows (1)). (1) One group of `triangle-bent` sits 6.22 px lower in the original's atlas than its bounding box (the side whose base is slanted and whose quad is not flat; same shape, same u): the rule for the
  cell height of a non-flat quad not on the x axis is not recovered, the test skips that one `v`. (2) The ear order of the plates (a square is clipped from corner 2, a 200 x 300 rectangle from corner 0)
  is still not recovered; the sets of triangles are equal. (3) The alert latch is global in the exe, per lab here. (4) The red ring the original's cursor shows over a refused place is the OS-level cursor, not drawn.
  (5) Step 1 does not show the three alerts of the editing area (`FUN_004a10d0` judges clicks there too).

### 9.27f The .bot writer against the original's, the plates' order, chassis step 1 (`fx/lab-files`, 2026-11-01)

Live on display :9 (flatpak Wine 9.0; `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` of the prefix copied before and restored after, sha256 lists equal). New captures (manifest rows, backup
`manifest-additions-2026-10-21-lab-files.tsv`): `teamhq-import-resave-orabuilt` and `teamhq-import-resave-oratriangle` (the `.bot` the **original** wrote when it imported two files our port's lab had written),
`teamhq-import-ours-built`, `botlab-import-ours-built-overview`, `botlab-import-ours-triangle-overview`, and for chassis step 1 `botlab-step1-hover-valid`, `-refused-close`, `-refused-cross`, `-refused-sharp`,
`-refused-latched`, `-drag-ok`, `-drag-alert` (PNG). Harness: `ra2sim --example livebots <game> <dir>` writes the two test designs (a triangle with a dragged top point; the stock chassis with an angle motor, a wheel and the
default board), `livebots2 <game> in out` loads and saves one in the lab model; the files were copied into the prefix's `Robot Designs/` and loaded by Team HQ > Import Bot into a scrapped cell (9.25).

* **Loads in the original, and what it writes back.** Both files imported and showed in the HQ (`teamhq-import-ours-built`: Lightweight, `No Robot Preview`), and the lab's Overview of the imported triangle is the
  captured one (weight 55.8, `botlab-import-ours-triangle-overview` against `botlab-adjust-overview`), the built bot 89.1 with its motor and wheel. The re-saved `Bot4.bot` is the original's writer's output for
  *our* file, so it shows what the writer does: for `OraBuilt.bot` (stock chassis with no skin, header line 4 `0`) it differs from ours in exactly two places: line 4 is `1`, and the shaped chassis gets the **white
  256 x 256 32-bit skin** (`false` -> `true` + TGA, 5 598 -> 267 759 bytes); for `OraTriangle.bot` (white skin, line 4 `0`) in line 4 only. A visit to the lab and `Back to HQ` kept the file byte for byte.
  Ours now does the same on load (`Build::load`: skin for a shaped chassis without one, line 4 `1`) and `Build::to_bot` writes the current format (the original always loads and saves 1.12): **our re-save of
  `OraBuilt.bot` is byte-identical to the original's** (577 lines, 267 759 bytes), and `load` + `save` of both re-saved files is the identity (test `a_design_the_original_saved_is_saved_again_unchanged`).
* **Header line 4.** The text writer is `FUN_004dd8c0` (`Class: ` `0x6fd7a8`, xref `0x4dd977`): after `Name:` and `Class:` it prints `FUN_004df560() & 0xff`, which is `find(pair(0x4df5c0, DAT_00716108)) == end` in a `std::map`
  at the robot object's `+0x18` (the same map `FUN_004dd780` looks components up in by `DAT_00716108`; `FUN_004df1a0`, called by `FUN_0043b890` with the snapshot, erases that key). What the key means is
  not recovered, so the rule is the observed one: **`0` in every file the lab itself made** (the 13 `botlab-newbot-*` / `botlab-adjust-*` captures and `teamhq-newbot-empty`), **`1` after any load** (the five shipped
  or imported designs, both re-saved test files, kept through a lab visit). Ours: `Build::new` / `blank` write `0`, a chassis edit (`commit_chassis`) sets `0`, `Build::load` sets `1`.
* **Number format.** The exe has no `%g` string (search: none) and `FUN_004dd8c0` sets no precision before it streams the floats (`FUN_004192a0`): an `ostream` at its default precision 6, i.e. `%g`. The port's `num`
  fell back to the shortest form when six digits did not round-trip the `f32`; now it is always `%g` with six significant digits (three-digit exponents), so a lab-made value like 0.6815805 is written `0.681581`
  as the original does. All 57 shipped bots still round-trip. **A negative zero** from the normal's division printed `-0` where the original prints `0` (no live file has a `-0`): `face_normal` adds `+ 0.0`.
* **A blank chassis has no buckling section.** The original's untouched new robot (`teamhq-newbot-empty`) goes from the armour row to the buckle-model flag; the reader and the writer both expected a buckling
  count there, so **the original's own file did not parse and ours wrote a line the original would have misread**. Fixed both ways; test `the_original_blank_robot_round_trips`.
* **Byte for byte.** `a_triangle_robot_is_saved_byte_for_byte_as_live`: the lab's file for the triangle with its first top point at (400,300), the counter-clockwise triangle and the vertical one equal
  `botlab-adjust-triangle-ok`, `-base-ccw`, `-base-vertical` line for line (159 lines, 263 712 bytes) but one number of the first (`0.68158` live, `0.681581` ours: a u in the unfolded top plate that is one `f32`
  step lower there; the original is x87 code, doubles in the unfolding changed nothing, so the test lets a number differ by 1.5e-6).
* **The plates' ear order is random, not recovered, and cannot be.** `FUN_0044ae70` (the extrusion) reads the triangles of the plate from a polygon class (vtable `0x6b176c`, ctor `FUN_0061d3c0`) whose
  `FUN_0061d5a0` is **Seidel's randomised trapezoid triangulation** (the segment table at `0x733c78`, stride `0x38`, `next`/`prev`/`root0`/`root1`): `FUN_00620b50` makes the random permutation with
  `srand(time(0))` (`FUN_0063d106(0)` -> `FUN_0063dce5`) and `_rand() % n`. The order of the triangles is therefore a function of the clock. The live files agree: the same square (200,200)-(800,800) is
  clipped `[0 3 1] [3 2 1]` in `botlab-newbot-height-min`, `-height-max` and `-square-small` but `[2 1 3] [1 0 3]` in `-tall`, `-thin`, `-square-big` and `botlab-adjust-square-taper`. Ours keeps one valid order
  (the sets of triangles are equal to every capture's); nothing to change.
* **The 6.22 px group of `triangle-bent` (open).** Its polygon is ours exactly (the quad `(800,200) (500,800) (500,800,h) (800,400,h)` projected on the plane of the second triangle's normal: across
  `0 0 53.492 57.641`, along `0 100.623 100.623 26.833`, all four `(u, v)` match). Only the cell's place differs: the base edge is at v 92.7755 live, 99 ours (cell `y` 153.2245 against 147, the skyline of the
  `side0` cell above it: 81 + ceil 56.25 + 9). No quantity of the file equals 6.2245 (cell heights 56.25, 57.641, 53.492, 62 for the group's bounds, widths, gaps) and the placement code is not in
  `FUN_0044ae70` / `FUN_0044a310` (it only reads the finished layout), so the rule stays open; the test still skips that one `v`.
* **Chassis step 1 judges every click and drags its points.** Live, with the outline open (`botlab-step1-*`): the point the click would add shows under the cursor snapped to the grid, a **green ring** (0,204,0) when
  the editing area's rules take it, **red** (204,0,0) when they refuse it, with a 1 px (204,204,255) line to it from the last point (the outline's own lines are 2 px white). A click on an edge (500,200), one that
  makes the outline cross itself (900,500) and one that makes a corner too sharp (700,700) add **no point and raise no alert**. The alert latch `DAT_006f52e0` explains the silence: a point added to the list clears
  it (`FUN_004a0e00`), and nothing sets it before an accepted drag. Live proof the latch is the same one as step 2's: after a step 2 drag was accepted (latch set), step 1 cleared and four points added again
  a refused click is still silent (`botlab-step1-refused-latched`), but **with the outline closed (a click on the first point) its points drag like step 2's** (the tip changes to *Click the step 2 button to adjust your
  chassis*), the first accepted drag sets the latch and a refused place then raises *Points should not be too close to other points or lines* (`botlab-step1-drag-alert`: the dragged point stopped 100 units from
  its neighbour). The point the second click of a new outline adds is taken (two points are one edge both ways round: `check_move` no longer calls that a crossing, n > 2). Ours: `adjust::check_add`,
  `Build::adjust_bottom_point`, `Builder::{on_point_screen, editable_points}` (press / move / release shared with step 2, alert through the same `adjust_armed`), the hover ring and the rubber band in the overlay.
  Tests `a_point_added_in_step_one_is_judged_by_the_same_rules`, `playthrough::chassis_step_one_refuses_points_by_the_editing_areas_rules_and_drags_them_once_closed`.
* **Height slider track.** The gib's track is an `Image` with no file, `_Size 4 121`, `_BufferWidth 11 0 0 0` in a 26 px column: live (`botlab-adjust-hover`) it is the bar at x 963..966, ours filled the column (952..977).
  A file-less `Image` now keeps its `_BufferWidth` clear on both sides (`ra2ui::sized`; the only image of the shipped screens that sets one).
* **Against the captures** (ours through `labshot`, the same clicks; MAE over the frame / the blueprint panel): `botlab-step1-hover-valid` 1.81 / 1.98, `-refused-close` 1.84 / 2.05, `-refused-cross` 1.84 / 2.07,
  `-refused-sharp` 1.81 / 1.98 (the rings are (0,204,0) and (204,0,0) as live, the refused click adds nothing and shows no alert), `botlab-adjust-hover` 1.40 / 1.19 with the slider track at x 963..966 as live.
  Gate: fmt, clippy `-D warnings` clean; 837 tests passed, 0 failed, 2 ignored; `find local/game -newer` printed nothing.
* **Title and logo (open, measured).** The blueprint panel is **one pixel shorter live** (bottom border row 664, ours 665, in step 1 and step 2 alike; the left, top and right borders agree) and its `RA2 Chassis` logo and
  caption (`topplate_label.bmp` at `Position 0.25 1 0.936 1`) sit 1 and 2 px lower in ours for that reason (rows 625 / 633 against 624 / 631). The `ROBOT WORKSHOP` title itself is at rows 13..25 in both. The panel's
  height comes from `AspectRatio 1.1` of a 487 px width, and ours gets 536 where the capture has 535; the cause is not found, so the panel is unchanged.
* **MP lobby chat's first line (blocked).** The box's `Position 0 0 0 1242948` is `{amount, flag}` per axis (the widget writer `FUN_005e7ca0` prints `Position` from `+0x34` with `FUN_00608380`, two 8-byte axes);
  the stray word is a non-zero flag, which reads as a fraction of the parent and the amount is 0, so it moves nothing; nothing in the writer, the field table or the gib gives the 2 px, and the DisplayBox's
  draw routine was not found. The hand placement `Position 0 0 2 0` (docs/38 16.8) stays.
* **Open.** The 6.22 px cell, the panel's missing pixel (title/logo), the key behind header line 4 (what a design edited after a load writes: only chassis edits were observed to clear it), the chat's 2 px.

## 9.28 Release check: Enter Event without a robot, Select Team after a restart (`fx/release`, 2026-10-28)

Live on display :9 (flatpak Wine 9.0, `ra2_drive.py`; the prefix's `Teams/`, `AI/`, `Robot Designs/` and `ra2.cfg` copied before the session and restored after it, sha256 list equal). New captures
(manifest rows, backup `manifest-additions-2026-10-21-release.tsv`): `teamhq-event-join-norobots`, `teamhq-select-restart-last`.

### Enter Event with no robot (`FUN_00481f20`)

The join window always opens (`FUN_00500c40` builds `joinevent.gib` whatever the roster is). `FUN_00481f20` starts with `if (-1 < *(int *)(this + 0x16c))`: the window's chosen robot, which the
constructor `0x481b60` sets to -1 (`in_ECX[0x5b]`), and which only the `robot grid` handler `FUN_00482590` changes. With no robot chosen it returns at once: no box, no sound of its own, the window stays open.
Live on a new team (no robots: six `New Robot` cells): Event Schedule -> Pit of Destruction -> `Enter Event!` (clicked twice) -> unchanged (`teamhq-event-join-norobots`, the grid
is six empty cells and the name box empty). Ours matched the "nothing happens" half but **closed the window** (it popped the window on any confirm) and dropped the pending event, so a robot picked afterwards could not
enter either. Now the confirm keeps the window when no robot is chosen (`Shell::click`, `join_chosen`), and `enter_event` no longer clears the pending event first. Also from `teamhq-event-join`
(The Rookies, six robots): **no robot is chosen when the window opens**; ours preselected the first robot (listed open in 9.22). Now none. Tests `playthrough::enter_event_without_a_robot_does_nothing`,
`finish_later_keeps_the_entered_event_in_the_team_record` (clicks a robot first).

### Select Team after a restart (`FUN_004ec2c0`)

The window's init reads the grid's selection (`+0xfc`), refills the grid (`FUN_004ec310`), then selects that row, or **count - 1 (the last team)** when it is -1 (`FUN_0043c930` = `+0xcc`, the row count).
The window is cached by name, so within a run it keeps the row last picked (live: pick the second team, Main Menu, Team HQ -> second team selected). Nothing is persisted: `Teams/Teams.txt` is
byte-identical after Continue (its head line `0 1 ` is the list of used slots, not a selection), `ra2.cfg` has no such key and the registry key `SOFTWARE\Gabriel Interactive\Robot Arena 2` holds only the install path.
Live with two teams, The Rookies (row 0) played, game restarted: the **last row** (the new team) is selected (`teamhq-select-restart-last`); after creating a team it is selected as well. Ours always selected row 0
(`fill_select_grid` takes the row now; `Shell::select_row` remembers it). Test `playthrough::select_team_opens_on_the_last_team_and_keeps_the_pick_within_a_run`.

### Release check as a new player (windowed `ra2-app`, scratch install, `ORA_SAVE_DIR` scratch, Xephyr :19 1280x1024, XTEST)

Empty save dir -> main menu -> Exhibition (defaults: Deathmatch, Combat Arena, C.R.U.S.H. vs Roly Poly, 3 min) -> Skip Intro -> W/A driving -> results (`Match Results`, Continue) -> Main Menu -> Team HQ -> New Team (typed
name) -> Continue -> Import Bot (the list's rows draw no text, as in the original: `teamhq-import-dialog`) -> Event Schedule -> join window -> Enter Event! -> bracket -> Finish Later -> Options (Audio: mute, music
slider; Video: resolution list 640x480..1280x1024, OK) -> Quit. Restart: the window opens at the saved 1024x768, Select Team lists both teams with the new one selected, and the Event Schedule shows
`E1/9 R1` with Blade Tournament in progress. The save dir holds `ra2.cfg`, `AI/Teams.txt`, `Teams/Teams.txt`, `Teams/career1.dat`, `Teams/Team1/Bot0.bot`; the install copy and `local/game` were not written,
`~/.local/share/open-robot-arena` was never created. A resolution change applies at the next start (the window is not resized live). No gamepad exists. Error messages for a wrong `--game`: `game::check`.

## 9.29 The Bot Lab end to end, the original against ours (`fx/botlab-e2e`, 2026-11-04)

One scripted build, run through the real input path in both: the click scripts are `tools/oracle/routes/botlab/*.steps` (the files `ra2_drive.py -f` plays on the live game and `labshot -f` replays on ours;
`labshot` drops `wait` / `hide`, turns `dragh` into `drag`, `click 55 18` into `back`, `SHOTS/` into `$LABSHOT_SHOTS`); `tools/oracle/botlab_e2e_compare.py LIVE OURS OUT` diffs the frames per region.
The route: blank cell, chassis step 1 (a 800 x 800 square), step 2 (a taper and the height slider to the top), armour, the Control Board, a refused drop, a cancelled drag, a Small Battery Pack placed / selected /
removed / placed, two Reversible Angle Motors each with a Buzzard wheel on its axle, a Red Bird Spin Motor with a Canine VI ChewBlade, three controls (analog W-S "drive", analog A-D "turn", button space "fire"),
every motor wired through its connection window, paint, Test Robot driven by W A S D / space, name, snapshot, Back to HQ. Live on display :9, `Teams/`, `AI/`, `Custom Textures/` and `ra2.cfg` copied before and
restored after (sha256 lists equal). New driver steps in `ra2_drive.py`: `key NAME [down|up]` (held 80 ms: the 30 ms tap was lost under a modal box), `cam PITCH YAW [DIST]` (writes the lab camera object: the injected
middle-button orbit does not reach the game), `orbit`, `wheel`; the driver no longer waits for `detach` (with a modal box open it hung for 25 s). Captures (backup `manifest-additions-2026-11-05-botlab-e2e.tsv`):
`botlab-e2e-*` (74 frames of the route), `botlab-e2e-newbot` (the saved `.bot`), `-armor-*` (the four default appearances), `-burst-default` / `-burst-adjusted`, `-paint-effect-list`,
`-comppreview-viewproj` (`ra2_viewproj.js`).

**The saved file.** Live `Bot4.bot` against ours `Bot5.bot` for the same clicks: the same 9 components with the same parents, mounts and (to 0.3 px of the mouse ray) positions, the same outlines, height, normals, group
rectangles and weight (293.1 both). What differs: the random ear order of the plates (9.27f), `0.382813` against `0.382812` (the exe's CRT rounds an exactly representable tie up, ours half to even: open),
`Name: drive` (live keysyms have no shift: the route is lowercase), line 4 / the skin (below), the armour atlas.

### Mismatches found, fixed

1. **The palette was not the original's.** Live hides `hidden = 2` parts (Cannon, Flamethrower, Hovercraft Engine, Magnet; also `standard = 0`) and orders every category by `sort` then **display name**, not file name
   (the Buzzard wheel, `yellow wheel.txt`, is the second wheel; the Angle Connector the first extender; Canine VI before Iron Spike). `ra2ui::components::all`.
2. **The palette could not be scrolled** (no arrow, thumb or track handling: parts past the fifth cell were unreachable in the player-facing lab). Live: the row scrolls by pixel, an arrow reveals the next
   cell whole (from 0: 75, 171, 267 = 96 k - 21 with the 501 px view), a press on the track puts the thumb's centre under the cursor and holds it, a sixth cell shows clipped. `Builder::palette_bar_press`,
   `ra2ui::input::set_horizontal_thumb`; ours against the survey frames: every category at every page equal to the 4-5 MAE of the thumbnails' filtering.
3. **Slider drags.** The structure screens' sliders (the height thumb) took only the press: now the held button follows (`slider_drag`). The route's height drag (0.825 to 1.32) had left ours at 0.825 and 142 kg against 182 kg.
4. **The height slider moves vertices only.** A top point dragged at 0.825 then the slider to 1.32 saves the 0.825 normals (live `0.959264 0.255804 0.119908`, ours `0.97898 0.163163 ...`) and the skin layout planned from them.
   `extrude_at`, `Build::normal_height`.
5. **A part under the cursor.** A wheel dragged over a motor's body goes on the motor's axle although the cursor is 40 cm from it (ours needed 9 cm of the socket): `placement::pick` takes the nearest part under the
   ray and its nearest free compatible socket. **The collision judge** tested bounding boxes and refused a round wheel whose box clipped the control board (live accepts it): exact GJK on the mesh objects' point
   sets (`workshop::gjk`, `ComponentInfo::hulls`), the box test only as broad phase.
6. **The component preview** (the part's picture with its green boxes) autoscaled to fill its box; live is one fixed camera for every part: the lab orbit camera at pitch -0.49, yaw -0.65, 1.7 m from the part's origin, 45
   degrees, near 0.001, far 10, viewport 215 (`ra2_viewproj.js`). Its header: the weight box (`Alignment 20`, `Buffer 16`) lacked the `kg`, sat 13 px low (a `TextBox` with the vertical-centre bit centres its line),
   ended at the panel's edge (the buffer keeps it 16 px in) and was dimmed to 0.7 (the name box's `Background` band drew over it; ordered before it now). `ra2ui::text_box_middle`, Buffer in the right-aligned case.
7. **Armour grid:** the selected row's bar, `armor name` and `concussion value` were not filled; ours showed no selection.
8. **The lab's wire frame** drew the skew diagonal of a tapered side; live draws each side's loop (`lab_view::ring_edges`).
9. Harness: `labshot` now sends the release after every click (a held picker followed the next click), `-f` route files, `key NAME down|up`, `space`, `hover`, `drawn`, `cam P Y [D]`.

### Mismatches found, open

* **Selecting a placed part shows its adjustment page** (live `adjust_none.gib`: *This component needs no adjustments.*, header with no weight; `adjust_burst.gib`: two sliders, *Range of motion start / end*,
  stored as the two angles after the burst motor's file line: untouched `0 1.5708` for the DDT (default end 90 degrees, `burst = 0 50 90`), `0.201241 2.12669` after the start track click and an end drag;
  `adjust_zone.gib` for the Smart Zone). **Ours has no adjustment page at all**, so a burst motor or piston cannot be given its swing range in the player-facing lab (`burst_angles` are only read from files).
  `botlab-e2e-burst-*`; the slider-to-angle map was not recovered (exe: `0x446aa2`, `0x446ae2` name the sliders).
* **Paint Shop effects.** Live has five (`Color Fill`, `Texture Fill`, `Surface Layers`, `Edge Lines`, `Decals`: `botlab-e2e-paint-effect-list`, pages `texturefill.gib`, `surfacelayer.gib`, `edgelines.gib`, `decals.gib` with
  the Texture Library lists); ours opens only Color Fill (the dropdown does not open, the model `ra2ui::workshop::paint` exists). **The armour's default appearance** ("apply default chassis appearance", checked
  by default): picking a plate paints the chassis with a texture and rail edges (plastic: `red_plastic.bmp`, steel: `diamondplate.tga`, aluminium: ridged panels with `dotted_rail`; the four atlases are
  `botlab-e2e-armor-*`); ours leaves the skin white, so every lab-built robot saves white and header line 4 stays 0 (live: 1 and a skin as soon as the skin is touched: the line is the skin flag, not "loaded").
* **Forward heading** slider on the Overview does nothing in ours.
* **Test Robot** frames differ widely (MAE 8-45) after the first seconds of driving; not analysed (the practice arena was matched at rest in 9.27b; the keys bound in the route do drive ours, W reaches the far wall).
* The mouse ray lands 0.002 m (a quarter pixel) from the original's in the baseplate's depth axis.
* Colour of the typed name box focus ring and the delete-control message are as before (9.27c).

Tests: `workshop::gjk` (2), the palette order and hidden parts in `components::tests`. Gate: fmt and clippy `-D warnings` clean; 870 tests passed, 0 failed, 2 ignored; `find local/game -newer` printed nothing.

## 9.30 The component adjustment pages, the forward heading, the CRT's rounding, the route again (`fx/botlab-adjust`, 2026-11-05)

### Which page a part gets (`Robot Arena 2.exe`)

Picking a placed part in the 3D view (`FUN_0042d280` -> `FUN_0042d310(part)`) calls `FUN_0042f440(2, part)`: the part's `Adjuster` (`Component` vtable slot 20) says which `UI\BotLab` page goes into the screen's
`component content group`, and `FUN_005ec4e0(part != 0)` lights REMOVE. Only two classes override the slot, every other `Component` class (the 20 of `ControlBoard`, `Motor`, `SpinMotor`, `ServoMotor`, `Piston`,
`BurstPiston`, `Battery`, `AirTank`, `Wheel`, `Weapon`, `Cannon`, `Flamethrower`, `Hovercraft`, `Magnet`, `Steering`, ...) inherits the default `Component::Adjuster` (vtable `0x6a6820`, page name `FUN_00446d00`):

| class | page | what it holds |
|---|---|---|
| `BurstMotor` (vtable `0x6a679c`, slot `0x4469e0`; `BurstMotorAdjuster` `0x6a6810`, ctor `0x446a20`, init `0x446a70`) | `adjust_burst.gib` | `start angle slider`, `end angle slider` |
| `SmartZone` (`0x4ef6c0`; `SmartZoneAdjuster` `0x6ab1c0`, init `0x4ef750`) | `adjust_zone.gib` | the `zone name` box (`SmartZone + 0x280`, written and read by `0x4ef9c0` / `0x4ef990`) |
| every other class | `adjust_none.gib` | *This component needs no adjustments.* |

Each page's header is the part's name (`component name textbox`, no weight). The Smart Zone is `hidden = 1`, which keeps it off the player's palette, so its page cannot be reached in the lab (not built). The pistons, wheels,
weapons need no adjustment: the BurstPiston and the Reversible Angle Motor get the *none* page.

**The burst motor's angles** (`BurstMotor + 0x2a0` start, `+ 0x2a4` end, radians): the constructor (`0x446200`) sets them to `0` and `pi/2` (`0x6a6788`) whatever the definition's `burst = 0 50 90` says (that line is
`+0x2a8..0x2b0`, the physics); a fresh DDT saves `0 1.5708` (`botlab-e2e-burst-default`). The sliders run 0..1: start = value x 2 pi (`LAB_00446b90`, `0x6a678c`), end = (value - 0.5) x 2 x 2.4 (`LAB_00446be0`,
`0x6a6790` = 2.4, so -2.4 .. 2.4 rad); the page opens (`0x446c40`) with start / 2 pi (a negative start gets a turn added) and end / 4.8 + 0.5. The `.bot` line is `BurstMotor`, the definition path, `start end`.

**The slider itself** (live `botlab-adjust-24e..24i`, the track 297 px, the thumb 16 px, 281 px of travel; the value is the thumb's pixel offset over 281, as a float): `9 / 281` and `265 / 281` give exactly the saved `0.201241`
and `2.12669` of `botlab-e2e-burst-adjusted`; a press on the **track** puts the thumb's **left edge** under the cursor and takes no drag (x 578 on a track from 561 gave pixel 17, saved `0.380122`); a press on the
**thumb** grabs it where it is and the held button moves it by the mouse's travel (grab 801, drag to 834: pixel 232 -> 265).

Also changed on the Components tab to match live: picking a part (or missing every part) **empties the preview slot** (live `botlab-e2e-22`: after REMOVE the panel is bare until a palette cell is picked again; the
page replaces the preview while a part is picked); REMOVE is **dim** until a part is picked (`FUN_005ec4e0`); the palette's lit cell (full white, the others 0.6 grey, with the grid's `Sel Bar`) **follows the cell last
picked** (ours stayed on the first cell). Ours: `Builder::{refresh_comp_preview, burst_press, adjust_to}`, `Build::{burst_angles, set_burst_angles}`; a new burst motor saves `0 1.5708`.

### The forward heading (`FUN_00437770` ... `FUN_0043bdf0`)

The Overview's `heading slider` (180 px, thumb 16, 164 px of travel) is the robot's heading, the line after the buckle model in the `.bot` (`Bot + 0xac`). `FUN_0043bdf0(value)`: `heading = round(value x 20) x
0.05 x 2 pi` (`0x6a57f0` = 20, `0x6a621c`, `0x6a6218`; `FUN_0043be60` rounds), so twenty steps of 18 degrees; the thumb itself is not snapped. Same press rules as the burst sliders (live: click 700 -> pixel 75 -> step 9; the
drag that began at 640, a track press, did not follow; the end of the route saved step 2, `0.628319`). Every press on it (`FUN_0043be70`) also shows the **heading arrow** in the 3D view for 2 s (`+0x218`,
counted down by `FUN_0043e630`): `UI\BotLab\heading_arrow.gmf` at (sin h, 0, cos h) x 3 turned by h about Y, and a four pixel green line (0.12, 1, 0) from the robot's origin to it (`FUN_00439150`; the line is hidden
by the chassis where it passes under it). Ours draws both (`Builder::heading_press`, `heading_arrow_items`); the arrow and the line match the live frames `botlab-adjust-25b` / `25d` where the robot is drawn alike (the
rest of those frames is the armour skin, open).

### The .bot's numbers (`ra2fmt::bot::g6`)

The exe's CRT (`_fltout` / `_fptostr`) rounds the decimal digits **half up**; Rust's formatting (and glibc) rounds an exactly representable tie to even: `0.3828125` is `0.382813` live and was `0.382812` ours. `g6`
now cuts the exact decimal expansion at six digits itself; non-ties are unchanged, the `%g` rules (exponent below -4 or from 6, three-digit exponents) as
before. Every float of the writer goes through this one function (`num`).

### The route again (live display :7, `Teams/ AI/ Custom Textures/ ra2.cfg` copied before and restored after, sha256 lists equal)

New route files `03c-adjust.steps` (a DDT placed, picked, both sliders, the battery's *none* page, the motor again) and `03d-heading.steps` (Overview, the slider, the arrow, the saved heading); `labshot` turns `wait S` into
`age S` (the arrow's two seconds). Captures (backup `manifest-additions-2026-10-21-botlab-adjust.tsv`): `botlab-rerun-*` (the full route, 75 frames, `botlab-rerun-newbot` its `.bot`), `botlab-adjust-24c..25e` (12 frames),
`botlab-adjust-newbot` (the `.bot`: `0.380122 2.12669`, heading `0.628319`). Ours writes the same two values from the same clicks, byte for byte on those lines.

Per-step mean absolute difference of the whole frame, ours against `botlab-rerun-*` (frame / 3D pane), the groups' means: chassis 01-08 **1.27**; armour 09-12 5.66 (the armour skin, open); components 13-36 **3.35**
(3D pane 8.3: the first lab session of a fresh game has its camera about 1.5 % nearer than the later ones, a second run gave 1.7 %; the box and the parts land 1.5 % off the live positions in the saved `.bot`; against the
second session's frames of the same steps, `botlab-adjust-20..24`, the 3D pane is 7.3); wiring 40-56 **2.53**; paint 60-65 6.96 (the paint effects and the skin, open, 2.1 to 2.3 for the steps that do not show it); test robot 70-76 21.3
(7.9 at rest, 14.2 after one second of driving, 27-32 later; open); save 80-82 5.29. New and changed frames: 21 `battery-selected` **2.24** (9.61 before), 22 `battery-removed` **2.07** (10.93), 17-20 3.2-3.5 (4.3-4.6),
the adjustment pages 24e..24i **1.9**, the page's own area (512,365)-(1008,660) **0.9**, the heading slider row 3.5 (the label's subpixel offset only; the thumb is pixel-equal). Full table: `tools/oracle/botlab_e2e_compare.py` on the two directories.

Open: the Smart Zone page (unreachable from the palette); the armour default skin, the Paint Shop effects and the Test Robot frames as in 9.29; the overview camera of the first lab session; the tip line (live shows none after
the first alerts, ours keeps one) and the 1.5 % position scatter above.

Tests: `ra2fmt::bot` (`a_decimal_tie_rounds_up_as_the_original_does`), `build_mode` (`a_picked_burst_motor_shows_its_sliders_and_saves_their_angles`, `the_overview_heading_slider_saves_steps_of_a_twentieth_of_a_turn`). Gate: fmt and clippy `-D warnings`
clean; 876 tests passed, 0 failed, 2 ignored; `find local/game -newer` printed nothing.

## 9.31 The armour's default appearance and the Paint Shop's five effects (`fx/paintshop`, 2026-11-05)

### How the original draws a skin (`Robot Arena 2.exe`)

Every appearance operation is a render into the chassis texture: `FUN_005b2ce0` opens a 256 x 256 render target, `FUN_005b2ba0` draws the current skin into it, `FUN_005b2e70` draws one textured quad
(A B / C D, two triangles, `SRCALPHA / INVSRCALPHA`, the destination alpha untouched: every saved skin is alpha 255), `FUN_005b2cb0` closes it. `FUN_005b1e60 (name, flag, 0, 0)` loads the texture: **flag 0 is point
sampled and clamped, flag 1 bilinear and wrapped**. Measured against live exports (`paintfx-export-*`): a clamped quad samples `floor(x * 256 / width)` at the pixel's corner, the 256 px fills are exact copies, the
wrapped strips match bilinear at the pixel centre. A pixel centre on the quad's diagonal is drawn once (top-left rule). `ra2ui::workshop::skinfx` is this drawing on the CPU (`Canvas::quad`).

**The armour pick** (`FUN_0043ca70`, the mass check `800`, then the shared tail `0x43ccc5`, one case per armour type, only when the screen's *apply default chassis appearance* checkbox is on, `+0xd4`): the texture over
the whole skin, then `FUN_0044f690 (width, width, matrix, 1, ratio)`, i.e. an edge strip along every edge of every panel polygon (`FUN_0044f6f0`):

| type | fill | edge strips (file, half width, texture units per pixel) |
|---|---|---|
| 0 plastic | `red_plastic.bmp` | `light_edge.tga` 3, 0.5 |
| 1 aluminium | `aluminum_horiz.bmp` | `dirty_edge.tga` 14, 0.015; `light_edge.tga` 3, 0.5; `rivets.tga` 5, 0.1 |
| 2 steel | `gray_metal.bmp`, then `diamondplate.tga` | `shiny_edge.tga` 5, 0.5 |
| 3 titanium | `light_metal.bmp` | `rim_line.tga` 4, 0.25 |

(the same `width` and `ratio` are the `Length ratio` / `Width` of `Texture Library/edges/*.txt`). A strip is centred on the edge, `width` pixels to each side, `u = length x ratio` along it, `v` 0..1 across, **the
polygon walked backwards** (vertex `i + 1` to `i`), `v` flipped for every panel but the first, and the polygon moved by half a pixel (`+0.5, +0.5`). The pick leaves the header's fourth line `0`. Against the four live skins
(`botlab-e2e-armor-*`, whose names for steel and titanium are swapped, docs 9.29): mean abs difference per channel **0.012 / 0.109 / 0.125 / 0.024** (plastic / aluminium / steel / titanium), 2 levels at most except one
aluminium pixel group (19, the strips' corner overlaps). `Build::apply_default_appearance`, `skinfx::default_appearance`, test `the_armour_defaults_match_the_originals_skins`. The Armor grid's `High Bar` took the
click on the selected row's neighbour (the fourth plate could not be picked): fixed. The Overview's `No Robot Preview` placeholder (`exhibition\sample_bot.bmp`) was a white box.

### The five effects (`Appearance`, `FUN_00423010`; mode in `+0xdc`)

`FUN_004241f0` fills the `Effect` dropdown, `FUN_00424010` loads the page into `effect settings group`: Color Fill `colorfill.gib` (`FUN_00426590`), Texture Fill and **Surface Layers both `texturefill.gib`**
(`FUN_00426d90`, a different list: `Texture Library\*.bmp` / `*.tga`), Edge Lines `edgelines.gib` (`FUN_004274c0`, the `edges\*.txt` styles, the preview is the style's `preview` bitmap), Decals `decals.gib`
(`FUN_00428050`, `decals\*.tga` + `*.bmp` in one list, a `scale slider`, the SHIFT / CTRL tips). The lists are directory listings sorted as plain strings (`FUN_00429800`: `Galvanized_Metal.bmp` first); every pick
of an effect refills its list, selects row 0, scrolls to the top (and puts the Decals slider back to 1, `FUN_00428230`). The list scrolls by pixels (the arrows 18, the last row ends at the bottom; a press on the track
does nothing), the pictures of the selection are point sampled, row text sits one pixel left of the arena list's. Undo is **one** step (`FUN_00423ea0` copies the skin first; `FUN_00423f80` puts it back and dims the
button); the button is dim until the first operation (live `botlab-e2e-60/62/63/64`).

A click on the 3D view (`FUN_00424670`) takes the nearest triangle under the ray and its panel (the triangle's group), then by mode:

* Color Fill / Texture Fill / Surface Layers (`FUN_00424850`): a quad over the **panel's rectangle** (the `x1 y1 x2 y2` of the file's group row), a colour or the texture with uv 0..1, point sampled; the entire-chassis
  buttons (`FUN_00426bc0`, `FUN_00427300`) the same over 0..256. Live: lava/holey_metal on a panel is `floor(x * 256 / 84)` to 0.19 per channel.
* Edge Lines (`FUN_00424a80`): the strips of that panel's polygon only (width truncated to an integer); *Edge Entire Chassis* (`FUN_00427ea0`) every polygon.
* Decals (`FUN_00426130`): a square of `80 x slider` pixels centred on the hit's skin point (the triangle's uv at the barycentric hit), turned by `+0x3ac` radians (SHIFT: `+0.1` per mouse step, wrapped at 36, `FUN_00424c50`;
  CTRL: size `+0.025` per step, `FUN_00424cc0`), the texture's alpha, the quad `(-h,-h) (h,-h) (-h,h) (h,h)` rotated; then four `CopyRects` bands put the skin back outside the panel's rectangle. While the cursor is over the
  chassis the same square is drawn as a floating quad on the hit triangle's plane (it runs on past the panel; live `paintfx-e2-81..83`): `Builder::decal_preview_items`.

Ours: `ra2-app::build_mode::effects`, `Paint::{color_group, texture_group, edge_group, decal_on, ...}`, `skinfx`. The effect pages against live (`paintfx-page-*`): whole frame 0.65 to 0.97 (the lists' text, the point-sampled
picture, the scroll bar), 0.76 for Color Fill. **Export Paint Template** (the modal window `export_texture.gib`, `FUN_00424400`): the name box starts `My Robot texture`, the checkbox exports the current texture instead of a blank
template, OK writes `Custom Textures/NAME.bmp` (24-bit bottom-up, 256 x 256, 196 664 bytes: 54 + 196 608 + two zero bytes): the current texture after the armour pick is **0.15** off the live export, the blank template (red,
panels outlined in green 4 px) 1.3 (the pen's exact pixels are not recovered: 93 % of the line pixels agree).

### Verified live, and what is open

Live display :7, `Teams/ AI/ Custom Textures/ ra2.cfg` copied before and restored after each session (sha256 lists equal); captures `paintfx-*` (backup `manifest-additions-2026-10-21-paintshop.tsv`): the four pages, the
scroll frames, the hover and click frames, the Export window, four exported skins (armour, one decal, a texture fill, the template), three saved `.bot`s. Routes `05a`..`05e` (`05b` the effects on three faces, `05c` decals,
`05e` the exports; `labshot` learned `key Shift_L down|up`; `tools/labgame.sh` now copies `Custom Textures/`).

Per-step mean absolute difference of the whole frame against `botlab-rerun-*` (the full route; before 9.30's numbers): armour 09-12 **1.00** (5.66), paint 60-65 **1.20** (6.96), save 80-82 **2.97** (5.29; 80 1.06,
81 2.03, 82 5.81).

Open:

* **The decal's band restore.** After a decal the live skin differs from ours in a block at the skin's top left (about 110 x 84 px, content not found in the pre-decal skin at any offset or flip): the four `CopyRects`
  (`FUN_005b30a0`) of `FUN_00426130` do not restore the bands from the undo copy as read. Ours clips cleanly.
* **The picker's colour.** The swatch colour for the same click is lighter live (`232 198 140` against our `229 191 125`; `166 170 206` against `155 158 199`): the Shade / Tint rule is not plain HSL; the painted skin
  carries it (3.9 / 7.5 / 14.1 per channel on the route's skin). Two samples are not enough to fit it.
* **The live 3D view drifts.** A click in the view turns the live camera a little, and `cam P Y 5` frames the box 6 % nearer at pitch -0.5 than at -1.2 (the best fit is `5.3` against ours), so the 3D pane differs
  3 to 8 in these frames; the routes set `cam` before every click. A click placed from a drifted view picks other uv points: decal positions were not compared pixel for pixel.
* Import Custom Paint (the `import_texture.gib` window and its `Custom Textures/` list), the lists' scroll thumb drag and the wheel, the Export window's Escape / typing details (a stray `e` stayed in the first name).
* The header's fourth line is not "painted" (live `0` after a paint without parts, `1` once the Test Robot ran), as before.

Tests: `skinfx` (4: copy, the diagonal, the decal clip, the four armour skins against the oracle). Gate: fmt and clippy `-D warnings` clean; 880 tests passed, 0 failed, 2 ignored; `find local/game -newer` printed nothing.

## 9.32 The Test Robot against the live run, header line 4, the picker's shade, Import Custom Paint, the lists' scroll thumb (`fx/testdrive`, 2026-11-05)

Live display :7, `Teams/ AI/ Custom Textures/ ra2.cfg` copied before and put back after every session (sha256 lists equal, the prefix's `Custom Textures/` included). Captures (backup
`manifest-additions-2026-11-05-testdrive.tsv`): `testdrive-run{a,b,c}-7x-*` (the Test Robot frames of three full live runs), `testdrive-pose-run{a,b}` (the robot's `plus.getLocation` / `getHeading` / `getSpeed` against game
time), `testdrive-camera-runc` and `testdrive-drive-runc` (the scene camera every 50 ms and the key schedule), `testdrive-line4-g{1,2}` (the line-4 hook, the `.bot`s), `testdrive-import-*`,
`testdrive-listscroll-*`. New: `tools/oracle/testdrive_live.sh` (the run), `testdrive_pose.py` (pose table), `frida/ra2_line4.js`, routes `05f-import`, `05g-listscroll`; `labshot` learned `LABSHOT_POSE=1`, `pose`.

### Test Robot: what the divergence was

The frames diverged for two reasons that were ours, and a third that no port can remove.

1. **The harness did not run the practice arena through the route's waits** (`labshot` turned `wait` into the heading arrow's two seconds): ours stood still where the live robot had been driven for seconds, and the tip
   line stayed on the refused-part alert (`Components should not intersect...`, an alert the original drops after 2 s) where live shows `Click the Components button to continue working on your robot`.
2. **The live driver's own time.** `ra2_drive.py` takes 1.86 s per `shot` (xwd + PNG), 0.1 s per `key NAME down` (the xtest process), 0.25 s per click, 0.13 s per `hide`: the route's W is held 3.1 s (stamps `eval Date.now()`:
   down 8.513 s, up 11.644 s from the tab click) while the script says `wait 1.2`. `labshot` now advances the practice arena (and the alert / arrow timers) through every `wait` and through those overheads
   (`tickarena`), in 1/60 s slices: key W lands at game time 8.25-8.30 as live (heading leaves 0.05 at live 8.43 / 8.47 s, ours 8.41 s).
3. **The live game is not repeatable.** Its frames are variable (28-40 ms per UI frame in the samples, the Havok step follows the frame time): three live runs of the same keys end 1-1.5 m apart (3 s after the heading starts to move:
   run a `(-1.06, 7.33)` heading 2.49, run b `(-2.77, 7.39)` 2.36, ours `(-2.74, 6.59)` 2.25; at 5 s a `(0.81, 8.47)`, b `(-0.21, 9.33)`, ours `(-0.66, 8.63)`). Ours is inside that spread: the robot accelerates to
   3.5 m/s in 0.5 s as live, turns left at 0.6-0.9 rad/s (the wheels' asymmetry), coasts to rest in 1.1 s (live 3.3 to 0.25 m/s, ours 3.1 to 0.7) and stops against the far wall at z 8.4-8.7.

**Camera, verified live** (`ra2_camera.js`, `testdrive-camera-runc`): the eye is fixed at `(-4.2, 2.5, 9.5)`, the target is the robot object's position (`+0x8c`, y 0.26 as ours' body), the field of view follows the distance with the
`FovRange` (`range 2 20 0.8976 0.3491`: 0.606 at 11.6 m, 0.855 at 6 m); there is no follow smoothing and no other mode. The HUD and the Obstacles list were equal already.

Mean absolute difference of the whole frame against `botlab-rerun-70..76` (the full route): **21.30 -> 16.75** (70 rest 7.88 -> 6.40, 71 14.18 -> 8.43, 72 29.71 -> 23.97, 73 30.09 -> 25.33, 74 27.29 -> 18.74, 75 32.25 -> 28.08,
76 reset 7.68 -> 6.32). The floor is the live game against itself: the same seven frames of the live runs a / b / c / the old capture differ by **11.8 / 14.3 / 15.4 / 14.8** from each other (rest frames 5.3-6.0 alone), ours against
the three runs by **12.9 / 10.5 / 13.4**. The remaining difference is the live run's own spread. Open: the robot's shadow on the practice floor is a large soft patch beside the robot live and a thin line under it in ours (the light
of `Practice.py`'s `SetShadowSource` is overhead); not traced (the hook's first frames were the lab's).

### Header line 4 (`FUN_004df560`)

Hooked live over the whole route (`ra2_line4.js`, `testdrive-line4-g1` with the Test Robot, `-g2` without): `FUN_004df560` is `find_if(begin, end, !part->flag)` over the robot object's part vector (`+0x1c` / `+0x20`, 4-byte
pointers; the predicate `0x4df5c0` calls `0x45d3b0`, the byte at `part + 0xf0`) and the line is whether it found none. The byte is 0 in the constructor (`0x461c55`, `0x461df8`) and set only by a loader (copied by
`0x42e12f`, `0x42f1f6`). Live: at the Test Robot tab's entry (the robot saved for the arena) the first part has 0 -> 0; without the Test Robot the final save finds the first part 0 -> **0**; with it the final save runs on a
**new robot object** (`0x8083330` against `0x1b6cab0`: the lab took the robot back from the file) and all nine parts have 1 -> **1** (`Bot4.bot` of `g1`: `1`, of `g2`: `0`). So the line is "every part was made by a loader":
`Build::new`, a chassis edit and **a part placed in the lab** write 0 (`attach_info`), a load writes 1 as before, and **leaving the Test Robot tab** makes every part a loaded one (`Build::reloaded_after_test`, the arena is dropped in
`sync_arena`). The older captures read through this rule (`botlab-e2e-armor-*`, `burst-*`) differ only in histories not recorded.

### The picker's colour (`FUN_00426d00`)

The swatch is `FUN_0061b6b0(h = x / w, s = 1 - y / h, l = FUN_005f4c30())` (a standard HSL to RGB, `FUN_0061b750` the hue helper): the two live swatches have the picker's hue and saturation (HSL 37.8 deg / 0.667 and
234 deg / 0.29) of ours and only the lightness differs (0.729 live, 0.694 ours). `FUN_005f4c30` is the slider's value, **the thumb's offset over the track less the thumb**: the press puts the thumb's *left edge* at the pointer, so
the value is `(x - left) / (width - 12)` (the 12 px `slider_hmini` thumb of the 239 px track), not `(x - left) / width`. With it ours reads `232 198 140` for the route's first click as live; the swatch bytes are cut down, not
rounded (`232 199 141` rounded). The second sample is `167 169 205` against live `166 170 206` (one count, a 2 px difference of the click in the picker). Paint frames 60-65: **1.20 -> 0.77**.

### Import Custom Paint (`FUN_00424520`, `ImportTextureWindow`)

`botlab/import_texture.gib` (`Import Custom Paint Job`): the list is `Custom Textures\*.bmp` (`FUN_00471ea0`) in **file-name order, case ignored** (`a texfill`, `B template`, `C decal`, `My Test A`), nothing picked on opening;
the rows' **text does not draw** (the list's font is `System`): what shows is a black line over every row and one under the last (5 lines for 4 files), no scroll bar while the rows fit (`PermScrolls false`). The pick is the
`Sel Bar` (violet, the list's whole 220 px), a hover the dim `High Bar`; a click below the last row drops the pick; OK with nothing picked only closes the window. **OK** posts message `0xcf` with the name
(`FUN_004720d0`); the Paint Shop's handler (`FUN_0043d740`) loads `Custom Textures\<name>` as the chassis' skin (`FUN_0044f4f0`): the skin is replaced as is (the exported current texture is byte for byte the imported file:
`testdrive-import-exported`) and **no undo step is taken** (the Undo button stays dim). Ours: `DialogKind::ImportTexture`, `Builder::open_import_dialog`, `Paint::import_skin`; against the five live frames the whole-frame difference is
1.1-1.4 (the window itself 3.2-3.7: the 3D view behind it).

### The lists' scroll thumb and the wheel

Texture Fill list (19 rows of 18 px, 263 px laid out): the thumb is draggable (`testdrive-listscroll-*`). It follows the pointer exactly (grabbed where it was pressed: thumb top 282 -> 392 for a 110 px drag), the list scrolls
**`floor(thumb travel / 201 * 90)` px** (0 / 49 / 90 / 13 for the four frames), the range being rows x 18 - 262 + 10 = 90. A press on the thumb reads the scroll from where the thumb is (`61c`: 54 became 53); a press on the rest
of the track does nothing. The arrows scroll 18 px to the end 80 and **show it as 104** (`61d`: 24 px past the content), the thumb then at the bottom, and one arrow back is 62 = 80 - 18 (`61e`). The thumb's position is
the whole pixel below its fraction (`trunc`). Ours (`effects.rs`, `ra2ui::input::set_scroll_fraction`): list frames 0.00-0.37 (`f00`..`f03`, `61`..`61e`; `61c` and `61e` were 10.4 and 31.2 before). **The wheel does nothing** over the list
(`wheel` raw events with the pointer on it, up and down, six notches: no frame changed; the lab's 3D view does take them).

### The decal's band restore (`FUN_00426130`), analysed, open

The tail builds four rectangles around the decal's rectangle `R` (`FUN_004287e0` of the size) and copies them (`FUN_005b30a0`, `this+0xd4` with the surface of `this+0xd0`): `(0, 0, A, R.top)`, `(0, R.bottom, A, B)`,
`(0, R.top, R.left, R.bottom)`, `(R.right, R.top, A, R.bottom)`, `A` and `B` the two surfaces' sizes taken in the other order (`ftol` of `0x4177c0`'s result is the right edge, of `0x406d90`'s the bottom): everything
outside `R` is put back from the undo copy. The live difference (`testdrive` exports `paintfx-export-armour` / `-decal`) is **exactly the block `(0, 0)-(219, 84)`** (the earlier 110 x 84 was read at half size), a region
of rails that is not in the pre-decal skin at any offset, flip, or scale (best 8.2 per channel), and equal to the later texture-fill export there: so it is a stale copy, not something the decal drew. Not reproduced:
what `0xd4` held at that moment is not known.

Tests: `workshop::build::tests` (line 4), `build_mode::tests` (the shade slider, the swatch cut), `effects::tests` (the scroll ranges). Gate: fmt and clippy `-D warnings` clean; 886 tests passed, 0 failed, 2 ignored; `find local/game -newer` printed nothing.

## 9.33 The Test Robot's shadow, the decal band restore, the suite (`fx/shadows`, 2026-11-05)

### The practice arena had no shadow at all

Not a thin line: nothing. `ScriptHost::create_arena_for_script` found the arena class by `__module__ == "Arenas.<stem>"`; `Practice.py` lives in `Scripts/` (`Arenas/__init__.py` ends with `import Practice`),
its module is `Practice`, so the class was never instantiated for the Test Robot, `AddShadowReceivers` never ran and `Duel::shadow` stayed `None` (probe: `update_effects` saw `state().arena == None`). The matching now also accepts
the plain stem. After it the practice arena declares what `Practice.py` writes: light `(0.653, 6.894, -0.418)` (the box arena's) and 8 receiver triangles (floor y 0.017, three walls), and the shadow is the same
`ShadowWizard` projection as the fight arenas (docs/34 11.1: the chassis mesh only, black alpha 0.3, one layer; every fight arena script, 13 of them, already instantiated through `Arenas.*` and drew it). The shape live is
the large dark patch the light's slant gives: the robot stands at z 8, the light at z -0.4, so the chassis' shadow runs about 1 m toward the camera under and in front of the robot.

Against the live frames (ours through `labshot` with the full route, receiver region `(400,380)-(640,450)` of the rest frame against the three live runs `testdrive-run{a,b,c}-70`): **7.69 / 7.23 / 7.13 -> 4.35 / 4.37 / 3.73**;
whole frames against `botlab-rerun-70..76`: mean of the seven **16.61 -> 16.69** (the frames are dominated by the robot's run-to-run position, 9.32; whole rest frame against runs a / b / c: 5.87 / 6.25 / 6.21 -> 5.65 / 6.34 / 6.15). Test: `match::tests::the_practice_arena_casts_the_robots_shadow`.

### The decal's band restore, probed live: it restores nothing

`tools/oracle/decal_live.sh` + `frida/ra2_decal.js` (`ra2_hook.py --bindir` keeps binary payloads) hook `FUN_00426130`, the snapshot `FUN_00423ea0` and `FUN_005b30a0` on the live game (menu -> lab -> chassis ->
armour default -> `05e-export-skins`). Captures `decalband-*` (backup `manifest-additions-2026-11-05-shadows.tsv`).

* `this+0xd4` is **a fresh snapshot of the skin made at the start of every effect** (`FUN_00423ea0`: `new FUN_005a87c0(skin texture at (this+0xd0)+0x2b4)`, a 256 x 256 format-22 (`A8R8G8B8`) texture). Read back with `LockRect`
  (`decalband-snapshot-pre`) it is **byte for byte the armour export**, and the snapshot taken by the next effect (`-post`) differs from it only in the nut. So the undo copy holds nothing stale.
* The first argument of `FUN_00426130` is the **group index** (1), not a size: `R = FUN_004287e0(group)` is the hit panel's rectangle `(95, 165, 179, 249)`, and the four rectangles are exactly
  `(0,0,256,165) (0,249,256,256) (0,165,95,249) (179,165,256,249)` (`decalband-trace`); `CopyRects(src = snapshot, dst = skin)` is called with them.
* **Yet the decal is not clipped.** `decalband-export-decal` (the nut centred at skin `(153.5, 183)`, 80 px) runs past `R` on three sides, 3266 of 4705 changed pixels are inside `R` and the rest outside: the
  `CopyRects` had no effect in the live game (Wine 9.0 wined3d, llvmpipe). Ours, drawn unclipped from the live armour skin, differs from the live decal skin by **0.16** per channel (clipped to `R`: 1.40).
  `Paint::decal_on` therefore draws on the whole skin (test `the_decal_matches_the_live_export_without_the_band_restore`); `Canvas::decal`'s clip rectangle stays for the unit test and `decal_group`.
* **The stale block `(0,0)-(219,84)` of `paintfx-export-decal` did not come back**: two more live runs of the same route (one logging only, one also reading the snapshot) show a clean skin, and the 3D view (`decalband-91-decal-top`)
  has no block (whole-frame 2.5 against `paintfx-e4-91-decal-top`, the nut at another point). Since the snapshot is exact and `R` is a different rectangle, the block is not the game's logic: it is a one-off of the wined3d
  `CopyRects` / llvmpipe path (stale memory in a copy that otherwise did nothing). It is **not reproducible and not reproduced**; the old `paintfx-export-decal` is kept as is. Whether a Windows D3D8 device would
  restore the bands is not known; the live oracle, the reference here, does not.

### The UI suite

`fidelity.py --renderer app` on this tree before and after: **1.83 -> 1.83** over 114 fixtures (the 1.59 of 11.12 was before the later arena fixtures; no UI change here). Open, measured: the Exhibition arena list
(`exhibition-arena-box`, 0.83) is 1 px high in ours below its top border (live border rows 120 / 282, rows start at 166; ours 120 / 281, 165): the list is one pixel short and its rows one pixel up;
`botlab-chassis` 0.45 and `botlab-overview` 0.39 not looked at.

Tests: practice shadow, decal against the live export. Probes: `tools/oracle/decal_live.sh`, `frida/ra2_decal.js`.

## 9.34 The UI suite: the arena list's ring, the repairs bench (`fx/uisuite`, 2026-11-05)

`fidelity.py --renderer app`, 114 fixtures, ours against `local/oracle/original`. Before 1.844, after 1.846 (the repairs fixture got worse for a reason that is a fix, below; the two exhibition arena fixtures improved).

### What the worst fixtures are

The top of the list is all 3D or state, not layout: `arena-metalskull-f2` 22.9 (ours has the two robots in the shot, the live frame has none yet and another clock: 2:47 vs 0:34), `arena-electric-intro/countdown`
11.0 / 9.5, `arena-bridge-intro2` 8.1, `arena-halfcircle-intro2`, `arena-parkinglot-f2`, `arena-octagon-f2/f4`, `arena-metalskull-intro2`, `-compressor-intro` (camera path sampled at a slightly different time: sub-pixel to few-pixel
offsets of whole meshes, the camera-name label ("Overhead") differing), `match-f1/pause`: **state or capture timing**. `event-bracket-duel-round1` 4.9: the 16 seeded teams are in another order (the bracket seeds at random), the
bracket's lines and boxes match: **state**. `event-award-photo` 4.5: the prize scene, a lost round in ours against a won one live: **state**. `event-repairs` 4.2: the live robot is another design (below). The UI-layout fixtures
are all at or under 1.5; `botlab-chassis` 0.45 and `botlab-overview` 0.39 are glyph-level noise (over8 1.1%, lowfreq 0.18 / 0.16), nothing layout-sized left.

### Fixed: the arena list (`exhibition-arena` 0.83 -> 0.73, `exhibition-arena-box` 0.83 -> 0.69)

Live (`exhibition-arena-box`): grey line at x 286 / 543, y 159 / 442, the black list 256 x 282 inside at (287, 160), the selection bar's first row at 178 (160 + 18). Ours drew the ring *outside* the scroller's 258 x 284
rectangle (rows 158 / 443). The file lists `[Scroller]` before `[Border]` for this list only (every other scroller list writes the border first and its ring is outside: `teamhq-editteam` got worse, 1.19 -> 1.34, when the
inset was applied to all of them), so the placement in `ra2ui` insets the list by the border's width when the scroller comes first. Test `the_arena_list_sits_inside_its_border_ring`.

### Fixed: the repairs bench

* The repairs screen forced every `SplitFrame` child to `Position 0 0 / Size 1 1 1 1` (`repairs::fit_splits_in`), which made the `Component / Time` header and the list the pane's 394 px; the file says 0.95 of it (374) and the
  bar column stands outside the header. Live: the header ends at x 932, "Time" at 820 (ours 834). Removed; the screen is placed as the file writes it (outside the robot panel the fixture's MAE is 1.29 -> 1.22).
* The 3D box stayed empty after the award because `Robot In Event` is reset to -1 when the event closes; the bench now shows that robot or, when it is closed, the first roster slot (what the match used). The fixture got *worse*
  (4.18 -> 4.70): the live capture's robot is another design (a diamond-plate box with a pole, none of Team0's six: probed) and a wrong robot costs more than an empty box. It is state.
* Tried and reverted: not clipping a ring to a split pane (the live bench's wood ring rows 720 / 721 are below the 720-row pane and ours are cut) fixed that edge but cost `botlab-paint` 0.58 -> 0.62; left open, with the
  bright inner 2 px of the bench's `repairbench_shadows` edge (live 181,151,103 at x 52, ours 146,119,76).

State-only or noise: all `arena-*`, `match-*`, `event-bracket-duel-*`, `event-award-photo`, `event-repairs` (robot), plus the glyph-level residue of the rest. No fixture was re-captured live; none of those states is reproducible
from our route (the robot of the live team, the seeded bracket order, the camera clock).

## 9.35 The event flow: the repairs bench, the team match, the kills (`fx/eventflow`, 2026-11-05)

Binary (`Robot Arena 2.exe`, Ghidra) and a live session (flatpak Wine, display :7; the prefix's `Teams/`, `AI/`, `ra2.cfg` copied before and restored after, sha256 list equal). New captures
(`local/oracle/original`, manifest rows, backup copy and `manifest-additions-2026-11-05-eventflow.tsv`): `event-repairs-atom`, `match-results-team`, `teamhq-kills-skulls`,
`team-match-ai-sample` (a trace).

### The Repairs bench (task 1)

* **When it opens.** `RepairsWindow::open` (`FUN_004d62a0`) has two callers. `FUN_0046ce50` (the end of a round: `FUN_0046c830` advances the bracket; a **won** round of a running event opens the bench
  with the fixed 30:00 clock, damaged or not; a lost round or the last round ends the event with `FUN_0046ca90` and **no bench**) and `FUN_00500c40` (the HQ's event button on an event that is
  already in progress, which shows the bench with the clock left over, `FUN_005011f0`, when `+0xf6` says repairs are pending). There is no bench from the HQ otherwise and none after the event closes.
* **Which robot.** `FUN_004d62a0` builds `Team::buildRobot(Robot In Event, copy damage)` (`FUN_004fa890`), which answers null unless the index is a key of the team's robot map
  (`Team+0x150`, `FUN_004fa3b0`): so the bench shows `Robot In Event` and, were it -1, an empty box. It is **not** the first roster slot (the previous round's choice, `docs/36` 9.34). Live
  (`event-repairs-atom`): the fifth robot of the join window, Atom Smasher, is the one in the box; the capture the fixture had was the prefix's own Bot0 (a modified C.R.U.S.H.), not "another design".
* **The title** is `"Round " + Team+0x110 + " Repairs"` (`FUN_004d6890`; `Team+0x110` is the round counter `FUN_004faaf0` has already incremented): "Round 1 Repairs" after the first round, "Round 2" after the
  second. Ours always said "Round 1".
* **The damage.** `FUN_004faaa0` (the event clear in `FUN_0046ca90`) sets `Robot In Event` and the event index to -1 and clears both damage maps (`FUN_004faa60`): what is left of the damage is
  dropped with the event. `Career::finish_event` clears `repairs`; the award's continue button returns to the HQ (it opened the bench when damage was pending).
* **The 3D box** is not a top view. The view object of the open window (probed live: `0x7b431b8`) holds a camera node at `(0, 0, -5)` with the identity rotation, a 45 degree vertical field of view,
  aspect 1 (455 x 455), near 1, far 2000; the robot stands at the origin with the identity orientation (`FUN_004d69d0`: `FUN_004dcff0`, quaternion (0,0,0,1), camera `(0, 0)` at distance 5.0:
  `FUN_0048cf10` / `FUN_0048cf30`): the robot is seen from 5 m behind, level. The lights were not probed; a white point light at the camera with a white specular term fits (the hex nuts at the bar's ends 206
  against 201, the blue plate 60/66/145 against 62/69/151; the lab's key light gave 103 and a dark plate). `build_mode::robot_bench_view`. Fixtures (`fidelity.py --only event-repairs --jobs 1`): `event-repairs` 4.2 -> 1.30, new `event-repairs-atom` 1.49; the
  whole app suite is 1.84 over 115 fixtures (1.846 before; in the parallel run the offscreen 3D box came out empty for both repairs fixtures under load, run them with `--jobs 1`).
* **The wood ring's bottom rows** (720 and 721) are drawn below the 720-row pane: a ring of `Width >= 2` on a split slot's child is no longer cut at the pane (`ra2-render::ui`). Open: the bright inner
  3 px of the bench's top and left edge (live 183,157,112 at x 52, ours 147,123,83): not the shadow image (black, alpha), it looks like the wood tile's wrapped edge row.
* Harness: `--event-robot N` (the join window's column) and `--force-round-win` give the between-rounds bench in a headless run; `fidelity.py` fixtures `event-repairs` (forced win) and `event-repairs-atom`.

### The team match (task 2)

A 2v2 TEAM MATCH (Combat Arena, Red Zone Scout + ALARM against Scout + Sentinel, the shipped brains) run live with `AI.running_ai` probes and in ours (`fidtrace team`, new). Ids are the slots in
reading order (0, 1 the red team, 2, 3 the blue). Verified: `GetEnemies` 0/1 = (2, 3), 2/3 = (0, 1) (a team mate is left out, ascending), `IsEnemy(i)` = `[0, 0, 1, 1]` / `[1, 1, 0, 0]` (self and mate 0), a robot
out of the fight drops from the *other* team's lists (`(3,)` at 13.0 s live) while its own list keeps its enemies, `GetNearestEnemy` goes through `GetEnemies`. Targeting in the first second is identical
(Engage targets 2, 2, 0, 1; `enemy_id` 2, 2, 0, 1 from 0.28 s; brain 1 switches to 3 at 1.1 s live / 1.18 ours), the start points equal to 0.01 m, tactics Invert/Unstuck the same set; the later fight
differs (live 2 is out at 13 s, ours at 23 s: physics, not the AI). `IsEnemy` on an id with no player, in a TEAM MATCH, reads a missing player (`FUN_004146c0` then `FUN_004177c0`): the probe never returned live; ours answers (true); not reproduced, no script asks. The AI branch needed no change.
**Found and fixed in the results window** (`match-results-team`): `FUN_004884b0` sorts the rows twice with `std::sort` (insertion sort at this size, stable): by points descending (`row+0xc4`,
`0x4889b0`) and then winners first (`player+0x74`, `0x488b00`). Live: ALARM 1147 and Scout 0 (the winning team) above Sentinel 23 and Scout 0, the slots being Scout, ALARM, Scout, Sentinel; the 1v1
capture `match-results` (Scout 26 winner above ALARM 663) agrees. Ours listed robot order. `MatchReport::results` sorts; test `the_results_rows_are_sorted_by_points_then_winners_first`.

### The rest of the flow (task 3)

* **Kills.** The Team HQ's `KILLS:` is `Team+0x50` drawn as skulls (`FUN_005009f0`: hundreds, twenties, fives, ones: `teamhq\skull100/20/5/1.tga`, each image one width after the last in the `kills group`).
  Ours drew nothing and reset the count at the end of every event, counting the *player's own* defeated robots. `FUN_004164a0` counts every player with state != 0 at the end of the match, `FUN_0046c400` adds
  the count when the round is **won**, nothing resets it (live: 3 skulls after a won round 1 with three opponents eliminated, a lost final, and after entering the next event). Fixed
  (`Career::add_kills`, `ra2ui::hq::kill_skulls`): the HQ crop is pixel-equal to `teamhq-kills-skulls` (MAE 0.27).
* **Entry fees, prize money, repair cost**: none in the exe (`docs/16` 3): the `10000` is the fastest-win sentinel; confirmed again live (the record after two events carries no cash field).
* Live checks that agree with ours: a won round 1 gives 80 event points (participation 160 / 2 rounds), the award shows `Rounds Won/Total: 1/2`, `Total Team Points Scored: 80`, the photo prize for one
  round won; the Blade Tournament is a 2-round bracket of 4-robot matches; "Event Completed" is on the finished cell.
* The live record after a won round plus two lost events ends `... 120 40 80 ... (league line), 0, 80, 3`: event points 0, season points 80, kills 3 (not compared line by line with ours).
* Not reproduced / open (the first three are closed in 9.37): the repairs bench from the HQ's event button with a partly served clock (`FUN_005011f0`'s minutes and seconds); the per-row time / damage text of a damaged bench (no damaged bench was reachable live); the wrapped bright edge of the bench's wood; the live fight of a
  2v2 against ours (physics).

## 9.36 Release-readiness pass: a player's session end to end (`fx/play4`, 2026-11-05)

Windowed `ra2-app` on Xephyr (:21, :22), XTEST input (`tools/capture/xtest.py`, window focused first: without a window manager the keys do not reach the app), a scratch install copy and
scratch `ORA_SAVE_DIR`s; `~/.local/share/open-robot-arena` never created, `local/game` untouched. No live (original) session was run in this pass.

### Found and fixed

* **A career round exited the app** (`ra2-app: could not locate game root (Components dir)`, black window, process gone). `find_game_root` walks up from a robot's own path, and a player's
  robots live in the save folder (`Teams/Team1/Bot3.bot`), never under the install; only the Bot Lab's practice arena set `RA2_GAME` for it. Any event round with an imported/built robot,
  and any save folder outside the install, hit it. `ra2_app::run` now names the install (`RA2_GAME`) as soon as it is resolved. Verified: Enter Event -> Play Next Round now plays, results,
  Continue, award.

### Checklist

Works (windowed, as a player):
* Start with `--game` (remembered in `game-dir.txt`); a wrong folder / no folder prints the reason on stderr (there is no folder dialog: documented in docs/40).
* Main menu; Options: Video tab (resolution list, fullscreen, detail), Audio tab (music mute, arena intros off), OK writes `ra2.cfg` in the save folder (`ArenaIntros=0`, `MusicMuted=1`); the install copy is not written.
* Select Team, New Team (name, motto, logo), Continue to the Team HQ; Import Bot into an empty cell; Workshop opens on a new robot, Chassis tab shows. (A full Bot Lab build is covered by the scripted routes of 9.29-9.32, not redone by hand.)
* Event Schedule, join window, Enter Event, bracket, Play Next Round, a lost round, Event Results (award scene), Continue; quit from the menu (clean exit) and restart: team, robot and cfg are loaded from the save folder.
* Exhibition: game types, arena list, computer-robot picker, a 2v2 Team Match with four computer robots, pause menu (Esc: Resume / End Match / Options / Quit), End Match back to the setup screen.
* Every arena and game type: `--play --gametype T --arena-index I` for deathmatch 0-7, battle royal 0-7, team match 0-7, tabletop 0-1, king of the hill 0-1 (28 runs) reach the results with no error, panic or traceback lines.
* LAN, two processes on loopback (two save dirs): host Create Game -> Start Game, joiner IP Address 127.0.0.1 -> Connect, both seated, chat both ways, Ready, Begin Match, both fight and end on the same Match Results, Continue returns both to the lobby with Ready cleared; joiner Disconnect frees its seat on the host at once; the joiner reconnects; a killed joiner (SIGKILL) leaves the host's lobby within 40 s.

Open / unverified:
* ~~A joiner's own **Disconnect** shows an OK box~~ **closed in 9.37**: the original shows no box (`FUN_00492540`, UI event 0x69, the server list's status "Retrieving Local Sessions ..."); ours matches.
* ~~End Match from the pause menu returns to the setup screen with no results window~~ **closed in 9.37**: checked live, the original does the same (`exhibition-end-match-setup`).
* ~~New Team name/motto: the XTEST helper sends no Shift~~ **closed in 9.37**: `tools/capture/xtest.py key A` holds Shift for a shifted symbol.
* The Team HQ's logo list and the Import Robot list draw no row text (as in the original, 9.28).
* `audio stream error: A buffer underrun or overrun occurred.` appears in stderr under heavy machine load (three agents building); no audible check.
* Not reached in this pass: the live damaged Repairs bench (Time text, per-row costs), the partly served clock (`FUN_005011f0`), the Repairs bench's bright inner edge (9.35): **all three reached and fixed in 9.37**. A by-hand Bot Lab build from nothing in a window.
* Windows/macOS builds, and Internet (GameSpy) play, remain unverified by design.

## 9.37 Network leave, the damaged bench, the shadow's edge, the Shift key, a regression test (`fx/netflow`, 2026-11-05)

Binary (`Robot Arena 2.exe`, Ghidra through REA) and one live session (flatpak Wine, display :7; the prefix's `Teams/`, `AI/`, `ra2.cfg` copied before and restored after, sha256 list equal, lock held 11 minutes).
New captures (`local/oracle/original`, manifest rows, backup copy and `manifest-additions-2026-11-05-netflow.tsv`): `exhibition-pause-menu`, `exhibition-end-match-setup`, `event-repairs-damaged`,
`event-repairs-partial`, `event-repairs-reopened`.

### 1. Regression test for the save-folder robot (`ra2-app/tests/save_folder_robot.rs`)

Copies a shipped design to `<save>/Teams/Team1/Bot3.bot` under a symlink-only scratch install, checks that nothing above the save folder looks like an install (`find_game_root` is `None`), runs
`ra2_app::run(--game <install> --save-dir <save> --offscreen ...)` and then loads the robot with `ra2_render::load_bot_components` and finds the root again. With the `RA2_GAME` line of `ra2_app::run` removed the
test fails (checked, then restored).

### 2. LAN: a joiner's own Disconnect (binary only)

* The prematch window's constructor `FUN_00490770` binds `disconnect button` (`0x6f46e0`, `0x4909f7`) to `FUN_00492540(1)`. It enables the begin / settings buttons again, shows the create window, the multiplayer and
  prematch windows' state, closes the session (`FUN_004d0380`) and, because the argument is 1, **posts UI event 0x69** (`FUN_00423700(0x69)`, `FUN_005882f0`). Nothing in it opens a message box.
* Event 0x69 has one consumer in this part of the exe: the server list's `FUN_0049a780` (the function that shows it, `0x49a8d4`) registers `FUN_00499560` for it. That handler writes the server list's status label
  (`[this+0x174]`) with **`Retrieving Local Sessions ...`** (`0x6f4c4c`; `Retrieving Internet Sessions ...` at `0x6f4c28` when the LAN radio is not the selected one), and nothing else.
* So after a voluntary Disconnect the original shows the server list with that status text and no box. The box captured as `mp-connection-termination` is the other path (the host vanishing: the dispatcher's case `0x75`,
  event `0x6c`, docs/38 13). Ours showed an OK box `Robot Arena 2` with an empty body: `NetPlay::on_step` now answers `NetAction::None`, refreshes the list and sets `net_play::RETRIEVING_LOCAL`; the test
  in `tests/netplay.rs` asserts no action, the status text and no window. Two original instances were not run (one flatpak Wine instance, and the multiplayer lobby does not open under Wine, docs/38 13).

### 3. End Match from the pause menu (live)

Exhibition, Scout against ALARM on the box arena, Esc after the countdown (`exhibition-pause-menu`: Resume Game / End Match / Options / Quit, time 2:46), End Match: the **Exhibition Match setup screen at once**
(Robots tab, both robots kept), shots taken immediately and a second later are the same screen, no Match Results window (`exhibition-end-match-setup`). Ours does the same; no change.

### 4. The damaged Repairs bench (live + binary)

Reached with `plus.damage(0, comp, 35, point)` on components 1-8 of Atom Smasher in round 1 of Strategic Engineering (a Blade Tournament has two rounds and no bench after the second), the AI eliminated with
`plus.eliminatePlayer`. What differs from what we had, and what is done:

* **The list draws no text.** Both columns are blank for every row (`event-repairs-damaged`), like the Team HQ's logo list; what it does draw is the cell lines of its rows: a 1 px black line at y = 251 + 18 k
  (the header's edge and above every row, one more under the last), x = 558, 819 (0.7 of the 373 px) and 931, and the selection bar (101,86,151) filling the selected row inside the lines. No rows (an undamaged
  robot, `event-repairs-atom`): none of it. `ra2ui::repairs` draws exactly that (pixel lines checked against the capture); the model rows keep their names and times.
* **Repair Item is immediate and costs time.** `FUN_004d6e10`: the selected row's component is restored at once, the row goes (`FUN_004d7010`) and its time comes off the clock (`FUN_0058b3d0`, hours / minutes /
  seconds with borrow) **if the clock still has that much** (`FUN_0058b470`: clock >= time); otherwise nothing changes. The row's time is `(max - hp) / max * 20 minutes` (`FUN_004d6480`, then
  `FUN_0058ada0(minutes, 1)` -> `FUN_0058b320`: `ftol` of the minutes, `ftol` of the rest times 60). Live: 35 damage on a 400 hp part cost 1:45 (30:00 -> 28:15 -> 26:30), 25 on another 1:16 (28:44).
  We advanced a progress bar in 6 s ticks against a per-row time of 30 min. Now `Career::repair_item`, `Repair::seconds = repair_row_seconds(hp, total)`, `Career::repair_clock` (30:00 when the bench opens after a won
  round, saved in the sidecar), `Session::repair_tick`; a row that does not fit stays (the headless loops stop there).
* **The 3D box ghosts the robot.** With rows on the bench the whole robot is drawn at a fifth of its colour over the (25,25,25) backing (median 0.20 over 1066 pixels) and the **selected row's part solid**
  (the blue plate, a wheel, a small block: `event-repairs-damaged`, `-partial`). `Session::prepare_repairs` draws the robot twice (all, and the selected part alone over a key colour) and composes them
  (`build_mode::robot_bench_view(.., only)`).
* **The bench reopened from the HQ** (`FUN_00500c40`, clock `FUN_005011f0` = `[hq+0xf8]` minutes, `[hq+0xfc]` seconds): reached live by leaving the bench with Main Menu, then Team HQ > Event Schedule > the event's
  cell: the bench again, `Round 2 Repairs`, **28:44** and the same rows (`event-repairs-reopened`). After `Finished` and Finish Later the same cell opens the bracket (the flag is `+0xf6`, cleared by `Finished`, not "damage
  left"). `Career::bench_pending` (saved) is that flag; `flow::reopen_bench` and `window.rs` open the bench instead of the bracket when it is set. Not done: the bench's own Main Menu button inside the event flow
  (the overlay has none), so the state is reachable only by closing the window there.
* Not decided: **which components get a row.** 8 damaged components gave 4 rows live; our report lists all 9 damaged parts. The map the match fills (`FUN_004dd110` writes `(id, hp, max)` per component through
  `FUN_004df6e0`) is not understood well enough to say which survive; the rows' count and the fractured icon's rule (`Team+0x13c`) stay as they were.
* Fixtures: `event-repairs` 1.30 -> 1.06, `event-repairs-atom` 1.49 -> 1.25 (`fidelity.py --renderer app --only event-repairs --jobs 1`).

### 5. The bench's bright inner edge

Not the wood tile and not the border: the **shadow image** (`repairbench_shadows.tga`, black with an alpha that is largest at its edges) is drawn with its alpha **faded in over its outermost four pixels** on
every side (live / ours ratio 1.27, 1.19, 1.12, 1.05 at the top, the same in all three channels; 1.24 at the bottom, 1.3 at the left and right, then 1.0). `ra2-render::ui::faded_edges` draws it as 81 bands with
the measured shares (`SHADOW_FADE_*`, corners take the product): the first pixel line's mean error against the live capture falls from 33-42 to 0.5 (top), 0.03 (bottom), 5.5 (left) and 9.7 (right). The left and right
edges keep a residual: the live pixel is bluer than a pure alpha fit gives (+8, +9, +12 in r, g, b); the cause is not found.

### 6. `tools/capture/xtest.py` and Shift

`key A` (any symbol that is the shifted level of its key: capitals, `!`, `?` ...) now presses Shift_L around the key (`down` holds both, `up` releases both). Checked with `xev` on Xephyr: `A`, `Q`, `question` arrive with
state 0x1, `b` and `1` with 0.

### Open

* Which components become rows (4 of 8 live) and the rule of the fractured icon; the bench's blue / additive residual at the left and right shadow edges.
* The bench has no Main Menu inside the event flow (the overlay is the Session's); the pending bench is reachable by closing the window.
* Whether the original keeps the partly served clock across an app restart is not known (it lives in the HQ object, `+0xf8`/`+0xfc`, and the live reopen was in one process); ours keeps it in the sidecar.

## 9.38 Which components get a bench row, the bench's bottom bar, the Paint Shop's fifth effect (`fx/benchrows`, 2026-11-06)

Binary only (`Robot Arena 2.exe`, Ghidra through REA and rizin). **No live session was run**: starting the shared flatpak Wine session was refused by the permission system, so nothing here is checked against a new
capture and no capture was added. The captures of 9.37 (`event-repairs-damaged`, `-partial`, `-reopened`) are the evidence used.

### 1. The row filter (`FUN_004dd110`, `FUN_004df6e0`, `FUN_004d6480`)

* `FUN_004dd110` (called from `FUN_004164a0` and `FUN_00415500`, career match, the end-of-round pass with `param_2 == 0`) walks the robot's component list (`this+0x18`, a vector of pointers) and
  records **every** component as `(id = [c+0x22c] (FUN_00416040), hp = ftol([c+0xec]), max = [c+0xe8] (FUN_00401780))` in a map through `FUN_004df6e0` (`FUN_004e2640` insert); the state `[c+0xe0]`
  (`FUN_004a6210`) goes into a second map (`FUN_004df740`). There is no kind filter in it.
* The id is unique: it is the counter `[factory+0x7c]` post-incremented by `FUN_004dbf30` and stored by `FUN_004dc2d0` in the component factory `FUN_004dbf40` (the function that then tests the type name `Wheel` ...). So no two components share a key and a map insert cannot drop one.
* `[c+0xe8]` (the max) is set in the component constructor `FUN_0045cc70`: 100 by default, then by the class switch at `0x45d097` (`FUN_0049aa30`, a six-way switch giving 400, 100, 1000 hitpoints), and overridden by the
  component file's own health (`FUN_004fe2c0`, `0x45d0f6`); `[c+0xec]` is the current hitpoints (float).
* The bench (`FUN_004d6480`, called by the window's constructor `FUN_004d62a0`) makes a row for a map entry that has none (`FUN_004dab00` = -1) **only if `max - hp > 0` as integers** (`0x4d66b6`..`0x4d66c0`:
  `mov ecx,[ebx+8]; sub ecx,[eax+4]; jle skip`), then fills the existing row with `(max - hp) / max` clamped to 1 times 20 minutes (the formula already in 9.37). The state map entries equal to 1 get the
  `UI\teamhq\damage.tga` icon (the fractured flag) on a row. Nothing else filters.
* So the rule is `trunc(hp) < max`, per component, for every component of the entered robot. Ours listed `detached || fractured || hp < total`; a part detached or fractured at full hitpoints got a row the
  original never makes. `ComponentDamage::needs_repair_row` is the original's test and `Career::repairs_from_report` uses it.
* **Not explained: 4 rows from 8 damaged components.** The filter cannot drop a component whose hitpoints went down, so the live count means only 4 of the components 1-8 actually lost hitpoints in the
  original (the `plus.damage(0, c, 35, point)` index `c` is not our design-order index, or the hit is absorbed: armour factor, a part with `max` 0 / not damageable), not that the bench hid 4. It is not
  settled which; ours still damages all 8 in its report. To settle it: hook `FUN_004df6e0` (cdecl-like thiscall, `ecx` = the record object, stack `(id, hp, max)`) with `ra2_drive.py eval`
  (`Interceptor.attach(ptr(0x4df6e0), ...)`), log the triples once at the end of the round, and compare with the `plus.damage` indices.
* Row times with several rows: each row's time is its own `(max - hp) / max * 20` minutes (`FUN_0058ada0(minutes, 1)`); the clock is only reduced by the Repair Item click of a row (`FUN_004d6e10`), never
  by the sum, so rows do not add up: they are paid one by one against the same clock (9.37, 28:44 after one 1:16 row).

### 2. The bench's bottom bar

The live bench shows the shared bottom bar (`Options`, `Main Menu`, `Help`, `event-repairs-damaged`); `UI/teamhq/repairs.gib` itself has only `repair button` and `finished button` (and the 48 px `Slot1` strip
the bar sits on). Ours drew the bar but the event flow's overlay took every click. In the window, a click on the bench that lands on the shell's `MainMenu button` (the shell is still on the bracket
screen under the overlay) now closes the overlay and gives the click to the shell: main menu, the rows and the clock are in the saved career (`Career::repair_clock`, `bench_pending`), and Team HQ >
Event Schedule > the event's cell opens the bench again (`flow::reopen_bench`, 9.37). `flow::nav_control_at`, `Session::on_repairs_screen`; test assertion in `playthrough.rs`
(`an_event_entered_in_the_shell_is_played_by_the_player_and_saved`). Options and Help on the bench are still not routed (the shell's window would be hidden by the overlay); the bar's drawing is as live
(`fidelity.py --renderer app --only event-repairs`: 1.06 / 1.25, unchanged).

### 3. The Paint Shop's "fifth effect"

There is none missing: 9.31 (merged as `d977df7`) already implements all five (`Color Fill`, `Texture Fill`, `Surface Layers`, `Edge Lines`, `Decals`; `FUN_00423010`, mode in `+0xdc`) and the 9.29 note that
said four was written before it. Surface Layers shares `texturefill.gib` and the Texture Fill list in the original too (`FUN_00426d90`), differing only in the mode's click handler (`FUN_00424850` takes the
same panel quad). Nothing to port; the open items are the ones 9.31-9.33 list.

### 4. The shadow's blue residual

Not looked at beyond reading: it needs the live draw path (blend state of `repairbench_shadows.tga`), which needs a session.

### Open

* Which of the damaged components the original really has hitpoints off (4 of 8), by logging `FUN_004df6e0` live.
* The bench's `Options` and `Help` buttons, the left/right shadow edge's +8/+9/+12 residual.
