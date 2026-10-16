# 05 — Validation against the original

An audit of what the engine has built so far (M1–M3) against the original game,
done before starting M4. Two questions are answered:

1. **Is the asset / engine code true to the original?** — checked against the
   game's own data files, its binary, and the official mod tools.
2. **Does our physics match Havok 1.7.1?** — checked against the physics
   settings the game authored into its files and Havok documentation of the era.

Evidence is marked **[CONFIRMED]** (primary evidence), **[LIKELY]**,
**[INFERRED]** or **[OPEN]**. Havok background is in
[`04-havok-1.7-research.md`](04-havok-1.7-research.md).

---

## 0. Ground truth discovered (new in this pass)

This pass turned up sources that are *stronger* than disassembly for format
work. They are now the reference material for the engine.

| Source | What it gives | Where |
|---|---|---|
| **`*_txt.gmf` ASCII GMF exports** | The complete physics model of **every stock arena**, in plain text: gravity, world settings, every rigid body, its mass/friction/elasticity, proxies and ranges. | `local/game/Arenas/*/**_txt.gmf` (25 files) |
| **`Armor_Definitions.txt`** | The armour table: type, weight per square unit, hardness. | inside `local/game/RA2.cfz` |
| **Official component templates** | The complete component-definition field set per base class. | `#Tools/Component Exporter v1.2/Component Exporter/*.txt` |
| **`Dummy's GMF Compiler` readme + quick help** | The `h_*` collision-object convention, "SIM objects at the end", and that motors are exported as Havok **constraints**. | `#Tools/Dummy's GMF Compiler/` |
| **PyRA2 `docs/`** | The bot file layout including the armour row, the chassis mass rule and the weight classes. | `tools/vendor/PyRA2/.../docs/` |
| **The binary** | Havok version, the component-definition field set and type enum, and the collision/constraint tag inventory. | `local/game/Robot Arena 2.exe` |

### Physics engine identity — **[CONFIRMED]**

The executable statically links **Havok Hardcore 1.7.1**:

```
E:\nightly_builds\code\hk171\havok\source\hkdynamics\world\world.cpp
E:\nightly_builds\code\hk171\havok\source\hkbase\debugutil\hkDefaultMonitorFactory.cpp
 Havok Hardcore - Version(%d) Build(%d)
.?AVHavokObject@Havok@@  .?AVReferenceObject@Havok@@  .?AVhkBaseObject@@
.?AVStepAction@Havok@@   .?AVAction@Havok@@           .?AVEventCallback@Havok@@
.?AVConstraint@Havok@@   .?AVRigidBodyMonitor@Havok@@ .?AVIntersectionSolver@Havok@@
```

Boost 1.27.0 is linked alongside it (`C:\PROJECTS\PUBLICLIBS\BOOST\BOOST_1_27_0`)
— the era checks out. **The project plan's "Havok 2.x" assumption was wrong; the
correct reference is 1.x.** Nothing in our code depended on that, but the physics
semantics differ (see §3).

### Component definitions — **[CONFIRMED]**

The binary's `ComponentDefinition` field parser (`FUN_0045cf40` /
`FUN_00462b08`) reads exactly the fields the exporter templates write, and maps
`type` to a small enum:

```
power → 1   extenders → 2   mechanics → 3   mobility → 4   weapons → 5   extras → 6
```

Fields confirmed present, in the binary's own string table: `name`, `dir`, `type`,
`base`, `model`, `preview`, `description`, `master`, `passthru`, `attaching`,
`contact`, `grip`, `resistance`, `power`, `burst`, `prismatic`, `material`,
`concussion`, `piercing`, `hitpoints`, `fracture`, `damageable`, `hidden`,
`alwaysvisible`, `standard`, `sort`, `styles`, `decal`, `normal`, `frequency`,
`electotal`, `elecmaxinoutrate`, `airtotal`, `airmaxinoutrate`, `damagesounds`,
`motorsound`, `firesound`, `attachsound`, `runsequence`, `mass`.

---

## 1. Asset formats — what holds up

| Area | Verdict | Evidence |
|---|---|---|
| `.gmf` mesh/material/attachment parsing | **Correct** | 181/181 stock meshes parse; geometry verified against the ASCII exports |
| `.gmf` Havok `RigidBody` binary layout | **Correct** | field-for-field identical to PyRA2's `readRigidBody` loader (name, mass, elasticity, friction, optimization level, unyielding, simulation geometry, geometry proxy, use display proxy, disable collisions, inactive, display proxy, node TM, geo type, child count) |
| `.gmf` `SimulationObject` layout | **Correct** | matches PyRA2's `SimulationObjectImporter` field-for-field |
| `.bot` all versions 1.07–1.12 | **Correct** | 57/57 stock bots assemble; every meshes renders |
| Component mount transform | **Correct** | the format doc's "rotated, then translated, then rotated by `angle` about the up axis, then translated along it by `height`" is what `component_local` implements (the doc's "Z axis" is Max's up axis, which is our Y) |
| Armour table | **Correct** | `Armor_Definitions.txt` matches PyRA2's table exactly: 0 plastic 8/80, 1 aluminium 12/100, 2 steel 20/175, 3 titanium 16/150 |
| Weight classes | **Plausible** | computed chassis + components land in the documented class for the one bot the doc names (Mud Runner, medium) |
| Havok object tag inventory (`GMID_HAVOK_*`, `GMID_COLLISION_*`) | **Correct** | the parser handles the tags the binary can actually read (`ERROR Reading GM_Havok…` strings enumerate them) |

### The chassis is where we were wrong

The `.bot` armour row is `<type> <weight> <unused> <hardness> <unused>`
(**[CONFIRMED]**: PyRA2's format doc states it, and the binary loads
`Armor_Definitions.txt`). We were **skipping that row entirely** and inventing
`45 kg` and `1000 hp` for every chassis.

Now parsed and used:

