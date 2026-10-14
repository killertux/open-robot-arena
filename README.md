# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**M1 complete; arena scenes assembled.** The engine (Rust) parses the original
`.gmf` meshes, `.bot` robots, and arena `.py` scripts and renders them with
their textures via `wgpu` — no Windows, no Wine in the loop:

```
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png                 # assembled robot
ra2-render "local/game/Arenas/octagon.py" octagon.png              # arena scene
RA2_OBSTACLE=crates RA2_HIDE=cieling \
  ra2-render "local/game/Scripts/Practice.py" practice.png         # arena + props
```

- All **57 shipped robots** (formats 1.07/1.08/1.11/1.12) render with every
  component resolved and placed (matching the game's own embedded snapshots).
- All **12 arena scripts** assemble into scenes (main model + script-referenced
  props, e.g. the practice arena's crates/barrels/cones); `ra2 arena <script>`
  lists the recovered composition.

Working today:
- `ra2fmt::gmi` — GMI `.gmf` parser (181/181 meshes), materials (incl.
  `Multi/Sub-Object` sub-materials and transparency), textures, attachment points,
  embedded Havok rigid bodies.
- `ra2fmt::bot` — `.bot` parser: components, mount transforms, inline chassis mesh
  + outline + skin, all version variants.
- `ra2fmt::arena` — arena scripts → scene (model, props, shadow, cameras).
- `ra2fmt::component` / `ra2fmt::cfl` — component definitions and a native
  **CFL3** (`.cfz`) archive reader (no external tools).
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`, `ra2 arena`.
- `ra2-render` — headless wgpu renderer (offscreen → PNG) for `.gmf`, assembled
  `.bot` robots, and arena `.py` scenes.

Remaining caveats: chassis *face-group* texture decals can appear
rotated/mirrored, and exact BurstMotor weapon rest poses are approximated.

Next: physics and gameplay. See `docs/01-project-plan.md` and
`docs/03-engine-design.md`.

## Layout

```
docs/      Design notes, reverse-engineering findings, format specs
tools/     Our own analysis/extraction/build scripts (Python)
engine/    The native engine (Rust workspace: ra2fmt, ra2-cli, ra2-render)
local/     (git-ignored) original game files and extracted data
```

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
