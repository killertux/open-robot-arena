# 04 — Havok 1.7 ("Havok Hardcore") technical research

Research note answering the Havok 1.7 / RA2 physics questions. Compiled 2026-10-14.

**Evidence sources used**

- The actual RA2 executable at `local/game/Robot Arena 2.exe` (sha256 in
  `02-recon-findings.md`), mined with `strings` for RTTI, build paths, version
  banners and the GMF-exporter token set. This is the **primary, authoritative
  evidence for what RA2 links**.
- The shipped `.gmf`/`_txt.gmf` assets and `PyRA2` (`tools/vendor/PyRA2`) — the
  authoritative view of the RA2 physics data model.
- Public Havok docs: the **Havok 1.7 Game Dynamics SDK product overview (©2002
  Telekinesys)** and the **Havok Xtra Lingo Reference Guide (©2000/2001
  Havok.com)** — the closest surviving 1.x-era primary documents.
- GDC/GDM June 2003 review of Havok 2.0 (which discusses 1.7/1.8).
- Community docs: RA2Wiki, beetlebros, GameFAQs, namu.wiki.

**Confidence key**

- **[CONFIRMED]** — direct primary evidence (binary, shipped asset, or a Havok
  document of the same era), or multiple independent sources.
- **[LIKELY]** — strong secondary/analogous evidence, not directly attested.
- **[INFERRED]** — reasoned from adjacent evidence; treat as a hypothesis.
- **[UNKNOWN]** — could not find evidence.

---

## 1. What "Havok Hardcore" was, and the 1.x line

### 1.1 Product identity — [CONFIRMED]

"Havok Hardcore" was a **rigid-body-only SKU** of Havok's game SDK, launched
**16 February 2001**, aimed at the cheaper end of the market. It is the
same core engine as the full "Havok game SDK", minus the deformable modules.

> "Havok has released a new rigid body physics technology aimed at the extreme
> game development market. Havok Hardcore is a game SDK offering rigid bodies,
> constraints, cars and rag dolls with a full range of support tools, demos and
> documentation. … The full Havok game SDK includes all of the Havok Hardcore
> functionality and additionally offers fully integrated soft bodies, cloth,
> particles, rope and fluids. Havok Hardcore is priced at $40,000 for the first
> title per platform, while the Havok SDK is priced at $75,000."
> — gamedeveloper.com (Gamasutra), 2001-02-16
> <https://www.gamedeveloper.com/game-platforms/havok-releases-havok-hardcore>

> "Havok has released Havok Hardcore, a new 'rigid body physics technology' game
> SDK that leaves out certain features (such as soft bodies and cloth) but costs
> almost half the price of the full SDK."
> — CGPress, 2001-02-22
> <https://cgpress.org/archives/industry_news_havok_releases_new_product_unveils_expansion_plans.html>

By the release of Havok 1.7 the branding had consolidated into the **"Havok
Game Dynamics SDK"**, whose 1.7 edition is explicitly documented:

> "The Havok 1.7 Game Dynamics SDK is available for PlayStation 2, GameCube,
> Xbox and PC and supports all commonly used compilers (e.g. Codewarrior,
> ProDG)."
> — *Havok Game Dynamics SDK* product overview, p.3 (© 2002 Telekinesys Research
> Ltd.) <https://cgl.ethz.ch/teaching/former/seminar02/papers/Havok_Overview.pdf>

The binary's own banner is the "Hardcore" one, so RA2's 1.7.1 is a Hardcore
build (not the full deformable SDK):

