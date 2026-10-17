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
- **A record ends at its own `NumChildren`, and `[Kind]` is optional.** Both
  are what the writer's own code says. `FUN_005e7ca0` (the base writer) emits
  `Name`, `ID`, `Position`, `Size`, `Visible`, `ChildOrder`, `ClipMode`,
  `NumChildren`, and then its children — so a record ends at its count, and no
  field of a record follows it. Checked over all 116 files and every record: a
  `NumChildren` line is followed only by a new record, never by more of its own.
  (The earlier note that a field could follow `NumChildren` was an artifact of
  the headerless records below being read as trailing fields.)
- **A headerless block is the base `Widget` class.** The kind comes from the
  writer's `FUN_005e7c20`, which emits the header from a virtual call — and the
  base class has no name to emit — so the header is simply absent. The binary's
  own schema says which class that is: `[Widget]` is the one entry in the field
  table whose class fields are just `Notes ` and `PassMouse ` (at
  `0x708c6c`), and a headerless record carries exactly those and nothing else.
  There are 758 of them; read as trailing fields of the record before, they made
  a fifth of every file disappear, and the corpus is 4,348 widgets, not 3,590.
- **`NumChildren` counts *named* children only.** Measured over the whole
  corpus, with no exceptions: `sum(NumChildren)` + the number of headerless
  records = `records - 1` in all 116 files. So a headerless child — and a
  button's caption holder, which is headerless too — does not fill a slot, and
  the writer's whole child loop is gated on `0 < NumChildren`, so a record with
  a count of zero has no children in the file at all. `bg group` of
  `credits.gib` is the worked case: its count is 13, its named children are 13,
  and its four headerless ones carry `ChildOrder` 3, 8 and 9 — exactly the gap
  the thirteen leave in 0..12.
- **An uncounted record joins the nearest record that still has a slot free, and
  when none has one it is a record of the file's top level — which is the same
  rule, because the top level is a sibling set too.** The count names only named
  children, so an uncounted record is *extra* by construction and the file does
  not say where it goes; the geometry and the `ChildOrder` do. `mainmenu.gib`'s
  caption holders duplicate their button's `Position`, which makes them the
  button's *siblings* — they land in `sawback grp` because that group still has
  a slot free, and the render shows the caption over its button. When nothing
  has a slot, what is left is chrome: 165 records across the corpus, 150 of them
  with no art at all, and the 15 that carry a panel are authored against a
  full-screen ancestor or are fragments, so their frames come out the same
  either way (`colorfill.gib`'s `instructions` at `0.05 0.2 0.5 0.65` is the
  same frame against the screen as against the full-screen split it sits under).
  The draw order then settles it: those records carry the `ChildOrder` that puts
  them *behind* what they belong to — `import_texture.gib`'s 1282x1007 panel at
  `(-1,-1)` (the screen plus one pixel on each side, i.e. authored against the
  screen) is `-1` against the `Window`'s `3` — so the top level sorts by
  `ChildOrder` exactly as a widget's children do. That is what makes
  `credits.gib`'s "infamous" window visible at all: its `metaldots_pattern`
  panel is drawn behind the dialog rather than over it.
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

### The workshop's own composition

The bot lab — the workshop — is `UI/botlab/botlabwin.gib`: a toolbar of six tabs,
a content area, a 49-pixel strip carrying the shell's navigation bar (through a
`Proxy`), and an info bar. A *tab* is a fragment loaded into a **named slot**, and
both names are the data's:

- **The slot is `content slot`.** Every tab fragment's root record says so:
  `overview.gib`, `chassis.gib`, `components.gib`, `wiring.gib`,
  `appearance.gib` and `armor.gib` all open with `Notes load this into content
  slot`. `testarena.gib` is the exception — its root is `Group "test arena main
  group"` with no note, and the file names the slot further in, on the placeholder
  the arena fills (`Notes load the test arena here (put placeholder into content
  slot)`). The binary looks the string `content slot` up by name (`0x6f0f74`,
  referenced from `0x437d4d`, just before it loads `botlab\overview.gib` at
  `0x6f0f60`).
