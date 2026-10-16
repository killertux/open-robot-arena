# Open Robot Arena

A native, cross-platform effort to bring **Robot Arena 2: Design & Destroy**
(Gabriel Interactive / Infogrames, 2003) to Linux, macOS, and modern Windows.

This repository contains **only our own tooling, engine code, patches, and
documentation**. It does **not** contain or redistribute any original game
assets or executables — those stay in `local/` (git-ignored) and are supplied
by the user.

## Status

**M1 (asset rendering), M2 (physics sandbox) and M3 (robot fights) are working.**
The engine (Rust) parses the original `.gmf` meshes, `.bot` robots, and arena
`.py` scripts, renders them with their textures via `wgpu`, and simulates them
with **Jolt Physics** — no Windows, no Wine in the loop:

```
# Render
ra2-render local/game/Components/mower_blade/mower_blade1.gmf out.png
ra2-render "local/game/AI/Team0/Bot0.bot" scout.png                 # assembled robot
ra2-render "local/game/Arenas/octagon.py" octagon.png              # arena scene

# Drive one robot around (M2)
ra2-sim "local/game/Arenas/octagon.py" "local/game/AI/Team1/Bot0.bot" out/ 6 10
RA2_OBSTACLE=crates RA2_YAW=180 RA2_SPAWN=0,3.5 \
  ra2-sim "local/game/Scripts/Practice.py" "local/game/AI/Team0/Bot0.bot" out/ 6 10

# Two robots fight to the death (M3)
ra2-duel "local/game/Arenas/octagon.py" \
  "local/game/AI/Team14/Bot2.bot" "local/game/AI/Team13/Bot1.bot" out/ 20 12
```

