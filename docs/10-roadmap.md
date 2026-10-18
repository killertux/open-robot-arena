# 10 — Roadmap to a finished game

M1–M6 are landed (M6 for Linux). This is the plan for the rest: from a verified
engine that renders and simulates, to a game a player can sit down and play with
their own copy of *Robot Arena 2*.

**Delivered 2026-10-17:** M7–M10 are in the client and M11/M12 are the measured
pass and the release. The gate is **306 tests**, fmt/clippy clean, the Linux
tarball smoke-tested; the honest remaining gaps are the ones each milestone
marks ◐ below and in `README.md`'s "what does not work yet".

It is ordered by **risk**, not by comfort: the things that can still invalidate a
design are first, and each has a "retire by" action with a measurement.

---

## 0. What "finished" means

**v1.0** is a native client on **Linux, macOS and Windows** where a player who
supplies their own game directory can:

1. launch and navigate the shell with mouse and keyboard, with music and sound;
2. build a robot in the workshop — chassis, components, wiring, paint — and save
   and reload it;
3. fight an exhibition match against the shipped AI, in any arena and game type,
   with the HUD, damage, scoring and a results screen;
4. run a season in the team HQ — select a team, enter events, earn points and
   prizes, repair broken robots — and have that career persist across runs;
5. do all of it with **the game's own data and its own Python**, not a
   paraphrase, and with every unresolved difference *named and measured*.

Three demoable tiers:

| Tier | Milestones | What you can do |
|---|---|---|
| **v0.5 — playable slice** | M7 + M8 | Launch, click through the shell, fight an exhibition match against the AI, see the result. |
| **v0.9 — content complete** | + M9 + M10 | Build and paint your own bot, save it, and run a season with prizes and repairs. |
| **v1.0 — release** | + M11 + M12 | The fidelity pass, the oracle, macOS/Windows, packages, CI. |

---

## 1. Baseline (measured)

What the plan can rely on, and where it is written down:

- **Parsers and data** — `.gmf` 181/181, 57 robots, 116 `.gib` screens (4,348
  widgets, one root each), nine `.giff` faces that all read, CFL3 archives,
  `Armor_Definitions.txt`, `Components.cfz` (131 components), `Events.txt` (25
  events), `Teams/Teams.txt` + `AI/Teams.txt` (16 teams), `ra2.cfg`.
- **The shell** — layout, draw order, input, the workshop's tabs and steps, the
  lists (armour, components, arenas, events, standings, results, match results),
  `+Background`/`+Caption` bodies, proxies, the shell's Python (`plus`,
  `giScript`, `Gooey`), the screen graph walked by the screens' own notes
  (`ra2-shell`).
- **Physics and scripts** — Jolt behind `PhysicsBackend`, the component-tree
  robot with motors/pistons/detach/buckle, contact damage, 14 arenas, the
  shipped AI, SmartZone hazards, the nav map, `ra2-sim`/`ra2-duel`.
- **Packaging** — the Linux tarball, launcher, installer, `tools/check.sh`.
- **267 tests**, fmt/clippy clean, `tools/check.sh` PASS.

### Where the match rules actually live (corrected by M8.1)

The first draft of this plan assumed the rules were mostly script-side. The
M8.1 recon (`docs/11-match-rules.md`) measured otherwise, and the plan follows
the measurement:

