# 23 — Robot previews and the four-player modes

This is the pass that closes two visible gaps: the robot picture the `.bot` file
already carries, and the exhibition screen's four robot slots (BATTLE ROYAL and
2v2 TEAM MATCH). Both use the game's own data; nothing is re-rendered and no
file is written into the game directory.

## 1. The picture path

A `.bot` file stores its snapshot as a raw uncompressed TGA blob
(`ra2fmt::bot::Bot::snapshot`, normally 128×128). It is not a file name, so no
`.gib` can resolve it on its own. The port now:

1. decodes the blob with `ra2ui::preview::Preview::decode_tga` (the `image`
   crate's TGA reader) into RGBA8, top row first;
2. names it with `Preview::key(label)`, a content-addressed
   `#bot-preview:<label>-<hash>` key (`#` keeps it out of the game's file
   namespace);
3. registers the pixels in the renderer's `UiAssets::add_preview`, which puts a
   `UiTexture` in the scene — **in memory, with no temp file**;
4. writes the key into the cell's `_Filename`: `result_cell.gib`'s
   `bot preview` for match results, `bot_cell.gib`'s `bot snapshot img` for the
   HQ inventory.

`ra2sim::match::Match` decodes each entry's snapshot once when the match is
built (`Match::previews`); `MatchReport::results()` carries the `Preview` beside
the synthetic key. `ra2-app::hq_mode::build_view` decodes the career roster's
`.bot` files the same way. A `.bot` with no snapshot keeps the shipped
placeholder: `exhibition/sample_bot.bmp` in a result cell, `teamhq/nobot.bmp`
in the inventory. The `.bot` format and its writer are untouched; the picker's
own `choosebot_cell.gib` still shows `sample_bot.bmp` (see §5).

The offscreen and windowed paths share `UiAssets`/`UiScene` and the same
`Session`, so the window draws the same registered textures.

## 2. The four slots

`exhibition/exhib_robots.gib` has four slot groups, each with its own note:

| slot | group | note |
|---|---|---|
| 1 | `player1 group` | `load appropriate gib for player 1 here` |
| 2 | `player2 group` | `load appropriate player 2 gib` |
| 3 | `player3 group` | `load appropriate gib for player 3 here` |
| 4 | `player4 group` | `load appropriate player 4 gib` |

The screen nests slots 1+2 in `team 1 group` (the player's side) and slots 3+4
in `team 2 group` (the computer's side). Both spellings of the note open the
same shipped pickers (`selectbot_player.gib` for 1/2,
`selectbot_computer.gib` for 3/4); `ra2-shell` keeps one `Pick` per slot and
`ra2-app::play_mode::Choice::match_entries` turns them into the match:

| game type | robots | team ids |
|---|---|---|
| `BATTLE ROYAL` | slots 1, 2, 3, 4 | `[0, 1, 2, 3]` (free-for-all) |
| `TEAM MATCH` | slots 1+2 vs 3+4 | `[0, 0, 1, 1]` (the screen's own side pairs) |
| `DEATHMATCH` / `TABLETOP` / `KING OF THE HILL` | slot 1 vs slot 3 | `[0, 1]` |

Slot 1 is entry 0 in every case, so it is match robot 0, the robot the player
drives, and the chase camera's subject. `MatchConfig::bot_count` is 4 for a
Battle Royal (`EventNumBots` accepts 2 or 4) and `Match::new` puts the robots on
the arena model's four start points in order. The default slot 2/4 picks are
the next robot on slot 1/3's own `Robots:` line, so the default Battle Royal is
four different `.bot` files.

## 3. Commands and frames

The four evidence frames are 1280×1005 (the shell's design size):

```sh
cd /path/to/open-robot-arena
GAME=local/game
BIN=engine/target/release/ra2-app

# The two result-cell pictures and the two-mode frames.
$BIN --game "$GAME" --play --drive "0-2:1,0;2-4:1,1" --seconds 8 \
    --out /tmp/ra2-preview-results
cp /tmp/ra2-preview-results/play-results.png local/evidence/preview-results.png

$BIN --game "$GAME" --hq --out /tmp/ra2-preview-inventory
cp /tmp/ra2-preview-inventory/hq.png local/evidence/preview-inventory.png

$BIN --game "$GAME" --play --gametype battle-royal \
    --drive "0-2:1,0" --seconds 4 --out /tmp/ra2-battle-royal
cp /tmp/ra2-battle-royal/play-results.png local/evidence/battle-royal-4.png

$BIN --game "$GAME" --play --gametype team-match \
    --drive "0-2:1,0" --seconds 4 --out /tmp/ra2-team-match
cp /tmp/ra2-team-match/play-results.png local/evidence/team-match-2v2.png
```

The run prints the four slots and the entries:

```text
slot 1: player team The Rookies (player slot 0), robot 0: C.R.U.S.H. (…/Teams/Team0/Bot0.bot)
slot 2: player team The Rookies (player slot 0), robot 1: Sting Ray (…/Teams/Team0/Bot1.bot)
slot 3: computer team PREHISTORIC (computer slot 1), robot 0: Roly Poly (…/AI/Team1/Bot0.bot)
slot 4: computer team PREHISTORIC (computer slot 1), robot 1: SaberTooth (…/AI/Team1/Bot1.bot)
match entries: 4 robot(s), teams [0, 1, 2, 3]
  arena start points: 4
results screen: 4 of 4 bot(s) placed
```

The TEAM MATCH run reads `match entries: 4 robot(s), teams [0, 0, 1, 1]`; the
result slots show the same team name for 1+2 and for 3+4.

## 4. Tests

- `ra2ui::preview::tests::a_shipped_bots_snapshot_decodes_to_its_stored_dimensions`
  — `AI/Team1/Bot0.bot`'s TGA is 128×128 and not white.
- `ra2ui::preview::tests::a_preview_key_is_content_addressed` — the same
  picture has one key; different pictures and different labels differ.
- `ra2ui::results::tests::the_four_result_slots_can_carry_four_distinct_previews`
  — four slots bind four distinct `#bot-preview:result-N-…` keys and four
  decoded pictures.
- `ra2-app::exhibition::a_battle_royal_carries_four_slots_four_spawns_and_four_result_rows`
  — four pickers, four distinct `.bot` paths, four start points at least 1 m
  apart, teams `[0,1,2,3]`, slot 1 driven, and the results tree's four names.
- `ra2-app::exhibition::a_two_v_two_team_match_uses_the_screens_side_pairs_and_ignores_teammate_hits`
  — teams `[0,0,1,1]`, a teammate hit scores 0, an opponent hit scores its
  damage.
- `ra2-app::play::a_battle_royal_writes_four_result_slots_with_the_bots_pictures`
  — the written 1280×1005 results PNG's four preview regions match the decoded
  snapshots' own texels.
- `ra2-app::play::a_team_match_runs_two_versus_two_and_writes_its_results` —
  the 2v2 run and its four-row frame.
- `ra2-app::hq_mode::tests::the_inventory_cells_draw_the_bots_own_snapshots`
  — the written `hq.png`'s inventory strips match the decoded snapshots.

## 5. What the data did not support

- **The picker cell's picture.** `choosebot_cell.gib`'s `bot preview` remains the
  shipped `exhibition/sample_bot.bmp`. The task's picture path is the result cell
  and the inventory; the picker's cell was not changed.
- **A per-slot headless flag.** The window can pick any of the four slots; the
  CLI's `--team/--bot/--opponent-team/--opponent-bot` still name slot 1 and slot
  3 only, and slots 2/4 use their roster's next robot.
- **The match-time dropdown's option list.** The shipped `time dropdown` shows
  `5 minutes`; the listbox under it is empty, so there are no other values to
  select. `--time` replaces the displayed value and `--seconds` is the run
  length.
- **The weight-class filter.** The Game Settings `weight class optiongroup` is
  not read; the picker still offers every robot in a roster.
- **The windowed run.** This environment has no display. The window loop shares
  `Session`, `Match` and `UiAssets` with the offscreen frames; the PNGs above
  are the evidence.
- **The four-player physics rebuild.** A four-robot spawn can print
  `ra2sim: rebuild failed: physics body: set_shape: UsedByConstraint(...)` from
  the duel's existing rebuild path; the match still runs. That path is another
  worktree's physics recon and was not changed here.
