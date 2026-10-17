# 14 — The match on screen (M8.6–M8.10)

This is the presentation pass on top of `docs/11-match-rules.md` §7. The rules
are `ra2sim::match`'s; this document is what is drawn and how to reproduce it.

## Commands

A full headless match, at the match's own `dt`, writing one PNG per named
moment:

```sh
./engine/target/release/ra2-app --game local/game \
    --match --seconds 30 --out /tmp/ra2-match
```

- `--seconds N` is both the fight length and the HUD clock's configured duration
  (the `time` box counts down from it). The default is 30.
- `--out DIR` writes `countdown.png`, `fight-first.png`, `fight-mid.png`,
  `fight-last.png` and `results.png`, prints every state transition with the
  clock, points and winners, and prints the winner.
- `--bots A.bot,B.bot` overrides the two shipped AI brains (the defaults are the
  ones the match tests use, `AI/Team1/Bot0.bot` and `AI/Team6/Bot2.bot`).
- `--arena PATH` overrides `Arenas/box.py`; the game type is `DEATHMATCH`.

The pause/quit proof, deterministic and offscreen:

```sh
./engine/target/release/ra2-app --game local/game \
    --match --seconds 10 --quit-at 4 --out /tmp/ra2-quit
```

It writes `pause.png` (HUD + `common/ingame_dialog.gib`), presses the dialog's
`quit button` through the same control hit test a mouse uses, and writes
`menu.png` from a fresh shell. The run prints the state transitions and the
`click … hits "quit button"` line.

The windowed path is the same composition, one frame per loop:

```sh
./engine/target/release/ra2-app --game local/game --match
```

`Esc` opens/closes the pause dialog; `C` cycles `watch → static → orbit → watch`;
the dialog's `resume button` resumes and `quit button` / `end match button`
return to the shell's main menu. The window could not be exercised in this
environment (no display), so its evidence is the shared offscreen path.

## What is drawn, in order

1. the duel's 3D scene (`Duel::render_to`) into the target, clearing it and
   drawing depth;
2. `UI/common/gamewindow.gib` over it, alpha-blended with `UiRenderer::render_over_to`
   (load, never clear, no depth);