- **Game types are declared in data** — `DEATHMATCH`, `BATTLE ROYAL`,
  `TEAM MATCH`, `TABLETOP`, `KING OF THE HILL` in `Arenas/*.py`'s
  `game_types` (filtered by `Arenas/__init__.py`'s `enumAvailable(type)`).
- **KOTH's zone scoring is script** — `king.py` is the one arena that scores,
  with `plus.addPoints`, `plus.getGameType`, `plus.isMatchOver`,
  `plus.isDefeated` and sounds.
- **Every win condition is C++** — `FUN_00418040` (last robot / last team),
  `FUN_00418770` (points finish), with the score in a robot field at `+0x20`.
  Deathmatch, Battle Royal and Team Match have no script rule at all.
- **The match shell is C++** — HUD (`0x4749c0`), countdown (`0x477480`,
  4.0 s), per-frame update (`0x412fc0`), results handoff (`0x4170e0`), and the
  `isMatchOver` byte (`0x416450`).
- **Our `plus` state is inert** — `game_type`, `players`, `paused`, `over` are
  never written outside construction, `Arena.tickInterval` is missing, and
  zone contents are empty. KOTH's `Tick` currently raises and is swallowed.
- Events/prizes are `Events.txt`, the HUD and dialogs are `.gib`, settings are
  `ra2.cfg`, sounds/music are files.

So M8 is **engine state + a C++ match shell reproduced from addresses + a
script API that actually carries match state**, then presentation. It is a
larger job than the first draft said, and M8.1 is done precisely so it is
estimated against evidence.

The oracle is also settled: the original runs under the installed flatpak
Wine 9.0 and its frames are captured in `local/oracle/` (`docs/12-oracle.md`),
so A/B comparisons are available now rather than at M11.

---

## 2. Rules that do not change

Every milestone below is held to the project's existing method:

1. **The data is the specification.** Run the game's own scripts; bind to the
   game's own tables. Never paraphrase a screen, an AI or a rule.
2. **Headless first, picture to confirm.** A feature is done when a headless run
   produces frames (and a word you can read), not when a window looks plausible.
3. **Two renderers where they overlap.** `ra2-render`'s wgpu path and
   `tools/ui_compose.py` stay in agreement on the shell.
4. **Unresolved is named with its measurement**, never approximated.
5. **No game files in the repository**; `local/` and `engine/target/` stay
   untracked. **No unsafe FFI** — a capability we cannot reach through the safe
   bindings stays named (F6).
6. The gate is `tools/check.sh` (fmt → clippy → tests → build → package smoke)
   plus whatever new baseline a milestone adds.

---

## 3. Cross-cutting workstreams

These are not milestones; they are threads several milestones pull on.

| # | Workstream | What it is | Needed by |
|---|---|---|---|
| **W1** | ✅ **Writers** | `bot::write` (1.12; 57/57 byte-identical), `team::write` (both tables byte-identical), `cfg::read/write` (byte-identical). See `docs/13-audio-and-writers.md`. | M9 (save a bot), M10 (career), W3 |
| **W2** | ✅ **Audio** | `ra2-audio` decodes and levels the MP3/WAV set; the client switches music per screen, plays the click sound and drains `plus`'s handle queue at the match camera. Physical playback and crowd fades are named unverified. | M7, M8 |
| **W3** | ✅ **Settings** | `ra2fmt::cfg` round-trips byte-identically; startup uses the display/audio/camera keys and the shipped options screen edits them. The resolution dropdown is read-only (the data has no list). | M7 |
| **W4** | ✅ **The oracle** | The original runs under the installed flatpak Wine 9.0; the main menu is captured at 1024×768 and 1280×1024, and the harness measures **1.439/255** MAE (pointer-masked) against our render. `docs/12-oracle.md`, `tools/oracle/`. | M8, M11 |
| **W5** | **Match-rule recon** | The C++ match shell: the match object, spawn order, countdown, camera modes (`watch`/`chase`/`top`), HUD field updates, end-of-match handoff, points for damage/KO/zone. Read it from the binary (REA) and from `ReadMe.txt` + the scripts. | M8 |
| **W6** | **Pacing and performance** | A fixed 50 ms update (`ra2.cfg`) with the physics at a fixed step; frame pacing; the UI at display resolution; no per-frame allocation in the hot path. | M8, M11, M12 |

---

## 4. Milestones

Each milestone has a **demo** (what you can see), **work items** (numbered, with
where it lands), **acceptance** (measurable, headless), **risks**, and a **size**
in work blocks (one block ≈ one focused session; ranges are honest, not
commitments).

### M7 — The interactive client

**Goal.** The shell becomes an application: a window, a real loop, input, sound,
and the settings file.

**Demo.** Click `TEAM HQ`, `BOT LAB`, a chassis step and back, in a window, with
menu music and button sounds; quit and relaunch with the window where you left it.

| # | Work item | Lands in |
|---|---|---|
| M7.1 | ✅ `ra2-app` binary: `winit` window + `wgpu` surface, reusing `ra2-render`'s UI pipeline (offscreen path preserved for tests) | new crate `engine/crates/ra2-app` |
| M7.2 | ✅ Input: mouse position (the shell's `PtInRect`/hover/click order), keyboard (Esc, arrows/Tab for focus), a bindings table; hit-testing already exists in `ra2ui::input` *(mouse mapping, Esc and the match's driving keys are in — `W`/`S`/`Up`/`Down` throttle, `A`/`D`/`Left`/`Right` steer, release centres, `C` cycles; arrows for shell focus and the general bindings table remain)* | `ra2-app`, `ra2ui::input` |
| M7.3 | ✅ The loop: fixed 50 ms update, present at vsync, resize, DPI, fullscreen/windowed *(the fixed tick, Fifo present, resize and DPI are in; a fullscreen toggle remains)* | `ra2-app` |
| M7.4 | ◐ Screen stack: move `ra2-shell`'s `nav` into a `Flow` the app drives; keep the scripted route runner as the headless entry *(the app and the headless path now drive one shared frame loop; `nav` has not yet moved into a `Flow`)* | `ra2-shell::nav` → shared |
| M7.5 | **done** — Audio: `ra2-audio` decodes/plays MP3+WAV, 2D/3D, maps music to the three screen groups and applies the `ra2.cfg` volume law; the client switches tracks per shell screen, plays `Sounds/button_click.wav` on an activating shell click, and drains `ra2script`'s sound requests at the match camera each tick (including the arena's own `DoIntro` when `ArenaIntros=1`) *(a physical output device was not exercised; crowd fades remain deferred)* | `ra2-audio`, `ra2script::plus`, `ra2-app::audio` |
| M7.6 | **done** — Settings: `ra2.cfg` read/write is byte-exact, startup reads `FullScreen`/`FSDisplayWidth`/`FSDisplayHeight` and the volume/mute keys, and the shipped `common/options.gib` + `audio_options.gib`/`video_options.gib` edit the file (volume, mute, fullscreen, camera and the other exposed keys; the resolution dropdown is read-only because the data carries no list) *(no windowed run in this environment)* | `ra2fmt::cfg`, `ra2-app::options` |
| M7.7 | ◐ Text entry: the workshop's name box edits and saves with backspace/enter; the team-name box and the clipboard path remain | `ra2-app` |
| M7.8 | ✅ Cursor: **no cursor art ships** in the data, so the OS cursor is used; hover state is mapped and held for the missing button art, which the corpus leaves blank. Recorded rather than invented | `ra2-render`, `ra2ui` |

