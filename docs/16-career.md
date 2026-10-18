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

## 3a. The event bracket (M10.3/M10.4 depth)

The bracket is the game's real structure, not one match dressed as an event.
The evidence below is from `Robot Arena 2.exe` (same SHA-256 as above) and
`Events.txt`; the model is [`ra2sim::career::EventBracket`].

| rule | evidence | status |
|---|---|---|
| **16 entrants per event** | `FUN_004695a0` loads `[0x6a7280]` = `0x41308000` (`16.0`) and compares it with `[0x6a5600]` = `0x3f800000` (`1.0`) at `0x4695c9`/`0x4695d5`; the loop stops when the field reaches 1 | established |
| **The tier count is the round count** | `Events.txt`'s line 4 is the losing-tier count; the results messages run “didn't win”, “won one”, … , “won the event”. Every 4-tier event has 4 losing tiers plus the trophy; every 2-tier event 2 plus the trophy. `FUN_0046c240` prints `"Final Match"` when the round index equals the bracket's `+0x8` minus one (`0x46c27e`–`0x46c295`), so `+0x8` is the number of rounds. The shipped split is 14 × 4-round, 11 × 2-round events. | established |
| **Bots per match** | `Events.txt`'s first trailing setting is `2` for every 4-round event and `4` for every 2-round event (all 25). `FUN_004695a0` divides the 16 entrants by the bracket config's `+0x10c` to get the first round's match count (`0x4695fc`–`0x469616`) and creates each match with `+0x10c` slots (`0x46968e`–`0x469697`); `16 / bots ^ rounds == 1`. `FUN_00468f50` stores the same value at a round's `+4`. This is the data's consistent reading; the model uses `settings[0]` when it satisfies `bots ^ rounds == 16`, otherwise derives it from the tier count. | measured correlation |
| **One winner per match (single elimination)** | `FUN_00468f50`'s second parameter is `+0x110`; `FUN_004695a0` scales the field by `+0x110 / +0x10c` (`0x4697a8`–`0x4697d2`), i.e. 1/2 or 1/4, and `FUN_004698f0` special-cases a two-winner match. The shipped flow is one advancing entrant. | established for the shipped path |
| **The player is match 0, slot 0** | `FUN_004695a0` skips the random draw exactly when `match == 0 && slot == 0` (`0x4696bf`–`0x4696cd`); `FUN_0046c830` writes the player back into the first slot when a round is won (`0x46c937`–`0x46c94c`, `FUN_0046cde0` with the zero flag). | established |
| **Opponents are drawn from the 15 AI teams** | every other slot calls `FUN_00419190(0xf)` (`0x4696d1`) and retries until the index names a valid AI record; the draw is per round and per slot. There is **no de-duplication within a match** in this loop, so a 4-bot melee can contain the same AI twice; the model mirrors that. | established |
| **Win advances; the next field is redrawn** | `FUN_0046ce50` calls `FUN_0046c830(win, robot)` first. When `round < rounds-1`, `FUN_0046c830` calls `FUN_004faaf0` (which increments `Team+0x110`) and fills the next round's slots with fresh draws (`0x46c8d4`–`0x46c9cf`); it returns 0 (the event continues) and `FUN_0046ce50` then awards the round with `FUN_0046c400`. | established |
| **A loss ends the event, not the season** | `FUN_0046ce50`'s event-continues branch calls `FUN_0046ca90(0,0)` when the match was lost (`0x46cf35`–`0x46cf45`). `FUN_0046ca90(0,…)` simulates the remaining rounds with `FUN_0046c830(-1,0)`/`FUN_0046c400(-1,0)` (AI-only points), picks a finalist as the event winner, folds the player's points into the season and clears the event. The HQ's schedule is still open; only the event is over. | established |
| **The trophy is stored only for the final win** | `FUN_0046c830` calls `FUN_0046ca90(1, robot)` at the final; only the `param_1 != 0` branch calls `FUN_0046cd30(1)` and `FUN_004fab50` (the trophy). A loss takes the `0` branch, which stores no trophy. | established |
| **The round heading** | `FUN_0046c240` (`0x46c240`–`0x46c37d`): round == rounds−1 → `"Final Match"` (`0x6f2ae4`), otherwise `"Round "` (`0x6f2adc`) + round+1. | established |
| **The bracket screen** | `event_bracket.gib`'s `bracket group` note is “build the event bracket inside this group”. `FUN_0046ad60` lays out one column per round (`FUN_00406d20()+1` loop) and puts `UI\teamhq\questionmark.bmp` (`0x6f2a58`) in the cells of rounds not reached. The play button's enabled state is `round != rounds` (`0x46ac6c`–`0x46ac99`, `FUN_005ec4e0`). | established shape; the generated columns in this port are a readable list |

