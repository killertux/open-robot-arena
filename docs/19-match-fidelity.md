# 19 — Match fidelity: damage → points, the title order, the HUD bars

This is the M11 pass on top of `docs/11-match-rules.md` §7 and
`docs/14-match-presentation.md`. It closes the three measurable gaps and names
what cannot be closed from the shipped binary.

Binary: `Robot Arena 2.exe`, SHA-256
`bc0f6833ece8c47e8625e5b12940d2509540cb393566577504b50b2318351a4f`, PE32,
image base `0x400000`, version 1.20.

---

## 1. Damage → points

### 1.1 The recon's inversion

`FUN_004175b0` (the hit handler) compares the game type with `KING OF THE HILL`
at `0x4176f2` through `FUN_004206a0`:

```
0x4206a0:  cVar = FUN_00402200(a, b); return cVar == '\0';
0x402200:  iVar = FUN_00402360(b); return iVar == 0;      // this = a
0x402360:  FUN_004023c0(0, a->len, b, strlen(b));          // string compare
```

So `FUN_00402200` is **string equality** (`compare == 0`) and `FUN_004206a0`
is **inequality**. The block at `0x4176f2` is therefore the **not-`KING OF THE
HILL`** branch, not a KOTH branch. That is the whole correction: damage scores
in `DEATHMATCH`, `BATTLE ROYAL`, `TEAM MATCH` and `TABLETOP`, and not in KOTH.
The later `FUN_004206a0(…, "TEAM MATCH")` at `0x417737` is also an inequality
test, so the team comparison it guards runs **in `TEAM MATCH`**, which is the
1.20 ReadMe's "hitting teammates no longer adds points".

### 1.2 The rule, with addresses

`FUN_00417330` is the collision entry. It returns immediately when the match's
`isMatchOver` byte (`+0x8f`) is set, and only calls the hit handler when the raw
damage is at least 10:

```
if (10.0 <= param_5) FUN_004175b0(attacker, …, victim, component, damage, …);
```

`FUN_004175b0` then:

1. records the impact event and the `(player, amount, position)` display record;
2. computes the **effective damage** with `FUN_00489620` — a virtual call on the
   hit component (`component vtable +0x1c`) with the raw damage and the hit
   info; the returned float is what the award uses;
3. if the type is **not** `KING OF THE HILL` (`0x4176f2`):
   - with an attacker (`param_1 >= 0`) and a victim id (`param_3 >= 0`), checks
     the victim's state (`FUN_00413810`, robot `+0x34`) equals `0` (still alive);
   - in `TEAM MATCH` (`0x417737`) compares `FUN_004177c0` (`robot+0x30`) and
     returns when the teams are equal; the other non-KOTH types have no team
     test;
   - calls `FUN_004177e0(attacker, effective_damage, …, store = 1)`.

`FUN_004178a0` is the write: `0x41790f` reads `robot+0x20`
(`FUN_00494f50`), `0x417918` is `fild [score]`, `0x41791c` `fadd amount`,
`0x417920` `__ftol`, and `0x417928` stores it back through `FUN_004034d0`. The
score is an integer; each hit's fractional part is truncated, not carried.

Hazards are the no-attacker path: `FUN_004148f0` calls `FUN_004175b0` with
`param_1 = -1`, which still shows a floating number but passes `store = 0`
(`0x4177a5`), so a hazard never adds score. `plus.damage` (`FUN_00417ae0`)
reaches the same handler with the player ids it is given.

### 1.3 The `2.0f` at `0x504d13` and `0x504d5f`

It is not a points constant. `0x504ca0` is the flame's burn callback, registered
by the flame constructor (`FUN_00504080`, strings `Arenas\particles\flame.tga`
and `Fire burst`). It calls `FUN_004148f0(robot, component, 2.0)` — the hazard
path, no attacker — and then calls `FUN_004177e0` with `store = 0` to place the
floating `2` at the victim. So 2.0 is the flame's **damage per tick**, and the
call is a number popup, not a score write.