The windowed path is built but was not exercised in this environment; the
shared loop's offscreen half and its tests are the evidence here.

**Acceptance.** A headless test drives the same `Flow` with a click script and
produces the same frames as the windowed path (the loop is one code path); a
windowed smoke on this machine opens, navigates and exits cleanly; the audio
layer reports what it played; `ra2.cfg` round-trips byte-identically for the
shipped keys.

**Risks.** `winit`/`wgpu` on this machine (low); MP3 decode (low, `rodio`);
audio device absence in CI (accept a null device and say so).

**Size.** 3–4 blocks.

---

### M8 — The match

**Goal.** From the arena picker to a result screen: two to four robots, the
HUD, damage, scoring, the end of the match.

**Demo.** Pick `Combat Arena`, two AI bots, `DEATHMATCH`; watch a fight with the
shipped HUD; see the result cell filled from the match, then return to the
menu. Then the same with `KING OF THE HILL` on `hilltop` — the zone score ticks
in the game's own script.

| # | Work item | Lands in |
|---|---|---|
| M8.1 | **done** — **W5 recon**: the match object, countdown, HUD writes, end handoff, per-type scoring, KO rules, the `plus` gap table | `docs/11-match-rules.md` |
| M8.2 | **done** — `Match` state machine (`Setup → Countdown → Fight → Over → Results`) in `ra2sim::r#match`, driven by `MatchConfig` (game type, robot entries, `EventNumBots`, explicit duration/step) | new `ra2sim::match` |
| M8.3 | **done (state)** — `game_type`/`players`/`paused`/`over`/scores written; `Arena.tickInterval`; zone + phantom contents; `addPoints`/`getPoints`; `eliminatePlayer` state 3; `zap` visual state; `Arena.Tick` errors surfaced. Audio remains M5 | `ra2script::{plus,arena}`, `ra2sim` |
| M8.4 | **done (rules)** — last robot (`FUN_00418040`), last team and team totals (`FUN_00418770`), the `+0x20` score, 4.0 s countdown, match clock. Damage→points (`FUN_004175b0`) remains unproven and is not invented | `ra2sim::match` |
| M8.5 | **done** — spawn on the arena's start points in ID order with the model's facing; `EventNumBots` (2 or 4, anything else 4) for Battle Royal, four distinct spawns | `ra2sim::match` |
| M8.6 | **done** — the match loop: fixed-step physics, the arena `Tick` at 0.25 s, the AI cadence, damage events, defeated/KO and elimination, the countdown and the clock; the live 3D frame composes into an arbitrary target (`Duel::render_to`) | `ra2sim::match`, `ra2sim::duel` |
| M8.7 | **done** — HUD: `gamewindow.gib` + `statusmeters.gib` placed and driven by live state (four status groups, names, points, the three bars, the clock, the countdown, the camera name), with `hud()` unit tests | `ra2ui` + `ra2-app` |
| M8.8 | **done** — camera: `watch`/`static`/`orbit` (the binary's own vocabulary) from `ra2.cfg`'s `DefaultCamera`; `C` cycles in the window. A player-controlled match starts on the driving **chase** camera (our choice) and cycles `chase → watch → orbit`. The C++ `watch`/`orbit` framing is named as ours | `ra2-app`, `ra2sim::duel` |
| M8.9 | **done** — results: `common/matchresults.gib`'s four slots filled from `MatchReport::results()` through `ra2ui::results`, the placed tree asserted, then the return to the menu | `ra2ui::results`, `ra2-app` |
| M8.10 | **done (pause/quit)** — `Esc` opens `common/ingame_dialog.gib` (`in game popup win`); resume and quit, with a deterministic `--quit-at` run writing `pause.png`/`menu.png`. The arena intro remains unimplemented (`skip group` is hidden) | `ra2-app` |
| M8.11 | **done (play)** — the shipped exhibition setup's `start game button` (note *"start the game"*) starts a player-controlled `Match`: the arena is the picker row (`Arenas/box.py` when none), the game type is the Game Settings tab's selection (`DEATHMATCH` when unopened, printed as the default), hazards follow the arena's `hazard checkbox`, and robot 0 is `Teams/Team0/Bot0.bot` against `AI/Team1/Bot0.bot`. `Match::step_with(&[Option<DriveInput>])` lets the window drive robot 0 (`None` = shipped AI), W/S/A/D and the arrows drive, and `Continue`/quit return to the setup screen. `--play --drive "…" --seconds N --out DIR` writes `play-setup.png`/`play-drive.png`/`play-results.png`. Robot/team selection and the match-time dropdown remain unwired (`docs/21-play.md`) | `ra2sim::match`, `ra2-app` |

Landed: the exhibition screen is playable. Its Arena tab's rows select the
arena, the Game Settings tab's option group selects the game type, and Start
Game builds the match; the window drives robot 0 with `Match::step_with`. The
headless evidence is `--play --drive "0-2:1,0;2-4:1,1" --seconds 8 --out DIR`
(the run in `docs/21-play.md` moved `C.R.U.S.H.` 23.29 m and reached `Results`
with `C.R.U.S.H.` 40 – Roly Poly 38). The robots tab, the match-time dropdown,
arena scrolling and the arena→game-type filtering remain unwired and are named
there.

**Acceptance.** Headless: a scripted `Match` on the box arena, two AI bots,
produces a winner and a filled result screen; the score sequence for
`hilltop` matches `king.py`'s 10 points per 0.5 s in the zone (a test against
the script's own constants); a Battle Royal with `EventNumBots=4` spawns four;
the HUD frames are read (a word, a bar, a clock). Windowed: the same run.

**Risks.** **The match shell is the biggest unknown** — retire it with M8.1
before writing M8.2. The C++ may own more of the flow than the scripts suggest.
Mitigation: build M8.2 against a minimal flow first (spawn → fight → result) and
let M8.1 refine it. Also: the AI may need `plus` calls the current run never
made (they are recorded — read the recorded drops).

**Size.** 5–7 blocks.

---

### M9 — The workshop

**Goal.** Build, paint and save a robot.

**Demo.** New bot → chassis → armour (the four real plates) → components from
the palette (attach, remove) → wiring (board + wiring) → paint (a texture from
the library, decals) → save → reload it → `Test Robot` in an arena.

| # | Work item | Lands in |
|---|---|---|
| M9.1 | ✅ **W1 `bot::write`**: 1.12, **57/57** semantic and byte-identical, measured over `Robot Designs/`, `AI/` and `Teams/` | `ra2fmt::bot` |
| M9.2 | ✅ Component placement: attach at a mount point with the F1 rule (already the renderer's), remove, undo; the palette's filtering and selection already exist, and `--build` wires a palette click to `Build::attach` *(the baseplate drag placement tool remains; see `docs/17-workshop-ui.md`)* | `ra2ui::workshop`, `ra2-app` |
| M9.3 | ✅ Wiring: the shipped `control grid` is filled with `control_cell.gib`; `new analog`/`new switch`/`new button` + a free cell adds a control (`ControlBoard::add`), `delete button` removes it, and a component + an action from the model's matrix binds it (`Build::bind_action`, `bind_checked`). Saving and re-reading the board is identical; a wrong action for the kind is refused. Click-to-place then click-to-bind (no drag) | `ra2ui`, `ra2-app` |
| M9.4 | ✅ Paint: the texture library and custom textures lists, `Paint::fill_group` on a face group, live preview, save/reload; decals/surface layers/texture-entire stay model-only | `ra2ui`, `ra2-app` |
| M9.5 | ✅ `Test Robot`: `--build` saves the bot and starts M8's `Match` on the box arena against a shipped AI bot, shows it with M8's presentation and reports the damage (reported, not applied) | `ra2-app` |
| M9.6 | ✅ The workshop's own state: `Save As` to `Robot Designs/`, `Open`, `Import`/`Export` of bots and custom textures, the name box and the round-trip | `ra2-app` |
| M9.7 | ✅ The bot's stats panel: weight (`Armor_Definitions` + mass rules), hitpoints, cost — from the same tables the sim uses | `ra2ui`, `ra2fmt` |

**Acceptance.** Round-trip test over all 57+ stock bots; a built bot's saved file
reopens with the same component tree, mass and hitpoints as the live one; a
painted chassis renders its decals in the same face groups the sim uses; the
import/export path is the shipped `Robot Designs/`.

Landed: `ra2-app --build` drives the shipped bot lab and a live `Build` —
palette click → `Build::attach`, remove/undo, armour plate, texture + face group
→ `Paint::fill_group`, Save As/Open to `Robot Designs/`, `Test Robot` through
M8's `Match`. The scripted build's manual run is **3 parts, 89.1 kg, 960 hp**
in the model, the reload and `ra2sim::rig` alike. Wiring is editable in the
shipped `control grid` (click-to-place, click-to-bind); the saved board is
identical on re-read. The commands, frames, editable/read-only split and what is
unproven are in `docs/17-workshop-ui.md`. The repair bench and its clock are in
`docs/18-hq-ui.md`.

**Risks.** The `.bot` writer's version quirks (mitigate: round-trip first, then
write); the wiring model is the least-read part of the format (M9.3 is where it
gets read).

**Size.** 5–7 blocks.

---

### M10 — The meta-game

**Goal.** A career: teams, a league season, events, prizes, repairs.

**Demo.** Pick a team, enter the 4-bot event, win rounds, earn points, collect a
trophy, repair a broken component, and come back next launch with the same
record.

| # | Work item | Lands in |
|---|---|---|
| M10.1 | ✅ **Season state**: the 15 ints are the fifteen AI teams' league points; the record's other career fields are named in `docs/16-career.md`, with the unproven ones listed | `docs/16-career.md` |
| M10.2 | ✅ `career` model: teams, roster, points, money, events entered, prizes, repairs, season phase; `W1` writes it where the game does | new `ra2sim::career` (or `ra2game`) |
| M10.3 | ✅ Team HQ flows: the shipped `teamhq.gib` and its four tabs, robot inventory (`hq_robots` — `bot_cell.gib`, not the `robot_cell.gib` its note names), league/season grids from the career's points (S2's fills), history/prizes (`hq_history`), event schedule (`hq_events`, with the completed/in-progress/disabled cells and the bracket state) | `ra2ui::hq` + `ra2-app::hq_mode` |
| M10.4 | ✅ Events: `Events.txt` → the event's arena, its arena script's own game type, the **16-entrant bracket** (tiers = rounds, first setting = bots per match, 1 winner; player match 0 slot 0; deterministic drawn opponents), each round through M8's `Match`, `common/matchresults.gib`, the next-round screen, the award (`event_winner.gib`) only when the event ends, the career's per-round points and the final-win trophy; `--hq --simulate-event N` and `--hq --simulate-season` are the headless runs | `ra2fmt::event`, `ra2sim::career`, `ra2ui::hq`, `ra2-app::hq_mode` |
| M10.5 | ✅ Repairs: `teamhq/repairs.gib` lists the career's damaged robots/components and the repair clock; `repair button` advances `Repair::elapsed` (`Career::advance_repair`, six-second UI tick) and a full bar clears the entry and saves the career. No cost is charged (0). The component-level `Team+0x12c` list and the `damage.tga` fractured icon remain unproven; the `--report-damage` knob is the evidence/test input because the shipped match reports zero damage here | `ra2ui::repairs`, `ra2sim::career`, `ra2-app::hq_mode` |
| M10.6 | ✅ Money/points: damage scores in every type except KOTH (the `0x4176f2` string test), through `FUN_004175b0`'s non-KOTH branch with the 10-damage gate and `trunc(score + effective)`; teammate hits score 0; KOTH stays `king.py`'s. Per-round payment is `robot_points/200 + 160/rounds` | `ra2sim::match`, `ra2sim::career` |
| M10.7 | ✅ The career save/load, versioned, with a "your game directory" location | `W1`, `ra2-app` |

