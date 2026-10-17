# The workshop UI (M9.2–M9.6)

This is the application half of M9: `ra2-app --build` boots the bot lab through
the shipped shell and edits a live `ra2ui::workshop::Build`. The model is
[`docs/15-workshop-model.md`](15-workshop-model.md); this page says what the
window and the offscreen runner actually do, what is read-only, and what is
ours rather than the game's.

```text
cargo run -q --release --offline -p ra2-app -- --build --game ../local/game
cargo run -q --release --offline -p ra2-app -- --build --game ../local/game \
    --start "AI/Team1/Bot0.bot" --save-as "My Bot" --seconds 4
cargo run -q --release --offline -p ra2-app -- --build --game ../local/game \
    --save-as "M9 Demo" --seconds 30 --out /tmp/ra2-build
```

| flag | meaning |
|---|---|
| `--build` | open `botlab/botlabwin.gib` as the editor (without `--out`: a window) |
| `--start BOT` | open a `.bot` (a path, or a name under `Robot Designs/`); default: a new bot from the shipped `AI/Team1/Bot0.bot` chassis |
| `--save-as NAME` | the design name the headless script types into the filename box |
| `--seconds N` | `Test Robot`'s match length (default 4; the headless proof used 30) |
| `--out DIR` | run the scripted build headlessly, write the frames, save/reload/compare, then fight |

The window has the same controls: `Enter` saves, `Ctrl+Z`/`Ctrl+Y` undo/redo,
printable characters go into the filename box, `Backspace` deletes. Escape
quits (and in a `Test Robot` match it pauses, as `--match` does).

## 1. What is editable

