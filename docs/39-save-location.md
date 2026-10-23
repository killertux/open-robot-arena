# 39 - Save location

The install (`--game`) is read-only data. Everything the player changes goes
to a per-user save directory, `ra2fmt::saves` (one module, `engine/crates/ra2fmt/src/saves.rs`).

## Where

In order of precedence:

1. a binding made in code with `saves::bind(install, save)` (tests, scratch
   runs; `bind(dir, dir)` gives the old write-in-place behaviour),
2. `--save-dir DIR` on `ra2-app`,
3. the `ORA_SAVE_DIR` environment variable,
4. the platform default: `$XDG_DATA_HOME/open-robot-arena` or
   `~/.local/share/open-robot-arena` (Linux), `%APPDATA%\open-robot-arena`
   (Windows), `~/Library/Application Support/open-robot-arena` (macOS).

`RA2_CONFIG` still overrides the settings file alone.

## Overlay

* Reads (`saves::resolve`, `saves::list`): the save copy when there is one,
  otherwise the install's file.
* Writes (`saves::prepare_write`, `saves::write`, `saves::copy`): only the save
  directory, at the same relative path, in the file formats the install uses
  (`Teams/Teams.txt`, `AI/Teams.txt`, `Teams/career*.dat`,
  `Teams/Team<n>/Bot<m>.bot`, `Robot Designs/*.bot`, `Custom Textures/*`,
  `ra2.cfg`). A user can copy these files between the save directory and the
  original game's folder.
* Deletion (`saves::remove`): the save copy is removed; a file that exists
  only in the install stays untouched and is hidden by a line in
  `deleted.txt` at the save directory's root (`/`-separated relative paths, one
  per line). Writing the file again lifts the line. `deleted.txt` is ours and
  not part of the original's layout.

What the original does on delete (`FUN_004ec110`, the Delete Team Yes): it
clears the team's record in `Teams/Teams.txt` and writes the table; the team's
robot files stay on disk, merely unlisted. The port does the same, so Delete
Team writes a new `Teams.txt` into the save directory and the install's
file is shadowed; only the client's own `career<slot>.dat` sidecar is removed
(tombstoned if it exists only in the install). The inventory's Scrap Bot (`FUN_004ff0c0`, docs/36 9.24) does the same for a robot: it removes the slot from the record's `Robots:` line and
never deletes the `.bot`, so the port has no robot-file deletion either; `saves::remove` stays the path one would use.

## Write paths rerouted

| Was | Now |
| --- | --- |
| `ra2sim::career::Career::save` (`Teams/Teams.txt`, `AI/Teams.txt`, `Teams/career*.dat`) | `saves::prepare_write` / `saves::write` |
| `ra2ui::teams::create_team`, `update_team`, `add_robot`, `delete_team` (table, `Teams/Team<n>/`, sidecar delete) | `write_table` through `prepare_write`, `redirect`, `saves::remove` |
| `ra2-app::build_mode` `save` (`Robot Designs/<name>.bot`), `save_to_team` (`Teams/Team<n>/Bot<m>.bot`), `export_bot`, `export_texture` (`Custom Textures/`) | `prepare_write`, `saves::copy`, `saves::write`; lists through `saves::list` |
| `ra2-app::options` (`ra2.cfg`) | `saves::target` (was `$XDG_CONFIG_HOME/open-robot-arena/ra2.cfg`) |

Reads that moved to the overlay: the two team tables (`career`, `ra2ui::teams`,
`ra2ui::standings`, `ra2sim::match`), the sidecars, every `.bot` path
(`bot_path`, `player_robot_path`, `ai_robot_path`), `Robot Designs/` and
`Custom Textures/` listings and opens, the logo list. Output of tools
(`--out` screenshots, WAVs, CLI dumps) is not a save and is unchanged.

## Tests

Tests that save run on a scratch install and bind it to itself
(`saves::bind(dir, dir)`), as before. `ra2-app/tests/saves.rs` copies an
install, strips write permission from the whole copy, runs a career save,
team create/update/delete and a robot save against it with a separate save
directory, and checks that every file in the install copy is byte- and
mtime-identical.

## Per-match script queues

`ra2script::prepare_match` now also clears the engine `requests`
(damage/force/zap) and the `deferred` list, besides the camera, emitter and
zone state it already reset; `ScriptHost::boot` clears the joint requests
(the flaky-spikes fix).

## Tests and the save directory

Tests that open the real install call `ra2fmt::saves::isolate_for_tests()` (from their `game_dir()`/`ui_root()` helper): the process then has an empty save
directory of its own (`$TMP/ora-test-save-<pid>`, never created unless written), ahead of `ORA_SAVE_DIR` and the platform default, behind a binding or
`--save-dir`. So a test result never depends on the user's `~/.local/share/open-robot-arena`, and the gate's check that this directory is never created
is unchanged.