**Acceptance.** A headless career run: start a season, fight the first event's
rounds, collect a prize, repair, save, restart, and the table/prizes/roster are
identical. The event list comes from `Events.txt` (25 events, the shipped
tiers); a save round-trips.

Landed: `--hq` boots the shipped HQ; `--simulate-event N` plays an event's
**whole bracket** (2 or 4 rounds, 2 or 4 bots per match, deterministic
opponents) and `--simulate-season` plays all nine events. A win advances and
redraws the field, a loss ends the event but not the season, and the trophy is
stored only on the final win. The evidence run is **1400 points, rank 1, nine
prizes (eight trophies + the golden sawblade), one lost event**, with
`season.png`, `bracket.png`, `round.png`, `champion.png` and `season-final.png`.
`teamhq/repairs.gib` lists the damaged robot, its 30-second clock and the
shipped repair button, and a full bar clears the entry and saves. The
`--report-damage` knob remains an explicit input because the shipped match
reports no damage here. Commands, frames, the bracket addresses and the
unproven items are in `docs/16-career.md` §3a and `docs/18-hq-ui.md`.

**Risks.** The season save format is partly unread (M10.1) — if the original's
format cannot be recovered, store the career in a documented sidecar and say so;
do not fake the original file. Prize scene geometry (`event_results.py`).

