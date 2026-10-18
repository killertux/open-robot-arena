# 21 — Playing an exhibition match

This closes the last gameplay gap: the shipped exhibition setup starts a real
`ra2sim::match::Match`, and the player drives robot 0 in the window. The rules
are `docs/11-match-rules.md`'s; the presentation is `docs/14`; this page is the
flow, the keys, the camera choice and the headless evidence.

## 1. The flow

With no `--match`, `--build` or `--hq`, the client boots the main menu. The
shipped route is:

1. `exhibition button` → `exhibition/exhib_setup.gib` (the screen's own
   `link to exhibition setup`);
2. `robots button` → `exhibition/exhib_robots.gib` in `content group`; its four
   `playerN group` slots carry the note *"load appropriate gib for player N
   here"*;
3. click a slot → the shipped picker window:
   - slots 1 and 2 (`team 1 group`) → `selectbot_player.gib`
     (`Select Player Robot`);
   - slots 3 and 4 (`team 2 group`) → `selectbot_computer.gib`
     (`Select Computer Robot`);
4. in the picker, `Down Button` opens its own `team dropdown`, a row selects a
   team, a cell of `robots grid` selects that team's robot, and `ok button`
   accepts; the chosen `choosebot_cell.gib` is loaded into the slot;
5. `arena button` → `exhibition/arena.gib` in `content group`; the list's note
   *"populate with arena names"* fills it from `ra2ui::arenas` (the original's
   `Arenas/__init__.py` `enumAvailable(type)`);
6. click an arena row — the row is the list's own `Sel Bar` at `RowHeight` 18;
   the list's `Rows 6` is its viewport, and the mouse wheel or its scroller
   reaches the rest;
7. `hazard checkbox` toggles the arena's hazards (the checkbox's own note is
   *"enable or disable hazards for this arena"*);
8. `gametype button` → `exhibition/gametype.gib`; its
   `game type optiongroup` names the selected member (`deathmatch checkbox`,
   `battle royal checkbox`, `teams checkbox`, `tabletop checkbox`,
   `kingofhill checkbox`);
9. `start game button` — its note is *"start the game"*, not a shell link.

The `start game button` click is intercepted before the shell's transition
logic has anything to do: the app reads the setup screen's choices and builds
the match. The screen stays `exhibition/exhib_setup.gib` underneath, so the
result screen's `Continue` and the pause dialog's `quit button` return to the
setup screen, not the main menu.

### The choices

| choice | source | default |
|---|---|---|
| player team | `Teams/Teams.txt`, one of its **16** records, through `selectbot_player.gib`'s `team dropdown` | the career's team: the first named record (slot 0, `The Rookies` in the shipped file) |
| player robot | that record's `Robots:` slots, one `choosebot_cell.gib` per slot, `.bot` path `Teams/Team<slot>/Bot<bot>.bot` | the first `Robots:` slot (0) |
| opponent team | `AI/Teams.txt`, one of its **15** records, through `selectbot_computer.gib`'s `team dropdown` | slot 1 (`PREHISTORIC`) |
| opponent robot | that record's `Robots:` slots, path `AI/Team<slot>/Bot<bot>.bot` | slot 0 (`Roly Poly`) |
| arena | the selected row of `arena listbox`; the script is `Arenas/<script>.py` | `Arenas/box.py` when the type supports it, otherwise the first arena that does |
| arena rows | the list's own `Rows 6` viewport; wheel/scrollbar scroll the twelve registered arenas | — |
| game type | `game type optiongroup`'s `Selection` | its own shipped `deathmatch checkbox` selection, whether or not the tab was opened |
| hazards | the arena fragment's `hazard checkbox` | left at the script's default when the fragment is not loaded; no value is invented |
| match time | `time dropdown`'s visible `Text` (`5 minutes`) | 300 s; the option list itself is not in the data |

A player team slot the shipped `Teams.txt` leaves empty is a real blank row with
no robots. Selecting it and pressing `ok button` refuses with the team's empty
`Robots:` line named — no robot is invented.

The two windows are the original's own pickers, not a new panel. The team list
is the table's own records; the robot cells are `choosebot_cell.gib`, with its
`weight icon` set from the `.bot`'s own class (`light_icon`/`medium_icon`/
`heavy_icon`). The dropdown's open list is the file's own `ListBox`: opening it
hangs it below the 20-pixel field and lifts the field/group clips so the rows
are visible and clickable. That clip and position are view state; the list, its
rows and its `Down Button` are the shipped records.

