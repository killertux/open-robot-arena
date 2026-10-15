# 03 — Native Engine Design

Target: a from-scratch, cross-platform engine (Linux / macOS / modern Windows)
that runs *Robot Arena 2* using the player's own game data.

## Why this is feasible

1. **Data formats are documented** — GMI/GMF meshes (with embedded Havok physics),
   `.bot` robots, components, arenas. Our `tools/gmi.py` already parses 181/181
   meshes.
2. **A lot of game logic is Python** — AI strategies, arena hazards, match flow,
   UI helpers. It is readable and reimplementable rather than opaque machine code.
3. **The engine's object model is exposed** — Boost.Python bindings reveal the C++
   classes the scripts use (`Arena`, `AI`, `SoundWizard`, `ArenaManager`).

## Proposed stack

| Concern | Choice | Rationale |
|---|---|---|
| Language | **Rust** | `wgpu` spans Vulkan/Metal/D3D12; strong asset/graphics crates; safety for a long-lived project. |
| Graphics | **wgpu** (Vulkan/Metal/D3D12) | The original is fixed-function D3D8; we emulate its look with modern shaders. |
| Window/input | `winit` | Cross-platform. |
| Audio | `rodio` / `kira` | Plays the original `.wav`/`.mp3`. |
| Physics | **Jolt** (via the safe `oxijolt` bindings) | Havok-style engine, closest to RA2's original feel. Hidden behind `ra2sim`'s `PhysicsBackend` trait so it can be swapped (e.g. Rapier) without touching the sandbox. The original is **Havok Hardcore 1.7.1** — see `docs/05-validation-report.md` for the mapping and the differences that matter (friction combination, torque-driven motors). |
| Math | `glam` | |
| Format parsing | our own crates | GMI, BOT, CFL3, GIB, TGA/BMP. |

## Repository layout (target)

```
engine/
  crates/
    ra2fmt/      # pure data: GMI(.gmf), GMA(ascii), BOT, CFL3(.cfz), GIB, textures
    ra2sim/      # physics (Jolt behind a PhysicsBackend trait), scene assembly, sim
    ra2script/   # the plus/Arenas/AI object model on an embedded CPython 3
    ra2ui/       # the shell's layout model: .gib widget trees -> rectangles
    ra2-render/  # wgpu renderer + scene graph (lib + `ra2-render` CLI)
    ra2app/      # winit app: menus, workshop, matches                            (planned)
tools/           # python tooling (format probing, conversion, diffs)
local/           # (git-ignored) game files + converted cache
```

## Milestones (each independently demoable)

- **M1 — Asset viewer.** ✅ Load a stock `.bot` + its components' `.gmf` +
  textures and render the assembled robot; load an arena `.gmf` and render it;
  assemble arena scenes from their `.py` scripts.
- **M2 — Physics sandbox.** ✅ Drop a bot into an arena with Jolt; drive it;
  collide with walls and movable props. *(Implemented as `ra2sim` + `ra2-sim`,
  verified headlessly by rendering a frame strip.)*
- **M3 — Two robots fight.** ✅ Weapons/motors via the component/controller model;
  damage + buckling *(implemented as `ra2sim::rig`/`robot`/`combat`/`duel` plus the
  `ra2-duel` runner)*.
- **M4 — Script layer.** 🚧 The engine side of the `plus`/`Arenas`/`AI` object
  model, on an embedded CPython 3, running the game's own Python 2 sources
  unmodified. The arena surface, the shipped AI and the hazards have all landed —
  with the AI in place the headline duel closes and trades blows, and standing on
  a registered SmartZone fires the arena's own spikes *(implemented as
  `ra2script` + `ra2sim::hazards`; see F13)*.
