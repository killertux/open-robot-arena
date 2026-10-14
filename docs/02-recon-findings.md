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
- **`Gooey`** — UI helper over `plus.addWindow/addText` (`Plain` class).
- **`giScript`** — host glue: `giScript.register_function(fn)`,
  `giScript.debug_stdout(s)`, `giScript.debug_stderr(s)`.

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
  Left-handed coords: +X left, +Y away, +Z up.

### `.cfz` — **CFL3** containers (Jari Komppa "Compressed File Library 3")
- Magic `CFL3`; header ints then zlib streams (`78 9c`). Holds component defs etc.
- Tools: `uncfl.exe`, `makecfl.exe`; format is simple enough to parse natively.

### `.gib` — GUI layout scripts
- **Plain text** (`T 1.05`, `[Window]`, `ReportedPos …`, `Draggable`, `Align …`).

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

`.bot` files are line-based text with two embedded uncompressed TGA blobs.
Handled by `ra2fmt::bot` (v1.09+; 1.07/1.08 differ and are TODO).

- Header: version, `Name:`, `Class:`, [default-texture int — absent in 1.07/1.08],
  snapshot flag, then a 128×128 TGA (size taken from its own header).
- Components: `count`, then per component:
  `parent_id`, two mount ids, translation (3f), rotation quaternion (4f),
  angle+height (2f), class, definition path (`Components\name.txt`).
  Classes with extra lines: `BurstMotor`, `SmartZone`. `Chassis` carries a large
  inline block (outline geometry, a 3D model in normalized units, a 256×256 TGA
  skin, face groups, armour, buckling points).
- Trailing: buckle model, forward heading, controllers, controller↔component bindings.

**Component definitions** (`.txt`) live inside `Components.cfz` — a **CFL3**
archive (Jari Komppa "Compressed File Library"; source: github.com/jarikomppa/cfl).
They are `key = value`; `dir` + `model` resolve to `Components/<dir>/<model>`.
`base` is the component class. Extracted with the bundled `uncfl.exe` (via Wine);
a native CFL3 reader is planned.

**Assembly model** (implemented): a component's world transform is
`world[parent] · A_parent(mount_on_component) · inverse(A_child(mount_on_parent)) · local`,
where `A_*` are the `.gmf` attachment-point transforms (attachment ids come from
each point's user data, `ID = n`) and `local` = `T(translation)·R(quat)·Rz(angle)·T(0,0,height)`.
The chassis mesh is in normalized units and is mapped onto its footprint from the
outline points (0..1000 = 0..1 world units), bottom at Z=0.
Known gaps: exact handedness/orientation and the embedded chassis skin TGA still
need work; older bot formats (1.07/1.08) unsupported.

## 8. Tooling we now have (engine)

- `ra2fmt::gmi` — `.gmf` meshes (181/181), materials, textures, attachments, Havok bodies.
- `ra2fmt::bot` — `.bot` header + components + chassis mesh/outline.
- `ra2fmt::component` — component `.txt` definitions.
- `ra2-render` — headless wgpu renderer (`.gmf` or assembled `.bot` → PNG).

## 9. Open questions for Stage 1/2

1. Direct3D 8 usage specifics: vertex/pixel shader model, fixed-function pipeline,
   render state set — from PE imports + Ghidra disassembly.
2. Boost.Python registration tables → enumerate the *complete* `plus`/`Arenas`/`AI`
   API (method names, not just C++ signatures).
3. `.bot` "Skin" and `.gib` full grammar; `.cfz` index layout (verify vs `uncfl`).
4. Arena scripting beyond `Practice.py` (mods add more: check Total Conversions).
5. Havok version + parameters to match simulation feel.
6. Copy protection: base 1.20 build appears unprotected (no SafeDisc/SecuROM import).
