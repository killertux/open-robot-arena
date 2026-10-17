# 18 — The Team HQ and the event flow (M10.3 + M10.4)

This is the screen half of M10: the career model (`engine/crates/ra2sim/src/career.rs`,
`docs/16-career.md`) drawn on the game's own Team HQ screens, and one event played
from entry to prize. The screens are composed by `ra2ui::hq` (the tabs, the grids
and their cell templates) and driven by `ra2-app::hq_mode`. No career value is
computed in the UI: the league points, the rank, the history counters, the roster
and the prize list all come from `Career`.

## Commands

```text
# the HQ in a window
cargo run -q --release -p ra2-app -- --game "$RA2_GAME" --hq

# headless: the roster, the league/events/history tabs, no event
cargo run -q --release -p ra2-app -- --game "$RA2_GAME" --hq --out /tmp/hq

# headless: enter schedule row N, play its whole event, award, save, reload
cargo run -q --release -p ra2-app -- --game "$RA2_GAME" \
    --hq --simulate-event 1 --out /tmp/hq --seconds 3

# headless: play all nine events with deterministic opponents and write the
# season evidence (season.png, bracket.png, round.png, champion.png)
cargo run -q --release -p ra2-app -- --game "$RA2_GAME" \
    --hq --simulate-season --out /tmp/season --seconds 3

# the same, with a damaged report for the repair bench (evidence knob, below)
cargo run -q --release -p ra2-app -- --game "$RA2_GAME" \
    --hq --simulate-event 1 --seconds 3 --report-damage 40 --out /tmp/hq
```

`N` is a **zero-based row of the nine-event season schedule**, not an
`Events.txt` id; the run prints both. The game directory is the usual one
(`--game`, `RA2_GAME`, or `local/game`); a career save writes
`Teams/Teams.txt`, `AI/Teams.txt` and `Teams/career.dat`, so a test or an
experiment should point `--game` at a copy. The repository's own tests make a
temporary root whose writable files are copies and whose UI/arenas/robots are
symlinks to the install.

## The flow on screen

| step | shipped screen | what fills it | career call |
|---|---|---|---|
| boot | `teamhq/teamhq.gib` | the team name, motto and logo; `robots button`'s fragment in `hq content group` | `Career::team_name`, `Career::player` |
| roster | `teamhq/hq_robots.gib` + `bot_cell.gib` | one cell per player `Robots:` slot: the `.bot`'s name and class | the player's `Team::robots` |
| league | `teamhq/hq_league.gib` + `league_cell.gib` | sixteen rows in rank order; `Season N`, `Events remaining: N` | `season_points`, `opponent_points`, `rank` |
| season results | `teamhq/season_results.gib` + `sresults_cell.gib` | the same standings, its own cell | the same |
| history | `teamhq/hq_history.gib` | the eight counters, the fastest win, the prize grid (most recent first) | the player's state fields, `Career::prizes` |
| events | `teamhq/hq_events.gib` + `event_cell.gib` (and the completed/in-progress/disabled cells) | one art cell per event on the nine-event schedule, with its state (`CHAMPION`/`PLAYED`, `round/rounds`, `OUT`) and the schedule title's in-progress line | `Career::schedule`, `Career::history`, `Career::bracket` |
| event entry | `teamhq/event_bracket.gib` | event art, title, description, `Round N`/`Final Match`, the bracket's rounds and next opponents, `Play Next Round!` | `Career::enter_event`, `EventBracket` |
| match | `gamewindow.gib` / `statusmeters.gib` over the 3D duel | M8's HUD | `ra2sim::match::Match` |
| results | `common/matchresults.gib` + `result_cell.gib` | the two bots, their points, the winner text | `MatchReport::results` |
| next round | `teamhq/event_bracket.gib` again | the next field and opponent; a between-rounds repairs bench when the match left damage | `Career::play_round`, `EventBracket::draw_round` |
| award | `teamhq/event_winner.gib` | team, robot, `rounds won/total`, event points, the prize text; the arena preview is composited behind the window | `Career::play_round` → `finish_event` |
| back | `teamhq/teamhq.gib` | the league and history, from the **reloaded** career | `Career::load` |