- All **57 shipped robots** (formats 1.07/1.08/1.11/1.12) render with every
  component resolved and placed (matching the game's own embedded snapshots).
- All **12 arena scripts** assemble into scenes (main model + script-referenced
  props, e.g. the practice arena's crates/barrels/cones); `ra2 arena <script>`
  lists the recovered composition.
- **M2:** a robot drops into an arena, drives on a scripted input timeline,
  collides with walls and with movable props, and the run renders to a frame
  strip. The physics engine sits behind a `PhysicsBackend` trait so it can be
  swapped.
- **Assembly:** a robot is built as its real **jointed bodies** — one body per
  rigidly-mounted group, a hinge per motor, wheels driven through those hinges at
  the speed and torque their own definitions declare. `ra2-sim` and `ra2-duel`
  both drive it; there is no arcade velocity controller left in the tree.
  Mount-point alignment (F1) is settled — the male-plug-into-female-socket flip is
  the default — and rendering a bot with and without it is a one-command check.
  What the honest drive still cannot do — a quarter of the stock bots will not get
  traction on their wheels — is the wheel/contact model and the arena colliders;
  tracked as F4/F11/F5 in `docs/06-deferred-fidelity.md`.
- **M3:** a robot is a *component tree* rebuildable from the game's own parts.
  Motors turn their attached subtree (spinners spin, wheels roll), weapons deal
  damage on contact, and parts whose hitpoints run out are torn off as flying
  debris — a fight between two AI-driven robots ends when a chassis is destroyed.
- **Validated against the original** (`docs/05-validation-report.md`). The game
  statically links **Havok Hardcore 1.7.1**, and its own data files state the
  physics outright: each arena carries its gravity (stock `-9.81`, the practice
  arena `1.3 g`), its fixed collision geometry as mass-0 rigid bodies (triggers
  flagged `DISABLE_COLLISIONS`), its per-body friction/elasticity, and its
  movable props. The engine now reads all of that instead of guessing, and the
  chassis takes its **mass from armour weight × chassis surface area** and its
  **hitpoints from the armour hardness** in the `.bot`.

Working today:
- `ra2fmt::gmi` — GMI `.gmf` parser (181/181 meshes), materials (incl.
  `Multi/Sub-Object` sub-materials and transparency), textures, attachment points,
  embedded Havok rigid bodies.
- `ra2fmt::bot` — `.bot` parser: components, mount transforms, inline chassis mesh
  + outline + skin, the chassis' **face groups** (each side's patch of the skin),
  its crush model and the top plate's corners, all version variants.
- `ra2fmt::arena` — arena scripts → scene (model, props, shadow, cameras).
- `ra2fmt::component` / `ra2fmt::cfl` — component definitions and a native
  **CFL3** (`.cfz`) archive reader (no external tools).
- `ra2sim::rig` / `ra2sim::robot` — the component tree and a live robot with
  motor animation, per-part health and detachable parts.
- `ra2sim::combat` / `ra2sim::duel` — contact-driven damage and the two-robot
  world (arena, props, debris, driving AI).
- `ra2sim` — backend-neutral physics types + the Jolt backend, arena/robot/prop
  scene assembly, and the jointed wheel drive (a hinge per motor).
- `ra2-cli` — `ra2 info`, `ra2 obj`, `ra2 cfz`, `ra2 arena`.
- `ra2-render` — headless wgpu renderer (offscreen → PNG) for `.gmf`, assembled
  `.bot` robots, and arena `.py` scenes.
- `ra2-sim` — headless physics sandbox driving one robot around an arena.
- `ra2-duel` — headless two-robot fight.

Remaining caveats: a chassis is dressed from the **face groups** its `.bot` records
for each side (that is what fixes decals coming out rotated or mirrored); exact
BurstMotor rest poses are approximated; a motor's
`speed × its radius` sets a wheel's top speed, so a robot's pace is a property of
its parts (a definition's `power = <speed> <torque>` is the pair the scripts set
through `SetPowerSettings`, whose implementation scales the torque by 40) — but
on the jointed assembly a quarter of the stock bots still cannot get traction and
simply will not move. That is mount *placement* and the unmodelled trigger/control
layer, not the parts' parameters: arenas now collide the geometry they draw
(`SIMULATION_GEOMETRY` 5/6 is the display proxy, plane primitives are synthesised)
and a duel starts on the arena's own `startpoint` attachments. Pistons are
prismatic joints too — a rod that slides with the travel its definition declares,
held at the definition's force — but nothing *drives* one yet: a servo piston
should follow a control-board output and a burst piston fire on a trigger, which
is the script layer again. Damage is an event
now — a blow priced by the speed the robots arrive with, a spinner's rotor tip
speed as a rate, contact tested across every body of both robots rather than their
two roots — which a deterministic ram (`RA2_DUEL_DRIVE=ram`) puts in the range the
game's own scripts describe (hazards do 25; its AI calls `amount > 10` a good
hit). The *driver* is now the game's own. `ra2script` runs the shipped Python AI —
`AI/Bindings.py` picks each robot's brain by its `Name:`, and `AI/SuperAI` plus
`AI/Tactics` do the rest — so the headline duel closes to contact and trades
blows, where the stand-in driver it replaced only orbited and pinned itself
against a wall. A robot the table does not cover keeps that stand-in. A
wheel's declared `contact` surface is parsed but not yet used as the collider —
using it makes some robots unable to move. Damage attribution picks the nearest
part rather than solving per-part contacts, the impact-to-damage conversion is
calibrated rather than recovered, and the `.txt`'s `fracture` and `material`
fields are parsed but their gameplay effect is unconfirmed. Component hitpoints
are no longer invented: they come from the engine's own per-category table
(power 400, extenders 100, mechanics/mobility 400, weapons/extras 1000) with a
declared `hitpoints =` overriding it. The
chassis *buckles* as it is hurt — the crush points the bot file marks shift down
and in, taking the drawn shape and its collider with them — but at an inferred
rate, and a chassis that loses a *part* is still *reshaped* rather than deforming
the way the original's does; and Jolt combines friction as `sqrt(µ₁·µ₂)`
where Havok 1.x used `√(µ₁²+µ₂²)`, which we cannot change through the safe
binding. Each item is tracked with its evidence and fix path in
`docs/05-validation-report.md` §4 and `docs/06-deferred-fidelity.md`.

