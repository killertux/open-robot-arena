# 36 — Playthrough: the rewrite as a game, flow by flow

**Measured 2026-10-19 on `fx/playthrough` (based on `fidelity` 50a779f).**
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
| Pause: Options | dead | **broken** |
| Pause: F1 controls reminder | missing | **broken** |
| Team HQ: select team, tabs, standings, history | works | works |
| Team HQ: robot inventory click | **crash** (client exits: `robot_cell.gib is not in UI`) | fixed |
| Team HQ: event cell click at 1280×1024 | **wrong event / dead-end screen** (`event_cell.gib` full screen) | fixed |
| Team HQ: join event → bracket | works | works (chosen robot now recorded) |
| Team HQ: `Play Next Round!` | **dead** | fixed |
| Event match driven by the player | **AI-only** (and Esc quit the game) | fixed |
| Round award, repairs, next round, back to HQ | unreachable from the menu | fixed |
| `Forfeit Event` | went to the HQ, event stayed open | fixed |
| Season end (after the ninth event) | unreachable | fixed (`season_results`/`golden_sawblade`, next season) |
| New Team: typing a name | **impossible** (no text input) | fixed (session only, see §7) |
| Selecting a team other than the first | opens the first team's HQ | **broken** |
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
* **differs** Options' Graphic Detail row: the first radio sits left of the
  column the other rows use (`50-options.png`). Visual only.
* **works** Credits modal and its Close (`53-credits.png`).
* **works** Help on select team, Team HQ, exhibition, the lab tabs; **fixed**
  multiplayer and create-game help (ids 12/13 of `FUN_004e4ab0`).
* **fixed** `Quit button`: the shell binds it to `Action::Quit` (`0x486c30`)
  but the window ignored it. The window now exits on any click the binding
  table maps to `Quit` (`flow::quits`). Test
  `playthrough::quit_and_the_multiplayer_buttons_are_bound`.
* **differs** Escape on any shell screen exits the whole client (this
  client's M7 choice). The original's menus are driven by its own DirectInput
  UI; Esc there was not checked this round. A stray Esc on the exhibition setup
  ends the game — worth checking against the original.

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
* **broken** Pause `Options` does nothing (the original opens the options
  window over the match). **broken** F1 "robot controls reminder" (the dialog
  itself tells the player to press it) is not implemented.
* **differs** `Space` is a drive key when the robot is wired to it, so it
  cannot skip the intro in a player match (Esc and the button do).
* **differs** The time dropdown shows `3` and does not open (its list is not in
  the data; the exe fills it). The weight-class filter is not applied.
* **differs** A match that times out is decided by this client's evidence
  rule (`plus.eliminatePlayer` on the robot with less armour+energy), not by
  the original's judges' decision.
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
* **broken** Selecting any team but the first named record still opens the
  first team's HQ, and a new team lives only in the session (never written to
  `Teams/Teams.txt`, no `Teams/Team<N>/` folder, no robots). The career model
  always plays `Career::from_tables`' first named record.
* **differs** Event matches are always one robot against one: a 4-bot event
  ("1 of 2, next: North Polers, HIGH VOLTAGE, RIOT") is played against the
  first listed opponent's first robot only. The match length is
  `flow::EVENT_MATCH_SECONDS` = 180 s (the exhibition default; `Events.txt` has
  no time); `--hq` keeps its own `--seconds`.
* **differs** The event flow screens (bracket, award, repairs) are drawn at
  the 1280×1005 design size letterboxed into the window, while the shell's HQ
  is laid out natively; the bracket is a text summary over the arena art
  (`? NEXT` glyphs for the missing arrow character), not the original's
  bracket graphic; the award draws the arena preview instead of the 3D prize
  scene.
* **differs** Opening the join window re-lays the schedule notebook behind it
  larger and shifted right (`41-join1.png`). Visual only.
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
* **differs** The workshop is drawn at the 1280×1005 design size letterboxed;
  its synthetic part/mount panel clips its text rows (`81-lab.png`); new parts
  on the baseplate are placed at a fixed spread (no drag placement); the
  snapshot button, the shift-mouse rotation and outline extrusion are open
  (docs/17); a new robot made from an empty selection is saved only to
  `Robot Designs/`, not added to the team roster.
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
* **differs** The LAN/Internet radio labels overlap and the `Internet` label is
  clipped; the create screen opens with an empty content area (the original's
  constructor loads `mpc_settings.gib`).

## 7. What remains for a working game (ordered)

1. **Team selection and new teams** (§4): play the selected record's career,
   write new teams to `Teams/Teams.txt` with their folder, add a new workshop
   robot to the roster.
2. **A save location**: the career and the workshop write into the install
   (`Teams/`, `Robot Designs/`), as the original does; a read-only or shared
   install needs a per-user copy.
3. **Event matches with the event's own field**: 4-bot rumbles and the
   opponent's chosen robot; the original's match length for events.
4. **Pause Options and the F1 controls reminder**.
5. **Match decisions at time-out** (judges) instead of the evidence rule.
6. **Escape on shell screens**: check what the original does before keeping
   "Esc quits".
7. Visual: the event flow and workshop at native resolution, the bracket
   graphic, the 3D prize scene, the join-window notebook shift, options radio
   column, multiplayer labels, the workshop panel's clipped rows.
8. Multiplayer networking (Join/Refresh/Start) if ever in scope.

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
