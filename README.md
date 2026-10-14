# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**M1 (asset rendering), M2 (physics sandbox) and M3 (robot fights) are working.**
The engine (Rust) parses the original `.gmf` meshes, `.bot` robots, and arena
`.py` scripts, renders them with their textures via `wgpu`, and simulates them
with **Jolt Physics** — no Windows, no Wine in the loop:

```
# Render
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png                 # assembled robot
ra2-render "local/game/Arenas/octagon.py" octagon.png              # arena scene

# Drive one robot around (M2)
ra2-sim "local/game/Arenas/octagon.py" "local/game/AI/Team1/Bot0.bot" out/ 6 10
RA2_OBSTACLE=crates RA2_YAW=180 RA2_SPAWN=0,3.5 \
  ra2-sim "local/game/Scripts/Practice.py" "local/game/AI/Team0/Bot0.bot" out/ 6 10

# Two robots fight to the death (M3)
ra2-duel "local/game/Arenas/octagon.py" \
  "local/game/AI/Team14/Bot2.bot" "local/game/AI/Team13/Bot1.bot" out/ 20 12
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
- **M3:** a robot is a *component tree* rebuildable from the game's own parts.
  Motors turn their attached subtree (spinners spin, wheels roll), weapons deal
  damage on contact, and parts whose hitpoints run out are torn off as flying
  debris — a fight between two AI-driven robots ends when a chassis is destroyed.
- **Validated against the original** (`docs/05-validation-report.md`). The game
  statically links **Havok Hardcore 1.7.1**, and its own data files state the
  physics outright: each arena carries its gravity (stock `-9.81`, the practice
  arena `1.3 g`), its fixed collision geometry as mass-0 rigid bodies (triggers
  flagged `DISABLE_COLLISIONS`), its per-body friction/elasticity, and its
  movable props. The engine now reads all of that instead of guessing, and the
  chassis takes its **mass from armour weight × chassis surface area** and its
  **hitpoints from the armour hardness** in the `.bot`.

Working today:
- `ra2fmt::gmi` — GMI `.gmf` parser (181/181 meshes), materials (incl.
  `Multi/Sub-Object` sub-materials and transparency), textures, attachment points,
  embedded Havok rigid bodies.
- `ra2fmt::bot` — `.bot` parser: components, mount transforms, inline chassis mesh
  + outline + skin, all version variants.
- `ra2fmt::arena` — arena scripts → scene (model, props, shadow, cameras).
- `ra2fmt::component` / `ra2fmt::cfl` — component definitions and a native
  **CFL3** (`.cfz`) archive reader (no external tools).
- `ra2sim::rig` / `ra2sim::robot` — the component tree and a live robot with
  motor animation, per-part health and detachable parts.
- `ra2sim::combat` / `ra2sim::duel` — contact-driven damage and the two-robot
  world (arena, props, debris, driving AI).
- `ra2sim` — backend-neutral physics types + the Jolt backend, arena/robot/prop
  scene assembly, and an arcade drive controller.
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`, `ra2 arena`.
- `ra2-render` — headless wgpu renderer (offscreen → PNG) for `.gmf`, assembled
  `.bot` robots, and arena `.py` scenes.
- `ra2-sim` — headless physics sandbox driving one robot around an arena.
- `ra2-duel` — headless two-robot fight.

Remaining caveats: chassis *face-group* texture decals can appear
rotated/mirrored; exact BurstMotor rest poses are approximated; a robot's
collider is a single convex hull rebaked when parts are lost (components are not
yet individually jointed bodies, so buckling is "parts come off" rather than the
original's chassis deformation); damage attribution picks the nearest part rather
than solving per-part contacts; the drive controller is arcade-style rather than
the original's **torque-driven motors** with per-wheel traction (`contact` /
`grip`); and Jolt combines friction as `sqrt(µ₁·µ₂)` where Havok 1.x used
`√(µ₁²+µ₂²)`, which we cannot change through the safe binding. Each of these is
detailed in `docs/05-validation-report.md` §4, with the fix path.

Next: M4 — the script layer (reproduce the `plus`/`AI`/`Arenas` API so the stock
AI strategies and arena scripts run). See `docs/01-project-plan.md` and
`docs/03-engine-design.md`.

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