- **The second slot is `lowersplit`**: `botlab\info_display.gib` (`0x6f0c88`) is
  looked up as `lowersplit` (`0x6f0ca0`) and loaded there (`0x438fdd`). It is a
  `Widget "Info Display"` at 80% width and 18 pixels tall, which lands on the
  pane above the content area and under the nav strip.
- **The tab table is the toolbar's own `Notes`.** Five of the six `CheckBox`es in
  `botlabwin.gib`'s `blab option group` name the screen they show —
  `overview button` says *"click to show 3dview.gib and overview.gib"*, and
  `chassis`, `components` and `wiring` say the same with their own file;
  `test robot button` says *"click to show testarena.gib"*. The sixth,
  `paintshop button`, says *"go to chassis appearance effects"*, which is
  `botlab/appearance.gib` — the file the binary pairs with the string `Appearance`
  (`0x6efda0`, referenced from `0x423099`).
- **The tab's own title is not set.** `title txt` holds `ROBOT WORKSHOP` in the
  file and the binary's per-tab strings (`Overview`, `Chassis 1`, `Chassis 2`,
  `Armor`, `Appearance`, `Test Robot`, …) are the **help** pages — they match
  `UI/help/help_ws_*.gib`, one per workshop area, which is what the shell shows
  when help is asked for. So the toolbar keeps its title and the screen changes.
- **A tab can compose further.** `chassis.gib` is itself a container with its own
  sub-slots (`ch_armor.gib` `into 'right content slot'`, the `Chassis 1`/`Chassis
  2` step screens, `ch_stepbuttons.gib`), and `components.gib` holds a list and a
  component-properties panel that the game fills at run time. That is the
  *interaction* layer, and it is the C++'s, not the data's.

### `_Size`, and the value that means *unset*

An `Image` states the size its art is drawn at in an unlabelled `_Size` field, and
that size overrides the box the layout gave the widget — a scroll bar's thumb is
`Size 1 1 1 1` inside its track and `_Size 16 0 30 0`. **But one value of it means
nothing at all: the design canvas, `1280 0 1005 0`.** It is what the editor writes
when the designer never gave the art a size, and **eighteen fields in fourteen
files carry it** — `teamhq/event_cell.gib` and the three `eventcell_*.gib` among
them. Drawing a 140×80 event logo at the canvas stretches it across the screen, so
the value is read as unset and the art is drawn in its own box. Anything else is a
real size: `88 0 72 0` for an armour icon, `20 0 12 0` for a weight icon.

### How a `ChildOrder` tie is broken, and the screen that decides it

`ChildOrder` orders a parent's children and higher is nearer the front. Where two
children **share** one, the **earlier record is in front** — the tie is broken by
the file with the first of the pair nearest the viewer.

That direction is not what the corpus's shape suggests, and the way it was settled
is worth keeping. There are **328 pairs of siblings** in the shell that share an
order *and* overlap, and their dominant shape is a named control followed by the
label that names it — `common/bottomnav.gib`'s `Options`/`MainMenu`/`Help`
buttons, `botlab/ch_stepbuttons.gib`'s step buttons and their numerals. That shape
looks like evidence and is not: those 122 buttons carry blank state art, so they
draw nothing for the label to be in front *of*, and the pair reads the same either
way round. Of the 328, only **6** put art over a sibling with a `Background` —
`exhibition/arena.gib`'s overlay over its `ListBox`, `teamhq/select_team.gib`'s
over its grid, three in `teamhq/repairs.gib` — and an overlay drawn over a list is
as plausible over as under.

What decides it is a screen whose own note requires a control to be **clickable**.
`botlab/components.gib`'s `components grid` and its `category optiongroup` are
children of `component main group` with the same `ChildOrder -1`, the grid later in
the file; the grid is `Position 0 0 0 0` with `Size 1 1 74 0` and the option group's
row sits at 7% of the parent with a 12% height, so the two overlap. The grid's own
note says *"fill with components in the current category, click to show preview"*
and the option group's says *"grid is filtered based on the category selected in
this optiongroup"* — so those six buttons are what the player clicks to change the
grid, and they cannot be behind the grid's cells. The editor had the same overlap
and never saw it, because only the game fills the grid.

