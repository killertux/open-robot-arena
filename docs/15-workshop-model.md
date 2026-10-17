# The workshop model (M9.2–M9.4, M9.7)

What landed: a tested **model library** for building, wiring, painting and
pricing a robot, over `ra2fmt::bot::Bot`, and the application slice that drives
it (`ra2-app --build`, `docs/17-workshop-ui.md`). The model is engine code; the
window is presentation and click routing over it.

The model lives in `ra2ui::workshop`:

| module | what it owns |
|---|---|
| `build` | the component tree, attach/remove/undo, mount validation, the F1 transforms, save/load |
| `wiring` | the control board, the `.bot` controller/binding rows, the action matrix, key names |
| `paint` | the chassis skin, the face groups, texture fill / surface layer / decals |
| `stats` | weight, hitpoints, weight class, cost |
| `library` | the game's data: `Components.cfz`, `RA2.cfz`, `Texture Library/`, `Custom Textures/` |

Two pieces moved so there is exactly **one** implementation, not a copy:

- the F1 mount rule is now `ra2fmt::mount` (`component_local`, `MountFrames`,
  `MountMode`, `worlds`), and `ra2-render` and `ra2ui::workshop::build` both call
  it. The renderer's `component_worlds_mounted` is a thin wrapper that builds the
  frames from its loaded `.gmf` scenes.
- the factory's hitpoint table, the `fracture` table and the F12 zero-mass fill
  are now `ra2fmt::component`; `ra2sim::rig` calls them. The workshop's stats use
  the same functions.

---

## 1. Component placement (M9.2)

`Build` keeps the `.bot`'s own component list, resolved `ComponentInfo` for every
component (mount points, mass, hitpoints) and the edit history.

### The rules, and where they come from

- **Placement is F1.** A chassis child is placed by its stored transform,
  `translation · quaternion · yaw(angle) · height`. A component-to-component link
  is `A_parent · R_z(start) · R_x(π) · R_z(angle) · A_own⁻¹`, where `start` is a
  burst motor's rest angle. [`Build::transforms`] calls
  `ra2fmt::mount::worlds` — the renderer's own function. `angle` is applied in the
  child's mount frame; the burst start on the parent's side; both pass through the
  child's named point, so the snap survives.
- **Parent before child.** The file stores the tree in topological order and the
  simulator only accepts a parent index below the child's. `attach` appends; a
  removed subtree is removed whole. The model never writes a forward reference.
- **One component per mount.** Of the 946 non-chassis components in the corpus,
  **423** hang off another component and **none** shares a `(parent, parent_mount)`
  pair. That is the socket rule. The **chassis mount is the exception**: all
  **523** chassis children in the corpus name mount `0`, and the chassis has no
  `.gmf` frames, so each is placed by its stored transform; the baseplate takes
  many children.
- **Mount compatibility.** When both assemblies carry attachment types, the
  parent's point must `Attachment::accepts` one of the child's (`axle_m` accepts
  `axle_f`, `base_f` accepts `base_m`). A missing type list is treated as
  "the data does not say", not as a refusal.
- **The 800 kg limit.** The editor's *"Total mass of robot must not exceed 800"*
  check (`FUN_0043ca70`, quoted in `docs/06-deferred-fidelity.md` F10) is
  enforced on attach. It is the **editor's** check: one stock AI bot weighs
  **818 kg** (`AI/Team5/Bot2`), so loaded bots are not refused; they are reported.
- **No size/overlap rule.** The baseplate placement is visual in the game and the
  file records only the transform, so there is no recovered rule to enforce.

### API shape

```rust
let library = GameLibrary::open(&game_root)?;
let mut build = Build::new("My Bot", chassis);          // chassis + empty board
let motor = build.attach(&library, "Components\\ztek1.txt",
                         0, 0, 1, Placement::default())?; // chassis → motor
let wheel = build.attach(&library, "Components\\miniwheel.txt",
                         motor, axle_m, axle_f, Placement::default())?;
let world = build.transforms();                          // F1, per component
build.save_to(&design_path)?;
```

`attach_info` is the same operation with the caller supplying the resolved
`ComponentInfo`; that is how the tests build a bot with no game directory at all.

### Test

`a_built_bot_saves_and_reloads_with_the_same_tree_mounts_and_transforms` builds a
chassis plus a synthetic motor and wheel, saves with `Bot::write`, re-reads, and
asserts the component list, the mount snap
(`world[wheel] · A_own` = `world[motor] · A_parent`) and the full transform vector
are identical. The same test is repeated against the game's real
`anglemotor1`/`miniwheel` meshes (`a_real_motor_and_wheel_keep_the_game_mount_rule`),
including a save to a temporary file and a reload through the game-data loader.