### The points correction

The earlier note said “`160 / robots`”. The divisor loaded at `0x46c454`
(`FUN_0046c400`, player bonus) and `0x46c52d` (the per-participant loop) is
`FUN_00406d20()` on the bracket object, i.e. `bracket+0x8` — and `FUN_0046c240`
is the proof that `+0x8` is the **round count**, not the match size. A 2-round
event therefore pays `160/2 = 80` participation, a 4-round event `160/4 = 40`,
for every round; the player also gets `robot_points / 200`. Over a whole event
the player's total is `rounds × (160/rounds + robot_points/200)`, and every AI
slot in every match of the round gets `160/rounds` (`0x46c4f0`–`0x46c545`).
The model keeps the old `160 / report.robots` only on the legacy path with no
bracket. This does not change the 15-int league line or the save format; it
corrects the per-round amount the earlier fixture (a 2-bot, 2-round event) could
not distinguish from `160 / robots`.

### How the model draws it

`EventBracket::new(event, data, seed)` uses the event's tier count and
`settings[0]`/the 16-entrant constant, then `draw_round` fills match 0 slot 0
with `Entrant::Player` and every other slot with a deterministic
`Ai(u32 % 15)`. `Career::play_round` awards the round, advances on a win (and
draws the next round), ends the event on a loss (simulating the remaining
rounds for AI points), and calls `finish_event` only at that point. The RNG
algorithm and seed were not recovered; the seed is explicit
(`Career::seed`, sidecar `seed`), and the same seed, event and round always
draw the same field.

---

## 4. Points, prizes and money

### Points

