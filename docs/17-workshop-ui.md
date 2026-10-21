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
| select a component | a click on a `components grid` cell (the shipped palette). It only selects: the shipped `comp_preview.gib` fills `component content group` (name, weight, description, `Component option` styles, the part and its green `AP_box_*.gmf` points) | `ra2ui::components`, `placement::selectable_mounts` |
| choose the part's own mount | a click on a green box in the component preview (or the panel's `green box` row) | `Drag::own_mount` |
| choose a style | the shipped `Component option` dropdown (`styles = Label, file, ...`) | `Shell::set_dropdown_items` |
| attach it | the shipped `attach button`: the mass check, then the part follows the mouse over the 3D view; SHIFT turns it (10 deg steps), CTRL raises it (5 cm steps); the part is red and the tip says why while it is refused; a click drops it, a click over nothing cancels (docs/36 §9.18) | `Build::check_attach_mass`, `pick`, `plan`, `judge`, `drop_part` (`ra2ui::workshop::placement`) |
| select a placed part | a click on it in the 3D view (a click over nothing deselects); the panel's part rows do the same | `Build::pick_part` |
| move a placed part | not in the game: remove it and attach it again | |
| remove | the shipped `remove button`: the selected part only, refused while parts hang on it | `Build::remove_leaf` |
| add a control | the shipped `new analog` / `new switch` / `new button` button, then a free `control grid` cell | `ControlBoard::add` |
| select / delete a control | a click on its `control grid` cell; the shipped `delete button` | `ControlBoard::remove` |
| bind a control | select the control cell, then the component row and an action row in the panel | `Build::bind_action` (`ControlBoard::bind_checked`) |
| undo / redo placement | `Ctrl+Z` / `Ctrl+Y` or the panel row | `Build::undo` / `redo` |
| choose a paint mode | the Paint Shop's `mode: texture fill / surface layer / decal` rows | `PaintMode` |
| paint a face group | select a `Texture Library/` row (or `Custom Textures/`), then a face row or the preview | `Paint::fill_group` |
| add a surface layer | the `surface layer` mode, a texture, then a face row or the preview | `Paint::layer_group` |
| place a decal | the `decal` mode, a `Texture Library/decals/` row, a `scale` row, then a click on the preview (the mouse point becomes the decal position) | `Paint::decal_group` |
| undo paint | `Ctrl+Z` on the Paint Shop tab | `Paint::undo` |
| draw a chassis outline | chassis step 1: the shipped `freeform` / `circle` / `rectangle` checkboxes, the `radius` / `length` / `width` sliders, and a click in the shipped `Editing Area`; `clear outline button` clears | `Build::add_chassis_outline_point` / `set_chassis_circle` / `set_chassis_rectangle` / `clear_chassis_outline` |
| move a top-panel point | chassis step 2: click a point in the `Editing Area`, then click where it should go | `Build::move_chassis_outline_point` (moves the mesh ring vertex too) |
| set the extrusion height | chassis step 2: the shipped `height slider` | `Build::set_chassis_height` |
| snap to grid | the shipped `snap checkbox` or the panel's `snap to grid` row | the outline's 100-unit grid |
| Save As | type a name, `Enter` (or the `m9:save` row) | `Build::save_to` → `Robot Designs/<name>.bot` |
| Open / import a bot | type a name under `Robot Designs/`, then the `m9:open` row | `Build::load` |
| export a bot | Save As, or `Build::export_bot` (the `exportbot.gib` operation) | copy in `Robot Designs/` |
| import a texture | the `Custom Textures/` rows in the Paint Shop panel | `Texture::load`, then `fill_group` |
| export a texture | the `export texture -> Custom Textures/` row | `Paint::skin().to_tga()` |
| Test Robot | the `m9:test` row on the tab | `ra2sim::match::Match` with the saved bot |

The component palette click selects one component per click. The click is
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

