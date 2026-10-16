# 02 — Reconnaissance Findings

Working notes from Stage 0. Derived from the user-supplied game files in `local/`
(git-ignored), the shipped mod tools, and public community documentation.

## 1. Distribution / packaging

- Source: oldgamesdownload.com →
  `Robot_Arena_Design_and_Destroy_Win_Bladez1992_LGU_Repack_EN.zip` (657 MB) →
  inner `Game Files/Robot Arena 2.exe` (692 MB) + `Readme.pdf`.
- The inner exe is a **Caphyon Advanced Installer** single-EXE bootstrapper
  (`Caphyon SRL` strings + `ADVINSTSFX` marker), wrapping **`RA2.msi` +
  `RA21.cab` + `RA2.ini`** (`PackageCode={4024EB24-C07B-4836-B676-D61B42940F74}`).
- **Extraction recipe (works):** run under Wine silently —
  `wine "Robot Arena 2.exe" /exenoui /qn /norestart` → installs to
  `C:\Program Files (x86)\Robot Arena 2\`.
- Installed base game is **version 1.20** (official patches applied), plus
  `#Mods/` (component packs, total conversions) and `#Tools/` (mod toolkit).
- Installed tree: **738 MB, 1770 files**.

## 2. Installed layout

| Path | Contents |
|---|---|
| `Robot Arena 2.exe` | Game binary, PE32 GUI, 3,260,416 B, sha256 `bc0f6833…351a4f` (2006). |
| `python22.dll` | **Embedded CPython 2.2** runtime. |
| `D3D8.dll`, `D3D9.dll`, `dgVoodoo.conf`, `dgVoodooCpl.exe` | Repack ships **dgVoodoo2** (D3D8/9 → modern API) for modern Windows. |
| `Scripts/` | Python game scripts: `Main.py`, `Practice.py`, `Gooey.py` (+ `.pyc`, `Lib/`). |
| `Components/` | 72 component dirs: `.txt` defs + `.gmf` meshes + texture maps. |
| `Arenas/` | Arena dirs (`practice arena`, `octagon`, `hilltop`, `tabletop`, …). |
| `AI/Team0..14/`, `Teams/` | AI bindings + team data. |
| `Robot Designs/`, `*.bot` (57) | Saved/stock robots. |
| `UI/`, `Texture Library/`, `Custom Textures/` | UI art + shared textures. |
| `Music/`, `Sounds/` | 273 `.wav` + 3 `.mp3`. |
| `Components.cfz`, `RA2.cfz` | **CFL3** archives (component defs + misc). |
| `UI/help/*.gib` (118) | Text GUI layout scripts. |
| `Events.txt`, `Events_2Player.txt`, `ra2.cfg` | Match/event tables + config. |
| `#Tools/` | GMF Compiler, BOT Editor, CFZ Compiler, MakeCFL/UnCFL, Component Exporter. |
| `#Mods/` | Component packs (zip/exe/rar) + Total Conversions (DSL, Darkpack, AceUplink/BattleBots, …). |

## 3. Engine / technology

- Developer/publisher: **Gabriel Interactive** (Gabriel Entertainment) / Infogrames.
- PE imports (authoritative):
  `d3d8.dll`, `DINPUT8.dll`, `DSOUND.dll`, `WINMM.dll`, `python22.dll`,
  `WS2_32.dll`, `ole32.dll`, `ADVAPI32`, `SHELL32`, `GDI32`, `USER32`, `KERNEL32`.
  → **Renderer = Direct3D 8**, input = DirectInput8, audio = DirectSound.
- Physics: **Havok** (rigid bodies/constraints; the "Havok explosion" glitch is a
  known engine behaviour we must reproduce for fidelity).
- Scripting: **embedded CPython 2.2** with **Boost.Python** bindings. Large numbers
  of MSVC-mangled Boost.Python symbols in the exe encode full C++ signatures.
- Networking: Winsock (multiplayer) — `Events_2Player.txt`, synced messages.

## 4. Engine API surface (from exe symbols + shipped scripts)

Python modules exposed to game scripts:

- **`plus`** — core C++ bindings. Seen: `plus.Arena.Tick(self)`,
  `plus.addWindow(name,x,y,w,h)`, `plus.addText(window,x,y,w,h)`.
- **`Arenas`** — arena base classes + registry: `Arenas.SuperArena`,
  `Arenas.register(cls)`. Methods on arenas: `SetShadowSource(x,y,z)`,
  `AddShadowTriangle(a,b,c)`, `AddXtra(name, gmf, material)`,
  `AddXtraSound(name, gmf, material, wav)`, `RemoveXtra(name)`, `Tick()`.
- **`AI`** — bot AI module.
- **`Gooey`** — UI helper over `plus.addWindow/addText` (`Plain` class). Note the
  wrapper's own signature against the engine's: `Plain.addText(self, name, x, y,
  w, h)` keeps `name` as its dictionary key and calls `plus.addText(self.window,
  x, y, w, h)` — five arguments, no name. This is the shipped file's call, and it
  is what a `plus` implementation has to accept.
- **`giScript`** — host glue: `giScript.register_function(fn)`,
  `giScript.debug_stdout(s)`, `giScript.debug_stderr(s)`. `Main.py` hooks
  `sys.stdout`/`sys.stderr` into the two and hands `dispatchEvent` to the third,
  which is how the engine calls back into the shell's Python.
- **The engine's own text markup.** The binary carries strings like
  `<text font fg_mcn14 color 0 0 0 1>` and `<image filename "UI\botlab\
  tip.bmp"><text font fg_mcn14 color .5 .6 1 1 align 2> `, so a `Text` widget's
  content can be marked up — font, colour and alignment, and an image beside it.
  What parses it is not found yet, and nothing here renders it: a screen's
  labels are plain strings in the same face these name.

C++ classes exposed via Boost.Python (from mangled symbols):

- `Arena` (callback `giPython::Arena_callback`; ctor takes a `.gmf` path string).
- `AI` (callback `giPython::AI_callback`).
- `ArenaManager::ArenaDetail` (singleton held instance).
- `SoundWizard::SWHandle` — audio handles; methods take (name, wav, tuple, bool).