* `Team+0x54` — the current event's points; `Team+0x58` — the season's.
* A match adds `entered robot points / 200 + 160 / rounds` for the player and
  `160 / rounds` for each other participant (`FUN_0046c400`; the divisor is the
  bracket's round count, see §3a). The entered robot's `points` are its `+0x20`
  score: King of the Hill's script score (`plus.addPoints`) or, in every other
  type, accumulated effective damage (`FUN_004175b0`'s not-KOTH branch;
  `docs/11-match-rules.md` §4.1 and `docs/19-match-fidelity.md` §1). It is
  awarded after **every** round, not once per event.
* `Team+0x50` accumulates the number of defeated robots (the call site passes
  the count from `FUN_004164a0`'s `FUN_00413810` loop). Its UI label was not
  recovered.
* **Teammates do not score.** The 1.20 ReadMe's `hitting teammates no longer
  adds points` is encoded in `Career::play_round`/`Career::award_match`: only
  the entered robot's points are added; a robot on the same report team is
  skipped. (The original's per-participant loop is over the bracket, where a
  career team has one entry.)

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

The full component-level recon, with the pseudocode and the remaining unknowns,
is `docs/22-component-repairs.md`. In brief:

* The damage is kept in the **team record's event block at `Team+0x12c`**,
  not in the `.bot` file. It is two `std::map<int, …>` instances:
  `Team+0x12c` maps a component id to `{current hp, max hp}` and
  `Team+0x13c` maps the same id to the fractured flag. `Team::save` writes both
  lists when `Robot In Event != -1` (`0x4fb377`–`0x4fb37f`), and `Team::load`
  reads them (`0x4faee9`–`0x4faef1`); `FUN_004df7f0` writes a count and
  `<id> <current> <max>` per node, then a count and `<id> <flag>`.
* The repairs window (`teamhq\repairs.gib`, constructor `FUN_004d5de0`, open
  `FUN_004d62a0`) binds `repairs list`, `repair time` and `repair button`.
  `FUN_004d6480` draws one row per damaged component: the component's name, a
  health bar computed as `(max - current) / max` scaled to 20 units, and the
  shipped `UI\teamhq\damage.tga` icon when the `Team+0x13c` flag is 1 (the
  1.20 patch note's fractured marker).
* Repairing is a **progress bar over time**: `FUN_004d6e10` advances the bar,
  and when `FUN_0058b470` reports it full it restores that one component
  (`Team+0x12c`: current = max; `Team+0x13c`: flag = 0) and calls
  `FUN_004d7010`/`FUN_005ff530`, which erase the selected component row and
  decrement the model count. It is per component, not per robot. No money is
  charged.
* The window passes `0x1e` (30) to `FUN_0058b150` with the **minutes** selector
  (`FUN_0058b2f0`), so `repair time` starts at `00:30:00`; the screen's own
  instructions say "You have 30 minutes to make repairs". The model therefore
  defaults `CareerPolicy::repair_seconds` to 1800; `repair_cost` stays 0.
* `MatchReport` now carries every component's `hp`/`total_hp` and its
  `detached`/`fractured` state. `Career::repairs_from_report` produces one
  row per damaged/detached/fractured component, grouped under its robot, with
  the component index/name/class and the recorded hitpoints; a completed row
  removes that component's damage. The binary's component id is the row key;
  the port uses the sim's 0-based component index and names it as such.

**What is still not proven:** the match routine that inserts into `Team+0x12c`,
the exact derivation of the component id, and whether every row gets the same
30-minute clock or a damage-scaled one (`docs/22-component-repairs.md` §3).

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
7. The **match writer** of the component damage block, the exact component-id
   derivation, and whether the 30-minute bench clock scales per row; the shape
   of `Team+0x12c`/`Team+0x13c` is established (`docs/22-component-repairs.md`).
8. The **season advance** path (`+0x98` increment, league reset); the model's
   `next_season` is explicit.
9. Whether the original writes `AI/Teams.txt` on a career save. This port writes
   it unchanged (byte-identical); the league reads its points from the player's
   record anyway.
10. The tier prize is **displayed** but whether it is ever stored was not
    observed; only the trophy and the golden sawblade are stored.
11. The **exact `Events.txt` → bracket-config writer**: `FUN_004f9a90` reads
    six ints from the team record and builds the 16-entrant field from them,
    but the code that copies the first trailing setting into those ints was
    not traced end-to-end. The correlation (2 ↔ 4 rounds, 4 ↔ 2 rounds, all 25
    events) is total; the model uses it and has an explicit fallback.
12. The **two-winner variant**: `FUN_004698f0` has a case for two advancing
    entrants, and `FUN_00468f50` stores `+0x110` as a parameter. The shipped
    events use one winner, and the model implements single elimination.
13. The **bracket screen's full geometry**: `FUN_0046ad60` draws one column per
    round with logos and `questionmark.bmp`, but the custom control it builds
    (`FUN_005f6010`) was not decoded. The port renders the rounds, results and
    next opponent as a list inside the shipped `bracket group`; the state and
    labels are the model's, the frame is the game's.

---

## 7. The model and the save decision

`ra2sim::career` (no new crate, no new dependency):

* `Career` — the player's `TeamTable`, the AI `TeamTable`, `Events.txt`, the
  nine-event schedule, the phase (`Schedule` / `InEvent` / `Complete`), prizes,
  pending repairs, per-event history, the bracket in progress and the explicit
  money counter.
* `Career::new(game, seed)` reads the tables and draws a schedule if the record
  has none; `Career::set_schedule(ids)` takes an exact one. The seed is kept on
  the career and in the sidecar.
* `Career::enter_event` (creates the 16-entrant bracket),
  `play_round(&MatchReport, entered_robot)` (the real event flow),
  `award_match`/`finish_event` (the legacy single-report path),
  `next_season(seed)`.
* `EventBracket`, `EventState` (in `ra2ui::hq`), `RoundResult`, `Entrant`.
* `Career::repairs_from_report`, `apply_repair`, `apply_repairs`.
* `Career::load(game)` / `save(game)`.

### Where the save lives

| State | Home |
|---|---|
| name, motto, logo, robots, `Robot In Event` | `Teams/Teams.txt` / `AI/Teams.txt` (the game's own lines) |
| league line (`+0x5c`…`+0x94`), player points (`+0x58`), event points (`+0x54`), defeated (`+0x50`) | `Teams/Teams.txt` |
| eight history ints (`+0x98`…`+0xb4`), fastest win (`+0xb8`), event id (`+0xbc`) | `Teams/Teams.txt` |
| schedule (`+0xe4`), prize models (`+0x100`), the nine `+0xf4` history ints | `Teams/Teams.txt` (the original's own lines) |
| season phase, explicit money, pending repair list, full event history (event id, rounds, points, won), prize descriptions/art, the bracket (event, shape, round, status, seed, champion, current opponents, per-round results) | **`Teams/career.dat`** (versioned sidecar) |

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
3. `nine_events_complete_the_season_and_the_next_one_resets_it` — the real
   bracket flow for nine events: every round is played, every event ends in a
   final win, the champion/points/prizes are asserted, then `next_season`
   resets the league and keeps the lifetime total-events count. The expected
   points are `9 × 160 + 10 × total_rounds` (each round pays
   `160/rounds + 2000/200`).
4. `every_shipped_event_is_a_sixteen_entrant_bracket` — all 25 events have
   `tiers == rounds`, `settings[0] == bots`, and `bots ^ rounds == 16`.
5. `a_loss_eliminates_from_the_event_but_not_from_the_season` — a first-round
   loss ends the event (no trophy), records `0/1`, folds the round's points
   into the season, and leaves the schedule open.
6. `a_final_loss_crowns_an_ai_and_stores_no_trophy` — the final loss still
   picks an AI champion and stores no trophy; the record is `1/1`.
7. `replaying_the_same_seed_reproduces_the_bracket` — two careers with the same
   seed draw the same field and the same opponents round after round.
8. `save_reload_mid_season_restores_the_bracket_and_opponents` — a save after a
   won round restores the seed, phase, bracket, round and next opponents.
9. `a_multi_round_event_writes_the_points_the_career_holds_after_every_round` —
   a four-round event saved after each round; the file's `+0x58` (season) and
   `+0x54` (event) equal the career's after every round and at the end. This is
   the save-path invariant the app-level test relies on.
10. `the_shipped_team_tables_round_trip_after_a_no_op_career` — the corpus
    test: both tables byte-identical after load/save.
11. `the_career_sidecar_is_byte_stable` — save, load, save; identical sidecar.
12. `the_sidecar_round_trips_every_text_field` — tabs and backslashes in a robot
    name survive.
13. `a_damaging_match_leaves_a_repair_the_clock_completes_and_saves` and
    `component_damage_leaves_one_row_per_component_and_repairs_in_order` — two
    damaged components leave two entries (cost 0), 40% of the 30-minute clock
    round-trips through the sidecar, the remaining 60% completes one row, the
    detached row keeps its name, the save reloads empty and no money was
    charged.
14. `engine/crates/ra2fmt/src/team.rs`'s
    `career_fields_are_typed_and_list_lines_round_trip` — the league line,
    prize/schedule list lines and the nine history lines parse and write
    byte-for-byte.

The career tests use a synthetic `MatchReport`; a real two-AI `Match` on the
box arena is exercised by `ra2sim::r#match`'s own tests and by
`ra2-app::hq_mode`'s season run (below), which is where the bracket meets the
shipped brains.
