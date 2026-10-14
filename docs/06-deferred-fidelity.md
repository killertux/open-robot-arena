# 06 — Deferred fidelity work

Things we know diverge from the original, kept so they can be picked up later
without redoing the research. Ordered by how much they block correct behaviour.
Evidence for each lives in [`05-validation-report.md`](05-validation-report.md);
the primary sources are the game's `*_txt.gmf` ASCII physics exports,
`Armor_Definitions.txt` (in `RA2.cfz`), the official component-exporter
templates, `tools/vendor/PyRA2`, and the binary itself (Havok Hardcore 1.7.1).

| Ref | Item | Status |
|---|---|---|
| F1 | Mount-point (attachment frame) alignment | **open — top priority** |
| F2 | Assembled-bodies path opt-in until F1 lands | open |
| F3 | Motors are velocity motors, not torque motors | open |
| F4 | Wheel `contact` / `grip` / `resistance` partly used | open |
| F5 | Collision shape kinds not decoded | open |
| F6 | Friction combination law differs from Havok 1.x | blocked on the binding |
| F7 | Damage model structurally aligned, numerically unsourced | open |
| F8 | Pistons are welded | open |
| F9 | Buckling is chassis deformation in the original | open |
| F10 | Chassis skin decals; 16 %-light weight gap | open |

---

## F1 — Mount-point (attachment frame) alignment — top priority

**What is missing.** A mounted component is placed only by its own translation,
quaternion, mount `angle` and `height`. The original also aligns the child's
attachment point to the parent's, which rotates the child. The `.bot` records
exactly which points are used:

```text
<int> mount point on the component      (mC)
<int> mount point on the parent         (mP)
```

and a `.gmf` attachment carries a full transform (`Point03: Type = Attach
ID = 4  Attach = Axle_M  Host = Axle ...` plus a `*NODE_TM`). The game's
placement is therefore

```text
local_child = local_parent * frame(parent, mP) * inverse(frame(child, mC))
```

which `ra2_render::component_local` omits (M1 recorded "no mount-point
alignment" as a deliberate simplification).

**Evidence that it matters.** `AI/Team14/Bot2.bot` ("Backyard Ripper") has a
Z-Tek motor on a baseplate anchor carrying a `disc5` rotor. The game's own
embedded 128×128 snapshot shows the rotor **flat on top** of the robot with four
axe heads horizontal; our render puts it **vertical**. The anchor and the motor
store the same rotation (`(0.707, 0, 0, 0.707)` — 90° about X) and differ only in
`angle` (90° vs 270°), so no reordering of `translation * quaternion * yaw *
height` fixes both the rotor *and* the wheels. The attachment frames are the
missing input.

**Consequences today.**

- Rotor/weapon assemblies can come out rotated 90°, so they hang below the
  wheels and drag on the floor.
- `05-validation-report.md` says all 57 bots are "placed correctly". That holds
  for position and for wheels, but is **approximate for orientations** and should
  be restated once F1 lands.
- Some bots' wheels come out with a vertical axle (e.g. `AI/Team13/Bot1.bot`, a
  1.07 drum bot), which get no drive torque on the assembly path.

**Plan.** Parse the attachment transforms from each component `.gmf` (we already
expose `SceneObject.transform` for attachments), work out the chassis' own
attachment points (they are not in the `.bot`), resolve `mC`/`mP` per component
and build the mount chain properly. PyRA2's `AttachmentPoint` model
(`tools/vendor/PyRA2/PyRA2-0.1.1/lib/PyRA2/GMIModel/Objects.py`) is the field
reference.

---

## F2 — The assembled-bodies path is opt-in until F1 lands

`ra2sim::rig::assemble` splits a robot into one body per rigidly-mounted group
with a hinge per motor; `ra2sim::robot` drives the wheels through those hinges
and reshapes a body in place (`PhysicsBackend::set_body_shape`) when a part comes
off, instead of rebuilding the world. It works — bodies are stable, wheels turn
at the commanded rate, damage lands and fights reach a KO — but because of F1 the
rotor can hang low, so a robot tends to rest on a weapon instead of its wheels
and loses traction.

So it is off by default:

| `RA2_ASSEMBLY` | Behaviour |
|---|---|
| `hull` (default) | one convex-hull body, arcade velocity drive — the M3 behaviour |
| `parts` | the jointed assembly: per-group bodies, hinge motors, torque/velocity drive |

Once F1 is fixed the parts path should become the default and the arcade
controller deleted.

---

## F3 — Motors are velocity motors; the original applies torque

The binary exposes `ApplyTorque`, `*ANGULAR_SPEED` and `GetMotorAngle`, and the
exporter notes motors round-trip as Havok constraints. The assembly uses a hinge
**velocity** motor whose target comes from the component's `power`
(`RA2_MOTOR_TORQUE`, 14 N·m per power unit) and `RA2_WHEEL_TORQUE` (1600 N·m).
Both scales are guesses: the original's `power` units were not recovered. A
torque-driven wheel model with the motor's own acceleration curve is the faithful
version.

## F4 — Wheel `contact` / `grip` / `resistance` are only partly used

Component definitions declare `contact = wheel_traction` (a separate
high-friction surface), `passthru = wheel_collision` (excluded from
self-collision), `grip` and `resistance`. A wheel body currently gets a flat
friction of 1.0; `resistance` (which should become a rolling-resistance friction
torque — Jolt's `max_friction_torque` on the hinge) and the separate traction
surface are not built.

## F5 — Collision shape kinds are not decoded

`SIMULATION_GEOMETRY` and the `GMID_COLLISION_BOX|SPHERE|CYLINDER|PLANE|MESH|
OBJECT` objects describe real shapes; we approximate with convex hulls
(components) and a triangle mesh (arenas). Values seen in the stock arenas:
2, 3, 5, 6.

## F6 — Friction combination law differs (blocked on the binding)

Havok 1.x's own era reference states contact friction is `sqrt(µ₁² + µ₂²)`; Jolt
combines as `sqrt(µ₁ · µ₂)`, and the safe `oxijolt` API exposes no combine-mode
hook. On the octagon (floor 0.80) with a 0.30 robot that is 0.854 vs 0.490 —
about 43 % less grip. Only raw FFI or a custom combine would close it.

## F7 — Damage model is structurally aligned, numerically unsourced

Weapons carry `concussion` and `piercing`; the script API exposes
`GetLastDamageReceived()` returning `(component, amount, time, by player, by
component)` (confirmed in the shipped `AI/Spinner.py`), and the community reports
piercing weighing more against the chassis. Our model applies `(concussion +
piercing) × impact speed` to the nearest destructible part. The original's formula
and the component `hitpoints` defaults were not recovered from the binary.

## F8 — Pistons are welded

`Role::Piston` components do not create a joint yet, so a piston's rod stays
welded to its body. The definition carries the axis and limits (`prismatic =
body, shaft, 0, 0, 1, 0, -.5, 0.4`), which maps onto a Jolt slider constraint.

## F9 — Buckling is chassis deformation in the original

The bot-format document describes the game subdividing the chassis faces and
storing "buckle points" that shift down and in on every hit. Our model only
detaches whole parts.

## F10 — Chassis skin decals and the 16 %-light weight gap

Chassis face-group decals can render rotated/mirrored. Separately, our computed
Mud Runner is 360.6 kg against the 418.6 kg the bot-format document records for
it (about 16 % light) — the chassis surface-area rule may need the collision mesh
rather than the display mesh.