Other placement tests: one mount refuses a second child; the chassis baseplate
takes many; an unknown mount is refused with the available ids; an incompatible
plug/socket is refused; removing a parent removes the subtree, and undo/redo
restore it; a component above 800 kg is refused.

---

## 2. Wiring (M9.3)

The ground truth is the two sections at the end of a `.bot`:

```text
Name: Forward          Name: LeftRight
1 0 1                  2 0 1
2                      2
0 0 17 0               0 0 30 0
0 0 31 0               0 0 32 0
```

The third controller number is the kind — **1 analog, 2 switch, 4 button** — the
first two are the grid cell, and each key row is a DirectInput scan code (17 =
`W`, 31 = `S`, 30 = `A`, 32 = `D`, 57 = Space). A binding row names a component
index, an action string and `cell x, cell y, slot`.

### What each control can drive

The matrix is the binary's own RTTI, not a guess — the complete set of
`Button`/`Switch`/`Analog<T>` template classes:

| control | component | actions |
|---|---|---|
| analog | `SpinMotor` | `Spin Clockwise`, `Spin Counter-clockwise` (2 slots) |
| switch | `SpinMotor` | `On/Off Clockwise`, `On/Off Counter-clockwise` |
| button | `SpinMotor` | `On Clockwise`, `On Counter-clockwise` |
| button | `BurstMotor`, `BurstPiston`, `Cannon` | `Fire` |
| analog / button | `ServoMotor` | the two spin directions |
| analog / button | `ServoPiston` | `Extend`, `Retract` |
| analog | `Steering` | `Forward`, `Backward`, `Left`, `Right` (both slots) |
| analog / button / switch | `Magnet` | `Repel On/Off`, `Attract On/Off` |

A **weapon is never bound directly**. All 94 stock weapons are driven by the
motor that carries them; the corpus' 31 `Fire` rows name a `BurstMotor` (25) or
`BurstPiston` (6). A wheel is likewise driven through its spin motor, not its own
row. A component with no template row (a weapon, a battery, a control board, a
wheel, or an unrecovered `Flamethrower`/`Hovercraft` class) is refused with the
list of actions it *does* allow.

For `Steering`, the slot is the player's key row, not the axis: the corpus has
`Forward` on slot 0 and `Backward` on slot 1 in one bot and the reverse in
another. Both slots therefore allow all four actions; the control's *name* says
which axis it is.

### Corpus numbers

Across the 57 shipped bots:

| fact | count |
|---|---|
| controls | **156** |
| — analog / switch / button | **119 / 10 / 27** |
| bindings | **707** |
| — analog / switch / button | **664 / 12 / 31** |
| distinct action strings | **9** |
| distinct control names | **19** |

The actions are exactly the nine `SHIPPED_ACTION_NAMES`; the binary's template
list defines six more (`On Clockwise`, `On Counter-clockwise`, `Extend`,
`Retract`, `Repel On/Off`, `Attract On/Off`) that no stock bot uses. The model
accepts all fifteen, because they are the game's, and quotes them when a name is
wrong.

Control names are free text in the file; the 19 shipped names are listed as
`SHIPPED_CONTROL_NAMES` for reference. Two bots carry two controls called
`New Analog`, so a name is not unique — `bind_named` uses the first and the
cell-based `bind` disambiguates.

### Test

`a_binding_written_saved_and_reread_is_identical` writes a board wired to a spin
motor, a burst motor and a steering component, saves with `Bot::write`, re-reads
and compares both sections. `every_shipped_board_round_trips_through_the_model`
does the same for all 57 bots (**707** rows) and checks that the corpus' 19 names
and 9 action strings are exactly `SHIPPED_CONTROL_NAMES` and
`SHIPPED_ACTION_NAMES`. `an_unknown_control_name_is_rejected_with_the_boards_list`
asserts the error carries `Forward`, `LeftRight`, `Fire`, `Spin`;
`a_cell_holds_one_control` covers the one-control-per-cell rule.
`bind_checked_refuses_an_action_the_kind_and_class_do_not_allow` covers the
editor's row: `Build::bind_action` calls `ControlBoard::bind_checked`, which
refuses an action outside `actions_for(kind, class, slot)` and quotes the
allowed list. The raw `bind` remains the file round-trip writer.

---

## 3. Paint (M9.4)

The `.bot` carries **one** 256×256 uncompressed TGA for the chassis and, next to
it, the face groups (F10). Each group is one side of the chassis: a polygon on
the skin, a bounding rectangle, and the triangles that dress it. There is **no
per-face texture name** in the file, so "assign a texture to a face group" means
compositing the texture into that group's rectangle and writing the one image
back — exactly what the game's writer stores.

The three shipped paint screens are the three operations:

