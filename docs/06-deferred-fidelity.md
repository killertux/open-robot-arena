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

**Status: the link rule is `A_parent · R_x(π) · A_own⁻¹` and it is now the
default** (`RA2_MOUNT=frames-flip`). It is the only rule that keeps every wheel's
axle horizontal *and* lifts a rotor clear of the chassis, and its output matches
the game's own botlab preview. One term is still missing — the free-spin `angle`
is not applied to chain links — which is what keeps the jointed
`RA2_ASSEMBLY=parts` path opt-in for now. The 128² embedded snapshot is a sanity
check only, never an oracle.

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

**Resolved:** there is no height to find. The rotor was low because the motor was
placed *axle-down*; the male-plug-into-female-socket flip
(`A_parent · R_x(π) · A_own⁻¹`) turns it *axle-up*, which lifts the disc 0.33 m
and puts it over the chassis. Chassis-mounted parts never move, so nothing floats.

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

### The mount model, from the shipped tools and the bots' own numbers

This pass went back to the **shipped artifacts** and settled most of the model. The
decisive sources are the official mod tools that ship with the game.

**(1) A component declares *which* of its mesh objects is its mount point.**
`#Tools/Component Exporter v1.2 - By Tim/Component Exporter/*.txt` are the
per-class templates the game's own exporter writes:

| template | the line that matters |
|---|---|
| `spinmotor.txt` | `master = motor_master`, `passthru = motor_Axle`, **`attaching = motor_Axle`** |
| `piston.txt` | **`attaching = piston`**, `prismatic = body, shaft, 0, 0, 1, 0, -.5, 0.4` |
| `wheel.txt` | `contact = wheel_traction`, `passthru = wheel_collision` |
| `cutting.txt` | `normal = false -1 0 0`, `decal = single_slash.tga …` |

`attaching` names a **mesh object inside the component's `.gmf`** — exactly the
objects that carry the `ID`/`Attach`/`Host` payloads we already parse. So a mount
point is not a tag in a table: it is a mesh object of the model, and its frame is
the point's frame. That is why `A_own` and `A_parent` are real 4x3 frames.

**(2) The botlab places a component by *snapping its chosen point onto the
parent's*.** `Tutorials/Mechanics_Tutorial.htm`, verbatim: rotate the preview so
you can see "the **green attachment point** on the back of the motor. Click it to
select it. **This means you will be placing the component based on that point.**
In this case, it will let us place the motor so that the axle faces up from the
baseplate." And for the next part: "we will use the **green attachment boxes** in
the preview window to select **one of the side attachment points** instead of the
default end point". Also: **CTRL raises** a component ("raise the motor up towards
the top of the chassis so that the axle sticks out") and **SHIFT rotates** it
("SHIFT rotate the extender when placing it if you would rather its starting
position be another direction"). That is `parent_mount`/`own_mount` (the two
named points), `height` (the CTRL lift) and `angle` (the SHIFT spin).

**(3) The `Attach` vocabulary is a mating scheme.** `Base_F`, `Axle_F` (wheels),
`Axle_M` (`Host = Axle`, motors), `Generic_F`/`Generic_M`. Every chassis child in
every stock bot uses `own_mount = 1`, which is its **`Base_F`** point, and
`parent_mount = 0` — the *baseplate*, not an attachment id. The `anchor3` whose
plug drives this bot carries `Base_F` (id 1) *and* `Generic_M` (id 2); the Z-Tek
motor carries `base_F, generic_F` (id 2) and `axle_M` (id 3). So the chain
`chassis → anchor → motor → disc` is `Base_F`-on-baseplate, then
`Generic_F`-into-`Generic_M`, then `Axle_F`-onto-`Axle_M` — all three mate
male-to-female.

**(4) The stored quaternion is a *constant per class of link*, not an
orientation.** `examples/mountdump` prints the raw fields:

- every **chassis child** — all four angle motors, the anchor, the four batteries
  and the control board — stores `q = (0.707, 0, 0, 0.707)` (90° about X), the
  *same* value regardless of the component, while the chassis itself stores the
  identity. So for a chassis child `q` is the **baseplate frame's orientation**.
- every **component-to-component child** — the four wheels, the Z-Tek motor, the
  disc, the four anglers and the four axe heads — stores `t = (0, 0, 0)` and
  `q = identity`. **The stored transform says nothing whatsoever about those
  links; they are determined entirely by the attachment frames** (plus `angle`).

**(5) Therefore the current default is provably wrong for chains.**
`RA2_MOUNT=stored` collapses anchor → motor → disc → anglers → axes onto the
anchor's origin, because every one of those links stores `t = 0, q = 0`. The
wheels only look right under `stored` because they are the *first* child of a
motor and their own point sits near their origin. The doc's earlier claim that
"chassis children are reproduced from the stored transform alone … and it is why
the wheels come out right" was true but misleading: it is not true of the chain.

