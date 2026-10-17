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
| **S1** | **The page's glyphs were mirrored in `u`; a cell-clipping residue remains.** Every label drew a glyph plus a stroke of its neighbour. | **mirror fixed (79570d1); residue measured** | The mirror was the defect: read as an image, the page's row is `Ǝ Ⅎ ⅁ H I J K L M N O P Q R S T U` — cells in reading order, each letter reversed inside its cell. Both renderers now swap the glyph quad's `u` pair (and `v`); a mirrored word reads as itself, so the page-as-image is the probe, and the toolbar is **baked `.bmp` art**. A **separate** residue: the rect samples a cell shifted a few columns from its glyph in **every** face — 58–82/94 left and 58–77/94 right borders land on ink, 0–5 cells bracket a blank-separated glyph. Shape probe (`F`/`L`/`J` per face): bbox fills the cell, letters upright, `F` density 0.43/0.49/0.50 vs `L` 0.57/0.48/0.59 — the cell clips a bar. Shift grows along each row (pitch, not jitter/slip). Header invariant; best alternative layout 9/92. The game **re-packs** (`FUN_00591b50`→`FUN_005cd560`); how it locates the cut is open (deserialiser reads floats-then-ints vs the file's ints-then-floats). Renderer stays faithful. Test: `the_font_file_is_internally_inconsistent` (replaces the retired `almost_no_record_brackets_its_glyph`). Next reads: `FUN_00591b50` stack args at `0x4e3805`; the ints/floats conversion; which field locates the cut. |
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