> `---- Havok Hardcore - Version(%d) Build(%d) ----`
> `** Havok libs built with version [%d], used with code built with [%d]. **`
> — strings in `local/game/Robot Arena 2.exe` (also seen in other Havok titles,
> e.g. <https://tcrf.tiago.zip/wiki/Rock_'n'_Roll_Adventures>)

### 1.2 Version timeline of the 1.x line

| Version | Evidence | Confidence |
|---|---|---|
| Havok 1.x engine in shipping games from ~1999–2000 | Wikipedia / Havok game-history material ("Evolution of Havok Physics Engine Games 1999–2018") | [LIKELY] — exact 1.0/1.1/1.2 dates not found |
| **Havok Hardcore / 1.x SKU launched Feb 2001** | gamedeveloper.com, CGPress (above) | [CONFIRMED] |
| **Havok 1.7 Game Dynamics SDK, ©2002** | product overview PDF | [CONFIRMED] (edition exists; exact ship date not given) |
| **Havok 1.7.1** | RA2 binary path `…\code\**hk171**\havok\source\hkdynamics\world\world.cpp` and `…\hkbase\debugutil\hkDefaultMonitorFactory.cpp` | [CONFIRMED] that a build tagged 1.7.1 was statically linked into RA2 |
| **Havok 1.8** | GDC/GDM June 2003 review: "prior experience with the 1.8 SDK" | [CONFIRMED] that 1.8 existed |
| **Havok 2.0** ("not an upgrade, a whole new product", islands, character control) | same GDC/GDM June 2003 review | [CONFIRMED] |
| exact dates for 1.0–1.6, 1.7.2+ | not found | [UNKNOWN] |

The GDC June 2003 product review is useful because it names three eras at once:

> "in the past with Havok 1.7 I could bring my 3GHz, 512MB, 64MB Radeon 9000 to
> its knees with a half-dozen rag dolls."
> "From prior experience with the 1.8 SDK on a real project, technical queries
> submitted late in the afternoon would elicit a response …"
> "New and improved. In the original Havok we heard a lot about dynamic
> characters — rag doll — but it was more akin to having a dynamic corpse. Havok
> 2.0 extends what is possible …"
> — *Game Developer* magazine, June 2003, "Havok 2: All Rag-Dolled Up" (Justin
> Lloyd) <https://media.gdcvault.com/GD_Mag_Archives/GDM_June_2003.pdf>

**"Havok Hardcore" vs "Havok 1.7":** they are not competing names. "Hardcore"
is a *SKU*/edition line; the numeric 1.x is the SDK release. RA2 ships the
rigid-body "Hardcore" build of the 1.7.1 SDK.

### 1.3 API style and namespaces — [CONFIRMED] (from the binary)

The 1.7.1 API is a C++ class library, and it is **mid-migration between two
naming schemes**, which is exactly what RA2's RTTI shows:

- A `Havok::` namespace for the older/public-facing classes:
  `Havok::HavokObject`, `Havok::ReferenceObject`, `Havok::Constraint`,
  `Havok::Action`, `Havok::Event`, `Havok::EventCallback`,
  `Havok::IntersectionSolver`, `Havok::BaseIntersectionSolver`,
  `Havok::Segment`, `Havok::SegmentTest`, `Havok::ClosestHitSegmentTest`,
  `Havok::SimpleHit`, `Havok::StepAction`, `Havok::RigidBodyMonitor`.
- The later `hk`-prefixed global classes: `hkBaseObject` (and the source tree
  `hkbase/`, `hkdynamics/world/`).

Exact RTTI strings recovered from `Robot Arena 2.exe`:

```
.?AVAction@Havok@@
.?AVBaseIntersectionSolver@Havok@@
.?AVClosestHitSegmentTest@Havok@@
.?AVConstraint@Havok@@
.?AVEventCallback@Havok@@
.?AVEvent@Havok@@
.?AVHavokObject@Havok@@
.?AVhkBaseObject@@
.?AVIntersectionSolver@Havok@@
.?AVReferenceObject@Havok@@
.?AVRigidBodyMonitor@Havok@@
.?AVSegment@Havok@@
.?AVSegmentTest@Havok@@
.?AVSimpleHit@Havok@@
.?AVStepAction@Havok@@
```

So: **yes** — the questioner's list is right about `Havok::HavokObject`,
`ReferenceObject` and `hkBaseObject`. `hkRigidBody`, `hkWorld`, `hkShape` and
`hkConstraintSolver` are attested not by RTTI but by:
- the build path `hkdynamics/world/world.cpp` [CONFIRMED];
- the GMF physics objects/constraint kinds (below) [CONFIRMED];
- the 1.x-era Xtra API referring to `hkRigidBody` and the world properties
  [CONFIRMED].

The exact class names `hkRigidBody` / `hkWorld` / `hkShape` *as C++ identifiers*
are [LIKELY] (very strongly implied; not literally in the RTTI of this stripped
build). `hkRigidBodyCinfo` is a **2.x-and-later** name — see §4.

`hkMemoryClass` enum entries recovered from the binary (these are exactly the
`HK_MEMORY_CLASS_*` the questioner mentioned):

```
BASE MATH GEOMETRY RIGID_BODY BROAD_PHASE COLLISION_AGENT CONSTRAINT VEHICLE
ACTION ARRAY BV_INFO CDINFO DENSE_VECTOR DISPLAY DUMMY EFFECTOR ENTITY EVENT
EXPORT GENERIC MAX NEVER_USED SHAPE_COMPILER SINGLETON STL STRING UNKNOWN USER
```

---

## 2. Class/API reference for the 1.x line

> **Caveat.** Havok **never shipped source for the core** — the 1.7 overview
> states plainly: *"Source is NOT shipped for: Core actions (including
> constraints), Collision Detection, Collision Resolution, Entities, World, Math
> Library, Geometry Lib, Export Lib."* So the class reference below is
> reconstructed from (a) the shipped 3ds Max exporter's GMF field set (which is
> the 1.7 public API surface, as serialised by the plugin), (b) the 1.x-era Xtra
> property reference, (c) the RA2 binary. Where a member's *C++ name* is
> unverified it is marked.

### 2.1 Rigid body attributes

**Attested RA2 GMF `GMID_HAVOK_RIGIDBODY` fields** (the game's own exporter
output; [CONFIRMED] via `_txt.gmf` files and `PyRA2/GMIModel/Import.py`
`readRigidBody`). Field order in the binary GMF node is exactly:

```
NODE_NAME, MASS, ELASTICITY, FRICTION, OPTIMIZATION_LEVEL, UNYIELDING,
SIMULATION_GEOMETRY, GEOMETRY_PROXY_NAME, USE_DISPLAY_PROXY,
DISABLE_COLLISIONS, INACTIVE, DISPLAY_PROXY_NAME, NODE_TM, HAVOK_GEO_TYPE,
NUMBER_OF_CHILDREN
```