**(6) A rule discriminator that needs no snapshot: the wheels must stay upright.**
Under `StoredOwn` (`M · A_own⁻¹`) every wheel's axle comes out
`(0, 1, 0)` — a wheel lying flat on its side, physically impossible — which
**refutes** that rule outright. `Frames` and `FramesFlipped` both keep the axles
horizontal.

**(7) `frames-flip` is the leading candidate.** It is the only mode that both
keeps the wheels upright *and* lifts the rotor above the chassis:

| mode | disc origin | axe-head reach | wheels |
|---|---|---|---|
| `stored` | collapses onto the anchor | — | upright |
| `frames` | `y = −0.219` (below the base) | `y = −0.73` | upright |
| `frames-stored` | `y = +0.219` | `y = −0.34` | upright |
| `stored-own` | `y = 0` | `y = −0.10` | **flat — refuted** |
| **`frames-flip`** | **`y = +0.446`** | **`y ≈ +1.0`** | upright |

`frames-flip` is `A_parent · R_x(π) · A_own⁻¹`: a male plug into a female socket
turns 180° between the frames, which is exactly the mate the payloads describe.

**(8) The snapshot agrees qualitatively — and gives a target.** Viewed (not
scored) the game's own 128² preview of `AI/Team14/Bot2` shows a large silver disc
**over the middle of the chassis**, seen face-on, with four claws on its rim. A
free camera cannot pin a plate's azimuth, so this is a sanity check rather than a
proof: the correct rule must put a *big* disc *above* the chassis with its normal
strongly tilted. `frames-flip` does that; the earlier "vertical plate" reading
came from scoring a silhouette, which cannot carry the claim.

**(9) The disc's geometry, measured from the mesh, not the preview.** `disc5`'s
plate is radius **0.795 m**, thickness 0.11, centred on its own origin (exported
with `ra2 obj` to be sure). Its four rim points sit at radius 0.62–0.71 and, under
the frame rules, the four anglers land **exactly** on them — a good check that the
rim links are being composed correctly. From the preview the plate's centre wants
to be at about `(0, 0.8–1.0, 0)` with normal ≈ `(0, 0.71, 0.71)`, i.e. the 45°
tilt of the `anchor3` wedge. Under `frames-flip` the disc's *origin* is where it
should be but the *plate* still renders about 0.45 low, so what remains open is
that offset — not the link rule.

**Exposed as** `RA2_MOUNT=stored` (default) `| frames | frames-flip | stored-own |
frames-stored`, and `examples/mountdump <bot> [index …]` prints every component's
world origin and world axes under all five, which is how the table above was made.

**(10) The rule is now the default, and the preview confirms it.** `frames-flip`
is `RA2_MOUNT`'s default and the sim reads it too (`rig::load` places the parts
with the same rule, so the render and the physics agree). Re-rendered with the
rule in place, `AI/Team14/Bot2` shows a big 45°-tilted disc over the middle of the
chassis with its four claws on the rim — the same reading as the game's own
preview. Checked across the stock set, `frames-flip` is also what makes the wheels
and rotors of most bots appear *outside* the chassis instead of collapsed inside
it. The per-mesh bounds from `examples/mountdump` and from the renderer's
`RA2_DEBUG=1` output agree to three decimals for every component, so the two
paths compose identically.

**Two smaller things the new test surfaced.**

1. **Burst-motor children do not meet their parent's point.** `component_worlds_mounted`
   applies a burst motor's start angle as `world[parent] * rest * link`, i.e.
   a rotation about the *motor's origin*, but a burst motor swings its axle, so
   the rotation belongs *inside* the link (`A_parent · R_z(start) · …`) — about
   the axle, which passes through `A_parent`'s origin. The new test skips
   burst-motor parents for this reason; it should not have to.
2. **A component may legitimately share the chassis' origin** (a SmartZone, for
   instance), so "no two components sit at the same point" is *not* an invariant
   — it looked like one until a stock bot broke it. Worth remembering before
   using overlap as a heuristic again.

**Still missing: the free-spin `angle` on chain links.** `mount_link` applies
`A_parent`, the flip and `A_own⁻¹`, but not `angle` — the SHIFT-rotate the player
used when placing the part. Chassis children get theirs (it is inside the stored
transform), so the gap is limited to chain-mounted parts; in the stock bots only
the four disc anglers use it (0/90/180/270°). That, plus the F2 items, is what
still keeps `RA2_ASSEMBLY=parts` opt-in.

**Superseded by the above**: the earlier claim in this document that "chassis
children are reproduced from the stored transform alone … and it is why the
wheels come out right" is only half true — it is why the *chassis children* come
out right, but the same rule collapses the chain.

### Decompile progress — the bot object model is mapped

