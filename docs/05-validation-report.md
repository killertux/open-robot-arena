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

**Open [OPEN]:** our component masses come from the `.gmf` `RBCollection`, but
Mud Runner totals 360.6 kg against the documented 418.6 kg — about 16 % light.
Not yet explained; a candidate is that the chassis surface area should be taken
over a collision mesh rather than the display mesh.

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
| Shapes | `SIMULATION_GEOMETRY` selects box / convex / concave / proxy (values 2, 3, 5, 6 observed) | tri-mesh + convex hull | **Partial** — we use the referenced geometry but do not decode the shape kind (see §4). |

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
   motors round-trip through *Havok constraints*. Our drive is a velocity
   controller over a single hull. **[OPEN]** — the planned M3 refinement
   (per-wheel jointed bodies + motor torque) is what closes this.
10. **Wheels have declared traction geometry.** Component defs carry
    `contact = wheel_traction` (the high-friction surface), `passthru =
    wheel_collision`, plus `grip` and `resistance`. We merge everything into one
    hull and compensate with a low hull friction (0.15 in the sandbox). **[OPEN]**
    — also closed by per-wheel bodies.
11. **Collision shape kinds are not decoded.** `SIMULATION_GEOMETRY` /
    `GMID_COLLISION_BOX|SPHERE|CYLINDER|PLANE|MESH|OBJECT` describe real shapes;
    we approximate with tri-meshes and convex hulls. **[OPEN]**
12. **Damage model.** The original exposes `GetLastDamageDone/Received` returning
    `(component, amount, time, by player, by component)` — **[CONFIRMED]** from
    the shipped `AI/Spinner.py` — and weapons carry `concussion` + `piercing`,
    with piercing weighing more against the chassis. We use a contact-driven
    model with `concussion + piercing` × tip speed and "nearest destructible
    part" attribution. Structurally aligned, numerically unsourced. **[OPEN]**
13. **Component hitpoints defaults** were not recovered: the parse site stores
    the field at a fixed offset but the constructor's default was not located.
    Stock bots mostly omit it, so our per-role defaults stand. **[OPEN]**
14. **Buckling is chassis deformation, not just detachment.** The format doc
    describes the game subdividing the chassis faces and holding "buckle points"
    that shift down/in on every hit. Our M3 model detaches whole parts. **[OPEN]**

15. **Mount-point (attachment frame) alignment is missing — found while building
    the jointed assembly.** The mount *data* is now parsed and the field labels
    corrected (see F1); the *placement algorithm* is still open. A component is placed by its own translation,
    quaternion, mount `angle` and `height` only. The original also aligns the
    child's attachment point (`mC` in the `.bot`) to the parent's (`mP`), and a
    `.gmf` attachment carries a full transform, so the real placement is
    `local_parent * frame(parent, mP) * inverse(frame(child, mC))`. The game's own
    snapshot of `AI/Team14/Bot2.bot` shows its rotor lying **flat on top**, while
    we render it **vertical**. **This means the M1 claim "all 57 bots are placed
    correctly" holds for position and wheels but is approximate for
    orientations.** **[OPEN — top priority]**

Each of these is tracked with its evidence and fix path in
[`06-deferred-fidelity.md`](06-deferred-fidelity.md) (F1–F10).

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
| `ra2-sim` drives on practice / octagon / hilltop / box | OK |
| `ra2-duel` on the octagon | OK — runs 40 s, damage and detachable parts land. No KO in 40 s since the mount rule was fixed (F1): the components now sit where the game puts them, so the two bots engage differently. The earlier "KO in ~11 s" was measured with the whole chain collapsed onto its root. |
| Jointed assembly (`RA2_ASSEMBLY=parts`) | Stable, wheels turn at the commanded rate, damage lands; still not the default — F1's link rule is identified but its free-spin term (`angle`) is not applied to chain links yet, and F2 covers the rest |
| `cargo fmt --check`, `clippy`, tests | clean, 10 tests |

## 7. Recommended order of work

The per-component bodies now exist (`ra2sim::rig` groups the components into
welded bodies, `ra2sim::robot` hinges every motor and drives the wheels through
them, and damage reshapes a body in place). Building it surfaced the real
blocker, which is now item **F1** in [`06-deferred-fidelity.md`](06-deferred-fidelity.md):
**mount-point alignment**. Until that lands, the assembly is available but
opt-in (`RA2_ASSEMBLY=parts`) and the single-hull arcade path stays the default.

Suggested sequence:

1. **F1 — attachment-frame alignment.** No reordering of the current transform
   chain can make both a rotor and a wheel come out right; the attachment frames
   are the missing input. Everything else depends on the geometry being right.
2. **F2 — flip the default** to the jointed assembly and delete the arcade
   controller.
3. F3/F4 — torque-driven motors with the wheel's `contact` surface, `grip` and
   `resistance`.
4. F5 — decode `SIMULATION_GEOMETRY` / `GMID_COLLISION_*` into real shapes.
5. F7 — the damage model, against `GetLastDamageReceived`'s five-tuple.
6. F9 — chassis buckling as a visual layer.