Boot sequence (`Scripts/Main.py`): imports `plus`, hooks stdout/stderr into
`giScript.debug_*`, imports `Arenas` + `AI`, registers `dispatchEvent` with C++
via `giScript.register_function`. → The engine calls back into Python for arena
and AI logic. **Game behaviour is substantially scriptable/observable.**

## 5. Data formats

### `.gmf` — GMI mesh (primary 3D + physics format)
- Binary form of 3ds Max `.ase`, plus RA2 attachment points and physics metadata.
- Preamble magic `GMI`; then version/type ints `3, 1, 0, 15`.
- Chunked nodes: `int id, int flags, int length` (+ payload); `flags` = 2 (container) or 4 (leaf).
- Node ids: `1` Scene, `7` MaterialList, `8` Material, `14` TextureList, `15` Texture,
  `16` Mesh, `17` TransformationMatrix, `18` ObjectList, `2` GeometricObject,
  `4` Camera, `5` Light, `21` AttachmentPoint, `30` SimulationObject,
  `31` RigidBodyCollection, `32` RigidBody, `33` RigidBodyList, `42` ConstraintSolver,
  `44` ConstraintList, `46/47/53` Pivot/Hinge/PointToPoint constraints,
  `49` AngularDashpot, `51` DisabledCollisionPairs.
- Mesh node: `time, pointcount, facecount, texturecount, colorcount, materialRef`,
  then points, faces (tri + material), texture coords, per-face tex indices,
  optional channels/colors, per-face-vertex normals, backface-cull.
- **Physics is embedded in geometry** → maps directly onto a modern physics engine.
- Human-readable twins exist as `*_txt.gmf` (ASCII).

### `.bot` — robot save/export (newline-separated text + binary blobs)
- Sections: Header (version, name, class, 128×128 TGA snapshot), Components
  (chassis first), buckle model, forward heading, controllers, controller↔component
  bindings.
- Chassis: base/top outline + height, 3D model + 256×256 TGA texturemap, face-group
  → texture-region mapping, corner (crush) points, armour data, buckling points.
- Component classes incl. `BurstMotor` (start/stop angles), `SmartZone` (AI events).
- Armour kg/u²: Plastic 8, Aluminium 12, Steel 20, Titanium 16.
- Weight classes: Light <250, Medium 250–400, Heavy >400 kg.
- 3D block: `'274' totalpoints totalindices totalfaces`; `indexcount facecount
  pointcount '0'`; `'RAW'`; normals, texcoords, points, face indices (CCW).
  **Model space is Y-up** (see §7); rotation quaternions are stored `(x, y, z, w)`.

### `.cfz` — **CFL3** containers (Jari Komppa "Compressed File Library 3")
- Magic `CFL3`; index offset/size, then zlib streams (`78 9c`). Holds component
  defs etc. **`ra2fmt::cfl` reads these natively** (`ra2 cfz <f> list|cat`); no
  external `uncfl.exe` needed.

### `.gib` — GUI layouts (the shell's screens)
- **Plain text, CRLF.** `T 1.09` then a sequence of `[Kind]` blocks. Every
  screen in the shell is one: main menu, team HQ, the whole bot lab, the
  in-match HUD, the dialogs — 116 files, 3590 widgets.
- A block is `[Kind]` + field lines. A field line is a label, a space, and a
  value **to end of line**, so values may contain spaces (`Name New [Image]`) or
  be empty (`Notes `). Two blocks carry one **unlabelled** value: an `Image`'s
  texture rect (four numbers, after `_Position`) and a `Glyph`'s (one, after
  `Compositor`) — 1539 and 40 of each, one per widget of that kind.
- **A block is a widget exactly when it carries `NumChildren`**; the five that
  never do — `Background`, `Border`, `Caption`, `DisplayBox`, `Scroller` — attach
  to the widget before them instead of being counted among its children. Nothing
  may depend on field order (`NumChildren` can be last or come before
  `Name`/`ID`/…); what holds in all 116 files is that a widget's own fields all
  come before its children, and that no child block is followed by a field.
- 19 widget kinds: `Window`, `Group`, `SplitFrame`, `Image`, `Text`, `TextBox`,
  `Button`, `CheckBox`, `Rectangle`, `Lines`, `Slider`, `ScrollBar`, `ListBox`,
  `Grid`, `OptionGroup`, `DropDown`, `Placeholder`, `Proxy`, `Glyph`.
- `Position x x_is_fraction y y_is_fraction` and `Size w w_is_fraction
  h h_is_fraction`: **the flag slot says whether the amount is a fraction of the
  parent or a pixel count** (`Position 237 0 191 0` is pixels,
  `ReportedPos 0.5 1 0.07 1` is anchored).
- `Button`/`CheckBox` carry `OnClick`, `RepeatRate`, `RepeatDelay` and up to four
  `StateImage` lines (index, enabled, image); a button's four `Image` children are
  its states, and the caption is a sibling `Text`.
- `_Filename` is relative to `UI/`; the leading `\` or `.\` is noise
  (`\mainmenu\ra2_logo.tga`, `.\botlab\blueprint.bmp`).
- Data quirks worth knowing: `1.#INF` (and `1.#IND`) appear as *numbers* in three
  bot-lab screens — the Microsoft runtime's spelling, written into a texture
  rect; and one `Position` flag slot holds `1242948` in `hq_league.gib`.
- **`Proxy` is the data's own slot.** It carries `Filename`, a `.gib` path, and
  the fragment is drawn inside the proxy's own rectangle — so nine shipped
  screens compose without any C++: `botlab/botlabwin.gib`, `teamhq/teamhq.gib`,
  the two `select_team`/`repairs`/`event_bracket` screens and four of the
  multiplayer ones all name `common\bottomnav.gib`, the bottom navigation bar,
  and nothing nests a proxy inside a proxy. The binary's `[Proxy]` entry lists
  exactly one field, `Filename ` (its writer sits with the other per-class
  tables at `~0x709200`).