So: **the earlier record is in front**, and the shell's own composition follows the
same rule — the chassis step layer, which is added at run time rather than written
in a file, is inserted *before* the screen it rides over, because it is the one
that has to be clickable.

### The event list, and the two places the shipped file is not regular### The event list, and the two places the shipped file is not regular### The event list, and the two places the shipped file is not regular

`hq_events.gib`'s grid says what fills it — *"fill with this season's events
(event_cell.gib)"* — and `event_cell.gib` says what a cell is: one `Image` and an
overlay, with its only note on the first of them, *"'preview img' child is the
event preview image"*. So a cell is one event's art, and the events are the game's
`Events.txt`.

That file is line-based and every record is the same shape — a name, the
description as `DisplayBox` markup, a count of losing tiers, two lines per tier
(the prize model and its message), the trophy's model, its winning message and its
art, the event's art, the arena and its preview, and four numbers. A record is
therefore `3 + 2 × tiers + 8` lines, and reading it strictly (the file has to be
consumed exactly, or the reader is wrong) gives **25 events**, with the tier count
**2 in eleven of them and 4 in fourteen**.

Two things in the file are defects rather than shape, and both are named rather
than smoothed over:

- **`Robo-Depot`'s second tier names the prizes directory** —
  `Arenas\event_results\prizes\` — and no model in it. It is the only tier whose
  model is not a `.gmf`.
- **The last record is one number short**: every event before it closes with four
  numbers and *Father-Daughter Brunch and Robotic Combat* closes with three,
  because the file ends there.

The file is also **Windows-1252, not UTF-8 or Latin-1**: it carries exactly one
high byte in all 481 lines, `0x92`, the apostrophe in *Hot Dog Harry’s Downtown
Rumble*. Read as Latin-1 that becomes `U+0092`, which no font can draw.

### Which category is current, and a click on an option group

The palette's category is the `category optiongroup`'s selected member, and the
screen states it the way a `CheckBox` states its own state: **`Checked true`** on
`power button`, with the checkbox showing its `len - 2` state image (for these six
that is the `co_*_s.bmp` art — `co_power_s.bmp` visible while every other button
shows `co_*_n.bmp`). A click on a member selects it in its group: the shell finds
the `OptionGroup` the checkbox is under **from the hit path itself** — the last
`OptionGroup` above the `CheckBox` that was hit — sets the group's `Selection` to
the member's name and puts every member into its selected or unselected state
(`ra2ui::input::select_option`). No table of button-to-group pairs is needed, and
the palette then refills for whatever the screen now says.

**One overlap is open, and it is measured.** `botlab/components.gib` puts the
grid and the option group's row in each other's way once the grid has cells:

| widget | `Position` | `Size` | `ChildOrder` | line |
|---|---|---|---|---|
| `comp upper grp` (holds the option group) | `0 0 0.07 1` = 7% down | `1 1 0.12 1` | `-1` | 479 |
| `components grid` | `0 0 0 0` | `1 1 74 0` | `-1` | 1417 |
| `Sel Bar` (the grid's) | `0 0 0 0` | `78 0 72 0`, `Visible true` | `0` | 1439 |
| `High Bar` (the grid's) | `78 0 0 0` | `78 0 72 0`, `Visible false` | `1` | 1493 |

The two are siblings with the *same* `ChildOrder`, so the file order decides, and
the grid is later — which by the draw order puts it in front. The grid spans
y 257.3 to 331.3 and the option group's row sits at 298.8 to 327.0 inside it, so
with cells in the grid a click inside a category button reaches the grid's own
`Sel Bar` instead. An *empty* grid is fine — the rule above is exercised by
clicking all six buttons on the screen as it loads — so what is unresolved is the
pair, not the rule: either the tie goes the other way for this pair, or one of the
two positions was authored against a different parent. What the game shows is the
buttons on top of a filled palette, so something in our reading of the pair is
off. Settling it wants the game's own hit test read for `Grid` and `Rectangle`
(the class-behaviour question, one level down from the base `Widget`), or a second
screen where two equal-`ChildOrder` siblings overlap and the front one is known.

### The chassis steps: a layer, and the class that a hit does not stop at

`botlab/ch_stepbuttons.gib` is the snap checkbox, the finished button and the two
step buttons, and it is a **layer over the chassis structure screens** rather than
part of them. Its own fields say where it goes: its root `steps and snap group` is
`Alignment 36` — *right + bottom* — with `Position 1 1 1 1` (both axes at 100%) and
`Size 0.46 1 70 0`, so it is pinned to the **bottom-right corner of whoever holds
it**. Neither structure screen has a slot for it (`chassis_structure1.gib` and
`chassis_structure2.gib` are `chassis split` — `Slot0` the blueprint, `Slot1` the
tools — and name nothing a 70-pixel group could go in), and both screens' own root
records say `Notes load this into content slot`. So the layer is the screens'
**sibling in that slot**, added after them, and the anchor then puts it in the
content area's corner. The C++ agrees in kind: `FUN_00437770` loads
`botlab\ch_stepbuttons.gib` (`0x6f0d7c`) the way it loads every screen, caching it
in a member and inserting a loaded screen into a slot it looks up by name.

The layer is the only place the two steps live: `step 1 button` is noted *"step 1
of chassis outline"*, `step 2 button` *"step 2 (extrude, top outline)"*, their
labels are literally `1` and `2`, and the binary loads
`botlab\chassis_structure1.gib` and `botlab\chassis_structure2.gib` from the same
table with the labels `Chassis 1` and `Chassis 2`. A click on one swaps the
content area's screen and nothing else.

### A hit does not stop at the base `Widget`

`ch_stepbuttons.gib`'s `step 1 button`, `step 2 button` and `snap checkbox` each
have a **nameless `Widget` one pixel larger drawn over them** — a 26×24 record over
a 24×22 `CheckBox`, holding the `1` and the `2`, at the same `ChildOrder` and *after*
it in the file, so by the draw order it is on top. `chassis.gib` does the same with
its two notes over its two buttons. A design like that only works if the overlay
does not take the click, and the class explains why it does not: the headerless
record is the **base `Widget`**, the writer's abstract class, whose only fields are
its note and `PassMouse` — it has no class behaviour to run, so it cannot be a
control, and a hit has to fall through it to what is behind. `ra2ui::input::hit`
does that, and the three overlays above are what requires it.

### Open: the glyph records do not bracket the glyphs

The page is fine and the records are not. The font's page — dumped with
`ra2 font RA2.cfz fg_mcn14 --page` — holds all 96 glyphs correctly formed and
packed in rows. But the **rect a record states does not frame its glyph**, and the
three below are pixel maps of the page read through the records' own rects, so
they are measurements rather than impressions:

```text
A  record 31  rect (217, 245, 9, 11)      T  record 50  rect (123, 232, 7, 11)
   .........                                 .......
   ...##..##                                 ....###
   ...##..##                                 ....##.
   ####...##                                 ....##.
   ####...##                                 ....##.
   #.##...##                                 ....##.
   #.##...##                                 ....##.
   ###....##                                 ....##.
   ###....##                                 ....##.
   ###....##                                 ##..##.
   .#.....##                                 #...#..