**M4 — the script layer — is under way.** The game's own scripts run unmodified:
`ra2script` embeds a CPython 3.12, exposes the engine to Python as the `plus`
module the scripts expect, and translates the shipped Python 2.2 sources as they
load (including their implicit relative imports and `list.sort(cmp)`). All
fourteen stock arenas register and configure themselves — cameras, shadow
receivers, the navigation map read from the `.map` files the arenas ship, the
hazard zones and the collision lines that keep the AI off them — and the shipped
AI drives the duel on the game's own eighth-of-a-second cadence. The hazards are
landed too: the box arena's spikes and saw become real prismatic joints with the
script's own travel, the hinges the models declare are parsed and handed to
`GetHinge` (the Compressor's smash arms swing under the script's own power
settings), and standing in a registered SmartZone reaches the script's own
`ZoneEvent` — the spikes fire, and they hurt. What is left of M4 is presentation.
See `docs/06-deferred-fidelity.md` F13.

**M5 — the game shell — has started.** Every screen in the shell is written in
the game's own GUI layout language, `.gib`: the main menu, the team HQ, the whole
bot lab, the in-match HUD. `ra2fmt::gib` reads all 116 of them into widget trees —
nineteen widget kinds, five kinds of attachment, and the geometry flags that say
whether an offset is a fraction of its parent or a pixel count — so a screen's
structure, art references and button notes can be printed without launching
anything:

```bash
cargo run -q -p ra2-cli -- gib local/game/UI/mainmenu/mainmenu.gib
```

`ra2ui` then *places* them: `Position` and `Size` carry a per-axis flag for
fraction-of-parent versus pixels, `Alignment` is a 3×3 anchor, and a split frame
hands its two children panes. Every one of the 3590 widgets resolves to a frame,
71 of the 116 files place entirely on screen, and the main menu's logo, backdrop
and button strip land exactly where its own artwork says they should:

```bash
cargo run -q -p ra2ui --example uidump -- local/game/UI/mainmenu/mainmenu.gib
python3 tools/ui_compose.py local/game/UI/mainmenu/mainmenu.gib /tmp/mainmenu.png
```

That second command is the check that matters: it composites the screen from the
engine's own rectangles and the game's own art, and the main menu comes out as
the main menu.

The `fg_*.giff` fonts are parsed too (`ra2fmt::giff`; `ra2 font <RA2.cfz>
<name>` prints one's metrics and dumps its page), and `tools/ui_compose.py` draws
a screen's labels with the game's own faces and fore-colours — the main menu's
`MULTIPLAYER` reads as `MULTIPLAYER` — and a resolver given those faces sizes a
zero-`Size` label from its own text. A screen file is one tree: a record ends at its own `NumChildren`, the base
`Widget` class — a fifth of every screen — is written without a `[Kind]` header
at all, and the file's own records are ordered by `ChildOrder` exactly as a
widget's children are. A screen is composed of fragments the data names: a `Proxy` loads another
`.gib` into its own rectangle (nine shipped screens load the bottom navigation
bar), a `Glyph` is a wrapped paragraph in its `_Margin` box, and a `Lines` is a
polyline in point pairs. The shell's own Python runs too: `plus` and
`giScript` are the engine's face to the scripts, the shipped `Scripts/Main.py`
boots the interpreter, and a `Gooey` window a script builds is a widget tree the
same renderer draws. Screens *draw*: `ra2-render`'s 2D path
turns a placed screen into quads and renders it headlessly, and the result is
checked against the Python composite, which shares no code with it. The
containers place their own children too
— `ClipMode` into a clip rectangle, a grid's cells from its rows and columns, a
scroll bar's parts at its two ends. The robot workshop composes from the same
data: `botlabwin.gib` plus the tab's own screen, loaded into the group that
screen's own `Notes` names, with the tab table taken from the toolbar's notes on
its six buttons (`cargo run -p ra2-render --example workshop` renders all six).
What remains of M5 is the cells' content (which is game data at run time) and the
workshop's *interaction* — its screens are in, and the steps a tab offers are the
game's C++.

## Layout

```
docs/      Design notes, reverse-engineering findings, format specs
tools/     Our own analysis/extraction/build scripts (Python)
engine/    The native engine (Rust workspace: ra2fmt, ra2-cli, ra2-render,
           ra2sim, ra2script — the script layer on an embedded CPython — and
           ra2ui, the shell's layout model)
local/     (git-ignored) original game files and extracted data
```

## Legal

Robot Arena 2 is abandonware, no longer sold or supported by its publishers.
We are not the rights holders and do not distribute the game. The goal is
interoperability and preservation on modern platforms.