- **`Glyph` is the shell's paragraph.** It carries `_Font`, `_Text`,
  `_ForeColor` and — on its `Decorator` base — `_Margin`, plus the `Scrollable`
  set `Params`/`Margins`/`ParamLimit`/`Compositor` and the one unlabelled value
  a glyph has. `_Margin` is **three numbers, `x w h`**, the box the paragraph is
  laid out in; the widget's own `Position`/`Size` are `0 0 0 0` in all forty.
  The binary's glyph writer (`FUN_00608c20`, called from the per-widget writer
  at `0x616f00`) emits `_Size <flag>` and then one `_Margin a b c` **per entry in
  a list**, and the label it writes first has no newline after it — which is why
  the file line reads `Margins _Size 1`, two fields glued together by the
  writer's own `fprintf`-style call. The `_Text` of a glyph writes its line
  breaks as the **two characters `\n`**, not as a newline.
- **`Lines` is a polyline in point *pairs*.** `Points n` then `Point x xf y yf`
  per point: the eight points of every `Lines` in the corpus are four segments —
  `controls_reminder.gib` repeats `(62,0)` and `(62,70)` so that the pairs are
  the four sides of a rectangle, and `selectbot_computer.gib`'s visible one
  insets its box by two pixels at a `Width` of four, which only reads as a
  rectangle when the list is cut into pairs. A `xf`/`yf` of 1 makes the amount a
  fraction of the frame the polyline sits in — the widget's own `Size` is
  `0 0 0 0` on all 77 of them. Ten are `Visible true`; the other 67 are the
  editor's own guides.
- **`MasterSize` is one axis, `amount is_fraction`.** All twenty-six values in
  the corpus are two tokens — `0.14 1`, `0.5 1`, `90 0`, `24 0` — and the reader
  that took them for a two-axis `Position`-style field found no four-number
  value and fell back to *half the frame* for every split whose master is not
  the first slot. The master keeps its own end of the axis and the other child
  takes the rest: `botlabwin.gib`'s lower split (`Master 1`,
  `MasterSize 0.12 1`) is the worked case, and `repairs.gib`'s `main split`
  (`Master 1`, `MasterSize 48 0`) puts its children at y 48 and y 552, the
  second exactly 48 tall.
- **Open: `NumChildren` does not always nest a file to one root.** A widget's
  count is written by the editor and read here as its child count, so a file
  should come back as a single `Window`. Twenty-eight of the 116 do; the other
  88 leave blocks a level up, and `multiplayer/prematch.gib` ends with
  seventeen roots. It has a visible cost: `credits.gib`'s screen-background
  `metaldots_pattern` tile, written *after* the dialog, becomes the last root and
  paints over everything the dialog draws — as does `selectbot_computer.gib`'s.
  The counts are on disk and the writer wrote them from a real tree, so the
  game's reader must nest by something else (the writer's own code is the place
  to look). Nothing here depends on the nesting being right: placement, clips,
  the selection bars and the proxies are all read from wherever a block lands.
- Fonts are `fg_*.giff` inside `RA2.cfz` (nine of them, referenced by `_Font`),
  not loose files.
- **The binary carries the same table.** The reader's field names sit in one
  block of `.rdata` (about `0x708900`–`0x709400`), each field appearing twice:
  bare, as the key the code looks up, and with the trailing space the writer
  actually emits (`Name`/`Name `, `NumChildren`/`NumChildren `, `_Filename`/
  `_Filename `). The block patterns are there too (`[Image]`, `[Text]`, `[Lines]`,
  `?[Group]`, `?[SplitFrame]`, …). It is arranged by class inheritance, and it
  settles the one thing a reader could get wrong: `[Widget]` owns
  `NumChildren `, while `[Decorator]` — the base of `Background`/`Border`/
  `Caption` — owns `Widget ` and no `NumChildren`. So "a block is a widget iff it
  carries `NumChildren`" is the game's own rule, not an inference from the corpus.
  `NumChildren ` is referenced from `0x5e7e04`.
- **Screens are addressed by name from the C++**, so the names in the layouts are
  the interface: literals like `"Multiplayer button"`, `"computer player button"`
  and `"player icon img"` are in the binary, matching `Name` fields in
  `UI/multiplayer/*.gib`.

### What the numbers mean (the layout model)