### 1.4 Implemented

- `ra2script::add_points_float` performs the original's `trunc(score + amount)`
  per event (`0x417918`); `plus.addPoints` keeps the integer path.
- `Duel::apply_hit(attacker, victim, component, damage, point)` is the engine's
  damage event; it records the hit when the raw damage is at least
  `SCORE_HIT_MIN_DAMAGE = 10.0` (`FUN_00417330`) and the victim was alive at the
  event (`+0x34 == 0`). `Duel::take_score_hits` hands them to the match.
- `Match::score_hits` applies the type rule: skip everything in
  `KING OF THE HILL`, skip a `TEAM MATCH` hit whose attacker and victim share
  `robot.team` (`+0x30`), otherwise add the damage.
- `Match::hit` is the deterministic one-event entry point (used by the tests;
  `FUN_004175b0`'s path is the same rule).
- `Match::step` drains hits during the fight; a countdown contact is not scored.

### 1.5 Measured

`ra2sim::r#match::tests::a_hit_scores_its_effective_damage_except_in_koth_and_against_teammates`:

| event | score before | score after | why |
|---|---|---|---|
| DEATHMATCH hit 12.7 | 0 | 12 | `trunc(0 + 12.7)` |
| DEATHMATCH hit 37.9 | 12 | 49 | `trunc(12 + 37.9)` |
| DEATHMATCH hit 9.9 | 49 | 49 | below `FUN_00417330`'s 10.0 gate |
| TEAM MATCH, teammate 40.0 | 0 | 0 | `0x417737` |
| TEAM MATCH, opponent 40.0 | 0 | 40 | different team |
| KING OF THE HILL hit 40.0 | 0 | 0 | damage never scores |

`a_real_deathmatch_reports_non_zero_points_to_the_results_rows` scores a 24
damage hit on a real box match and reads the results rows back: `rows[0].points
== 24`.

`a_team_match_last_team_finish_beats_a_higher_points_total` gives the team that
will be eliminated 50 points, eliminates it, and asserts the surviving team
wins: `FUN_00418040` settles a last-team finish; `FUN_00418770`'s totals only
settle the time-out. This ordering matters now that damage can put points on
both sides.

A real 30 s `--match` DEATHMATCH now reports points instead of zero:

```
Fight -> Over at 30.02s, points [31, 32], winners [0]
Results: reason Some(LastRobot), winners [0],
         points [("Roly Poly", 31), ("Little Metal Friend", 32)]
```

The previous run of the same command reported `points [0, 0]`; the rendered
`results.png` now shows **31** and **32** in the two cells
(`/tmp/ra2-title-after30/results.png`; before,
`/tmp/ra2-title/results.png`).

The KOTH script score is unaffected: the existing
`king_of_the_hill_scores_ten_every_half_second` still reads 10 at the first
0.5 s boundary and 90 at 5.0 s, because the match no longer adds damage points
there.

---

## 2. The results title order

### 2.1 What the data and the C++ say

`common/matchresults.gib`:

| record | Position / Size | attachment |
|---|---|---|
| `match results window` | `0.5 1 0.5 1`, `520×426` | none |
| `bg group` | `0 0`, `1 1` | `+Background` body at `0 0`, `1 1` (520×426, white 70%) |
| `New [TextBox]` (the title) | `0 0`, `1 1 28 0` | `+Background` body at `0 0`, `1 1 28 0` (black 20%) |
| `result slot 1..4` | `22,36 / 126 / 216 / 306`, `478×80` | `+Background` body at `21,35 …`, `480×82` |

The writer/reader pair is the one in `docs/02`: `FUN_00607de0` writes
`Widget true` and the **decorated widget** record, then `FUN_005e9eb0` writes
the decorator's own base `Widget` fields *after* the decorated widget's whole
subtree. The reader (`FUN_006141c0` → `FUN_006148b0`) attaches that body to the
decorator, and `FUN_00607b10` copies the decorated widget's position/size into
the decorator when it is installed. The body is therefore **the attachment's
own record, not a sibling**: it is drawn **before** the widget it dresses.

The body's serialized `Position`/`Size` are measured against the **decorated
widget's parent**, not against the widget itself. The result slots prove it:
the slot is at `22,36` and its shadow body is at `21,35` — one pixel up and
left of the slot, i.e. against the `bg group`, not inside the slot. Reading the
same body against the slot would put the shadow at `43,71`.

Consequently the body's own `ChildOrder` is not a sibling order. The panel's
body is `ChildOrder 1` and the title TextBox is `-1`; if the body were inserted
into the `bg group`'s sibling set they would be sorted, the panel (1) would be
drawn after the title (−1), and its 520×426 white body would cover the title's
lower half. That is the overdraw the task describes. The fix is the data's
intent: keep the body on the attachment, drawn before its widget.

### 2.2 Measured scene order

`match_mode::tests::the_results_title_is_not_covered_by_the_panel` reads the
built `UiScene` and asserts the order and the absence of a later overlap. The
quads around the title are:

```
panel  (white 70%, 520×426 at 380,289.5)
… slot bodies and cells …
title bar (black 20%, 520×28 at 380,289.5)
title glyphs  (fg_d24, black, 12 cells at y 287.5..309.5)
```

No quad after the glyphs overlaps their visible ink. At `640×502` the same
holds (`panel` y 145, glyph ink y 144..155, the title bar 145..158); the 1–2 px
top of the glyph cell is clipped by the TextBox's own `ClipMode 1`, not covered
by the panel, because the baseline is `_Position y = 19` and `fg_d24`'s ascent
puts the cell top just above the 28-pixel bar. Moving the title, or ignoring
the box's clip, would break the file's `Position`; this pass does neither.

The base already drew `Background` bodies before their widget
(`draw_order_parented`); this pass adds the regression test and records the
C++/data proof so a later change cannot re-introduce the sibling order.

---

## 3. The HUD bars

### 3.1 Binding

`0x4b2f20` (one status meter) stores:

| offset | widget | string |
|---|---|---|
| `+0xc4` | name | `Name`/`&DAT_006f25a4` |
| `+0xc8` | `points` | `0x6f3f7c` |
| `+0xcc` | `main background` | `0x6f59f4` |
| `+0xd0` | `battery bar` | `0x6f59e8` |
| `+0xd4` | `air bar` | `0x6f59e0` |
| `+0xd8` | `armor bar` | `0x6f59d4` |
| `+0xdc` | the white remainder | created from `common\whitebar.bmp`, 26×4 |
| `+0xe0` | `green dot` | `0x6f59c8` |
| `+0xe4` | `yellow dot` | `0x6f59bc` |
| `+0xe8` | `red dot` | `0x6f59b4` |

### 3.2 Per-frame write

`0x412fc0` (GameWindow update) loops the robots and calls `0x476b60` with the
status meter (`[gamewindow + player*4 + 0x15c]`) and the robot. `0x476b60`:

```
battery  ← FUN_0043e5f0(robot)  → robot+0xa8
air      ← FUN_0043e5e0(robot)  → robot+0xa4
armor    ← FUN_0043e600(robot)  → robot+0xb4 / robot+0xb0   (0.0 if max <= 0)
dot      ← FUN_00416940(robot)  → robot+0xbc, clamped to 2
```

Then `0x4b3540` / `0x4b3570` / `0x4b35a0`:

```
battery width = (int)(battery * 52.0)      // __ftol at 0x4b3556
air width     = (int)(air     * 52.0)      // __ftol at 0x4b3586
armor width   = (int)(armor   * 26.0)      // __ftol at 0x4b35d9
```

with `0x6a9020 = 52.0f` and `0x6a9024 = 26.0f`. The armour setter also gives the
white remainder `x = 49 + width`, `width = 26 - width`
(`0x4b35e6`–`0x4b3636`, constant `0x421c0000 = 39.0` is the y), which is only
coherent for a **left-anchored** fill that shrinks to the right. `x` is never
written by any of the three setters.

### 3.3 Implemented and measured

`hud::set_bar` now keeps the file's x and sets `Size`/`_Size` to
`trunc(fraction * base)`, so the fill is left-anchored and truncated. The test
`the_status_group_takes_the_reports_names_points_and_bars` measures a 0.5
battery as **26 px at x = 23**, a 0.25 air bar as **13 px at x = 23**, and a
0.75 armour bar as **19 px at x = 49** (`26 × 0.75 = 19.5`, truncated to 19).

**What remains ours:** the fractions' sources. `robot+0xa8` and `robot+0xa4` are
floats on the status object behind `FUN_0043e5f0`/`FUN_0043e5e0`; what feeds
them (the `Battery`/`AirTank` energy model) was not decoded, and the dot's
`+0xbc` producer is not traced. This port therefore still feeds component
hitpoints (battery = `Battery` components, air = `AirTank`, armor = chassis).
The geometry and truncation are the binary's; the energy model is not.

---

## 4. What is still unproven

1. **The component damage scale.** `FUN_00489620` calls the hit component's
   vtable `+0x1c` and returns its float; the armour/material arithmetic inside
   that virtual was not decoded. This port scores the damage its own model
   applies, which is the same quantity at the write but not proven equal to the
   original's per-material scale.
2. **The status fields' producers.** What aggregates `robot+0xa8` (battery bar),
   `+0xa4` (air bar) and `+0xbc` (dot) was not traced (the component classes
   `Battery` at `0x4b3dd0` and `AirTank` at `0x4b4170` are registered, but their
   contribution to those robot fields was not read). The bar geometry is
   established; the energy model is not.
3. **The exact raw-vs-effective gate.** `FUN_00417330` gates on the raw damage
   `param_5` (10.0); this port gates on the damage its model applies, which is
   the same value at our single damage point. If a weapon path ever passes raw
   and effective damage separately, the gate belongs on the raw value.
4. **The time-out end** (`+0x17c` countdown to `0x418040`/`0x418770`) is still
   not traced; the match length remains explicit config (`docs/11` §6).
5. **The title's 1–2 px top clip** is the TextBox's own `ClipMode 1` plus the
   baseline (`_Position y = 19`, `fg_d24` ascent), not a covering quad. Fixing
   it would mean moving the title or ignoring the box's clip, both of which
   contradict the file's `Position`; it is left as measured.

---

## 5. Evidence frames

All from `./engine/target/release/ra2-app --game local/game --match --seconds
30 --out DIR` on the box arena (the shipped `AI/Team1/Bot0.bot` vs
`AI/Team6/Bot2.bot`), hazards off as the presentation pass documents:

| frame | before (M8.2–M8.10) | after (M11) |
|---|---|---|
| `results.png` points cells | `0`, `0` | `31`, `32` |
| results stdout | `points [0, 0]` | `points [31, 32]` |
| title order | panel → slots → title bar → glyphs | same (the base already drew the body before its widget; the regression test now pins it) |
| bar widths at 0.5/0.25/0.75 | 26/13/19.5 at x 49/49/55.5 | 26/13/19 at x 23/23/49 |

The before run is `/tmp/ra2-match-final/results.png` (a 30 s fight from the
M8.10 pass, points still zero); the after run is `/tmp/ra2-m11-final/results.png`
(same command, points `31` and `32` in the two filled cells). The title is
identical in both because
the overdraw was the *sibling-order* treatment of the body, which this base
does not do; the measured scene order and the C++ proof are §2.