The same frame stores the camera it drew with (`view_proj` and its inverse, the
preview rectangle and its pixel size). A click is unprojected into a ray: on the
Components tab it is the placement drag's mouse ray (bot-local; it meets a free
socket within 9 cm, else the chassis' `y = 0` plane, `Build::pick`) or, with no
drag, the pick of a placed part (`Build::pick_part`); on the Paint Shop it is tested against the
chassis triangles (Möller–Trumbore), and the winning triangle's group and
barycentric skin coordinate become the paint target. The framing is ours; the
ray-to-baseplate and ray-to-skin laws are the model's data (the blueprint scale
and the face groups' coordinates).

## 4. Wiring: click-to-place, click-to-bind

The shipped `control grid` is the editor's surface (`Rows 3, Columns 4`,
`RowHeight 70`), filled with `control_cell.gib` cells, one per control, at the
control's own cell (`cell[0]`, `cell[1]`), with the kind's
`ctrl_analog`/`ctrl_switch`/`ctrl_button` art, the player's name and the key
label (`W / S`, `A / D`, `Space`). The panel row for a control is the same
selection, and the saved rows are the board's own.

The flow is **click-to-place, then click-to-bind** — no mouse drag:

1. click the shipped `new analog`, `new switch` or `new button`. The panel says
   `placing Analog: click a free cell`; the control's corpus name is
   `New Analog`, `Flip` or `New Button`.
2. click a free cell of the `control grid`. `ControlBoard::add` refuses an
   occupied cell (`CellTaken` names the control already there) and the grid's
   `Rows`/`Columns` bound the click. The cell is now selected.
3. click a component row in the panel. Only components whose class has an
   action for this control kind are listed (a weapon, wheel, battery or chassis
   has none — a weapon is driven through its motor).
4. click an action row: `slot 0: Spin Clockwise`, `slot 1: Spin
   Counter-clockwise` for an analog, one slot for a switch/button. The action
   list is `ControlBoard::actions_for(kind, class, slot)` — the binary's RTTI
   matrix, not a UI copy. The bind goes through `Build::bind_action` →
   `ControlBoard::bind_checked`, so an action the kind cannot give is refused
   with the list it *can* give; the raw `bind` stays the file round-trip writer.
5. the shipped `delete button` removes the selected control and every binding
   that named its cell (`ControlBoard::remove`).

A cell holds one control. A new binding replaces the row in the same
`(component, control, slot)` (the panel unbinds first), so a repeated click
does not grow the board. Placement/board edits are not in the placement undo
history (that history is `Build`'s component tree, not the board); the file is
the source of truth and a save/re-read is identical.

A new build uses `ControlBoard::default_board` (Forward/LeftRight/Fire/Spin on
W/S, A/D, Space, Q) and therefore has controls but **no bindings** until a part
is wired; `--start` a stock bot to see the model's full board (the corpus has
707 binding rows). `Test Robot` still drives both robots with the shipped AI,
not with the player's board.

The panel's action rows are limited to what fits the placeholder; the board
itself is not.

## 5. Paint