K  record 41  rect (46, 232, 5, 11)
   .....
   ...##
   ...##
   ...##
   ...##
   ...##
   ...##
   ...##
   ...##
```

`A`'s window holds the tail of one glyph and the stem of the next, `K`'s holds a
bare stem, `T`'s holds a stem with a bottom hook.

And it is nearly every record, not a few. A rect that frames its glyph leaves the
columns at `u0 - 1` and `u1` blank, and of the 96 records in `fg_mcn14` only
**five** do — **87 have ink just outside** on one side or both. `fg_mcn16` is 86
and `fg_d20` 78. The test `almost_no_record_brackets_its_glyph` keeps those three
numbers **and the two that localise the fault**: every one of the 89 scored rects
in each face **does hold ink** (the rects are *near* their glyphs), and no constant
shift of a packed row — every `d` in `-10..=10` — comes close (the best still cuts
**17 of 20** for `fg_d20` and **18 of 33** for `fg_mcn14`). So the rects and the
page describe **different packings**, and the day the mapping is read right the
test will say so.

The record's four floats are **not** the open question any more. The loader that
*builds* a runtime page (`FUN_005cd560`) writes `u0 = x/W`, `v0 = y/H`,
`u1 = (x + width)/W`, `v1 = (y + height)/H`, so the order is `u0 v0 u1 v1` and `v`
is measured from the page's bottom — which is what `Glyph::rect` turns over. And
the cheap candidates are exhausted: reading the page as the **low** byte, as **one
of two planes**, **big-endian**, at **`512x128`**, **`128x512`**, **`512x256`**,
**`256x512`**, or as half-rows repeated, all leave ~85 of the ~92 scored rects with
ink in the one-pixel gap column they share with the next glyph, and none of the 384
field-order × flip × rounding × sample-pairing combinations brackets and keeps ink
inside at the same time.

So the page's glyph blocks sit a few columns left of the rects and at a slightly
different per-glyph pitch, which is why **a single character renders as two
shapes**: the glyph itself and a stroke of its neighbour. (`A` in a `Text` came out
as an `A`-shaped blob beside a vertical bar, in `fg_mcn14`, `fg_mcn16` and
`fg_mcn12`, and a `Text` written exactly as `botlab/ch_stepbuttons.gib` writes its
own does the same, so it is the font reader and not the widget. The page itself is
not the question either: drawn through the rects it is the smeared thing, dumped
raw with `ra2 font … --page` it is every glyph clean.)

The next read is the **`.giff` → runtime-font conversion**. `FUN_005ce100` opens
the file by name and `FUN_005ce320` deserialises it, but `FUN_005ce320` reads
**four floats then three ints** per glyph, where the stored record is **three ints
then four floats**: a conversion layer — or a second, unshipped file format — sits
between them, and that is where a record's `u0`, or the page the records point
into, may be re-derived. `almost_no_record_brackets_its_glyph` carries the exact
inside/cut/best-shift numbers to check it against.

Also settled on the way: the row at y 233 is *not* exact (an earlier reading said
so) — its records are the same `0..7, 9..15, 17..23 …` and the ink runs sit a few
columns left of them, like every other row.

### Open: a glyph's quad draws its cell and a slice of its neighbour

Found while checking the arena list's rows by eye, and it is the renderer rather
than the list. **A single character in a `Text` draws two shapes**: one `Text`
holding `A`, `M`, `O` or `N` each renders as its own glyph *and* a second shape
beside it, and a longer string renders as its letters with extra strokes
overlapping them. It is not the widget's construction — a `Text` written exactly
as `botlab/ch_stepbuttons.gib` writes its own (`_Position 0 0 22 0`, a zero `Size`)
smears the same way — and it is not one face: `fg_mcn14`, `fg_mcn16` and `fg_mcn12`
all do it.

So a glyph quad's uv, or the page texture it is taken from, is off by something
that repeats per cell. What to read next: `glyph_page` against the quad `text_run`
builds — the uv is `rect / (page_width, page_height)`, so a page uploaded at a
different size than the rects were computed against would do exactly this.

This is why the readings in this document that came from *ink in a page* stand
(the `O` measurements are pixel counts, not pictures) while readings that came from
*looking at rendered words* had to be re-checked each time.

### What the shell's screens name, and the two things the game does not ship

Every art name a shipped screen states — an `Image`'s `_Filename`, a `Glyph`'s
`_Font` — resolved the way the renderer resolves it (the loose tree under `UI`,
then the game's own root, then `RA2.cfz` by stem) gives the shell's inventory:
**1,303 names across the 116 screens, and nine that do not resolve.** Seven of the
nine are `_Font System`, and those are not gaps: every widget that asks for it
carries an empty `_Text` as well, and `System` is nowhere in the game — no
`.giff` of that name in either archive and no such file on disk — so it is the
editor's default for a widget with neither a face nor words. There is nothing to
draw in those seven, and nothing in `RA2.cfz` for it to draw with.

The other two are art the game **names and does not ship**:

- `teamhq/xout.tga`, the event cell's overlay — the one that marks an event not
  entered. Hiding it in a render changes nothing, because there is nothing to
  hide.
- `teamhq/event_bba.tga`, the BBA event's winner art.

So the shell is complete but for two files the shipped build left out, and the
test keeps the inventory — the count of names, the seven `System` screens and the
two missing files — as the baseline.

### A letter that does not draw: `fg_d20`'s `O`

The shell's biggest face is `fg_d20` — ascent 26, descent −14, so a 40-pixel line —
and **15 widgets in 5 files** use it, `botlab/botlabwin.gib`'s `title txt` among
them. Its metrics are right (the glyph run's baseline is one ascent below the
box's top, and `fg_d20`'s line height has a test beside it), and the face does
draw — but **not every letter**. A probe of `ROBOT WORKSHOP D20` in `fg_d20` comes
out `R BT W RKSH P D2`, with **every `O` missing**: the four in `ROBOT` and
`WORKSHOP` all draw nothing, while the surrounding letters are correct. **Two faces have no `O`, not one.** Measured across all nine: every face ships a
96-record table and record 45 is the character `O` in all of them, but `fg_d20`'s
rect holds 21 pixels across 3 of its 11 columns and `fg_mcn12`'s 12 pixels across 2
of 8, where the other seven draw a whole letter (40 pixels and up, over six columns
or more). The advances are intact (17 and 9 against their neighbours' 10 and 5), so
the letter is *skipped* rather than drawn in the wrong place — which is what
`teamhq/select_team.gib`'s title shows, reading `SELECT Y UR R B TI CS TEAM` at 12
pixels, and what `botlab/botlabwin.gib`'s `title txt` shows at 20.

The cause is now measured, and it is the *font file*, not the reader. `fg_d20`
page is 256×256 and its record 45 — the character `O` — is a full 11×14 rect whose
ink sits in its last three columns: **21 pixels of a letter**, where the record's
neighbours each hold a whole one. The **same record in `fg_mcn14` draws a whole
`O`** (45 pixels over eight columns), and the rects around it are a packed row —
each beginning one to three columns after the last one ended — so the record
mapping and the rects are right and the *page* is simply blank where that `O`
should be. Its advance is intact (17 against a neighbour's 10), which is why the
letter is **skipped** rather than drawn in the wrong place: `ROBOT WORKSHOP D20`
becomes `R BT W RKSH P D2` with nothing out of position.

Two things follow. The game reads the same file, so the game's `fg_d20` text has
no `O` either — this render is faithful to the data. And the old suspicion that
the glyph table might carry "a one-record slip somewhere in the middle" is
**refuted**: the same index gives the right letter in another face, which a slip
would not.

That is what `botlabwin.gib`'s `title txt` actually is: a label whose text cannot
be spelled in its own face. It is in the draw list and it *is* drawn — the word
just loses its `O`s, and with the `O`s gone the remainder is easy to miss
altogether against the toolbar art.

### The UI project document: the shell's own index of itself, and a check on it

`local/game/UI/Project Header.html` ships with the game and is the UI editor's
project summary — a tree of the project's seven directories and the 115 `.gib`
files in them, each with **the name and class of its root widget** and a list of
the widgets the designer tagged, with the note attached to each. It is the same
kind of document as a screen's own `Notes`, one level up: not the game's
definition of a screen — the `.gib` is that — but the designer's index of all of
them.

That makes it a *second* statement of what each file holds, and a check on the
reader. For the 113 entries that both ship and are indexed: the root is the name
and class the document states, and **every one of its 489 tagged widgets** is
present in the class it states. The check inlines a screen's `Proxy`s first — the
document tags what a screen *shows*, so `botlabwin.gib` is tagged with
`Options button` and `MainMenu button`, which live in `common/bottomnav.gib` and
reach `botlabwin.gib` through its nav `Proxy`. Its class vocabulary is exactly
the reader's: the 4 root classes and 13 tagged child classes are all
`WidgetKind`s or the 5 `AttachmentKind`s, and `Background` is the one that is an
attachment rather than a widget.

Two things the document says that the files do not:

- **A file's "root" is its first record.** Nine entries are rooted at an unnamed
  `[Background]` — and in all nine that is the root's own *attachment*, which the
  writer put before the root's header: `hq_league.gib` opens `[Background]` and
  then `[SplitFrame]`. Our reader dresses the following widget with it, so those
  roots are the widgets the attachment belongs to (8 of the 9 ship; the ninth is
  the editor's scratch file).
- **Attachment names.** The document names nine `DisplayBox`es (`chat displaybox`,
  `component description`, `description`, …) and **not one attachment in the
  corpus carries a `Name` field — 0 of 758**. The editor's project keeps names the
  `.gib` writer does not serialize, so those nine are checked by class.

And it is a **snapshot**, which is why it cannot be the specification: two files
it names are not shipped (`common/skipintro.gib`, `testfile.gib` — the editor's
own scratch document) and three screens ship that it does not index
(`botlab/comp_properties.gib`, `botlab/info_display.gib`, `help/help_blank.gib`).
The third of those is the reason the binary's own table has to exist:
`info_display.gib` is the info bar the C++ loads into `lowersplit`, and it is in
no document.

The notes also state the shell's *links*, and three of them are of the load form
— `options.gib`'s *"click to load video_options.gib into 'options content
group'"* and *"…audio_options.gib…"*, and `chassis.gib`'s *"click to load
ch_armor.gib into 'right content slot'"*. The last names a file that is **not
shipped**, and the shipped screen's own `Notes` names it too — so the staleness is
the data's, not a misreading, and the document is the second witness to it. The
document also shows what became of the 3D view: the tabs' notes say *"click to
show 3dview.gib and overview.gib"*, `3dview.gib` ships in no directory and in no
archive, and what the shipped screens have instead is a [`Placeholder`] —
*"common 3d view for various areas of botlab"* — so `Placeholder` is the widget
kind the native 3D view left behind when it stopped being a screen.

### Lists and grids: cells are made, and the data says what each row binds

A `Grid`/`ListBox` in a screen is not a list of children in the file — it is a
**template repeated per row**, and the screen's own `Notes` name both the template
and what goes in it:

| screen | its note | template |
|---|---|---|
| `botlab/armor.gib` | *"load this grid with the four armor cells"* | `armor_cell.gib` |
| `teamhq/hq_league.gib` | *"fill with league_cell.gib with the teams in order of rank"* | `league_cell.gib` |
| `teamhq/hq_events.gib` | *"fill with this season's events (event_cell.gib)"* | `event_cell.gib` |
| `teamhq/hq_robots.gib` | *"fill grid with team robots (robot_cell.gib)"* | `robot_cell.gib` |
| `teamhq/select_team.gib` | *"load team_cell.gib's into these grid cells for each team"* | `team_cell.gib` |
| `teamhq/season_results.gib` | *"fill with sresults_cell.gib just like league standings"* | `sresults_cell.gib` |
| `common/matchresults.gib` | *"load result_cell.gib into this slot with appropriate data"* (×4) | `result_cell.gib` — four *slots*, not a list |

The cell's geometry comes from the list's own `Rows`/`Columns`/`RowHeight`/
`ColWidths`: `armor.gib` is `Rows 4`, `Columns 1`, `RowHeight 84`,
`ColWidths 300 17`, so its cells are 300×84 at y = 0, 84, 168, 252. A template's
root is `Size 1 1 1 1` — it is written to be *placed*, not to know where it goes —
so the *filler* writes each instance's `Position`/`Size`. The list's own children
in the file are only its two selection bars (`Selection 100`/`Highlight 101`).

**A template names its binding points in its own `Notes`**, and they are its
children's names:

- `armor_cell.gib`: *"'armor img' child should be the preview for each armor
  type"*, *"armor description displayed here"*, *"show weight value here"* →
  `armor img`, the description, `weight value`.
- `bot_cell.gib`: *"load bot image into 'bot snapshot img' child of this group"*,
  *"robot name here"*, *"'Lightweight' or 'Heavyweight' here"*.
- `result_cell.gib`: *"set 'team logo' and 'bot preview' images"*, *"display the
  bot's total points here"*.

Prose is the one thing not named: the template's own `Glyph` *is* the prose, and
it carries the text **twice** — as `_Text` and inside its `DisplayBox`'s markup,
`Display <text font fg_mcn14 color .8 .8 .8 1>Plastic armor is …</text>` — so a
binding writes both and leaves the markup's font and colour as the file has them.

The content itself is shipped: `Armor_Definitions.txt` inside `RA2.cfz` holds the
four armour types as name, description, texture and the numbers
(`0 8 0 0 80` = type 0, weight 8, …, hardness 80), which is exactly the three
things `armor_cell.gib` binds.

### Input: what a click lands on, and what it changes

- **`PassMouse` distinguishes nothing.** It is `true` on all 1,298 widgets in the
  corpus that carry it, and only the *container* classes carry it at all —
  `Window`, `Group`, `Button`, `CheckBox`, `OptionGroup`, `TextBox`, … — while
  `[Image]` records never do. So it is a base-class field the writer always
  emits, not a flag the screens vary, and a hit test cannot use it.
- **The state images are a table, and the corpus fills it in with no
  exceptions.** A `CheckBox` carries up to four `Image` children — the states —
  and exactly one is `Visible`: the group's `Selection` shows image **2** when it
  has four and image **1** when it has three, and every other member shows image
  **0**. The binary names the four states in one schema entry — `Normal`,
  `Rollover`, `Clicked`, `Disabled` — so the first image is the unselected look
  and the **second-to-last** is the selected one, the last being disabled. That
  is the whole rule for showing a selection.
- **An `OptionGroup` names its selection by `Name`.** `botlabwin.gib`'s
  `blab option group` says `Selection overview button`, and that member is the
  one with `Checked true` ✓; every other member is `Checked false`. So a click on
  a tab is: set the group's `Selection`, set the clicked member's `Checked` and
  image, clear the others'.

### The workshop's steps, and a stale note

The chassis tab is not one screen. `chassis.gib` is its **intro** — the 3D view
on the left and a `right content slot` on the right holding the `chassis choices
grp`: the word "Chassis", a note, and two buttons — and each button starts a
*step* by putting another screen into the tab's own `content slot`:

| button | note | screen | its own root note |
|---|---|---|---|
| `structure design button` | *"click to go to chassis step 1 (…)"* | `botlab/chassis_structure1.gib` | *"step 1 of the chassis creation process"* |
| `armor button` | *"click to load ch_armor.gib into 'right content slot'"* | `botlab/armor.gib` | *"load this into content slot"* |

**The `armor button`'s note is stale in both respects**, and the screen is what
says so: the game ships `armor.gib`, not `ch_armor.gib`, and `armor.gib`'s own
root record names `content slot`, not `right content slot`. The geometry agrees
with the screen — `armor.gib` is a 50/50 split whose left pane is **400 pixels**,
wider than the 320-pixel `right content slot` the note names — and so does the
binary: `right content slot` is **not in it at all**, while `content slot`
(`0x6f0f74`) is. The two `chassis_structure*.gib` screens are full-content shaped
the same way (`chassis split`, `Size 1 1 1 1`) and name their step, which is the
half of the button's note that is *live* ("go to chassis step 1").

Under the structure screens is one more layer the C++ loads: `ch_stepbuttons.gib`
— `steps and snap group` with the `snap checkbox` (*"click to set snap on or
off"*), a "Snap to grid" label, and a `step button grp` holding `step 1 button`,
`step 2 button` (*"click to complete chassis shape"* → `finished button`). No
shipped `.gib` names that file, so which slot it fills is the C++'s (the loader
caches the target widget in a member — `+0x188`, `+0x1cc` — rather than looking a
name up), and `chassis_structure1.gib`'s labels are laid out to expect it.

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
7. ~~Where an uncounted `.gib` record goes when its parent's count is full.~~
   **Answered** — the nearest record with a slot, else the file's top level,
   which sorts by `ChildOrder` like any other sibling set. See `.gib` above; the
   residue is the data's own ambiguity and is harmless (chromeless, and ordered
   behind).
