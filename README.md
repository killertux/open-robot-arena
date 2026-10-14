# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

Phase 0 — reconnaissance and asset/format reverse engineering.

See `docs/01-project-plan.md` for the staged plan and milestones.

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