The Paint Shop is the three shipped screens in one tab. Three `mode` rows choose
**texture fill**, **surface layer** or **decal**; the list below them is
`Texture Library/` (fill/layer) or `Texture Library/decals/` (decal), plus the
player's first `Custom Textures/` file for fill/layer. Selecting a face row
applies the current mode to that group; clicking the live preview applies it to
the group under the cursor. For a decal the click's point inside the group is the
position (the screen's *"position the decal with your mouse and click to apply
it"*), and the `scale` row steps `0.25, 0.5, 0.75, 1.0, 1.5, 2.0` of the group's
width. Every operation goes through `Paint::{fill_group, layer_group,
decal_group}`; `Ctrl+Z` on this tab is `Paint::undo`.

The preview re-renders from `chassis_items(&build.to_bot(), …)`, so the
composited skin is visible immediately. The panel rows are limited to what fits
the placeholder; the model has every group and the whole library.

`fill_all` ("texture entire") is the one model operation still without a row.
The appearance dropdown and its effect screens are not loaded; the panel's list
is the model's `GameLibrary::textures()/decals()/custom_textures()`, the same
directories the screens' own lists are filled from.

Paint survives save/reload: `Build::save` writes `to_bot()`'s painted TGA
through `ra2fmt::bot::write`, and `Build::load` re-reads it. The headless test
does fill → layer → decal, compares the skin's pixels after the round-trip, and
undoes the layer back to the filled skin.

## 6. Chassis drawing

The chassis tab's own structure steps are the drawing screens, and the model
acts on their shipped widgets:

- **step 1** (`chassis_structure1.gib`): `freeform checkbox` appends a baseplate
  outline point where the player clicks the shipped `Editing Area`; `circle
  checkbox` / `rectangle checkbox` use the click as the centre and the
  `radius` / `length` / `width` sliders as their size. Clicking a horizontal
  slider sets it from the click's fraction; `height slider` on step 2 is read
  vertically. `clear outline button` empties the outline. `Snap to grid` (the
  shipped checkbox, plus a panel row because its state images cover it) rounds
  to the 100-unit blueprint grid.
- **step 2** (`chassis_structure2.gib`): a click on the `Editing Area` selects
  the nearest top-panel point; a second click moves it. The matching mesh ring
  vertex moves with it (the corpus' 57/57 ring rule), so the preview shows the
  slanted side. The `height slider` scales the extrusion's `y`.
- `finished button` validates the outline and reports its point count; it does
  **not** regenerate the mesh (the game's extrusion/UV atlas is not recovered).

The panel on step 1 is a three-row strip at the bottom of the blueprint (the
screen has no 3D placeholder to put it in), so it does not cover the drawing
area. On step 2 it sits in the screen's own `3d view placeholder`.

The drawing rules are the tutorial's: at most 16 points, no two closer than one
grid unit, no self-intersection, last point joins the first. The unit is the
blueprint's own `AP = .22 M` bar: 100 outline units = 0.22 m, 1000 units = 2.2 m.
The 100-unit cell for snapping is our reading of that scale (the shipped
blueprint image does not paint its grid lines).

---

## 7. Save, import, export, and the round-trip

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

## 8. Test Robot (M9.5)

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

## 9. The scripted proof and its numbers

```text
start: 1 parts, 56.1 kg, 160 hp
  chassis step: Some("structure design button") (chassis split)
  chassis: snap off
  chassis: moved top point 0 from [600, 200] to Some([575, 274]) (height 0.74 m)
  placement: component 1 at 0.37, -0.22 m, angle 0.00, height 0.00
  attached: 0 Chassis -> -1@0, 1 SpinMotor -> 0@0, 2 Wheel -> 1@3
  wiring: added New Analog at cell 3,1
  wiring: bound component 1 to "Spin Clockwise" on Some("New Analog") slot 0
  paint: 10 group(s), changed true, modes fill/layer/decal
  saved .../Robot Designs/M9 Demo.bot
  wiring reloaded: 5 control(s), 1 binding(s)
  reloaded: 3 parts, 88.6 kg, 960 hp (rig: 3 parts, 88.6 kg, 960 hp)
```

The rig is `ra2sim::rig::load` — the simulator's own loader — and agrees with
the model after save/reload: **3 parts, 88.6 kg, 960 hp** on both sides. The
chassis' moved top point changes the surface area (89.1 kg before it). The
scene's prints in `--out` are:

| file | what it shows |
|---|---|
| `build-chassis.png` | the chassis tab on the shipped `structure design` step, the preview of the stock chassis |
| `chassis-step.png` | chassis step 2 after the top-panel point move, with the shipped height slider and the panel's snap state |
| `chassis-draw.png` | the Components tab preview after the move: the slanted side the drawing tool produced |
| `placement.png` | the motor after a click on the baseplate preview: the panel's `place 1: click the preview (0.37, -0.22 m, 0 deg, +0.00)` |
| `build-parts.png` | the Components tab: the palette, the attached Z-Tek motor + mini wheel, the part list and mount rows |
| `build-wiring.png` | the Wiring tab: the shipped `control grid` with one `control_cell.gib` per control, and the binding rows |
| `wiring-add.png` | after `new analog` + a click on the free cell 3,1: five cells and the `New Analog` selection (unbound) |
| `wiring-bound.png` | after the component and action clicks: `SpinMotor <- Spin Clockwise @ 3,1 slot 0` in the panel, the same row the save writes |
| `layer.png` | Paint Shop after fill + surface layer on face group 0; the preview shows the composited skin |
| `decal.png` | after the decal mode, a decal row and a click on the preview; the red decal is on the face under the cursor |
| `build-paint.png` | the Paint Shop after all three operations (mode rows, list, faces, scale) |
| `build-test.png` | M8's HUD + 3D frame of the test match (the first fight frame) |

The test's own frames are 640×502; the manual evidence run above is
1280×1005.

## 10. What is read-only, out of scope, or unproven

- **Wiring drag** — the shipped `control grid` note says *"controls can be
  dragged and dropped here"*; this slice uses click-to-place instead (the
  `new …` buttons plus a cell). A drag would be a second gesture over the same
  `add` call.
- **Wiring undo** — the board is not part of `Build`'s undo history.
- **The shipped armour grid** — filled and drawn, but its rows sit under the
  shell's bottom bar; plate selection is the panel's row.
- **Texture-entire** — `Paint::fill_all` is still model-only; the three shipped
  mode operations are wired.
- **The outline→mesh extrusion** — a changed outline is stored and saved, and a
  step-2 point move moves its mesh vertex, but the game's extrusion and UV
  atlas rebuild are not recovered. `finished button` validates, it does not
  regenerate the mesh.
- **`buckle_model`** — a chassis geometry edit drops the saved buckle copy.
- **The palette's scroll** — the headless script scrolls the grid to the
  requested row; the window has no scroll-wheel/scrollbar interaction yet.
- **The appearance effect dropdown** — not populated or clickable; the panel's
  texture/decal lists are the model's directory lists instead.
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
  click is a shipped widget or the game's own data. The chassis step-1 panel is
  a strip along the blueprint's bottom because that screen has no 3D placeholder.

## 11. Tests

| test | what it pins |
|---|---|
| `build_mode::tests::a_palette_click_attaches_one_component_and_undo_reverses_it` | one palette row click = one `Build::attach`; undo removes exactly that component |
| `build_mode::tests::an_armour_cell_sets_the_chassis_plate` | a plate row sets the `.bot` armour (steel 20 kg/350 hp, twice the editor's hardness) and is undoable |
| `build_mode::tests::the_shipped_chassis_slider_and_clear_button_act_on_the_outline` | a click on the shipped `radius slider` sets 400, a click in the `Editing Area` adds a baseplate point, and `clear outline button` empties the outline |
| `build_mode::tests::the_scripted_build_saves_reloads_and_matches_the_rig` | the scripted run draws a top point, places a motor on the preview, fills/layers/decals, writes `Robot Designs/<name>.bot`, reloads it, compares components, stats, board and skin, compares model == reloaded == rig, and writes the twelve frames |
| `build_mode::tests::adding_a_control_binding_it_and_saving_round_trips_the_same_board` | `new analog` + free cell 3,1, bind the motor's `Spin Clockwise`, save/re-read the same board; an invalid action (`On/Off Clockwise` on an analog) is refused; a second control in Forward's cell is refused |
| `ra2ui::workshop::wiring::tests::bind_checked_refuses_an_action_the_kind_and_class_do_not_allow` | the model's refusal and its `allowed` list (`Spin Clockwise`, `Spin Counter-clockwise`) |
| `cli::tests::the_build_flags_parse` | `--build`, `--start`, `--save-as`, `--seconds`, `--out` |
| the model's own tests | `docs/15-workshop-model.md` |

The workspace gate is `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test --release --offline --workspace` and
`cargo build --release --offline`.
