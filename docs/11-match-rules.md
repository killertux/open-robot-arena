# 11 — The match rules: what is script, what is C++, and what M8 has to build

This is the M8.1 / W5 recon. It answers one question: **where does a match rule
live?** Every claim below is a file and line in `local/game/` or an address in
`Robot Arena 2.exe` (PE32, image base `0x400000`, 1.20). Scripts were read in
full (`Arenas/*.py`, `AI/*.py`, `Scripts/*.py`); the binary was searched by
string, xref and decompilation.

The short answer contradicts the roadmap's premise. The *data* of the game
types is script-side (`game_types`), and KOTH's zone scoring is script-side, but
the win condition for every type is C++ (`FUN_00418040`); Deathmatch, Battle
Royal and Team Match have no script rule at all (no arena branches on those
strings); Team Match's winner is computed in C++ from per-robot points
(`FUN_00418770`); the score is a C++ field on the robot (`+0x20`), and the only
damage-to-points rule found is KOTH's branch of `FUN_004175b0`; the match shell
(window, countdown, clock, HUD, results) is C++.

## 1. Game types

`Arenas/__init__.py:333-344` (`enumAvailable`) filters the registry by
`type in x.game_types`; that list is the only place a type is declared. The
shipped declarations:

| Type | Declared at | Arenas |
|---|---|---|
| `DEATHMATCH` | `box.py:13`, `Bridge.py:15`, `compressor.py:11`, `electric.py:11`, `event_results.py:8`, `half.py:11`, `octagon.py:11`, `parkinglot.py:11`, `Skull.py:11` | 9 (8 listed; `Event Results` is `in_list = False`, `event_results.py:9`) |
| `BATTLE ROYAL` | the same eight listed files | 8 |
| `TEAM MATCH` | the same eight listed files | 8 |
| `TABLETOP` | `clawtop.py:12`, `flextop.py:11` | 2 |
| `KING OF THE HILL` | `hilltop.py:42`, `king.py:71` | 2 |

Also: `Practice Arena` (`Scripts/Practice.py:5-9`) has no `game_types` and
`in_list = False`; the binary knows a sixth string, `OBSTACLE COURSE`
(`0x6f316c`, checkbox manager `0x496cb0`), but no shipped arena declares it.

### 1.1 DEATHMATCH

Declared by the listed combat arenas (`Event Results` is `in_list = False`).
**No script contains a rule for it**: `grep -n '"DEATHMATCH"' Arenas/*.py` only
hits `game_types`. The rules are all in the match shell: the last active robot
wins. `FUN_00418040` counts robots whose state (`FUN_00413810`, field `+0x34`)
is `0`; a destroyed robot takes state `!= 0`, which is exactly
`plus.isDefeated` (`0x4b9ca0`). When one remains it calls the results handoff
`FUN_004170e0` (`0x418040`).

### 1.2 BATTLE ROYAL

Same eight arenas; same absence of a script rule. The only type-specific
number is the config's `EventNumBots=4` (`ra2.cfg:16`), read by `FUN_004e5260`
at `0x4e52dc`: the reader accepts only `2` or `4` and rewrites anything else to
`4` (`0x4e530e`–`0x4e531d`). The spawn loop that consumes the field was not
traced. `getPlayers()` only reports who is in the match; the arenas use it for
intro cameras (`box.py:158-181`).

### 1.3 TEAM MATCH

Same eight arenas. Team membership is a robot field read by `FUN_004177c0`
(`+0x30`); the winner is computed in C++:

- `FUN_00418040` counts survivors, but for `TEAM MATCH` the match continues
  only while another team is alive (`0x418655`, `0x4186ac`, string `0x6ef75c`).
- `FUN_00418770` (`0x418770`) is "finish on points": for `TEAM MATCH` it sums
  each side's points and chooses the higher side; otherwise it settles the
  winner from the per-robot points. It then calls the results handoff with the
  two ids taken from the match's ordered player list (`FUN_0047a220(0/1)`).
- the 1.20 rule "teammates do not score" is the `TEAM MATCH` branch of the
  damage award: `FUN_004175b0` compares team ids and returns when equal
  (`0x417737`); the game-type test is `FUN_004206a0`, which is string
  **inequality**, so the outer branch is "not `KING OF THE HILL`" (`0x4176f2`).

### 1.4 TABLE TOP

`clawtop.py` and `flextop.py`. The only rule in the scripts is falling off:

- `flextop.py:187-188`: `if plus.getLocation(each)[1] < -6: plus.eliminatePlayer(each)`
- `clawtop.py:248-249`: same with `< -4`

Both run from `Tick`. The win condition is again the C++ last-robot check.
`TABLETOP` also changes the AI: `AI/__init__.py:104-106` adds
`AvoidEdges`/`PushOffEdge`; `AI/Tactics.py:80` checks it again.

### 1.5 KING OF THE HILL

