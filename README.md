# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**M1 (asset rendering) complete; M2 physics sandbox working.** The engine (Rust)
parses the original `.gmf` meshes, `.bot` robots, and arena `.py` scripts, renders
them with their textures via `wgpu`, and simulates them with **Jolt Physics** —
no Windows, no Wine in the loop:

```
# Render
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png                 # assembled robot
ra2-render "local/game/Arenas/octagon.py" octagon.png              # arena scene

# Simulate: drop a robot into an arena and drive it, headless, to PNG frames
ra2-sim "local/game/Arenas/octagon.py" "local/game/AI/Team1/Bot0.bot" out/ 6 10
RA2_OBSTACLE=crates RA2_YAW=180 RA2_SPAWN=0,3.5 \
  ra2-sim "local/game/Scripts/Practice.py" "local/game/AI/Team0/Bot0.bot" out/ 6 10
```

- All **57 shipped robots** (formats 1.07/1.08/1.11/1.12) render with every
  component resolved and placed (matching the game's own embedded snapshots).
- All **12 arena scripts** assemble into scenes (main model + script-referenced
  props, e.g. the practice arena's crates/barrels/cones); `ra2 arena <script>`
  lists the recovered composition.
- **M2:** a robot drops into an arena, drives on a scripted input timeline,
  collides with walls and with movable props, and the run renders to a frame
  strip. The physics engine sits behind a `PhysicsBackend` trait so it can be
  swapped.

Working today:
- `ra2fmt::gmi` — GMI `.gmf` parser (181/181 meshes), materials (incl.
  `Multi/Sub-Object` sub-materials and transparency), textures, attachment points,
  embedded Havok rigid bodies.
- `ra2fmt::bot` — `.bot` parser: components, mount transforms, inline chassis mesh
  + outline + skin, all version variants.
- `ra2fmt::arena` — arena scripts → scene (model, props, shadow, cameras).
- `ra2fmt::component` / `ra2fmt::cfl` — component definitions and a native
  **CFL3** (`.cfz`) archive reader (no external tools).
- `ra2sim` — backend-neutral physics types + the Jolt backend, arena/robot/prop
  scene assembly, and an arcade drive controller.
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`, `ra2 arena`.
- `ra2-render` — headless wgpu renderer (offscreen → PNG) for `.gmf`, assembled
  `.bot` robots, and arena `.py` scenes.
- `ra2-sim` — headless physics sandbox driving a robot around an arena.

Remaining caveats: chassis *face-group* texture decals can appear
rotated/mirrored; exact BurstMotor weapon rest poses are approximated; the robot
collider is a single convex hull (wheels are not yet separate jointed bodies), so
the controller is arcade-style rather than torque-driven.

Next: M3 — two robots fight (weapons/motors, damage, buckling). See
`docs/01-project-plan.md` and `docs/03-engine-design.md`.

## Layout

```
docs/      Design notes, reverse-engineering findings, format specs
tools/     Our own analysis/extraction/build scripts (Python)
engine/    The native engine (Rust workspace: ra2fmt, ra2-cli, ra2-render, ra2sim)
local/     (git-ignored) original game files and extracted data
```

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