| Field | Meaning | Confidence |
|---|---|---|
| `MASS` | mass in **kg** | [CONFIRMED] (Xtra: "mass … heft of a rigid body (specified in kg.)") |
| `ELASTICITY` | restitution/bounciness (0 rigid … 1 perfect bounce) | [CONFIRMED] (Xtra `hkRigidBody.restitution`; beetlebros: "A high number means it is very elastic (will bounce)") |
| `FRICTION` | friction coefficient | [CONFIRMED] (Xtra `hkRigidBody.friction`) |
| `OPTIMIZATION_LEVEL` | exporter/collision-quality hint (0.5 in all stock parts) | [CONFIRMED] field exists; semantics [LIKELY] "collision tolerance / LOD" |
| `UNYIELDING` | 0 movable, 1 immovable/static | [CONFIRMED] (beetlebros: "0 means movable, 1 means it won't budge no matter what") |
| `SIMULATION_GEOMETRY` | which mesh is the collider: 1 convex, 2 concave, 3 proxy concave, 4 proxy convex, **5/6 display-proxy concave/convex** (5 in every stock arena's main collider) | [CONFIRMED] (RA2Wiki GMF; 5/6 inferred from the data — see below) |
| `GEOMETRY_PROXY_NAME` | collision-mesh object, used when SIM_GEOM is 3/4 | [CONFIRMED] |
| `USE_DISPLAY_PROXY` / `DISPLAY_PROXY_NAME` | the mesh the body is *shown* as; 5/6 simulate it too | [CONFIRMED] field; 5/6 semantics [LIKELY] |
| `DISABLE_COLLISIONS` | 0 collides, 1 passes through | [CONFIRMED] |
| `INACTIVE` | starts deactivated | [CONFIRMED] field |
| `HAVOK_GEO_TYPE` | `Standard` (mesh) or `Plane` (an infinite plane primitive placed by `NODE_TM`) — the only two in the shipped data (166 / 3) | [CONFIRMED] |

**Directly documented 1.x rigid-body properties** (Havok Xtra Lingo Reference,
©2000/2001 — the `hkrb` object maps onto `hkRigidBody`):

- `hkRigidBody.mass` — "specified in kg."
- `hkRigidBody.restitution` — "A restitution value of 1 gives a perfect bounce.
  A restitution value greater than 1 means a bouncing object gains energy after
  each collision."
- `hkRigidBody.friction` — "the coefficient of friction or stickiness …
  (0 = no friction, typical values are 0.8…1.0)."
- `hkRigidBody.active` — activate/deactivate.
- `hkRigidBody.pinned` — "A pinned object never moves when struck by another
  object … unlike an object initially created as fixed." (This is the
  pin/static mechanism; see §3.4.)
- `hkRigidBody.position`, `.rotation`, `.linearVelocity`, `.angularVelocity`,
  `.linearMomentum`, `.angularMomentum`, `.force`, `.torque`, `.centerOfMass`.
- Functions: `.applyForce`, `.applyForceAtPoint`, `.applyImpulse`,
  `.applyImpulseAtPoint`, `.applyTorque`, `.applyAngularImpulse`,
  `.attemptMoveTo`, `.interpolatingMoveTo`, `.correctorMoveTo`.
- **Corrector** sub-API: `.corrector.enabled`, `.corrector.threshold`,
  `.corrector.multiplier`, `.corrector.level` (levels include `HOLD`),
  `.corrector.maxTries`, `.corrector.maxDistance`. This is the "push a
  kinematic/aimed body toward a pose without tunneling" system — the ancestor of
  later character/keyframe correctors.
  <https://eclass.hmu.gr/modules/document/file.php/TP194/Havok%20Xtra%20Lingo%20Reference.pdf>

**Not found / inferred:** `damping` per body (the 1.x drag is a *world* property
— `INCLUDE_DRAG/LINEAR_DRAG/ANGULAR_DRAG` in the GMF sim object, and
`havok.dragParameters`); "optimization level" as a C++ setter name; an explicit
`hkRigidBodyCinfo` struct in 1.x.

### 2.2 `hkWorld`

Attested by the RA2 GMF `GMID_HAVOK_SIMOBJECT` block [CONFIRMED] and by the Xtra
[CONFIRMED]:

```
*GMID_HAVOK_SIMOBJECT
{
    *GRAVITY             0.000000 -9.809980 0.000000
    *WORLDSCALE          39.369999
    *SIMTOLERANCE        0.100000
    *RESOLVER            3
    *INCLUDE_DRAG        0
    *LINEAR_DRAG         0.000000
    *ANGULAR_DRAG        0.000000
    *INCLUDE_DEACTIVATOR 1
    *SHORTFREQ           5.000000
    *LONGFREQ            0.100000
    *USE_FAST_SUBSPACE   1
    *UPDATES_PER_TIMESTEP 1.000000
    *NUM_COLLISION_PAIRS 0
}
```

- **gravity** — `(0, -9.80998, 0)` m/s² (metric; [CONFIRMED]).
- **`RESOLVER`** — the **collision-resolution/friction model selector** (value 3
  in stock RA2). The 1.7 overview says there are several: *"There are a number
  of friction models provided with the Havok SDK, from simple to complex
  (handling both static and dynamic friction)."* [CONFIRMED] that a selector
  exists; the exact 1/2/3 semantics are [UNKNOWN].
- **`USE_FAST_SUBSPACE`** — the *"Fast Subspace"* cheap solver: *"A new collision
  response algorithm may be selected for cases where you want low CPU hit and
  are prepared to sacrifice some accuracy."* (1.7 overview). Enabled in RA2.
- **`UPDATES_PER_TIMESTEP`** (1.0) — number of solver/simulation updates per
  displayed timestep. This is the likely ancestor of the modern
  "substeps"/iterations notion. [CONFIRMED] field; numeric semantics [LIKELY].
- **`SIMTOLERANCE`** (0.1) — collision tolerance. [CONFIRMED] field; analogous to
  modern `m_collisionTolerance = 0.1`.
- **deactivation**: `INCLUDE_DEACTIVATOR`, `SHORTFREQ` (5 Hz), `LONGFREQ`
  (0.1 Hz). The Xtra documents these exactly: *"short-range frequency … is
  typically 1/20th of a second … The long-range test is more aggressive but acts
  over a longer time period (typically 10 seconds). … default short frequency is
  2 Hz, long frequency default is 0.1 Hz."* [CONFIRMED] — RA2 dials the short
  frequency up to 5.
- **timestep/substeps**: the Xtra world object exposes `havok.timeStep`,
  `havok.subSteps`, `havok.simTime`, `havok.gravity` [CONFIRMED existence].
  Concrete C++ names `setTimestep` / `setSubsteps` are [UNKNOWN] (not attested),
  though the concepts certainly exist.

**`SOLVER_TYPE`** appears inside the RA2 **RBCollection** header
(`*GMID_HAVOK_RBCOLLECTION … *SOLVER_TYPE 0`) [CONFIRMED field]. This is a
per-collection solver selection distinct from the world `RESOLVER`; its
enumeration is [UNKNOWN].

