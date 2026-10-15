# 06 — Deferred fidelity work

Things we know diverge from the original, kept so they can be picked up later
without redoing the research. Ordered by how much they block correct behaviour.
Evidence for each lives in [`05-validation-report.md`](05-validation-report.md);
the primary sources are the game's `*_txt.gmf` ASCII physics exports,
`Armor_Definitions.txt` (in `RA2.cfz`), the official component-exporter
templates, `tools/vendor/PyRA2`, and the binary itself (Havok Hardcore 1.7.1).

| Ref | Item | Status |
|---|---|---|
| F1 | Mount-point (attachment frame) alignment | data done, rule identified (`A_parent·A_own⁻¹`); the spinner's height source is open |
| F2 | Assembled-bodies path is the only robot path | **landed** |
| F3 | The motor law, from the game's own data | **landed** (speed is rad/s, torque x40 — both from the binary) |
| F4 | Wheel `contact` / `grip` / `resistance` | **landed** (`contact` is the material tag, not a collider) |
| F5 | Collision shape kinds (`SIMULATION_GEOMETRY`, `HAVOK_GEO_TYPE`) | **landed** |
| F6 | Friction combination law differs from Havok 1.x | blocked on the binding |
| F7 | Damage model: an event, not a per-step integral | shape and hitpoints landed; the impact formula is still calibrated |
| F8 | Pistons slide their children (prismatic) | **landed** (nothing drives them yet) |
| F9 | The chassis buckles (crush points from the `.bot`) | **landed** (rate inferred) |
| F10 | Chassis skin decals; 16 %-light weight gap | **landed** (weight: see F12) |
| F11 | The Octagon's "hole" was a start block | diagnosed; spawns now use the arena's start points |
| F12 | Component mass when the `.gmf` stores zero | identified (wheels); rule open |
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

## F6 — Friction combination law differs (blocked on the binding)

Havok 1.x's own era reference states contact friction is `sqrt(µ₁² + µ₂²)`; Jolt
combines as `sqrt(µ₁ · µ₂)`, and the safe `oxijolt` API exposes no combine-mode
hook. On the octagon (floor 0.80) with a 0.30 robot that is 0.854 vs 0.490 —
about 43 % less grip. Only raw FFI or a custom combine would close it.

## F7 — Damage is an event; its numbers are half-sourced

**Landed** (this round): the *shape* of the model, and two real defects.

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
  (`AI/__init__.py`: `if weapon == damage[0] and damage[1] > 10.0: # min damage
  = 10`). Our model *integrated* an instantaneous speed over every step a contact
  lasted, so a fast impact contributed almost nothing (a 5.6 m/s head-on ram:
  3 points) while two motionless robots eroded each other. Now the closing speed
  the robots arrive with is dealt **once**, when contact begins, and a spinning
  weapon's rotor tip speed is applied as a rate while it works the same joint.
  A constant term (`speed_floor`) that made resting contact damage is gone.
- Measured on a deterministic head-on ram (`RA2_DUEL_DRIVE=ram`, two identical
  `AI/Team1/Bot0` on the box arena): the opening blow is **34** points, then
  resting contact adds ~1 point a second. That is the range the original's own
  scripts describe — above the AI's 10-point "good hit" line and around the
  hazard's flat 25. `damage_scale` is 6.0 for that (it was 22.0, tuned to the old
  per-step integral).

**Still open**, and now precisely scoped:

- The formula's numeric core. `concussion + piercing` is read from the component
  `.txt` (zero to 1.3 across the stock weapons) and shot through `impact speed ×
  6`, which is calibrated to the anchors above, not *recovered*. The original's
  own function was not found: the binary is symbol-free, its damage arrives as a
  `giBotDamageMessage` (no vtable xrefs) and the arena's script `damage` simply
  posts one, so there is no straight line from `GetLastDamageReceived`'s thunk to
  the computation. Community measurement is `D = 183.9·P + 100·C` for a reference
  setup ([CONFIRMED, empirical], `docs/04` §5.2) — useful as a cross-check, not a
  source.
- **Component hitpoints — recovered.** The component definitions declare
  `hitpoints =` for only 9 of 131 parts; every other part's maximum comes from the
  engine's own table, which the component factory (`FUN_0045cf40`) applies by
  switching on the definition's **`type`** category — power **400**, extenders
  **100**, mechanics **400**, mobility **400**, weapons **1000**, extras **1000** —
  after which a declared `hitpoints =` overrides it. The accessors are
  `component + 0xe8` (maximum) and `+0xec` (current); damage is a plain
  subtraction (`FUN_0044f370`), so the model is now on the original's numbers
  instead of our invented 250/300/350/600. The `.txt` parser that reads the keys
  is `FUN_004622f0` (`hitpoints` → `+0x98` on the *definition*, `type` → `+0x24`),
  and `FUN_0049aa30` reads the category back.
- **`fracture` and `material` are unparsed.** `fracture` is declared by 8
  components (discs and the flipper 20, the ramplate 40) and `material = metal(.4)`
  / `rubber(0)` by 5. The community note that a chassis `fracture` of **41** is
  "how much you have to deal to the control board" fits a reading where
  `fracture` is a per-hit *reduction*: hazard hits are 25, so they can never
  dent a ramplate (40) or reach the control board behind a chassis (41) — which
  matches how those parts behave in play. That reading is [LIKELY], not decoded,
  and it needs the per-hit event shape above to be expressible at all.
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
  - **down and in**, linear in the damage the chassis has taken, reaching half
    its height and half its footprint as its last hitpoint goes.

  The direction is the format's; the *rate* is ours and marked as such.