`hilltop.py` and `king.py` (the latter's `KingArena` is "LumaZone"). This is the
one type whose scoring is script-side:

- `king.py:26` sets `king_of_hill` only when `plus.getGameType()` is
  `"KING OF THE HILL"` (`Activate`).
- zone: `king.py:75` registers `"kingzone"` id 5; `hilltop.py:46` passes
  `"King_Zone"`, id 1, location `(0, 2, 0)`.
- `king.py:30-53` `Tick`: `initial_delay = 0` (line 32), `scoring_delay = .5`
  (line 33), `points = 10` (line 34), then
  `while self.scoring_delay > scoring_delay: plus.addPoints(self.scoringPlayer, points)`
  (lines 41-45). That is **10 points every 0.5 s, no initial delay**. The comment
  on line 31 ("10 pts every 1 second after initial 3 second delay") is wrong on
  both numbers; lines 32–34 are the authority. `self.tickInterval` is the
  0.25 s arena tick (§2(c)).
- `OneTrueKing` (`king.py:55-59`) asks
  `GetRobotsInPhantom(self.scoring_zone_name)` and keeps only robots for which
  `plus.isDefeated(robot - 1)` is false; exactly one such robot scores. The
  `- 1` matters: the C++ returns 1-based ids. When the zone empties both delays
  reset (`king.py:49-51`); `GetScoringPlayer`/`GetScoringLocation`
  (`king.py:61-65`) publish the holder for the HUD.
- the win is still last-robot-alive (C++), with points deciding a finish on
  demand (`FUN_00418770`).

## 2. The `plus.*` API the game actually uses

Every `plus.<name>` token in `Arenas/`, `AI/` and `Scripts/` (all `.py`) gives
43 distinct names: 40 free functions plus `Arena`, `AI`, `ArenaDetail`.
"sites" is call count plus one representative location.

| name | sites (representative) | in `plus.rs`? | what the match needs |
|---|---|---|---|
| `createSound` | 87 (`Hazards.py:22`) | present, handle only (`plus.rs:358`) | load wav, return handle, looping/position |
| `animateCamera` | 81 (`box.py:142`) | present, deferred (`plus.rs:457`) | interpolate camera over a time window |
| `playSound` | 56 (`Hazards.py:43`) | present, deferred (`plus.rs:371`) | play a handle once |
| `fadeOutLoop` | 24 (`box.py:144`) | present, deferred (`plus.rs:413`) | fade a loop out over ms |
| `getLocation` | 19 (`Hazards.py:385`) | real (`plus.rs:162`) | robot position |
| `loopSound` | 18 (`Hazards.py:133`) | present, deferred (`plus.rs:383`) | loop a handle |
| `getPlayers` | 16 (`flextop.py:33`) | reads `state.players` (`plus.rs:137`), never populated | list of bots in the match |
| `removeSound` | 16 (`Hazards.py:25`) | present, deferred (`plus.rs:395`) | free a handle |
| `setCameraFOV` | 13 (`flextop.py:66`) | present, deferred (`plus.rs:440`) | set FOV |
| `setCameraPosition` | 13 (`box.py:116`) | present, deferred (`plus.rs:426`) | set eye |
| `setCameraRotation` | 13 (`box.py:117`) | present, deferred (`plus.rs:433`) | set yaw/pitch |
| `setVolume` | 13 (`box.py:123`) | present, deferred (`plus.rs:401`) | dB volume |
| `fadeFromBlack` | 12 (`box.py:119`) | present, deferred (`plus.rs:463`) | fade the screen in |
| `fadeInToLoop` | 12 (`box.py:141`) | present, deferred (`plus.rs:407`) | fade a loop in |
| `setBackColor` | 12 (`box.py:17`) | present, deferred (`plus.rs:419`) | clear colour |
| `getTimeElapsed` | 7 (`Flipper.py:71`) | real (`plus.rs:115`) | match clock |
| `stopSound` | 6 (`Hazards.py:140`) | present, deferred (`plus.rs:389`) | stop a handle |
| `Arena` | 5 (`Arenas/__init__.py:40`) | class present (`arena.rs`) | base of every arena |
| `isDefeated` | 5 (`king.py:57`) | real (`plus.rs:329`) | `robot state != 0` |
| `isMatchOver` | 5 (`Hazards.py:312`) | reads `state.over` (`plus.rs:122`), never set | end flag |
| `getGameType` | 4 (`king.py:26`) | reads `state.game_type` (`plus.rs:104`), never set | the type string |
| `eliminatePlayer` | 4 (`flextop.py:188`) | queues a request (`plus.rs:346`); `ra2sim` ignores it (`duel.rs:867`) | remove from match, set state 3 |
| `isMatchPaused` | 3 (`Hazards.py:325`) | reads `state.paused` (`plus.rs:128`), never set | pause flag |
| `setArena` | 2 (`Arenas/__init__.py:353`) | real (`plus.rs:143`) | install the arena object |
| `damage` | 2 (`Hazards.py:326`) | real queue (`plus.rs:180`), applied (`duel.rs:827`) | damage a component |
| `force` | 1 (`Hazards.py:387`) | real queue (`plus.rs:203`), applied (`duel.rs:848`) | N for one step |
| `zap` | 1 (`Hazards.py:399`) | queues (`plus.rs:318`); `duel.rs:867` ignores it | electric damage + visual |
| `setCameraFOVRange` | 1 (`Arenas/__init__.py:38`) | present, deferred (`plus.rs:447`) | watch-cam distance/FOV range |
| `rayTest` | 1 (`Hazards.py:323`) | real (`plus.rs:219`) | `(bot, component, x, y, z)` |
| `playOnce` | 1 (`Arenas/__init__.py:86`) | present, deferred (`plus.rs:377`) | one-shot play |
| `isUpsideDown` | 1 (`Flipper.py:55`) | real (`plus.rs:174`) | inverted flag |
| `isHuman` | 1 (`AI/__init__.py:257`) | returns `false` always (`plus.rs:341`) | true only for the player's bot |
| `isEliminated` | 1 (`octagon.py:264`) | aliases `isDefeated` (`plus.rs:335`) | `state == 3` only |
| `isDebugMode` | 1 (`AI/__init__.py:58`) | real, env (`plus.rs:110`) | debug flag |
| `introComplete` | 1 (`Arenas/__init__.py:120`) | no-op (`plus.rs:134`) | tell the shell the intro is over |
| `getWeight` | 1 (`Hazards.py:387`) | real (`plus.rs:169`) | mass in kg |
| `enumArena` | 1 (`Arenas/__init__.py:344`) | deferred (`plus.rs:150`) | fill the arena picker |
| `ArenaDetail` | 1 (`Arenas/__init__.py:338`) | class present | picker record |
| `addWindow` | 1 (`Gooey.py:5`) | real widget tree (`plus.rs:492`) | script debug window |
| `addText` | 1 (`Gooey.py:10`) | real widget tree (`plus.rs:509`) | one line in it |
| `addPoints` | 1 (`king.py:44`) | returns 0, defers (`plus.rs:529`) | add to robot `+0x20` |
| `AddParticleEmitter` | 1 (`Hazards.py:300`) | inert object (`plus.rs:470`) | flame emitter |
| `AI` | 6 (`SimpleWeapon.py:21`) | class present (`plus.rs:583`) | base of every brain |

### (a) Implemented and matches the observed original

`getLocation`, `getWeight`, `isUpsideDown`, `damage` (`duel.rs:827`), `force`
(`duel.rs:848`), `rayTest`, `getTimeElapsed`, `isDebugMode`, `setArena`, the
three classes, `addWindow`/`addText`.

### (b) Present but wrong or incomplete

- **`addPoints`** — `plus.rs:526-532` returns `0` and defers
  `"Gooey UI (M5)"`. In the original it is the score write: binding
  `0x4b9d80` → `FUN_004177e0` → `FUN_004178a0`, which does
  `fild [robot+0x20]` / `fadd amount` / `FUN_004034d0` (`0x417918`–`0x417928`).
- **`getGameType` / `isMatchOver` / `isMatchPaused` / `getPlayers`** — each
  reads a `State` field that is never written outside `State::new`
  (`plus.rs:104,122,128,137`; `lib.rs:133-139`). KOTH and the AI therefore see
  `""`, hazards keep firing after a match, and the arenas' intro cameras see an
  empty player list.
- **`isHuman`** — hard-coded `false` (`plus.rs:341`). The original asks the
  robot's vtable slot `+0x1c` (`0x4cd640`). Fine for an all-AI exhibition,
  wrong for the player's bot.
- **`isEliminated` / `eliminatePlayer` / `zap`** — `isEliminated` aliases
  `isDefeated` (`plus.rs:335`), while the original is `state == 3`
  (`0x4b9c70`) and `isDefeated` is `state != 0` (`0x4b9ca0`). Both
  `eliminatePlayer` and `zap` queue a request that `ra2sim` drops
  (`duel.rs:867`), so Tabletop's fall-out and the electric hazard cannot work.
- **`introComplete`** — no-op (`plus.rs:134`). The original uses it to end the
  intro and let the countdown begin (`Arenas/__init__.py:120`); ours never
  starts the intro in the first place (`DoIntro` is never called from Rust).
- **`enumArena`** — defers the picker (`plus.rs:150`); the type/arena list the
  original builds is not reproduced.
- **the audio group** — `createSound` … `fadeOutLoop` (`plus.rs:357-416`):
  handles are allocated and every call is deferred; nothing is decoded, mixed
  or played.
- **the camera group** — `setBackColor`, `setCameraPosition`,
  `setCameraRotation`, `setCameraFOV`, `setCameraFOVRange`, `animateCamera`,
  `fadeFromBlack` (`plus.rs:419-466`): all defer and change nothing.
- **`AddParticleEmitter`** — returns `emitting = false` (`plus.rs:470-483`);
  `Hazards.Flame` sets it, but no renderer reads it.

### (c) Missing

No *called* free function is absent from the module — all 40 are registered.
The missing surface is (i) class methods and (ii) functions the original
registers but no shipped script happens to call:

- **`plus.Arena.tickInterval`** — every arena reads `self.tickInterval`
  (`Arenas/__init__.py:94`, `king.py:40,47`), but `arena.rs` has no getter.
  `ARENA_TICK_INTERVAL = 0.25` exists in `lib.rs:114` but is not exposed. The
  call raises `AttributeError`, which `run_arena_tick`'s `unwrap_or(true)`
  swallows (`host.rs:425-427`), so a KOTH `Tick` never runs at all.
- **`plus.Arena.GetRobotsInPhantom`** — returns `Vec::new()` unconditionally
  (`arena.rs:785-791`). This is the KOTH sensor; with it empty
  `OneTrueKing` never returns a king, so **the shipped KOTH rule scores zero**.
- **`plus.Arena.GetZoneContents`** — returns empty (`arena.rs:792-795`), and
  `State::zone_contents` (`lib.rs:133`) is never written. The AI's
  `AI.GetZoneContents` (`plus.rs:795`) reads the same empty map.
- **`plus.Arena.NextStaticCamera` / `NextWatchCamera` / `NextAutoCamera`** —
  return `""` and do not apply a camera (`arena.rs:820-834`); nothing drives the
  arenas' `StaticCams`/`WatchCams`. `plus.Arena.Winner` is a base no-op
  (`arena.rs:811`) but `SuperArena.Winner` overrides it (`Arenas/__init__.py:79-86`).
- **registered by the original, not by us** (from `FUN_004cd6b0`): `getSpeed`
  (`0x4cd5f0`), `getHealth`, `getHitpoints`, `describe`, `disable`, `practice`,
  `usePower`, `magicMobility`, `gravity`, `areHazardsOn`, `isStandard`,
  `getScreenSize`, `elecZoneEvent`. No shipped `.py` calls them.

## 3. The match shell in the binary

| address | what it is | evidence |
|---|---|---|
| `0x496cb0` | fills the game-type checkboxes; maps each type string to a checkbox name | strings `DEATHMATCH`, `BATTLE ROYAL`, `TEAM MATCH`, `TABLETOP`, `KING OF THE HILL`, `OBSTACLE COURSE` |
| `0x473da0` | refreshes the same checkboxes from an arena's `game_types` | six `FUN_00401d30` string pushes then `FUN_004021a0` membership tests |
| `0x4733f0` | exhibition setup window: arena list, weight class, game type, time dropdown | strings `exhibition\\arena.gib`, `exhibition\\gametype.gib`, `time dropdown`, `No limit`, `game_type optiongroup` |
| `0x4e5260` | reads `ra2.cfg`; `EventNumBots` at `0x4e52dc`; `DefaultCamera` at `0x4e5503` | `EventNumBots` clamp `0x4e530e`–`0x4e531d`; camera string stored at `[edi+0x64]` (`0x4e552d`) |
| `0x46bb70` | creates the match window (`common\\gamewindow.gib`) and selects a game-type string | strings `Game Window`, `common\\gamewindow.gib`, switch cases `DEATHMATCH`/`BATTLE ROYAL`/… |
| `0x4749c0` | GameWindow HUD constructor: loads `gamewindow.gib`, binds `count number`, `camera textbox`, `time group`, chat widgets, four `playerN status` groups, each a `statusmeters.gib` | strings `count number` `0x6f3474`, `camera textbox` `0x6f3490`, `time group` `0x6f34a0`, `player1 status` `0x6f350c` |
| `0x4b2f20` | one status meter: `points`, `battery bar`, `air bar`, `armor bar`, `green dot`, `yellow dot`, `red dot`, `common\\whitebar.bmp` | strings `0x6f59xx`; called from `0x4749c0` |
| `0x415500` | match-shell init: nine sound handles (`match_countdown.wav`, `chat.wav`, `critical_hit.wav`, `immobile_countdown.wav`, `frc_generic1..5`), a lookup of the `match results window` class, and points zeroed per robot (`0x415a75`) | strings at `0x6ef7xx`; `FUN_004034d0(0)` at `0x415a75` |
| `0x475540` | start of a match: calls `0x4885d0(0x32)` and `0x414170(0x32)` (the literal 50; the setters are not identified), resets the four meters, intro branch, then countdown | `0x32` = 50, the same value as `ra2.cfg:26 UpdateInterval=0.050000` |
| `0x4758c0` | pre-countdown state: hides `countdown group`, shows `skip group`; `+0x1d0 = 2`, `+0x1d4 = 1` | strings `countdown group` `0x6f3574`, `skip group` `0x6f3568` |
| `0x4759c0` | starts the countdown: hides skip, sets timer `+0x1dc = 4.0` (`0x475a97`), starts the clock, picks the camera | strings `watch` `0x6f3594`, `static` `0x6f358c`, `orbit` `0x6f3584` |
| `0x477480` | per-frame countdown and clock: decrements `+0x1dc`, writes `count number`, plays `match_countdown_high.wav`; at zero calls `0x416360` | `fld [esi+0x1dc]` `0x477519`; `call 0x416360` `0x477666`; `mov [esi+0x1dc],0` `0x47766b` |
| `0x416360` | begin fight: hides the countdown, sets `+0x8e = 1` | `0x416408` |
| `0x412fc0` | `GameWindow::Update(dt)`: match clock, popup decay (`0x413b20`, `0x416210`), score refresh (`0x4136e6` → `0x476b60`), end-of-match timer | callers `0x4e5b90`, `0x4e5e30`; `+0xea`/`+0xec` block at `0x4130xx` |
| `0x418040` | last-robot / last-team check; calls the results handoff | count of state-0 robots; `TEAM MATCH` string `0x6ef75c` twice |
| `0x418770` | winner by points; team totals for `TEAM MATCH`; passes two ids from the ordered player list | `getPoints` `0x494f50`, `getTeam` `0x4177c0`, `FUN_0047a220(0/1)` |
| `0x4170e0` | end of match → `MatchResultsWindow`, fill cells, `0x416450` | `match results window` `0x6ef60c`, `common\\matchresults.gib` `0x6ef708` |
| `0x416450` | sets `+0x8f = 1` (the `isMatchOver` byte) and deactivates every robot | `0x416458`; getter `0x476500` returns `[ecx+0x8f]` |
| `0x487d50` | results screen buttons: `continue button`, `Quit Session button` | strings `0x6f2d34`, `0x6f3ff4` |
| `0x4884b0` | fills each result row: robot state, points, winner flag | `FUN_00487830(state)`, `FUN_00487710(points)`, `FUN_00487950(1)` |

The table is also the order: `ra2.cfg` → picker → match window/HUD → countdown
→ per-frame update → results.

### Camera modes

`0x4759c0` compares the `ra2.cfg` string against exactly three literals:
`watch` → `0x4770a0`, `static` → `0x477030`, `orbit` → `0x476f00`.
`ra2.cfg:25` ships `DefaultCamera=watch`. `search_strings` finds no `chase` and
no `top` literal anywhere in the binary, so the roadmap's "watch/chase/top"
naming is not the config's vocabulary. The per-arena cameras are the scripts'
`StaticCams`/`WatchCams` (`Arenas/__init__.py:150-182`), cycled by
`NextStaticCamera`/`NextWatchCamera`; `NextAutoCamera`
(`Arenas/__init__.py:184`) is empty in the base class.

## 4. Out of match: results → points → standings

### 4.1 A finished match produces per-robot points

- The score is a C++ integer at robot `+0x20`, zeroed at match start
  (`0x415a75`). `plus.addPoints(player, amount)` is bound at `0x4b9d80`; it
  resolves the player (`0x4146c0`) and calls `FUN_004177e0` →
  `FUN_004178a0`, whose write is `fild [robot+0x20]` / `fadd amount` /
  `FUN_004034d0` (`0x417918`–`0x417928`). Setter `FUN_004034d0`, getter
  `FUN_00494f50`.
- The only script that calls `addPoints` is `king.py:44`, so KOTH's zone score
  is script-generated and everything else must be C++.
- **Damage scores in every type except KOTH.** `FUN_004175b0` tests the game
  type with `FUN_004206a0` (`0x4176f2`); that helper is
  `FUN_00402200(...) == 0`, and `FUN_00402200` is *string equality*
  (`FUN_00402360` = `compare(...)`, `FUN_004023c0`) — so `FUN_004206a0` is
  **inequality**. The award block is therefore the **not-`KING OF THE HILL`**
  branch. Inside it the victim must still be alive (`FUN_00413810`, `+0x34 == 0`)
  and, when the type is `TEAM MATCH` (`0x417737`, again `FUN_004206a0`), the
  attacker's and victim's team ids must differ (`FUN_004177c0`, `+0x30`); in the
  other non-KOTH types there is no team test. The amount is the effective damage
  returned by `FUN_00489620` (the component's vtable `+0x1c`, called with the raw
  damage), and `FUN_004178a0` writes `score = trunc(score + amount)` at
  `0x417918`–`0x417928`. The collision path only reaches this when the raw
  damage is at least `10.0` (`FUN_00417330`). A hazard's damage has attacker
  `-1` (`FUN_004148f0`) and awards no stored points. The `2.0f` at `0x504d13`
  and `0x504d5f` is the **flame's burn tick**: `0x504ca0` applies 2.0 damage
  through `FUN_004148f0` and shows a `2` with `FUN_004177e0`'s store flag zero;
  it is not a points formula.
- `FUN_00412fc0` refreshes the HUD score from `getPoints` after every update
  (`0x413657-0x4136e6` → `0x476b60`).

### 4.2 The result screen

`FUN_004170e0` loads `common\\matchresults.gib` through `MatchResultsWindow`;
`0x4884b0` fills it and `0x487d50` provides Continue/Quit. `FUN_004884b0`
takes each robot in turn and calls `FUN_00487830(state)`,
`FUN_00487710(points)` and `FUN_00487950(1)` for each winner id passed by
`FUN_00418770`.

That is the template `ra2ui::results` already models
(`engine/crates/ra2ui/src/results.rs:59-66`): team logo, bot preview, points,
bot name, team name, immobilized text, winner text. The four slots are
`result slot 1`…`result slot 4` (`results.rs:44-49`), the template is
`common/result_cell.gib` (`results.rs:52`), and `fill` places one cell per
result (`results.rs:90-110`). The binary confirms the source is the live match,
not a data file: the screen's sample strings are editor placeholders.

### 4.3 The team record and the 15-int line

`Teams/Teams.txt`'s first record is 24 lines; line 21 is the 15-int season
line, all zeros in the shipped copy:

```
Teams/Teams.txt:21
0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
```

The binary writes it in `Team::save`, `FUN_004fb010` (`0x4fb010`): a loop of
15 iterations copies `[team+0x5c]` … `[team+0x94]` (`puVar4 = param_2 + 0x5c;
iVar3 = 0xf; do { ... } while`). `Team::load`, `FUN_004fabf0` (`0x4fabf0`),
reads them back. The record also carries `Robots:`, `Robot In Event: ` and
identity, which is all `engine/crates/ra2fmt/src/team.rs` parses — **the 15 ints
are not read at all**, so a career cannot round-trip through the current type.

The event result screen binds a separate trio: `FUN_0046def0` looks up
`rounds won`, `team points` and `prize description` (`0x46def0`, strings
`0x6f2d04`, `0x6f2cf8`, `0x6f2ce4`) into `+0x59`, `+0x5a`, `+0x5b`. The league
screen binds `points` (`FUN_004fcf50`, `0x4fd05b`) from the team object, not
from the 15-int array.

**Established:** the bytes and write site of the 15-int line, and that points
reach the result screen and the event-result trio. **Not established:** what
each int means and the copy into `Team+0x5c`; next reads are `0x5024e0`,
`FUN_0046e2b0`, `FUN_004faae0`.

## 5. What M8 must build

1. **Populate the script state before the scripts are built**: `game_type`,
   `players`, `paused`, `over`, set before `create_arena_for_script`/`create_ai`.
2. **Expose `plus.Arena.tickInterval`** (0.25 s) — without it KOTH's `Tick`
   raises on `self.tickInterval`.
3. **Make `plus.addPoints` add** to a per-robot score field (original:
   `fild [robot+0x20]; fadd`), initialised to 0 at match start.
4. **Give the match a robot state** with three values: active (0), defeated
   (non-zero), eliminated (3).
5. **Implement `eliminatePlayer` and `zap`** in `ra2sim` instead of dropping
   them; Tabletop's fall-out and the electric hazard depend on them.
6. **Fill `zone_contents` and `Arena.GetRobotsInPhantom`** (1-based robot ids),
   so KOTH can pick a king and the AI's zone tactics work.
7. **Add the type rules**: last-alive, one-team-left, and the KOTH 10-point /
   0.5 s tick against `king.py`.
8. **Spawn**: start points with facing, `EventNumBots` 2/4 for Battle Royal,
   teams from the event data.
9. **HUD**: `gamewindow.gib` + `statusmeters.gib`; bind points, three bars,
   four `playerN status` groups, `time group`, `count number`; 4.0 s countdown.
10. **Cameras**: watch/static/orbit, the arenas' `StaticCams`/`WatchCams`,
    `DefaultCamera` from `ra2.cfg`.
11. **Results**: winners by points, four `result slot`s through
    `ra2ui::results`, `isMatchOver`, the 3.5 s delay.
12. **Season state**: read/write the 15 ints in `ra2fmt::team`, then bind event
    points to `Team+0x5c`.

## 6. What this pass could not establish

- **The damage formula's inner scale and the time-out end.** The outer rule is
  now established (§4.1 and §7): damage scores in every type except KOTH, the
  amount is `FUN_00489620`'s return, and the write truncates. What
  `FUN_00489620`'s virtual (`component vtable +0x1c`) does to the raw damage —
  the armour/material scale — was not decoded; this port awards the damage its
  own model actually applies. The `2.0f` at `0x504d13`/`0x504d5f` is the flame
  tick, not a scale. This pass did not find a path from `+0x17c` to
  `0x418040`/`0x418770`; M12 did, and corrected the field: `+0x17c` is the
  **camera-name timer** (`docs/24` §5), while the match clock is `+0xe0` and
  its zero calls `0x418770` from `0x4133b3` (`docs/24` §1).
- **Mode values and flags.** `GameWindow+0x9c` takes values 0–5 (`0x4145f0` is
  the raw getter); the mode names are not recovered. `+0x8c`, `+0x8d`, `+0x8e`,
  `+0x8f`, `+0x158` are only partly known (`+0x8f` = `isMatchOver`,
  `+0x8e` = "fighting").
- **Status-meter fill.** The widgets are bound in `0x4b2f20`, but the per-frame
  write into the bars was not traced; `0x412fc0` writes only the score text.
- **The 15 ints and the slots.** Their meanings, the copy from the event result
  (`+0x59`/`+0x5a`) to `Team+0x5c`, and how four result slots map when fewer
  than four results exist (`0x4884b0` fills in list order).
- **Spawn.** `EventNumBots` is read and clamped (`0x4e52dc`–`0x4e531d`), but
  the loop that turns it into robots and start-point assignments was not traced.
- **`Match Over` / `chase` / `top`.** None of these strings exists in the
  binary; the results are announced by `match results window` / `in game popup
  win` (`0x6ef60c`, `0x6ef5f8`). The camera vocabulary is `watch`, `static`,
  `orbit`; if the roadmap keeps "chase/top", they are our labels.

## 7. Implemented in M8.2–M8.6

**Script state (`ra2script`).** `game_type`, `players`, `paused`, `over` and a
per-robot score store are now written by the match and read by the scripts:
`getGameType`, `isMatchOver`, `isMatchPaused`, `getPlayers`, `getPoints`.
`plus.addPoints` adds to the `+0x20` store (it does not replace; the binary's
write is `fild [robot+0x20]; fadd`). `plus.Arena.tickInterval` is exposed as
the engine's 0.25 s arena tick. The simulator publishes every registered
SmartZone's contents each step, and `GetRobotsInPhantom` reads the model's own
phantom objects (`King_Zone`, `kingzone`) — `king.py`'s zone is not a registered
SmartZone, so the volume comes from the `.gmf` object. Eliminations now reach
`Robot`'s state 3 and `plus.isEliminated`/`isDefeated` distinguish it from a
knocked-out robot; `plus.zap` records the electric visual deadline (the shipped
hazard applies its own damage). A Python error from `Arena.Tick` is returned
with its traceback and logged once, not turned into `true`.

**The match (`ra2sim::r#match`).** `MatchConfig` (arena, game type, robot
entries, `EventNumBots` through `event_num_bots`: only 2 or 4, anything else 4,
explicit duration/results delay/step) builds the state machine
`Setup → Countdown → Fight → Over → Results`. The countdown is 4.0 s; the
fight steps the existing `Duel` at 1/60 s, with its AI cadence and the arena
`Tick` at 0.25 s. Spawns are the arena's own start points in model order and
facing (four for `BATTLE ROYAL`). Win conditions are last robot, `TEAM MATCH`
team totals, and a points finish when the explicit duration runs out; KOTH is
not an end condition. `MatchReport` carries the winners, per-robot points,
damage dealt/taken, fight duration and the `ra2ui::results::MatchResult` rows
(team logo/name from `AI/Teams.txt`, bot name, points, winner flag).

**Observed tests.** Box + two shipped AI brains + `DEATHMATCH` reaches `Over`
with a winner after the real fight (then the tabletop elimination path decides
it). Hilltop + `KING OF THE HILL` with a chassis parked in `King_Zone` scores
10 points at the first 0.5 s boundary and 80 more over the next 4.0 s; the score
is the `plus.addPoints` store and the script's own `scoringPlayer` is set. A
`BATTLE ROYAL` with `EventNumBots=4` spawns four distinct model start points.

**Damage → points (M11, measured).** The whole award is the **not-KOTH** branch
of `FUN_004175b0`: `FUN_004206a0` is `FUN_00402200(...) == 0`, and `FUN_00402200`
is string equality, so `0x4176f2` reads "if the type is not `KING OF THE HILL`".
For a landed hit whose raw damage is at least `10.0` (`FUN_00417330`), with a
real attacker and a victim still in state `0` (`+0x34`), the attacker's score
gets the effective damage `FUN_00489620` returns, written as
`trunc(score + amount)` (`0x417918`–`0x417928`). In `TEAM MATCH` (`0x417737`) a
teammate hit is rejected; the other non-KOTH types have no team test, so a
teammate hit scores there. KOTH awards no damage points at all — its only score
is `king.py`'s `plus.addPoints`. `FUN_004178a0` ignores non-positive amounts, so
a hit under 10 damage that is passed to it would add nothing; the 10 gate is the
collision path's. The `2.0f` at `0x504d13`/`0x504d5f` is the flame hazard's
2.0-damage tick (`0x504ca0` → `FUN_004148f0`), shown with a zero-store
`FUN_004177e0` call; it is not a points multiplier. The duel exposes each
scoring hit (`Duel::ScoreHit`) and `Match` applies the type rule; the tests
measure 12.7 → 12, +37.9 → 49, 9.9 → 0, teammate 40 → 0, opponent 40 → 40. A
`TEAM MATCH` last-team finish still beats a higher points total: `FUN_00418040`
hands the survivor to the results, and only the time-out uses `FUN_00418770`'s
totals.

**Still unproven / deliberate gaps.**

- The component virtual behind `FUN_00489620` (`+0x1c`) — the armour/material
  scale applied to raw damage — was not decoded. This port scores the damage
  its own model applies, which is the same quantity at the write but not proven
  to equal the original's per-material scale.
- The time dropdown's option labels (`0x4733f0`): its displayed value is data,
  the list is empty in the shipped file, and `--time`/`--seconds` replace it.
- Which robots "immobilized text" covers is not established; the text stays
  hidden. The result cell's preview is now the `.bot` file's own embedded TGA
  snapshot (`docs/23-previews-and-modes.md`); the exact image the original's
  live preview renderer would produce is not established.
- The match clock, the three-minute default, the 3-2-1 countdown, the time-out
  winner/draw rule, the results handoff's 3.5 s and the `playerN` slot mapping
  are now measured; `docs/24-match-shell.md` carries the addresses and what
  remains (the `+0x74` flag's name, the intro timer, the watch camera's name).
- The `EventNumBots` spawn loop (above the four start points) was not traced;
  the port uses the model's start points in ID order.
- `plus.zap`'s own damage: not established, and it awards none.
- The hilltop model's *visible* hill is not in the static collider this port
  builds (the collider is the backdrop box), so the KOTH test parks a chassis
  in `King_Zone`; the script/zone/score path it exercises is the real one.

**M8.6–M8.10 (presentation)** are in `docs/14-match-presentation.md`: the
`--match` frame commands, the HUD binding, the three camera rules, the results
slots and the pause dialog. That document also names the bar mapping and the
`watch`/`orbit` framing as ours, and the intro as still unimplemented.

**M11** (`docs/19-match-fidelity.md`) closes the three measurable gaps:
damage→points above, the status-meter fill (`0x476b60` → `0x4b3540`/`0x4b3570`/
`0x4b35a0`: left-anchored, `trunc(fraction * 52 / 26)`), and the results title
order (the panel's `+Background` body is an attachment, drawn before the
window; it is not a sibling with the body's `ChildOrder`). What each
status field is made of and the component damage scale remain unproven.

**M12 — the match shell (`docs/24-match-shell.md`).** The clock, time-out,
slot mapping, countdown, results handoff, camera textbox and chat are now
measured:

- The clock is `+0xe0` seconds remaining, set to `+0xdc * 60` at match init
  (`0x415e90`, minutes from `FUN_00414230`/`0x414230`), and it ticks only while
  `+0x8e` (fighting) is set — **the countdown is not in the clock**. The
  match object's constructor default is a **3-minute** timed match
  (`0x41276d`); "No limit" is `FUN_00414230(0)` (`0x414238`).
- At zero, `0x4133b3` calls `FUN_00418770(0)` (`0x418770`). Only **alive**
  robots count. A unique positive highest score wins; a shared or zero best sets
  `+0x74` and is a **draw** (5-5 and 0-0 alike). `TEAM MATCH` sums alive robots by team; a tie
  is a draw, otherwise the first two robots of the winning side are the two
  result ids (`FUN_0047a220(0/1)`). The time-out is checked before the
  last-robot rule in `0x412fc0`.
- `playerN status` is bound in creation order: meter 0 → `player1`,
  1 → `player2`, 2 → `player3`, 3 → `player4` (`0x474abc`–`0x474c2d`). The
  file's corners are 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right.
- The countdown timer is 4.0 s (`0x475a97`), but the number is
  `trunc(timer)` and the fight starts when it reaches 0 (`0x4775b1`–`0x47766b`).
  The visible sequence is `3`, `2`, `1`, one second each; the fight starts at
  1.0 s remaining. The previous "hold 1 for the last second" reading was
  wrong.
- `FUN_004170e0(p1, p2)` (`0x4170e0`) passes the two winner **row ids** to
  `FUN_004884b0`; a row gets the winner text only if its id is `p1` or `p2`
  and `+0x74 == 0`. The handoff writes the 3.5 s results delay
  (`+0xec = 0x6a5788`, `0x41721c`); `0x412fc0` shows `match results window`
  and `in game popup win` when it expires (`0x41309c`).
- `camera textbox` is `+0x178`; `FUN_00477180` shows it for 3.0 s
  (`+0x17c = 3.0`, `0x4771a2`) and `0x4774f1` hides it. The names are
  `Watch Cam` / `Static Cam` / `Orbit Cam` (or the arena's own
  `AddStaticCamera` name); `chase` is this client's mode, labelled as ours.
- The chat group opens on `\` (`0x475ee0`); this port implements open/close
  and a local entry only. The original's close key and display-box retention
  are still unproven (see `docs/24` §8).
