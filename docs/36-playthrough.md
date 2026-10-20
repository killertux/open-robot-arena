# 36 — Playthrough: the rewrite as a game, flow by flow

**Measured 2026-10-19 on `fx/playthrough` (based on `fidelity` 50a779f);
round 2 on `fx/play2` (based on `fidelity` beb1249), 2026-10-20, see §9.**
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
| Multiplayer: Join, Refresh, Start Game | dead | broken (no network layer; out of scope) |

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
* **differs** The resolution "dropdown" steps through modes on each click; the
  original opens a list (the list comes from the D3D device at run time).
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
* **differs** The time dropdown shows `3` and does not open (its list is not in
  the data; the exe fills it). The weight-class filter is not applied.
* **fixed (round 2)** A match the player plays that times out goes to the
  judges' decision (`FUN_00418770`), not the evidence rule (§9.6).
* **differs** With a deathmatch, slot 2 shows the same robot as slot 1 in the
  log (`slot 2: … robot 1: Sting Ray`) — unused, but the original leaves the
  slot `OPEN`.
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
  (`FUN_0046ad60`, §9.8). **differs** The award still draws the arena preview
  instead of the 3D prize scene.
* **not reproduced (round 2)** The join window's notebook shift: at the
  native 1280×1024 layout the schedule's frames are identical with and without
  the window (measured headlessly), and the round-2 capture shows no shift.
* **differs** An event entered and left with `Finish Later` before any round
  is played is kept for the session only (nothing is written until a round is
  awarded); the original keeps the entered event in its team record.
* **differs** `End Match` in an event leaves the round unplayed (our choice;
  the original's result-4 path for events was not recovered).

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
* **broken (out of scope)** Join Game, Refresh List, Connect, Start Game do
  nothing: there is no network layer.
* **fixed (round 2)** The LAN/Internet radio labels sit beside their radios
  (option-group labels are baseline-placed, §9.9); the create screen opens on
  its `mpc_settings.gib` tab.

## 7. What remains for a working game (ordered, after round 2)

1. **A save location**: the career and the workshop write into the install
   (`Teams/`, `Robot Designs/`), as the original does; a read-only or shared
   install needs a per-user copy.
2. **Event details**: the AI team's robot choice per event (robot 0 is used),
   `Finish Later` before a round is played (kept for the session only), the
   event `End Match` result-4 path, the award's 3D prize scene.
3. **Shell lists the exe fills at run time**: the resolution and match-time
   dropdown lists, the weight-class filter.
4. **A capture of the original's bracket and options windows** to confirm the
   recovered layouts (§9.8, §9.9).
5. Multiplayer networking (Join/Refresh/Start) if ever in scope.

Done in round 2 (§9): team selection and persistent teams, the event's own
field and match length, pause Options and F1, the judges at time-out, Esc on
shell screens, native event flow and workshop with the bracket graphic,
option-group labels, the workshop panel rows, the create-game tab, the new
workshop robot on the roster.

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
| 8 | Event screens and workshop native; bracket graphic | **fixed** (award's 3D prize scene still open) |
| 9 | Smaller items | 7 fixed, 1 not reproduced, rest open (9.9) |

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
* **open** Resolution dropdown list, time dropdown list, weight-class filter,
  deathmatch slot 2 label, `Finish Later` before a round (session only), event
  `End Match` result-4 path, the award's 3D prize scene, drag placement in the
  lab, the original's AI robot choice per team in an event (robot 0 is used).

### 9.10 Gate (round 2)

`cargo fmt --all --check` and `cargo clippy --release --offline --workspace
--all-targets -- -D warnings` clean; `cargo test --release --offline
--workspace --no-fail-fast` **551 passed, 0 failed** (538 at the start of the
round); `find ../local/game -newer <stamp>` printed nothing (every test that
writes a team, a career or a robot runs on a scratch install).