| operation | how | model call |
|---|---|---|
| choose a tab / a chassis step | the shipped toolbar and step buttons | `ra2ui::workshop::BotLab` |
| select an armour plate | a plate row in the panel on the `armor` step (the shipped `armor grid` is filled and drawn as well; its lower half is under the shell's bottom bar, so the panel row is what is clickable) | `Build::set_armour` |
| select a component | a click on a `components grid` cell (the shipped palette, filled by its own `Notes` rule) | `ra2ui::components` |
| attach it | the same click attaches at the panel's selected mount | `Build::attach` (the F1 rule) |
| choose the parent mount | a click on a mount row in the panel (`chassis baseplate 0:0`, or `component N mount M (axle_m)`) | `Build::attach`'s parent/own mounts |
| remove | the shipped `remove button` (removes the selected part and its subtree) | `Build::remove` |
| undo / redo placement | `Ctrl+Z` / `Ctrl+Y` or the panel row | `Build::undo` / `redo` |
| paint a face group | select a `Texture Library/` row (or a `Custom Textures/` row), then a face row | `Paint::fill_group` |
| undo paint | `Ctrl+Z` on the Paint Shop tab | `Paint::undo` |
| Save As | type a name, `Enter` (or the `m9:save` row) | `Build::save_to` → `Robot Designs/<name>.bot` |
| Open / import a bot | type a name under `Robot Designs/`, then the `m9:open` row | `Build::load` |
| export a bot | Save As, or `Build::export_bot` (the `exportbot.gib` operation) | copy in `Robot Designs/` |
| import a texture | the `Custom Textures/` rows in the Paint Shop panel | `Texture::load`, then `fill_group` |
| export a texture | the `export texture -> Custom Textures/` row | `Paint::skin().to_tga()` |
| Test Robot | the `m9:test` row on the tab | `ra2sim::match::Match` with the saved bot |

The component palette click is one component per click. The click is
hit-tested against the placed grid and identified by the cell's **rank** in the
filled palette (the placed children are in draw order; several stock components
share one preview bitmap, so art cannot identify a row).

The palette is a one-row window (`Rows 1, Columns 4`) with 30 mechanics
components. The runner scrolls it by hand to the row holding the requested
component before clicking; the window has no scroll-wheel wiring yet.

## 2. The panel, and why it exists

The shipped tabs have **no slot** for:

- the current part list (a tree: `7: Z-Tek Motor (25.0 kg) <- 0`);
- a mount chooser (the game's own is a click on a mount point in the 3D view);
- a face-group list (the game's own is a click on a chassis panel in the 3D
  view);
- the armour plates on the `armor` step, whose shipped grid sits mostly under
  the shell's bottom bar;
- a texture filename list on the appearance tab (the shipped `texturefill.gib`
  is loaded by the `effect dropdown`, whose list the C++ fills);
- the filename box (the game's Save dialog is `exit botlab window`).

So the build mode draws a synthetic `.gib` panel inside the screen's own 3D
placeholder (`3d placeholder`, `3d component placeholder`, `3d wiring
placeholder`, `3d appearance placeholder`). It is a `Group` with a filled
`Rectangle` and one `Text` row per line; a row's `Name` is its action
(`m9:tex:3`, `m9:face:0`, `m9:mount:1:3`, …), so it is hit-tested through the
same `ra2ui::input` path as the shipped screens. **This panel is ours, not the
game's**, and it is the one place where a control has no shipped counterpart.

The panel is clamped above the shell's `common buts` bottom bar for the same
reason: the shipped nav strip is drawn over the content area.

## 3. The live preview

The preview is a texture rendered by `ra2-render` itself and composited into
the UI draw list at the placeholder's rectangle:

- the geometry is `ra2_render::chassis_items(&build.to_bot(), identity)` for the
  chassis and `ra2_render::scene_items(&scene, identity, …)` for each attached
  component (the renderer's own item loader, including its texture resolution);
- the model matrices are **`Build::transforms()`**, the model's F1 output;
  there is no second transform rule;
- the camera is an orbit framing around the item bounds. That framing is ours;
  the geometry and transforms are not;
- the painted chassis is visible because `to_bot()` applies `Paint`.

The rendered RGBA is pushed into the `UiScene` as a `UiTexture` and a clipped
`UiQuad`; the window and the offscreen runner draw that scene with the same
`UiRenderer` they use for the shell. The preview is cached by the model's
revision counter and rebuilt after an edit.

## 4. Wiring: read-only this pass

The Wiring tab displays the model's whole `ControlBoard`:

- the shipped `control grid` is filled with `control_cell.gib` cells, one per
  control, at the control's own cell (`cell[0]`, `cell[1]`), with the kind's
  `ctrl_analog`/`ctrl_switch`/`ctrl_button` art, the player's name and the key
  label (`W / S`, `A / D`, `Space`);
- the panel lists every control and every binding (`#4 Spin Clockwise <- Forward @ 1,0 slot 0`).

Adding a control (`new analog`/`new switch`/`new button`), dragging it onto a
cell and choosing an action from the shipped template list is **not** wired.
The button notes and the cell template are there; the interaction is the next
slice. `Test Robot` drives both robots with the shipped AI, not with the
player's board, so a read-only board does not block the match.

A new build uses `ControlBoard::default_board` (Forward/LeftRight/Fire/Spin on
W/S, A/D, Space, Q) and therefore has controls but **no bindings** until a part
is wired; `--start` a stock bot to see the model's full board (the corpus has
707 binding rows).

## 5. Paint

`Paint Shop` lists the shipped texture library (`Texture Library/`, the model's
own file list) and the player's `Custom Textures/`. Selecting a row sets the
texture; selecting a face row applies `Paint::fill_group` to that group's
rectangle on the 256×256 skin. The preview re-renders from
`chassis_items(&build.to_bot(), …)`, so the filled patch is visible
immediately.

The Shipped `decals.gib`/`surfacelayer.gib` operations exist in the model
(`Paint::decal_group`, `Paint::layer_group`) and are **not** wired in this
slice. `fill_all` ("texture entire") is also model-only for now. The
appearance dropdown and its effect screens are not loaded; the panel's list is
the model's `GameLibrary::textures()/custom_textures()`, which is the same
directory the screen's own list is filled from.

Paint survives save/reload: `Build::save` writes `to_bot()`'s painted TGA
through `ra2fmt::bot::write`, and `Build::load` re-reads it. The headless test
compares the skin's pixels after the round-trip.

## 6. Save, import, export, and the round-trip

- **Save As** writes `Game/Robot Designs/<name>.bot`. The name is the filename
  box's contents; `/ \ : * ? " < > |` are refused rather than rewritten. An
  existing file is overwritten silently (the shipped dialog has its own confirm
  string, not modelled here).
- **Open** reads the same directory back through `GameLibrary::read_bot` +
  `Build::from_bot_with_info`; the tree, mounts, transforms, board, stats and
  skin are the model's.
- **Export bot** is `Robot Designs/` as well (the shipped export box names a
  filename); **import texture** lists `Custom Textures/`; **export texture**
  writes the current skin there as a `.tga`.
- The shipped `importbot.gib`/`exportbot.gib`/`import_texture.gib`/
  `export_texture.gib` list boxes are not composed; the panel's rows are the
  same directory operation with the game's own paths.

The headless run writes the design, deletes it after re-reading it (so the
player's install is not littered), and the test uses a temp directory with the
game's entries symlinked and a real `Robot Designs/` inside it.

## 7. Test Robot (M9.5)

`Test Robot` saves the current build and starts `ra2sim::match::Match` on
`Arenas/box.py` as a DEATHMATCH: the player's built bot against
`AI/Team6/Bot2.bot`, the shipped AI. The window shows M8's 3D frame + HUD and
its results screen; the offscreen runner uses `match_mode::run_headless` and
copies the first fight frame to `build-test.png`.

The player's bot is **AI-driven** in this pass, exactly as `--match` drives both
robots; the workshop's control board is not consumed by the AI. The report is
printed (`damage taken`, armour/battery remaining) and, in the window, stored
for the panel's `last test:` line. It is **not applied** to the workshop
state — a damaged component has no workshop representation yet (`ra2sim`'s
damage lives in the match's rig, not in a `.bot`).

## 8. The scripted proof and its numbers

```text
start: 1 parts, 56.1 kg, 160 hp
  chassis step: Some("structure design button") (chassis split)
  attached: 0 Chassis -> -1@0, 1 SpinMotor -> 0@0, 2 Wheel -> 1@3
  paint: 10 group(s), changed true
  saved .../Robot Designs/M9 Demo.bot
  reloaded: 3 parts, 89.1 kg, 960 hp (rig: 3 parts, 89.1 kg, 960 hp)
```

The rig is `ra2sim::rig::load` — the simulator's own loader — and agrees with
the model after save/reload: **3 parts, 89.1 kg, 960 hp** on both sides. The
scene's prints in `--out` are:

| file | what it shows |
|---|---|
| `build-chassis.png` | the chassis tab on the shipped `structure design` step, the preview of the stock chassis |
| `build-parts.png` | the Components tab: the palette, the attached Z-Tek motor + mini wheel, the part list and mount rows |
| `build-wiring.png` | the Wiring tab: the shipped `control grid` with one `control_cell.gib` per control, and the binding rows |
| `build-paint.png` | the Paint Shop: the texture list, the 10 face groups, and the preview with group 0 filled |
| `build-test.png` | M8's HUD + 3D frame of the test match (the first fight frame) |

The test's own frames are 640×502; the manual evidence run above is
1280×1005.

## 9. What is read-only, out of scope, or unproven

- **Wiring editing** — display only; no add/drag/bind.
- **The shipped armour grid** — filled and drawn, but its rows sit under the
  shell's bottom bar; plate selection is the panel's row.
- **Decals, surface layers, texture-entire** — model-only.
- **Chassis drawing** — `chassis_structure1/2.gib` are shown by their own
  buttons, but the freeform/circle/rectangle drawing tools do nothing; a new
  bot starts from a stock chassis mesh, not a player-drawn one.
- **Baseplate placement** — a new chassis child is placed at a fixed spread on
  the chassis' top plate; the game's own is a mouse placement in the 3D view,
  which is not wired. Component-to-component links do use the model's F1 rule.
- **The palette's scroll** — the headless script scrolls the grid to the
  requested row; the window has no scroll-wheel/scrollbar interaction yet.
- **The appearance effect dropdown** — not populated or clickable; the panel's
  texture list is the model's directory list instead.
- **The shipped weight/class boxes** — the panel shows the model's mass,
  hitpoints and computed class; the screen's own `weight`/`weightclass`
  `TextBox`es are not written.
- **Damage** — reported, not applied.
- **Cost** — `None`, as in the model.
- **The windowed path** — built and compiled, but the evidence here is the
  offscreen path; no display was available in this environment.
- **The default control board** — `ControlBoard::default_board` (Forward,
  LeftRight, Fire, Spin on W/S, A/D, Space, Q) is the model's convenience, not
  the game's new-bot state (the tutorial's blank controller).
- **The panel** — ours. It is the only invented control surface; every other
  click is a shipped widget or the game's own data.

## 10. Tests

| test | what it pins |
|---|---|
| `build_mode::tests::a_palette_click_attaches_one_component_and_undo_reverses_it` | one palette row click = one `Build::attach`; undo removes exactly that component |
| `build_mode::tests::an_armour_cell_sets_the_chassis_plate` | a plate row sets the `.bot` armour (steel 20 kg/350 hp, twice the editor's hardness) and is undoable |
| `build_mode::tests::the_scripted_build_saves_reloads_and_matches_the_rig` | the scripted run writes `Robot Designs/<name>.bot`, reloads it, compares model == reloaded == rig, and writes the five frames |
| `cli::tests::the_build_flags_parse` | `--build`, `--start`, `--save-as`, `--seconds`, `--out` |
| the model's own tests | `docs/15-workshop-model.md` |

The workspace gate is `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test --release --offline --workspace` and
`cargo build --release --offline`.
