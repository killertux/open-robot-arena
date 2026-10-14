# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**Milestone M1 complete: native rendering of real game assets, including fully
assembled robots.**

The engine (Rust) parses the original `.gmf` meshes and `.bot` robot files and
renders them with their textures via `wgpu` — no Windows, no Wine in the loop:

```
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/Arenas/practice arena/practice_arena.gmf" arena.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png      # assembled robot
```

All 57 shipped `.bot` robots (formats 1.07/1.08/1.11/1.12) render with every
component resolved and placed.

Working today:
- `ra2fmt::gmi` — GMI `.gmf` parser (181/181 binary meshes), materials, textures,
  attachment points, embedded Havok rigid bodies.
- `ra2fmt::bot` — `.bot` parser: components, mount transforms, inline chassis mesh
  + outline + skin, all version variants.
- `ra2fmt::component` / `ra2fmt::cfl` — component definitions and a native
  **CFL3** (`.cfz`) archive reader (no external tools).
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`.
- `ra2-render` — headless wgpu renderer (offscreen → PNG) for `.gmf` and assembled
  `.bot` robots.

Remaining M1 caveats: chassis *face-group* texture decals can appear
rotated/mirrored, and exact BurstMotor weapon rest poses are approximated.

Next: arenas assembled from their Python scripts, then physics and gameplay.
See `docs/01-project-plan.md` and `docs/03-engine-design.md`.

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