3. at `MatchState::Results`, `UI/common/matchresults.gib` replaces the HUD (its
   own note: *"replace the game window/status bars with this match results
   window"*);
4. when paused, the HUD **and** `UI/common/ingame_dialog.gib` (root `in game
   popup win`) are resolved into one scene, so the dialog is centred over the
   live frame.

The HUD and the results screen always resolve at the app's design size,
**1280×1005**; the renderer letterboxes that scene into the window or PNG target
through `ra2_render::ui::fit`.

## The HUD binding

`ra2-app::hud` loads `common/gamewindow.gib` and instantiates
`common/statusmeters.gib`'s `status group` into each of the screen's own
`player1 status` … `player4 status` groups. Nothing about the four positions is
invented: the groups' `Position`/`Size`/`Alignment` come from the layout.

`hud(screen, &report, state, clock, countdown_left)` returns a `Hud` with the
visible fields, and is covered by unit tests. It binds:

| screen field | value |
|---|---|
| `playerN status` / `name` text | `MatchReport::robots[N].bot_name` |
| `playerN status` / `points` text | `MatchReport::robots[N].points` |
| `battery bar` (52 px) | `robots[N].battery` |
| `air bar` (52 px) | `robots[N].air` |
| `armor bar` (26 px) | `robots[N].armor` |
| `time` text | fight clock as `m:ss`, counting down from `MatchReport::duration_limit` (up when the match has "No limit") |
| `count number` | `3`, `2`, `1` during the recovered 4.0 s countdown |
| `camera textbox` | `watch` / `static` / `orbit` (`hud::set_camera`) |

Visibility is the data's own note: `countdown group` is shown during
`Countdown` and hidden otherwise; `time group` is hidden during `Countdown` and
shown from `Fight` on. Slots with no robot (two bots in a four-slot HUD) are
hidden. `skip group` is hidden because the arena intro is not implemented.

**The fill rule is the binary's; the values are still ours.**
`0x4b2f20` binds the meter widgets (`+0xc8` points, `+0xd0` battery, `+0xd4`
air, `+0xd8` armour, `+0xdc` the white remainder, `+0xe0`/`+0xe4`/`+0xe8` the
green/yellow/red dots). The per-frame write is `0x412fc0` → `0x476b60`:
battery ← `FUN_0043e5f0` (status field `+0xa8`), air ← `FUN_0043e5e0`
(`+0xa4`), armour ← `FUN_0043e600` (`+0xb4 / +0xb0`, clamped at 0), dot ←
`FUN_00416940` (`+0xbc`, clamped to 2). `0x4b3540`/`0x4b3570`/`0x4b35a0` then
set the width to `(int)(fraction * 52.0)` for battery/air and
`(int)(fraction * 26.0)` for armour with `__ftol` — the fraction is truncated,
not rounded. `0x4b35a0` also places the white remainder at `x = 49 + width`
with width `26 - width`, so the fill is **left-anchored and shrinks to the
right**, keeping the file's x (23, 23, 49). What `+0xa8`/`+0xa4` are made of
(the `Battery`/`AirTank` energy model) was not decoded, so the fractions this
port feeds are still the live component hitpoints: battery = the robot's
`Battery` components, air = its `AirTank` components, armor = its chassis. A
robot with no battery or air tank reports 0. The mapping is named in
`RobotReport`'s fields and in `hud.rs`; the geometry is now the binary's.

## The camera

`ra2.cfg`'s `DefaultCamera` is read with `ra2fmt::cfg`; the three strings are the
binary's (`0x4759c0`): `watch`, `static`, `orbit`. `C` cycles them in the window.

- **watch** — *our rule*: focus is the mean position of every live robot; the
  distance is `clamp(largest pair separation × 0.85 + 5, 6, 42)` along the
  high three-quarter direction `(0.55, 0.85, 0.55)`; the camera aims at the
  focus. **The C++ `watch` implementation at `0x4770a0` was not established.**
- **static** — the arena script's own first `AddStaticCamera` position, aimed at
  the arena centre. `ra2fmt::arena` parses the position but not the camera's
  rotation/FOV, so aiming at the centre is our reading, not the file's angles.
  An arena with no static camera gets a fixed high three-quarter view of the
  arena centre (printed as the fallback).
- **orbit** — *our rule*: one revolution every 24 s around the live midpoint at
  `clamp(separation × 0.85 + 6, 7, 44)`, elevated by 55% of the distance.
  `0x476f00` was not traced.

## Results

On `MatchState::Results` the app loads `common/matchresults.gib` and calls
`ra2ui::results::fill` with `MatchReport::results()` (`result_cell.gib` is the
template). The run prints `results screen: N of M bot(s) placed`; the test reads
the placed tree back and asserts that slot 1 and slot 2 carry the report's
`points`, `bot name` and `team name`, that slots 3/4 stay empty for a duel, and
that the scene has a real glyph run (not a blank panel).

The title's order is pinned by
`match_mode::tests::the_results_title_is_not_covered_by_the_panel`: the panel's
`+Background` body is drawn first, then the title's bar, then its glyphs, and no
later quad overlaps the ink. The body is an attachment, not a sibling; its
serialized `Position`/`Size` are against the decorated widget's parent (the
result slots' 21,35 shadow against the 22,36 slot proves it). The C++ evidence
and the measured frames are in `docs/19-match-fidelity.md` §2.

## Pause and quit

`Esc` opens `UI/common/ingame_dialog.gib`, whose root is `in game popup win` —
the in-game pause popup, not one of the out-of-match dialogs. Its own notes name
`resume button`, `end match button`, `options button` and `quit button`;
resume closes the dialog, quit/end-match return to the shell's already-booted
main menu. The original shows a confirmation dialog before quitting
(`0x487d50` mentions one); this port returns directly, which is named as a
difference. Options is not wired.

## Unproven / deliberately not done

- **The meter fill's sources** at `0x4b2f20`/`0x476b60`: the widget mapping and
  the fill geometry are established (`docs/19`); what the status object's
  `+0xa8`/`+0xa4` are made of is not, so the fractions above are ours.
- **The `watch` framing** (`0x4770a0`) and **the orbit parameters**
  (`0x476f00`): ours, stated above.
- **The static camera's rotation/FOV**: `ra2fmt::arena` keeps the position and
  name only; the aim is at the arena centre.
- **The pause dialog's identity**: the popup is identified by its root
  (`in game popup win`) and its notes; the confirmation dialog the quit path is
  said to show in the original is not reproduced.
- **The countdown's per-second schedule inside the 4.0 s**: the 4.0 s timer and
  the `3` sample are data; `3 → 2 → 1` with the last second holding `1` is our
  reading.
- **Which robot gets which status corner**: `0x4749c0` loads the four
  `playerN status` groups, but the assignment was not traced. Robot 0 goes in
  `player1 status`, robot 1 in `player2 status`, and 3/4 are hidden for a duel.
- **The text layout correction**: `ra2ui` now treats a `TextBox`'s `Alignment`
  as text alignment (the box keeps its authored `Position`) and places a
  `TextBox`/`Group`'s `Text` child by its baseline. This is measured against the
  shipped title/status boxes (see the module docs and
  `a_textbox_is_a_box_and_its_text_is_a_baseline`), and it moves the corpus's
  off-screen count from 204 to 155.
- **The evidence decision**: if neither chassis is destroyed by `--seconds`,
  the offscreen run ends the fight through `plus.eliminatePlayer`'s own request
  (the state-3 path `flextop.py` uses) for the robot with fewer hitpoints left.
  It prints that line; it is a demo script, not a match rule. If a real finish
  arrives before the scheduled midpoint, `fight-mid.png` is written from the
  final pose and the run prints that it did.
- **The demo runs with `RA2_HAZARDS=0`**: the same switch the match tests use,
  so the box fight stays in the middle of the arena. Hazards are an arena
  feature already implemented; the offscreen evidence does not claim otherwise.
- **The windowed loop** was not run here (no display); it compiles and shares
  every drawing function with the offscreen path.
