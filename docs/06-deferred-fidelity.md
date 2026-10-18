# 06 — Deferred fidelity work

Things we know diverge from the original, kept so they can be picked up later
without redoing the research. Ordered by how much they block correct behaviour.
Evidence for each lives in [`05-validation-report.md`](05-validation-report.md);
the primary sources are the game's `*_txt.gmf` ASCII physics exports,
`Armor_Definitions.txt` (in `RA2.cfz`), the official component-exporter
templates, `tools/vendor/PyRA2`, and the binary itself (Havok Hardcore 1.7.1).

| Ref | Item | Status |
|---|---|---|
| F1 | Mount-point (attachment frame) alignment | **landed** — `A_parent·R_z(start)·R_x(π)·R_z(angle)·A_own⁻¹`; the "spinner's height" was a motor mounted axle-down, not a missing offset |
| F2 | Assembled-bodies path is the only robot path | **landed** |
| F3 | The motor law, from the game's own data | **landed** (speed is rad/s, torque x40 — both from the binary) |
| F4 | Wheel `contact` / `grip` / `resistance` | **landed** (`contact` is the material tag, not a collider); the robot's own bodies now share a collision group (a wheel no longer collides with its chassis) — survey **28 → 39 / 45** under Jolt's law, **37 / 45** with the game's own law (F6; vendored build) |
| F5 | Collision shape kinds (`SIMULATION_GEOMETRY`, `HAVOK_GEO_TYPE`) | **landed** |
| F6 | Friction combination law differs from Havok 1.x | **landed and on by default**: Havok's `sqrt(µ₁²+µ₂²)` through `ContactSettings::set_combined_friction` in `oxijolt` **1.1.0+jolt-5.6.0** (the newest on crates.io); the survey is **37/45** (vendored build; 38 with the prebuilt archive), the two losses being the F1 wheel-height residual — measured below |
| F7 | Damage model: an event, not a per-step integral | **landed, scale recovered**: the engine's own formula and `fracture`, and the hit's `scale` is the **attacker component's** `MaterialObject +4` — Metal **0.1**, Rubber **0.0**, Arenium **0.5**, with the ram plate's `metal(.4)` = **0.4**; the four armour plates still measure ×1.00 (they carry no term in the formula) |
| F8 | Pistons slide their children (prismatic) | **landed** (nothing drives them yet) |
| F9 | The chassis buckles (crush points from the `.bot`) | **landed, collider included**: travel is the file's own movable region, the curve is absolute-from-rest and measured at four points, and `set_body_shape` now removes/recreates the chassis' hinges with their angle as the reference — collider top falls 0.205 m at half damage and the robot still drives 6.81 m |
| F10 | Chassis skin decals; 16 %-light weight gap | **landed** (weight: see F12) |
| F11 | The Octagon's "hole" was a start block | diagnosed; spawns now use the arena's start points |
| F12 | Component mass when the `.gmf` stores zero | **landed** as our rule — size-based, density pinned to local evidence; the two anchors conflict; `0x5c6820` re-checked: it is a vtable slot, not a function, and takes the file's zero at face value |
| F13 | The script layer (M4) | **landed**: arenas, the shipped AI, the hazards and their zones all run the game's own code |

---

## F1 — Mount-point (attachment frame) alignment — top priority

**Status: landed.** The link rule is
`A_parent · R_z(start) · R_x(π) · R_z(angle) · A_own⁻¹` — the parent's mount
frame, a male plug turned 180° into a female socket, the player's free spin, and
the child's own point — and it is the default. It is the only rule that keeps
every wheel's axle horizontal and lifts a rotor clear of the chassis, and its
output matches the bots' own previews. The 128² embedded snapshot is a sanity
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

### The height that seemed to be missing — and why there was none to find

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

**Verified** (the rule is `mount_link`'s `FramesFlipped` arm, and the default:
`parent_frame · R_z(start) · R_x(π) · R_z(angle) · A_own⁻¹`, which is the rule
above, in that order). `AI/Team6/Bot2` rendered both ways at 1024×768
(`ra2-render` with `RA2_CAM=1.6,1.4,2.6,0,0.35,0`) is the check, because the
difference is unmistakable by eye rather than a matter of IoU points:

- `RA2_MOUNT=frames` (no flip): the two Z-Tek motors' rotors are **buried in the
  chassis** — one black disc hangs low at the right, the hammer arms are inside
  the hull and simply not drawn;
- the default (flipped): both motors sit **up on the chassis** with their arms and
  hammers clear of it.

The game's *own* view of the same bot agrees with the second: the 128² snapshot in
the file (an uncompressed TGA) shows the motor housings on the sides and **both
hammer arms out and clear**. Both are reproducible:

```bash
# the game's own render, straight out of the bot file
tools/bot_preview.py local/game/AI/Team6/Bot2.bot /tmp/preview.png
# ours, with and without the flip
RA2_CAM="1.6,1.4,2.6,0,0.35,0" ./engine/target/release/ra2-render local/game/AI/Team6/Bot2.bot /tmp/flipped.png
RA2_MOUNT=frames RA2_CAM="1.6,1.4,2.6,0,0.35,0" ./engine/target/release/ra2-render local/game/AI/Team6/Bot2.bot /tmp/stored.png
```

**A leftover the verification turned up, not part of the mount rule:** in the
game's snapshot the two hammer arms hang **down and out**, while ours lie
**level**, as if spun a quarter turn on. The arms' *plane* and their *rest phase*
are properties of the spin motor, not of the link — a spinner's axle is the mount
normal, so an arm that should hang means either the axle is not where we think or
the start angle is. `RA2_MOUNT=frames` versus the default make it easy to see;
worth its own pass, and not evidence against the rule (the *no-flip* render is
wrong in a much bigger way: buried rotors).

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

**Fixed: the free-spin `angle` on chain links, and a burst motor's start angle.**

`angle` is the rotation the player dialled in with SHIFT, and it is heavily used:
in the stock bots, chain links carry values like 1.571, 2.094, 3.491, 4.014 — the
odd ones are aimed weapons and extenders, not accidents. Two things had to be
settled, and the data settled the first on its own:

- **The axis is the mount normal, i.e. the point's Z.** `AI/Team14/Bot2` hangs
  four identical anglers on disc5's four rim points. Those points are 90° apart
  but share their axes, and their `angle` values are exactly the rim azimuths:
  points 2..5 sit at +Y, +X, −Y and −X in the disc's plane with angles 0, π/2, π
  and 3π/2. Only a spin about the point's Z explains that. The test
  `the_mount_spin_turns_about_the_mount_axis` checks it without hard-coding a
  sign: undo each angler's spin by its own `angle`, one way round or the other,
  and exactly one winding lines all four arms up.
- **The winding is settled by a bot preview.** Both windings give a
  self-consistent arrangement (all four claws folded one way, or a pinwheel), so
  the data cannot choose. `AI/Team6/Bot2`'s own preview does: rendered from the
  same camera, one winding splays its two arm-mounted maces **down and out**,
  exactly as the preview shows, and the other puts them up behind the chassis.
  The spin therefore lives in the *child's* frame — the same place a chassis
  child's `angle` sits — which is `A_parent · R_x(π) · R_z(angle) · A_own⁻¹`.

A burst motor's start angle is the *axle* turning, so it belongs on the parent's
side: `A_parent · R_z(start) · R_x(π) · …`. The old code applied it as
`world[parent] · R_z(start) · link`, a rotation about the motor's *origin*, which
both mis-placed and mis-oriented the child. With it inside, every link in every
stock bot lands its named point on the parent's — including burst-motor parents,
which the snap test no longer has to skip.

One more trap worth remembering: **a component may legitimately share the
chassis' origin** (a SmartZone, for instance), so "no two components sit at the
same point" is *not* an invariant. It looked like one until a stock bot broke it.

**One loose end in the burst term.** The snap test cannot see it: the rotation is
about the mount point's Z through its own origin, so a burst child's named point
lands on the parent's for *any* start angle — including none at all. Its only
evidence is therefore the placement itself, and the placement's orientation is
not settled: `AI/Team0/Bot0`'s DDT burst motor mounts a `generic_f` child point
on `Point03` (`Generic_M`), the ordinary male-into-female pair, and the arm's
visual mesh extends well past its collider, so an arm that looks out of place in
a 128² preview may be a viewing artefact. Settle it from a higher-resolution
render before trusting the start angle.

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

## F2 — The assembled-bodies path is the default — **landed**