Both the drawn chassis and **its collider** follow (the chassis body's shape is
re-baked), so a hurt robot is visibly crushed *and* smaller to hit — before this
a chassis only ever died whole. Only the chassis has a buckle model; parts still
detach.

Verified: a test pins that the movable region's footprint and top shrink while
the floor plate stays put, and that the crush is monotone; `RA2_BUCKLE=<0..1>` on
`ra2-duel` renders a chassis crushed to any fraction — two otherwise-identical
`AI/Team1/Bot0` at 0.0 and 0.8 side by side show only the chassis change (wheels
and floor plate untouched); and the 45-bot survey is **bit-for-bit unchanged**,
because an undamaged robot never buckles.

Still open: the crush is smooth rather than faceted (the game's subdivided model
is what would give it the original's silhouette), and the game *writes the
crushed model back* on save, which we do not.

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

**Discovered while closing F7's hitpoints; it very likely explains F10's 16 %.**

Every stock wheel's rigid body stores `mass = 0` — `fatboy_wheel1`, `miniwheel`,
`mudtire`, `shinywheel1`, `wheel1`, `wheel2`, `wheel3`, `wheel_slim` — as do
`slipperbottom` and `cannon_txt`. PyRA2's own note is that "a body with no mass
(or an explicit `UNYIELDING`) never moves", so the engine has to be giving these
their mass somewhere. **Ours does not:** `ra2sim::rig` sums the scene's rigid-body
masses and falls back to an invented **8.0 kg** when the sum is zero.

That is a better explanation of F10's weight gap than the armour reading was. Mud
Runner carries four `mudtire` wheels; at 8 kg each they are 32 kg of its 360.6 kg,
and the documented 418.6 kg wants **58 kg more — exactly 22.5 kg per wheel**,
where the "different armour" reading (steel) overshot by 2.4 kg. Wheels are also
the only stock family with a zero mass, which is the kind of single-cause effect
a 16 % gap should have.

What is missing is the rule itself. The candidates are a mass computed from the
collision geometry (`volume × density`, Havok's idiom for a zero-mass body) or a
value from the definition — the factory `FUN_0045cf40` does read one definition
number into `*(component + 0x64) + 4`, but no shipped definition or exporter
template declares `mass` for a wheel, so that field is 0 there. **[OPEN]**

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

Five real bugs came out of this, and all five were in the engine rather than the
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
- **A drive speed past the solver's limit was silently dropped.** The Half Circle
  Arena's hammer asks for 95 rad/s; the constraint refused it and the arm did not
  move at all.
- **The arena path went stale after the script layer's `chdir`** (above).

**Still open, in order:**

1. **Presentation** — audio, camera motion, particles and the Gooey UI — is
   accepted and *recorded* (a run reports what it dropped) and belongs to M5.
   `plus.zap`'s visual is the same. `AddHazard`'s locations feed the AI's hazard
   avoidance through the Python base class, so nothing is owed there.
2. **`plus.force`'s unit is a reading, not a recovery.** It is applied as an
   impulse in N·s, which is what `force(bot, x, y, z)` most plainly means, but its
   one caller — `Hazards.Electricity`'s `115 * getWeight(bot)` lift — comes out a
   hundred times too strong for a 200 kg robot at that reading. Either `getWeight`
   is not in kilograms or the original scales the argument; the Electric Arena is
   the only place that could settle it, so its zap is the single hazard figure
   that is unverified. Everything else a hazard does is a number the script states
   itself.
3. **`SetAutoLock` is accepted and has no separate effect.** It says whether a
   joint locks itself at its stop; Jolt's limits stop it either way, and the
   scripts' own `Lock`/`SetDirection` do the rest, so both settings behave alike.

**Verified:** the hazards are pinned end to end. Standing a robot in
`spike_zone2` fires the hazard: the zone event reaches `BoxArena.ZoneEvent`, the
script's own `Tick` counts its two quarter-seconds, `Spikes.FireTeeth` unlocks the
prismatic the script created, and the spike body shoots a fifth of a metre up out
of the floor with the robot standing on it (0.8 m, its whole travel, when nothing
is — `a_slider_lets_its_body_rise`). The Compressor Arena's four smash arms swing
under `Hazards.Smasher`'s own `SetPowerSettings`/`SetDirection`
(`the_models_own_hinges_are_driven_by_the_script`; all four Octagon hinges build
too), the Half Circle Arena's hammer
swings on the power it only asks for once a robot is in its zone
(`a_hammer_that_asks_for_power_late_still_swings`), and a Parking Lot cinder block
leaves the world, is pinned mid-air and drops when unpinned
(`the_arena_can_hide_a_body_and_pin_one`). The box arena's three joints (two spikes
and the saw) all build with the script's own travel. The shipped AI closes the
fight — the test drives Roly Poly against
Little Metal Friend in the Combat Arena for 20 s at the engine's own 1/60 step and
requires the closest gap to go under a metre and damage to land (it reaches contact
by t≈8.5 s and both take hits); all fourteen stock arenas register from their own
modules;
`Arenas/box.py`'s configuration arrives intact — 3 cameras, `Some([0.653, 6.894,
-0.418])` and 10 shadow triangles, a 30×30 grid at 1 m with 207 walls and 24
hazard cells, and after `HazardsOn(True)` the saw and two spike zones with ids 1,
2 and 3; `RayTest` finds the floor under start point 0 and `GetPath` routes from
it to the arena centre; and the translation is pinned against the constructs the
survey found, including that text inside a string is left alone.
