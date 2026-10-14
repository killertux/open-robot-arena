# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**Milestone M1 core reached: native rendering of real game assets.**

The engine (Rust) parses the original `.gmf` meshes and renders them with their
textures via `wgpu` — no Windows, no Wine in the loop:

```
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/Arenas/practice arena/practice_arena.gmf" arena.png
```

Working today:
- `ra2fmt` — GMI `.gmf` parser (181/181 binary meshes), materials, textures,
  attachment points, embedded Havok rigid bodies.
- `ra2-render` — headless wgpu renderer (offscreen → PNG); loads meshes +
  textures and renders them.

In progress / next: `.bot` robots, `.cfz` archives, arenas assembled from
scripts, physics and gameplay. See `docs/01-project-plan.md` and
`docs/03-engine-design.md`.

## Layout

```
docs/      Design notes, reverse-engineering findings, format specs
tools/     Our own analysis/extraction/build scripts
engine/    The native engine implementation (later phases)
local/     (git-ignored) original game files and extracted data
```

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
