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
* **The chassis side-adjust's bend alert is not reproduced:** the port has the outline editor only (`chassis::move_outline_point`); the mesh's side-adjust screen (4) with the normals' `< 0.75` test does not exist
  there. The alert API is ready for it (`Builder::show_alert`).
* **Practice-arena sounds, against `audio-practice-test`** (Test Robot, C.R.U.S.H.; W 2.5 s, Space, W 3 s into the far wall). Live: four `motorloop` voices (one per wheel) are created when W goes down, their pitch
  (`F`, Hz of the 22 050 Hz sample) ramps 11 536 -> 16 223 -> 21 128 -> 24 409 -> 28 591 over 1.1 s (0.52 -> 1.30 of the rate) and they stop (`S`) when W is released; no crowd, announcer or ceremony sounds; the
  workshop music keeps playing; at the wall `sounds\steel_collision.wav` (-7.95 dB, -13.41 dB) and `sounds\hammer_hit.wav` (-13.45, -14.51 dB) play. Ours (`labshot ... keys down 17 tick ... cues`): four
  `motorloop` voices on W, off on release, pitch 0.685 / 0.955 / 1.33 at 0.2 / 0.5 / 1.0 s (live, interpolated: 0.69 / 0.9 / 1.25-1.3: **equal to the sampling error**), the weapon key gives `burst_trigger.wav`
  (live: a sound of id 5 played with no call row, not named). **Differs:** ours plays **no impact** at the wall; the practice arena's `Impact` cues come from the match's priced contacts (`duel.impacts`), and a
  robot against the wall is not priced there. Live plays them for chassis and hammer against the arena. Owner: the combat / physics side (`fx/bell-landing`); the cue and its volume rule (`hundredths of dB` of
  the contact magnitude, docs/35) are the match's. New `labshot` step `cues`.
* **Tip RNG and the 1 px items** are in docs/34 11.8 (snapshot box, DisplayBox rows, the chat line).

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