- **chassis mass** = armour weight per square unit × the chassis mesh's surface
  area (the documented rule: "a 3d face with an area of 1 square unit using steel
  armouring, weighs 20 Kg"),
- **chassis hitpoints** = the armour hardness the bot file stores.

Measured effect (all **[CONFIRMED]** against the bot files):

| Bot | armour | surface area | chassis mass | chassis hp |
|---|---|---|---|---|
| Team1/Bot0 "Roly Poly" | Plastic (`0 8 … 160`) | 7.0 | 56.1 kg | 160 |
| Team14/Bot2 "Backyard Ripper" | Titanium (`3 16 … 300`) | 10.0 | 159.6 kg | 300 |
| Team13/Bot1 "Tornado" | Steel (`2 20 … 300`) | 6.4 | 128.4 kg | 300 |
| Team2/Bot1 "Mud Runner" | Aluminium (`1 12 … 200`) | 7.6 | 90.6 kg | 200 |

AI bots store 2× the editor's hardness (steel 350 on some, 175 on others) — the
format doc warns about exactly this ("computer-controlled bots often have a higher
armour hardness specified"), so the value must be read from the file, not looked
up from the table.

**Resolved — the rule is the engine's own.** The script binding `getWeight` is a
stub that tail-calls `FUN_004df0b0`, the same function the editor's "Total mass of
robot must not exceed 800" check calls; it accumulates, per component,
`FUN_0045a4f0` — a read of the stored float at `component + 0x128`. So the game's
weight is the **sum of stored per-component masses**, with the chassis' being
area × armour weight: exactly the rule implemented here. The `Class:` field
cannot corroborate it (each AI team ships one bot per class, and no single scale
factor reconciles the classes with any weight), and Mud Runner's own row is
self-consistent (type 1 aluminium, weight 12, hardness 200 = 2× the editor's 100,
the AI-bot convention). The documented 418.6 kg needs **steel** on the same
chassis (421 kg, within 0.6 %), so the doc's copy most likely carried different
armour. **[INFERRED]** — see F10.

---

## 2. Physics: what the game actually asks for

Every stock arena carries its own Havok world block. Stock values:

```
*GMID_HAVOK_SIMOBJECT
  *GRAVITY 0 -9.80998 0      *WORLDSCALE 39.37     *SIMTOLERANCE 0.1
  *RESOLVER 3                *INCLUDE_DRAG 0       *LINEAR_DRAG 0  *ANGULAR_DRAG 0
  *INCLUDE_DEACTIVATOR 1     *SHORTFREQ 5          *LONGFREQ 0.1
  *USE_FAST_SUBSPACE 1       *UPDATES_PER_TIMESTEP 1
```

**Gravity is per-arena, not a constant.** 19 of the 20 arena files say
`-9.80998`; the practice arena says **`-12.752974` (1.3 g)**. We now read it
instead of hardcoding.

**The collider is declared, not guessed.** Every arena's fixed geometry is a
`GMID_HAVOK_RIGIDBODY` with **mass 0**; trigger zones carry
**`DISABLE_COLLISIONS 1`** and must be skipped. Worked example, the octagon:

| body | mass | disable col. | role |
|---|---|---|---|
| `main_collision` | 0 | 0 | the arena floor + walls (friction **0.80**) |
| `outerfloor_collision` | 0 | 0 | outer apron |
| `topplane_collision`, `hammerplate…` | 0 | 0 | plates |
| `hellzone*`, `hammerzone*` | 0 | **1** | trigger zones — *not* solid |
| `startblock1…4` | 200 | 0 | movable start blocks |
| `hellraiser*`, `hammer*` | 100 | 0 | movable hazards |
| `crates` (crates arena) | 40 | 0 | movable props (friction 0.50, elasticity 0.30) |

The `h_*` naming noted in the exporter readme ("Most maps have a `h_floor` or
similar name object. This defines the actual collision bounding box") is the
same mechanism: name-suffixed collision geometry referenced by a rigid body.

---

## 3. Havok 1.7.1 vs Jolt 5.6

Jolt is a deliberate substitution (see `03-engine-design.md`), hidden behind the
`PhysicsBackend` trait. What follows is what that costs.

| Property | Havok 1.7.1 (RA2) | Jolt 5.6 | Action |
|---|---|---|---|
| **Friction combination** | `√(μ₁² + μ₂²)` — Havok's own 1.x-era Xtra reference states it verbatim | `√(μ₁ · μ₂)` (documented on `ContactSettings::combined_friction`) | **Differs.** For floor 0.80 vs robot 0.30: Havok 0.854, Jolt 0.490 — ~43 % less grip. Not compensable without raw FFI (the safe API exposes no combine-mode hook). **Documented.** |
| Restitution combination | **[OPEN]** — no 1.x source found | Jolt's default | Documented. |
| Per-body damping | **None.** Drag is a *world* setting (`LINEAR_DRAG`/`ANGULAR_DRAG`), and RA2 leaves both at 0 | per-body `linear_damping`/`angular_damping` | **Fixed** — robots no longer damp. |
| Mass / unyielding | kg; fixed bodies have no mass; `UNYIELDING 1` = immovable | mass kg; static motion type | Matches. |
| Integration | selectable ODE integrators incl. Euler; `UPDATES_PER_TIMESTEP 1`, `USE_FAST_SUBSPACE 1` | semi-implicit Euler, fixed dt | Equivalent class; RA2's *resolver/fast-subspace* choice has no Jolt analogue. |
| Step rate | game fixed-step | our `1/60 s` | Plausible; the original's rate is **[OPEN]**. |
| World scale | `39.37` (inches per unit) — a numerical-conditioning scale | n/a | Our unit is 1 m; unaffected. |
| Shapes | `SIMULATION_GEOMETRY` picks the geometry source (1/2 own mesh, 3/4 proxy, 5/6 display proxy) and `HAVOK_GEO_TYPE` the kind (`Standard` mesh or `Plane` primitive) | arena: static tri-mesh (all fixed bodies merged) + a quad per plane; components: convex hull or tri-mesh | **Landed (F5)** — every shipped value is decoded. Merging the fixed bodies into one mesh still loses per-body friction (see the friction rows). |

---

## 4. Divergences found, and status

Fixed in this pass:

1. **Arena gravity** — now read from each arena's `SIMOBJECT` (was hardcoded
   −9.81, wrong for the practice arena and unsourced for the rest).
2. **Arena collider** — now built from the `RBCollection`'s fixed bodies
   (mass 0, `DISABLE_COLLISIONS 0`) with their real geometry. Octagon 164
   triangles of *declared* collision geometry (identical to the game's), instead
   of a `*collision*`-name heuristic that wrongly included the movable hammer's
   proxy. Falls back to proxy names, then visible geometry, each gated by a floor
   ray cast; `RA2_COLLISION=rb|proxy|visible` forces one.
3. **Arena friction/restitution** — taken from the fixed rigid bodies (octagon
   0.80, hilltop 0.90, practice 0.50) instead of a flat 0.9/0.05.
4. **Arena movable bodies** — the `RBCollection`'s mass>0 bodies are spawned with
   the game's own mass/friction/elasticity (start blocks 200 kg, hammers 100 kg),
   in addition to the script props we already loaded.
5. **Chassis mass and hitpoints** — from the armour row (§1).
6. **Robot friction/elasticity** — the stock component values 0.30 / 0.30
   (was 0.15 / 0.15).
7. **Engine damping** — removed; Havok 1.x has no per-body drag and RA2 sets the
   world drag to zero.

Known, documented, not yet fixed:

8. **Friction combination law** (§3) — differs; Jolt exposes no hook.
9. **Motors are torque-driven in the original.** The binary exposes
   `ApplyTorque`, `*ANGULAR_SPEED`, `GetMotorAngle`, and the exporter notes that
   motors round-trip through *Havok constraints*. **Landed (F3):** a definition's
   `power = <speed> <torque>` is exactly the pair the scripts set with
   `SetPowerSettings(speed, torque)`, and its implementation multiplies the
   torque by 40 before storing it. The hinge is a velocity motor at the
   definition's speed with the definition's torque as the cap. Both units are now
   the binary's own: the speed is stored **unscaled** (`0x4810d9` writes it to
   `+0xb0`/`+0xbc` untouched) and the torque is multiplied by the constant at
   `0x6a69f8`, which is exactly 40.0. **[LANDED]**
10. **Wheels have declared traction geometry.** Component defs carry
    `contact = wheel_traction`, `passthru = wheel_collision`, `grip` and
    `resistance`. `grip` is the wheel body's friction and `resistance` the joint's
    rolling-resistance friction torque. `contact` is a **material tag, not a
    collider**, and that is now measured rather than assumed: the traction ring is
    *larger* than the visible tyre (0.473 m against 0.415 m) and the `passthru`
    hub sits inside both, so the whole-scene hull already rolls on the game's
    contact radius — using the ring alone is what stalls a robot, by costing it
    its tyre's footprint. **[LANDED]**
11. **Collision shape kinds.** `SIMULATION_GEOMETRY` (1 convex, 2 concave, 3/4
    proxy convex/concave, **5/6 display proxy**) and `HAVOK_GEO_TYPE`
    (`Standard`, `Plane`) are decoded; arena colliders are assembled per body and
    plane primitives synthesised. **Landed (F5).** What remains is that we merge
    each body's geometry into one static mesh and lose its individual
    friction — see the table above.
12. **Damage model.** The original exposes `GetLastDamageDone/Received` returning
    `(component, amount, time, by player, by component)` — **[CONFIRMED]** from
    the shipped `AI/Spinner.py` — and weapons carry `concussion` + `piercing`.
    **[Landed, F7]**: contact is tested across *every* pair of the robots' bodies
    (it used to be the two root bodies only, so a spinner arm or a wheel meeting
    the opponent did nothing), a blow is dealt once when contact *begins* and is
    sized by the relative speed the robots arrive with, and a spinning weapon's
    tip speed grinds as a rate while it works the joint. Resting contact no
    longer damages anything. **The formula is recovered**: the engine's hit
    (`FUN_00489470`, from the collision handler `FUN_00503c30`) is
    `(piercing · contact_speed · 10 + concussion · magnitude) · material_scale`,
    where `magnitude` is the collision's impulse (`0.5 · μ · v`, from
    `FUN_0045a5a0`) and the coefficients 10 and 1 are the engine's own — the same
    linear shape the community's measurements fit (`D = 183.9·P + 100·C`). A blow
    under `0.5` is discarded, as the handler does. **`fracture` is the engine's
    per-hit threshold** (`FUN_0045f2b0`): a single blow past it takes the part
    off whatever its hitpoints, so a hazard's flat 25 never dents a ramplate (40)
    or the control board behind a chassis (41) while a spinner shears parts away.
    Stock weapons declare both parameters (inside `Components.cfz`) and a chassis
    declares neither, so a bare ram is priced by the collision itself — our parser
    had been inventing `0.5` for both. A deterministic head-on ram lands 36. Only
    the contact material's own scale is calibrated (see F7). **[LANDED]**
13. **Component hitpoints — recovered.** The factory `FUN_0045cf40` switches on
    the definition's `type` (the `+0x24` field the `.txt` parser `FUN_004622f0`
    fills, read back by `FUN_0049aa30`) and stores the per-category maximum at
    `component + 0xe8`: power 400, extenders 100, mechanics 400, mobility 400,
    weapons 1000, extras 1000. A `hitpoints =` line overrides it, and only 9 of
    131 stock parts declare one. Current hitpoints live at `+0xec`, and
    `FUN_0044f370` applies damage as a plain subtraction, so the sim now uses the
    original's numbers. `fracture` is now the engine's per-hit threshold (see
    F7); `material` is parsed into the rig and is the tag the damage scale hangs
    off, but the per-material table itself is unreachable in this build.
    **[LANDED]**