| screen | its own note | model |
|---|---|---|
| `botlab/texturefill.gib` | *"Click a panel on your chassis to paint it with this texture"* | `Paint::fill_group` |
| `botlab/surfacelayer.gib` | *"Click a panel on your chassis to add this texture layer"* | `Paint::layer_group` |
| `botlab/decals.gib` | *"Position the decal with your mouse and click to apply it"* + `scale slider` | `Paint::decal_group` |

`fill_all` is the screens' *"texture entire"*. The texture list is the files in
`Texture Library/`; the decal list is `Texture Library/decals/`; the edge styles
are `Texture Library/edges/*.txt` (`Filename`, `Preview`, `Length ratio`, `Width`)
and the player's own folder is `Custom Textures/` (any image file, not the
40-byte `CustomTextures.txt` note). `Paint::undo` is the appearance tab's own
*"undo last filter"* button, separate from placement undo.

### The face-group indices

The model's groups are `Chassis.face_groups` in file order, and
`Chassis.faces[i].group` indexes that list — the same flat list the renderer and
the sim use. Across the 57 shipped bots the corpus has **496** face groups and
**1,808** face rows; **60** rows name a group past the end. The reader keeps them
(they are the game's bytes), the renderer ignores them, and the model reports
them (`Paint::face_range_counts`), so the paint target and the sim cannot drift.

`face_groups_are_the_sims_flat_file_ordered_list` asserts group count, bounds,
face rows, indices and the 60 out-of-range rows on all 57 bots.
`a_painted_chassis_round_trips_through_the_writer` paints two groups and a decal,
writes the bot, re-reads it, and compares the face groups, face rows and the skin
pixels. `an_untouched_paint_writes_the_original_bytes_back` covers the
byte-exact no-op case.

### What is our rule, not the game's

The *compositing* is not recovered from the binary: the model stretches the
texture nearest-neighbour into the group rectangle, alpha-blends, treats the
group's `x1 y1 x2 y2` as top-left pixels on a 256×256 skin, and sizes a decal as
`scale × group width` about a normalised point. The screen notes specify the
operations and the file specifies the target patches; the sampling law is ours.

---

## 4. Stats (M9.7)

`BotStats` reports the robot's mass, the sum of the components' structural
hitpoints, the chassis' own hitpoints, the declared and computed weight class and
a `cost`.

The rules are the simulator's, shared through `ra2fmt`:

- chassis mass = `Chassis::mass()` = surface area × armour weight, floored at 5 kg;
- component mass = the definition's `mass =`, else the top-level `.gmf`
  rigid-body masses, else the F12 zero-mass fill (density 139, the value that
  keeps the mini wheel at 8 kg);
- a component with no geometry contributes no mass;
- chassis hitpoints = the armour hardness the bot file records;
- component hitpoints = `hitpoints =`, else the factory's category table
  (`FUN_0045cf40`: power/extenders/mechanics/mobility/weapons/extras →
  400/100/400/400/1000/1000), else the role fallback for an unloaded component;
- weight class thresholds: light < 250 kg, medium 250–400, heavy > 400.

### Measured equality with the sim

The test `every_shipped_bots_stats_equal_the_rigs_own` loads each bot twice —
through the workshop model and through `ra2sim::rig::load` — and compares the
numbers directly, part by part and in total. It also compares the model's
`Build::transforms()` with the rig's per-component `local` and finds a **maximum
matrix-element difference below 1e-5 on all 57 bots**, which is the transform
half of M9.2 pinned to the simulator. **57/57** bots:

| comparison | maximum difference |
|---|---|
| per-part mass | **0.000000 kg** |
| per-part hitpoints | **0.000000** |
| total mass | **0.000000 kg** |
| total hitpoints | **0.000000** |

The corpus range is **188.4 kg** (`AI/Team11/Bot0`) to **818.0 kg**
(`AI/Team5/Bot2`); **1** bot exceeds the editor's 800 kg, and **45/57** declared
weight classes agree with the mass (the class field is a save-time label, not a
rule — `Class:` is not corroborated by the weights).

The built-bot test `a_built_bots_stats_equal_the_rigs_after_a_save_and_reload`
builds a real motor + wheel onto a stock chassis, saves it, loads it with the rig
and compares: **80.1 kg, 960 hp, 3 parts** on both sides, every part equal.

### Cost is not recovered

No shipped component definition, event, team or bot file carries a price, and
the match layer scores points, not money. `BotStats::cost` is `None`. This is
named rather than guessed: a cost column, if it exists in some later build, is
not in the shipped data.

---

## 5. Tests and the gate

New tests, all in the model's own modules plus one `ra2sim` integration test:

| test | what it pins |
|---|---|
| `a_built_bot_saves_and_reloads_with_the_same_tree_mounts_and_transforms` | chassis + 2 components round-trip; component list, mounts, F1 transforms |
| `a_real_motor_and_wheel_keep_the_game_mount_rule` | the same with the shipped meshes, save/reload through the library |
| `one_mount_holds_one_component`, `the_chassis_baseplate_takes_many_children` | the socket rule and its chassis exception |
| `removing_a_parent_removes_its_subtree_and_undo_restores_it` | remove/undo/redo |
| `the_editor_weight_limit_is_enforced` | the 800 kg check |
| `a_binding_written_saved_and_reread_is_identical` | wiring round-trip |
| `every_shipped_board_round_trips_through_the_model` | 57 bots, **707** binding rows; 19 names and 9 action strings match the constants |
| `an_unknown_control_name_is_rejected_with_the_boards_list` | the shipped name list in the error |
| `a_painted_chassis_round_trips_through_the_writer` | paint + face groups + TGA round-trip |
| `face_groups_are_the_sims_flat_file_ordered_list` | **496** groups, **1,808** rows, **60** out of range, all 57 bots |
| `a_custom_texture_is_listed_and_loads_by_name` | the custom-texture folder, list order and image decode |
| `every_shipped_bots_stats_equal_the_rigs_own` | 57/57 exact mass and hitpoints, and the model's transforms equal the rig's `local` matrices |
| `a_built_bots_stats_equal_the_rigs_after_a_save_and_reload` | 80.1 kg / 960 hp on both sides |

The crate is `cargo test --release --workspace` clean (with `RA2_GAME` pointing at
the shipped data the game-gated tests run; without it they skip, as the rest of
the project's corpus tests do) and `cargo clippy -- -D warnings` clean.

---

## 6. What remains unproven

- **Cost.** No shipped table. `None`.
- **Paint compositing.** Fill/layer/decal sampling, the decal's scale law, and
  the top-left rectangle convention are ours; the screens/flags are the game's.
- **Baseplate size and overlap.** No recovered rule; attach does not check the
  footprint.
- **The control grid's dimensions.** The corpus' highest cell is column 3, row 1
  (4 columns × 2 rows at least); the screens' `control grid` does not state a
  size, so the model does not enforce one.
- **The default board.** `ControlBoard::default_board` (Forward/LeftRight on
  W/S-A/D, Fire on Space, Spin on Q) is a convenience; a new game bot starts with
  an empty controller and the player drags controls.
- **Joystick axes.** The key rows are DirectInput scan codes; a gamepad axis
  binding is written through the same rows but no stock bot uses one, so the
  axis encoding is not proved.
- **`On Clockwise`/`On Counter-clockwise`, `Extend`/`Retract`, Magnet actions.**
  Defined by the binary's template classes but absent from the shipped bots; the
  model accepts them, the action-to-slot behaviour is unexercised.
- **`Flamethrower` and `Hovercraft`.** Their template classes exist
  (`Analog<Flamethrower>`, `Analog<Hovercraft>`, `Button<Flamethrower>`), but no
  action string is shipped; the model refuses them rather than invent one.
- **Duplicate control names.** Two bots use `New Analog` twice; what the game's
  `Input(name, index)` does when a name is ambiguous is not recovered.
- **TGA alpha.** The bot format says the game writes alpha 0; a painted skin is
  written with the alpha it has. Untouched skins keep their original bytes.
- **The `default_texture` flag.** Preserved on the `Bot`, not modelled as a
  paint operation; its exact effect when armour changes is the game's.
- **Non-chassis socket sharing.** The corpus has no counterexample, but a modded
  `.gmf` could expose a point intended for more than one child; the model follows
  the corpus.

## 7. The UI, now

The first application slice landed in `ra2-app --build`; its behaviour, frames
and read-only list are in `docs/17-workshop-ui.md`. What it took from this page:

- the tabs and chassis steps are the shipped `BotLab` screens, not a new UI;
- the component palette click is `Build::attach` (the F1 rule), the `remove
  button` is `Build::remove`, and Ctrl+Z is `Build::undo`;
- the parts, mounts and bindings are drawn in a synthetic panel in the screens'
  3D placeholder — the shipped screens have no slot for those lists;
- the preview composites `ra2-render`'s own `chassis_items`/`scene_items` at
  `Build::transforms()`;
- paint uses `Paint::fill_group` plus the model's texture lists, and the skin
  survives `Bot::write`/`parse`;
- Save As/Open are `Robot Designs/<name>.bot`, Test Robot is M8's `Match`.

Wiring editing landed as click-to-place then click-to-bind (`new analog` /
`new switch` / `new button` + a free `control grid` cell, then a component row
and an action row; `delete button` removes). Still open: **wiring drag**,
decals/surface layers/texture-entire in the UI, chassis drawing,
mouse→face-group picking, a mouse baseplate placement tool, and applying match
damage back to the build (the career's repair bench is the separate M10.5
path, `docs/18-hq-ui.md`).
