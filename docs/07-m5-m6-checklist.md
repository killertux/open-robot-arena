# 07 — M5/M6 worklist

The shell (M5) and packaging (M6) are the last two milestones. This file is the
orchestration checklist: every open item, its evidence, and where its rule lives.
It is kept so the work is *countable* — an item is done when its measurement is a
test or a picture, not when it looks done.

Numbering: M5 items are `S<n>`, M6 items are `P<n>`. Each item names the file
that decides it.

## M5 — the game shell

The screens, the workshop, the fonts, the layout model and the shell's own Python
have landed. What is left:

| # | Item | State | Evidence / rule |
|---|---|---|---|
| **S1** | **A glyph's rect does not bracket its glyph.** Every label in the shell draws a glyph plus a stroke of its neighbour. | **open — the shipped `.giff` is internally inconsistent** | The cut count is **confounded** (consecutive rects share the gap column), the clean toolbar is **baked `.bmp` art**, and **every face smears** (both renderers, pixel-identical). The page's picture is the **high byte** (the low byte is colour). Overlaying every rect on the page at 4× shows **jitter, 0–6 columns from the nearest blank**, rect generally left of glyph: 69/74/71 of 94 left borders and 66/69/58 right borders land **on ink**, only ~5 bracket a blank-separated cell. Fit `a≈1.00, b≈0, residual≈3 px`; no shift/scale/pitch. Header invariant; of the size-admitting layouts the best strict-bracket is **9/92** (`512x128`). The game's builder `FUN_005cd560` re-packs from per-glyph bitmaps, but the `.giff` parser and text draw were not located; the deserialiser reads floats-then-ints where the file is ints-then-floats. Renderer stays faithful (no invented correction). Tests: `the_font_file_is_internally_inconsistent`, `almost_no_record_brackets_its_glyph`. Next reads: `FUN_00591b50`'s stack args at `0x4e3805`; the ints/floats conversion; the `font+0x38`/`+0x638` quad path. |
| **S2** | **The lists whose content is *runtime*.** The team HQ's standings and the match results' slots. | **open** | The same `ra2ui::cells` rule (a template repeated per row, values bound into the children the template's own `Notes` name) with a **live** source instead of a shipped table. Screens: `teamhq/hq_league.gib`, `teamhq/event_results.gib` (and the match-results screens). The rule is the screen's own `Notes`; the data is the shell's own scripts/state. |
| **S3** | **The nesting rule.** `NumChildren` closes 28 of 116 files to one root; 88 leave blocks a level up, and on two the background tile lands *over* the dialog. | **open** | Named, measured and in the open questions rather than smoothed over. The authority is the game's own reader (`FUN_005fbf90` reads a Grid/ListBox; the `.gib` writer is `FUN_005e7ca0`). |
| **S4** | **The workshop's interaction remainder.** A tab's list tools and paint tools. | **open** | The tab table and the chassis steps have landed (`ra2ui::workshop`). What is left is the game's C++: the list tools and the paint tools. At minimum *named* with what it would need; the parts that are data should follow the data. |
| **S5** | **`Placeholder`** — the 3D view rect. | **done (named)** | Correctly not drawn: it is the rectangle the 3D view fills, not the shell's to draw. The 3D preview inside it is the renderer's 3D path. |
| **S6** | **The shell's entry point.** Booting from the game directory through the menu into a match. | **open** | The pieces exist (`ra2ui`, `ra2-render`'s UI path, `ra2script`); what is missing is one runner that starts at the main menu and can reach the workshop and a match. |

## M6 — packaging

| # | Item | State | Rule |
|---|---|---|---|
| **P1** | **Game directory discovery.** `RA2_GAME_DIR` / a search, and a clear error when the game is absent. | open | The repo ships no game files; the user supplies them. |
| **P2** | **Linux build + run story.** Release binaries, a launcher, a `.desktop` file, an install/uninstall path (tarball first, AppImage/Flatpak if it earns it). | open | Linux first, tested on this machine. |
| **P3** | **macOS / modern Windows.** Build config and a documented path for each. | open | Not testable here; must be honest about that. |
| **P4** | **One-command verification.** Build, tests, fmt/clippy, and a headless smoke render of a shipped screen. | open | Headless verification is this project's rule. |
| **P5** | **Docs for the player.** How to supply the game, what works, what does not. | open | README + `docs/`. |
