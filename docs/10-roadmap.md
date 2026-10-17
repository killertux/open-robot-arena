# 10 — Roadmap to a finished game

M1–M6 are landed (M6 for Linux). This is the plan for the rest: from a verified
engine that renders and simulates, to a game a player can sit down and play with
their own copy of *Robot Arena 2*.

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
- **151 tests**, fmt/clippy clean, `tools/check.sh` PASS.

### The fact that shapes the rest of the plan

**The match rules are largely *data*, not black-box C++.** Measured in the
shipped scripts:

- `Arenas/king.py` (`KingOfHill`) scores with `plus.addPoints`, asks
  `plus.getGameType`, `plus.isMatchOver`, `plus.isDefeated`, and plays
  `plus.createSound`/`plus.playSound` — all of which are ours to implement in
  `ra2script::plus`.
- Game types are arena data: `DEATHMATCH`, `BATTLE ROYAL`, `TEAM MATCH`,
  `TABLE TOP`, `KING OF THE HILL` (`Arenas/*.py`'s `game_types`, filterable —
  `Arenas/__init__.py`'s `enumAvailable(type)`).
- Events, prizes, tiers and arenas are `Events.txt`; the HUD and every dialog
  are `.gib`; the settings are `ra2.cfg`; the sounds and music are files.
- The 1.20 ReadMe names the rules the patches touched — teammates not scoring,
  King-of-the-Hill zones, fractured parts, spectator names — which is a
  free specification for the match shell.

So M8 is mostly **engine API + flow + presentation**, not a decompilation wall.
The C++ still owns the *match shell* (spawn, countdown, camera, HUD refresh,
end-of-match handoff) and the *application glue* (screen stack, save files).

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
| **W1** | **Writers** | `ra2fmt` only reads. Add `bot::write` (target 1.12, round-trip every format), `team::write` (the three identity lines + the 15-int season line + money), `cfg::read/write` for `ra2.cfg`. | M9 (save a bot), M10 (career), W3 |
| **W2** | **Audio** | Decode the shipped `.mp3` (three tracks, one per screen group) and `.wav` (component, weapon, motor loop, crowd, announcer, UI); play 2D and 3D-positional; music per screen; the `ra2.cfg` volumes. `plus.createSound`/`playSound` become real. | M7, M8 |
| **W3** | **Settings** | `ra2.cfg` in the game's own keys (`FullScreen`, `FSDisplayWidth`, `DefaultCamera=watch`, `SoundVolume=-600`, `MusicMuted`, `ArenaIntros`, `EventNumBots`, `UpdateInterval`). Same file, same semantics. | M7 |
| **W4** | **The oracle** | There is **no Wine on this machine** (`which wine` is empty), so there is no A/B against the original's own pixels. Either install Wine and drive it, or capture reference frames/screens on a machine that has it, and keep them as fixtures. | M8, M11 |
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
| M7.1 | `ra2-app` binary: `winit` window + `wgpu` surface, reusing `ra2-render`'s UI pipeline (offscreen path preserved for tests) | new crate `engine/crates/ra2-app` |
| M7.2 | Input: mouse position (the shell's `PtInRect`/hover/click order), keyboard (Esc, arrows/Tab for focus), a bindings table; hit-testing already exists in `ra2ui::input` | `ra2-app`, `ra2ui::input` |
| M7.3 | The loop: fixed 50 ms update, present at vsync, resize, DPI, fullscreen/windowed | `ra2-app` |
| M7.4 | Screen stack: move `ra2-shell`'s `nav` into a `Flow` the app drives; keep the scripted route runner as the headless entry | `ra2-shell::nav` → shared |
| M7.5 | Audio: `rodio`/`cpal` (MP3 + WAV), 2D/3D, music per screen group, volumes from `ra2.cfg`; `plus.createSound`/`playSound` real | new `ra2-audio`, `ra2script::plus` |
| M7.6 | Settings: read/write `ra2.cfg`; a settings dialog drawn from the shipped `.gib` (options screen) | `ra2fmt::cfg`, `ra2-app` |
| M7.7 | Text entry where the shell needs it (team name), the OS clipboard for import/export paths | `ra2-app` |
| M7.8 | Cursor: the shipped cursor art, hover/click states per the corpus | `ra2-render`, `ra2ui` |

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
| M8.1 | **W5 recon**: the match object, spawn order, countdown, camera modes, HUD field writes, end handoff, per-type scoring, KO/immobilise rules; write it in `docs/02` with addresses | `docs/02-recon-findings.md` |
| M8.2 | `Match` state machine: `Setup → Spawn → Countdown → Fight → Over → Results`, driven by data (`ra2.cfg`'s `EventNumBots`, the arena's start points, the event's numbers) | new `ra2sim::match` |
| M8.3 | Grow `plus` to what the arenas call: `getGameType`, `addPoints`, `isMatchOver`, `isDefeated`, `getPoints`, `playSound`, `createSound`, `SetCamera`, `GetCamera`, plus what the HUD scripts need | `ra2script::plus` |
| M8.4 | Spawn: two teams' robots on the arena's start points, with the original's order and facing; `EventNumBots` for Battle Royal | `ra2sim::match` |
| M8.5 | The match loop: fixed-step physics, the arena `Tick`, the AI cadence, damage events, defeated/KO, the countdown and the clock | `ra2sim::match` |
| M8.6 | HUD: the shipped HUD `.gib` screens placed and driven by live state (health bars, points, clock, names, messages) | `ra2ui` + `ra2-app` |
| M8.7 | Camera: `watch`/`chase`/`top` and the arena's own cameras (`AddCamera`), with the shipped bounds | `ra2-app` |
| M8.8 | Results: the `common/matchresults.gib` slots filled from the match (`ra2ui::results` already binds them), then the return to the menu | `ra2ui::results`, `ra2-app` |
| M8.9 | Pause/quit, Esc, and the arena intro (the shipped `.gib` + music) | `ra2-app` |

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
| M9.1 | **W1 `bot::write`**: write 1.12, round-trip **every** shipped `.bot` (byte-identical where the original is canonical, semantically identical otherwise) | `ra2fmt::bot` |
| M9.2 | Component placement: attach at a mount point with the F1 rule (already the renderer's), remove, undo; the palette's filtering and selection already exist | `ra2ui::workshop` |
| M9.3 | Wiring: the control board's buttons and the wiring canvas (`control grid`), the game's own control-board model (.gib + the `.bot`'s wiring data) | `ra2ui`, `ra2fmt::bot` |
| M9.4 | Paint: the texture library and custom textures lists, decals/surface layer, the chassis face groups (F10) as the paint target | `ra2ui`, `ra2fmt::bot` |
| M9.5 | `Test Robot`: open a match with the current bot (M8's `Match`) and return its damage to the workshop | `ra2-app` |
| M9.6 | The workshop's own state: what a tab remembers, `Save As`, `Import`/`Export` to `Robot Designs/` | `ra2-app` |
| M9.7 | The bot's stats panel: weight (`Armor_Definitions` + mass rules), hitpoints, cost — from the same tables the sim uses | `ra2ui`, `ra2fmt` |

**Acceptance.** Round-trip test over all 57+ stock bots; a built bot's saved file
reopens with the same component tree, mass and hitpoints as the live one; a
painted chassis renders its decals in the same face groups the sim uses; the
import/export path is the shipped `Robot Designs/`.

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
| M10.1 | **Season state**: where the league table lives (the `Team` 15-int line — all zero in the shipped copy — and/or the TeamHQ object), how a season starts/ends; write it down | `docs/02` |
| M10.2 | `career` model: teams, roster, points, money, events entered, prizes, repairs, season phase; `W1` writes it where the game does | new `ra2sim::career` (or `ra2game`) |
| M10.3 | Team HQ flows: select/create team, robot inventory (`hq_robots` — `bot_cell.gib`, not the `robot_cell.gib` its note names), league/season grids (S2's fills), history/prizes (`hq_history`), event schedule (`hq_events`) | `ra2ui` + `ra2-app` |
| M10.4 | Events: `Events.txt` → entry rules, tiers, prize list (`golden_sawblade`/`event_results` scene), the prize screen at the end of an event | `ra2fmt::event`, `ra2-app` |
| M10.5 | Repairs: the damage a match leaves on components, the repair list (`repairs.gib`), cost and time, the `ReadMe`'s "graphic for fractured items" | `ra2ui`, `ra2sim` |
| M10.6 | Money/points: the per-type scoring (damage, KO, zone) and what it is worth outside the match; teammates do not score (the 1.20 rule) | `ra2sim::match`, `ra2sim::career` |
| M10.7 | The career save/load, versioned, with a "your game directory" location | `W1`, `ra2-app` |

**Acceptance.** A headless career run: start a season, fight the first event's
rounds, collect a prize, repair, save, restart, and the table/prizes/roster are
identical. The event list comes from `Events.txt` (25 events, the shipped
tiers); a save round-trips.

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
| M11.2 | The traction survey: 27/45 → the mount/contact layer (F11/F4/F5) with the oracle to tell us which side is wrong | `ra2sim` |
| M11.3 | F9's crush curve and subdivided silhouette; F12's mass rule (one density cannot satisfy both anchors — keep the conflict named unless the oracle settles it); F7's per-material damage scale | `ra2sim` |
| M11.4 | F6 friction combination — blocked on `oxijolt`; if it gains a hook, land Havok's `√(µ₁²+µ₂²)`; otherwise it stays named | `oxijolt` |
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
| M12.1 | macOS: `.app` bundle, `libpython3.12.dylib`/Jolt, Metal, notarisation notes, arm64 | `packaging/`, `docs/09` |
| M12.2 | Windows: MSVC build, `python3.12.dll`, DX12, portable zip + launcher (or MSI) | `packaging/`, `docs/09` |
| M12.3 | Linux: Flatpak and/or AppImage built and smoke-tested (network permitting), else the tarball stays the release | `packaging/` |
| M12.4 | CI: `tools/check.sh` on Linux; build jobs for macOS/Windows; artifact upload | `.github/` |
| M12.5 | Versioning, changelog, release notes, a `--version`, and the "supply your own game" first-run flow | `ra2-app` |
| M12.6 | Docs: the player guide, the troubleshooting page (Python, drivers, game files), the fidelity statement | `docs/` |

**Acceptance.** `tools/check.sh` green on Linux; the tarball unpacks and runs;
macOS/Windows artifacts exist **and** were run by a human or CI on that platform
(otherwise they are marked unverified, as `docs/09` does now).

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
| R1 | The match shell owns more than the scripts suggest (countdown, camera, end handoff) | med / **high** | M8.1 recon before M8.2; a minimal flow first | M8 start |
| R2 | The `.bot` writer cannot reproduce every shipped file | med / high | round-trip all 57 bots before any UI work (M9.1) | M9 start |
| R3 | The career save format is not recoverable | med / med | M10.1 read; documented sidecar if not | M10 start |
| R4 | No Wine ⇒ no A/B oracle | high / med | W4: install/drive, else captured fixtures | M7–M8 |
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

1. **W5/M8.1 recon**: read the match shell (REA) and write the address map in
   `docs/02`; list every `plus.*` call the arenas/AI make that we do not
   implement (from a real run's recorded drops).
2. **W4**: try the original under Wine (`xvfb-run`), or set up captured
   fixtures; write down which.
3. **M7.1**: `ra2-app` opens a window and presents `mainmenu.gib` through the
   existing UI pipeline; the offscreen path stays.
4. **M7.3/M7.2**: the fixed-step loop and mouse/keyboard hit-testing; click the
   menu's `TEAM HQ` and land on `select_team`.
5. **M7.5/W2**: decode and play one MP3 and one WAV; music per screen group;
   the `ra2.cfg` volumes.
6. **M7.6/W3**: `ra2.cfg` read/write with the shipped keys.
7. **M8.2**: the `Match` state machine headless (spawn → fight → over) on the
   box arena, with the result screen.
8. **M8.3**: `plus.addPoints`/`getGameType`/`isMatchOver`/`isDefeated`, and the
   King-of-the-Hill score test against `king.py`.
9. **M8.6**: the live HUD, driven by the match state, read in a frame.
10. **M9.1/W1**: the `.bot` writer and the 57-bot round-trip.

After that, the milestones' own tables are the backlog; each item is small
enough to finish, measure and commit on its own.
