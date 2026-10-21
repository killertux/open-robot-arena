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
  reproduced (gap).
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
  still open after 9.21 (the black bands turned out to be already equal).
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
