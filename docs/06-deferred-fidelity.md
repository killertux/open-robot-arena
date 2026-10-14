# 06 — Deferred fidelity work

Things we know diverge from the original, kept so they can be picked up later
without redoing the research. Ordered by how much they block correct behaviour.
Evidence for each lives in [`05-validation-report.md`](05-validation-report.md);
the primary sources are the game's `*_txt.gmf` ASCII physics exports,
`Armor_Definitions.txt` (in `RA2.cfz`), the official component-exporter
templates, `tools/vendor/PyRA2`, and the binary itself (Havok Hardcore 1.7.1).

| Ref | Item | Status |
|---|---|---|
| F1 | Mount-point (attachment frame) alignment | data done, rule identified (`A_parent·A_own⁻¹`); the spinner's height source is open — top priority |
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

### A second pass changed the model

**(1) The anchors are a tilted *family*.** `anchor1`..`anchor5` are
0°/22.5°/45°/67.5°/90° wedges: the Z axis of their `Point02` plug runs from
`(0,1,0)` (vertical) to `(1,0,0)` (horizontal). Stock bots use `anchor5` for the
majority of mounts, `anchor3`/`anchor4` for several, `anchor1` rarely. So a
spinner built on `anchor3` is a **45° spinner** — the earlier assumption that a
spinner has to come out flat was wrong, and it was the target the searches below
were (mis)fitting. It is also the strongest evidence yet that the child's
orientation really does come from the attachment frame.

**(2) Frame alignment is the rule for component-to-component mounts.**
`A_parent · A_own⁻¹` leaves the wheel's plane *and* position essentially
unchanged — the motor's `axle_M` and the wheel's `axle_F` are near-symmetric
frames there, so the two rules agree on the one placement the snapshot pins down
— while the rotor lands on the anchor's plug direction, i.e. the 45° tilt the
snapshot shows. That is the behaviour this item wants.

**(3) No transpose is at play.** 63 of 323 mesh objects carry a non-identity
transform and those render correctly, so a stored triple is an axis row (PyRA2's
`TransformMatrix` transposes the same way). Attachment points use the same
reader, so their frames are read correctly too — which matters, because 50 of
them are asymmetric and a transpose would flip all five anchors' plugs.

### What is left: where the spinner's height comes from

