# 24 — The match shell: clock, time-out, slots, countdown, results, camera, chat

This is the M12 pass over the C++ match shell. It closes the five details
`docs/11-match-rules.md` §7 and `docs/19-match-fidelity.md` §4 left open and
records the one that stays partial. Every address is in the shipped
`Robot Arena 2.exe` (SHA-256
`bc0f6833ece8c47e8625e5b12940d2509540cb393566577504b50b2318351a4f`, PE32,
image base `0x400000`, version 1.20).

The short version:

| detail | answer |
|---|---|
| the clock | `+0xe0` is the remaining clock, `+0xdc * 60` at match init; it runs only while `+0x8e` (fighting) is set, so it **excludes the countdown** |
| zero | `0x4133b3` calls `FUN_00418770(0)`; a unique positive best alive total wins, a tie (including 0-0) is a draw |
| the handoff | `0x418770` → `FUN_004170e0(p1, p2)` |
| `playerN status` | meter *i* is attached to `player{i+1} status`; 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right |
| countdown | timer 4.0 s, number `trunc(timer)`: `3`, `2`, `1` for one second each, fight starts at 1.0 s remaining |
| two result ids | the ids of the result rows that get the winner text; `-1` / a shared best (`+0x74`) suppresses it |
| results delay | 3.5 s (`+0xec = 0x6a5788`), then `match results window` + `in game popup win` are shown |
| camera textbox | `Watch Cam`, `Static Cam`, `Orbit Cam` (or the arena's own name), shown for 3.0 s; `chase` is ours |
| chat | backslash shows `chat group` and focuses `chat entry`; local only in this client |

---

## 1. The clock and the time-out

### 1.1 The fields

`GameWindow` (the object `0x412fc0` updates) keeps the match clock in four
fields:

| offset | meaning | evidence |
|---|---|---|
| `+0x8e` | fighting: set by `0x416360` at the bell (`0x416408`) | `0x412fe1` |
| `+0x8f` | `isMatchOver`, set by `0x416450` (`0x416458`) | getter `0x476500` |
| `+0xd8` | timed match: `FUN_00414230(0)` clears it, a positive value sets it | `0x414238`, `0x414247` |
| `+0xdc` | the match length in **minutes** | `FUN_00414230` (`0x414241`) |
| `+0xe0` | seconds remaining; the clock the HUD shows | `0x415ea7`, `0x4132d0` |
| `+0xe4` | seconds elapsed, reset at the bell | `0x412ff8`, `0x41640f` |
| `+0xe8` | hold: no clock tick and no winner check while set | setter `0x43e0c0`; read `0x41328b`, `0x4133c6` |
| `+0xea`/`+0xec` | results-window countdown / its 3.5 s | `0x41721c`–`0x417229` |

The constructor `FUN_00412570` writes the defaults at `0x41276d`–`0x41278a`:
`+0xd8 = 1`, `+0xdc = 3`, `+0xe0 = 0`, `+0xe4 = 0`, `+0xe8 = 0`. That is a
**timed three-minute match**; `MatchConfig::new` now starts from
`DEFAULT_MATCH_SECONDS = 180.0`.

`0x415500` turns the minutes into seconds at `0x415e90`–`0x415ea7`:

```
eax = +0xdc
eax = eax * 3
ecx = eax * 5
ecx = ecx << 2       // *60
+0xe0 = (float) ecx
```

The setup window's time dropdown reaches the same fields through
`FUN_00414230` (`0x414230`): `0` clears `+0xd8` (the "No limit" entry) and
anything else stores `+0xdc = minutes`, `+0xd8 = 1`.

### 1.2 The update, and what the clock includes

`0x412fc0` increments the elapsed clock only when the fighting byte is set:

```
0x412fe1  if (+0x8e) +0xe4 += dt
0x412ffe  _DAT_0071473c += dt          // a free-running 1 s tally timer
```

The remaining clock is a separate block:

```
0x41327d  if (+0xd8 && !+0xe8)
0x413299      +0xe0 -= dt
0x4132b0      +0xe0 = max(+0xe0, 0.0)     // FUN_00420370
0x41339c      if (+0xe0 == 0.0)
0x4133b3          FUN_00418770(0)
```

Because the whole block is inside the update's `+0x8e && !+0x8f` guard and
`+0x8e` is only set by `0x416360` at the bell, **the countdown does not tick
the clock**. The fight clock starts at the full configured length when the
countdown ends. Our `Match::clock` (fight time) and the HUD's countdown from
`duration_limit` are therefore the original's two numbers.

The HUD's `time group` follows the same flag: `0x4758c0` hides it before the
countdown, and `0x416360` calls `FUN_00477460(+0xd8)` at the bell
(`0x416422`), which shows the group only for a timed match. A "No limit" match
never shows the time box; this port now hides it too.

### 1.3 Zero: the points comparison

At `0x4133b3` the update calls `FUN_00418770(0)` (`0x418770`). That function
is the points finish for both the time-out and the quit path (`0x475ee0` calls
it with `1`).

Non-team types (`0x418770`'s else branch):

- only robots with state 0 (alive: `FUN_00413810`) are considered;
- `local_2c` takes the id of the robot that takes a **strictly** higher score
  than the running best (`0x4189xx`, `LAB_0041899c`), and the running best
  starts at 0, so a winner needs a positive total;
- a score **equal** to the best sets `+0x74 = 1` and leaves `local_2c` alone.
  So a shared best — 5-5, or the all-zero time-out — is a draw: `+0x74 = 1`
  suppresses the winner text in `FUN_004884b0`;
- the handoff at the end is `FUN_004170e0(local_2c, -1)`.

`TEAM MATCH` (the first branch):

- sums only alive robots by team id (`+0x30`, `FUN_004177c0`);
- a tie (`local_24 <= local_28`) sets `+0x74 = 1` — a draw;
- the higher side is turned into a list (`FUN_00406a00`), and the handoff takes
  its first two entries: `FUN_0047a220(0)` and `FUN_0047a220(1)`. In a
  2-robot-per-side event those are the side's two robots; the handoff carries
  **at most two ids**.

The last-robot/last-team rule is the other function: `FUN_00418040`
(`0x418040`) counts survivors and calls `FUN_004170e0(first_alive, -1)` for a
last-robot finish. For `TEAM MATCH` it passes the first survivor plus the first
alive robot of a different team when there is one; when only one team remains
that second id is `-1`, so only one row is marked. The clock is checked first
in `0x412fc0`, so a time-out on the same frame as a kill wins.

### 1.4 Implemented

- `MatchConfig::new` defaults to `DEFAULT_MATCH_SECONDS` (180 s, `0x41276d`);
  `.with_duration(None)` is "No limit" (`FUN_00414230(0)`).
- `Match::end_reason` checks the time-out first, then the last-robot/team
  rules, mirroring `0x412fc0`'s order.
- `Match::decide_winners`:
  - non-team time-out: unique positive highest score among **alive** robots,
    otherwise no winners (a tie at any score, including 0-0);
  - team time-out: alive team totals, unique higher side, first two robots of
    that side; a tie is a draw;
  - last robot: the survivor; last team: the first survivor (`-1` second id).
- `Match::report().winners` is exactly the rows the results screen marks, at
  most two.

Measured tests:

| test | points | winners |
|---|---|---|
| `a_pointless_timeout_reports_a_draw` | 0 / 0 | `[]` |
| `a_points_timeout_names_the_unique_higher_total` | 7 / 3 | `[0]` |
| `a_tied_points_timeout_is_a_draw` | 5 / 5 | `[]` |

---

## 2. Which robot gets which `playerN status`

`0x4749c0` creates four status meters into `+0x15c`, `+0x160`, `+0x164`,
`+0x168` in that order (`0x474abc`–`0x474af9`), then attaches them to the
screen's groups:

| meter | group | address |
|---|---|---|
| 0 (`+0x15c`) | `player1 status` | `0x474afb`–`0x474b43` |
| 1 (`+0x160`) | `player2 status` | `0x474b46`–`0x474b91` |
| 2 (`+0x164`) | `player3 status` | `0x474b94`–`0x474bdf` |
| 3 (`+0x168`) | `player4 status` | `0x474be2`–`0x474c2d` |

The group records in `UI/common/gamewindow.gib` place them:

| group | `Position` | corner |
|---|---|---|
| `player1 status` | `0.01 1 0.02 1` | top-left |
| `player2 status` | `0.99 1 0.02 1` | top-right |
| `player3 status` | `0.01 1 0.98 1` | bottom-left |
| `player4 status` | `0.99 1 0.98 1` | bottom-right |

So robot 0 is the top-left corner, robot 1 top-right, robot 2 bottom-left and
robot 3 bottom-right. A two-robot match shows the top row and hides the bottom
row. This port already bound slot *i* to `PLAYERS[i]`; the test
`the_four_status_groups_take_the_robots_in_order_from_the_top_left` now pins
all four names and the four corners.

The same constructor binds the other shell widgets:

| field | widget | address |
|---|---|---|
| `+0x1b4` | `chat group` | `0x474c35` |
| `+0x1b8` | `chat entry` | `0x474c73` |
| `+0x1bc` | `chat displaybox` | `0x474ce0` |
| `+0x1c4` | (`0x6f34ac`, the time label) | `0x474d71` |
| `+0x1c8` | `time group` | `0x474daf` |
| `+0x178` | `camera textbox` | `0x474ded` |
| `+0x1cc` | `count number` | `0x474e26` |

---

## 3. The countdown schedule

`0x4759c0` writes the timer:

```
0x475a97  +0x1dc = 0x40800000      // 4.0f
0x475aa1  +0x240 = GetTickCount() * 0.001
```

`0x477480` is the per-frame countdown (a virtual at the vtable slot
`0x6a7b4c`; Ghidra does not make it a procedure because `0x477460` is a thunk
with a jump table). It draws the number and hands off:

```
0x47751f  if (+0x1dc > 0)
0x4775b1      eax = trunc(+0x1dc)          // 0x63aabc: fistp, truncation
0x4775bc      if (eax > 0) draw eax in `count number`
0x47766b      else +0x1dc = 0
0x477666      call 0x416360                 // begin the fight
```

Therefore:

| timer | number |
|---|---|
| `(3, 4]` | `3` |
| `(2, 3]` | `2` |
| `[1, 2]` | `1` |
| `< 1.0` | none; the fight starts |

The visible sequence is `3 → 2 → 1`, one second each, and the fight begins with
1.0 s still on the 4.0 timer. The visible countdown is therefore three seconds;
the 4.0 is data and the handoff point is the truncation test, not zero. Our previous reading (`1` held for the last second,
fight at 0.0) was wrong and is corrected:

- `Match::step` changes to `Fight` when `countdown_number(left)` is `None`,
  i.e. `left < 1.0`, and sets `countdown_left = 0` (as `0x47766b` does);
- `hud::countdown_text` is `trunc`, so `3.0` still shows `3` and `2.999` shows
  `2`; under 1.0 the group is hidden.

The file's own note is "show `countdown group` when match is ready to begin,
then hide during match"; `0x416360` hides it.

The countdown is preceded by a 2 s pre-countdown (`0x4758c0`: `+0x1d0 = 2`,
`+0x1d4 = 1`, `skip group` shown) while the arena intro plays. That is the
intro path this port still does not run; it is not part of the 4.0 number
timer.

---

## 4. The results handoff

`FUN_004170e0(p1, p2)` (`0x4170e0`) is the end of the match:

1. `0x416450` sets `+0x8f` and deactivates every robot.
2. It loads `common\matchresults.gib` into a `MatchResultsWindow`
   (`0x417104`–`0x4171cf`).
3. `FUN_004172d0(p1, p2)` (`0x4171e0`) stores `+0x70 = p1` and forwards both
   ids to every robot's vtable `+0x34` and the scene's vtable `+0x38`.
4. `FUN_00487d50(p1, p2, +0x74)` (`0x4171fc`) reaches `FUN_004884b0`
   (`0x4884b0`), which fills each result row and calls `FUN_00487950(1)` —
   the winner text — only when the row's id equals `p1` or `p2` **and**
   `+0x74 == 0`.
5. If this is not a network session (`+0x8c == 0`), `0x41721c`–`0x417229`
   writes `+0xea = 1` and `+0xec = *0x6a5788` (**3.5f**). The update's
   `0x41309c`–`0x4131fb` block decrements it and, at zero, shows
   `match results window` and `in game popup win`.

So the two ids are **winner row ids**, not a winner/loser screen and not a
sound: a last-robot finish passes one survivor and `-1`, a points finish one
unique best and `-1`, and a team finish the first two robots of the winning
side. `+0x74` is the "shared best / no winner" byte that suppresses the text.
The 3.5 s is the delay before the already-filled results window is shown; this
port's `MatchState::Over → Results` delay is the same number.

The same handoff also runs when the player quits mid-match: `0x475ee0` calls
`FUN_00418770(1)` and `FUN_004170e0` follows.

---

## 5. The camera textbox

`0x4749c0` binds `camera textbox` to `+0x178` (`0x474ded`). The three camera
entry points each call `FUN_00477180` (`0x477180`):

- `0x476f00` (orbit) pushes the literal **`Orbit Cam`** at `0x476fd1`;
- `0x477030` (static) calls `FUN_00418d70`, which asks the scene camera for its
  name (`0x418d70`'s vtable `+0x40`);
- `0x4770a0` (watch) calls `FUN_00418e20`, the same getter through `+0x44`.

The camera classes name themselves `Watch Cam` (`0x43a020`, literal at
`0x6f1044`), `Static Cam` (`0x439f20`, `0x6f1038`) and `Orbit Cam`
(`0x439d60`, `0x6f102c`). An arena's `AddStaticCamera("Static 1", …)` /
`AddWatchCamera("Watch 1", …)` (`Arenas/box.py:22-28`) supplies the name the
scene getter returns, so the box normally shows the arena's own camera name.

`FUN_00477180` shows the box and writes `+0x17c = 3.0f` at `0x4771a2`.
`0x4774f1` decrements it and, at zero, hides `+0x178` (`0x47750d`). The box is
therefore a **3-second notification**, not a permanent label.

This port:
- names the modes `Watch Cam`, `Static Cam`, `Orbit Cam` (the class defaults),
  and uses the arena's own name for static when the script has one (box:
  `Static 1`);
- labels its driving camera `Chase Cam (ours)` — `chase` is not a binary
  string (`ra2.cfg` and `0x4759c0` know only `watch`, `static`, `orbit`);
- shows the box for `CAMERA_LABEL_SECONDS = 3.0` after a change and hides it
  afterwards, on the match's own dt (`0x4774f1`).

---

## 6. The chat group

`gamewindow.gib` names the group and its own note is *"show/hide this group
using the `chat` key (backslash?)"*. `0x475ee0` confirms it: on the `\` key
with the match state `1` (playing) it calls `(*(+0x1b4)+0x54)(1)` (show
`chat group`) and `FUN_005edd90(+0x1b8)` (focus `chat entry`). The entry is a
shipped `TextBox`; the display is a `DisplayBox` whose note says it shows a few
lines and clears after a few seconds.

This port implements the minimum: `\` opens the group and clears the entry,
typed characters fill `chat entry`, Backspace edits, and Enter/Escape close it.
The submitted line is printed to the log, not drawn in the `DisplayBox`, and
nothing is sent anywhere — there is no network path in this client. Escape
does not open the pause dialog while the entry is open.

**Unproven:** which key the original's text field uses to close (the entry is
the engine's generic `FUN_005ee6b0` text widget), and the chat display's exact
line retention/clear timer.

---

## 7. Tests added

`ra2sim::r#match::tests`:

- `a_pointless_timeout_reports_a_draw` (kept);
- `a_points_timeout_names_the_unique_higher_total` — 7 vs 3, winner row 1;
- `a_tied_points_timeout_is_a_draw` — 5 vs 5, no winner text;
- `the_countdown_is_three_two_one_and_starts_at_one_second_left` — the
  truncation boundaries plus a real match transition at ~3.0 s;
- `the_match_defaults_to_the_three_minute_clock`;
- the real-fight test now checks "still counting at 2.9 s", not 3.9 s.

`ra2-app::hud::tests`:

- `the_four_status_groups_take_the_robots_in_order_from_the_top_left` — all
  four names and the four corners;
- `the_camera_textbox_shows_the_cameras_name_then_hides` — the three labels
  and `Chase Cam (ours)`;
- `the_chat_group_opens_and_closes_with_the_entry_text`;
- `a_no_limit_match_hides_the_time_group`;
- `the_clock_counts_down_from_the_configured_length` — the corrected `trunc`
  schedule.

`ra2-app::match_mode::tests`:

- `the_camera_textbox_names_the_camera_and_hides_after_three_seconds` — a real
  session: `Watch Cam` → `Static 1` → `Orbit Cam`, then hidden after 3.0 s.

---

## 8. What remains unproven

1. **The `+0x74` reading.** It is inferred from `FUN_004884b0` (a nonzero
   third argument suppresses the winner text) and from `FUN_00418770`'s
   equal-best branches. No call site reads it as anything else, but its name in
   the original is unknown.
2. **The pre-countdown/intro timer.** `0x4758c0` sets `+0x1d0 = 2` and
   `+0x1d4 = 1`; `0x477737` counts `+0x1d0` down and calls `0x4162e0`, which
   only toggles a scene-camera flag. The intro path (`ArenaIntros`, `skip
   group`) is still not implemented, so this port starts the number countdown
   directly.
3. **The watch/static camera names.** `FUN_00418d70`/`FUN_00418e20` ask the
   scene camera, but the vtable `+0x40`/`+0x44` implementations were not read;
   the class defaults and the arena's `AddStaticCamera` argument are measured,
   the watch camera's arena name is not. Our watch mode is a synthetic framing
   anyway, so it uses `Watch Cam`.
4. **The chat entry's close key and the display box's retention.** The generic
   text widget is `FUN_005ee6b0`; only the opening key is traced. This port's
   Enter/Escape close and log-only history are ours.
5. **The 1 s points tally.** `0x4132d6`–`0x413397` snapshots the four point
   totals into `FUN_004138b0` and plays/announces something one second after
   the clock stops (`_DAT_0071473c > 1.0`). It does not decide the winner; the
   winner is already `FUN_00418770`'s. Its exact output/sound was not decoded.
6. **The time dropdown's menu.** The setter `FUN_00414230` and the unit
   (minutes) are measured; the item list in `0x4733f0` was not re-read this
   pass, so "No limit" plus whole minutes is the established contract, not the
   menu's exact labels.