14. **Pistons are prismatic joints — landed.** A `Role::Piston` splits its child
    group onto a slider instead of welding it (see F8). The component mesh ships
    the rod and housing as two rigid bodies and the children mount on
    `Host = shaft`; the definition's `prismatic` line gives the axis (in the
    piston's own space) and the travel, which is already Jolt-shaped. Getting the
    axis from the *mount point* instead — the rule a hinge follows — made
    `AI/Team8/Bot2`'s rod slide vertically and stall it. Parked pistons hold at
    the definition's force; nothing drives one yet (M4). The 45-bot survey holds
    at **27/45**. **[LANDED]**
15. **The chassis buckles — landed.** A chassis carries a crush model in the
    `.bot` (buckle points, plus the top plate's corner quads) that was being read
    and discarded. As the chassis takes damage the points the file marks as
    movable shift **down and in** — the floor plate pinned by the lowest of them —
    and both the drawn chassis and its collider follow. The plate-unit scale is
    2.2 mm and all 57 stock bots are uncrushed, so the *rate* is ours (linear to
    half height and half footprint at death) while the direction is the format's.
    The 45-bot survey is unchanged, bit for bit. **[LANDED]**

16. **Mount-point (attachment frame) alignment — resolved.** The mount *data* was
    already parsed and its field labels corrected; the *placement* rule is now
    settled and is the default: a component is placed by
    `A_parent · R_z(start) · R_x(π) · R_z(angle) · A_own⁻¹` — the parent's mount
    frame, a male plug turned 180° into a female socket, the player's free spin and
    the child's own point. It is the only rule that keeps every wheel's axle
    horizontal *and* lifts a rotor clear of the chassis, and it reproduces the
    stock bots' own previews. Note the claim in the original entry — that the game's
    snapshot shows the rotor "flat on top" while we render it vertical — was a
    misreading of a 128² preview; the rule was fixed from the attachment data and
    the mod tools instead. See F1 in
    [`06-deferred-fidelity.md`](06-deferred-fidelity.md).

17. **The chassis is dressed from its face groups — landed.** Every chassis side's
    skin region is described in the bot file (a polygon, its rectangle, and the
    side's triangles); the coordinates are the skin coordinates of the group's
    vertices **in ascending index order** — verified on all 45 stock bots, whose
    every group coordinate lands inside its own rectangle. We had been dressing
    the chassis from the model's **per-point** UV table, which cannot express a
    seam: a point shared by two sides carries one coordinate, so one side samples
    the wrong patch. `AI/Team13/Bot1` had only 48 of 138 face-vertex coordinates
    right, and its round top carried a stray square decal that is now the rim the
    file describes. The 45-bot survey is bit-identical. **[LANDED]**

18. **The 16 %-light weight — explained.** The engine's own `getWeight` binding
    tail-calls the editor's mass function, which sums a stored per-component mass
    — the rule we implement. Mud Runner's row is self-consistent aluminium and the
    documented 418.6 kg would need steel on the same chassis (421 kg). **[INFERRED]**

19. **Component mass where the `.gmf` stores zero — identified (F12).** Every
    stock wheel's rigid body stores `mass = 0` (eight wheel meshes, plus
    `slipperbottom` and `cannon_txt`), and PyRA2 notes a zero-mass body never
    moves, so the engine supplies the mass somewhere. We do not: `ra2sim::rig`
    falls back to an invented 8 kg. Four `mudtire` wheels at 8 kg are 32 kg of
    Mud Runner's 360.6 kg, and the documented 418.6 kg wants 58 kg more — exactly
    22.5 kg per wheel — which supersedes the armour reading in F10. The rule
    itself (volume-derived or a definition value) is still open. **[OPEN]**

20. **The script layer — landed for the arenas (M4/F13).** The game's own Python
    2.2 `Arenas/` package runs unmodified on an embedded CPython 3.12
    (`ra2script`), through a translating import hook that handles the constructs
    Python 3 will not parse *and* Python 2's implicit relative imports. The engine
    side (`plus.Arena`) implements exactly the method surface the scripts call,
    including the navigation map — which comes from the `.map` files the arenas
    ship — so `GetPath` and the hazard-avoidance collision lines are the game's
    own. All fourteen arenas register; `Arenas/box.py`'s cameras, shadow
    receivers, grid and hazard zones all arrive. The shipped AI is landed too:
    `AI/Bindings.py` picks each robot's brain by its `Name:`, and the headline
    duel now closes to contact and trades blows where the stand-in driver orbited
    and pinned itself against a wall. The hazards are landed as well and they fire:
    the box arena's two spikes and its saw become real prismatic joints with the
    script's own travel, the model's hinges are parsed and handed to `GetHinge`
    (the Compressor's four smash arms swing under the script's own power settings),
    standing in a registered SmartZone reaches the script's `ZoneEvent`, and
    `Arena.SetActive`/`SetPinned` take a body out of the world and put it back.
    `plus.rayTest` answers against the parts' oriented boxes rather than the balls
    around them. Two readings that had been guesses are now recovered from the
    binary: `plus.force` is a *force* — the same operation a joint's `ApplyForce`
    performs — so a step of it is `force * dt`, and `SetAutoLock` holds a joint
    whose motor is stopped, which is what keeps the Octagon's trap floors under
    four robots until their script opens them. The engine also calls the arenas'
    own `Activate` now, without which those floors never moved at all. What is left
    is presentation (M5). **[LANDED]**

21. **The shell's layout model — started (M5).** The first piece of the game shell
    is the thing every screen is written in: `.gib`, the game's own GUI layout
    language. `ra2fmt::gib` reads it and `ra2 gib <screen>` prints the tree, so a
    screen's structure, its art references and its button notes are readable
    without the game. All 116 shipped screens parse — 3590 widgets — with a closed
    vocabulary of nineteen widget kinds and five attachments, and the corpus test
    pins that vocabulary, the counts, and the two things that are easy to get
    wrong: that a block is a *widget* exactly when it carries `NumChildren` (which
    is also what decides that an attachment is not one of its children), and that
    `Position`/`Size` carry a per-axis flag saying whether the amount is a fraction
    of the parent or a pixel count. Two data quirks are handled rather than
    assumed away: three bot-lab screens carry `0 0 1.#INF 1.#INF` as an image's
    texture rect — the Microsoft runtime's spelling of infinity, written into the
    layout by the game's own editor — and one `Position` flag slot holds
    `1242948`.

    The layout *model* has landed too (`ra2ui`), and it is measured rather than
    assumed. `Position`/`Size` are two axes each of an amount and a
    fraction-or-pixels flag — the two states the binary's own field table names
    `To Self` and `To Parent` — and `Alignment` is a 3×3 anchor (`1`/`2`/`4`
    left/centre/right, `8`/`16`/`32` top/middle/bottom) saying which point of the
    widget goes at that position. Every one of the 841 `Alignment` values in the
    shipped corpus is one of those nine combinations and nothing else, and the
    worked cases agree three ways in `mainmenu.gib`: the window is left|top at
    `0 0` and covers the screen, `logo grp` is centre|top at `0.5 0.07` and lands
    512×76 at x=384, y=70, `sawback grp` is centre|middle at `0.5 0.5` and lands
    centred, and `common buts` is centre|bottom at `0.5 1` and lands flush with
    the screen's bottom edge — as does a button, in pixels, inside the centred
    backdrop group. Split frames hand their children two panes
    (`Horizontal`/`Master`/`MasterSize`, corroborated by the screens' own child
    names: `"LR split"` holding `"left slot"`, `"Left TB split"` holding
    `"UL slot"`). All 3590 widgets resolve to finite frames and 71 of the 116
    files place entirely on screen; the 101 that do not are *fragments* (files a
    slot loads, like `botlab/control_cell.gib`, which are not meant to be placed
    against the screen) and children whose declared `Size` is taller than the
    pane they sit in, which the game clips where this resolver does not yet.
    The check that settles it is a picture, as everywhere in this project:
    `tools/ui_compose.py` composites a screen from the resolver's own output
    (`uidump --rects`) and the game's own art, and `mainmenu.gib` comes out as
    the main menu — the cement background, the centred saw plate with its blade
    teeth, the `ROBOT 2 ARENA` logo at the top centre and the two gears along the
    bottom. Drawing it wrong first is what found the last rule: `ChildOrder` is
    the draw order *ascending* (the background is 1 and the logo group 3), so
    tree order paints the background over the whole screen. The same pass
    settled what an `Image`'s four unlabelled numbers are: a rect in a 1024²
    *page*, with the shipped files already cropped to their cell — and 25 rects
    that instead run past the page are *tiles* (`metaldots_pattern.bmp`), which
    is why a renderer draws a cell whole and repeats a tile. **[IN PROGRESS]** —
    and the containers followed: `ClipMode` is 0/1/2 (2288/1287/15 widgets) into
    a clip rectangle, with the parent-clip reading pinned by eleven of the
    fifteen sticking out of their parents; a `Grid`/`ListBox`'s cells come from
    `Rows`/`Columns`/`RowHeight`/`ColWidths`, which reproduces the authors' own
    selection bars in 31 of 37 and names the six that are their own numbers; and
    a `ScrollBar`'s less button, more button and track land at its two ends and
    between, which the render shows as an up arrow at the top, a trough and a
    down arrow at the bottom of `hq_league.gib`. What remains is the cells'
    *content* (a `LoadCell` field the game's own reader consumes), tiles, the
    real 2D renderer, the `giScript`/`Gooey` Python surface and the workshop.

    The renderer is in. `ra2-render`'s `ui` module turns a placed screen into
    textured quads — art (sampled 0..1, since the file is already cropped to its
    cell, and at the `_Size` an image states for itself), filled rectangles, and
    glyph runs off the face pages — each carrying the clip `ClipMode` worked out,
    and draws them headlessly: `uishot <screen.gib> <out.png>` renders any
    shipped screen. It is checked against the composite, which shares no code
    with it: mean per-channel difference 0.42 of 255 on `mainmenu.gib`, 0.24 on
    `hq_league.gib`, the rest at glyph and art edges. Writing it corrected three
    things, two of them in the *check*: the composite drew no `Rectangle` fills,
    and it sampled every glyph's rows turned over — the `V`-ish `M` that had been
    read as a property of the font — and the engine's own draw order was wrong
    at the top level, where `ChildOrder` does not apply: the menu's buttons and
    labels sit on top of the cement, which sorting the file's top-level blocks
    put under it. What the shell still does not draw is named: tiles (repeats),
    `Lines` borders, `Glyph` widgets, `Proxy` and `Placeholder`, and the chrome
    the game draws from its own code — every `Button`'s four state images ship
    with a blank `_Filename`, and only `Point Selection.tga` and the `UI_Scroll*`
    family are in `RA2.cfz` to go on, so a button draws its caption and nothing
    else.

    The visual pass then closed the rest: tiles repeat at their texture's own
    size (the rect is the authoring size over the texture, exactly, in all 26),
    the 36 selection bars draw that `Point Selection.tga`, captions are found by
    the `Caption` attachment they carry — 122 attachments, 122 artwork-less
    buttons — and centred in their buttons, and the bottom bar's three chrome
    images are placed in the bar whose geometry their positions fit to the pixel
    even though its `NumChildren` denies them. It also turned up a reader bug
    worth its own line: attachments dress the widget that *follows* them, and
    this reader had hung them on the one before, which is why captions were on
    the wrong side of their buttons. `armor.gib` and `hq_league.gib` are
    fragments, so their own stray scroll trough now fills the screen — faithful
    to the file, and wrong for the same reason a fragment is: it has no slot
    until a screen loads it.

    The reader's own rule turned out to be **the record boundary**, and finding
    it changed a fifth of every screen. A record ends at its `NumChildren`, not
    at the next `[Kind]` — and `[Kind]` itself is *optional*: the base `Widget`
    class is the one kind the writer leaves headerless, which the binary's own
    schema says (`[Widget]` is the single entry in the field table whose class
    fields are just `Notes ` and `PassMouse `, `0x708c6c`), and 758 records in
    the corpus are like that. Read as trailing fields of the record before them,
    they made the corpus look like 3,590 widgets when it holds **4,348**, and
    they made 88 files read as a forest instead of one tree: the main menu's
    buttons and their caption holders were top-level blocks. The counts agree
    once the records are cut correctly — `sum(NumChildren)` plus the headerless
    records is `records - 1` in all 116 files, with no exceptions — so the count
    names only the children that carry a kind, and the writer gates its child
    loop on `0 < NumChildren`. The picture is what settled the last question:
    with a headerless child absorbed by the record before it, `mainmenu.gib`'s
    `exhibition button` swallowed its own caption holder, whose `Position` is the
    button's own and which therefore belongs beside the button in `sawback grp`
    — the render showed the caption clipped to nothing. `botlabwin.gib`,
    `teamhq.gib`, `credits.gib` and the main menu now each nest to their
    `Window`, and 49 of the 116 files do.

    The workshop composes. `ra2ui::workshop` loads `botlabwin.gib`, the info bar
    into `lowersplit` and a tab's fragment into the group the fragment itself
    names — `content slot`, on the root record of five of the six — and the tab
    table is the toolbar's own `Notes`: `overview button` says *"click to show
    3dview.gib and overview.gib"* and `chassis`, `components`, `wiring` and
    `test robot` say the same with their own file; `paintshop button` says *"go to
    chassis appearance effects"* and the binary is what pairs that with
    `appearance.gib`. `cargo run -p ra2-render --example workshop` renders all
    six, and looking at them shows the thing working: the same window with its
    six tabs (the selected one in its own state, which is the file's `Checked`),
    the crossing-wires chrome, the instructions panel, the info bar and the shell's
    nav bar through the window's proxy — and a different screen in the content
    area on the paintshop tab. The binary's per-tab title strings turned out to be
    the *help* pages (`help_ws_*.gib`), so the toolbar's own `ROBOT WORKSHOP`
    stays put; it does not draw in the render, which is a toolbar detail and not
    the composition (it is absent with `botlabwin.gib` rendered alone too — it is
    in the draw list at `557.5,23.34` off `fg_d20`, and the toolbar's wires cover
    it from x 630, so what to look at next is the face's coverage and the
    toolbar's own draw order).

    The workshop is live. `ra2ui::input` hit-tests a click against the placed
    tree in draw order reversed — the last thing drawn is the first thing hit,
    and a child before its parent, so a click on a tab's icon still selects the
    tab — and `workshop::BotLab` keeps the composed screen as state: clicking a
    tab sets the toolbar's group `Selection`, flips the two members' state images
    and loads that tab's fragment into `content slot`, without rebuilding from
    disk (which would put the file's baked selection back). The rule for the
    images is measured off the corpus and has no exceptions: the selected member
    shows image 2 of four or 1 of three, every other member image 0. `PassMouse`
    is *not* consulted — it is `true` on all 1,298 widgets that carry it, so it
    distinguishes nothing. `cargo run -p ra2-render --example workshop` clicks
    each of the six tabs at the centre of where the layout put its button and
    renders the result: six different screens, and the toolbar shows the clicked
    tab in its own selected art with the rest normal. The chassis tab's own steps
    are the next layer, and its buttons name them — `structure design button`
    goes to chassis step 1, and `armor button` loads a screen into `'right
    content slot'`, naming `ch_armor.gib`, which the game does not ship: the file
    is `armor.gib`.

    A tab can have steps, and the chassis one does. Its intro offers two buttons,
    and each puts a screen into the tab's own `content slot` with the tab still
    selected: `structure design button` -> `chassis_structure1.gib` (*"step 1 of
    the chassis creation process"*) and `armor button` -> `armor.gib`. The
    button's own note for the second is **stale in both respects** and the screen
    is what says so: it names `ch_armor.gib`, which the game does not ship, and
    `'right content slot'`, which is not a string in the binary at all — while
    `armor.gib`'s own root record says `load this into content slot`, and its
    left pane is 400 pixels, wider than the 320-pixel slot the note names. The
    demo walks both steps and renders them, and it finds each click point by
    hit-testing rather than by taking the button's centre, because
    `chassis.gib`'s `structure note` label is drawn over its own buttons: a click
    on one's middle lands on the label.

    A second list, on a screen outside the workshop, to the same two rules. The
    event list's grid says *"fill with this season's events (event_cell.gib)"* and
    the template says what a cell is, so `ra2ui::events` fills it from the game's
    own `Events.txt` — 25 events, whose tier counts are 2 in eleven and 4 in
    fourteen, and whose file has two defects worth naming: `Robo-Depot`'s second
    tier names the prizes directory and no model in it, and the last record closes
    with three numbers instead of four because the file ends there. The file is
    Windows-1252 with exactly one high byte, an apostrophe.

    Those cells did not draw at first, and what they drew was the *record* of a
    renderer rule: each cell showed only a blurred wash of its logo's background.
    The cells were in the right places and carried the right art — the nine
    colours differed per event — so the question was what size the art was drawn
    at, and one line in `art_quad` answered it: the quad's rect was **1280×1005**,
    the design canvas, because `event_cell.gib`'s images carry
    `_Size 1280 0 1005 0`. That is the editor's *unset* — **eighteen fields in
    fourteen files** hold it — and drawing a 140×80 logo at the canvas stretches it
    across the screen. Read as unset, the cells draw their logos: *F.O.R.C.E.
    Tournament*, *Robot Magazine Combat Event*, *RoboDepot*, *Remote Xplosion*,
    *Powerbolt*. A real `_Size` is untouched by it — the armour grid's `88 0 72 0`
    still draws exactly as it did.

    The palette now follows the screen's own category and can be walked. Which
    category is current is the group's selected member — `Checked true` on
    `power button`, with the `co_power_s.bmp` state image showing — and a click on
    a member selects it in its group, the group being found from the hit path
    itself (the last `OptionGroup` above the `CheckBox` that was hit) rather than
    from a table. All six categories fill and render, and the counts are the
    archive's: **9, 38, 30, 13, 31, 10**. The mechanics palette draws a motor and
    then pistons — which is what that button's own note says it shows, *"motors
    and pistons"* — so the words in the data describe what actually appears.

    A click inside a category button reaches the grid instead of the button, and
    that is now understood rather than worked around. This turn settled it. `ChildOrder`'s tie is broken with the
    **earlier record in front**, and the witness is a screen's own note rather than
    the corpus's shape: `components.gib`'s palette grid shares the category bar's
    order and is later in the file, and that screen's notes (*"grid is filtered
    based on the category selected in this optiongroup"*, *"click to show
    preview"*) require the buttons to be clickable, which they cannot be behind the
    grid's cells. The 328-pair survey could not decide it — the dominant shape is a
    control followed by the label naming it, and those 122 buttons carry blank
    state art, so the label is in front of nothing either way — and only 6 pairs
    are even informative. The shell's own composition follows the same rule now:
    the chassis step layer is inserted *before* the screen it rides over, since it
    is the one that has to be clickable. A test drives each of the six category
    buttons with the palette filled and watches the selection move.

    How far that was *checked by eye* is worth stating, because it is bounded: of
    the six pairs in the corpus where art overlaps a sibling with a `Background`,
    the render says nothing for five — `exhibition/arena.gib` and the three in
    `teamhq/repairs.gib` and `teamhq/select_team.gib` all overlap a `ListBox` or a
    `Grid` that is **empty**, so the panel has nothing to cover and the picture is
    the same either way. The palette is the one where the grid can be filled, and
    there the flip is what made its category bar appear over the cells and take a
    click. So the change's evidence is the palette's own note plus its render, and
    the rest of the corpus's ties are unobservable rather than unexamined. Earlier: there are **328** sibling pairs in the corpus whose `ChildOrder` ties
    and whose frames overlap, and their dominant shape is a named control followed
    by the label that names it — but that shape reads the same either way round,
    because a button with a `Caption` attachment draws its caption itself from a
    `Text` such as that one. Only 6 of the 328 put art over a sibling with a
    `Background`, and an overlay over a list is as plausible over as under. So
    which sibling is in front stays **open**, and the one argument points *against*
    what we do: in `components.gib` the grid shares the category bar's
    `ChildOrder`, is later in the file, and so is drawn over the buttons the
    palette needs clicked. The hit test is what settles it. The overlap
    itself is the data's, and the editor had it too: `comp upper grp` reports
    `ReportedPos 0 0 33 0` against a `Position 0 0 0.07 1` of a ~460-pixel parent,
    and the grid is `Position 0 0 0 0` with `Size 1 1 74 0`. The designer saw an
    *empty* grid there — only the game fills it — so the overlap only appears once
    the C++ puts cells in. How the shell should compose it is the binary's
    business: `FUN_00437770` caches both widgets in members, and the tab's own
    set-up (`FUN_0042c0c0`) looks up the option group, sets `power button`'s state
    and selects category 0 — the file's own baked state, read the same way — but
    never moves, resizes or re-orders the grid. The filler it calls
    (`FUN_0042d8a0`) is what decides where a cell goes, and is the next thing to
    read. So this screen's
    category is set from its own `Checked` member, and the click rule is tested
    where it *is* reachable — `chassis_structure1.gib`'s `drawstyle optiongroup`,
    whose labels are base `Widget`s and so fall through.

    The component palette is filled from the game's own definitions. Its grid
    says what fills it — *"fill with components in the current category"* — and
    the six categories are the option group's buttons, whose notes name the
    definition's `type` field, the same field the component factory switches on.
    `ra2ui::components` reads `Components.cfz`: **131 components over 73 mesh
    directories**, partitioned by category into **power 9, extenders 38, mechanics
    30, mobility 13, weapons 31, extras 10**. A cell is the preview art at its own
    size, centred on the grid's cell — every one of the 131 previews is 96×72 with
    its icon inset eight pixels, so the 80 columns of content land in the
    78-pixel cell and the 72-pixel height on the 72-pixel row exactly. The render
    shows the palette's first four cells: two *Smart Zone* and one *BIG VOLT*
    battery, with the grid's selection bar on the first — and the words on that
    art read correctly, left to right, which also settles the earlier suspicion
    that the shell's art might be mirrored: it is not, and neither is the text
    drawn from the glyph page.

    The chassis structure screens now carry the layer that the game pins over
    them. `ch_stepbuttons.gib` is the snap checkbox, the finished button and the
    two step buttons, and its own fields place it: `Alignment 36` (right + bottom)
    with `Position 1 1 1 1` and `Size 0.46 1 70 0`. No structure screen has a slot
    for it and both say *"load this into content slot"*, so it is their *sibling*
    in that slot — and the test measures the consequence: the placed layer's frame
    is the content slot's bottom-right corner within half a pixel, 46% of its width
    and 70 tall, and the armor step (which has no layer) has none. Its own two
    buttons swap the two structure screens and leave the layer where it is, which
    a click test drives end to end.

    Getting that click to land found a second rule. `step 1 button`, `step 2
    button` and `snap checkbox` each have a **nameless `Widget` drawn one pixel
    larger over them** — a 26×24 record over a 24×22 `CheckBox` — at the same
    `ChildOrder` and *after* it in the file, so it is on top in the draw order; the
    click was landing on the label. The record is a **base `Widget`**, the
    abstract class the writer leaves headerless, so it has no behaviour to run and
    cannot be a control: a hit now falls through it to what is behind, which is
    what makes those three controls clickable at all. `chassis.gib` draws its two
    notes over its two buttons the same way.

    The shell is now checked against a document it ships that was not being
    read: `UI/Project Header.html`, the UI editor's index of the whole project —
    115 files, each with the name and class of its root widget, and the 489
    widgets the designer tagged with their notes. For the 113 entries that both
    ship and are indexed, the root is the name and class it states, and every
    tagged widget is present in the class it states; the check inlines each
    screen's `Proxy`s first, since the document tags what a screen *shows*
    (`botlabwin.gib` is tagged with the buttons of the `common/bottomnav.gib` it
    proxies). Its class vocabulary is exactly the reader's — 4 root classes and 13
    child classes, all `WidgetKind`s or the 5 `AttachmentKind`s.

    It also measured three asymmetries worth keeping. The document calls a file's
    *first record* its "root", and nine entries are rooted at an unnamed
    `[Background]` — the root's own attachment, written before the header
    (`hq_league.gib` opens `[Background]` then `[SplitFrame]`). It names nine
    `DisplayBox`es, and **no attachment in the corpus carries a `Name` at all: 0
    of 758** — the editor keeps names the writer does not serialize. And it is a
    snapshot: two files it names are not shipped (`common/skipintro.gib`,
    `testfile.gib`) and three screens are newer than it
    (`botlab/comp_properties.gib`, `botlab/info_display.gib`,
    `help/help_blank.gib`) — the last being the info bar the binary loads into
    `lowersplit`, which is why the binary's table had to be read at all.

    The shell has *content* now, and it is the game's. A grid or list is a
    template repeated per row, and the screen's own `Notes` name both: `armor.gib`
    says *"load this grid with the four armor cells"*, and `armor_cell.gib`'s own
    notes name what a row binds — *"'armor img' child should be the preview for
    each armor type"*, *"armor description displayed here"*, *"show weight value
    here"*. So the armour grid is filled from `Armor_Definitions.txt` inside
    `RA2.cfz`: four cells at `RowHeight 84` and `ColWidths 300`, each carrying its
    own armour type's texture, description and weight — the prose taken from the
    cell's `Glyph` *and* the `DisplayBox` markup that holds it a second time, with
    the markup's font and colour left as the file wrote them. The render shows the
    four real previews, plastic through titanium, and the test asserts the
    geometry, the bindings and the markup. That is the last of the shell's
    *content*: the rest of M5's lists want the same rule with a different table.

    The arena list is filled from the game's own picker. `exhibition/arena.gib`'s
    listbox says *"populate with arena names"*, and `Arenas/__init__.py` says
    exactly which arenas that is:

    ```python
    list = [x for x in arenas if x.in_list and type in x.game_types]
    list.sort(sortByName)
    ```

    so an arena is a script that registers one and does not set `in_list = False`,
    sorted by name, with the name and `preview` the picker hands the C++ beside
    each. That is **12** — `__init__.py`, `Hazards.py` and the prize scene
    `event_results.py` are out — and the list fills with a row each at the 18
    pixels its own `Sel Bar` states. The rows' *text*, though, is where the open
    item below shows.

    And one long-standing thing is now named, found while checking those rows: a
    glyph's quad draws its cell **and a slice of its neighbour**. One `Text`
    holding a single `A`, `M`, `O` or `N` renders as two shapes; a string renders
    as its letters with extra strokes over them; a `Text` written exactly as the
    corpus writes one smears the same; and `fg_mcn14`, `fg_mcn16` and `fg_mcn12`
    all do it. It is the renderer — the quad's uv or the page texture it comes
    from — and it means readings this report made from *rendered words* were less
    reliable than readings made from *ink counts in a page*. The `O` measurements
    above are the latter kind.

    The shell's art can be counted, and that is worth having before M6: resolving
    every name a shipped screen states the way the renderer does gives **1,303
    names across the 116 screens and nine that do not resolve**. Seven are
    `_Font System`, which is the editor's default on widgets that carry an empty
    `_Text` too — `System` is nowhere in the game, so there is nothing to draw and
    nothing to draw it with. The other two are art the game *names and does not
    ship*: `teamhq/xout.tga` (the event cell's overlay — which is why hiding it
    changes nothing) and `teamhq/event_bba.tga`. Beyond those two files, every
    screen's art is present.

    One label the notes have carried as "drawn but does not paint" for a while is
    `botlabwin.gib`'s `title txt`, and it is a *font* artifact rather than a
    layout one. The face is `fg_d20` — the shell's biggest, ascent 26 and descent
    −14, used by 15 widgets in 5 files — and a probe of `ROBOT WORKSHOP D20` in it
    comes out `R BT W RKSH P D2`: **every `O` draws nothing**, the four in `ROBOT`
    and `WORKSHOP` among them, while the letters around them are correct. So the
    title is drawn and its `O`s are not, which is why the word is easy to miss
    against the toolbar art. Measured across all nine faces it is **two** of them, not
    one: `fg_d20`'s `O` holds 21 pixels over 3 of its 11 columns and `fg_mcn12`'s 12
    over 2 of 8, where the other seven draw a whole letter. `teamhq/select_team.gib`'s
    title is the second face's — it reads `SELECT Y UR R B TI CS TEAM` at 12 pixels.
    The cause is the *font file*: `fg_d20`'s
    record 45 for `O` is a full 11×14 rect whose ink sits in its last three
    columns (21 pixels), while the **same record in `fg_mcn14` draws a whole `O`**
    (45 pixels over eight columns), and the rects around it are a packed row. So
    the record mapping and the rects are right and the page is blank where that
    `O` should be; its advance is intact, which is why the letter is *skipped*
    rather than misplaced. The game reads the same file, so this render is
    faithful to the data — and the old suspicion of "a one-record slip" in the
    glyph table is refuted, since the same index gives the right letter in
    another face.

    The labels that looked wrong were not a face's problem but every face's: the
    glyph page is stored **bottom-up** and neither renderer swapped the `v`s of a
    glyph's quad, so every label in the shell was drawn upside-down. It was found
    by measuring a shape rather than reading one — a capital `T` carries its bar
    at the top and an `L` its foot at the bottom, so in a right-way-up drawing
    `T`'s ink is top-heavy; it came out bottom-heavy, and the same asymmetry
    inside the page's own byte rows said why. The fix is the swap, in the
    engine and in `tools/ui_compose.py`, and the proof is a picture with words in
    it: the workshop's toolbar now reads `Overview Chassis Components Wiring
    Paint Shop Test Robot`. That both renderers agreed while both were wrong is
    the one thing the second renderer cannot catch, and it is now written down as
    such. The horizontal axis was checked and is **not** flipped — `LOCAL FJORD
    12345` draws as itself in `fg_d26` and `fg_mcn16` — after a column-based check
    had suggested otherwise: that check measured a window that can catch a
    neighbouring glyph's ink, and the drawing is what settled it.

    The last case the count cannot express turned out not to need one. An
    uncounted record joins the nearest record that still has a slot free — the
    caption holders duplicate their button's `Position`, so they are the button's
    siblings, and `mainmenu.gib`'s land in `sawback grp` and draw over their
    buttons. When nothing has a slot the record is a record of the file's top
    level, which is the same rule applied to the screen: 165 such records across
    the corpus, 150 of them chromeless, and the 15 that carry a panel are
    authored against a full-screen ancestor or are fragments, so their frames are
    the same either way. Their `ChildOrder` then places them: the top level sorts
    like any other sibling set, which is what draws `import_texture.gib`'s
    screen-plus-one-pixel backdrop behind its dialog and makes `credits.gib`'s
    "infamous" window visible over its own `metaldots_pattern` panel for the
    first time. That sort retires two special cases — the "the top level keeps
    its own order" rule, read off a main menu that was a thirteen-root forest, and
    the resolver's re-parenting of three bar-chrome images onto the bar written
    before them. Placement is unchanged by it (the corpus counts above are the
    same before and after) and the pictures are right. `ui_compose.py` draws the art, the
    labels and the new `Lines`/`Glyph` quads; it does not yet inline proxies, so
    the composite of a proxied screen is that screen without its nav bar.

    The shell's own Python runs. `plus` now answers `addWindow`/`addText` for
    real and `giScript` is a module the shipped `Scripts/Main.py` can hook
    itself into: the boot file runs as written — its `print` reaches `giScript`,
    it imports every arena and brain, and it registers `dispatchEvent`, which the
    engine reaches by name — and `Gooey.Plain`, the shipped wrapper, builds its
    debug windows through it. What it builds is a real `ra2fmt::gib::Widget`
    tree, so `ra2ui` places it and the UI renderer draws it by the same path a
    screen takes; `ra2-render --example gooeyshot` boots the shell, has the
    game's Python build a three-line debug window, and writes the PNG. Four new
    tests cover the boot, the window, the colour and the callback. What a script
    window does *not* yet have is the engine's own chrome — the grey panel the
    game draws behind a debug window — because that is the C++'s and nothing in
    the data says what it is.

    The last of the corpus's widget kinds are drawn, and the fragments compose.
    A `Proxy` names another `.gib` and nine shipped screens name
    `common\bottomnav.gib` — the bottom navigation bar — so a screen is composed
    before it is placed and `botlabwin.gib`'s own 49-pixel strip at the bottom of
    its content area carries the bar at `512,444` rather than the file being
    re-authored. A `Glyph` is a paragraph: `_Margin 0 w h` is the box, `_Text`'s
    breaks are the two characters `\n`, and the binary's own glyph writer is what
    says so — it emits `_Size <flag>` and one `_Margin a b c` per list entry, and
    the label it writes first carries no newline, which is why the file line is
    `Margins _Size 1`. A `Lines` is a polyline in point *pairs*: four segments,
    and the ten that are visible are the focused list's box. `Placeholder` is the
    one kind left, and it is the rectangle the 3D view fills — not the shell's.

    Writing them found a resolver bug with a picture's worth of consequence.
    `MasterSize` is **one axis** — all twenty-six in the corpus are two tokens,
    `0.14 1`, `90 0` — and the resolver read it as a two-axis pair, failed to
    find four numbers, and fell back to *half the frame*. The fallback is exact
    whenever `MasterSize` is `0.5 1`, which most of them are, so nothing had
    caught it; `botlabwin.gib`'s `0.14 1` is what did. With the axis read
    properly, `botlabwin.gib`'s title group lands at y 158 rather than 512.5, and
    across the corpus 23 fewer widgets fall outside the screen and two more
    files place entirely: **78 off-screen widgets in 116 files, 73 of them
    entirely on screen** (was 101 and 71). The corpus test carries the new
    numbers and the reason.

    Placing the containers also caught a bug in the *check*, not the engine: the
    compositor sampled an image at its page rect, so every art cell whose rect did
    not start at the page's origin was drawn from the wrong part of its file —
    `ra2_logo.tga`'s 512×76 rect starts at `u0 = 0.15625`, so the middle of the
    logo was stretched across the logo's own frame. The file is already cropped to
    its cell, so the file is sampled 0..1 and the rect only says whether the thing
    is a cell, a tile or a whole-page image. With that corrected the main menu
    comes out as the main menu — the `ROBOT 2 ARENA` logo legible, the saw plate's
    teeth above and below the plate, the rails, the cord and the battery — and the
    options panel's radio buttons come out checked and unchecked rather than as
    two plain circles. Several earlier "recognisable" pictures were drawn wrong
    for this reason, which is the argument for reading the render rather than
    nodding at it.

    Text landed. The font container is pinned — `68 + 95*28 + 256*256*2 =
    133800` on every small face (`40 + 96*28` is the same sum, and the loader's
    own glyph arrays settle which), `527016` on the two large ones — the loader
    is decoded (`FUN_005cd560` reached from *"Failed to load font"*:
    rasterise from `0x20`, page size from the face, GDI metrics out of 26.6,
    cached in AppData, with `FUN_00591b50` the thing that registers a face), and
    the record for a character is `(c - 34) * 28` — measured by drawing, since at
    that offset the main menu's own `MULTIPLAYER TEAM HQ` label comes out reading
    `MULTIPLAYER TEAM HQ` and at neighbouring offsets it does not. Drawn spaced at
    four times size the face reads `A B C D E F G …` and `O P Q R S T …` with
    every letter its own, and all 96 rects are distinct — two earlier readings
    that said otherwise were the *reading*: a blend bug in the harness, and the
    compositor sampling every glyph's rows turned over — which is the `V`-ish `M`
    that made `MULTIPLAYER` read as `VELAIIPLOA ER`. `ra2ui::Fonts`
    loads the nine faces from `RA2.cfz` and `resolve_with` sizes a zero-`Size`
    label from its text, which is what `Size 0 0 0 0` means on every label in
    every screen. `tools/ui_compose.py` draws those labels with the game's own
    faces and fore-colours.

Each of these is tracked with its evidence and fix path in
[`06-deferred-fidelity.md`](06-deferred-fidelity.md) (F1–F13).

---

## 5. Verification commands

Everything above is reproducible:

```bash
# Havok/version identity and physics semantics
strings -n 6 "local/game/Robot Arena 2.exe" | grep -E 'hk171|Havok|GMID_'

# The physics model of every stock arena, in plain text
grep -A16 GMID_HAVOK_SIMOBJECT "local/game/Arenas/octagon/octagon_txt.gmf"
grep -B2 -A12 GMID_HAVOK_RIGIDBODY "local/game/Arenas/octagon/octagon_txt.gmf"

# The armour table
engine/target/release/ra2 cfz local/game/RA2.cfz cat "Armor_Definitions.txt"

# Our engine's recovery of the same data
engine/target/release/ra2 info "local/game/Arenas/octagon/octagon.gmf"
engine/target/release/examples/rigdump "local/game/AI/Team2/Bot1.bot"   # chassis mass/hp
engine/target/release/ra2-sim "local/game/Arenas/octagon.py" \
    "local/game/AI/Team1/Bot0.bot" /tmp/check 2 2                     # collider source + gravity
```

Binary analysis of `Robot Arena 2.exe` was done with REA + Ghidra 12.1.4
(procedure `0x45cf40`/`0x462b08` = component-definition loading; string searches
`concussion`, `hitpoints`, `fracture`, `Armor_Definitions.txt`).

---

## 6. Regression status

| Check | Result |
|---|---|
| All 57 stock bots render | 57/57 |
| All 12 arena scripts assemble | 12/12 |
| `ra2-sim` drives on practice / octagon / hilltop / box | OK — one jointed robot, scripted drive, frames written. The sandbox is no longer arcade-driven: it drives the same assembly the duel does |
| Stock bots that drive on the jointed assembly | **27/45** travel ≥ 2 m in 3 s (`AI/Team*`, full throttle, box arena). The motor law is the game's own (F3) and the arenas now collide the geometry they draw (F5), so what is left is geometry *placement*: mount residuals and the unmodelled trigger/control layer. The four stock bots that use pistons are the only ones F8 moves: `AI/Team8/Bot2` 2.09 → 3.58 m and `AI/Team13/Bot0` 1.59 → 1.66, against `AI/Team5/Bot1` 7.49 → 6.29 and `AI/Team0/Bot1` 0.69 → 0.53 |
| `ra2-duel` on the octagon | **Engages again.** With F3's honest speeds the bout had ended with both robots parked in open floor; the cause was spawn placement, not a collider hole (F11): the duel invented a spread that landed on the rim of the Octagon's 3.7 m drop. It now spawns on the arena's own four `startpoint` attachments |
| `ra2-duel` damage model (F7) | **Validated on a deterministic ram.** `RA2_DUEL_DRIVE=ram` with two identical `AI/Team1/Bot0` on the box arena: contact at t≈5 s, an opening blow of **34** points, then ~1 point/s while the two rest against each other. Both ends of that match what the original's scripts say — hazards apply 25, `AI/__init__.py` calls `amount > 10` a "good solid hit" — after two real defects were fixed (contact was tested only between root bodies; a blow was a per-step *integral* of speed, so a fast impact weighed almost nothing) |
| `ra2-duel` with the AI | **Does not engage.** 30 s of `AI/Team1/Bot0` vs `AI/Team6/Bot2` on the box arena: the pair closes to ~7.7 m, then one robot pins itself against a wall (at `(-11.8, -7.3)` from t = 15 s on, reversing included) and neither touches the other all bout. This is the control layer (M4), not the damage model: the AI is a stand-in that only ever drives at the opponent |
| 45-bot drive survey | **Bit-identical** to F9 after F10 | the chassis is now dressed from the file's face groups, but only the *skin coordinates* changed, and the survey never looks at a texture |
| `cargo fmt --check`, `clippy`, tests | clean, 41 tests |

## 7. Recommended order of work

The per-component bodies exist (`ra2sim::rig` groups the components into welded
bodies, `ra2sim::robot` hinges every motor and drives the wheels through them,
and damage reshapes a body in place), and **both F1 (mount-point alignment) and F2
(the flip) have landed**: the jointed assembly is now the only robot path, the
arcade velocity controller is deleted, and `ra2-sim` was rebuilt on `Robot`.

Flipping it made the next blocker visible rather than removing it, and **F3/F4
have since landed** — the motor and wheel models now come from the game's own
data (the `power` pair, `grip`, `resistance`) rather than from guessed scales —
as has **F5**, which decodes `SIMULATION_GEOMETRY` (1/2 own mesh, 3/4 proxy,
5/6 display proxy) and `HAVOK_GEO_TYPE` (`Standard`, `Plane`), and fixes the
arena-coverage probe that had been silently rejecting every arena dug deeper
than 10 m. **F7's shape has landed too**: damage is an event (a blow priced by
the speed the robots arrive with, a spinner's tip speed as a rate), contact is
tested across every pair of bodies rather than the two roots, and a
deterministic ram puts the numbers in the range the game's own scripts describe.
The remaining blocker is *behaviour*, not the parts' parameters or the arenas:

1. **The control layer (M4) and mount placement's residuals.** This is now the
   thing that stops the headline duel: `AI/Team1/Bot0` vs `AI/Team6/Bot2` never
   touch over 30 s on the box arena, because the stand-in driver pure-pursues
   into a mutual orbit and pins itself against a wall. A spun weapon whose arm
   reaches the floor also presses with its full torque and lifts the robot; the
   original only spins one when it is told to. The game's own answer is the
   shipped Python AI, which needs the script layer.
2. **F7's remaining constant.** The formula, its coefficients and `fracture`
   are the engine's own; what is still calibrated is the contact **material's**
   damage scale — the per-material table lives in `MaterialObject` subclasses this
   build exposes only as RTTI, so the scale is set by the scripts' own anchors.
   Hitpoints are no longer on the list: they are the engine's per-category table
   (see §4.13).
3. **F9's remainder.** Buckling exists, and its *travel* is now the file's own
   movable region — the top half of the chassis, verified across all 43 stock
   bots that carry a buckle model. What is left is the *curve* between the ends,
   the "in" rate (ours: the footprint halves) and the silhouette, which is smooth
   where the game builds a subdivided crush mesh at runtime from indices that
   point into a mesh the file does not contain.
4. **F12 — the mass a zero-mass rigid body gets** (F10's residual). Every wheel
   stores `mass = 0`, so it, not the armour, is the likely source of the 16 %.
   Landed as a size-based rule, and that landing exposes a conflict: the documented
   mudtire (22.5 kg) wants a density that would put the mini wheel — the wheel
   every stock AI bot runs — at 4.2 kg, where the rest of the simulation is
   calibrated against 8. No single density satisfies both, so the constant is
   pinned to the local evidence (the bots' own behaviour, which is unchanged) and
   the doc says so. The engine's own fill is not reachable: its `.gmf` reader
   takes the zero at face value (`FUN_005c6820`), and the material classes are
   RTTI-only in this build.
5. **F6 — the friction combination law**, if the drive survey needs it.
6. **A spin motor's arm pose** — found while verifying F1, and separate from it:
   the game's own snapshot draws a hammer arm hanging down where ours lies level.
   The axle is the mount normal, so this is the spin motor's rest phase or its
   plane, not the link rule (see F1).