This pass broke the structural question open. The `.bot` versions are stored as a
**float** (no version strings exist in the binary); the thresholds 1.04–1.12 sit
in a table at `0x6a69d4..0x6a69f4` in `.rdata`, and the reader gates on them.

**The per-component setup pipeline:**

| Address | Role |
|---|---|
| `FUN_004dde50` | bot-object load/setup: reads the version float, then `if (1.06 <= v)` and `if (1.12 <= v)` |
| `FUN_00416360` | the per-component loop: for each component runs `FUN_004dd090` then `FUN_004dca90` |
| `FUN_004dca90` | **the mount/connection wiring.** Dispatches on the component's class name and `dynamic_cast`s: `SpinMotor` → `FUN_004dca70`, `AxleMount` → `FUN_004dca60`, `Steering` → a loop over `FUN_004f6d00`/`FUN_004f6d10`; each then calls `FUN_00508040` |

Those three per-class handlers turned out to be **not** the placement:
`FUN_004dca60` and `FUN_004dca70` are field accessors (`return this[0x280]` /
`this[0x2b8]`), and `FUN_00508040` merely resets fields at `+0x28c..+0x298`. So
`FUN_004dca90` only *links and clears* per-class state (which motor drives which
mount) — it does not compose transforms.

**Where that leaves it.** The per-component setup loop only wires, so the
placement is *not* computed at load time; it must be composed where a component's
world transform is needed, i.e. on the sim/draw path. Next candidates, in order:

1. walk the users of the class RTTI with `find_xrefs_to_name` on
   `Chassis::RTTI_Type_Descriptor` / `Component::RTTI_Type_Descriptor` — cheap now
   that Ghidra names them — and look for a pass that reads *both* a child's and a
   parent's mount data;
2. `Component` virtual method `FUN_0045ebd0` (works with 64-byte, i.e. 4x4,
   buffers) and its neighbours in `0x45cd50..0x45f2b0`;
3. `FUN_004dbf40` (907 bytes) which also references the `Component` TD.

**Also worth modelling**: `AxleMount` and `Steering` are real connection classes in
the game's model that we have no equivalent for.

**The real component class model** (class-name strings, then RTTI):

| Class | Name string | Notes |
|---|---|---|
| `Component` | — | base class; TD `0x6ee8e8`, COL `0x6bd118`, **vtable `0x6a6aa4` (22 methods)** in `0x45cd50..0x45f2b0`; nested types are `AttachDetails` (UI, an MFC `CRect`) and `Adjuster` |
| `Chassis` | `0x6f0664` | |
| `AxleMount` | `0x6f4314` | a *connection* class we do not model; RTTI `.?AVAxleMount@@` at `0x6f4344` |
| `Wheel` | `0x6f4334` | |
| `Battery` | `0x6f5a3c` | |
| `Steering` | `0x6fd760` | another connection class we do not model |
| `SpinMotor` | `0x6fd76c` | |
| `Weapon` | `0x6ff4f0` | |

Controller classes live in a `Bot` namespace: `Bot::Input` (`0x6ef904`),
`Bot::Command` (`0x6f1e54`), `Bot::Output` (`0x6f243c`), plus templates
`Bot::Button<SpinMotor>`, `Bot::Switch<SpinMotor>`, `Bot::Analog<SpinMotor>`
(`0x6feb54`…), and the binding action strings `Spin Clockwise` (`0x6fe7e4`),
`Spin Counter-clockwise` (`0x6fe7cc`), `On/Off Clockwise` (`0x6feb20`).

**Ghidra names the RTTI symbols** — the decompiler emits
`&Component::RTTI_Type_Descriptor`, `&SpinMotor::RTTI_Type_Descriptor`,
`&AxleMount::RTTI_Type_Descriptor`, `&Steering::RTTI_Type_Descriptor`. So from here
`find_xrefs_to_name` can walk a class's usage directly, which is much cheaper than
the byte-level RTTI walk used to get here.

Also noted: `Component` virtual method `FUN_0045ebd0` works with 64-byte buffers
(a 4×4 matrix), so matrix-carrying methods do exist on the class.

### Superseded: the byte-level RTTI walk

Kept because it is reproducible and was needed before the named symbols were
found: `AttachDetails` TD `0x6f0480` (name at `+8`, shared `type_info` vftable
`0x006b1e44`), COL `0x6bc53c`, vtable `0x6a5ecc` — one entry, the destructor
`0x45c860`; vtable writers `0x42b3b8` → `FUN_0042b380` (ctor, builds an MFC
`CRect` at `+0x24`) and `0x461988` → `FUN_00461920` (copy ctor, 14 dwords). This
is what proved `AttachDetails` is a UI class rather than the geometry one.

Other landings: `FUN_00470ff0` enumerates `Robot Designs\*.bot`; `FUN_0042c850` is
botlab placement *UI*; `FUN_00437770` is 7.3 KB, references `Chassis`, and contains
no attach/mount/matrix identifier at all.

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