**Size.** 5–7 blocks.

---

### M11 — Fidelity and content pass

**Goal.** Close the measured gaps and prove it against the original.

| # | Work item | Lands in |
|---|---|---|
| M11.1 | **W4 the oracle**: Wine under `xvfb` (or captured fixtures) for A/B frames: the main menu, a bot lab screen, one arena, one match. Record the differences. | `tools/`, `docs/` |
| M11.2 | The traction survey: **28 → 39 / 45** after the contact-layer fixes (the robot's own bodies share a collision group; internal control/power fittings add no hull). The six left are four rest-pose weapon/mount cases, one chassis-child placement case and one curved path — measured in `docs/06` F4 | `ra2sim` |
| M11.3 | F9's crush curve **closed as a testable linear curve** (four samples 0.0927/0.1855/0.2782/0.3709 m), with the collider re-bake named as the residual (`UsedByConstraint`); F12's mass rule re-checked (`0x5c6820` is a vtable slot; the file's zero is taken at face value) and the two anchors kept; F7's per-material damage scale **not established** — `+0x1c` is apply-damage, the four armour plates measure ×1.00, and the next read is the `MaterialObject` `+4` float | `ra2sim` |
| M11.4 | F6 friction combination — checked `oxijolt` **1.1.0+jolt-5.6.0**: no combine hook; stays named, and the survey does not need it | `oxijolt` |
| M11.5 | The shell's residues: title boxes narrower than the advance extent; the decorator-layout question (S7); the spin motor's rest pose (F1) | `ra2ui`, `ra2render`, `ra2sim` |
| M11.6 | Content sweep: every arena in every game type, every stock bot in a match, every event entered — the counts as tests | `ra2sim`, `ra2ui` |
| M11.7 | Performance: the fixed step, the UI at resolution, a measured frame budget on the reference machine | `ra2-app`, `ra2sim` |

**Acceptance.** The oracle's screen pairs are within a stated per-channel
difference (the project's existing 0.24–0.42 standard for the shell); the
traction survey improves and the number is written down; every named residue is
either fixed or has a *newer* measurement.

**Risks.** Wine may not run the game here at all (D3D8); the fallback is
captured fixtures and an honest note. Physics tuning can regress the duel — the
deterministic ram is the tripwire.

**Size.** 4–6 blocks.

---

### M12 — Release engineering

**Goal.** A package a stranger can install on each platform.

| # | Work item | Lands in |
|---|---|---|
| M12.1 | ◐ macOS: `.app` bundle, `libpython3.12.dylib`/Jolt, Metal, notarisation notes, arm64 — `packaging/macos/bundle.sh` and the notes exist; **the script was never run and no macOS artifact exists** | `packaging/`, `docs/09` |
| M12.2 | ◐ Windows: MSVC build, `python3.12.dll`, DX12, portable zip + launcher (or MSI) — `packaging/windows/zip.sh` writes the layout; **never run, no `.exe`, no zip, no `run.cmd`** | `packaging/`, `docs/09` |
| M12.3 | ◐ Linux: Flatpak and AppImage recipes — Flatpak manifest on runtime 24.08 (CPython 3.12), first-run portal picker, generated `cargo-sources.json`, AppImage `AppRun` + build script, both with desktop/icon/metainfo. **Statically checked only: not built — `flatpak-builder`, the 24.08 SDK/runtime and `appimagetool`/`linuxdeploy` are missing.** The tarball remains the release | `packaging/`, `docs/08` §6 |
| M12.4 | ◐ CI: `.github/workflows/ci.yml` on Linux (fmt, clippy `-D warnings`, tests, build, `package.sh --no-smoke`, artifact upload), plus advisory macOS/Windows builds (`continue-on-error`). **Not yet run on GitHub**; `check.sh` itself cannot run without the game, so the workflow uses the individual steps | `.github/` |
| M12.5 | Versioning, changelog, release notes, a `--version`, and the "supply your own game" first-run flow | `ra2-app` |
| M12.6 | Docs: the player guide, the troubleshooting page (Python, drivers, game files), the fidelity statement | `docs/` |

**Acceptance.** `tools/check.sh` green on Linux; the tarball unpacks and runs;
macOS/Windows artifacts exist **and** were run by a human or CI on that platform
(otherwise they are marked unverified, as `docs/09` does now).

**State after M12.1–M12.4 (this round).** The Linux tarball now ships
`ra2-app` (with `ra2-audio` and `ra2script` linked in), the launcher and
`.desktop` start it, and the smoke checks the client's main-menu frame against
an independent `uishot` render while keeping one `ra2-shell` route run. That is
the only platform claim in this table with a measurement behind it. The macOS
and Windows scripts and CI build jobs are marked but unverified; a green
advisory build would still not be a packaged, run artifact. M12.3's Flatpak
and AppImage recipes are complete and statically checked, but **not built**:
this host has no `flatpak-builder`, no 24.08 SDK/runtime and no
`appimagetool`/`linuxdeploy`, so the tarball is still the release.

**Size.** 3–5 blocks (plus whatever the untested platforms cost).

---

## 5. Sequencing and critical path

```
M7 client ──► M8 match ──► M9 workshop ──► M10 meta-game ──► M11 fidelity ──► M12 release
     │            │             │                │
     └─ W2 audio ─┘             └─ W1 writers ───┘
     └─ W3 settings             (bot, team, cfg)
                   └─ W5 recon (first item of M8)
```

- **Critical path:** M7 → M8 → M10 → M12. M9 can start once W1's `.bot` writer
  exists; it need not wait for M8.
- **Can be parallelised** (different trees, no shared seam): W2 audio, W1
  writers, W5 recon, and the M11 oracle setup. Use worktrees and merge; the
  collision points are `ra2script::plus` (M7.5/M8.3) and `ra2ui` (M8.6/M9/M10).
- **Order inside a milestone:** read the data first (a `docs/02` entry), then the
  headless model + test, then the window/eye pass.

---

## 6. Risk register

| # | Risk | Likelihood / impact | Retire by | When |
|---|---|---|---|---|
| R1 | ~~The match shell owns more than the scripts suggest~~ | retired | M8.1 recon (docs/11) + a working match | done |
| R2 | The `.bot` writer cannot reproduce every shipped file | med / high | round-trip all 57 bots before any UI work (M9.1) | M9 start |
| R3 | The career save format is not recoverable | med / med | M10.1 read; documented sidecar if not | M10 start |
| R4 | ~~No Wine ⇒ no A/B oracle~~ | retired | flatpak Wine 9.0 runs the original; `tools/oracle` measures | done |
| R5 | The AI/arena scripts call `plus` APIs not yet implemented | med / med | the recorded drops from a real run are the list | M8.3 |
| R6 | Jolt's friction law (F6) differs and cannot be hooked | high / low-med | accept and keep named; the oracle says how much it shows | M11 |
| R7 | macOS/Windows toolchains (Jolt, Python, wgpu) | med / med | a CI build early, not late | M12 start |
| R8 | Performance on the reference machine at display resolution | low / med | W6 budget measured in M8, enforced in M11 | M8/M11 |
| R9 | The 27/45 traction gap is deeper than placement (contact model) | med / med | M11.2 with the oracle | M11 |

---

## 7. Verification per milestone

Each milestone adds one baseline and keeps the old ones:

| Milestone | New baseline (kept as tests or fixtures) |
|---|---|
| M7 | The windowed loop and the headless loop produce the same frames for a click script; `ra2.cfg` round-trip; the audio report. |
| M8 | A scripted match reaches a winner and a filled result screen; the King-of-the-Hill score sequence against `king.py`'s constants; four-bot Battle Royal spawns four; HUD fields read. |
| M9 | Every shipped `.bot` round-trips; a built bot's saved and live trees agree; the paint's face groups match the sim's. |
| M10 | A career run survives save/restart identical; the event list is `Events.txt`'s; teammates score nothing. |
| M11 | The oracle's screen pairs within the stated difference; the traction survey number; each residue's measurement. |
| M12 | `tools/check.sh` on Linux; the platform artifacts' run status stated truthfully. |

---

## 8. Deliberate non-goals for v1.0

Named so they are decisions, not omissions:

- **Network multiplayer.** The data exists (`Events_2Player.txt`, the 1.20 sync
  notes, the server list screens), and the arena picker is already typed. v1.0
  is single-player; a later version can take the 2-player events.
- **Mod support** beyond the game's own `#Mods`, `Custom Textures` and
  `Robot Designs` directories.
- **Localisation** — the shipped strings are English; no translation layer.
- **Video** — the game ships none (checked: no movie directory; the "intro" is
  music and a skip screen).
- **The exact Havok "feel"** beyond the measured constants and the oracle's
  tolerance — F6 stays named if the binding cannot express it.
- **Redistributing the game**, or any asset; the deliverable is engine +
  tooling + docs.

---

## 9. The next ten tasks (first sprint)

Concrete, in order, each ending in a commit:

1. **done** — **W5/M8.1 recon**: `docs/11-match-rules.md`. The rules are C++,
   not script; KOTH alone scores in `king.py`.
2. **done** — **W4**: the original runs under the installed flatpak Wine 9.0;
   the main menu is captured at 1024×768 and 1280×1024 (`docs/12-oracle.md`).
3. **done** — **M7.1/M7.2/M7.3**: `ra2-app` presents the shell offscreen and
   in a window, with exact click mapping (`engine/crates/ra2-app`).
4. **done** — **M7.3/M7.2**: the loop and hit-testing; the main menu →
   `select_team` → `teamhq` click path is a test.
5. **M7.5/W2**: decode and play one MP3 and one WAV; music per screen group;
   the `ra2.cfg` volumes.
6. **M7.6/W3**: `ra2.cfg` read/write with the shipped keys.
7. **M8.2**: the `Match` state machine headless (spawn → fight → over) on the
   box arena, with the result screen.
8. **M8.3**: wire the inert `plus` state (`game_type`, `points`, `over`,
   `paused`, `players`), `Arena.tickInterval`, zone contents, and the
   King-of-the-Hill score test against `king.py`.
9. **M8.7**: the live HUD, driven by the match state, read in a frame.
10. **M9.1/W1**: the `.bot` writer and the 57-bot round-trip.

After that, the milestones' own tables are the backlog; each item is small
enough to finish, measure and commit on its own.
