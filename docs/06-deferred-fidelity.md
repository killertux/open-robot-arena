# 06 — Deferred fidelity work

Things we know diverge from the original, kept so they can be picked up later
without redoing the research. Ordered by how much they block correct behaviour.
Evidence for each lives in [`05-validation-report.md`](05-validation-report.md);
the primary sources are the game's `*_txt.gmf` ASCII physics exports,
`Armor_Definitions.txt` (in `RA2.cfz`), the official component-exporter
templates, `tools/vendor/PyRA2`, and the binary itself (Havok Hardcore 1.7.1).

| Ref | Item | Status |
|---|---|---|
| F1 | Mount-point (attachment frame) alignment | data done, algorithm **open — top priority** |
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

**Status: the data side is done and verified; the placement algorithm is still
open.** Without it some weapon rotors land in the wrong plane — `AI/Team14/Bot2`
("Backyard Ripper") renders its disc vertical where the game's own embedded
snapshot shows it flat.

### In the code now

- `ra2fmt::bot::Component` exposes **`parent_mount`** (the point on the parent)
  and **`own_mount`** (the point on this component). The file stores
  `parent_mount` first. The community bot-format document has these two labels
  the other way round, and we did too. Verified three ways: PyRA2's reader reads
  `parentMount` then `attachmentPoint`; a stock wheel declares `4` on its parent
  and a stock angle motor has exactly one point 4 (`axle_M`); and that point
  mates with the wheel's own `axle_F`.
- `ra2fmt::gmi::Attachment` parses an attachment payload into `id`, `types`
  (`base` / `generic` / `axle`, each `_F`/`_M`), `host` (`Host = Axle`) and its
  frame, with `GmiScene::attachment_table()` keyed by the id a `.bot` names and
  `Attachment::accepts()` for the female/male mate test.
- `ra2sim::rig::Component` carries `parent_mount`, `own_mount` and the
  component's `attachments`; `ra2 info` prints each point's frame and
  `examples/botdump` prints the whole mount graph.

### Established about mounts

- Attachments are real frames, not tags: an `axle_M` point is hosted by the
  rotor (`Host = Axle`), which is why a wheel's `axle_F` hangs off a motor
  rather than the chassis.
- The chassis has a **single** mount point — every chassis child in every stock
  bot names `0` — and chassis children are reproduced from the **stored
  transform alone** (`translation * quaternion * yaw * height`). That is what
  `ra2_render::component_local` already does, and it is why the wheels come out
  right.
- So only *component-to-component* links (motor→wheel, anchor→motor→disc) can
  involve a mount frame.

### Ruled out

Test: the rotor's mesh `+Z` must end up vertical in bot space; the wheels' `+Z`
must stay horizontal (they are the control — the snapshot fixes them).

| Rule | Result |
|---|---|
| `A_parent * S * A_own⁻¹` for any fixed 90°/180°/45° `S`, chassis links included | wheels and rotor always share one plane — both vertical or both flat |
| same, applied only to component-to-component links | best fit still leaves the rotor **45°** out |
| brute force over ordered products of `{A_p, A_p⁻¹, A_c, A_c⁻¹, M, M⁻¹}` ± a 180° mate flip | same 45° residual |
| reading the quaternion as `(w,x,y,z)` | fixes the rotor, breaks the wheels |
| swapping the `yaw × quaternion` multiplication order | the same trade-off, exactly inverted |
| transposing the attachment frame | a no-op for every point that matters (all diagonal) |

The residual is exactly **45°**, and it comes from `anchor3.gmf`'s `Point02`
(`Generic_M`): `X=(-0.707,0.707,0)  Y=(0,0,1)  Z=(0.707,0.707,0)` — a 45° plug.
No rigid, shared treatment of the chassis mount turns that diagonal into a
vertical axis, so the game is not simply aligning the child's frame to the
parent's. Either the mate rolls about the frame's own Z instead of aligning the
full frame, or the point's axes carry a different meaning (PyRA2's own VRML
export of attachment points — `lib/PyRA2/GMIModel/VRMLExport.py` — wraps mounted
children in an extra `rotation 1 0 0 3.14159`, a 180° mate flip, which is a
strong hint that a flip *and* a roll convention are involved).

### Next step

Decompile the botlab's attach routine and read the formula. Leads already
located in `Robot Arena 2.exe`: RTTI `.?AVAttachDetails@Component@@`
(`0x6f0488`), the component-definition key strings `attach` (`0x6f23e8`) and
`attaching` (`0x6f2530`), the UI strings at `0x6f07f4`/`0x6f0828` ("Click the
Attach button…", "Click a green box to change the component's attachment
point…"), and the `GMID_ATTACHMENTPT` node-type entry around `0x6f0ba4`.

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