The event's arena is `Events.txt`'s `arena` line (matched against
`ra2ui::arenas::all`, the scripts' own `name`). Its game type is the arena
script's `game_types` list — `Combat Arena` → `DEATHMATCH`, `LumaZone` →
`KING OF THE HILL`, `FlexTop` → `TABLETOP` — not a number in `Events.txt`.
The player enters their first roster robot. The event is the measured
16-entrant bracket: `Events.txt`'s tier count is the number of rounds, its first
setting is 2 or 4 bots per match, the player is match 0 slot 0, and every other
slot is a deterministic draw from the fifteen AI teams (`docs/16-career.md`
§3a). A win advances and redraws the next field; a loss ends the event but not
the season; the trophy is stored only on the final win. `Play Next Round!`
starts the current round, so the same screen is shown again with the next
opponent until the event ends.

## ## The repair bench (`teamhq/repairs.gib`)

After the award, the run opens the shipped repairs window when the match left a
pending repair (the windowed flow also goes there on `continue button`); the
headless evidence run opens it even when empty so the `0:00` state is in the
frame set. It is the game's own screen (`FUN_004d5de0`) and the model is the
only source:

| the screen's widget | what fills it | career call |
|---|---|---|
| `repairs list` (`Rows 16, Columns 2`) | one row per pending repair: the damaged component in the `Component` column, remaining / total and the bar's percentage in the `Time` column | `Career::repairs`, `Career::repair_progress` |
| `repair time` | the longest remaining repair, `m:ss` (`0:30` for the policy's default) | `Career::repairs` |
| `repair button` (`click to repair the selected item`) | advance the selected repair's clock by one six-second tick; a full bar completes it | `Career::advance_repair` |
| `finished button` | leave the bench, reload the career and return to the HQ | `Career::load` |
| `repairs 3d placeholder` | the screen's own dark backing (the robot preview is not composed) | — |

**It is time, not a purchase.** `docs/16-career.md` §5: `FUN_004d6e10` advances
a progress bar and `FUN_0058b470`/`FUN_004d7010` complete it; no cost string or
subtraction path was recovered. The model's `Repair.cost` is 0 and
`Career::advance_repair` never touches `money`. `Repair.elapsed` is persisted in
`Teams/career.dat` (`repair` line, fifth field), so a partially served repair
survives a save; completing the bar removes the entry and the application saves
the career immediately, which is what "the damage is cleared" means here.

**Where the damage comes from.** `Career::repairs_from_report` runs in
`award_match`: one entry per robot on the player's team with `damage_taken > 0`
or `defeated`. The entry names the robot as its component: a `MatchReport` has
no component damage, and `Team+0x12c`'s component list is not decoded.

**The evidence knob.** The shipped match in this environment ends on the
evidence decision (`plus.eliminatePlayer`, the tabletop path) and reports
**zero damage** — the brains do not close on the opponent even in 20–60 s, so
no real report produces a row. `--report-damage N` adds `N` damage to the
player's `ReportRobot` after the match and before `award_match`; it is a
test/evidence input, not a game rule. The career then sees exactly what a
damaging match would hand it. The normal run without the flag shows the
screen's empty state.

### Evidence run (row 1, `--seconds 3`, `--report-damage 40`)

```text
fight ended at 3.02s, results screen is next
report damage: +40 on "C.R.U.S.H." (evidence knob; the match dealt 0)
  repairs: 1 entry (C.R.U.S.H.), clock 0:30
  repair complete: C.R.U.S.H.
  repairs done after 5 clock tick(s)
result: won true, rounds 1/1
reloaded: 80 points, rank 1, 1 prize(s)
```

Five six-second ticks fill the policy's 30-second bar; the fifth removes the
entry and saves. `return_to_hq` reloads the career, so the `hq-after.png` frame
is drawn from a save with no pending repair. Without `--report-damage`, the
same run prints `repairs: 0 entries (), clock 0:00` and both frames show the
empty list (`0:00`, no rows).

## Frames

`--hq --simulate-event N --out DIR` writes:

| frame | what it shows |
|---|---|
| `hq.png` | the HQ on the robots tab, the team's six robots by name and class |
| `hq-events.png` | the schedule, nine event art cells |
| `event.png` | the bracket for the entered event, with the arena preview behind it |
| `match.png` | the live HUD at the first fight's midpoint |
| `results.png` | `common/matchresults.gib` filled from the match report |
| `award.png` | `event_winner.gib`: team, robot, rounds, points and the prize text |
| `repairs-list.png` | `teamhq/repairs.gib`: the damaged robot's row, `0:30 / 0:30 (0%)`, the clock at `0:30` |
| `repairs-done.png` | the same screen after five ticks: empty list, `0:00` |
| `hq-after.png` | the league after the award, read back from the save |
| `hq-history.png` | the history after the award, with the stored trophy art |

`--hq --simulate-season --out DIR` writes the season-level set:

| frame | what it shows |
|---|---|
| `season.png` | the events tab **while the first event is in progress**: the nine cells, the in-progress cell's `round/rounds`, the title's `Event 1/9: Round N of M — next: …` |
| `bracket.png` | the first event's bracket: every round, the player's opponents and `NEXT` |
| `round.png` | a later round of the first event: the completed round's `WON` line and the new opponent |
| `champion.png` | the season's last screen: `golden_sawblade.gib` when rank 1, otherwise `season_results.gib` |
| `season-final.png` | the events tab after the ninth event, every cell `CHAMPION` or `PLAYED` |
| `hq.png` | the HQ at the start, before the season |

Without `--simulate-event`/`--simulate-season`, `--hq --out DIR` writes `hq.png`,
`hq-league.png`, `hq-events.png` and `hq-history.png`.

### The evidence run (row 0, `--seconds 0.2`)

The event's `Events.txt` shape decides the rounds. Row 0 in the seed-7 schedule
is `Robo-Depot`, a 2-round, 4-bot event, so it pays `160/2 = 80` per round:

```text
round 1: vs The Good ol' Boys, The Scrappers, North Polers — won; +80 event points (80 + 0)
round 2: vs Team Z, The Good ol' Boys, The Scrappers — won; +80 event points (80 + 0)
award: 160 event points, won true, rounds 2/0
reloaded: 160 points, rank 5, 1 prize(s)
```

`--simulate-event` now plays the **whole event** (all rounds), not one match.
The points are the model's: `play_round` adds `160 / rounds` participation and
`robot points / 200` per round; `finish_event` folds the event total into the
season and stores the event's trophy only on the final win. The print's `rank`
is `Career::rank` (the number of AI teams strictly ahead, plus one), not a UI
table.

### The season evidence (`--simulate-season --seconds 0.2`)

The seed-7 calendar, with the real match engine and the model's bracket:

```text
row 0  Robo-Depot                 2 rounds, 4 bots   won final   +160  trophy
row 1  Strategic Engineering      4 rounds, 2 bots   won final   +160  trophy
row 2  Biggie Bot Mash            2 rounds, 4 bots   won final   +160  trophy
row 3  BBA Midwest Event          4 rounds, 2 bots   lost round 3, eliminated  +120
row 4  Hot Dog Harry's Rumble     2 rounds, 4 bots   won final   +160  trophy
row 5  Mortal Robot               4 rounds, 2 bots   won final   +160  trophy
row 6  Robot Scrapyard TV         4 rounds, 2 bots   won final   +160  trophy
row 7  F.O.R.C.E. Tournament      4 rounds, 2 bots   won final   +160  trophy
row 8  Powerbolt Championship     4 rounds, 2 bots   won final   +160  trophy
season 0 complete: 1400 points, rank 1, 9 prize(s), 9 event(s) played
```

Nine prizes = eight event trophies plus `UI\teamhq\golden_sawblade.tga`; the
lost event stored none (its tier message is on the award screen only). The
round lines in `bracket.png` are `Round 1: You vs … — NEXT` and
`Final Match: You vs … — not drawn`; `round.png` is the next round after a
win, and `champion.png` is the golden sawblade because the final rank is 1.
The same deterministic draw is what `replaying_the_same_seed_reproduces_the_bracket`
asserts.

## Tests

- `ra2ui::hq::tests::the_tabs_are_the_screens_their_notes_name` — re-reads the
  four buttons' notes.
- `the_robots_tab_is_filled_from_the_roster` — `bot_cell.gib` into `robot grid`.
- `the_league_grid_is_the_careers_standings` — the 16 rows, the player's 42
  points, `Season 3`, `Events remaining: 7`, and the content area is not clipped
  to one row.
- `an_event_cell_click_is_the_event_on_the_schedule` — cell geometry → event id.
- `the_history_tab_shows_the_counters_and_the_prizes` — rank/wins/events won,
  the `N/A` fastest win, and the two prize arts in most-recent-first order.
- `the_season_results_screen_is_the_same_standings`.
- `ra2-app::hq_mode::tests::a_click_on_the_event_row_enters_that_event` — the
  app-level click and `Career::enter_event` agree with the schedule row.
- `a_simulated_event_saves_and_the_next_app_run_reads_the_same_record` — a real
  `Match` through the flow, the save, a second `Session::open`, the points,
  prize, roster and the game's own `Teams.txt`.
- `a_headless_hq_run_writes_the_named_frames` — the nine PNGs at the requested
  size, including `repairs-list.png` and `repairs-done.png`.
- `a_headless_season_run_completes_and_writes_the_season_frames` — nine events
  through the real match engine, the career ends `Complete`, and
  `season.png`, `bracket.png`, `round.png`, `champion.png` are PNGs at the
  requested size.
- `the_hq_view_carries_the_bracket_shape_and_the_next_opponent` — the view's
  round count is the event's tiers, the first round has `16 / bots` matches and
  the next opponent is one of the AI teams.
- `ra2ui::hq::tests::the_bracket_group_is_filled_from_the_view` — the shipped
  `bracket group` gets the status line, one line per round and the next
  opponent.
- `ra2sim::career::tests::every_shipped_event_is_a_sixteen_entrant_bracket`,
  `a_loss_eliminates_from_the_event_but_not_the_season`,
  `replaying_the_same_seed_reproduces_the_bracket` and
  `save_reload_mid_season_restores_the_bracket_and_opponents` — the bracket
  rules, the elimination rule, the deterministic replay and the mid-season save.
- `damage_appears_on_the_repairs_screen_and_completing_it_persists` — a damaged
  report → one row (`My Robot`, `0:30`), five six-second ticks → `0:00`, the
  save/reload is empty, money stays 0, and a healthy career opens the empty
  list.
- `ra2ui::repairs::tests::the_list_is_the_careers_rows_and_the_empty_state_is_empty`
  and `the_repair_button_and_a_row_are_hit_tested_by_the_shipped_widgets` — the
  list/time fills, the empty state, the button/row hit test.
- `ra2sim::career::tests::a_damaging_match_leaves_a_repair_the_clock_completes_and_saves`
  — 40% elapsed round-trips, the completing tick clears the entry, no charge.
- `the_event_arena_and_game_type_come_from_the_data` — Combat Arena →
  Deathmatch, FlexTop → Tabletop, LumaZone → King of the Hill.

## What is not proven

1. **The bracket's visual columns.** The model has the real rounds, the field
   and the next opponent; `event_bracket.gib`'s `bracket group` is filled with
   one labeled line per round. The original's `FUN_0046ad60` draws a column per
   round with bot logos and `questionmark.bmp` through a custom control
   (`FUN_005f6010`) that was not decoded, so the port's arrangement is its own
   while the data is the game's.
2. **The draw's RNG algorithm and seed.** The original calls `FUN_00419190`
   with 15; its algorithm and seed are not recovered. The career keeps an
   explicit seed (`hq_mode::SEED` = 7, sidecar `seed`) and draws every
   non-player slot with a small deterministic PRNG. The *rule* (0…14, per
   slot, per round, no de-duplication) is measured; the numbers are not the
   original's.
3. **The two-winner variant.** `FUN_004698f0` contains a two-advancing-entrants
   case; no shipped event uses it. The port implements single elimination
   (one winner per match).
4. **The exact `Events.txt` → bracket-config writer.** The data correlation is
   total (2 ↔ 4 rounds, 4 ↔ 2 rounds, `bots ^ rounds == 16`); the code path that
   copies the setting into the team record's six bracket ints was not traced.
   The model prefers the shipped setting and has a tier-derived fallback.
5. **The event's game type.** `Events.txt` does not name one. The port reads the
   arena script's `game_types` and picks Deathmatch first when the arena offers
   several; the original's event setup may have carried a different selection.
6. **The match length.** The event setup window's time dropdown was not read; the
   length is the explicit `--seconds` (30 by default), and the evidence run
   prints its own decision at that limit. That decision is the `--match`
   evidence script (`plus.eliminatePlayer`, the tabletop path), not a rule.
7. **Robot snapshots.** A `.bot` stores a raw 128×128 TGA blob, not a file name
   the UI can resolve. The inventory draws the shipped `teamhq/nobot.bmp`
   silhouette in the `bot snapshot img` cell; the robot's own picture is unshown.
   This is a screen whose source has no model field.
8. **Prize art and the trophy scene.** The stored prize's `art` is a `.tga`
   (`UI\teamhq\events\trophies\…`) and is drawn in the history grid; the trophy
   `model` is a `.gmf` in `Arenas\event_results`, and the `event_results`
   3D scene (`Arenas/event_results.py`) is not composed. The award is the
   shipped `event_winner.gib` over the event arena's preview.
9. **The league's competition level.** `hq_league.gib`'s `competition level`
   note names `AMATEUR`/`SEMI-PROFESSIONAL`/`PROFESSIONAL`, but the career model
   has no level field; the screen keeps its own `LEAGUE STANDINGS` label.
10. **Rank on a fresh season.** `Career::rank()` counts AI teams strictly ahead;
   with all points zero every team ties and the player is rank 1, while the
   shipped team record carries `16`. The screens show the model's rank; the
   shipped 16 is not recomputed.
11. **The schedule draw.** The original's RNG and seed were not recovered
   (`docs/16-career.md` §3). The career uses seed 7 (`hq_mode::SEED`); the
   schedule is the nine distinct events that seed draws.
12. **`hq_history`'s prize grid has no cell template.** The grid's note says
    "prize bitmaps … fixed at 80×80", but no `.gib` cell ships. The port adds an
    `Image` per prize at the grid's own 82×82 cell geometry. The note also says
    "only show last 32"; the career's `max_prizes` is 20 and the grid's visible
    area is 4×5, so the model's list is what is drawn.
13. **The award's prize text clips.** The `prize description` `DisplayBox` is
    authored at a 211×64 margin. Long tournament messages wrap past it and the
    screen shows the visible part; the complete description is in the run's
    print and in the history's stored prize.
14. **The event description clips.** `event_bracket.gib`'s description
    `DisplayBox` is authored at 400×50; long `Events.txt` descriptions wrap past
    it. The full text is in `Events.txt`.
15. **The repair screen's component-level damage.** A `MatchReport` has no
    component list, so a repair row names the robot; the `Team+0x12c` decoder
    and the `damage.tga` fractured icon are not implemented. The screen's
    `repairs 3d placeholder` is left as the screen's own dark backing, not the
    robot preview.
16. **A live damaging match.** The shipped match in this environment reports
    zero damage (the evidence knob above), so the repair flow is proven with a
    `MatchReport` — the career's own input — rather than a match that closed on
    damage. The repair time unit is the model's explicit 30 s; the original
    passes `0x1e` to the progress widget but its unit was not established.
17. **The bench's canvas.** `repairs.gib` is authored at 800×600 and its bench
    is drawn at that size on the 1280×1005 shell; the arena backdrop does not
    stretch to the rest of the target, so the lower part of `repairs-list.png`
    is the clear colour. The screen's own art and widgets are unchanged.
18. **The windowed `--hq` loop is not exercised here.** This environment has no
    display, so `run_hq` shares the same `Session` and frame composition as the
    headless run but was not opened in a window; the PNGs are the evidence. The
    `--simulate-event` flow is the headless path and requires `--out`.
