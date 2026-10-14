# 01 — Project Plan

## Goal

A **native, cross-platform** implementation of *Robot Arena 2: Design & Destroy*
(Gabriel Interactive / Infogrames, 2003) that runs on Linux, macOS, and modern
Windows — loading the player's own original game assets and data files.

No original game files are redistributed by this repository. The deliverable is
an engine + tooling + patches; the user supplies the game.

## What we're up against

| Aspect | Reality |
|---|---|
| Source | Closed. 32-bit Windows PE (i386), ~2003 toolchain. |
| Renderer | Direct3D (DX8-era) via a Windows-only API. |
| Physics | **Havok** (licensed SDK) — cannot be shipped; must be substituted. |
| Game logic / AI | Partial: AI is driven by **Python scripts** shipped with the game, so a chunk of behaviour is *data*, not compiled code. Big advantage. |
| Data formats | Community-documented (see `02-recon-findings.md`): `.gmf` meshes, `.bot` robots, `.txt` components, `.tga/.bmp/.jpg` textures, arenas, `.wav`/`.mp3` audio. |
| Distribution | "Bladez1992 LGU Repack" = Advanced Installer single-EXE → `RA2.msi` + `RA21.cab`. |

## Strategy: staged, always-demoable

We build outward from the **data**, because the data formats are documented and
the compiled engine is not. Each stage produces something runnable.

### Stage 0 — Reconnaissance *(in progress)*
- Acquire the game, extract the installer payload.
- Confirm engine tech: renderer API, physics, embedded Python, file I/O.
- Inventory every asset type and count.

### Stage 1 — Asset pipeline *(first deliverable)*
- Port the PyRA2 parsers (Python 2 → 3) and/or reimplement in Rust.
- Decode `.gmf` (GMI) meshes, `.bot` robots, component `.txt`, arenas.
- Convert to portable intermediates (glTF/GLB, PNG, WAV) for inspection.
- **Milestone:** render any stock robot + component mesh with textures, offline.

### Stage 2 — Engine map
- Use **REA/Ghidra** on the original `.exe` to identify subsystems:
  main loop, D3D device init, input, audio, physics stepping, file resolution,
  the embedded Python runtime and its exposed API.
- Produce a subsystem + call-graph map and the engine's data tables.

### Stage 3 — Native engine skeleton
- Window + modern renderer (**Rust + wgpu**: Vulkan/Metal/D3D12).
- Load and display arenas and robot meshes from Stage 1's pipeline.
- **Milestone:** walk a static arena with assembled robots on Linux.

### Stage 4 — Simulation
- Substitute Havok with an open engine (Jolt / Rapier).
- Map original rigid-body + constraint data from `.gmf`/`.bot` onto it.
- Motor/weapon control via the controller binding model.
- **Milestone:** two robots fight; damage/buckling model reproduced.

### Stage 5 — Gameplay
- Robot workshop (chassis building, component attachment, painting),
  tournaments/leagues, the Python AI system, audio, UI.

### Stage 6 — Packaging
- Linux (AppImage/Flatpak), macOS (.app/universal), Windows (MSI/portable).

## Key technical decisions

- **Language:** Rust. `wgpu` (works across Vulkan/Metal/D3D12), strong asset
  ecosystem (`gltf`, `image`, `rodio`, `rapier`/Jolt bindings), memory safety.
  *Alternative:* C++ + bgfx — rejected for velocity in a 1–2 person port.
- **Physics:** Rapier or Jolt-via-FFI. Must reproduce the *feel* (including the
  infamous Havok explosions) — tune, don't just swap.
- **Assets:** convert on load from original formats; cache to GLB/PNG.
- **AI:** reimplement the Python AI API natively, or embed a Python runtime.
- **Oracle:** run the original under **Wine** to A/B compare behaviour.

## Legal & ethics

The game is abandonware and unsupported by its rights holders. We help users run
*their* copy on modern systems and preserve it. We will not distribute the game,
its assets, or Havok binaries. All reverse engineering is for interoperability.
