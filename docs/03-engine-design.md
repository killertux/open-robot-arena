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
| Physics | **Jolt** (via `jolt-rs`) or Rapier | Substitute for licensed Havok; map `.gmf` rigid bodies/constraints onto it. |
| Math | `glam` | |
| Format parsing | our own crates | GMI, BOT, CFL3, GIB, TGA/BMP. |

## Repository layout (target)

```
engine/
  crates/
    ra2fmt/      # pure data: GMI(.gmf), GMA(ascii), BOT, CFL3(.cfz), GIB, textures
    ra2sim/      # physics world, component tree, damage/buckling model
    ra2script/   # native reimplementation of plus/AI/Arenas/Gooey object model
    ra2render/   # wgpu renderer + scene graph
    ra2app/      # winit app: menus, workshop, matches
tools/           # python tooling (format probing, conversion, diffs)
local/           # (git-ignored) game files + converted cache
```

## Milestones (each independently demoable)

- **M1 — Asset viewer.** Load a stock `.bot` + its components' `.gmf` + textures
  and render the assembled robot; load an arena `.gmf` and render it.
  *This is the first real "we did it" moment.*
- **M2 — Physics sandbox.** Drop a bot into an arena with Jolt; drive it; collide
  with hazards.
- **M3 — Two robots fight.** Weapons/motors via the component/controller model;
  damage + buckling.
- **M4 — Script layer.** Reproduce the `plus`/`AI`/`Arenas` API and run the stock
  AI strategies and arena scripts.
- **M5 — Game shell.** Workshop (build a robot), tournaments, menus.
- **M6 — Packaging.** Linux/macOS/Windows builds.

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
