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
2. `arena button` → `exhibition/arena.gib` in `content group`; the list's note
   *"populate with arena names"* fills it from `ra2ui::arenas::all`;
3. click an arena row — the row is the list's own `Sel Bar` at `RowHeight` 18;
4. `hazard checkbox` toggles the arena's hazards (the checkbox's own note is
   *"enable or disable hazards for this arena"*);
5. `gametype button` → `exhibition/gametype.gib`; its
   `game type optiongroup` names the selected member;
6. `start game button` — its note is *"start the game"*, not a shell link.

The `start game button` click is intercepted before the shell's transition
logic has anything to do: the app reads the setup screen's choices and builds
the match. The screen stays `exhibition/exhib_setup.gib` underneath, so the
result screen's `Continue` and the pause dialog's `quit button` return to the
setup screen, not the main menu.

The choices:

| choice | source | fallback |
|---|---|---|
| arena | the selected row of `arena listbox`; the script is `Arenas/<script>.py` | `Arenas/box.py` when no row was selected |
| game type | `game type optiongroup`'s `Selection` (`deathmatch checkbox` → `DEATHMATCH`, …) | `DEATHMATCH`, printed as the default when the tab was not opened |
| hazards | the arena fragment's `hazard checkbox` | left at the script's default when the fragment is not loaded; no value is invented |
| player robot | the career/team table's team 0, robot 0 — `Teams/Team0/Bot0.bot` | fixed |
| opponent | the first shipped AI team's first robot — `AI/Team1/Bot0.bot` | fixed |

The shell remembers the arena and game-type choices when another content tab
replaces `content group`; selecting the arena row and then opening Game
Settings does not lose the arena.

## 2. The keys

| key | action |
|---|---|
| `W` / `Up` | throttle +1 |
| `S` / `Down` | throttle −1 |
| `A` / `Left` | steer −1 |
| `D` / `Right` | steer +1 |
| release | that axis centres (the input is sampled at the next fixed tick) |
| `Esc` | open/close the shipped `common/ingame_dialog.gib` pause popup |
| `C` | cycle the camera |

The match steps at its own fixed `MatchConfig::dt`; the window's 50 ms update
accumulates wall-clock time into those steps, so a key edge changes the value
sampled at the next physics step and never changes the step size.

`Match::step_with(inputs)` is the new entry point: `inputs[i] == Some(DriveInput)`
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

```sh
./engine/target/release/ra2-app --game local/game \
    --play --drive "0-2:1,0;2-4:1,1" --seconds 8 --out /tmp/ra2-play
```

`--play` uses the existing `ra2sim::DriveScript` grammar
(`<start>-<end>:<throttle>,<steer>`, fight-time seconds). `--seconds` is the
explicit match length (the same engine field `--match` uses), `--out` is
required, and `--arena PATH` selects a different picker row by script/name
(`Arenas/box.py` is the default). The run:

1. clicks `exhibition button`, `arena button`, the arena row and the hazard
   checkbox (off, for a deterministic box-arena frame);
2. writes `play-setup.png` from the shipped setup screen with the chosen row
   selected;
3. clicks `gametype button`, reads its selection, and clicks
   `start game button`;
4. drives robot 0 with the script, samples the drive at the match's fixed step,
   and writes `play-drive.png` after the fight has begun;
5. runs to `Results` (the same evidence decision `--match` uses at the explicit
   length: `plus.eliminatePlayer` on the robot with less material when neither
   chassis was destroyed) and writes `play-results.png`.

All three frames are the design size, **1280×1005** by default.

Printed (this run, `--seconds 8`):

```text
  click "exhibition button" -> exhibition/exhib_setup.gib [Button "exhibition button"] ... note "link to exhibition setup"
  click "arena button" -> exhibition/exhib_setup.gib [CheckBox "arena button"] ... content exhibition/arena.gib
  click 561.5 234.89 -> exhibition/exhib_setup.gib [ListBox "arena listbox"] ...
  click "hazard checkbox" -> exhibition/exhib_setup.gib [CheckBox "hazard checkbox"] ...
  click "gametype button" -> exhibition/exhib_setup.gib [CheckBox "gametype button"] ... content exhibition/gametype.gib
  click "start game button" -> exhibition/exhib_setup.gib [Button "start game button"] ...
exhibition: Combat Arena (box.py), game type DEATHMATCH, hazards off (hazard checkbox unchecked)
player robot: C.R.U.S.H. (robot 0, Teams/Team0/Bot0.bot) vs Roly Poly (AI/Team1/Bot0.bot)
robot selection: fixed in this client (the setup screen's robots tab is not selectable yet)
  Countdown -> Fight at 0.00s, points [0, 0], winners []
  Fight -> Over at 8.00s, points [40, 38], winners [0]
state Results, winner C.R.U.S.H., points [("C.R.U.S.H.", 40), ("Roly Poly", 38)]
player robot: C.R.U.S.H. (robot 0), start (-9.82, -1.73, 10.01),
              end (8.17, -1.84, -4.78), distance 23.29 m
```

The exact end position and points depend on the arena's physics; the run prints
its own numbers. The tests assert the invariants, not those floats:

- `ra2-app::play_mode::a_scripted_drive_moves_robot_zero_and_reaches_the_results`
  — the scripted drive moves robot 0 **at least 1 m net**, `Over` was observed,
  the final state is `Results`, and the three frames are **1280×1005**;
- `ra2-app::play_mode::the_start_button_starts_a_match_and_is_not_a_shell_link`
  — clicking `start game button` keeps the setup screen, carries no navigation
  note, and a real `Match` (two robots, player `Teams/Team0/Bot0.bot`,
  `DEATHMATCH`) can be built from the choices underneath.

The windowed keys are compiled and share `Session`/`Match` with this run. **No
windowed run is claimed here: this environment has no display.** The shared
offscreen path is the evidence.

## 5. What is not wired

- **Robot selection.** The setup screen's `robots button` loads
  `exhib_robots.gib`, but its `load appropriate gib for player N here` slots are
  not driven by this client. The player is always `Teams/Team0/Bot0.bot`.
- **Team selection.** The career model has the team table, and the HQ flow uses
  it; the exhibition flow does not ask which career team, so team 0 is the
  default.
- **The match-time dropdown.** The gametype fragment's `Match time: 5 minutes`
  display is not read or edited; a windowed exhibition match uses "No limit"
  (`MatchConfig::duration = None`), and `--play` uses `--seconds` explicitly.
- **Arena scrolling.** The picker shows six rows at a time and this client does
  not scroll the listbox; the default `Combat Arena` is the third visible row
  and reachable.
  `--play --arena` can only pick a visible row.
- **Game-type availability.** The original filters the arena picker by the
  arena's own `game_types`; this client does not, and it does not validate a
  chosen game type against the chosen arena.
- **Hazard discovery.** The checkbox is read only when the arena fragment has
  been loaded. With no fragment, no hazard value is invented and the script's
  default is left in place.
- **The windowed run itself**, as above: no display in this environment.