### Team selection

The exhibition screen has **no team picker of its own** beyond the robot
windows' `team dropdown`. The player's team is the career's team; this client
reads `Teams/Teams.txt` and defaults to the first named record, exactly the rule
`ra2sim::career::Career::from_tables` uses. There is no separate career-team
menu here, and none is invented.

### Game type and arena availability

The original filters the arena picker by the arena's own `game_types`
(`Arenas/__init__.py`):

```python
list = [x for x in arenas if x.in_list and type in x.game_types]
```

`ra2ui::arenas::available(game, type)` is that filter: 8 arenas for
`DEATHMATCH`/`BATTLE ROYAL`/`TEAM MATCH`, 2 for `TABLETOP` (`clawtop`,
`flextop`) and 2 for `KING OF THE HILL` (`hilltop`, `king`). The list is filled
with all twelve until Game Settings has been opened, so the default headless
route can click any of the twelve; after that the list carries only the
selected type's arenas. If the arena already selected does not support a newly
selected type, the first arena that does is used and the run prints:

```text
arena: Combat Arena (box.py) does not support KING OF THE HILL; using the first arena that does
```

`--arena-index N` is the row in the list the tab actually shows (the filtered
list when `--gametype` selected the type first). `--arena` names the script and
is looked up in the same list.

## 2. The keys and the mouse

| input | action |
|---|---|
| `W` / `Up` | throttle +1 |
| `S` / `Down` | throttle −1 |
| `A` / `Left` | steer −1 |
| `D` / `Right` | steer +1 |
| release | that axis centres (the input is sampled at the next fixed tick) |
| `Esc` | open/close the shipped `common/ingame_dialog.gib` pause popup |
| `C` | cycle the camera |
| wheel | scroll the arena list, or the open team dropdown |

The match steps at its own fixed `MatchConfig::dt`; the window's 50 ms update
accumulates wall-clock time into those steps, so a key edge changes the value
sampled at the next physics step and never changes the step size.

`Match::step_with(inputs)` is the entry point: `inputs[i] == Some(DriveInput)`
drives robot `i`, `None` leaves it to the shipped AI (or the stand-in driver for
a robot the binding table does not cover). `Match::step()` is the empty slice.

## 3. The camera

A player-controlled match starts in **chase**:

```rust
DuelCamera::Chase { robot: player_robot, distance, height }
```

`distance`/`height` are scaled to the robot's own bounding radius (the same
2.6×/1.9× rule `ra2sim::CameraMode::chase_for` uses). **This is our choice**, not
one of the binary's three config cameras; the binary's vocabulary is
`watch`/`static`/`orbit` (`0x4759c0`). In a driving match `C` cycles
**chase → watch → orbit**, and the HUD's `camera textbox` prints the name it is
actually using (so it can read `chase`). An AI-only `--match`/HQ event keeps the
existing `watch → static → orbit` cycle from `ra2.cfg`.

The HUD still binds robot 0 to `player1 status`, so the player's own battery,
air, armour and points are the first slot; the audio listener is the chase
camera's eye, so positional sounds are heard from the driving frame.

## 4. Headless evidence

The default route (career team, AI team 1, `box.py`, hazards off for a
deterministic frame):

```sh
./engine/target/release/ra2-app --game local/game \
    --play --drive "0-2:1,0;2-4:1,1" --seconds 8 --out /tmp/ra2-play
```

The chosen route (team 0 robot 1, AI team 5 robot 1):

```sh
./engine/target/release/ra2-app --game local/game \
    --play --team 0 --bot 1 --opponent-team 5 --opponent-bot 1 \
    --drive "0-2:1,0;2-4:1,1" --seconds 8 --out /tmp/ra2-play-chosen
```

The other selectors:

```sh
# KING OF THE HILL: the arena list is filtered to hilltop and king, and no
# row clicked means the first supporting arena (hilltop) is used.
./engine/target/release/ra2-app --game local/game \
    --play --gametype king-of-hill --seconds 8 --out /tmp/ra2-play-koth

# Row 10 of the twelve (Parking Lot) needs the list scrolled five rows.
./engine/target/release/ra2-app --game local/game \
    --play --arena-index 10 --seconds 8 --out /tmp/ra2-play-row10

# TABLETOP's 2-row list; row 1 is FlexTop.
./engine/target/release/ra2-app --game local/game \
    --play --gametype tabletop --arena-index 1 --seconds 8 --out /tmp/ra2-play-tabletop

# The setup screen's match length (the dropdown shows 5 minutes; its option
# list is not in the data), overridden by --seconds for this run.
./engine/target/release/ra2-app --game local/game \
    --play --time 12 --seconds 8 --out /tmp/ra2-play-time
```

