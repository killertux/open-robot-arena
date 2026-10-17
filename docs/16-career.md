# 14 — The career: teams, a season, points, prizes and repairs

This is the M10.1 recon and the M10.2/M10.5/M10.6/M10.7 model. Every claim
below is tied to an address in `Robot Arena 2.exe` (SHA-256
`bc0f6833ece8c47e8625e5b12940d2509540cb393566577504b50b2318351a4f`, PE32,
x86) or to a shipped data file. The model is `engine/crates/ra2sim/src/career.rs`;
the record accessors are `engine/crates/ra2fmt/src/team.rs`.

**One correction up front.** Earlier notes (`docs/11-match-rules.md` §4.3 and
the `Team::money` alias) called `Team+0xb8` the team's money. It is not: the
team-HQ screen formats it as a time and shows `N/A` when it is `10000.0`
(`FUN_004fe2d0`, the comparison at `0x4fe3xx`, and the string at `0x6f0cec`,
`"N/A"`). The shipped records carry `10000` there because they have no fastest
win yet. No cash field and no cash deduction were recovered (see §6).

---

## 1. The team record

`Teams/Teams.txt` is written by `Team::save` (`FUN_004fb010`) and read by
`Team::load` (`FUN_004fabf0`); the team manager calls them from
`FUN_005024e0` (save) and `FUN_00502150` (load). A record is 23 lines (24 with
the AI file's `index N` head); when `+0xf4` is true, nine history lines are
inserted between the `+0xf4` line and the schedule line.

| Line / field | Offset | Meaning | Evidence | Status |
|---|---|---|---|---|
| 1–3 | — | name, motto, logo | `FUN_0047d0a0`, `FUN_00472bf0`, `FUN_00494a10` | established |
| 4 | — | `Robots: 0 1 2 …` | `FUN_004fa3b0` loop (`0x4fb010`) | established |
| 5 | `+0xc0` | `Robot In Event:` (`-1` = none) | `0x4fb…`; cleared by `FUN_004faaa0` (`+0xc0 = -1`) | established |
| 6–13 | `+0x98`…`+0xb4` | eight career ints, see §2 | eight `FUN_00450db0` reads at `0x4fad33`–`0x4fada3` | established |
| 14 | `+0xb8` | float, **fastest win**, `10000` = `N/A` | `0x4fadb1`; `FUN_0046c5f0` keeps the minimum; `FUN_004fe2d0` shows `N/A` at `10000` | established (money disproven) |
| 15 | `+0xf6` | bool; true in every shipped record; meaning not recovered | `0x4fadb1`–`0x4fadc…` | unknown |
| 16 | `+0x100` vector | prize model paths: `count path path …` | `FUN_005069f0` (`>4` size), save loop `0x4fb…`; `FUN_004fab50` | established shape |
| 17 | `+0xf4` | bool: the nine history lines follow | `FUN_004fb460`; `FUN_0046cdd0(1)` sets it when the schedule is created | established |
| 18–26 | — | nine history ints, one per line (only when `+0xf4`) | save `FUN_004fb460` branch; load `0x4fad…` reads exactly 9 | established shape, meaning unknown |
| 27 | `+0xe4` vector | event schedule: `count event-id …` | `FUN_00500110` returns `this+0xe4`; `FUN_00500120` appends ids | established |
| 28 | `+0xbc` | current event id (`-1` = none) | `FUN_00450db0` at `0x4fad…`; reset `FUN_004faaa0` | established |
| 29 | `+0x5c`…`+0x94` | fifteen ints: the AI teams' league points | save loop `0x4fb387`–`0x4fb3ab`; load loop `0x4faeff`–`0x4faf17`; readers in §2 | established |
| 30 | `+0x54` | points earned in the current event | save `0x4fb3b9`; load `0x4faf19`; writer `FUN_0046c5c0` | established |
| 31 | `+0x58` | the player's league/season points | save `0x4fb3d0`; load `0x4faf24`; writer `FUN_0046cd00`; reader `FUN_00500230` | established |
| 32 | `+0x50` | accumulated defeated robots (label ours) | save `0x4fb…`; writer `FUN_0046c5b0` (adds the call's count) | writer established, label unknown |

The 18–26 numbering above is for a played season; the shipped files are fresh
(`+0xf4 = false`) and the nine lines are absent, which is why each AI record
is 24 lines including its `index N` head.

### The fifteen ints — the table that was M10.1's question

The league screen's fill, `FUN_00500230` (called by the Team HQ refresh
`FUN_004fe0c0`), builds sixteen cells:

* the player's cell at `0x500392`–`0x5003a2` reads `Team+0x58` through
  `FUN_0046e5e0` and puts it in the points field (`FUN_004fd2d0`);
* the loop at `0x500403`–`0x50040c` reads `Team+0x5c + i*4` through
  `FUN_0046e5f0(i)` for each of the fifteen AI teams (`i = 0…14`), in the
  order the manager loaded `AI/Teams.txt`;
* the comparison at `0x50041e`–`0x500422` increments the rank counter for each
  AI team whose value is greater than the player's.

So the fifteen ints are **the fifteen AI teams' league points**, in AI-table
order, and `+0x58` is the player's. At season end `FUN_0046e2b0` reads the same
pair: if any AI value is greater than the player's, it opens the season-results
window; if none is, it opens the golden-sawblade champion window
(`0x46e2f5`–`0x46e31c`, `0x46e41b`, `0x46e31e`).

The writer is `FUN_0046c400`, called at the end of every match round. It adds
`robot points / 200 + 160 / robots` to `+0x54` for the player and
`160 / robots` to the fifteen entries (`0x46c439`–`0x46c475` for the player,
`0x46c534`–`0x46c545` for the loop). The constant is `0xa0` = 160 and the
divisor 200 (`0x51eb851f` magic). That is the only established score/points
conversion outside a match. Inside a match, damage becomes points in every
type except King of the Hill (`FUN_004175b0`'s not-KOTH branch;
`docs/19-match-fidelity.md` §1); KOTH's points are its script's
(`king.py`'s `plus.addPoints`).

At the end of the **event**, `FUN_0046ca90` folds the event points into the
season (`FUN_0046cd00` → `+0x58 += +0x54`), counts the event (`FUN_0046cd10`
→ `+0xa4 += 1`), counts an event win (`FUN_0046cd30` → `+0xb0 += 1`), stores the
trophy (`FUN_004fab50`) and clears the current event/robot (`FUN_004faaa0`).

---

## 2. The eight ints at `+0x98`…`+0xb4`

The team-HQ history screen binds these widgets in `FUN_004fd380`
(`0x4fd380`–`0x4fe02c`) and formats them in `FUN_004fe2d0`. Reading the
getters in that function (each is called with `ECX = Team*` from
`[TeamHQ+0x1ec]`) gives:

| Offset | Field | Evidence |
|---|---|---|
| `+0x98` | season index (zero-based; the title shows `+1`) | `FUN_004fe2c0`; title `"Event Schedule - Season "` at `0x6feff4`, `FUN_004fe1e0` |
| `+0x9c` | current league rank | `FUN_004145f0`; shipped value 16 |
| `+0xa0` | best rank | `FUN_0049fa50`; shipped value 16 |
| `+0xa4` | total events played | `FUN_004fe730`; writer `FUN_0046cd10(1)` |
| `+0xa8` | "wins" on the history screen | `FUN_004fe740`; `FUN_0046c5f0`'s `FUN_0046c6c0` adds 1 |
| `+0xac` | "losses" on the history screen | `FUN_004fe750`; `FUN_0046c6e0` adds 1 |
| `+0xb0` | events won | `FUN_004fe760`; writer `FUN_0046cd30(1)` |
| `+0xb4` | highest robot score | `FUN_0046c6a0`; `FUN_0046c5f0` keeps the maximum |
| `+0xb8` | fastest win (float) | `FUN_0046c6b0`; keeps the minimum; `N/A` at 10000 |

The first eight are integers; `+0xb8` is the float. The shipped values
(`0, 16, 16, 0, 0, 0, 0, 0, 10000`) are a fresh team: season 0 (shown as
"Season 1"), rank 16, best rank 16, no history, no fastest win.

**What is not established:** `FUN_0046c5f0`'s increment is keyed on the match
**round index** (`if round == 0`, `0x46c62a`–`0x46c641`), while the history
labels are "wins"/"losses". The labels come from the widget bindings; the exact
mapping between the increment condition and those labels is not proven. The
career model therefore counts a win/loss per match report and says so.

`+0x110` (not in the file) is the current **round index** of the event: the
getter is `FUN_004faae0`, `FUN_0046c240` shows `"Final Match"` when it equals
the bracket's last index and `"Round N"` otherwise.

---

## 3. A season

* **Nine events.** `FUN_00500120` fills the schedule vector
  (`Team+0xe4`) in a loop of exactly nine (`iVar5 = 9`), drawing distinct
  entries from the event list. `FUN_004ffcc0` lays the schedule out as a
  **3×3 grid** (`iVar5 < 3`, `iVar6 < 3`); when more than eight cells are
  complete (`8 < local_98`) it opens the event bracket. `FUN_0046e2b0` compares
  the completed count with 9: below 9 it returns to Team HQ, at 9 it shows the
  season results or the champion screen.
* **Entry is individual.** `FUN_00500c40` (the Join Event button) shows the
  message *"Each team may enter one event at a time. Finish your current event
  before beginning a new one."* (`0x6ff2f8`). A team has at most one
  `Robot In Event` (`+0xc0`) and one `+0xbc` event id; `FUN_004faaa0` clears
  both at the end.
* **The calendar is drawn at the start of a season**, not shipped. A fresh
  record has `+0xf4 = false` and an empty schedule; `FUN_004ffcc0` sees that,
  calls `FUN_00500120` (nine random distinct events), and `FUN_0046cdd0(1)`
  sets `+0xf4 = true`. `Events.txt` supplies the 25 candidates (11 events with
  2 tiers, 14 with 4; `ra2fmt::event` parses them).
* **How a season ends.** After the ninth event result, `FUN_0046e2b0`
  (Continue) compares the player's `+0x58` with each AI team's entry. If any AI
  is ahead, `SeasonResultsWindow` (`0x4e8630`, `season_results.gib`) shows the
  final table; if not, `GoldenSawbladeWindow` (`0x4e83c0`,
  `golden_sawblade.gib`) shows the champion and `FUN_004e84b0` stores
  `teamhq\golden_sawblade.tga` as a prize.

**Not established:** the RNG (`FUN_00419190`) and its seed, whether the draw
is filtered by season or tier, and the writer that advances `+0x98`/resets the
league for the next season. The model exposes `Career::next_season(seed)` as
explicit behaviour rather than pretending to reproduce an unread path.

---

## 4. Points, prizes and money

### Points

* `Team+0x54` — the current event's points; `Team+0x58` — the season's.
* A match adds `entered robot points / 200 + 160 / robots` for the player and
  `160 / robots` for each other participant (`FUN_0046c400`; see §1). The
  entered robot's `points` are its `+0x20` score: King of the Hill's script
  score (`plus.addPoints`) or, in every other type, accumulated effective
  damage (`FUN_004175b0`'s not-KOTH branch;
  `docs/11-match-rules.md` §4.1 and `docs/19-match-fidelity.md` §1).
* `Team+0x50` accumulates the number of defeated robots (the call site passes
  the count from `FUN_004164a0`'s `FUN_00413810` loop). Its UI label was not
  recovered.
* **Teammates do not score.** The 1.20 ReadMe's `hitting teammates no longer
  adds points` is encoded in `Career::award_match`: only the entered robot's
  points are added; a robot on the same report team is skipped. (The original's
  per-participant loop is over the bracket, where a career team has one entry.)

### Prizes

* The event result screen (`event_winner.gib`) shows `rounds won`, `team
  points` and `prize description` (`FUN_0046def0`, strings `0x6f2d04`,
  `0x6f2cf8`, `0x6f2ce4`). The description is the tier's message from
  `Events.txt`; the tier index follows the rounds won.
* The **stored** prize is the event trophy: when the event is won,
  `FUN_0046ca90` takes the event's trophy string (`FUN_0046cce0`) and appends
  it to the team's prize vector (`FUN_004fab50`). The vector keeps at most 20
  (`0x13` guard: the oldest is dropped).
* The prize models are the `tiers[i].0` lines of `Events.txt`
  (`Arenas\event_results\prizes\…\*.gmf`) and the trophy is
  `Arenas\event_results\prizes\…\*.gmf`; the scene that displays them is
  `Arenas/event_results/event_results.gmf` with the fixed camera in
  `Arenas/event_results.py`. No path is stored in the arena model: the paths
  come from `Events.txt`.
* The season champion's prize is `UI\teamhq\golden_sawblade.tga`
  (`FUN_004e84b0`).

### Money

**No money field was recovered.** The `10000` in the shipped records is the
`N/A` fastest-win sentinel, not cash (§1). Repairs have no cost string and no
subtraction path; prizes are models. The career model still carries a `money`
counter because the task asks for one, but it is **explicit, sidecar-only and
zero by default**; nothing in the game's files is written from it.

---

## 5. Repairs

* The damage is kept in the **team record's event block at `Team+0x12c`**,
  not in the `.bot` file. `Team::save` writes it when `Robot In Event != -1`
  (`0x4fb377`–`0x4fb37f`), and `Team::load` reads it (`0x4faee9`–`0x4faef1`).
  `FUN_004df7f0` writes two lists of `(index, value)` pairs, which is the shape
  of a component damage list; the exact component ids are not decoded here.
* The repairs window (`teamhq\repairs.gib`, constructor `FUN_004d5de0`) binds
  `repairs list`, `repair time` and `repair button`. `FUN_004d6480` draws the
  damaged components, the `damage.tga` icon for fractured items (the 1.20
  patch note) and a health bar computed as `(max - current) / max`.
* Repairing is a **progress bar over time**: `FUN_004d6e10` advances the bar,
  and when `FUN_0058b470` reports it full it calls `FUN_004d7010`, which
  decrements the damaged-component count and `FUN_005ff530` restores the
  components. The `repair time` text is driven by the same list. No money is
  charged.
* The window passes `0x1e` (30) to the progress widget (`0x4d6334`), but the
  unit (seconds, ticks) was not established. **Cost and time are explicit
  config** (`CareerPolicy::repair_seconds`, `repair_cost`); the defaults are
  30 and 0.
* `MatchReport` carries per-robot `damage_taken`/`defeated`, not components.
  `Career::repairs_from_report` therefore produces one repair per damaged robot
  and names the robot as the component; the component-level list would need the
  `+0x12c` decoder, which is not implemented.

---

## 6. What was not established

1. The **RNG, seed and filter** of the nine-event draw (`FUN_00500120`).
2. The **exact index mapping** of the fifteen-int league line across rounds:
   the writer indexes bracket participants, while the reader indexes the AI
   table. The model maps report team slots to AI order and documents the
   approximation.
3. The meanings of `+0x50` (defeated robots, our label), `+0xa8`/`+0xac`
   (the history labels say wins/losses, the increment is round-based) and the
   nine `+0xf4` history ints.
4. The meaning of `+0xf6`, `+0xf8`, `+0xfc` (the last two are written on the
   closing line only when `+0xf6` is false).
5. Whether `+0xb8` is exactly "fastest win" rather than another minimum stat:
   the `N/A` sentinel and the minimum update are strong, the UI label is the
   evidence; **money is disproven**.
6. Any **cash economy** or **repair cost**; none was found.
7. The **repair time unit** and the component-level damage encoding in
   `Team+0x12c`.
8. The **season advance** path (`+0x98` increment, league reset); the model's
   `next_season` is explicit.
9. Whether the original writes `AI/Teams.txt` on a career save. This port writes
   it unchanged (byte-identical); the league reads its points from the player's
   record anyway.
10. The tier prize is **displayed** but whether it is ever stored was not
    observed; only the trophy and the golden sawblade are stored.

---

## 7. The model and the save decision

`ra2sim::career` (no new crate, no new dependency):

* `Career` — the player's `TeamTable`, the AI `TeamTable`, `Events.txt`, the
  nine-event schedule, the phase (`Schedule` / `InEvent` / `Complete`), prizes,
  pending repairs, per-event history and the explicit money counter.
* `Career::new(game, seed)` reads the tables and draws a schedule if the record
  has none; `Career::set_schedule(ids)` takes an exact one.
* `Career::enter_event`, `award_match(&MatchReport, entered_robot)`,
  `finish_event(won)`, `next_season(seed)`.
* `Career::repairs_from_report`, `apply_repair`, `apply_repairs`.
* `Career::load(game)` / `save(game)`.

### Where the save lives

| State | Home |
|---|---|
| name, motto, logo, robots, `Robot In Event` | `Teams/Teams.txt` / `AI/Teams.txt` (the game's own lines) |
| league line (`+0x5c`…`+0x94`), player points (`+0x58`), event points (`+0x54`), defeated (`+0x50`) | `Teams/Teams.txt` |
| eight history ints (`+0x98`…`+0xb4`), fastest win (`+0xb8`), event id (`+0xbc`) | `Teams/Teams.txt` |
| schedule (`+0xe4`), prize models (`+0x100`), the nine `+0xf4` history ints | `Teams/Teams.txt` (the original's own lines) |
| season phase, explicit money, pending repair list, full event history (event id, rounds, points, won), prize descriptions/art | **`Teams/career.dat`** (versioned sidecar) |

`Teams/Teams.txt` is written with `ra2fmt::team::TeamTable::write`; the AI table
is written back unchanged. The sidecar is a fixed-order, versioned text file
(`ra2career 1`); it is never a substitute for the game's table and the game
never reads it. A no-op load/save is byte-identical for both shipped tables
(tested); a career save adds the schedule/prize lines and the nine history
lines only when a season exists, exactly where `Team::save` puts them.

**Why the sidecar:** the phase, the pending repairs and the detailed history
have no recovered field in the record; the `+0x12c` damage block is not
decoded. Rather than writing a plausible-looking field the game would misread,
those live in `career.dat`.

---

## 8. Tests

`engine/crates/ra2sim/src/career.rs` (run with `RA2_GAME` pointing at the
shipped game; the game-dependent ones skip when it is absent):

1. `a_career_run_scores_repairs_saves_and_reloads_identically` — a real
   `Teams/Teams.txt` + `AI/Teams.txt` + `Events.txt` copy; draw a nine-event
   schedule, enter the first event, award a 2-robot report (500 points,
   20 damage), check `80 + 2 = 82` event points, the teammate's 900 does not
   count, finish the event (82 season points, trophy, rank 1), set the explicit
   money to 1234, save, reload, assert every field including the 15-int line,
   the closing lines, the nine history ints and the money, then re-save and
   compare bytes. The written table is read back
   with `ra2fmt::team::read_table` to prove the schedule/prizes/history have a
   game-file home.
2. `the_event_list_is_the_shipped_twenty_five` — 25 events, 11 with two tiers
   and 14 with four.
3. `nine_events_complete_the_season_and_the_next_one_resets_it` — nine wins,
   `81` points each (2 + 80), 729 season points (the model's fixture uses
   `200/200 + 160/2`), `Complete`, ten prizes (nine trophies + the golden
   sawblade), then `next_season` resets the league and keeps the lifetime
   total-events count.
4. `the_shipped_team_tables_round_trip_after_a_no_op_career` — the corpus
   test: both tables byte-identical after load/save.
5. `the_career_sidecar_is_byte_stable` — save, load, save; identical sidecar.
6. `the_sidecar_round_trips_every_text_field` — tabs and backslashes in a robot
   name survive.
7. `engine/crates/ra2fmt/src/team.rs`'s
   `career_fields_are_typed_and_list_lines_round_trip` — the league line,
   prize/schedule list lines and the nine history lines parse and write
   byte-for-byte.

The career tests use a synthetic `MatchReport`; a real two-AI `Match` on the
box arena is exercised by `ra2sim::r#match`'s own tests and takes seconds per
round, which is why this suite drives the report directly (the task allows it).
