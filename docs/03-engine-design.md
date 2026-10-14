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
    ra2script/   # native reimplementation of plus/AI/Arenas/Gooey object model   (planned)
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
- **M4 — Script layer.** Reproduce the `plus`/`AI`/`Arenas` API and run the stock
  AI strategies and arena scripts. **(next)**
- **M5 — Game shell.** Workshop (build a robot), tournaments, menus.
- **M6 — Packaging.** Linux/macOS/Windows builds.

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
- **Damage is contact-driven.** Jolt's `WereBodiesInContact` tells us *whether*
  two hulls are engaged, but not which parts touched; guessing from bounding
  volumes was unreliable (a robot's parts are spread across its body, so its own
  hull surface often touches where no single part's box does). `ra2sim::combat`
  therefore attributes the hit: the attacker contributes its best weapon
  (`concussion + piercing` times its rotor tip speed) and the damage lands on the
  opponent's nearest *destructible* part. Indestructible fittings (`damageable =
  false`) are skipped so they cannot soak a fight forever.
- **Buckling.** When a part's hitpoints reach zero the whole subtree detaches:
  each part becomes an independent rigid body at its current world transform,
  gets an outward impulse, and the robot's collider is rebaked from the parts it
  still has. Destroying the chassis ends the fight (the rest scatters).
- **AI.** Two behaviours share one driver: `cycle` (the default) grinds for ~1.6 s
  then reverses for ~0.8 s so the fight keeps moving; `press` (`RA2_AI=press`)
  never disengages. The trigger is the real physics contact, not an estimate.
- **Verification.** `ra2-duel` writes a frame strip plus a log of breakage
  (`t=… Name: part knocked off`, `… chassis destroyed`).

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