`ra2sim::rig::assemble` splits a robot into one body per rigidly-mounted group
with a hinge per motor; `ra2sim::robot` drives the wheels through those hinges
and reshapes a body in place (`PhysicsBackend::set_body_shape`) when a part comes
off, instead of rebuilding the world. **It is now the only robot path.** The
arcade velocity controller is gone from the tree — `apply_drive`, the single-hull
collapse (`collapse_to_single_body`, `hull_shape`, `ARCADE_FRICTION`), the
`RA2_ASSEMBLY` switch and the hull-only loader (`scene::BotRig` / `load_bot`)
with it — and `ra2-sim`, the M2 sandbox, was rebuilt on `Robot` so that both
runners drive the same assembly.

Two bugs had to be fixed for the flip to hold, both found by it:

- **Burst-motor hinges crashed Jolt.** `rig.rs` passed the axle's absolute
  `[start, end]` through as the hinge limits, and Jolt requires those to straddle
  zero (`min ∈ [-π, 0]`, `max ∈ [0, π]`). The constraint is created in the rest
  pose, which the mount rule leaves at the axle's start, so the limits are the
  travel from there: `[(end-start).min(0), (end-start).max(0)]`, clamped to ±π.
  `AI/Team0/Bot0`'s DDT burst motor is the case that crashed.
- **The hinge axis and pivot were the motor's frame, not the mount point's.** F1
  established that a mount point's Z is the axis a child turns about, and its
  origin the pivot; the hinge still used the motor component's own origin and Z.
  For an `axle_M` mount the two differ only by a sign, but the DDT burst motor's
  child mounts on a `Generic_M` point whose Z is *perpendicular* to the motor's,
  so the arm turned about the wrong axis. `JointInfo` now carries the pivot and
  axis, and both the hinge and the wheel drive's rolling direction take them from
  there.

**What the flip revealed, and what it hands off.** The arcade controller drove
each robot by *setting its velocity*, which is why it could paper over geometry
the wheels cannot use. Driving the wheels honestly is harder, and a third of the
stock set does not manage it yet. Running every `AI/Team*` bot for 3 s at full
throttle in the box arena: **25 of 45 travel 2 m or more, 9 creep under 2 m, and
11 barely move at all** (`Team2/Bot1`, `Team2/Bot2`, `Team13/Bot1`, `Team8/Bot0`,
`Team0/Bot0` and friends). Their *lowest* hull points are their wheels, so it is
not a robot resting on its belly — it is the wheel model, the friction
combination law and the collision shapes: F3/F4 (a hinge-velocity motor with a
guessed torque scale, and a wheel `contact`/`grip`/`resistance` only partly
used), F6 and F5. The arcade path used to hide all three.

The duel runs end to end on the jointed assembly — the robots close, trade hits
and take damage — but it is gentler. The same 30 s bout (`AI/Team0/Bot0` vs
`AI/Team6/Bot2`) that knocked five parts off on the arcade path now produces
none, because one of the two cannot drive on the jointed path at all; a bout
between two that *can* (`AI/Team1/Bot0` vs `AI/Team6/Bot2`) closes and lands
damage but breaks nothing off. That is the same wheel and damage-scale gap.

---

## F3 — The motor law, from the game's own data — **landed**

A component definition's `power` line is two numbers and they are a **speed and a
torque**. The shipped set shows it: `Servo Motor 302/502/702` declares `3 26`
(slow, strong), its sibling `4 40`; `Z-Tek Motor` declares `36 24` and `Z-Tek
Motor (HP)` `36 48` — the same speed with twice the torque. The old code read
only the first number, used it for both, and guessed the rest (`power × 14`, plus
a flat 1600 N·m cap on wheels); this is the item that called both scales guesses.

The scripts pin the units down. The arena scripts drive their hinges through the
same pair — `Arenas/Hazards.py`: `SetPowerSettings(95, 15000)` to smash,
`SetPowerSettings(12, 700)` to retract, with `SetDirection(100)` for direction
and `Lock(True)` to park — and the implementation of `SetPowerSettings`
(`0x4810e0`) stores the *speed* unchanged and the *torque* **times 40**:

```c
this->speed = speed;  this->torque = torque;
this->speed2 = speed; this->max_torque = torque * 40.0;
```

So it is a velocity motor at the definition's speed with a torque cap of the
definition's torque × 40. The check that the factor is right: `Z-Tek Motor (HP)`'s
`48` becomes 1920 N·m, within 20% of the 1600 N·m the old guess needed to be for a
wheeled robot to move at all.