`--play` uses the existing `ra2sim::DriveScript` grammar
(`<start>-<end>:<throttle>,<steer>`, fight-time seconds). `--seconds` is the
explicit match length (the same engine field `--match` uses), `--out` is
required, and `--arena PATH` selects a picker row by script/name
(`Arenas/box.py` is the default). Without `--seconds`, a `--time` value is both
the setup's length and the run's; otherwise the run is 8 s. `--arena` and
`--arena-index` are exclusive. `--play` still needs `--out`; the window's route
is the same `Session` started by clicking `start game button`.

Every run:

1. clicks `exhibition button`, `robots button`, the player and computer slot
   regions, each picker's `Down Button`/`team dropdown` row/`robots grid`
   cell/`ok button`, `arena button`, the arena row (scrolling it into view),
   the hazard checkbox, `gametype button`, and clicks `start game button`;
2. writes `play-setup.png` from the shipped setup screen with the chosen row
   selected;
3. drives robot 0 with the script, samples the drive at the match's fixed step,
   and writes `play-drive.png` after the fight has begun;
4. runs to `Results` (the same evidence decision `--match` uses at the explicit
   length: `plus.eliminatePlayer` on the robot with less material when neither
   chassis was destroyed) and writes `play-results.png`.

All three frames are the design size, **1280×1005** by default.

Printed (the default run above, `--seconds 8`; the 8-second route is the one
`docs/14` measures):

```text
  click "exhibition button" -> exhibition/exhib_setup.gib [Button "exhibition button"] ... note "link to exhibition setup"
  click "robots button" -> exhibition/exhib_setup.gib [CheckBox "robots button"] ... content exhibition/exhib_robots.gib
  click 515 228.32 -> exhibition/exhib_setup.gib [Group "player1 group"] ... content exhibition/selectbot_player.gib
  click "Down Button" -> ... content exhibition/selectbot_player.gib
  click 480.5 425.5 -> ... [ListBox (unnamed)] ... content exhibition/selectbot_player.gib
  click 416 584.5 -> ... [Grid "robots grid"] ... content exhibition/selectbot_player.gib
  click "ok button" -> ... content exhibition/exhib_robots.gib
  click 515 380.68 -> ... [Group "player3 group"] ... content exhibition/selectbot_computer.gib
  click "Down Button" -> ... content exhibition/selectbot_computer.gib
  click 480.5 443.5 -> ... [ListBox (unnamed)] ... content exhibition/selectbot_computer.gib
  click 557 581.5 -> ... [Grid "robots grid"] ... content exhibition/selectbot_computer.gib
  click "ok button" -> ... content exhibition/exhib_robots.gib
  click "arena button" -> ... content exhibition/arena.gib
  click 561.5 234.89 -> ... [ListBox "arena listbox"] ... content exhibition/arena.gib
  click "hazard checkbox" -> ... content exhibition/arena.gib
  click "gametype button" -> ... content exhibition/gametype.gib
  click "start game button" -> exhibition/exhib_setup.gib [Button "start game button"] ...
exhibition: Combat Arena (box.py), game type DEATHMATCH, hazards off (hazard checkbox unchecked)
player team: The Rookies (Teams/Teams.txt slot 0), robot 0: C.R.U.S.H. (Teams/Team0/Bot0.bot)
opponent team: PREHISTORIC (AI/Teams.txt slot 1), robot 0: Roly Poly (AI/Team1/Bot0.bot)
selected rows: arena row 2 of 12, game type deathmatch checkbox, player team 0, player robot 0, opponent team 1, opponent robot 0
match time: 5 minutes (300 s from the time dropdown's displayed value; its option list is not in the data)
match length for this run: 8 s (--seconds when given, otherwise --time or the default; it overrides the dropdown's value)
  Countdown -> Fight at 0.00s, points [0, 0], winners []
  Fight -> Over at 8.00s, points [40, 38], winners [0]
state Results, winner C.R.U.S.H., points [("C.R.U.S.H.", 40), ("Roly Poly", 38)]
player robot: C.R.U.S.H. (robot 0), start (-9.82, -1.73, 10.01),
              end (8.17, -1.84, -4.78), distance 23.29 m
```

The chosen-route run prints the records and slots the click flow selected:

```text
player team: The Rookies (Teams/Teams.txt slot 0), robot 1: Sting Ray (.../Teams/Team0/Bot1.bot)
opponent team: Team SPARK (AI/Teams.txt slot 5), robot 1: The Boxer (.../AI/Team5/Bot1.bot)
selected rows: arena row 2 of 12, game type deathmatch checkbox, player team 0, player robot 1, opponent team 5, opponent robot 1
```

The exact end position and points depend on the arena's physics and the robots;
the run prints its own numbers. The tests assert the invariants, not those
floats:

- `ra2-app::play_mode::a_scripted_drive_moves_robot_zero_and_reaches_the_results`
  — the scripted drive moves robot 0 **at least 1 m net**, `Over` was observed,
  the final state is `Results`, and the three frames are **1280×1005**;
- `ra2-app::play_mode::the_start_button_starts_a_match_and_is_not_a_shell_link`
  — clicking `start game button` keeps the setup screen, carries no navigation
  note, and a real `Match` (two robots, player `Teams/Team0/Bot0.bot`,
  `DEATHMATCH`) can be built from the choices underneath;
- `ra2-app::exhibition::choosing_team_five_bot_one_and_opponent_team_three_bot_zero_changes_the_robots`
  — the two picker windows select `Teams/Team5/Bot1.bot` and
  `AI/Team3/Bot0.bot` (a test game gives team 5 the shipped team 0 roster), and
  the spawned names and `MatchConfig` paths are those files, not the defaults;
- `ra2-app::exhibition::king_of_the_hill_filters_the_picker_and_picks_hilltop`
  — `enumAvailable(KING OF THE HILL)` is `[hilltop.py, king.py]`, the box arena
  is absent, and the fallback is `Hill Top Arena`;
- `ra2-app::exhibition::a_game_type_that_the_selected_arena_does_not_support_is_replaced_and_named`
  — a box row selected before Game Settings then selecting KOTH produces the
  named replacement and `hilltop.py`;
- `ra2-app::exhibition::arena_index_ten_scrolls_the_picker_and_selects_parking_lot`
  — row 10 of 12 needs `_ScrollTop == 5` (six visible rows) and selects
  `Parking Lot`;
- `ra2-app::exhibition::the_arena_lists_scroller_and_wheel_move_the_viewport`
  — the list's own `+Scroller` bar is shown for twelve rows, its `More` arrow
  moves one row, and the wheel over the list moves two more;
- `ra2-app::exhibition::the_time_dropdowns_displayed_value_is_the_match_length`
  — `5 minutes` reads as 300 s, `--seconds` overrides it for a run, `--time`
  replaces the dropdown's value;
- `ra2ui::input::the_arena_list_scrolls_its_own_rows` — the list's `Rows 6`
  viewport hides rows 0–4 at offset 5, moves row 5 to the top, and moves the
  selection bar with its row;
- `ra2ui::arenas::the_available_list_is_filtered_by_the_scripts_game_types` —
  the counts and the box arena's own three types.

The windowed keys and the wheel are compiled and share `Session`/`Match` with
this run. **No windowed run is claimed here: this environment's display is left
to the capture agent.** The shared offscreen path is the evidence.

## 5. What is not wired

- **Player slots 2 and 4.** `exhib_robots.gib` has four slots; a two-robot
  match uses slot 1 (the player) and slot 3 (the computer's first robot). Slots
  2 and 4 are the same shipped pickers and would be the second robot per side
  in a four-robot Battle Royal; this client does not spawn them.
- **The match-time dropdown's option list.** The control's current value
  (`5 minutes`) is read and used, but the listbox under it is empty in the
  shipped file, so there is no data for the other choices. `--time` is the
  explicit replacement, and `--seconds` remains the headless run length.
- **The robot cell's picture.** `choosebot_cell.gib`'s `bot preview` image
  stays the shipped `exhibition/sample_bot.bmp`; the `.bot` file's 128×128 TGA
  snapshot is not decoded into the cell. The cell's `weight icon` is set from
  the bot's own `class`, and the picker's `robot name` is the `.bot`'s name.
- **The weight class option group.** The Game Settings tab's
  `weight class optiongroup` is not read; the picker offers every robot in the
  team's roster. The original's filtering by class is not implemented.
- **Hazard discovery.** The checkbox is read only when the arena fragment has
  been loaded. With no fragment, no hazard value is invented and the script's
  default is left in place.
- **A windowed run**, as above: no display run is claimed here.