Whichever rule is used, the rotor assembly ends up **inside** the chassis (its
top reaches y ≈ 0.34 against the chassis' 0.748), while the game's snapshot shows
the disc covering the chassis' *top* face. The anchor's stored height is 0 and so
is each motor's; they name the same chassis mount (`0`) and the same own-mount
index, and their own mount frames differ by ~0.1 — so **no shared chassis frame
can lift the anchor without lifting the motors**, and a chassis frame carrying a
rotation would tilt the wheels. The lift has to come from a frame that is not the
`(parent point, own point)` pair.

The obvious candidate is a raised *chassis* mount plane. It was tested by
rendering with the chassis children lifted, scored by bounding-box-normalised
silhouette IoU against the embedded snapshot with the camera fitted **per**
hypothesis:

| hypothesis | best IoU |
|---|---|
| no lift | 0.399 |
| lift 0.374 m | 0.398 |
| lift 0.560 m | 0.404 |
| lift 0.748 m | 0.434 |
| lift 0.374 m + frames | 0.427 |
| lift 0.560 m + frames | 0.416 |

Nothing separates them. **Swings I first read into these numbers — a refutation,
then its reversal — were both noise**: the first pass compared renders through a
fixed colour threshold that mistook the snapshot's background gradient for
geometry. The 128² preview is a sanity check, not an oracle, so this question has
to be answered from the binary. `tools/mount_compare.py` is the harness (camera
by search, flood-fill foreground); it is worth keeping, but not worth trusting to
a few IoU points.

### A third pass: the stored transform is *not* the placement

**`translation.y` is exactly 0 in every component of every stock bot.** Across
all 57 bots and 882 components, min = max = mean = 0.0000, while x spans
-1.22..1.29 and z -1.00..2.52. The `height` field, by contrast, is really used:
94 components carry a non-zero height in 0.05 m steps up to 0.4.

So a `.bot` does **not** store a full local placement. It stores a two-dimensional
offset in the mounting plane, a rotation, a mount-plane yaw (`angle`) and a
height along the mount axis. The format document's "these positions are stacked:
rotate, translate, rotate by angle, translate by height" is incomplete — the
plane the offset lives in, and that plane's height, come from the mount frames.
This is why our renderer draws every component *at* the mount plane.

Every chassis-mounted part — all four angle motors, the anchor, the batteries,
the control board — names `parent_mount = 0`, so they all share one chassis
plane, and a raised plane lifts the motors as much as the anchor. Rendered a
sweep on `AI/Team14/Bot2` (`RA2_MOUNT=frames`, 3/4 camera at 45°):

| chassis plane | what the render shows |
|---|---|
| 0.00 | rotor buried inside the chassis; wheels hang nicely below it |
| 0.32 | rotor's wedge just emerging over the top; wheels at the chassis' lower middle |
| 0.50 | rotor a large wedge on top, but the batteries begin floating above the chassis' rim |
| ≥ 0.65 | batteries, control board and motor bodies sit on top of the chassis like a town |

Nothing in 0..0.75 puts the rotor on top *and* keeps the parts inside the
chassis *and* keeps the wheels on the ground. So a raised chassis plane is
refuted as the answer: the remaining freedom is the **chain**, which is exactly
where a per-link frame rule acts. The disagreement is in the link rule, not in
the chassis plane.

Exposed as `RA2_CHASSIS_PLANE` (default 0) so the sweep can be re-run.

### What the frame rule actually produces

Measured, not eyeballed: under `A_parent · M · A_own⁻¹` the rotor's own `+Z` — its
face normal, since `disc5`'s mesh is a plate in its XY plane — comes out
`(0.707, 0.707, 0)`. That is a **vertical** plate turned 45° in azimuth (the
`anchor3` wedge's 45°). A rendered top-down view confirms it: from directly above
the rotor is a thin edge, not a disc.

Whether that is *right* is a separate question, and the honest answer is that
**the snapshot cannot say**. Scoring the whole silhouette by bounding-box IoU
against the game's 128² preview, with the camera fitted *per hypothesis*, three
times over:

| rule | plane | best IoU |
|---|---|---|
| stored | 0 | 0.410 |
| stored | 0.374 | 0.423 |
| frames | 0 | 0.419 |
| frames | 0.374 | 0.387 |
| frames-flip | 0 | 0.402 |
| frames-flip | 0.374 | 0.347 |

Every difference is inside the noise of a 48×48 normalised mask over a coarse
camera grid. **Do not read the snapshot again for this** — it has now produced a
"refuted", its reversal, and a third near-conclusion in one session, each time
from a metric too coarse to carry the claim. The preview is a sanity check.

(The same caution applies to the height estimate of ~0.375 m: it is the value
that makes the wheels' reach the ground *and* the rotor clear the chassis' top,
but with the rule unsettled it stays an unverified estimate.)

### Decompile progress

The `AttachDetails` hunt is a **dead end, and a useful one**: it is a *UI* class,
not a geometry one. Its constructor (`FUN_0042b380`, in the botlab's `0x42xxxx`
range) zeroes a payload, calls `FUN_006084a0` and constructs an **MFC `CRect`** at
`+0x24` before installing its vtable; its copy constructor (`FUN_00461920`) copies
14 dwords. A `CRect` means it is the on-screen **green attachment-point box** the
UI strings talk about, not the mount maths.

The vtable walk that got there, for the next pass:

| Step | Value |
|---|---|
| `AttachDetails` TypeDescriptor | `0x6f0480` (name at `+8`; shared `type_info` vftable `0x006b1e44`) |
| CompleteObjectLocator | `0x6bc53c` (`+12` = TD) |
| vtable | `0x6a5ecc` — **one** entry, `0x45c860` (a destructor) |
| writers of that vtable | `0x42b3b8`, `0x461988` → `FUN_0042b380` (ctor), `FUN_00461920` (copy ctor) |
| `Component` TypeDescriptor | `0x6ee8f0`; its only nested types are `AttachDetails` and `Adjuster` |

Other landings: `FUN_00470ff0` enumerates `Robot Designs\*.bot`; `FUN_0042c850` is
botlab placement *UI* (compares a class to `Chassis`, prints "Robots cannot exceed
a maximum size", drives the attachment-point boxes); `FUN_00437770` is 7.3 KB and
references `Chassis` but contains no attach/mount/matrix identifier at all.

**Where the maths must be.** Since `AttachDetails` is UI and the botlab's *draw*
path places every component, the placement has to be reachable from it — but the
botlab classes are large and full of virtual dispatch. Two sharper routes for a
next pass:

1. find the **`.bot` reader** (it reads all eight component fields together, and a
   loader often computes the world transforms as it goes); it must compare a class
   string to `Chassis` (`0x6f0664`) and handle `SmartZone` (`0x6ee918`), and it is
   *not* any of the functions checked so far;
2. find the code that **consumes the 12-float attachment frames** — the gmf
   attachment reader lives at `0x5c38b0`, which Ghidra does not recognise as a
   procedure entry, so reach it by file offset and walk forward from the nearest
   prologue.

### Decompile progress

The `AttachDetails` hunt is a **dead end, and a useful one**: it is a *UI* class,
not a geometry one. Its constructor (`FUN_0042b380`, in the botlab's `0x42xxxx`
range) zeroes a payload, calls `FUN_006084a0` and constructs an **MFC `CRect`** at
`+0x24` before installing its vtable; its copy constructor (`FUN_00461920`) copies
14 dwords. A `CRect` means it is the on-screen **green attachment-point box** the
UI strings talk about, not the mount maths.

The vtable walk that got there, for the next pass:

| Step | Value |
|---|---|
| `AttachDetails` TypeDescriptor | `0x6f0480` (name at `+8`; shared `type_info` vftable `0x006b1e44`) |
| CompleteObjectLocator | `0x6bc53c` (`+12` = TD) |
| vtable | `0x6a5ecc` — **one** entry, `0x45c860` (a destructor) |
| writers of the vtable | `0x42b3b8`, `0x461988` → `FUN_0042b380` (ctor), `FUN_00461920` (copy ctor) |
| `Component` TypeDescriptor | `0x6ee8f0`; its only nested types are `AttachDetails` and `Adjuster` |

Other landings from this pass: `FUN_00470ff0` enumerates `Robot Designs\*.bot`;
`FUN_0042c850` is botlab placement *UI* (compares a class to `Chassis`, prints
"Robots cannot exceed a maximum size", drives the attachment-point boxes);
`FUN_00437770` is 7.3 KB and references `Chassis` but contains no
attach/mount/matrix identifier at all.


REA's MCP surface is what unlocked this: `resolve_containing_procedure` resolves
interior addresses that the CLI's `decompile` rejects, and `read_bytes` /
`search_bytes` make the RTTI walk possible.

| Found | What it is |
|---|---|
| `FUN_00470ff0` | the bot file enumerator (references `Robot Designs\*.bot`) |
| `FUN_0042c850` | botlab component-placement *UI*: compares a class to `Chassis`, prints "Robots cannot exceed a maximum size", drives the attachment-point boxes — not the maths |
| `FUN_00437770` | 7.3 KB and references `Chassis`, but contains no attach/mount/matrix identifier at all |
| `0x6ee8f0` | `Component` RTTI TypeDescriptor |
| `0x6f0480` | `AttachDetails` TD (nested in `Component`), name at `+8`; shared `type_info` vftable `0x006b1e44` |
| `0x6bc50c`, `0x6bc548` | reference the `AttachDetails` TD — CompleteObjectLocator candidates |

`Component` has exactly two nested types: **`AttachDetails`** and `Adjuster`.
`AttachDetails` owns the mounting data, so its vtable is the way in. Next step:
finish the COL → `pVftable` walk from `0x6bc50c`, list the class's methods and
decompile them (`batch_decompile` can take several at once).

### Ruled out

Test: the rotor's mesh `+Z` should end up along the anchor's plug direction; the
wheels' `+Z` must stay horizontal (they are the control).

| Rule | Result |
|---|---|
| `A_parent * S * A_own⁻¹` for any fixed 90°/180°/45° `S`, chassis links included | moves the wheels off the position the snapshot fixes |
| reading the quaternion as `(w,x,y,z)` | breaks the wheels |
| swapping the `yaw × quaternion` multiplication order | the same trade-off, exactly inverted |
| transposing the attachment frame | wrong for meshes, so wrong for attachments (finding 3) |
| a fixed `S` chosen to make the rotor flat | a 45° residual throughout — the target was wrong (finding 1) |
| brute force over ordered products of `{A_p, A_p⁻¹, A_c, A_c⁻¹, M, M⁻¹}` ± a 180° mate flip, against the flat-rotor target | same, for the same reason |

### The decompile

Started, not finished. Leads ruled out and confirmed:

- `0x5c38b0`, which references `Error Reading GM_AttachmentPt Data`, is not a
  procedure entry Ghidra recognises — reach it by file offset
  (`decompile --file-offset`) instead.
- the binary has no RTTI-derived procedure names: searching procedures for
  `Attach` and `Component` returns nothing, so the `AttachDetails` class has to be
  found through its vtable/RTTI rather than by name.
- the botlab attach UI at `0x42e8fe` only manages attachment-point *selection*
  ("Click a green box…", `UI_BotLab_AP_box_*`) — it is not where the placement is
  computed; that is a sibling function it calls.
- still the best entry points: `.?AVAttachDetails@Component@@` (`0x6f0488`), the
  `attach`/`attaching` definition keys (`0x6f23e8`, `0x6f2530`), the
  `GMID_ATTACHMENTPT` node-type table around `0x6f0ba4`, and the attachment reader
  at `0x5c38b0`.

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