- **M5 — Game shell.** 🚧 Under way, and the layout now places. A `.gib` reader
  (`ra2fmt::gib`) parses all 116 shipped screens — the main menu, team HQ, the
  bot lab, the HUD, every dialog — into widget trees, and `ra2ui` resolves those
  trees into rectangles: `Position`/`Size` with their fraction-or-pixels flags,
  `Alignment` as the 3×3 anchor, split frames into their two panes. All 3590
  widgets place with finite frames, 71 of the 116 files entirely on screen, and
  the main menu's logo, backdrop and button strip land where its own art says
  they should — which is checked by *looking*: `tools/ui_compose.py` rasterises a
  screen from the engine's own rectangles and the game's own art, and the main
  menu comes out as the main menu. Still to come: text metrics from the `fg_*.giff` fonts, clip
  modes, the containers that lay their own children out (`ListBox`, `Grid`,
  `OptionGroup`), the 2D renderer, the `giScript`/`Gooey` Python surface, and the
  workshop. The `fg_*.giff` faces are parsed and *drawn* (`ra2fmt::giff`, `ra2
  font`, and `tools/ui_compose.py` renders a screen's labels with the game's own
  faces and colours — the main menu's `MULTIPLAYER TEAM HQ` reads as itself), and
  a resolver given those faces sizes a zero-`Size` label from its own text.
- **M6 — Packaging.** Linux/macOS/Windows builds.

## M5 design notes

- **The screens are data, so the screens are what run.** As with the arenas and
  the AI, the shell's screens are not re-authored: every menu, the whole bot lab,
  the team HQ and the in-match HUD are the game's own `.gib` files, parsed and
  placed. What is ours is the reader (`ra2fmt::gib`) and the layout model
  (`ra2ui`), and both are pinned by the corpus rather than by example — the
  reader by parsing all 116 files into a closed vocabulary, the layout by
  resolving all 3590 widgets and checking that 71 files place entirely on screen
  and that the main menu's own art lands where it says it should.
- **Rules are measured, and the measurement is kept.** `Alignment` is a 3×3
  anchor because every one of the 841 values in the corpus is one of those nine
  combinations, and that check is a test rather than a comment. Where a rule is
  only *inferred* — the split frame's slot order, the anchor's y-down frame —
  the evidence that supports it is written down next to it.
- **The check is a picture, as everywhere else in this project.** The geometry
  rules were read out of the corpus and confirmed three ways by numbers, but the
  thing that settles a layout is the eye: `tools/ui_compose.py` takes
  `uidump --rects` — the resolver's own output — and the game's own `.tga`/`.bmp`
  files, and composites a screen. The main menu's cement background, centred saw
  plate with its blade teeth, `ROBOT 2 ARENA` logo at the top centre and the two
  gears along the bottom are all recognisable at a glance, and the one thing that
  was *not* obvious from the data — that `ChildOrder` is the draw order
  ascending, not the file's order — was found by looking at a wrong picture
  first.
- **What is not resolved is not guessed.** Zero `Size` (the editor's automatic
  size, which needs font metrics), `ClipMode`, and the containers that lay their
  own children out are named as unresolved rather than approximated, and the
  corpus test records how many widgets that affects.

## M4 design notes

- **The scripts are the specification, so the scripts are what runs.** Every
  arena and every AI in the game is a subclass of an engine class, and the
  behaviour we want is in *their* bodies, not in ours. `ra2script` therefore boots
  a real CPython, puts the game's own `Arenas/`, `AI/` and `Scripts/` on
  `sys.path`, and runs them as written. Nothing in the repository paraphrases a
  strategy or a hazard.
- **They are Python 2.2.** Rather than rewrite them, a translating import hook
  (`legacy.py`) rewrites the constructs Python 3 will not parse — `print`
  statements, `raise E, msg`, `except E, e`, `<>`, `d.iteritems()`, `it.next()`,
  `file()`, `xrange()` — plus Python 2's *implicit relative imports* (`import
  Hazards` inside package `Arenas` means `Arenas.Hazards`), and reinstates the 2.2
  library functions the scripts import (`string.split`, `types.TupleType`, …).
  Only the game's own directories are translated; the shipped `Scripts/Lib` is the
  Python 2.2 standard library and is deliberately *not* used — the host CPython 3
  one is, behind the same shim. One semantic is not translated: `/` on two ints
  truncates in 2.x; the shipped scripts were checked and none relies on it.
- **What is ours is only the engine side.** `plus.Arena` answers the questions the
  scripts ask the world — `RayTest`, `GetStartPointLocation`, the navigation map
  (`SetMapSize`/`SetMapHeight`/`SetMapWall`/`GetPath`/`FromGrid`), the hazard
  joints (`AddPrismatic`, `GetHinge`) and the SmartZone ids (`RegisterZone`) —
  plus the geometry `AddShadowTriangle`/`SetShadowSource` feed. Camera, audio,
  particle and Gooey calls are accepted and *recorded*, so a run reports exactly
  what it dropped instead of ignoring it.
- **The navigation map is the game's own.** Arenas ship a baked `arena.map`
  beside their model, and `Arenas.SuperArena.LoadMap` reads it; `CreateMap` is the
  fallback for arenas that ship none, and it bakes the grid with `RayTest` against
  the model. The AI paths over that grid (`GetPath` is A* on it), and
  `AddCollisionLine` walls it around each hazard — which is how the scripts keep
  the AI off the spikes.
- **The engine calls back in.** `Arena.Tick` each frame, `Arena.ZoneEvent` when a
  robot enters one of the registered SmartZones, `AddShadowReceivers` once after
  construction — the same sequence the original drives.
- **The AI is the game's own, and it is what makes a fight a fight.** The
  stand-in driver in `ra2sim::duel` only ever charges, so the headline duel
  orbited and pinned itself against a wall. With `AI.SuperAI` + `AI.Tactics` +
  the per-bot brain `Bindings.py` picks by the robot's own `Name:` line, the
  robots close to contact and trade blows: the gap goes negative and both take
  damage. A bot the table does not cover keeps the stand-in.
- **The brains run on the game's cadence, not the physics'.** `AI.SuperAI.Tick`
  refreshes its sensors when its own counter reaches 8 and its comment says
  "once per second", and `AimToHeading` multiplies a turning rate by
  `self.tickInterval` — so a brain tick is an eighth of a second and the physics
  steps many times between them, with the controls held. (`Hazards.Flame` and
  `Hazards.Electricity` count the same way and put the arena's own `Tick` at a
  quarter second.) Getting this wrong changes the AI's behaviour, because its
  `countdownToEvaluation` counts ticks.
- **Two conventions had to be read off the scripts, not guessed.** The scripts
  build a direction from an angle with `vector3(a) = (sin a, 0, cos a)`, so
  `GetHeading` is `atan2(x, z)` of the facing and `GetHeadingTo` is the *error*
  (`AimToHeading` receives it directly). And `Turn(t)` is positive for a
  *positive* heading error while `Robot::drive` applies `yaw = -steer * rate`, so
  the drive negates — the stand-in does the same, and getting it wrong sends the
  robots away from each other.
- **The engine answers, the scripts act.** The simulator publishes a
  [`ra2script::BotSnapshot`] per robot each step (position, facing, speed, yaw
  rate, mass, per-component class and health, component spheres for `rayTest`)
  and drains a queue of [`ra2script::Request`]s afterwards (`plus.damage`,
  `plus.force`, `plus.zap`). Keeping it data rather than a callback trait means
  neither side holds the other's lock while Python runs.
- **Verification.** `ra2script`'s tests assert that all fourteen stock arenas
  register, that `Arenas/box.py`'s configuration arrives intact (3 cameras, 10
  shadow triangles, the 30×30 map with its 207 walls and 24 hazard cells, and the
  saw plus two spike zones with their ids), that `RayTest` finds the floor under a
  start point and `GetPath` routes across the arena, and that the translation
  rewrites the constructs the survey found and nothing else. `scriptdump`
  (`cargo run -p ra2script --example scriptdump -- local/game`) prints what every
  arena configures.


## M2 design notes
- **Swappable physics.** `ra2sim::backend` defines neutral `BodyDesc`/`ShapeDesc`
  /`BodyState` and a `PhysicsBackend` trait (create/remove body, impulses,
  velocities, step, ray cast). `ra2sim::jolt` is the only module that knows about
  Jolt.
- **Bodies from game data.** The arena becomes a static triangle-mesh collider
  built from its rendered geometry; the robot becomes a convex hull of its mesh
  vertices; the arena script's `AddXtra` obstacles become *movable* props from
  their embedded `RBCollection` (crates 40 kg, barrels 1500 kg) using the game's
  own mass/friction/elasticity.
- **Two conventions that bit us** (recorded so they are not re-learned):
  1. Jolt's mesh collider is **one-sided**: the counter-clockwise side is solid.
     RA2 geometry is clockwise as seen from outside, so its winding must be
     reversed for Jolt.
  2. The collider is a single sliding hull, so ordinary friction pins it; the
     controller models tyre grip explicitly and the hull keeps low ground
     friction.
- **Controller.** Arcade `throttle`/`steer` → target forward speed + yaw rate,
  with lateral grip. Per-wheel jointed bodies + motor torque is M3 work.
- **Verification.** `ra2-sim` runs the timeline headlessly and writes
  `frame_NNN.png` plus a `strip.png` montage, so driving and collisions can be
  checked without a display.

## M3 design notes

- **A robot is a component tree.** `ra2sim::rig` rebuilds the `.bot`'s parts as
  nodes with a role (chassis / wheel / motor / weapon / piston / structure), a
  mount transform, geometry, and stats read from the component definitions:
  mass from the mesh's `RBCollection`, `hitpoints`/`damageable`/`concussion`/
  `piercing`/`power` from the `.txt`. `ra2sim::robot` binds that tree to one
  physics body and tracks per-part health.
- **Motors turn their subtree.** A motor rotates its attached parts about the
  component-local +Z axis (RA2's axle direction, confirmed by the saw blade and
  wheel meshes: discs lie in XY). `RobotRig::worlds` re-walks the tree with the
  current rotor angles, so a spinner's disc, arms and weapons all sweep together
  and wheels turn.
- **Pistons slide their subtree.** A `Role::Piston` component splits its child
  group onto a prismatic joint along the axis its definition declares
  (`prismatic = <housing>, <rod>, ax, ay, az, neutral, lower, upper`), with that
  travel as the joint's limits. The mesh ships the housing and the rod as two
  rigid bodies (`burstpiston1.gmf`: `body` 14 kg, `shaft` 8 kg) and the children
  mount on `Host = shaft`, so the rod is the moving half. A parked piston holds
  its rod at the definition's force rather than being welded (F8).
- **Damage is contact-driven, per body.** Jolt's `WereBodiesInContact` tells us
  *whether* two bodies are engaged, but not which parts touched; guessing from
  bounding volumes was unreliable (a robot's parts are spread across its body, so
  its own hull surface often touches where no single part's box does). A robot is
  several hinged bodies, so `ra2sim::combat` asks which bodies of each side met
  and attributes through those: the attacker contributes its best weapon
  (`concussion + piercing`) among the bodies that are touching, and the damage
  lands on the opponent's nearest *destructible* part on the other side of the
  contact. Indestructible fittings (`damageable = false`) are skipped so they
  cannot soak a fight forever.
- **A blow is an event, a grind is a rate.** The relative speed the robots
  *arrive* with (measured before the step, since the collision absorbs it) is
  dealt once when contact begins — that is the game's `amount` per hit — while a
  spinning weapon's rotor tip speed is applied every step, because it works the
  same joint repeatedly. Motionless contact therefore does nothing, two rammers
  trade blows, and a stationary spinner still chews. `CombatConfig.damage_scale`
  converts impact speed to points and is calibrated to the game's own numbers:
  its hazards apply a flat 25, and its AI scripts treat `amount > 10` as a good
  hit.
- **The chassis is dressed from its face groups.** Each side of a chassis gets
  its skin from its own face group in the `.bot` — a polygon on the 256×256 skin
  and the skin coordinate of each of the side's vertices, listed in ascending
  index order. The model's own per-point UV table cannot express a seam (a point
  shared by two sides carries one coordinate), which is what made a decal come
  out rotated or mirrored. See F10.
- **The chassis buckles.** A chassis carries a crush model in the `.bot`
  (buckle points and the top plate's corners). As it takes damage the points the
  file marks as movable shift **down and in** — the floor plate stays, the sides
  cave — and both the drawn chassis and its collider follow (the chassis body's
  shape is re-baked). Before this a chassis only ever died whole. See F9.
- **Parts detach.** When a part's hitpoints reach zero the whole subtree detaches:
  each part becomes an independent rigid body at its current world transform,
  gets an outward impulse, and the robot's collider is rebaked from the parts it
  still has. Destroying the chassis ends the fight (the rest scatters).
- **AI.** Two behaviours share one driver: `cycle` (the default) grinds for ~1.6 s
  then reverses for ~0.8 s so the fight keeps moving; `press` (`RA2_AI=press`)
  never disengages. The trigger is the real physics contact, not an estimate.
- **Verification.** `ra2-duel` writes a frame strip plus a log of breakage
  (`t=… Name: part knocked off`, `… chassis destroyed`). `RA2_DUEL_DRIVE=ram`
  drives every robot straight ahead instead of running the AI, which turns a bout
  into a deterministic head-on collision — the only reliable way to observe the
  damage model, since a driver's own pursuit frequently never closes to contact
  (F7).

### The assembled robot (rigid bodies per component group)

`ra2sim::rig::assemble` groups a robot's components into **one rigid body per
rigidly-mounted group**, with a hinge wherever a motor sits between parent and
child — the structure the `.gmf` motor constraints describe. `ra2sim::robot`
builds each body as a compound shape of its members' geometry, drives the wheel
hinges at the speed and torque their own component definitions declare, spins the
weapon hinges at their rated speed, and reshapes a body in place
(`PhysicsBackend::set_body_shape`) when a part is shot off. It is **the only robot
path** (`ra2-sim` and `ra2-duel` both drive it); the arcade single-hull controller
it once replaced is gone. What the honest drive still cannot do — a quarter of the
stock bots will not get traction — is geometry and collision rather than drive
parameters: F11/F5 in `docs/06-deferred-fidelity.md`.

### Physics gotchas worth remembering (all hit during M3)

1. **Sleeping bodies ignore velocity commands.** A body that settles is parked
   by Jolt and later `set_linear_velocity` calls do nothing, so a robot would
   freeze mid-fight. The backend now activates before setting velocity, and
   game-driven bodies opt out of sleeping entirely.
2. **Jolt mesh colliders are one-sided.** Arena proxies in the wild are not wound
   consistently, so the collider is emitted double-sided (both windings). A
   single-sided proxy had robots being ejected from the box arena at 40 m/s.
3. **Arenas ship both display and proxy geometry.** The proxies are the intended
   physics, but some arenas ship none (the practice arena) and some ship a proxy
   that misses the floor. `load_arena` prefers the proxy, verifies with a ray
   cast that something is under the arena centre, and otherwise falls back to the
   visible geometry; `RA2_COLLISION=proxy|visible` forces either.

## Key design decisions to settle

1. **Scripting strategy** — embed CPython (run original 2.2 scripts) vs. native
   reimplementation of the API. (See question to user.)
2. **Physics fidelity** — matching Havok "feel" including the famous explosions.
3. **Asset conversion** — convert-on-load vs. pre-baked GLB/PNG cache.
4. **Coordinate system** — GMI is left-handed (+X left, +Y away, +Z up); decide
   the engine convention and convert at load.

## Fidelity oracle

Keep the original running under Wine (already set up) to A/B compare rendering,
movement, and physics behaviour as we implement.