Implemented: `MotorStats` carries `speed` (rad/s) and `torque` (N·m), the hinge
uses them directly, and a wheel's top speed is therefore `its motor's speed × its
own radius`. A robot's speed is now a property of its parts — a servo-motor bot
crawls and a Z-Tek one does not — rather than the old global. Steering still adds
a yaw rate to the desired hub velocity (a tank differential); the original steers
through its `Steering` components and the control wiring, which is M4's.

**Burst motors park instead of spinning.** The tutorial is explicit — a burst
motor "slowly, but powerfully cocks the axle in a starting position and waits to
be triggered" — and the scripts park one with `Lock(True)`, which is the joint's
friction. `JointUse::Burst` does the same: motor off, friction torque set to the
motor's own torque, nothing triggering it yet. Driving one continuously made its
arm fight the floor and lift the robot off its wheels.

**The speed's unit is rad/s.** `SetPowerSettings` (`0x4810d9`, reached from the
script binding registered at `0x4bc7e0`) stores its first argument *unscaled* into
the controller (`+0xb0`, and again at `+0xbc`) while its second argument is
multiplied by the constant at `0x6a69f8` — which is exactly **40.0** — and stored
at `+0xb8`. So only the torque is rescaled, `RA2_TORQUE_UNIT`'s default of 40 is
the engine's own number, and the speed the scripts pass is the speed the hinge
motor is given: rad/s, since Havok 1.x's angular unit is. The motor data agrees —
`Z-Tek`'s `36 24` is 5.7 rev/s on a stock wheel, which is a robot's pace, while a
degrees reading would be 0.12 m/s. The hazards' `95` is simply a violent press,
bound to its own `HingeController` in the script.

**What it fixes and what it does not.** On the same 45-bot survey the count goes
from 25 to 26 — a wash — but the per-bot swings are large, and the reasons are
*not* the motor law: 10 bots improve (several stallers wake up; `Team5/Bot2` goes
from stationary to driving) and 21 get worse. The dominant mechanism is visible:
`AI/Team7/Bot1`'s blade ends up lying on the floor, propping the robot off its
wheels, and a spun weapon motor then presses with its full torque — more torque
makes that worse. That is mount placement and the unmodelled trigger/control
layer, not the motor. F5's collider work lifted the survey again, to 27/45.

## F4 — Wheel `contact` / `grip` / `resistance` — **landed, with a caveat**

A wheel's definition declares `contact = <object>` (the geometry carrying the
traction material — a second copy of the wheel, marked transparent: `miniwheel`
and `wheel_traction` have identical topology), `passthru = <object>` (excluded
from collision, so the visible hub does not collide), `grip` (1 for every stock
wheel) and `resistance` (0.1, 0.12 or 0.2).

Now used: `grip` is the wheel body's friction (it was hardcoded to 1.0 — the same
number, now from the data), and `resistance` becomes the joint's friction torque
— Jolt's `max_friction_torque`, which it applies *only while the motor is off*,
which is exactly where rolling resistance belongs. It never binds while a wheel
is driven, which is why it makes no difference to the survey; it matters when a
robot coasts. The hinge format carries the same idea (`FRICTION` in the exported
hinge fields).

The caveat is the `contact` surface itself. *Making it the collider* measurably
hurts: with it, `AI/Team10/Bot2` goes from driving 5.5 m to not moving at all.

**Retired, with a measurement.** The shipped objects say what `contact` is.
`Components/mudtire/mudtire.gmf` carries three overlapping meshes:

| object | vertices | radius (XZ) | thickness (Z) |
|---|---|---|---|
| `mud_tire` (the visible tyre) | 26 | 0.415 | ±0.170 |
| `wheel_traction` (the `contact` object) | 18 | **0.473** | ±0.130 |
| `h_wheel2` (the `passthru` object) | 26 | 0.349 | ±0.160 |

So the traction mesh is *bigger* than the tyre, not smaller: it is the wheel's
actual contact ring, and the hub named by `passthru` sits **inside** both, which
is why excluding it cannot matter to a convex hull. Our whole-scene hull already
contains the traction ring, so the wheel already rolls on the game's contact
radius; making the collider the *ring alone* is what stalls a robot, because the
ring is 4 cm narrower than the tyre it replaces and the robot loses its lateral
footprint. `contact` is therefore a **material tag**, not a collider — which is
exactly the role it plays in the model (`grip` is the friction). The caveat is
closed with no code change.

### The contact layer: a robot's own bodies do not collide

The survey's dominant mechanism was not friction at all — it was **Jolt colliding
a wheel with the chassis it is bolted to**. Every stock wheel sits partly inside
the chassis' hull (a mini wheel's top half is inside the box), and Jolt's
constraints do not disable collision between their two bodies by default. A
direct probe of `AI/Team0/Bot0` showed the two wheel bodies **in contact with
their own chassis on every step**: the hinge motor reached 17.7 rad/s for one
step, then the pinched wheel stopped and the robot sat still.

`BodyDesc` now carries a per-robot collision group (`CollisionGroupDesc`), and
`Robot::spawn` puts every part of one robot in a `GroupFilterTable` in which no
two sub groups collide. Different robots have different group ids, so they still
collide with each other (the Jolt unit test
`parts_of_one_robot_do_not_collide_but_two_robots_do` pins both halves).

A second, smaller mechanism was internal fittings: a **control board** is
already `non-physical` in the role model and a **power cell** sits inside the
chassis, but both were adding hulls. `Robot::part_shape` now excludes
`Role::Control` and `Role::Power`; a battery no longer provides a floor contact
under the robot. (The chassis' *drawn* battery is unchanged.)

Measured on the same 45-bot survey (box arena, arena start point, full throttle,
3 s, net displacement ≥ 2 m):

| tree | count | failing |
|---|---|---|
| before this pass | **28/45** | 17 |
| + self-collision group | **36/45** | 9 |
| + internal fittings add no hull | **39/45** | 6 |

The recorded 27/45 predates this harness; the same command measures 28/45 on
the tree before the fixes (`Team9/Bot1` is the borderline robot at 2.09 m). The
eight that remain, with the measured cause:

| bot | travel | measured cause (lowest hull point vs the wheel line) |
|---|---|---|
| `Team0/Bot1` | 0.51 m | **(b) a real stock contact:** the burst piston's **hammer head** is **0.298 m** below the wheels in the first 0.5 s, then the robot settles on a motor **0.046–0.087 m** below them. The piston's `prismatic` neutral is `0.0` on every stock definition, so the file's own rest is the hanging rod; the control layer (F8/M4) is what would retract it |
| `Team0/Bot2` | 1.39 m | **not a rest contact:** the lowest hull point *is* a wheel (0.000 m below the line) for all 3 s; it is slow and decelerating (0.35 → 0.19 m/s), so this is wheel/torque slip, not a dragging part |
| `Team11/Bot0` | 0.06 m | **(a) mount residual:** the four angle-motor bodies sit **0.079 m below** the wheels at rest and **0.081–0.101 m** while driving, so the chassis rests on its motors; the stored chassis-child transform is unchanged from the file |
| `Team12/Bot1` | 0.85 m | **(b)/(a):** at rest the **lawn-mower blade** is **0.443 m** and a motor **0.433 m** below the wheel line; after 1 s the **chassis itself** is **0.141–0.152 m** below the wheels and the wheels are lifted — the robot is lying on its blade/hull |
| `Team8/Bot0` | 0.12 m | **(a) mount residual:** the **small wedge** is **0.097 m** below at 0.5 s and a motor **0.082–0.100 m** below afterwards; it stalls on them |
| `Team3/Bot1` | 2.00 m | **(b) a curved path, not a stall:** 2.30 m by t=2.0 s, then it comes back to 1.47–2.00 m net; at rest no part is more than 0.02 m below a wheel. Its four wheel hubs sit at y = +0.10/+0.10/−0.10/−0.10, so the chassis is not level and full throttle traces an arc |

None of these is a friction failure on the default tree. The one data-supported
mount residual is the chassis-child placement in `Team11/Bot0`/`Team8/Bot0`
(stored transform puts a motor body below its axle); a candidate baseplate-frame
rule was measured (43/45 wheels-lowest instead of 29/45) but it breaks two bots
whose wheel axles come out fore-aft, so it was **not** landed. With F6's Havok
law switched on, the arc in `Team3/Bot1` and the 0.194 m axle-height split in
`Team7/Bot1` become the two extra failures (37/45 on the vendored build, 38 on the prebuilt archive); see F6.

## F5 — Collision shape kinds, decoded — **landed**

`SIMULATION_GEOMETRY` says which geometry a body simulates and `HAVOK_GEO_TYPE`
which kind it is. Both are honoured now.

**`HAVOK_GEO_TYPE` is `Standard` (0, a triangle mesh) or `Plane` (1)** — only
those two occur anywhere in the shipped data (166 `Standard`, 3 `Plane`). A plane
body owns no mesh at all: its `NODE_TM` gives a point and a normal (the matrix's
Y row). All three are planes you can name — `Clawtop`'s `Plane06` and `FlexTop`'s
`Plane01` are *floors* (normal up, 7.5 m and 9.9 m down) and the Octagon's
`topplane_collision` is a **ceiling** (normal down, 15.6 m up). We synthesise a
200 m quad for them; before, they produced zero triangles and the arenas fell
back to their visible meshes.

**`SIMULATION_GEOMETRY`**: 1 (convex) and 2 (concave) name the body's own object,
3/4 (proxy convex/concave) name `GEOMETRY_PROXY_NAME`, and **5/6 are the
display-proxy forms**, naming `DISPLAY_PROXY_NAME`. That last one is the decode
that mattered, and the shipped files are unanimous:

- Every arena's main collider declares `geom 5` — the Octagon's `main_collision`,
  the box's `arena_collision`, Hilltop's `Collision Box`, FlexTop's
  `floor_collision`, the Octagon's `outerfloor_collision` — and every one of them
  sets `USE_DISPLAY_PROXY` and names a *group* node (`main_display`,
  `arena_display`, `Arena`, `floor_display`). Components never do: they use
  `geom 3` with a real proxy mesh (`anglemotor_collision`). That is why 5 and 6
  were missing from the enum while 1-4 were documented.
- The named group has no vertices of its own — the Octagon's `main_display` is 57
  child objects — so its children are the geometry.
- Taking the body's *own* mesh instead drops whatever the artist put in the
  group. For the Octagon that is the floor of the apron 3.7 m below the play
  platform.

**A bug this exposed.** The collider is only taken from the rigid bodies when a
downward ray at the arena centre lands within 10 m of the lowest *visible*
geometry — but that test compared the ray's **distance** against that **height**.
Near y = 0 the two are close enough to pass; every arena dug deeper than 10 m
failed and silently fell back to the visible mesh. So `Hilltop` (-20 m),
`Clawtop` (-7.5), `FlexTop` (-9.9) and `Electric` never used their own rigid
bodies. Fixed. (The test itself is still one centre probe; the ring probe below
is what would make it honest.)

Resulting collider sizes: Octagon 328 → 1056 triangles, box 396, Bridge 1424,
Clawtop 1796, compressor 916, Electric 1904, half 1014, Hilltop 60 → 892, king
648, parkinglot 1024, Skull 922. `FlexTop` is 4 — correctly, because its floor
*is* the plane primitive and its play surface is a 400 kg movable body.

## F11 — The Octagon's "hole" was a start block

**Not a collider bug.** F3 recorded this as a hole: a robot at `(8.04, -9.87)`
fell from y 0.22 to -2.97, while `RA2_COLLISION=visible` kept it on the floor.
Chasing it with `arenadump` (a new height-map/profile tool) shows what actually
happens. The Octagon's play surface is a raised platform (top at -0.26) whose
extent stops at r = 10.2 along the diagonals, and the floor is 3.7 m below it
beyond that. The surface the robot was standing on is one of **four 200 kg
`startblock` bodies** the arena declares — `geom 2`, movable, rotated 45° and
straddling the platform's diagonal corners. Under `RA2_COLLISION=visible` those
blocks are frozen static meshes; that is the entire difference.

Two real defects came out of chasing it:

1. **Spawns ignored the arena's own start points.** Every stock arena declares
   four `type = startpoint` attachments in its model, and they sit on the play
   surface (the Octagon's are 8 m out, on the platform; the box arena's are at
   ±9.8). The duel invented a spread of `(max-min)*0.12`, up to 9 m — which for
   the Octagon lands on the rim of the 3.7 m drop. `ra2-render::start_points`
   reads them and the duel spawns on them, facing along the point's Z axis (which
   always looks at the arena centre). The 30 s `AI/Team1/Bot0` vs
   `AI/Team6/Bot2` bout that ended with both robots parked in open floor now
   closes to contact by t = 12 s and stays engaged.
2. The collider a robot is dropped onto must include every surface the arena
   draws — that is F5's display-proxy decode, and it is why the apron floor now
   exists as collision.

What is left is behaviour, not geometry, and it belongs to other items: the two
robots eventually grind hull to hull with **no damage**, because the damage model
only fires on impacts (F7), and on the box arena they still end pressed into
opposite walls. Deciding *when* to engage is what the original's control layer
does (M4); this AI only ever drives at the opponent.

## F6 — Friction combination law — **implemented, on by default**

Havok 1.x's own era reference states contact friction is `sqrt(µ₁² + µ₂²)`; Jolt
combines as `sqrt(µ₁ · µ₂)`. On the octagon (floor 0.80) with a 0.30 robot that
is 0.854 vs 0.490 — about 43 % less grip.

**Version checked.** crates.io's newest `oxijolt` is **1.1.0+jolt-5.6.0**
(published 2026-10-10), the version already pinned in `Cargo.toml` /
`Cargo.lock`; there is no newer release. `BodySettings` still has one scalar
`friction` and no combine mode, and there is no `EFrictionCombineMode` binding.
**But 1.1.0 does expose the hook the earlier check missed:** the safe
`ContactListener` receives Jolt's resolved `ContactSettings`, whose
`set_combined_friction` (`src/listener/contact.rs`) is applied by
`ContactSolver` (`vendor/.../ContactConstraintManager.cpp:826`). No upgrade, no
FFI, no unsafe.

**What is implemented.** `JoltBackend` records each body's `BodyDesc::friction`
by raw body id and installs a listener that sets
`sqrt(µ₁² + µ₂²)` on every added and persisted contact (`havok_combined_friction`
in `jolt.rs`). It is the default; `RA2_FRICTION_LAW=jolt` switches to Jolt's law for comparison.

**Measured, both ways.**

| measurement | Jolt `sqrt(µ₁·µ₂)` | Havok `sqrt(µ₁²+µ₂²)` |
|---|---|---|
| 1 kg box on a 20° slope, µ = 0.3 both sides, 1 s | slides **0.295 m** | holds, **0.001 m** |
| deterministic head-on ram, opening blow (`AI/Team1/Bot0`) | **24.4** | **30.3** |
| 45-bot traction survey (box arena, 3 s, ≥ 2 m, vendored build) | **39/45** | **37/45** |

**Why the survey drops by one.** The law is right, and it exposes the F1
wheel-mount residual rather than failing on its own: `AI/Team7/Bot1`'s two
wheels on one side have their lowest hull points at **−2.207 m** and the other
side at **−2.013 m**, a **0.194 m** axle-height difference, so only one pair
touches the floor and the robot spins: 2.03 m under Jolt's law becomes 1.49 m
under Havok's. `AI/Team3/Bot1` goes 2.00 → 1.47 m the same way (its four wheel
hubs are at y = +0.10/+0.10/−0.10/−0.10). The law therefore stays the default, and the two
survey losses are the F1 residual rather than the law; they disappear when
those wheel mounts are placed symmetrically.

## F7 — Damage is an event; its numbers are fully sourced

**Landed.** The *shape* of the model, the *formula*, `fracture`, and the
per-material scale.

- **Contact spans every body, not the roots.** A robot is several hinged bodies
  (a chassis group plus a body per motor), and the model tested only the two
  robots' root bodies — `Robot::body()` is `parts[0]`. A spinner arm or a wheel
  meeting the opponent is a different body pair, and those contacts silently did
  no damage. `combat::contacting_parts` now returns which bodies on each side
  touched, and both the attacker and the target are chosen among those.
- **A blow is an event; a grind is a rate.** The game reports damage in amounts
  *per hit*; hazards apply a flat **25** (`Arenas/Hazards.py`:
  `plus.damage(bot, 0, 25, ...)` — the flame and the electric zone both), and the
  shipped AI treats `amount > 10` as "a good solid hit"
  (`AI/__init__.py`). Our model *integrated* an instantaneous speed over every
  step a contact lasted, so a fast impact contributed almost nothing while two
  motionless robots eroded each other. Now the closing speed the robots arrive
  with is dealt **once**, when contact begins, and a spinning weapon's rotor tip
  speed is applied as a rate while it works the same joint.
- **The formula is the engine's own.** The hit (`FUN_00489470`, reached from the
  collision handler `FUN_00503c30`) is

  ```text
  damage = (piercing * contact_speed * 10 + concussion * magnitude) * scale
  ```

  — linear and additive in the two parameters a component declares, with
  coefficients **10** and **1**. That is the shape the community's own
  measurements fit (`D = 183.9·P + 100·C`, `docs/04` §5.2), which is the cross-check
  that says the reading is right. `magnitude` is the collision's *impulse*: the
  engine's magnitude function (`FUN_0045a5a0`) works from the contrast of the two
  bodies — their masses against their damping/resistance terms — and returns
  `0.5 · μ · v`, halving it at the last step. `scale` is a float on the contact
  material, `*(record + 0x64) + 4`.
- **Every stock weapon declares its two parameters**, and they are inside
  `Components.cfz` rather than in loose files: the blade is `concussion = .8`,
  `piercing = .3`, the sawblade `.9`/`.1`. A **chassis declares neither**, so its
  whole factor is the collision's own — which is what makes a spinner bite and a
  bare ram only shove. Our parser had been inventing `0.5` for both, pricing every
  part as half a weapon; they default to zero now, as the factory leaves them.
- **`fracture` is a per-hit threshold.** `FUN_0045f2b0` applies damage as a float
  subtraction clamped to what is left, and takes the part off outright when a
  *single* blow is **larger than** its `fracture` — extenders **15**,
  mechanics/mobility/weapons **20**, extras **60**, and anything with no category
  (a chassis) **0**, which is the factory's own table. So a hazard's flat 25 can
  never dent a ramplate (40) and never reaches the control board behind a chassis
  (41), while one solid pass shears a weapon off rather than grinding it down —
  the reading the community's notes describe, now with the comparison behind it.
- **A blow under `0.5` is discarded** — the handler's own gate. That is what stops
  two robots leaning on each other from eroding one another.
- **The per-role weights are gone.** They were ours and the engine has none: a
  non-weapon hit is priced by the collision alone, whatever part it lands on.
- **Component hitpoints — recovered.** The component definitions declare
  `hitpoints =` for only 9 of 131 parts; every other part's maximum comes from the
  engine's own table, which the component factory (`FUN_0045cf40`) applies by
  switching on the definition's **`type`** category — power **400**, extenders
  **100**, mechanics **400**, mobility **400**, weapons **1000**, extras **1000** —
  after which a declared `hitpoints =` overrides it. The accessors are
  `component + 0xe8` (maximum) and `+0xec` (current); damage is a plain
  subtraction (`FUN_0044f370`). The `.txt` parser that reads the keys is
  `FUN_004622f0` (`hitpoints` → `+0x98` on the *definition*, `type` → `+0x24`), and
  `FUN_0049aa30` reads the category back.

Measured on the deterministic head-on ram (`RA2_DUEL_DRIVE=ram`, two identical
`AI/Team1/Bot0` on the box arena): the opening blow is **24.4** on the default
(friction-law-off) tree and **30.3** with Havok's law on, and a six-second grind
shears two parts off. That is the range the game's own scripts describe — above
the AI's 10-point "good hit" line, around the hazard's flat 25.

**The scale is recovered: it is the attacking component's `MaterialObject +4`.**

- `FUN_00489470` reads `*(float *)(*(int *)(this + 0x64) + 4)`
  (`0x48950f`..`0x489522`); the same object's `+0x68`/`+0x6c` are the
  `piercing`/`concussion` the formula just used, and the `.txt` parser
  (`FUN_004622f0`) writes those two from the definition. It is the **attacker's**
  component, not the target's.
- The material object is created by `FUN_004893c0` (by name) and installed on the
  component by **`FUN_00489390`** (`0x489390`: `*(this + 100) = material`, i.e.
  `+0x64`). The component base constructor (`FUN_0045cc10` → `FUN_00489110`)
  installs a **Metal** material, and the factory (`FUN_0045cf40`) then overwrites
  its `+4` from the definition (`0x45d017`: `mov edx,[esi+0x64]; fstp [edx+4]`)
  when the parsed value (`definition + 0xb0`, parser `FUN_004622f0`) is `>= 0`.
- The constructors give the class defaults, stored at `this[1]` (`+4`):

  | class | constructor | `+4` |
  |---|---|---|
  | `Material` (base) | `0x402c70` | `0x3dcccccd` = **0.1** |
  | `Metal` | `0x4891e0` | inherits **0.1** |
  | `Rubber` | `0x4899e0` | **0.0** |
  | `Arenium` | `0x402c50` | `0x3f000000` = **0.5** |

- Only **five** of the 135 shipped definitions declare `material =`: the four
  casters are `rubber(0)` and `ramplate1` is `metal(.4)`. Every other component
  keeps Metal's 0.1. (A negative override leaves the class default; the factory's
  gate is `0 <= value`.)
- **Measured in our path**, same head-on ram, setting the attacker's components:
  Metal 0.1 → **24.4** damage, ram plate 0.4 → **97.5**, Rubber 0.0 → **0.0**
  on the default tree (Havok's law: 30.3 / 121.1 / 0.0); the 0.4/0.1 ratio is
  **4.00** either way. The parser agrees on the shipped files
  (`the_shipped_definitions_carry_three_scales`: ram plate 0.4, caster 0.0, saw
  blade 0.1).
- `material_scale` is no longer a `CombatConfig` constant; `RA2_DAMAGE` (the old
  calibration knob) is gone.

- **The four armour plates still carry no damage multiplier.** `FUN_0043c090`
  reads `Armor_Definitions.txt` (string `0x6f1278`, rows via `FUN_0043c950`) and
  labels the four stats "weight", "puncture", "concussion", "heat"
  (`FUN_0045d800`, `0x43c830`, `0x43c840`, `0x43c850`). The shipped rows are
  `0 8 0 0 80` / `1 12 0 0 100` / `3 16 0 0 150` / `2 20 0 0 175`: puncture and
  concussion are **zero on every plate**, and weight/heat are mass and
  durability. Measured multiplier per plate: **×1.00 for all four**
  (`no_stock_plate_scales_damage`) — the armour is not in the hit formula at all.
- The `+0x1c` slot is still **not** a multiplier: it is `FUN_0045f2b0`,
  apply-damage, which clamps/subtracts and returns the amount applied.
- Damage is still reported as a running total, not as the five-tuple
  `(component, amount, time, by player, by component)`. The pieces are in
  `Impact`; the script layer is what will need it.

## F8 — Pistons are prismatic joints — **landed**

A `Role::Piston` component now splits its child group onto a **slider** joint
instead of welding it, exactly as a motor splits its rotor onto a hinge. The
component mesh settles how: `burstpiston1.gmf` ships the piston as **two rigid
bodies** — `rb "body"` (14 kg, the housing) and `rb "shaft"` (8 kg, the rod) —
and the definition's `attaching = shaft` with `Host = shaft` on the mount point
is what hangs the children off the rod.

`prismatic = body, shaft, ax, ay, az, neutral, lower, upper` gives the travel.
Two details cost a debugging pass:

- **The axis is in the piston's own component space**, not the mount point's
  frame. Taking the mount's Z (the rule a hinge follows, F1) makes
  `AI/Team8/Bot2`'s rod slide *vertically*: its piston is mounted pointing
  forward, so the rod sank onto the floor and took the weight off the wheels
  (2.09 m → 0.24 m travelled). The declared axis is `(0, 0, 1)` on every stock
  piston — the same local +Z as every other RA2 axle — and transforming *that*
  by the piston's rest transform gives the real direction.
- **The limits are already Jolt-shaped.** `lower <= 0 <= upper`, metres from
  where the two frame points coincide, which is the rest pose. `burstpiston1`'s
  `-0.60, .8` becomes `limits(-0.60, 0.8)` directly.

A parked piston holds its rod with `friction_force` = the definition's `power`
force (× [`torque_unit`], the same inference F3 makes for torque), and the motor
is left off — the same treatment F3 gave burst motors. So a piston is a *stiff*
joint rather than a weld: it carries load until the load beats its force, and it
can never travel past its stops.

Verified: `AI/Team8/Bot2` 2.09 → **3.58 m**, `AI/Team5/Bot1` 7.49 → 6.29,
`AI/Team13/Bot0` 1.59 → 1.66, and the 45-bot survey holds at **27/45**. `oxijolt`
does expose `SliderConstraint`, so unlike F6 this needed no unsafe binding.

**Still open:** nothing *drives* a piston. A servo piston should follow its
control-board output and a burst piston should fire on a trigger; both need the
control layer (M4), so they park. The speed unit is also unverified: `power`'s
first number is metres per second here (a burst piston's `60` would be 60 m/s,
which is not a piston), so the linear scale is a guess pending the same binary
work F3 did for the wheel.

## F9 — The chassis buckles — **landed**

The bot file's chassis carries a crush model, and it was being read and thrown
away: `bucklePoint`s (a position plus the mesh vertices that meet there) and the
"top corners" (index quads around the top plate). Both are parsed now, and the
chassis deforms as it takes damage:

- **What the file says.** The game subdivides the chassis' sides and top, and
  "every hit then will shift these 'buckling' points down and in, causing the
  sides of the robot to buckle" (`docs/.../Bot file format analysis.txt` §"3D
  model of bot with buckling points"); it even saves the crushed model back to
  the bot file when it can.
- **What the data gives.** The buckle positions are in the chassis mesh's own
  space — the plate-point count of them (8 for the Octagon-sized `AI/Team1/Bot0`,
  6 for `AI/Team8/Bot2`) match mesh vertices exactly, and the rest are points the
  game adds. The plate outlines scale at **1 unit = 2.2 mm** (verified on two
  bots with different footprints).
- **What it does not give.** The vertex *indices* point into the model the game
  builds at runtime, which is not the one in the file, and the crush's magnitude
  is nowhere in the data: **all 57 stock bots are uncrushed** (their mesh
  footprint equals the bottom plate), and the binary has no `buckle`/`crush`
  string to anchor a constant. So the rule implemented is:

  - the movable region is the file's own — the lowest buckle point pins the floor
    plate, and the motion grows with height above it, so the top caves in
    furthest (matching the file: `AI/Team1/Bot0`'s buckle points sit at half
    height and at the top, none at the floor);
  - **down and in**, linear in the damage the chassis has taken.

  **How far it moves is now the file's own limit, not a figure of ours.** The
  buckle points are a ring at mid-height and a ring at the top, and it holds for
  *every* stock bot that carries a buckle model — **43 of the 57** (the other 14
  are from before the format had the section, and ship with no movable points at
  all) — so the movable region is the top half of the chassis and a full crush
  lands the top ring exactly on the mid ring, which is as far as it can go before
  the model turns inside out. `Robot::buckle` reads that region out of the file
  and moves through it, and `a_buckle_model_is_the_top_half_of_the_chassis` holds
  the invariant across the stock AI bots. What is still ours is the *curve*
  between the two ends (linear in the damage taken) and how far the sides come
  *in* — the footprint halves over the same sweep (`AI/Team1/Bot0`'s movable ring
  is 36 points: three level with each other at mid-height, three at the top, and
  the rest randomized in place, which is the format's own "randomizing their place
  so the crushed model doesn't look too symmetrical").

The drawn chassis follows, so a hurt robot is visibly crushed — before this a
chassis only ever died whole. **The collider follows too** (the re-bake is
described below): the backend removes the chassis' hinges, re-shapes the body,
and recreates them. Only the chassis has a buckle model; parts still detach.

Verified: a test pins that the movable region's footprint and top shrink while
the floor plate stays put, and that the crush is monotone; `RA2_BUCKLE=<0..1>` on
`ra2-duel` renders a chassis crushed to any fraction — two otherwise-identical
`AI/Team1/Bot0` at 0.0 and 0.8 side by side show only the chassis change (wheels
and floor plate untouched); and the 45-bot survey is **bit-for-bit unchanged**,
because an undamaged robot never buckles.

**The curve, measured and made absolute.** The crush used to apply a *relative*
step to the already-crushed mesh (`p.y -= step·span·w(current height)`), which is
asymptotic: at a full crush the top had moved **0.2536 m of the 0.3709 m span
(68 %)** and never reached the file's own floor. `Robot::buckle` now keeps the
chassis' pristine hull and drawn vertices (`rest_hull`, `rest_items`) and applies
the **absolute** displacement from them:
`p.y = y₀ − f·span·w₀`, `x/z = x₀/z₀·(1 − f·0.5·w₀)`. The curve is therefore
linear in the hitpoint fraction, and `the_crush_curve_is_linear_in_damage`
prints the four samples:

| fraction | top displacement | linear |
|---|---|---|
| 0.25 | 0.0927 m | 0.0927 |
| 0.50 | 0.1855 m | 0.1855 |
| 0.75 | 0.2782 m | 0.2782 |
| 1.00 | 0.3709 m | 0.3709 |

A full crush now lands the top ring exactly on the movable floor, as the file's
travel says it should.

**The collider follows too — the re-bake is landed.** `buckle` calls
`PhysicsBackend::set_body_shape` to re-bake the chassis body, but Jolt refuses to
change the shape of a body that a constraint uses:
`set_shape: UsedByConstraint(BodyId …)`, which is every motorized stock robot's
chassis. The backend now removes every joint that uses the body, re-shapes it and
recreates the joints in place, keeping their handles, their drive command and
their travel reference: a hinge's current angle becomes the new frame's reference
(`R_z(+θ)` on the second normal, verified by
`a_hinged_body_can_be_reshaped_without_losing_its_angle`), and a slider's
body-local anchors already preserve its offset
(`a_sliding_body_can_be_reshaped_without_losing_its_offset`).

**Measured on `AI/Team1/Bot0` at half damage** (`a_crushed_chassis_collider_follows_and_the_robot_still_drives`):
a 21×21 grid of downward rays over the chassis' footprint, keeping only hits on
the chassis body, puts the collider's top at **−1.106 m** pristine and **−1.311 m**
crushed — **0.205 m** lower, matching the drawn 0.1855 m plus the settle. The robot
then drives **6.81 m in 3 s** with the recreated hinges, and the joint count is
unchanged. The cost is four `remove_constraint` + `create_constraint` calls per
buckle step (one per chassis hinge on a stock 4-wheel bot), not per frame; it runs
only when the damage fraction changes.

**The subdivided silhouette** remains ours (see below); the curve is no longer
part of the gap.

Still open: the crush is smooth rather than faceted — the game's subdivided model
is what gives it the original's silhouette — and the game *writes the crushed
model back* on save, which we do not.

**The subdivided model, measured (and one attempt that did not earn its keep).**
The engine builds that mesh before anything can buckle: it "divides the triangular
faces that make up the sides and top of the 3D model of the bot into many more,
adding points to construct the vertices and faces", and the bot file shows the
result exactly. `AI/Team1/Bot0`'s chassis is **48 points and 28 triangles** in the
file (a welded, indexed mesh, indices up to 47), while its **36 buckle points
address an unwelded array that runs to index 115** — a **117-slot, 39-triangle**
mesh — and take **87 of those slots** (about 2.4 per point). The telling number is
this one: **28 of the 36 points are not vertices of the file's mesh at all**. They
are points the game added *to* it. That is why the subdivision exists, and why our
crush has to bend the surface through a weight function: a point that is not a
vertex cannot be moved.

I implemented exactly that — insert each buckle point into the faces it lies on,
fan those faces open, and move only the points the file names. It works
mechanically (the points become vertices, nothing else moves, and tests pinned
both), but it is not the game's mesh: a 28-triangle chassis came out at **192
triangles** with **481 of its 576 vertices marked movable**, against the game's 39
and 87. A mesh where 83 % of the vertices may move still bends nearly everywhere,
so the silhouette would not have improved — it would only have cost seven times
the triangles. Backed out rather than shipped.

Why it ran away, for whoever picks it up: the file's points sit on *shared* edges
and on the plate corners, so one point belongs to several faces at once, and a
per-triangle fan duplicates every insertion into each of them. The engine instead
retriangulates the movable region as a **shared-vertex mesh** — its 2.4 slots per
point is a handful of faces meeting at one vertex, not a copy per face. That is
the shape the fix has to take: a constrained retriangulation of the sides and top
with the file's points as single shared vertices, not a soup fan.

## F10 — Chassis skin decals — **landed**; the 16 % weight gap — **explained**

### The decals

The chassis is dressed from the **face groups** in the bot file, and we were
reading them and throwing them away. Each group is one side of the chassis (a
side, the top, the bottom): a polygon on the 256×256 skin, a rectangle bounding
that polygon, and the triangles that make it up.

- **The rule** (verified, not guessed): a group's coordinates are the skin
  coordinates of **its vertices in ascending index order**. Every group's
  coordinates land inside the group's own rectangle on **all 45 stock bots**, and
  the rule reproduces the model's own per-point UV table exactly on the chassis
  whose sides do not share vertices (`AI/Team1/Bot0` 48/48, `AI/Team8/Bot2`
  36/36, `AI/Team5/Bot2` 84/84).
- **The bug**: we dressed the chassis from the model's **per-point** UV table. A
  mesh point shared by two sides can only carry one coordinate there, so one of
  the two samples the wrong patch of skin — a decal rotated or mirrored. On
  `AI/Team13/Bot1` only **48 of 138** face-vertex coordinates came from the right
  patch; the fix corrects the other 90.
- **The evidence**: `AI/Team13/Bot1` ("Tornado") is the bot it shows on — its
  round top used to carry a stray green **square** decal; it now carries the
  green **rim** the file describes, and the tread pattern runs continuously. The
  other 44 stock bots are dressed exactly as before (their tables already
  agreed), so the change is surgical.

The file's own data was the oracle throughout: nothing here was tuned by eye.

### The 16 % weight gap

Retired as a formula error. The engine's own chain says the weight is what we
compute:

- the script binding `getWeight` (at `0x6fca00`, registered at `0x4cd74e`) is a
  two-instruction stub that tail-calls `FUN_004df0b0` (`0x4df0b0`), which is also
  what the bot editor's "Total mass of robot must not exceed 800" check
  (`FUN_0043ca70`) calls;
- `FUN_004df0b0` sums, over the robot's parts, `FUN_004e23f0`, which sums
  `FUN_004e24f0`, which accumulates `FUN_0045a4f0` — a plain read of the float at
  `component + 0x128`. So the game's weight is **Σ per-component stored masses**,
  with the chassis' being area × armour weight, exactly the rule we implement.

The `Class:` field cannot corroborate it: each AI team ships one light, one
medium and one heavy bot, and no single scale factor makes the classes follow from
any weight (fixing the eight "heavy" bots that weigh under 400 kg needs a factor
of ≥ 1.23, which would in turn misclassify 23 others). The six editor-saved bots
in `Robot Designs/` all match our class, but all six sit far enough inside their
class to prove little.

For Mud Runner itself the file stores `1 12 … 200`: **aluminium**, weight 12,
hardness 200 — which is 2× the editor's 100, precisely the AI-bot convention, so
the row is self-consistent. Our 7.55 m² chassis at 12 kg/m² gives 90.6 kg and
360.6 kg total. Two readings fit the doc's 418.6 kg: **steel** armour on the same
chassis gives 421 kg (0.6 % over), while the wheels' missing mass (F12) accounts
for the 58 kg exactly. **F12 supersedes this paragraph** — the wheels are the only
stock family whose rigid bodies store no mass at all, which is the single-cause
effect a 16 % gap should have.

---

## F12 — Component mass when the `.gmf` stores zero

**Landed as our rule, with the conflict it exposes written down.**

**`FUN_005c6820` re-checked.** It is **not a Ghidra function** in this analysis
(`No procedure exists at the requested address`); it is a slot in the
rigid-body reader's vtable at `0x6af710` (entry 11, alongside `0x5c6770` and
`0x5c6d10`). Disassembling from `0x5c6820` shows the reader's packfile calls:
each field is a `FUN_005ce970(dest, 4, 1)` and the first destinations are
`edi+0x20`, `+0x24`, `+0x28`, `+0x2c`, `+0x30`, `+0x34`, `+0x38`, `+0x3c`,
`+0x40`, `+0x41` — i.e. the float at `+0x20` (mass) is read **straight from the
file**, exactly as this section said. Nothing in the reader substitutes a
density or a material mass. The conflict therefore stands unchanged.

Every stock wheel's rigid body stores `mass = 0` — `fatboy_wheel1`, `miniwheel`,
`mudtire`, `shinywheel1`, `wheel1`, `wheel2`, `wheel3`, `wheel_slim` — as do
`slipperbottom` and `cannon_txt`. The engine's own fill is **not reachable**: the
body's reader takes the file's zero at face value (`FUN_005c6820`, the vtable
entry the packfile reader calls, reads the mass straight into the Havok body at
`+0x20`), so whatever gives those bodies their mass does so later, where the body
enters the simulation — and this build's material classes (`Metal`, `Rubber`, …)
are present as RTTI only. What we had was worse than a guess in one way that
mattered: a **flat 8 kg**, which made a 0.30 m³ mudtire and a 0.06 m³ mini wheel
weigh the same.

Now a zero-mass part is priced by the room it takes up: `bounding volume × 139`.
The density is the one that leaves the wheels the *stock bots* actually run (the
mini wheel, 0.652 × 0.652 × 0.135 m) exactly where they were, so nothing else in
the simulation moved — the drive survey's 27/45 baseline is unchanged.

**The two anchors cannot both be satisfied, which is the finding.** The one stock
wheel whose weight is documented is the mudtire at **22.5 kg** — the figure that
closes Mud Runner's own total (`360.6 + 4×22.5 − 4×8 = 418.6`, the documented
weight, and it is the reading F10's 16 % gap wants). Its bounding box is 0.947 ×
0.947 × 0.340 m, so that wants a density of **74** — which would make the mini
wheel **4.2 kg**. A density of **139** keeps the mini wheel at 8 and puts the
mudtire at 42 kg, badly overshooting the documented total. No single density does
both, so either the engine's fill is not volume-proportional at all, or one of
the two anchors is wrong. The rule now has the right *shape*; the constant is
pinned to the local, shipped evidence (the bots' own behaviour) rather than to the
community figure that contradicts it.

**The bots' weight class is the only oracle that ships with the game**, and it is
too coarse to settle it: each `.bot` declares `Class:` (light **< 250 kg**, medium
**250–400**, heavy **> 400**), and **35–37 of the 45 stock bots are inside their
declared class at any density between 0 and 139** — so it neither confirms nor
refutes a value, but it does say the masses are in the right country.

**A side-finding, and a test fix that came with it.** Two tests
(`a_ram_is_a_blow_and_resting_contact_does_nothing`, `contact_spans_a_robots_own_bodies`)
drove two bots at each other from the arena's own start points **25 m apart**, and
a 0.05 % change in total mass — 200.1 kg to 200.0 — was enough to make the
trajectories miss entirely: they drove fine (2.7 m/s) and passed each other 2.3 m
wide. A long head-on approach is a chaotic trajectory, so those tests were
measuring luck. They now spawn 6 m apart facing each other (`BotSetup.spawn`),
which is what they were actually about, and hold whatever the masses are.

---

## F13 — The script layer (M4)

**Landed: the arena half.** The game's own `Arenas/` package runs, unmodified, on
an embedded CPython 3.12.

*Robot Arena 2*'s game logic is Python 2.2 talking to a C++ engine through a
module called `plus`. Almost every behaviour still missing — hazards that fire,
pistons that stroke, the AI that actually fights — lives in *those* scripts, so
`ra2script` boots a real interpreter and runs them as written rather than
paraphrasing them:

- **The interpreter is the host's CPython 3**, exposed to Rust through `pyo3`
  (`build.rs` supplies the `libpythonX.Y.so` symlink several distributions omit,
  so no `-dev` package is required).
- **The Python-2 sources are translated on load** by `legacy.py`: `print`
  statements, `raise E, msg`, `except E, e`, `<>`, `.iteritems()`/`.next()`,
  `file()`, `xrange()` — and Python 2's *implicit relative imports*, which the
  game relies on (`import Hazards` inside `Arenas` means `Arenas.Hazards`; without
  this, every arena fails to import). It also reinstates the 2.2 library functions
  the scripts call (`string.split`, `string.replace`, `types.TupleType`, …). Only
  the game's own directories are translated; the shipped `Scripts/Lib` is a copy
  of the Python 2.2 standard library and is deliberately ignored in favour of the
  host's. The one semantic *not* translated is `/` on two ints (truncating in 2.x,
  exact in 3.x); the shipped scripts were surveyed and none depends on it.
- **The engine side is `plus.Arena`**, with exactly the method surface the
  scripts call (surveyed, not guessed): the navigation map
  (`SetMapSize`/`SetMapResolution`/`SetMapAltitude`/`SetMapHeight`/`SetMapWall`/
  `SetMapHazard`/`SetMapExtent` and the `Get`/`Is`/`ToGrid`/`FromGrid`,
  `GetPath`, `IsStraightPathClear`, `SetSearchRadius`, `DistanceToEdge`,
  `HeadingAwayFromEdge`), the geometry (`RayTest`, `GetStartPointLocation`,
  `AddShadowTriangle`, `SetShadowSource`), the hazards (`AddPrismatic`,
  `GetHinge`, `RegisterZone`, `AddCollisionLine`, `AddPOV`, `SetSubMaterial*`,
  `SetActive`, `SetPinned`) and lightning.
- **`AddShadowReceivers` is the engine's call**, not the script's — each arena
  fills in a hook and the shadow source and receiver triangles only reach the
  engine when we call it. **`AddStaticCamera`, `AddWatchCamera` and `AddHazard`
  are the Python base class's**, not ours: `Arenas.SuperArena` keeps the cameras
  in `self.StaticCams` and the hazards in `self.hazards` and drives them through
  the free functions, so binding them in Rust would shadow the game's own
  behaviour.
- **The navigation map is the game's own data.** Arenas ship a baked `.map`
  beside their model and `SuperArena.LoadMap` reads it; `GetPath` is A* over that
  grid (8-connected, no corner cutting, `SetSearchRadius` as clearance) and
  `AddCollisionLine` walls it around each hazard — the mechanism the scripts use
  to keep the AI off the spikes.

**Landed: the AI half, and it changes the game.** `AI.SuperAI` + `AI/Tactics` +
`Bindings.py` + the per-bot brains all run, and the robots now close to contact
and trade blows where the stand-in driver only orbited and pinned itself against
a wall. Eleven brains register and the table picks them by the robot's own
`Name:` line (`Mud Runner` → `Chopper`, `Tornado` → `Spinner`, `CatFish` →
`Rammer`); a bot with no entry keeps the stand-in.

Three things had to be got right, and each was a real bug first:

- **The brains run on the game's cadence.** `AI.SuperAI.Tick` refreshes its
  sensors after 8 of its own ticks "once per second", and `AimToHeading` scales a
  turning rate by `self.tickInterval` — so a brain tick is **0.125 s** (and
  `Hazards.Flame`'s four-quarter counting puts the arena's `Tick` at **0.25 s**).
  Physics steps between them and the controls hold. Running the brains every
  physics step instead changes the strategies, because `countdownToEvaluation`
  counts ticks.
- **The heading convention.** The scripts build a direction from an angle with
  `vector3(a) = (sin a, 0, cos a)`, so `GetHeading` is `atan2(x, z)` of the
  facing, `GetHeadingTo` is the *error* (`AimToHeading` takes it directly), and
  `GetHeading(reverse)` is the same for the reversed facing.
- **The steer sign.** `Turn(t)` is positive for a positive heading error, while
  `Robot::drive` applies `yaw = -steer * rate`; the drive has to negate. Getting
  it wrong makes the robots drive *away* from each other.

Two Python-2 constructs surfaced here that the first survey missed and the
translator now handles: `xs.sort(cmpfn)` (Python 2's positional comparison
function, rewritten to `sort(key=cmp_to_key(cmpfn))` with `cmp_to_key` shimmed
into builtins) and the `tickInterval` attribute the engine owes a brain.
`AI/*.py`'s `import Gooey` is served by a stub, because the shipped
`Scripts/Gooey.py` reaches the engine's UI module `giScript`, which is M5.

**Landed: the hazards half, and the spikes fire.** The scripts' hazards now
reach the world. The seam is *data*, as everywhere else: a script asks for a
joint, the engine records the request with the live settings object the script
will go on driving, and the simulator builds the joint at the top of the next
step — by which time every hazard's `__init__` has run and its power settings are
in place.

- **The joints are the scripts' own.** `AddPrismatic('arena_collision',
  'spikes1', 0, 1, 0, 0, .8, 0)` becomes a real prismatic joint with the script's
  travel. The scripts write the limits *unordered* (`0.8` then `0` — `neutral`
  first), so the engine takes the min and the max relative to the neutral pose:
  the spikes get `[0, 0.80]` and the saw `[0, 0.56]`, both exactly what
  `Arenas/box.py` asks for.
- **The model's hinges** were being thrown away: `gmi` skipped the solver's
  constraint list entirely. It now reads it — name, both bodies, pivot, axis,
  `IS_LIMITED`, `FRICTION`, `ANGLE_LIMITS` — taking the pivot from the node's
  `TM_ROW3` and the axis from `TM_ROW1`, which is exactly how the exported `.gmf`
  spells out `*POINT`/`*SPIN_AXIS`. `GetHinge('Hinge01')` is now a real hinge, and
  the Compressor Arena's four smash arms come back on the model's own axes, limited
  to the model's own `0.00 .. 0.27 rad`. The *travel* is read from the model's own
  lower stop, because Jolt measures a hinge from where its frames coincide and
  insists on `-π ≤ min ≤ 0 ≤ max ≤ π` while the models state their limits in their
  own frame with the two sides exported at that stop — the Octagon's hellraiser
  arms ship `[1°, 70°]`, which Jolt refuses outright. (The script layer changes the process's
  working directory to the game root, so the arena path is made absolute at the top
  of `Duel::new` — otherwise the *model* the script names can no longer be found
  from the relative path the runner was given, and every `GetHinge` silently
  failed.)
- **The zones** are the model's own trigger volumes, resolved by name against the
  arena's geometry. Each step the engine tests every robot's components against
  them and calls `ZoneEvent(direction, id, robot, chassis)` on entry and exit —
  `robot` counting from one, the way `Hazards.Electricity` and `AI.SuperAI` both
  immediately do `robot - 1`.
- **The arena's `Tick`** runs at its own quarter-second cadence, so the hazards'
  timers advance. Ticking the hazards themselves is the *scripts'* job and needs
  nothing from us: `BoxArena.Tick` calls `self.spikes1.Tick()`.
- **The drive is the script's own numbers.** `Lock(True)` leaves the motor off,
  which is how a joint parks — its friction holds it, the way a parked piston does
  (F8); `SetPowerSettings` + `SetDirection` drive at the script's speed, with
  twice the script's torque (×40, F3) as the cap; `SetDirection` is read as the
  sign selector it is (`PitSlider` passes `1`, `Hammer` `±100`); and
  `ApplyForce`/`ApplyTorque` are one-shot impulses along the joint's own axis,
  which is what `Spikes.FireTeeth`'s single `ApplyForce(40000)` is.
- **`plus.force` is a force, and it is the *same* operation as a joint's
  `ApplyForce`.** The engine adds the vector into the body's force accumulator
  (`FUN_0045e940`, reached through `FUN_004b0950`), and the joint's `ApplyForce`
  calls that very function with `axis * power` — so the two are one call, one
  unit. That settles the reading that had been a guess: the impulse a step is
  `force * dt`, which is what makes `Hazards.Electricity`'s `115 * getWeight(bot)`
  sane — 115 N per kilogram is ~11.7 g held for one step, about 1.9 m/s of lift
  per zap, where an impulse reading gave it 115 m/s. The impulse lands on the
  chassis, and the original's robot is a *single* rigid body, so it is scaled by
  the chassis's share of the robot's mass to leave the robot's own acceleration
  alone.
- **`SetAutoLock` holds a joint the script has not set moving.** Recovered from
  the joint's own per-step update: it locks the joint when its motor is stopped
  (`direction == 0`) and again when the joint arrives at the stop it is driving
  toward. `SetAutoLock(x)` on a slider is one flag covering both; a hinge's
  `SetAutoLocks(at_stops, when_idle)` is those two halves separately — the
  Compressor's smash arms grip at whichever stop they reach (`True, False`) while
  the Clawtop claws only hold once their motor stops (`False, True`). The shipped
  scripts already set it explicitly wherever it matters, and the joint is *free*
  by default: the Octagon's start blocks are `PitSlider`s whose floors would
  otherwise drop out from under four robots at the bell, and `SetAutoLock(True)` is
  the one thing `PitSlider.__init__` asks for before anything else.
- **The strength is re-stated, not fixed at build time.** A hazard asks for its
  power whenever it likes: `Hammer.__init__` parks its arm with `Lock(True)` and
  *no* power settings at all, and the 1200 N·m only arrives in `Smash`, when a
  robot walks into the zone. The engine therefore pushes the script's torque (and
  the friction that holds a parked joint) to the constraint whenever it changes,
  not only when the joint is built. The scripts also ask for drive speeds past
  what a solver can express — `SetPowerSettings(95, 1200)` is 15 turns a second —
  and the backend *refuses* such a target rather than clamping it, which silently
  left the hammer standing still; the target is clamped to the solver's limit
  instead. Every hinge that asks for that speed has angle limits a few degrees
  wide, so the arm reaches its stop within a step either way.
- **`Arena.SetActive`/`SetPinned`** are the arena's own switches, and the scripts
  state a *want* rather than an event (`parkinglot.py` hides all 25 cinder blocks
  on the way in and drops them in when a trap door opens; `flextop.py` pins its
  flexing floor). The engine reads those wishes every step and reconciles the
  world against them: a hidden prop's body is removed and rebuilt from its own
  `.gmf` definition when asked for, and a pinned one is made static where it is.
- **`plus.rayTest`** — the flame's "did the jet reach a part" — now answers
  against each component's oriented box rather than the sphere around it. A part
  is rarely a ball, and a flame half a metre off a spinner's blades should not set
  them alight.

Six real bugs came out of this, and all six were in the engine rather than the
scripts:

- **A shared collision proxy was being re-placed per body.** The Combat Arena has
  one `spike_collision` object, sitting at spikes1's position, named as the
  collision proxy by *both* spike bodies — so placing it absolutely put spikes2's
  collider 8.6 m away at spikes1's, and Jolt rejected any joint on it. Geometry is
  now placed relative to the proxy it hangs under, which is what a reused proxy
  means.
- **Impulses were ignored on sleeping bodies.** Jolt parks a settled body and drops
  an impulse into one — the same gotcha `set_velocity` already had. A hazard pushes
  a part that has not moved since the match began.
- **A hinge's angle limits did not contain zero.** Jolt requires the range to
  span the frames' meeting point, and the models do not state theirs that way: the
  Octagon's hellraiser arms are `[1°, 70°]`, so the constraint was refused and both
  arms were simply not there.
- **The arena was never told the match had begun.** `Arena.Activate(on)` is the
  arenas' own start-of-match hook and the engine is what calls it; it was an empty
  stub. That is not cosmetic: the Octagon and the Skull only open their trap floors
  from `PitSlider.setactive(True)` inside `Activate`, so those floors never moved
  at all. King of the Hill reads the game type there, and the base class retires
  its intro generator.
- **The scripts' requests were serviced on one code path only.** `plus.damage`,
  `plus.force`, `plus.zap` and `eliminatePlayer` were drained in `Duel::step`,
  which the game's own AI drives — so a hazard that hurt a robot reached it in a
  match but not when a test drove the robots directly. Servicing belongs to the
  step itself and now happens inside `advance`.
- **A drive speed past the solver's limit was silently dropped.** The Half Circle
  Arena's hammer asks for 95 rad/s; the constraint refused it and the arm did not
  move at all.
- **The arena path went stale after the script layer's `chdir`** (above).

**Still open, in order:**

1. **Presentation** — audio, camera motion, particles and the Gooey UI — is
   accepted and *recorded* (a run reports what it dropped) and belongs to M5.
   `plus.zap`'s visual is the same. `AddHazard`'s locations feed the AI's hazard
   avoidance through the Python base class, so nothing is owed there.

**Verified:** the hazards are pinned end to end. Standing a robot in
`spike_zone2` fires the hazard: the zone event reaches `BoxArena.ZoneEvent`, the
script's own `Tick` counts its two quarter-seconds, `Spikes.FireTeeth` unlocks the
prismatic the script created, and the spike body shoots a fifth of a metre up out
of the floor with the robot standing on it (0.8 m, its whole travel, when nothing
is — `a_slider_lets_its_body_rise`). The Compressor Arena's four smash arms swing
under `Hazards.Smasher`'s own `SetPowerSettings`/`SetDirection`
(`the_models_own_hinges_are_driven_by_the_script`; all four Octagon hinges build
too), the Octagon's four trap floors stay shut under the robots and only let go
ninety arena ticks later (`the_engine_tells_the_arena_the_match_has_begun`,
`a_pit_floor_is_held_until_its_script_opens_it`), a `plus.force` of 115 N/kg lifts
a robot 1.9 m/s in one step (`plus_force_is_a_force_and_not_an_impulse`), the Half
Circle Arena's hammer swings on the power it only asks for once a robot is in its
zone
(`a_hammer_that_asks_for_power_late_still_swings`), and a Parking Lot cinder block
leaves the world, is pinned mid-air and drops when unpinned
(`the_arena_can_hide_a_body_and_pin_one`). The box arena's three joints (two spikes
and the saw) all build with the script's own travel. The shipped AI closes the
fight — the test drives Roly Poly against
Little Metal Friend in the Combat Arena for 30 s at the engine's own 1/60 step and
requires the closest gap to go under a metre and damage to land (it reaches contact
by t≈8.5 s and both take hits; on the vendored-CMake Jolt build the first hit
lands at t≈20.23 s, which is why the window is 30 s and not 20); all fourteen
stock arenas register from their own
modules;
`Arenas/box.py`'s configuration arrives intact — 3 cameras, `Some([0.653, 6.894,
-0.418])` and 10 shadow triangles, a 30×30 grid at 1 m with 207 walls and 24
hazard cells, and after `HazardsOn(True)` the saw and two spike zones with ids 1,
2 and 3; `RayTest` finds the floor under start point 0 and `GetPath` routes from
it to the arena centre; and the translation is pinned against the constructs the
survey found, including that text inside a string is left alone.