### 2.3 Collision pipeline

- `hkCollisionDispatcher` / broad phase / `hkConstraintSolver`: [LIKELY] as C++
  class names (they are the standard Havok names; and RA2's GMF has a
  `GMID_HAVOK_CONSTRAINTSOLVER` node). The 1.7 overview describes the
  subsystems (broad-phase, narrow-phase, collision response, "subspaces",
  constraint solving) [CONFIRMED at feature level]. The memory classes
  `BROAD_PHASE`, `COLLISION_AGENT`, `CONSTRAINT` [CONFIRMED] show these are
  separate modules. The literal identifiers `hkCollisionDispatcher` /
  `hkConstraintSolver` are not in RA2's RTTI → [LIKELY], not [CONFIRMED].
- The Xtra exposes `havok.disableCollision()` / `enableCollision()` /
  `disableAllCollisions()` / `enableAllCollisions()` and a `havok.collisionList`
  [CONFIRMED] — i.e. collision filtering is a first-class 1.x feature
  (matching the GMF's `DIS_COLLISION_PAIRS`).

### 2.4 Shapes and "simulation vs proxy/display geometry"

Shape set: the 1.7 overview lists supported collision geometry as **Spheres,
Planes, Convex objects, Concave objects, fixed polygon soups (landscapes),
heightfields** [CONFIRMED]. Named shape *classes* like `hkSphere`, `hkBox`,
`hkCylinder`, `hkCappedCylinder`, `hkConvexVertices`, `hkTriangleShape`,
`hkTransformShape` are **not** recovered from this binary; they are the standard
Havok shape vocabulary and appear in later SDKs [LIKELY for 1.x; CONFIRMED only
for the *geometry kinds*, not the identifiers]. Note the RA2 3ds Max exporter
does expose `*GMID_COLLISION_SPHERE/CYLINDER/BOX/MESH/PLANE/OBJECT` serializer
tokens [CONFIRMED] — the plugin could author primitive or mesh colliders.

The **decoupling of display and collision geometry** is an explicit, documented
1.7 feature:

> "Display proxies — Represent any object in the game by an arbitrary collision
> geometry. … by decoupling display and collision shapes, you can display
> something relatively complex while simulating it as a box or sphere."
> — 1.7 overview, "Collision Detection"

RA2 maps this directly onto its GMF:
`SIMULATION_GEOMETRY 1/2/3/4` (convex / concave / proxy-concave / proxy-convex),
`GEOMETRY_PROXY_NAME` (the `*_collision` mesh) and
`USE_DISPLAY_PROXY`/`DISPLAY_PROXY_NAME`. [CONFIRMED]

### 2.5 Constraints

The RA2 3ds Max exporter's constraint grammar is [CONFIRMED] from the binary's
own reader/writer strings, and these are the 1.x constraint types:

```
GMID_HAVOK_CONSTRAINT, GMID_HAVOK_CONSTRAINT_LIST, GMID_HAVOK_CONSTRAINTSOLVER
GMID_HAVOK_POINTTOPOINT   (PtToPt)
GMID_HAVOK_HINGE_CONSTRAINT
GMID_HAVOK_LIMITEDPTTOPT  (limited point-to-point)
GMID_HAVOK_PRISMATIC_CONSTRAINT
GMID_HAVOK_WHEEL_CONSTRAINT
GMID_HAVOK_RAGDOLL
GMID_HAVOK_SPRING
GMID_HAVOK_DASHPOT / LINEAR_DASHPOT / ANGULAR_DASHPOT
GMID_HAVOK_COLLISION_PAIR / DIS_COLLISION_PAIRS
(also present but unused by RA2: FASTCLOTH, FASTROPE, FASTSOFT + F*COLLECTION)
```

Fields actually parsed from stock RA2 GMFs (`PyRA2` + `_txt.gmf`) [CONFIRMED]:

- **Hinge** (`id 47`): name, TM, body1, body2, point, spin axis, `IS_LIMITED`,
  `FRICTION`, `ANGLE_LIMITS`.
- **Pivot** (`id 46`): point, spin axis, 8 unknowns (car-steering use).
- **Point-to-point** (`id 53`): point1, point2.
- **Angular dashpot** (`id 49`): body1, body2, `STRENGTH`, `DAMPING`,
  allow-interpenetrations, quad.
- Also present in the format: `*SPIN_AXIS`, `*ANGLE_LIMITS`, `*PARALLEL_LIMITS`,
  `*PERPENDICULAR_LIMITS`, `*TWIST_LIMITS` (ragdoll/limited), and the wheel
  constraint's `*SUSPENSION_AXIS`, `*SUSPENSION_FRICTION`, `*SUSPENSION_LIMITS`
  [CONFIRMED tokens in the binary's serializer].

> **Note on the questioner's constraint list:** `hkWheelConstraint`,
> `hkLimitedPointToPoint`, `hkPrismaticConstraint`, `hkRagdoll`, `hkHingeConstraint`,
> `hkPointToPoint`, `hkSpring`, `hkDashpot`/`hkLinearDashpot`/`hkAngularDashpot`
> are all attested here. `hkLimitedPointToPoint`/`hkPrismatic`/`hkRagdoll` are
> **defined by the format but I did not find them in the stock game GMFs** (only
> Hinge, Pivot, P2P and AngularDashpot appear in shipped components). The
> identifiers `hkHingeConstraint` etc. as literal C++ names are [LIKELY]; the
> *constraint kinds* are [CONFIRMED].

`SUSPENSION_FRICTION` specifically: this token belongs to the **wheel/car
constraint** serializer (`GM_HavokWheelConstraint`) and to the 3ds Max "CarWheel"
helper (`.suspensionFriction`), i.e. it is the **suspension/tire friction of the
vehicle model**, not a body-material friction. [CONFIRMED as a token; the RA2
*gameplay* use is [UNKNOWN] — I found no wheel constraint and no
`SUSPENSION_FRICTION` string in the 169 stock component GMFs I scanned, so in
stock RA2 it is likely export-tool surface area rather than used data.]

---

## 3. Collision-resolution semantics

### 3.1 How two friction coefficients are combined — the key finding

The **only surviving same-era statement** is in the Havok Xtra Lingo Reference
Guide (©2000/2001 Havok.com), and it is explicit. Under `hkRigidBody.friction`:

> "**Note** — For objects in contact you need to specify both friction parameters
> to get the desired result. **The friction used is the square root of the sum of
> the squares of the friction coefficients.**"
> — Havok Xtra Lingo Reference Guide, p.19
> <https://eclass.hmu.gr/modules/document/file.php/TP194/Havok%20Xtra%20Lingo%20Reference.pdf>

i.e. **μ_contact = √(μ₁² + μ₂²)**, *not* the average, *not* the geometric mean
√(μ₁·μ₂), *not* min/max. **[CONFIRMED as Havok's own 1.x-era documentation;
MEDIUM confidence as ground truth for the core engine]** — the Xtra is a wrapper
doc, the core source was never shipped, and the formula is unusual enough that
doc imprecision cannot be ruled out.

For contrast, the modern Havok plugin (Babylon.js, post-2019 engine) is
documented to *average* friction and take *max* restitution:

> "When two shapes with different material properties collide, Havok combines
> them using a formula that averages friction and takes the maximum restitution."
> <https://www.abratabia.com/game-physics/havok.php>

**This is not the 1.x behaviour** and should not be applied to RA2. [CONFIRMED
that the modern formula differs from the 1.x doc.]

### 3.2 Restitution/elasticity combination and the low-speed threshold

- **Combination formula for 1.x:** not documented in any source I found.
  [UNKNOWN]. Modern Havok uses **max**, and several engines use max — [LIKELY]
  but unverified for 1.x. The Xtra only defines a single body's restitution
  ("0 = no bounce … 1 = perfect … >1 = gains energy").
- **Restitution velocity threshold / low-speed bouncing damped:** Havok's later
  SDKs have `hkWorldCinfo::m_contactRestingVelocity` — *"The velocity which
  defines when two objects are assumed to be in resting contact … a special more
  accurate collision restitution algorithm is called … the value used in Havok
  3.1.0/3.1.1 was 0.2."* ([CONFIRMED for 3.x via the SDK header],
  <https://raw.githubusercontent.com/nitaigao/engine-showcase/master/etc/vendor/havok/Source/Physics/Dynamics/World/hkpWorldCinfo.h>).
  Whether 1.7.1 had an equivalent threshold, and its value, is **[UNKNOWN]**.
  RA2's `SIMTOLERANCE 0.1` is a *collision tolerance*, not proven to be a
  restitution threshold — do not conflate them. [CONFIRMED not to conflate.]
- [INFERRED] The likely mechanism for "low-speed bouncing is damped" in 1.x is
  the same class of fix as later engines (a resting-velocity cut-off below which
  restitution is zeroed), but this is unverified for 1.7.1.

### 3.3 Friction vs "suspension friction"

These are **different frictions** [CONFIRMED]:

- **Body/collision friction** — the per-rigid-body `FRICTION` coefficient
  (`hkRigidBody.friction`) used by the contact solver; combined per §3.1.
- **Suspension friction** — a parameter of the vehicle/car-wheel constraint
  (`*SUSPENSION_FRICTION`, and 3ds Max CarWheel `.suspensionFriction`) that
  governs the wheel/suspension model, not general contact.

In stock RA2 no wheel constraint was found in the component GMFs, so the game
appears to model wheels through its own `.txt` `grip`/`resistance` fields rather
than the Havok vehicle constraint. [CONFIRMED absence in stock GMFs;
INFERRED conclusion.]

### 3.4 Mass and `unyielding` (static)

- **Mass** is per-body, in **kg** (Xtra). Contacts use it via standard impulse
  dynamics; RA2 part masses come from each component GMF's RBCollection and sum
  to the component mass. [CONFIRMED]
- **Fixed/"unyielding" bodies have no mass:**
  > "Only movable rigid bodies can have mass. If a fixed rigid body needs to
  > move, then create it as a movable rigid body and lock its position (see
  > `hkRigidBody.pinned`)."
  > — Xtra, `hkRigidBody.mass`
- The GDC review equates fixed = infinite mass: *"…that were pinned, i.e.
  infinite mass and immovable."* [CONFIRMED]
- So `UNYIELDING 1` ⇒ treated as static/infinite-mass (arena floors, walls);
  `UNYIELDING 0` ⇒ dynamic. `pinned` is the *runtime* way to make a dynamic body
  act fixed. [CONFIRMED]

### 3.5 Integration scheme, substeps, solver family

- **Integrators:** the 1.7 SDK ships selectable ODE solvers — *"A selection of
  ODE solvers is provided including Euler and RK45."* and *"you can specify the
  integrator to use."* [CONFIRMED]. A semi-implicit (symplectic) Euler is the
  [LIKELY] default for games; RK45 is the high-accuracy option.
- **Solver family:** later Havok is documented as a velocity-based iterative
  solver in the **Projected Gauss-Seidel / sequential-impulse** family with
  **Baumgarte** position correction (tau/damping):
  > "Havok Physics uses a velocity-based iterative solver … an approach
  > consistent with the Projected Gauss-Seidel (PGS) family … positional error
  > correction … consistent with Baumgarte stabilization."
  > <https://www.havok.com/blog/how-havoks-constraint-solver-works-pgs-baumgarte/>
  Whether 1.7.1 used exactly PGS is [INFERRED]: the 1.x overview lists multiple
  friction models, a "Fast Subspace" lower-accuracy solver option, and
  `UPDATES_PER_TIMESTEP`, which are consistent with an iterative
  velocity/impulse solver family but do not prove PGS by name.
- **Default substeps:** not documented for 1.x. [`UNKNOWN`]. RA2 sets
  `UPDATES_PER_TIMESTEP = 1` and `USE_FAST_SUBSPACE = 1` in the stock sim object.
- **Units:** **kg, m, s** (gravity −9.81 m/s²; mass in kg; Xtra says friction
  typical 0.8–1.0 and restitution 0–1). [CONFIRMED]

### 3.6 Typical coefficient ranges — [CONFIRMED, same-era doc]

> friction: "0 = no friction, typical values are 0.8…1.0"
> restitution: "0 = no bounce, 1 = perfect bounce, >1 = gains energy"
> — Xtra, `hkRigidBody.friction` / `.restitution`

(RA2's own stock values are lower — see §5.)

---

## 4. Surviving public documentation

### 4.1 What survives for Havok 1.x

| Document | What it is | URL | Confidence |
|---|---|---|---|
| *Havok Game Dynamics SDK* product overview (©2002) | 33-page PDF; **explicitly the 1.7 SDK**; features, demo list, manual list, platform support, "source not shipped" list | <https://cgl.ethz.ch/teaching/former/seminar02/papers/Havok_Overview.pdf> (mirror <https://cgl.ethz.ch/Downloads/Seminar_Arbeiten/2002_03/Havok_Overview.pdf>) | [CONFIRMED] |
| *Havok Xtra Lingo Reference Guide* v1.0 (©2000/2001 Havok.com) | The closest thing to a **1.x API reference** that survives: `hkRigidBody.*`, world timestep/substeps/gravity, springs, linear/angular dashpots, deactivation/drag, and the **friction-combination formula** | <https://eclass.hmu.gr/modules/document/file.php/TP194/Havok%20Xtra%20Lingo%20Reference.pdf> | [CONFIRMED] |
| GDC/GDM June 2003 review of Havok 2.0 | Names 1.7 and 1.8 and contrasts with 2.0 | <https://media.gdcvault.com/GD_Mag_Archives/GDM_June_2003.pdf> | [CONFIRMED] |
| News of Havok Hardcore launch | Product identity/pricing | <https://www.gamedeveloper.com/game-platforms/havok-releases-havok-hardcore> | [CONFIRMED] |

### 4.2 What does **not** survive (searched, not found)

- The **Havok 1.x "SDK User Guide"**, the **CHM "Reference Manual"**, the
  **Collision Detection** guide, the **Constraints User Guide**, the **Havok
  Base** guide, and the **Class Reference** are **not publicly available**.
  The 1.7 overview lists them all as shipping PDFs/CHM, but no copy is indexed
  on the open web, archive.org, docslib, vdoc, or scribd (only third-party
  summaries such as the Scribd re-upload of the same overview exist).
  **[SEARCHED, NOT FOUND]**
- **archive.org** has *Havok Content Tools 2010 / 2011 / 2013* only — no 1.x
  SDK installer or manual (advancedsearch `title:havok` / full-text `havok`).
  <https://archive.org/advancedsearch.php?q=title%3A%28havok%29&output=json>
  **[SEARCHED, NOT FOUND]**
- No leaked Havok 1.x SDK source/headers were found. A `hkTransformShape` string
  appears in a random malware memory-scan page, which is not usable evidence.

### 4.3 Later Havok material that survives (2.x → 2013 → modern), and how it differs

- **`hkpWorldCinfo` header (Havok SDK, ~2009/2013)** — full solver config:
  `m_gravity`, `m_solverTau`, `m_solverDamp`, `m_solverIterations` (default 4),
  `m_solverMicrosteps`, `SolverType` enum
  (`SOLVER_TYPE_2ITERS_* / 4ITERS_* / 8ITERS_*` × SOFT/MEDIUM/HARD),
  `m_contactRestingVelocity`, `m_collisionTolerance` (0.1), deactivation
  params, `m_simulationType`, islands. Source:
  <https://raw.githubusercontent.com/nitaigao/engine-showcase/master/etc/vendor/havok/Source/Physics/Dynamics/World/hkpWorldCinfo.h>
  and the public 2013 SDK mirror
  <https://github.com/sigmaco/havok-2013-v1.0r1>.
- **Mirrors of the public SDK source** exist for much later versions, e.g.
  `sigmaco/havok-v5.1.0r1-b2007.09.19`, `sigmaco/havok-2013-v1.0r1` — but by 5.x
  the classes are `hkp*` (`hkpWorld`, `hkpRigidBody`, `hkpWorldCinfo`,
  `hkpRigidBodyCinfo`), not the 1.x `hk*` names.
- **Havok 3ds Max plugin** (modern) — `PredatorCZ/HavokMax`
  <https://github.com/PredatorCZ/HavokMax>.

**Concrete 1.x → 2.x differences** [CONFIRMED at the level of observable API]:

| Aspect | Havok 1.7 (RA2) | Havok 2.x → later |
|---|---|---|
| Physics namespace | `Havok::` + global `hk*` (`HavokObject`, `ReferenceObject`, `hkBaseObject`) | unified `hk*`/`hkp*` |
| Body class | `hkRigidBody` with properties (`mass`, `friction`, `restitution`, `pinned`) | `hkpRigidBody` constructed from **`hkpRigidBodyCinfo`/`hkRigidBodyCinfo`** |
| World class / config | `hkWorld` + `hkWorldCinfo`-like settings (gravity, resolver, fast subspace, updates-per-timestep, deactivation freq/drag) | `hkpWorld` + **`hkpWorldCinfo`** (solver type/tau/damp/iterations, simulation type, islands) |
| Solver | "fast subspace" on/off, `resolve`/`RESOLVER` friction models, `UPDATES_PER_TIMESTEP` | `SolverType` enum (2/4/8 iters × soft/med/hard), microsteps, `m_contactRestingVelocity` |
| Broad phase / islands | manual "subspaces", collision groups/filter/layers | automatic **simulation islands** (a headline 2.0 feature) |
| Config source | 3ds Max "Havok" exporter → `.gmf` `GMID_HAVOK_*` | `.hkx` / Havok Content Tools |

So `hkRigidBodyCinfo` and `hkWorldCinfo` are **the 2.x+ spellings**; they are the
right *analogy* but not the 1.7.1 classes. Any "1.7 uses hkRigidBodyCinfo"
statement should be treated as **reconstructed, not attested**.

---

## 5. Robot Arena 2 specifics

### 5.1 Confirmed engine facts (from the shipped binary/assets)

- RA2 statically links **Havok Hardcore 1.7.1**: build paths
  `E:\nightly_builds\code\hk171\havok\source\hkdynamics\world\world.cpp` and
  `E:\nightly_builds\code\hk171\havok\source\hkbase\debugutil\hkDefaultMonitorFactory.cpp`,
  plus the Hardcore banner. [CONFIRMED]
- Physics data is authored in 3ds Max with the Havok exporter and baked into
  `.gmf` under `GMID_HAVOK_*` nodes. [CONFIRMED]
- The `.gmf` rigid-body block (Mass, Ellasticity[`ELASTICITY`], Friction,
  Unyielding, Optimization_Level, Simulation_Geometry, Geometry_Proxy_Name,
  Use_Display_Proxy, Disable_Collisions, Inactive, Display_Proxy_Name) is
  exactly as the questioner described. The canonical community reference:
  > "**ELASTICITY** determines how elastic this component is… **FRICTION**
  > determines the friction… **UNYIELDING** determines whether or not this
  > object can move. 0 means movable, 1 means it won't budge no matter what.
  > **SIMULATION_GEOMETRY**… **GEOMETRY_PROXY_NAME**… **DISABLE_COLLISIONS**
  > determines whether or not this object can collide."
  > — beetlebros, "How to Mod RA2 Without 3D Studio Max"
  > <https://beetlebros.gametechmods.com/modding.htm> [CONFIRMED]
- RA2Wiki documents `SIMULATION_GEOMETRY` 1=convex, 2=concave, 3=proxy
  concave, 4=proxy convex, and the `GMID_HAVOK_SIMOBJECT` block (gravity,
  worldscale, simtolerance, resolver, drag, deactivator, short/long freq,
  fast subspace, updates per timestep). <https://wiki.gametechmods.com/GMF>
  [CONFIRMED]

### 5.2 Component stats and combat model

- Component stats live in the component `.txt` (inside `Components.cfz`),
  not the mesh: `hitpoints`, `fracture`, `mass`, `material = metal(x)`,
  `concussion`, `piercing`, `damageable`, `frequency`, `grip`, `resistance`,
  `passthru`, `contact`, etc. [CONFIRMED]
- **Part hitpoints:** most weapons default **1000 HP**; **razor tips 500**;
  ramplates **1000** (RA2Wiki). "Hitpoints … determines how much damage is
  needed before a component can fall off."
  <https://wiki.gametechmods.com/Hitpoints> · <https://beetlebros.gametechmods.com/ra2stats.htm>
  [CONFIRMED, community-measured]
- **Damage model:** two coefficients, **concussion** and **piercing**, combined
  into a "damage potential" — the community's empirical formula is
  **D = 183.9·P + 100·C** (P=piercing, C=concussion) for a reference setup;
  in-game damage scales with impact speed, and is attributed to whatever is
  struck. Piercing does more *chassis* damage on average. <https://beetlebros.gametechmods.com/ra2stats.htm>
  [CONFIRMED, empirical; formula is community-derived, not from the binary]
- **Chassis armour:** four selectable types (+ the default DSA). Community
  values (strength / weight):

  | Armour | Strength | Weight | Source |
  |---|---|---|---|
  | Plastic | 80 | 8 | GameFAQs (Korium9) |
  | Aluminium (default in manual) | 100 | 12 | GameFAQs |
  | Titanium | 150 | 16 | GameFAQs |
  | Steel | 175 | 20 | GameFAQs |
  | **DSA** (Double Strength Aluminium, the *actual* default) | ~Steel strength, Aluminium weight | 12 | RA2Wiki / community |

  The weights match the project's own recon note ("Armour kg/u²: Plastic 8,
  Aluminium 12, Steel 20, Titanium 16"). [CONFIRMED, community]
  <https://gamefaqs.gamespot.com/pc/589525-robot-arena-design-and-destroy/faqs/36292>
  · <https://wiki.gametechmods.com/Chassis_Armours>
  · <https://wiki.gametechmods.com/Armor>
- **Chassis durability → control board:** when chassis HP is gone, the chassis
  stops protecting the control board; the chassis `fracture` value is reported
  as **41** ("you will have to deal at least more than 41 damage points to
  damage the control board"). [CONFIRMED, community]
- Typical stock physics values seen in the shipped GMFs: friction 0.30,
  elasticity 0.30, optimization 0.5, unyielding 0; props e.g. crates 40 kg
  (friction 0.50, elasticity 0.30), barrels 1500 kg (friction 0.10)
  (project recon §9b). [CONFIRMED]

### 5.3 Community / tooling references

- **PyRA2** (LGPL) — the authoritative third-party parser for `.gmf`/`.bot`;
  its `GMIModel.Import`/`Objects` modules give the exact binary field order for
  rigid bodies and constraints used above. Vendored locally at
  `tools/vendor/PyRA2`; upstream
  <https://sourceforge.net/projects/pyra2/>. [CONFIRMED]
- **RA2Wiki (gametechmods)**: GMF format, Armor, Chassis Armours, Hitpoints,
  Havok Explosion, Creating Components.
  <https://wiki.gametechmods.com/GMF> and related pages. [CONFIRMED]
- **beetlebros** modding guide + `ra2stats.htm` (the damage/hitpoint tables).
  [CONFIRMED]
- **GameFAQs** general bot-building FAQ (armour table, F=MA note).
  [CONFIRMED]
- **apanx/RA2_AI** and **SilverfishGTM/BotFileChecker** — AI/component tooling
  that document the `.txt` component fields and damage types. [CONFIRMED]
- **"Havok explosion"** — a community-documented RA2 glitch (an invisible
  residual force field left where a component was torn off), consistent with a
  physics solver artifact. <https://wiki.gametechmods.com/Havok_Explosion>
  [CONFIRMED as community phenomenon; root cause [UNKNOWN].]
- **No dedicated "Robot Arena 2 Technical Reference"** document exists; the
  closest are the RA2Wiki GMF page and the beetlebros guides. [SEARCHED, NOT
  FOUND]

---

## 6. Bottom line — confirmed vs inferred vs unknown

**Confirmed**
- "Havok Hardcore" = rigid-body-only SKU launched Feb 2001; Havok 1.7 Game
  Dynamics SDK is ©2002 and supports PS2/GC/Xbox/PC; 1.8 and 2.0 followed.
- RA2 statically links a **1.7.1** Havok build (`hk171` paths + Hardcore banner).
- The API is a C++ class library with a `Havok::` namespace plus `hk*` classes
  (`Havok::HavokObject`, `Havok::ReferenceObject`, `hkBaseObject`, …), and the
  RA2 GMF serialises rigid bodies, a constraint solver, hinge/pivot/P2P/
  angular-dashpot constraints and a simulation object with the exact fields
  listed here.
- **Friction combination in the 1.x doc = √(μ₁² + μ₂²)** (Havok Xtra, ©2000/2001).
- Mass in kg; `unyielding`/fixed = immovable/infinite-mass; `pinned` = runtime
  fixed.
- World settings in RA2: gravity −9.81 m/s², `RESOLVER 3`,
  `USE_FAST_SUBSPACE 1`, `UPDATES_PER_TIMESTEP 1`, `SIMTOLERANCE 0.1`,
  `SHORTFREQ 5`, `LONGFREQ 0.1`; RA2 supplies these per `.gmf`.
- Havok 1.x ships selectable ODE integrators incl. **Euler** and **RK45**, and
  multiple friction/"resolver" models, with a "Fast Subspace" cheap option.
- No Havok 1.x SDK User Guide / CHM reference survives publicly; only the
  product overview and the Xtra guide.

**Inferred / likely (not directly attested)**
- Semi-implicit Euler is the default integrator; the solver is
  sequential-impulse/PGS-family with Baumgarte-style correction (documented for
  later Havok, not proven for 1.7.1).
- Restitution combination is **max** (modern Havok behaviour; unverified for 1.x).
- `hkCollisionDispatcher`/`hkConstraintSolver`/`hkShape`/`hkSphere`… are the
  right class names, but only their *modules/geometry kinds* are attested.

**Unknown**
- Exact dates for Havok 1.0–1.6 and whether a public "1.7.1" product label
  existed (the `hk171` tag is internal to the SDK build tree).
- Numeric semantics of `RESOLVER` (1/2/3) and `SOLVER_TYPE`, and of
  `UPDATES_PER_TIMESTEP`.
- 1.x **restitution combination** and any **restitution velocity threshold**.
- Whether RA2 uses `SUSPENSION_FRICTION` at runtime (it is present in the
  serializer but absent from the 169 stock component GMFs scanned).

### Sources
- Havok 1.7 overview PDF: https://cgl.ethz.ch/teaching/former/seminar02/papers/Havok_Overview.pdf
- Havok Xtra Lingo Reference (©2000/2001): https://eclass.hmu.gr/modules/document/file.php/TP194/Havok%20Xtra%20Lingo%20Reference.pdf
- GDC/GDM June 2003 (Havok 2.0 review, mentions 1.7/1.8): https://media.gdcvault.com/GD_Mag_Archives/GDM_June_2003.pdf
- Hardcore launch: https://www.gamedeveloper.com/game-platforms/havok-releases-havok-hardcore · https://cgpress.org/archives/industry_news_havok_releases_new_product_unveils_expansion_plans.html
- Later Havok world config: https://raw.githubusercontent.com/nitaigao/engine-showcase/master/etc/vendor/havok/Source/Physics/Dynamics/World/hkpWorldCinfo.h
- Later Havok solver: https://www.havok.com/blog/how-havoks-constraint-solver-works-pgs-baumgarte/
- Modern friction combine (contrast): https://www.abratabia.com/game-physics/havok.php
- RA2Wiki: https://wiki.gametechmods.com/GMF · https://wiki.gametechmods.com/Armor · https://wiki.gametechmods.com/Chassis_Armours · https://wiki.gametechmods.com/Hitpoints · https://wiki.gametechmods.com/Havok_Explosion
- beetlebros: https://beetlebros.gametechmods.com/modding.htm · https://beetlebros.gametechmods.com/ra2stats.htm
- GameFAQs bot-building FAQ: https://gamefaqs.gamespot.com/pc/589525-robot-arena-design-and-destroy/faqs/36292
- PyRA2: https://sourceforge.net/projects/pyra2/ (vendored locally)
- Local primary evidence: `local/game/Robot Arena 2.exe`, `local/game/**/*.gmf`, `tools/vendor/PyRA2`