- **`Position x xf y yf` / `Size w wf h hf`**, each axis an amount and a flag. The
  flag is `0` (pixels) or `1` (a fraction of the parent's extent along that axis);
  the binary names the two states `To Self` and `To Parent` right where its field
  table sits. An amount with the flag set is ≤ 1 in every shipped file. The one
  exception is a flag slot holding `1242948` in `hq_league.gib` — a typo in the
  data.
- **`Alignment` is a 3×3 anchor** on the widget itself: `1`/`2`/`4` are
  left/centre/right and `8`/`16`/`32` are top/middle/bottom, or-ed together. It
  says which point of the widget is put at the resolved `Position`. Across all
  116 files every one of the 841 widgets that carries the field uses one of the
  nine combinations and nothing else — the measurement that pins it. The worked
  cases that confirm the reading are all in `mainmenu.gib`: the `Window` is
  `9` (left|top) at `0 0` with `Size 1 1` and covers the screen; `logo grp` is
  `10` (centre|top) at `0.5 0.07` and lands 512×76 at x=384, y=70; `sawback grp`
  is `18` (centre|middle) at `0.5 0.5` and lands 640×480 centred; `common buts` is
  `34` (centre|bottom) at `0.5 1` and lands flush with the screen's bottom edge.
- **A screen file is a forest, not a tree.** The `Window` in `mainmenu.gib` holds
  only its first four children; the buttons and the button strip after it are
  *top-level siblings*, placed against the screen. The `Notes` on `armor.gib`'s
  split frame — *"load this into content slot"* — is the other half of the same
  idea: a file can also be a fragment that a slot loads, and those are not
  placed against the screen at all.
- **`SplitFrame`** hands its two children panes: `Horizontal true` puts slot 0 on
  the left and slot 1 on the right, `false` puts slot 0 on top; `Master` names
  the slot that `MasterSize` sizes — **one axis, `amount is_fraction`**, not a
  `Position`-style pair — and the other takes the remainder. The master keeps
  its own end: with `Master 0` its pane is at the start and the other follows
  it, and with `Master 1` the pane is where the master's own child order puts
  it, the other child taking the rest. The screens' own names are the check —
  `"LR split"` is horizontal with `"left slot"` first, `"Left TB split"` is
  vertical with `"UL slot"` first, and `botlabwin.gib`'s vertical split has a
  67-pixel-tall `"Slot0"` bar on top.
- A zero `Size` is the editor's *automatic* size: a `Text` is zero-sized and takes
  its extent from its font (the `fg_*.giff` metrics, not decoded yet).
- **An `Image`'s four unlabelled numbers are its rect in a 1024×1024 texture
  *page*, not a rect inside the file it names.** The shipped files are already
  cropped to their own rect: `(u1-u0)×1024` equals the file's width for every
  *cell* (130 of the 155 rects that lie inside a page; another 104 rects are the
  whole page, where the file is simply the whole image). The 25 that differ are
  *tiles* — `common/metaldots_pattern.bmp` is a 64×64 texture whose rects run up
  to 9.7 pages — so a renderer draws a cell whole and repeats a tile.
- **An attachment dresses the widget that comes *next*.** The shipped writer
  puts a widget's `[Background]`/`[Border]`/`[Caption]` *before* it — every
  attachment in the corpus is followed by a widget, and `armor.gib` has a
  `Background` and a `Border` between one group and the group they dress — so
  the reader carries them forward; a file that is nothing but an attachment is a
  *fragment*, a slot's worth of chrome with no widget of its own. This reader had
  them one widget early, which put every caption in the game on the wrong side
  of its button.
- **A tile repeats at its own size.** `metaldots_pattern.bmp` is 64×64 and its
  box is 341×201; its rect is `0 0 5.32813 3.14063`, and `5.32813 x 64 = 341`
  exactly — in **every one of the 26 tiles in the corpus** the rect is the
  authoring size over the texture's size. So the pattern's period is the
  texture's own size and the widget's frame says how much of it to draw: a
  scroll trough is a 16×16 texture over a 16×973 track, sixty-odd repeats of it.
- **A caption is a `Caption` attachment plus the `Text` after the button.** There
  are exactly **122** `Caption` attachments in the corpus and exactly **122**
  buttons and check boxes whose four state images all have a blank `_Filename` —
  the ones with no art — and each is followed by its text. The game centres it:
  `mainmenu.gib`'s three bar captions are at x = 14, 18 and 9 in a group whose
  buttons are at 32, 105 and 183, so read against the group they pile up at the
  left, while against their buttons (67, 72 and 40 wide) each is roughly centred.
- **The bottom bar's chrome is written after the bar, not in it.** `round left`,
  `round right` and `gray block` carry positions measured against the bar —
  `0 0` and `224 0` with 32-pixel corners in a 256-pixel bar, `gray block` 25 to
  231 — and the same three names at the same positions are in `bottomnav.gib` and
  `bottomnav_main.gib`. The bar's own `NumChildren` says 1, so it is the count
  that is wrong, not the geometry.
- **The selection bars' art is the game's, and it is named oddly.** Every one of
  the 36 `Grid`/`ListBox` selection bars is a `Rectangle` with `ID` 100 or 101
  holding an `Image` with a blank `_Filename`; the art is in `RA2.cfz` as
  **`Point Selection.tga`** — a `.cfz` entry with a space in it — and no shipped
  screen mentions it.
- **And a button has no art at all.** 122 of the corpus's 265 buttons and check
  boxes have four blank state images, and `RA2.cfz` holds `Point Selection.tga`
  and the `UI_Scroll*` family but no button art: what a button draws is its
  caption and nothing else.
- **An `Image` also states `_Size`**, the size its art is *drawn* at when that
  differs from the box the layout gave the widget. A scroll bar's thumb is
  `Size 1 1 1 1` — its track — and `_Size 16 0 30 0`, and drawing it at the box
  stretches a 16×30 ribbed thumb down nine hundred pixels. Where the two agree,
  as they do for the full-screen cement background (`_Size 1280 0 1005 0`), it
  changes nothing.
- **Some of the shell's art is drawn by the game's code, not by the data.** Every
  `Button` in `mainmenu.gib` carries four state images with a *blank*
  `_Filename`, and `RA2.cfz` holds `Selection.tga` and the `UI_Scroll*` family
  (four arrows, the sliders and the trough) but no button art at all — so the
  buttons and the selection bars are the C++'s, with only those names to go on.
  What the engine draws for a button today is therefore nothing.
- **And the file is sampled 0..1, not at the rect.** Because the shipped file is
  already cropped to its cell, `u` and `v` are positions inside a 1024² *page*
  and not offsets into the file: `mainmenu/ra2_logo.tga` is 512×76 and its rect
  starts at `u0 = 0.15625`, so sampling at `u × 512` draws the *middle* of a
  512-wide logo. Drawing the file across the whole frame — which is what the
  rect being a cell already means — is what puts the logo on the main menu. The
  rect's only jobs are telling a cell from a tile and a whole-page image.
- **`ChildOrder` is the draw order, ascending — among siblings.** Within
  `mainmenu.gib`'s window the cement background is 1, the saw backdrop 2 and the
  logo group 3, in file order too, so drawing the file's order there would paint
  the background over the logo. A button is −1 and the `Text` beside it is 0,
  which is what puts a label on top of its button, and a button's four state
  images are 0–3 with only the first visible. **The top level is not sorted**:
  the same file's blocks are the window (3), then the `Multiplayer button` (−1)
  and its `Multiplayer` label (0), then the bottom bar (4) and its decorations
  (5–7). Sorted by `ChildOrder`, the button and its label would be drawn *before*
  the window and the window's full-screen cement would cover both; in file order
  they land on top of it. The engine draws the screens this way and the composite
  matches it.
- **Some widgets are placed at run time by the C++.** The two rounded corners of
  the main menu's bottom bar (`common/rounded_rectL/R.tga`) sit at `0 0` in the
  layout and only get moved when the screen is built, so a static composite of
  the file puts them in the corner. The `Name`s are the handle the code holds
  them by.

#### Containers: the widgets that place their own children

Four kinds do not place their children from the children's own `Position`:

- **`ClipMode` is `0`, `1` or `2`** — the base widget's byte at `+0x78`, right
  after `Visible` at `+0x74`, written by the `.gib` writer from there. Across the
  3590 widgets of the corpus it is only ever 0 (2288), 1 (1287) or 2 (15).
  `1` clips the widget's own drawing *and its children* to the widget's frame;
  every container by nature is a 1 (`Window` 42, `Group` 460, `SplitFrame` 40,
  `ListBox` 23, `Grid` 12, `OptionGroup` 20, `ScrollBar` 30, `Slider` 41,
  `TextBox` 331, `Button` 193, `CheckBox` 69, `DropDown` 12, `Placeholder` 12),
  and only two `Image`s in the whole game bother. `2` clips to the *parent's*
  frame instead: **eleven of the fifteen** are placed so that they stick out of
  the parent, where a self-clip would do nothing — the two `robots grid`s
  anchored at the parent's bottom edge, the bot lab's five `selected label`
  textboxes, `botlabwin.gib`'s `wires grp` at `x = -1.0`, and
  `event_bracket.gib`'s heading textbox at `y = -20` with a height of 24. The
  four that are inside at authoring time are chrome that moves afterwards: the
  two selection bars, which the grid repositions down its rows, and two of
  `comp_properties.gib`'s tabs.
- **`Grid`/`ListBox` cells**: `Rows`, `Columns`, `RowHeight` and `ColWidths`
  give the cell rectangles. `ColWidths` is **two numbers per column** and the
  first is the width — the x of a cell is the widths before it, its y is
  `row × RowHeight`. The binary's own hit test walks a row the same way, adding
  each column's width to x. The second number is `17` in **every one of the 36
  shipped grids and lists**, so it is recorded and not interpreted.
- **`Selection` and `Highlight` name children by `ID`** (100 and 101 in all 36,
  and the writer round-trips the child *pointer* it keeps by writing the child's
  ID). `RowSelect` decides the span: `true` covers the whole row (the sum of the
  column widths), `false` one cell. The shipped files have done that arithmetic
  themselves, and the corpus checks it: all **37** bars hang from the row they
  name at the row height, at the summed x of the column they name, and 31 of the
  37 are as wide as the columns they span. The six widths that differ are the
  authors' own numbers, not a second rule: `arena.gib` 159 where the columns sum
  to 300 and `prematch.gib` 187 where they sum to 136 are both the *container's*
  width, and the three `wiring_connection_*.gib` fragments want 161 where their
  column sums to 177 — 161 is the *slot* they are loaded into, in a screen the
  fragment does not name.
- **`ScrollBar`** names three of its children by **index** — `Less true 0`,
  `More true 1`, `Slider true 2` — and in all 30 shipped bars the index, the
  child's own `Name` (`Less`, `Down`) and its kind (`Slider`) agree, which is
  what pins it. `Orientation` is `1` for vertical (29) and `2` for horizontal
  (1). The less button opens the bar, the more one closes it, and the third
  child (a `Slider`) is the track between them. A `Slider` in turn names its
  `Thumb` child by index and travels `From`..`To`: its `To` is the track less the
  thumb — 68→38, 160→130, 48→18, each with a 30-pixel thumb — so the thumb's
  resting place is state, and the file records the authoring-time value.
- **`OptionGroup`** names its selected child by **`Name`** (`Selection video
  tab`), not by index or id, and its children keep their own rectangles: it is
  selection state, not layout. **`DropDown`**'s parts are authored: its button at
  the right edge, its textbox across the left, its list below — all three agree
  with the container's own box in every shipped dropdown.
- **A tile is a repeat, and the trough is one.** `UI_ScrollBack.bmp` — the
  scroll bar's trough — is drawn at uv `(0, 0, 1, 17.94)`, nearly eighteen pages
  of screen, so it is a pattern to be repeated, like `metaldots_pattern.bmp`'s
  9.7 pages. A `uv` that runs past one page means *tile*; one that does not but
  still fails the cell test is a whole-page image whose file is bigger than a
  page (the 1280-wide cement background), and drawing it whole is right.

### `fg_*.giff` — the shell's bitmap fonts (inside `RA2.cfz`)
- Nine of them, named by every label's `_Font`: `fg_d18/20/24/26`,
  `fg_dcn14/16`, `fg_mcn12/14/16` (plain, condensed, and the `d`/`m` pair).
- Fixed layout, and the arithmetic is exact on all nine files:
  `68` bytes of header, then `95` records of `28` bytes (three `i32`s and four
  `f32`s), then the page — `256×256×2` for the small faces
  (`68 + 95*28 + 256*256*2 = 133800`) and `512×512×2` for `d24`/`d26`
  (`= 527016`).
- The header is `ascent`, `descent` (negative), a third metric, `1.0f`, and the
  page size. A record is an advance, a bearing and a third value, then the
  glyph's rect **in the page, as a fraction**.
- The page is **16-bit**, and its two bytes are two pictures of the same glyphs:
  the high byte is the antialiased coverage and the low byte a hard 1-bit mask.
- **The loader is `FUN_005cd560`** (reached from the string *"Failed to load
  font"*). It rasterises from character `0x20` upwards with a counter of `0x5f`
  (95) — one record per character — and it picks the page size from the face:
  256 for anything under 21, 512 under 41, else 1024. The metrics it copies are
  GDI's, scaled out of 26.6 fixed point with `>> 6`. The page is written row-major
  from the top, and the whole thing is a **cache in the user's AppData**
  (`SHGetSpecialFolderLocation(CSIDL_APPDATA)`), which is why the shipped files
  are prebuilt. There is a `[Begin Font Config]` / `[End Font Config]` section
  and a `glyph_name` field for it, which the game writes and the shipped
  `ra2.cfg` does not carry.
- `ra2 font <RA2.cfz> <name> [--page out.pgm]` prints a face's metrics per glyph
  and writes the page out; `ra2ui`'s `Fonts` loads all nine from a `RA2.cfz` so a
  resolver can size the labels the editor left at zero (`Size 0 0 0 0`), which is
  what every `Text` in every shipped screen is written as.
- **Which record is which character: `(c - 34) * 28`.** Measured by drawing: at
  that offset `mainmenu.gib`'s own `MULTIPLAYER TEAM HQ` label comes out reading
  `MULTIPLAYER TEAM HQ`, and at 32/33/35/36 it does not. The loader's loop starts
  at `0x20`, so the shipped files and the loader's enumeration disagree by two
  characters — most likely the game's font tool against a later loader.
- **The table is 96 records at offset 40**, not 95 at 68: `40 + 96*28 = 68 +
  95*28 = 2728` fits either way, and what settles it is the runtime object the
  loader fills — its glyph array is `0x600` bytes of four floats, which is 96
  glyphs of 16 bytes, and its integer array is 96 of 12.
- **Verified, and there is no slip.** Drawn out, the label reads
  `MULTIPLAYER TEAM HQ 0123456789` exactly; all 96 rects are distinct, so even
  `Q` has its own cell. Three earlier readings that said otherwise were all the
  *reading*, not the data: the compositor's text blend added `v/255` instead of
  blending by coverage; the compositor sampled each glyph's rows **turned
  over**, which is why an `M` came out as the `V`-ish shape that made
  `MULTIPLAYER` read as `VELAIIPLOA ER`; and the offset was then re-measured
  with the turn removed, which is the reading above. `Font::page` and
  `Glyph::rect` already agree — the ink sits at the rows the rect names, first
  row first — and only the *uv* in a record is measured from the bottom. The
  extent helper uses the **median** advance for a character the table has no cell
  for, which is the space.

### Other
- Textures: BMP / TGA / JPG (mostly 256²/128²); many uncompressed TGA.
- Audio: WAV (273) + MP3 (3).
- `Components/*/…`: `.txt` (JSON-ish component definition) + `.gmf` + texture maps.

## 6. Tooling obtained

- `tools/vendor/PyRA2/` — reference parsers for GMI/GMF + `.bot` (LGPL, Python 2).
- Game's own `#Tools/` — GMF Compiler (text↔binary GMF + 3ds Max exporter),
  BOT Editor (TGA extract/import), CFZ Compiler, MakeCFL/UnCFL, Component Exporter.
- `tools/extract_sfx.py` — stream-zip extractor (superseded by Wine install).

## 6b. Full script API inventory (from shipped `.py`)

The game ships readable Python under `AI/` and `Arenas/`, which together with
`Scripts/` document the engine bindings:

**AI layer** (`AI/*.py`): `AI/Bindings.py, Chopper, Flipper, Poker, Pusher,
Rammer, SimpleWeapon, Spinner, Whipper, Tactics, Other, nobrain, __init__`.
- Base class `AI.SuperAI` with `Activate(active)`, `Tick()`, `Input(trigger, x, y)`,
  `RegisterSmartZone(name, n)`, `CanDriveUpsideDown(bot)`; attrs `sensors`,
  `weapons`, `tactics`, class attr `debugging`.
- Modules imported by scripts: `plus`, `AI`, `Arenas`, `Gooey`, `Tactics`, and
  `AI.vector3` (a 3-vector type exposed from C++).
- `plus` functions seen: `plus.isDefeated(bot)`, `plus.isUpsideDown(bot)`,
  plus earlier `plus.Arena.Tick`, `plus.addWindow/addText`.

**Arena layer** (`Arenas/*.py`): `box, Bridge, clawtop, compressor, electric,
event_results, flextop, half, Hazards, hilltop, king, octagon, parkinglot, Skull,
__init__`.
- Base class `Arenas.SuperArena` (+ `Arenas.register(cls)`), with
  `SetShadowSource`, `AddShadowTriangle`, `AddXtra`, `AddXtraSound`, `RemoveXtra`,
  `Tick`, and arena class attrs `name`, `preview`, `in_list`.

**Implication for the port:** bot AI, arena hazards, and a lot of match logic are
*Python data*, not compiled code. A native engine needs to reproduce the `plus` /
`AI` / `Arenas` / `Gooey` object model and either embed a Python runtime (to run
the original scripts) or provide a native reimplementation of the same API.

## 7. Robot assembly (`.bot`) — findings

`.bot` files are line-based text with two embedded uncompressed TGA blobs
(a 128×128 snapshot and, for the chassis, a 256×256 skin). Handled by
`ra2fmt::bot`; all three shipped formats (1.07, 1.08, 1.11, 1.12) now parse.

**Coordinate system.** RA2 *model/mesh* space is **Y-up** (not Z-up as the
community doc claims): the chassis mesh's two plates are separated along Y
(its Y extent equals the stored chassis height) and the component `.gmf`
meshes agree — the mini-wheel is thin in Z (upright, axle horizontal), the
caster and control board are thin in Y (flat). The whole renderer works in
Y-up space.

**Quaternion order.** The stored rotation quaternion is **(x, y, z, w)** — not
`(w x y z)` as the doc says. Confirmed by PyRA2's `quatToAxisAndAngle` (which
reads element 3 as the scalar) and by the chassis rotation `0 0 0 1`, which is
identity only under `(x,y,z,w)`.

**Component placement.** There is **no mount-point alignment**: a component's
world transform is simply chained from its parent as
`T(translation) · R(quaternion) · Ry(angle) · T(0, height, 0)`
(`angle` yaws and `height` offsets along the up axis). A `BurstMotor` also
poses its children at its start (rest) axle angle — approximated as a Z-axis
rotation of the children.

**Chassis geometry.** The chassis mesh is embedded in the bot in **normalized
units** that are consistently ~2.2× the world units used by the component
translations, and its footprint maps onto the outline points (`outline/1000`
≈ world). The outline is editor metadata; the mesh needs no rescale once
placed at the same scale as the components (components correctly end up
*inside* the chassis and wheels stick out past its sides, matching the game's
own snapshots). Chassis skin is Y-up like everything else.

**Version table** (empirically derived from the 57 shipped bots):

| Feature | 1.07 | 1.08 | 1.11 | 1.12 |
|---|---|---|---|---|
| Default-texture int in header | no | no | yes | yes |
| Chassis "unknown section" flag | no | yes | yes | yes |
| Chassis buckling points | no | no | yes | yes |
| Buckle model + forward heading | no | no | yes | yes |

**Component definitions** (`.txt`) live inside `Components.cfz` — a **CFL3**
archive (Jari Komppa "Compressed File Library 3", zlib license). Now read
natively by `ra2fmt::cfl` (no `uncfl.exe`/Wine needed); `ra2 cfz <f> list|cat`
exposes it. Format: `CFL3` + index offset + index size; at the index a
compression id + a length-prefixed resource holding the directory; then
sequential entries (`unpacked_size`, `offset`, `compression`, `name_len`,
`name`). Resources are zlib streams prefixed with the uncompressed size and
(missing) trailing adler32.

**Still approximate / open:** the chassis *face-group* texture decals (a
"painter" feature) can appear with rotated/mirrored orientation because the
mesh's own UVs are used; and exact BurstMotor weapon rest poses need the full
axle/attachment solver.

## 8. Arena scenes (from the Python scripts)

Arena *behaviour* is Python, but the scene it describes is data. `ra2fmt::arena`
does a static scan of `Scripts/*.py` and `Arenas/*.py` to recover:

- the **main model** — first quoted string in an `__init__(self, "…")` call;
- **extra props** — `AddXtra(name, gmf, material)` and
  `AddXtraSound(name, gmf, material, wav)` (e.g. the practice arena's obstacle:
  crates / cinderblocks / barrels / cones / ramps);
- the **shadow light** (`SetShadowSource`) and flat shadow-receiver triangles
  (`AddShadowTriangle`);
- the play-field `extent` and the script's **static cameras**
  (`AddStaticCamera`).

The renderer assembles a scene (`.py` input → main model + selected props) via
`ra2-render <arena.py> <out.png>` with:
`RA2_OBSTACLE=<name|all|none>`, `RA2_HIDE=<name,...>` (e.g. hide `cieling`/`h_box`
to see indoors), `RA2_CAM=<ex,ey,ez,tx,ty,tz>` or `@<n|name>` (a script camera),
`RA2_CULL=back|front`. `ra2 arena <script.py>` prints the recovered scene.

**Material handling discovered here** (matters for arenas, which are
multi-material):

- Materials are `Standard` or **`Multi/Sub-Object`**; a `Multi/Sub-Object`
  carries **sub-materials** and each face's material index selects one. These
  are now parsed (`Material::sub_materials`) and the renderer batches geometry
  **per resolved material** so each gets its own texture.
- **Collision/physics proxies are marked by name**: objects named `*collision*`,
  `*traction*` (the component def's `contact` object), `*proxy*`, or meshes whose
  material is named `havok` / `collision` / `collision proxy` are not drawn.
- Materials carry a **transparency** value; strongly-transparent ones
  (≥ 0.5, e.g. `12 - Default` room volumes, hazard `zone`s) are invisible
  volumes and are skipped, while mildly-transparent ones are alpha-blended
  (the box arena's glass cube). Geometry is wound **clockwise** (D3D8).
- All 12 shipped arena scripts render; the practice arena's interior renders
  correctly once `cieling`/`h_box` are hidden and an obstacle is selected.

## 9. Tooling we now have (engine)

- `ra2fmt::gmi` — `.gmf` meshes (181/181), materials (+ sub-materials, transparency), textures, attachments, Havok bodies.
- `ra2fmt::bot` — `.bot` header + components + chassis mesh/outline (all versions).
- `ra2fmt::arena` — arena scripts → scene (model, props, shadow, cameras).
- `ra2fmt::component` — component `.txt` definitions; `ra2fmt::cfl` — native CFL3 archive reader.
- `ra2sim` — physics: backend-neutral `PhysicsBackend` trait, Jolt backend, arena
  colliders, jointed robot bodies with a hinge per motor, movable props from
  script rigid bodies.
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`, `ra2 arena`.
- `ra2-render` — headless wgpu renderer for `.gmf`, assembled `.bot` robots, and
  arena `.py` scenes (all 57 bots and 12 arenas render).
- `ra2-sim` — headless physics sandbox: drive a robot in an arena, write frames.

## 9b. Physics data in the assets (M2 findings)

- **Arena obstacles are real rigid bodies.** `Arenas/<prop>/<prop>.gmf` (crates,
  barrels, cinderblocks, cones, ramps) carry a `RigidBodyCollection` whose body
  names match the mesh object names, with mass/friction/elasticity. E.g. crates
  are 40 kg (friction 0.50, elasticity 0.30); barrels 1500 kg (friction 0.10).
  So the props can be simulated with the game's own parameters.
- **Arena collision geometry.** Some arenas ship explicit collision meshes
  (octagon `main_collision`/`outerfloor_collision`, hilltop `Collision Box`, ...)
  tagged with a `havok` material; others (practice arena) have none and rely on
  their visible geometry. Using the rendered geometry as the collider works
  everywhere.
- **Wheels.** `.bot` components include class `Wheel` (e.g. `Wheel(Mini wheel)`);
  the component `.txt` gives `grip` and `resistance` and names its `contact`
  (traction) and `passthru` (collision) objects — the data needed for a
  per-wheel physics model in M3.
- **Jolt collider conventions.** Jolt's triangle-mesh collider is one-sided (the
  counter-clockwise side is solid), the opposite of RA2's clockwise-outward
  winding, so triangles are rewound when building the static arena collider.
- **Scale.** RA2 units map cleanly to metres (a small robot is ~1.3 x 1.8 x 1.05);
  the robot's origin sits at the chassis bottom with the wheels reaching ~0.3
  below it.

## 9c. Component model & combat data (M3 findings)

- **Base classes** across the 57 shipped robots: `Chassis`, `Wheel`, `SpinMotor`,
  `ServoMotor`, `BurstMotor`, `BurstPiston`, `ServoPiston`, `Weapon`, `Battery`,
  `AirTank`, `ControlBoard`, `SmartZone`, `AxleMount`, `Steering`, `Component`.
  The `.bot` stores that class per component; the definition `.txt` adds a `base`
  (usually identical) and the behaviour fields.
- **Mounts are a tree.** A component's parent is an index into the component
  list, and `component_local` chains translation + quaternion + mount `angle`
  (yaw about +Y) + `height` (offset along +Y). The quaternion is stored
  `(x, y, z, w)`. **Caveat:** each component also names a mount point on its
  parent and one on itself (`ra2fmt::bot::Component::parent_mount` /
  `own_mount`; the format document has those two labels swapped), and a `.gmf`
  attachment point carries a full frame (`ra2fmt::gmi::Attachment`). The original
  aligns the child's attachment frame to the parent's, which we do not — see
  `docs/06-deferred-fidelity.md` F1. Chassis children are placed by the stored
  transform alone (that is why the wheels come out right); only
  component-to-component mounts are affected, and there orientations can be off
  by 90°.
- **Motors are defined by attachments, not a field.** The definition's
  `attaching = axle` names the attachment type its children use, and the mesh's
  attachment point with `Attach = Axle_M` carries `Host = Axle` — the *rotating
  hub object*. So `master` is the body object and the hub comes from the
  attachment payload. Attachment payloads are CRLF-separated `Key = Value` lines
  (not space-separated, as it looks once whitespace is collapsed).
- **Everything spins about local +Z.** Saw blades and wheels are discs lying in
  XY (`saw_display` x/y ±0.327, z −0.056..0.028; `miniwheel` x/y ±0.197,
  z −0.131..0.004), so +Z is the axle in component space for both motors and
  wheels.
- **Weapon and durability stats live in the `.txt`**: `concussion`/`piercing`
  (damage mix), `frequency`, `hitpoints` (only some parts declare it),
  `damageable = false` (14 parts, e.g. control boards, anchors, casters and axle
  mounts — indestructible), `fracture`, `mass`, `material = metal(.4)`.
- **Part mass comes from the mesh**: each component `.gmf` has an `RBCollection`
  whose body masses sum to the component's mass (angle motor 6 + 10 = 16 kg,
  Z-Tek 25 kg, saw blade 16 kg).
- **A `.bot` also stores control wiring** at the end of the file (`Spin Clockwise`
  / `Spin Counter-clockwise` with control-board output ids), which is the natural
  hook for M4's script layer.
- **Damage attribution cannot be solved from geometry alone.** A hull contact
  does not imply that any *single* component's bounding box overlaps the
  opponent's, because a hull face can span several components; the engine
  therefore uses Jolt's `WereBodiesInContact` for the contact and attributes the
  damage to the nearest destructible part.

## 9d. Physics ground truth (validation pass)

Two sources make the original's physics fully readable, and both are now the
engine's reference:

- **`*_txt.gmf`** — the ASCII export of every stock arena's `.gmf`, in
  `local/game/Arenas/*/**_txt.gmf` (25 files, including `octagon_txt.gmf` and
  `practice_arena_txt.gmf`). They list the whole Havok model in text:
  `GMID_HAVOK_SIMOBJECT` (gravity, `WORLDSCALE`, `SIMTOLERANCE`, `RESOLVER`,
  drag, deactivator, `SHORTFREQ`/`LONGFREQ`, `USE_FAST_SUBSPACE`,
  `UPDATES_PER_TIMESTEP`), `GMID_HAVOK_RBCOLLECTION` → `RIGIDBODY` per body
  (mass, elasticity, friction, optimization level, unyielding, simulation
  geometry, geometry/display proxy, disable collisions, inactive, node TM).
- **`Armor_Definitions.txt`** — inside `RA2.cfz`; the armour table.

Recovered facts:

- **Gravity lives in the arena file, per arena.** Stock is `-9.80998`; the
  practice arena is `-12.752974` (1.3 g).
- **`WORLDSCALE 39.37`** (inches per unit) everywhere — a Havok conditioning
  scale, not a gameplay unit. The game unit is ~1 m.
- **Fixed collision geometry is a mass-0 rigid body.** Triggers are marked
  `DISABLE_COLLISIONS 1` and must be skipped. Naming is `*_collision` /
  `h_*` (`h_box`, `h_floor`), and the exporter readme says plainly: "Most maps
  have a `h_floor` or similar name object. This defines the actual collision
  bounding box and without it robots will fall through a map."
- **Arena friction/elasticity are per body** and are real values, not defaults:
  octagon floor 0.80, hilltop 0.90, practice 0.50; elasticity 0.10–0.30.
- **Movable arena bodies are mass>0 rigid bodies**: crates 40 kg (fric 0.50,
  elas 0.30), octagon start blocks 200 kg, hammers/hellraisers 100 kg, box
  spikes 40 kg, floorsaw 100 kg.
- **`SIMULATION_GEOMETRY` / `GMID_COLLISION_*`** describe mesh / proxy / plane
  shapes. Values observed in stock arenas: 2, 3, 5, 6; 5/6 are the **display
  proxy** (every arena's main collider uses 5 and names its visual group node),
  and `HAVOK_GEO_TYPE` is only ever `Standard` (a mesh) or `Plane` (an endless
  plane primitive placed by the body's `NODE_TM`). Decoded in F5.
- **The chassis carries its own armour**: the `.bot` armour row is
  `<type> <weight> <unused> <hardness> <unused>`, and **chassis mass = armour
  weight per square unit × chassis mesh surface area** ("a 3d face with an area
  of 1 square unit using steel armouring, weighs 20 Kg").
- **Buckling is chassis deformation**: the game subdivides the chassis faces and
  shifts "buckle points" down/in on each hit; it is separate from parts falling
  off.
- **Motors are exported as Havok constraints** ("Motors should now decompile and
  recompile properly. Was due to a small bug in Constraint Handlers
  compilation" — GMF compiler readme), and the runtime exposes `ApplyTorque`,
  `*ANGULAR_SPEED` and `GetMotorAngle`.

Engine: **Havok Hardcore 1.7.1**, statically linked (build path `hk171`).

## 10. Open questions for Stage 1/2

1. Direct3D 8 usage specifics: vertex/pixel shader model, fixed-function pipeline,
   render state set — from PE imports + Ghidra disassembly.
2. Boost.Python registration tables → enumerate the *complete* `plus`/`Arenas`/`AI`
   API (method names, not just C++ signatures).
3. `.bot` "Skin" and `.gib` full grammar; `.cfz` index layout (verify vs `uncfl`).
4. Arena scripting beyond `Practice.py` (mods add more: check Total Conversions).
5. ~~Havok version + parameters to match simulation feel.~~ **Answered** —
   Havok Hardcore 1.7.1; every parameter is readable from the arena `.gmf` and
   mirrored by the engine. See `docs/05-validation-report.md`.
6. Copy protection: base 1.20 build appears unprotected (no SafeDisc/SecuROM import).
7. **How the game's `.gib` reader nests a file.** The writer's `NumChildren` is
   on disk, but read that way only 28 of the 116 screens nest to one root, and
   the leftover blocks land on top of the dialog they belong behind
   (`credits.gib`'s background tile). The reader — not the writer — is where the
   rule is; `FUN_005e7ca0` is the writer.
