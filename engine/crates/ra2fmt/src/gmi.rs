//! Parser for RA2 `.gmf` files (the GMI binary mesh format).
//!
//! `.gmf` is a binary form of the 3ds Max `.ase` export, extended with RA2
//! attachment points and embedded Havok physics nodes. Field order follows the
//! community PyRA2 analysis; this is an independent implementation.

use crate::Error;

use std::collections::BTreeMap;

const MAGIC: &[u8; 3] = b"GMI";

/// A parsed `.gmf` scene.
#[derive(Debug, Clone, Default)]
pub struct GmiScene {
    pub materials: Vec<Material>,
    pub objects: Vec<SceneObject>,
    /// The Havok world settings baked into the file (one per scene).
    pub simulation: Option<SimulationObject>,
    /// The hinges the file's constraint solver declares.
    pub hinges: Vec<HingeJoint>,
    /// The scene's static background colour (RGB 0..1).
    pub background: [f32; 3],
    /// The scene's static ambient colour as stored. The original engine does
    /// not use it: it fixes `D3DRS_AMBIENT` to `0xff262626` (`FUN_0058f390`).
    pub ambient: [f32; 3],
}

/// A hinge declared by a model's Havok constraint solver.
///
/// Arenas use these for their own moving hazards — the compressor's smash arms
/// and the ClawTop's claws are hinges in the file, which is why the scripts only
/// have to ask for them by name (`plus.Arena.GetHinge("Hinge01")`) and then
/// drive them. The pivot and axis are already in world space.
#[derive(Clone, Debug, Default)]
pub struct HingeJoint {
    /// The name the arena scripts use (`Hinge01`).
    pub name: String,
    /// The two objects the hinge joins; the first is usually part of the fixed
    /// scenery.
    pub body_a: String,
    pub body_b: String,
    /// The pin's position and axis, both already in world space.
    pub pivot: [f32; 3],
    pub axis: [f32; 3],
    /// `*IS_LIMITED`: whether the hinge stops at its angle limits.
    pub limited: bool,
    pub friction: f32,
    /// `*ANGLE_LIMITS`, in radians.
    pub limits: [f32; 2],
    /// The hinge node's whole frame (rows are axes, the last the origin):
    /// `frame[1]` is the spin axis, `frame[3]` the pivot.
    pub frame: [[f32; 3]; 4],
    /// `*POINT` and `*SPIN_AXIS` as the file stores them (the exporter's own
    /// copy; for robot components they are not in component space).
    pub point: [f32; 3],
    pub spin_axis: [f32; 3],
}

impl GmiScene {
    /// Scene light objects (`*LIGHT`).
    pub fn lights(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter().filter(|o| o.light.is_some())
    }

    /// Geometric objects that carry a mesh.
    pub fn meshes(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter().filter(|o| o.mesh.is_some())
    }

    /// Attachment points (`Point01`, `Base_F`, ...).
    pub fn attachments(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects
            .iter()
            .filter(|o| o.kind == ObjectKind::Attachment)
    }

    /// Attachment points keyed by the `ID` a `.bot` refers to.
    pub fn attachment_table(&self) -> BTreeMap<i32, Attachment> {
        self.attachments()
            .filter_map(Attachment::from_object)
            .map(|a| (a.id, a))
            .collect()
    }

    /// Every rigid body in every `RBCollection`, in file order (children follow
    /// their parent, depth first).
    pub fn rigid_bodies(&self) -> Vec<&RigidBody> {
        fn walk<'a>(body: &'a RigidBody, out: &mut Vec<&'a RigidBody>) {
            out.push(body);
            for child in &body.children {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        for o in &self.objects {
            if o.kind != ObjectKind::RigidBodies {
                continue;
            }
            for rb in &o.rigid_bodies {
                walk(rb, &mut out);
            }
        }
        out
    }

    /// A geometry object by name (case-insensitive, `(null)` is never a match).
    pub fn find_object(&self, name: &str) -> Option<&SceneObject> {
        let want = name.trim();
        if want.is_empty() || want.eq_ignore_ascii_case("(null)") {
            return None;
        }
        self.objects
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(want))
    }
}

/// The Havok world settings (`GMID_HAVOK_SIMOBJECT`) serialised into a `.gmf`.
///
/// RA2 authors each arena's physics in 3ds Max; the values below are what the
/// game hands to Havok 1.7.1 at load. `gravity` is the one the simulation must
/// honour — 19 of the 20 shipped arenas use `-9.80998`, the practice arena uses
/// `-12.752974` (1.3 g).
#[derive(Debug, Clone, PartialEq)]
pub struct SimulationObject {
    pub name: String,
    pub gravity: [f32; 3],
    pub world_scale: f32,
    pub tolerance: f32,
    pub resolver: i32,
    pub include_drag: bool,
    pub linear_drag: f32,
    pub angular_drag: f32,
    pub include_deactivator: bool,
    pub short_freq: f32,
    pub long_freq: f32,
    pub fast_subspace: bool,
    pub updates_per_timestep: f32,
    pub collision_pairs: i32,
}

impl Default for SimulationObject {
    fn default() -> Self {
        SimulationObject {
            name: String::new(),
            gravity: [0.0, -9.809_98, 0.0],
            world_scale: 39.37,
            tolerance: 0.1,
            resolver: 3,
            include_drag: false,
            linear_drag: 0.0,
            angular_drag: 0.0,
            include_deactivator: true,
            short_freq: 5.0,
            long_freq: 0.1,
            fast_subspace: true,
            updates_per_timestep: 1.0,
            collision_pairs: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectKind {
    Geometric,
    Attachment,
    Light,
    Camera,
    RigidBodies,
    Simulation,
    Constraints,
    Dashpot,
    Unknown(i32),
}

impl Default for ObjectKind {
    fn default() -> Self {
        ObjectKind::Unknown(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Texture {
    pub name: String,
    pub class: String,
    /// Referenced bitmap file name (may be empty).
    pub bitmap: String,
    /// `*MAP_AMOUNT`.
    pub amount: f32,
    /// The map slot (`GMTexture+0x34`): 0 ambient, 1 diffuse, 2 specular,
    /// 3 shine, 4 shine strength, 5 self-illumination, 6 opacity, 7 filter
    /// colour, 8 bump, 9 reflect, 10 refract, ... The original's material
    /// builder (`FUN_005ae630`) only acts on 1, 5, 6 and 9.
    pub kind: i32,
    /// `*MAP_TYPE` (Screen / Explicit / Spherical ...), as stored.
    pub map_type: i32,
    /// u offset, v offset, u tiling, v tiling, angle (kept as stored; the
    /// original never applies a texture matrix, bar the sphere map).
    pub uv: [f32; 5],
    /// `*BITMAP_COLORMAP_ENABLE`.
    pub colormap: i32,
    /// `*BITMAP_INVERT`. On an opacity map the original alpha-tests only when
    /// this is 1 (`FUN_005ae630` case 6), and sorts-and-blends otherwise.
    pub invert: i32,
    /// `*BITMAP_FILTER`: 0 Pyramidal, 1 SAT. The original maps 0 to point
    /// sampling and 1 to bilinear (`FUN_005b24c0`, texture entry `+0xc`).
    pub filter: i32,
    /// `*BITMAP_MAP_CHANNEL` (1 based).
    pub channel: i32,
}

/// `MAP_*` slot numbers (`Texture::kind`).
pub const MAP_DIFFUSE: i32 = 1;
pub const MAP_SELFILLUM: i32 = 5;
pub const MAP_OPACITY: i32 = 6;
pub const MAP_REFLECT: i32 = 9;

#[derive(Debug, Clone, Default)]
pub struct Material {
    pub name: String,
    pub class: String,
    pub ambient: [f32; 3],
    pub diffuse: [f32; 3],
    pub specular: [f32; 3],
    /// `*MATERIAL_SHINE` (the original's D3D specular power is this times 100).
    pub shine: f32,
    /// `*MATERIAL_SHINESTRENGTH` (scales the specular colour).
    pub shine_strength: f32,
    /// `*MATERIAL_SELFILLUM`. The original's builder never reads it.
    pub self_illum: f32,
    /// `*MATERIAL_XP_TYPE` as stored: 1 Filter (alpha blend), 3 Additive.
    pub xp_type: i32,
    /// Transparency (0 = opaque).
    pub transparency: f32,
    /// The GMF material's "two sided" flag as stored. The original's material
    /// apply (`FUN_005b24c0`) sets `D3DRS_CULLMODE` to CCW when the byte at
    /// material+0x10 is non-zero and to NONE when it is zero.
    pub two_sided: i32,
    pub textures: Vec<Texture>,
    /// Sub-materials of a `Multi/Sub-Object` material. A face's `material` index
    /// selects one of these (falling back to the parent material).
    pub sub_materials: Vec<Material>,
}

impl Material {
    /// Resolve the effective material for a face within this (top-level) slot,
    /// the way the original's batch builder (`FUN_005ae630`) does: without
    /// sub-materials every face uses this material; with `n` of them a face id
    /// above `n` is clamped to the last one (`n - 1`), and a face whose id is
    /// exactly `n` (or negative) matches no sub-material pass and is not drawn
    /// (`None`). The compressor's fence bars are face id 3 of 2 sub-materials.
    pub fn resolve_face(&self, face_material: i32) -> Option<&Material> {
        let n = self.sub_materials.len() as i32;
        if n == 0 {
            return Some(self);
        }
        let id = if face_material > n {
            n - 1
        } else {
            face_material
        };
        if id < 0 || id >= n {
            return None;
        }
        self.sub_materials.get(id as usize)
    }

    /// First referenced bitmap file name, if any.
    pub fn first_bitmap(&self) -> Option<&str> {
        self.textures
            .iter()
            .map(|t| t.bitmap.as_str())
            .find(|b| !b.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub v: [u32; 3],
    /// Material index; `-1` denotes a collision (non-rendered) mesh.
    pub material: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub time: i32,
    pub material_ref: i32,
    pub points: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
    pub texcoords: Vec<[f32; 3]>,
    /// Per-face texture indices (parallel to `faces`), when present.
    pub texfaces: Vec<[u32; 3]>,
    /// Four per face: the face normal, then corner A, B, C's vertex normals
    /// (the order the exe's GMA writer prints them, `0x5c2568`).
    pub normals: Vec<[f32; 3]>,
    pub backface_cull: i32,
    /// Vertex colours (`ccount` entries, RGB 0..1) and their per-face indices.
    pub colors: Vec<[f32; 3]>,
    pub colorfaces: Vec<[u32; 3]>,
    /// Extra UV channels (`*MESH_MAPPINGCHANNEL`): the lightmaps' second set.
    pub channels: Vec<TexChannel>,
}

/// One additional texture-coordinate channel of a mesh.
#[derive(Debug, Clone, Default)]
pub struct TexChannel {
    /// The channel number a texture's `channel` refers to (2 is the first extra).
    pub index: i32,
    pub texcoords: Vec<[f32; 3]>,
    pub texfaces: Vec<[u32; 3]>,
}

impl Mesh {
    /// A collision mesh is tagged with material `-1` (used for Havok proxies).
    pub fn is_collision(&self) -> bool {
        self.material_ref < 0 || self.faces.iter().any(|f| f.material < 0)
    }
}

/// `HAVOK_GEO_TYPE`: a triangle mesh (the default).
pub const GEO_TYPE_STANDARD: i32 = 0;
/// `HAVOK_GEO_TYPE`: an infinite plane primitive.
pub const GEO_TYPE_PLANE: i32 = 1;

#[derive(Debug, Clone, Default)]
pub struct RigidBody {
    pub name: String,
    pub mass: f32,
    pub elasticity: f32,
    pub friction: f32,
    /// Collision-quality hint (`OPTIMIZATION_LEVEL`), 0.5 stock.
    pub optimization_level: f32,
    /// 1 means the body never budges (static / infinite mass).
    pub unyielding: i32,
    /// Which geometry the simulation uses (1 convex, 2 concave, 3 proxy
    /// concave, 4 proxy convex, 5 display-proxy concave, 6 display-proxy
    /// convex). 5 is what every stock arena's main collider declares; see
    /// [`RigidBody::simulation_objects`].
    pub simulation_geometry: i32,
    /// Optional mesh used for simulation (`GEOMETRY_PROXY_NAME`), `(null)` when
    /// the body simulates its own geometry.
    pub geometry_proxy: String,
    pub use_display_proxy: bool,
    pub disable_collisions: bool,
    pub inactive: bool,
    /// Object drawn in place of the collision geometry (`DISPLAY_PROXY_NAME`).
    pub display_proxy: String,
    /// Placement of the body (`NODE_TM`); equal to the display object's own.
    pub transform: [[f32; 3]; 4],
    pub geo_type: i32,
    pub children: Vec<RigidBody>,
}

impl RigidBody {
    /// The geometry object this body simulates: the proxy when set, else the
    /// body's own name.
    pub fn collision_object(&self) -> &str {
        if self.geometry_proxy.trim().is_empty()
            || self.geometry_proxy.eq_ignore_ascii_case("(null)")
        {
            &self.name
        } else {
            &self.geometry_proxy
        }
    }

    /// `HAVOK_GEO_TYPE = 1` is an **infinite plane primitive**: the body has no
    /// mesh at all and its `NODE_TM` gives the plane's point and normal (the
    /// matrix's Y row).
    ///
    /// The shipped data uses it three times and never anything else besides
    /// `Standard` (0): `Clawtop` and `FlexTop` declare their arena *floor* this
    /// way (normal up) and the Octagon its *ceiling* — `topplane_collision`,
    /// normal down, 15.6 m up. So a static body with no triangles is not
    /// necessarily a mistake.
    pub fn is_plane(&self) -> bool {
        self.geo_type == GEO_TYPE_PLANE
    }

    /// The geometry objects this body simulates, by `SIMULATION_GEOMETRY`.
    ///
    /// 1/2 (convex, concave) is the body's own object; 3/4 (proxy
    /// concave/convex) is `GEOMETRY_PROXY_NAME`; **5/6 are the display-proxy
    /// forms**: `DISPLAY_PROXY_NAME` — the mesh the body is *shown* as, which
    /// is normally a group node whose children are the real geometry.
    ///
    /// This names the display proxy for 5/6, but when the body has geometry
    /// of its own the original collides with that instead (live, the
    /// Octagon's collidable is `main_collision`'s own 65-vertex slab, not the
    /// `main_display` group; `ra2_render::collect_havok` applies that, docs/29
    /// section 12h).
    pub fn simulation_objects(&self) -> Vec<&str> {
        let source = match self.simulation_geometry {
            3 | 4 => self.geometry_proxy.as_str(),
            5 | 6 => self.display_proxy.as_str(),
            _ => self.name.as_str(),
        };
        let source = if source.trim().is_empty() || source.eq_ignore_ascii_case("(null)") {
            self.name.as_str()
        } else {
            source
        };
        source
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("(null)"))
            .collect()
    }

    /// The object to draw: the display proxy when requested, else the body name.
    pub fn display_object(&self) -> &str {
        if self.use_display_proxy
            && !self.display_proxy.trim().is_empty()
            && !self.display_proxy.eq_ignore_ascii_case("(null)")
        {
            &self.display_proxy
        } else {
            &self.name
        }
    }

    /// A body with no mass (or an explicit `UNYIELDING`) never moves.
    pub fn is_static(&self) -> bool {
        self.unyielding != 0 || self.mass <= 0.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct SceneObject {
    pub kind: ObjectKind,
    pub name: String,
    pub parent: String,
    /// 3x4 (four vec3 rows) transformation matrix.
    pub transform: [[f32; 3]; 4],
    pub data: String,
    pub mesh: Option<Mesh>,
    pub light: Option<Light>,
    pub rigid_bodies: Vec<RigidBody>,
    /// A rigid-body collection's `GMID_HAVOK_DIS_COLLISION_PAIRS`: pairs of
    /// bodies (by name) Havok never collides with each other.
    pub disabled_pairs: Vec<(String, String)>,
}

/// A `*LIGHT` object. Only omni lights occur in the shipped data; the original
/// turns each into a `D3DLIGHT_POINT` (`FUN_005ba1e0`): diffuse = colour x
/// intensity, specular = colour, no ambient, position = the node's translation, range =
/// `attn_end` and attenuation `(0.4, 3/range, 40/range^2)` when `far_atten`,
/// else range 10000 and attenuation `(1, 0, 0)`.
#[derive(Debug, Clone, Default)]
pub struct Light {
    /// 0 = Omni.
    pub kind: i32,
    pub shadows: i32,
    pub use_light: i32,
    pub color: [f32; 3],
    pub intensity: f32,
    pub aspect: f32,
    pub attn_start: f32,
    pub attn_end: f32,
    pub far_atten: bool,
}

/// One attachment point on a component mesh.
///
/// A `.bot` refers to these by `ID`, so that is the key; the object name
/// (`Point01`, `Base_F`, ...) is arbitrary and must not be trusted for ordering.
///
/// `types` are the mount classes the point offers. The names seen across the
/// stock components are `base`, `generic` and `axle`, each suffixed `_F` or
/// `_M` (female/male) — a mount pairs the child's point with the parent's. A
/// motor's axle point (`axle_M`, and `Host = Axle`) sits on the rotor, which is
/// why a wheel's `axle_F` mates with the motor rather than the chassis.
///
/// `has_body` marks the points whose payload also declares rigid-body fields
/// (`Mass`, `Friction`, `Simulation_Geometry`, ...) — those are the Havok
/// attachment bodies, i.e. the points that rotate or translate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attachment {
    /// The `ID` a `.bot` names.
    pub id: i32,
    pub name: String,
    /// Mount classes, lower-cased (`["base_f", "generic_f"]`).
    pub types: Vec<String>,
    /// Mesh object hosting the point (`Host = Axle`), lower-cased.
    pub host: Option<String>,
    /// The payload declares rigid-body fields.
    pub has_body: bool,
    /// Point frame in component space (rows are axes, last is the origin).
    pub transform: [[f32; 3]; 4],
}

impl Attachment {
    /// Parse an attachment object's `data` payload.
    ///
    /// The payload is `Key = Value` chunks separated by runs of two or more
    /// spaces; a value may contain commas (`Attach = Base_F, Generic_F`), so the
    /// split cannot be on single whitespace. Keys are not case-consistent across
    /// the shipped files (`Type`/`type`, `ID`/`id`).
    pub fn from_object(o: &SceneObject) -> Option<Attachment> {
        let mut a = Attachment {
            transform: o.transform,
            name: o.name.clone(),
            ..Default::default()
        };
        let mut seen_id = false;
        for chunk in split_attach_fields(&o.data) {
            let Some((key, value)) = chunk.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            match key.as_str() {
                "id" => {
                    a.id = value.parse().ok()?;
                    seen_id = true;
                }
                "attach" => {
                    a.types = value
                        .split(',')
                        .map(|t| t.trim().to_ascii_lowercase())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                "host" => {
                    let host = value.trim().to_ascii_lowercase();
                    if !host.is_empty() && host != "<none>" {
                        a.host = Some(host);
                    }
                }
                "mass" | "simulation_geometry" => a.has_body = true,
                _ => {}
            }
        }
        seen_id.then_some(a)
    }

    /// Whether the child's point class mates with this point's class.
    ///
    /// Classes pair across gender: `generic_f` accepts `generic_m`. `base` is
    /// the same family as `generic` (both are flat mounting faces).
    pub fn accepts(&self, other: &str) -> bool {
        fn family(t: &str) -> Option<&str> {
            t.strip_suffix("_f").or_else(|| t.strip_suffix("_m"))
        }
        fn gender(t: &str) -> &str {
            t.rsplit('_').next().unwrap_or("")
        }
        let other = other.to_ascii_lowercase();
        self.types
            .iter()
            .any(|mine| family(mine) == family(&other) && gender(mine) != gender(&other))
    }
}

/// Split a `.gmf` attachment payload into `Key = Value` chunks.
fn split_attach_fields(data: &str) -> Vec<&str> {
    let bytes = data.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            // A single space stays inside the value; a run ends a field.
            if j - i >= 2 {
                let chunk = data[start..i].trim();
                if !chunk.is_empty() {
                    out.push(chunk);
                }
                start = j;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let tail = data[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

/// Parse the bytes of a `.gmf` file.
pub fn parse(data: &[u8]) -> Result<GmiScene, Error> {
    Parser {
        r: Reader::new(data),
        pending_hinges: Vec::new(),
    }
    .parse()
}

// ---------------------------------------------------------------------------

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn new(d: &'a [u8]) -> Self {
        Reader { d, p: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.p + n;
        if end > self.d.len() {
            return Err(Error::UnexpectedEof {
                at: self.p,
                wanted: n,
            });
        }
        let s = &self.d[self.p..end];
        self.p = end;
        Ok(s)
    }

    fn i32(&mut self) -> Result<i32, Error> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.i32()? as u32))
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn string(&mut self) -> Result<String, Error> {
        let n = self.i32()?;
        if n <= 0 {
            return Ok(String::new());
        }
        let mut v = self.take(n as usize)?.to_vec();
        if v.last() == Some(&0) {
            v.pop();
        }
        Ok(String::from_utf8_lossy(&v).into_owned())
    }

    fn color(&mut self) -> Result<[f32; 3], Error> {
        let b = self.take(4)?;
        Ok([
            b[0] as f32 / 255.0,
            b[1] as f32 / 255.0,
            b[2] as f32 / 255.0,
        ])
    }

    fn vec3(&mut self) -> Result<[f32; 3], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn triplet(&mut self) -> Result<(i32, i32, i32), Error> {
        Ok((self.i32()?, self.i32()?, self.i32()?))
    }

    fn tm(&mut self) -> Result<[[f32; 3]; 4], Error> {
        Ok([self.vec3()?, self.vec3()?, self.vec3()?, self.vec3()?])
    }

    fn node(&mut self) -> Result<(i32, i32, i32), Error> {
        self.triplet()
    }
}

struct Parser<'a> {
    r: Reader<'a>,
    /// Hinges read from the constraint list now being parsed.
    pending_hinges: Vec<HingeJoint>,
}

impl<'a> Parser<'a> {
    fn parse(mut self) -> Result<GmiScene, Error> {
        if self.r.take(3)? != MAGIC {
            return Err(Error::BadMagic("not a GMI file".into()));
        }
        let hdr = [self.r.i32()?, self.r.i32()?, self.r.i32()?, self.r.i32()?];
        if hdr != [3, 1, 0, 15] {
            return Err(Error::Structure(format!("unexpected GMI header {hdr:?}")));
        }
        let (background, ambient) = self.read_scene()?;
        let mut scene = GmiScene {
            background,
            ambient,
            materials: self.read_material_list()?,
            ..GmiScene::default()
        };
        self.read_object_list(&mut scene)?;
        Ok(scene)
    }

    fn expect_node(&mut self, id: i32, flags: i32, what: &str) -> Result<(), Error> {
        let (nid, nflags, _) = self.r.node()?;
        if nid != id || nflags != flags {
            return Err(Error::Structure(format!(
                "expected {what} node {id}/{flags}, got {nid}/{nflags}"
            )));
        }
        Ok(())
    }

    fn read_tm_node(&mut self) -> Result<[[f32; 3]; 4], Error> {
        self.expect_node(17, 2, "transformation matrix")?;
        self.r.string()?; // node name
        self.r.tm()
    }

    fn read_scene(&mut self) -> Result<([f32; 3], [f32; 3]), Error> {
        self.expect_node(1, 2, "scene")?;
        self.r.string()?; // source
        self.r.i32()?; // first frame
        self.r.i32()?; // last frame
        self.r.i32()?; // speed
        self.r.i32()?; // ticks
        let background = self.r.color()?; // static background
        let ambient = self.r.color()?; // static ambient
        Ok((background, ambient))
    }

    fn read_material_list(&mut self) -> Result<Vec<Material>, Error> {
        self.expect_node(7, 2, "material list")?;
        let count = self.r.i32()?;
        (0..count).map(|_| self.read_material()).collect()
    }

    fn read_material(&mut self) -> Result<Material, Error> {
        self.expect_node(8, 2, "material")?;
        self.r.i32()?; // reference number
        let mut m = Material {
            name: self.r.string()?,
            class: self.r.string()?,
            ..Default::default()
        };
        m.ambient = self.r.color()?;
        m.diffuse = self.r.color()?;
        m.specular = self.r.color()?;
        m.shine = self.r.f32()?;
        m.shine_strength = self.r.f32()?;
        m.transparency = self.r.f32()?;
        self.r.f32()?; // wire size
        self.r.i32()?; // shading type
        self.r.f32()?; // xp falloff
        m.self_illum = self.r.f32()?;
        m.two_sided = self.r.i32()?;
        m.xp_type = self.r.i32()?;
        let texcount = self.r.i32()?;
        if texcount != 0 {
            m.textures = self.read_texture_list()?;
        }
        let subcount = self.r.i32()?;
        if subcount != 0 {
            m.sub_materials = self.read_material_list()?;
        }
        Ok(m)
    }

    fn read_texture_list(&mut self) -> Result<Vec<Texture>, Error> {
        self.expect_node(14, 2, "texture list")?;
        let count = self.r.i32()?;
        (0..count).map(|_| self.read_texture()).collect()
    }

    fn read_texture(&mut self) -> Result<Texture, Error> {
        self.expect_node(15, 4, "texture")?;
        let mut t = Texture {
            name: self.r.string()?,
            class: self.r.string()?,
            bitmap: self.r.string()?,
            ..Default::default()
        };
        t.amount = self.r.f32()?;
        t.kind = self.r.i32()?;
        t.map_type = self.r.i32()?;
        for k in 0..8 {
            // u/v offset + tiling, angle, blur, blur offset, noise amount
            let v = self.r.f32()?;
            if k < 5 {
                t.uv[k] = v;
            }
        }
        self.r.f32()?; // noise size
        self.r.i32()?; // noise level
        self.r.f32()?; // noise phase
        t.colormap = self.r.i32()?; // *BITMAP_COLORMAP_ENABLE
        t.invert = self.r.i32()?; // *BITMAP_INVERT
        t.filter = self.r.i32()?;
        t.channel = self.r.i32()?;
        let subcount = self.r.i32()?;
        if subcount != 0 {
            let _ = self.read_texture_list()?;
        }
        Ok(t)
    }

    fn read_object_list(&mut self, scene: &mut GmiScene) -> Result<(), Error> {
        self.expect_node(18, 2, "object list")?;
        let count = self.r.i32()?;
        for _ in 0..count {
            let (id, flags, _len) = self.r.node()?;
            match id {
                2 => scene.objects.push(self.read_geometric(flags)?),
                21 => scene.objects.push(self.read_attachment(flags)?),
                5 => scene.objects.push(self.read_light()?),
                4 => scene.objects.push(self.read_camera()?),
                31 => scene.objects.push(self.read_rigidbody_collection(flags)?),
                30 => {
                    let sim = self.read_simulation()?;
                    scene.objects.push(SceneObject {
                        kind: ObjectKind::Simulation,
                        name: sim.name.clone(),
                        ..Default::default()
                    });
                    scene.simulation = Some(sim);
                }
                42 => {
                    let solver = self.read_constraint_solver()?;
                    scene.objects.push(solver);
                    scene.hinges.append(&mut self.pending_hinges);
                }
                49 => scene.objects.push(self.read_angular_dashpot()?),
                other => scene.objects.push(SceneObject {
                    kind: ObjectKind::Unknown(other),
                    ..Default::default()
                }),
            }
        }
        Ok(())
    }

    fn read_geometric(&mut self, flags: i32) -> Result<SceneObject, Error> {
        if flags != 4 {
            return Err(Error::Structure(format!("geometric flags {flags}")));
        }
        let mut o = SceneObject {
            kind: ObjectKind::Geometric,
            ..Default::default()
        };
        o.name = self.r.string()?;
        o.parent = self.r.string()?;
        let _shade_verts = self.r.u8()?;
        o.transform = self.read_tm_node()?;
        o.mesh = Some(self.read_mesh()?);
        Ok(o)
    }

    fn read_mesh(&mut self) -> Result<Mesh, Error> {
        self.expect_node(16, 4, "mesh")?;
        let mut m = Mesh {
            time: self.r.i32()?,
            ..Default::default()
        };
        let pcount = self.r.i32()?;
        let fcount = self.r.i32()?;
        let tcount = self.r.i32()?;
        let ccount = self.r.i32()?;
        m.material_ref = self.r.i32()?;

        m.points.reserve(pcount.max(0) as usize);
        for _ in 0..pcount {
            m.points.push(self.r.vec3()?);
        }
        m.faces.reserve(fcount.max(0) as usize);
        for _ in 0..fcount {
            let (a, b, c) = self.r.triplet()?;
            let mat = self.r.i32()?;
            m.faces.push(Face {
                v: [a as u32, b as u32, c as u32],
                material: mat,
            });
        }
        for _ in 0..tcount {
            m.texcoords.push(self.r.vec3()?);
        }
        if tcount != 0 {
            for _ in 0..fcount {
                let (a, b, c) = self.r.triplet()?;
                m.texfaces.push([a as u32, b as u32, c as u32]);
            }
        }
        if self.r.i32()? != 0 {
            m.channels = self.read_texture_channels()?;
        }
        for _ in 0..ccount {
            m.colors.push(self.r.vec3()?);
        }
        if ccount != 0 {
            for _ in 0..fcount {
                let (a, b, c) = self.r.triplet()?;
                m.colorfaces.push([a as u32, b as u32, c as u32]);
            }
        }
        for _ in 0..(fcount.max(0) * 4) {
            m.normals.push(self.r.vec3()?);
        }
        m.backface_cull = self.r.i32()?;
        Ok(m)
    }

    fn read_texture_channels(&mut self) -> Result<Vec<TexChannel>, Error> {
        let n = self.r.i32()?;
        let mut out = Vec::new();
        for _ in 0..n {
            let mut ch = TexChannel {
                index: self.r.i32()?,
                ..Default::default()
            };
            let tcount = self.r.i32()?;
            self.r.i32()?;
            let fcount = self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            for _ in 0..tcount {
                ch.texcoords.push(self.r.vec3()?);
            }
            if tcount != 0 {
                for _ in 0..fcount {
                    let (a, b, c) = self.r.triplet()?;
                    ch.texfaces.push([a as u32, b as u32, c as u32]);
                }
            }
            out.push(ch);
        }
        Ok(out)
    }

    fn read_attachment(&mut self, flags: i32) -> Result<SceneObject, Error> {
        if flags != 2 {
            return Err(Error::Structure(format!("attachment flags {flags}")));
        }
        let name = self.r.string()?;
        let transform = self.read_tm_node()?;
        let data = self.r.string()?;
        Ok(SceneObject {
            kind: ObjectKind::Attachment,
            name,
            transform,
            data,
            ..Default::default()
        })
    }

    fn read_light(&mut self) -> Result<SceneObject, Error> {
        let name = self.r.string()?;
        let transform = self.read_tm_node()?;
        let target = self.r.string()?;
        if !target.is_empty() {
            let _ = self.read_tm_node()?;
        }
        let mut l = Light {
            kind: self.r.i32()?,
            shadows: self.r.i32()?,
            use_light: self.r.i32()?,
            color: self.r.color()?,
            intensity: self.r.f32()?,
            aspect: self.r.f32()?,
            ..Default::default()
        };
        self.r.take(8)?; // hotspot / falloff (spot lights only)
        l.attn_start = self.r.f32()?;
        l.attn_end = self.r.f32()?;
        self.r.f32()?; // target distance
        l.far_atten = self.r.u8()? != 0;
        self.r.take(3)?;
        Ok(SceneObject {
            kind: ObjectKind::Light,
            name,
            transform,
            light: Some(l),
            ..Default::default()
        })
    }

    fn read_camera(&mut self) -> Result<SceneObject, Error> {
        let name = self.r.string()?;
        let transform = self.read_tm_node()?;
        if self.r.i32()? != 0 {
            let _ = self.read_tm_node()?;
        }
        self.r.i32()?;
        for _ in 0..6 {
            self.r.f32()?;
        }
        Ok(SceneObject {
            kind: ObjectKind::Camera,
            name,
            transform,
            ..Default::default()
        })
    }

    fn read_simulation(&mut self) -> Result<SimulationObject, Error> {
        let name = self.r.string()?;
        let mut g = [0.0f32; 3];
        for v in g.iter_mut() {
            *v = self.r.f32()?;
        }
        Ok(SimulationObject {
            name,
            gravity: g,
            world_scale: self.r.f32()?,
            tolerance: self.r.f32()?,
            resolver: self.r.i32()?,
            include_drag: self.r.u8()? != 0,
            linear_drag: self.r.f32()?,
            angular_drag: self.r.f32()?,
            include_deactivator: self.r.u8()? != 0,
            short_freq: self.r.f32()?,
            long_freq: self.r.f32()?,
            fast_subspace: self.r.u8()? != 0,
            updates_per_timestep: self.r.f32()?,
            collision_pairs: self.r.i32()?,
        })
    }

    fn read_rigidbody_collection(&mut self, flags: i32) -> Result<SceneObject, Error> {
        if flags != 4 {
            return Err(Error::Structure(format!(
                "rigid body collection flags {flags}"
            )));
        }
        let name = self.r.string()?;
        let paircount = self.r.i32()?;
        self.r.i32()?; // solver type
        let rigid_bodies = self.read_rigidbody_list()?;
        let disabled_pairs = if paircount != 0 {
            self.read_disabled_pairs()?
        } else {
            Vec::new()
        };
        Ok(SceneObject {
            kind: ObjectKind::RigidBodies,
            name,
            rigid_bodies,
            disabled_pairs,
            ..Default::default()
        })
    }

    fn read_rigidbody_list(&mut self) -> Result<Vec<RigidBody>, Error> {
        self.expect_node(33, 2, "rigid body list")?;
        let count = self.r.i32()?;
        (0..count).map(|_| self.read_rigidbody()).collect()
    }

    fn read_rigidbody(&mut self) -> Result<RigidBody, Error> {
        self.expect_node(32, 4, "rigid body")?;
        let name = self.r.string()?;
        let mass = self.r.f32()?;
        let elasticity = self.r.f32()?;
        let friction = self.r.f32()?;
        let optimization_level = self.r.f32()?;
        let unyielding = self.r.i32()?;
        let simulation_geometry = self.r.i32()?;
        let geometry_proxy = self.r.string()?;
        let use_display_proxy = self.r.u8()? != 0;
        let disable_collisions = self.r.u8()? != 0;
        let inactive = self.r.u8()? != 0;
        let display_proxy = self.r.string()?;
        let transform = self.read_tm_node()?;
        let geo_type = self.r.i32()?;
        let childcount = self.r.i32()?;
        let mut children = Vec::new();
        if childcount != 0 {
            children = self.read_rigidbody_list()?;
        }
        Ok(RigidBody {
            name,
            mass,
            elasticity,
            friction,
            optimization_level,
            unyielding,
            simulation_geometry,
            geometry_proxy,
            use_display_proxy,
            disable_collisions,
            inactive,
            display_proxy,
            transform,
            geo_type,
            children,
        })
    }

    fn read_disabled_pairs(&mut self) -> Result<Vec<(String, String)>, Error> {
        self.expect_node(51, 2, "disabled collision pairs")?;
        let count = self.r.i32()?;
        (0..count)
            .map(|_| Ok((self.r.string()?, self.r.string()?)))
            .collect()
    }

    fn read_constraint_solver(&mut self) -> Result<SceneObject, Error> {
        self.pending_hinges.clear();
        let name = self.r.string()?;
        self.r.f32()?; // threshold
        self.r.string()?; // rigid body collection name
        let n = self.r.i32()?;
        if n != 0 {
            // List is documented but not needed for the first milestones; skip by structure.
            self.read_constraint_list()?;
        }
        Ok(SceneObject {
            kind: ObjectKind::Constraints,
            name,
            ..Default::default()
        })
    }

    fn read_constraint_list(&mut self) -> Result<(), Error> {
        self.expect_node(44, 2, "constraint list")?;
        let count = self.r.i32()?;
        for _ in 0..count {
            let (cid, _cflags, _) = self.r.node()?;
            match cid {
                46 => {
                    // A constraint with eight trailing floats. No stock arena
                    // uses one; skipped by structure.
                    let _ = self.r.string()?;
                    let _ = self.read_tm_node()?;
                    let _ = self.r.string()?;
                    let _ = self.r.string()?;
                    let _ = self.r.vec3()?;
                    let _ = self.r.vec3()?;
                    for _ in 0..8 {
                        self.r.f32()?;
                    }
                }
                47 => {
                    // A hinge: its name, its frame, the two bodies it joins and
                    // its limits, in the same order the exported `.gmf` writes
                    // (`*BODY1`, `*BODY2`, `*POINT`, `*SPIN_AXIS`, `*IS_LIMITED`,
                    // `*FRICTION`, `*ANGLE_LIMITS`).
                    let name = self.r.string()?;
                    let transform = self.read_tm_node()?;
                    let body_a = self.r.string()?;
                    let body_b = self.r.string()?;
                    let point = self.r.vec3()?;
                    let spin_axis = self.r.vec3()?;
                    let limited = self.r.u8()? != 0;
                    let friction = self.r.f32()?;
                    let limit_min = self.r.f32()?;
                    let limit_max = self.r.f32()?;
                    // The node's transform *is* the hinge's frame: `TM_ROW3` is
                    // the pivot and `TM_ROW1` the spin axis.
                    let pivot = transform[3];
                    let a = transform[1];
                    let len = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
                    let axis = if len > 1e-9 {
                        [a[0] / len, a[1] / len, a[2] / len]
                    } else {
                        [0.0, 1.0, 0.0]
                    };
                    self.pending_hinges.push(HingeJoint {
                        name,
                        body_a,
                        body_b,
                        pivot,
                        axis,
                        limited,
                        friction,
                        limits: [limit_min, limit_max],
                        frame: transform,
                        point,
                        spin_axis,
                    });
                }
                53 => {
                    let _ = self.r.string()?;
                    let _ = self.read_tm_node()?;
                    let _ = self.r.string()?;
                    let _ = self.r.string()?;
                    let _ = self.r.vec3()?;
                    let _ = self.r.vec3()?;
                }
                other => {
                    return Err(Error::Structure(format!("unknown constraint node {other}")));
                }
            }
        }
        Ok(())
    }

    fn read_angular_dashpot(&mut self) -> Result<SceneObject, Error> {
        let name = self.r.string()?;
        let _ = self.read_tm_node()?;
        let _ = self.r.string()?;
        let _ = self.r.string()?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.u8()?;
        for _ in 0..4 {
            self.r.f32()?;
        }
        Ok(SceneObject {
            kind: ObjectKind::Dashpot,
            name,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FUN_005ae630` clamps a face id above the sub-material count to the last
    /// one and draws no face whose id equals the count (`0x5ae8fa`: `jle`).
    #[test]
    fn a_face_id_past_the_sub_materials_clamps_and_one_equal_to_the_count_is_dropped() {
        let sub = |name: &str| Material {
            name: name.into(),
            ..Default::default()
        };
        let multi = Material {
            name: "Metal".into(),
            sub_materials: vec![sub("Chain Fence"), sub("Bars")],
            ..Default::default()
        };
        let name = |id: i32| multi.resolve_face(id).map(|m| m.name.clone());
        assert_eq!(name(0).as_deref(), Some("Chain Fence"));
        assert_eq!(name(1).as_deref(), Some("Bars"));
        assert_eq!(name(2), None, "an id equal to the count matches no pass");
        assert_eq!(
            name(3).as_deref(),
            Some("Bars"),
            "the compressor fence's bars"
        );
        assert_eq!(name(-1), None);
        let plain = sub("Floor");
        assert_eq!(
            plain.resolve_face(7).map(|m| m.name.as_str()),
            Some("Floor")
        );
    }

    fn attachment(name: &str, data: &str) -> Attachment {
        let o = SceneObject {
            kind: ObjectKind::Attachment,
            name: name.to_string(),
            data: data.to_string(),
            ..Default::default()
        };
        Attachment::from_object(&o).expect("id present")
    }

    /// `SIMULATION_GEOMETRY` picks the geometry source, and `HAVOK_GEO_TYPE`
    /// tells a plane primitive apart from a mesh.
    ///
    /// 5/6 are the display-proxy forms: every stock arena's main collider uses
    /// 5 and names a group node (`main_display` and friends), while components
    /// use 3 with a real proxy mesh.
    #[test]
    fn simulation_geometry_names_the_geometry_source() {
        let mut rb = RigidBody {
            name: "body".into(),
            ..Default::default()
        };
        rb.simulation_geometry = 2;
        assert_eq!(rb.simulation_objects(), ["body"]);

        rb.simulation_geometry = 3;
        rb.geometry_proxy = "body_sim".into();
        assert_eq!(rb.simulation_objects(), ["body_sim"]);

        // 5 is the display proxy, not the body's own mesh — the decode that
        // matters for arenas.
        rb.simulation_geometry = 5;
        rb.display_proxy = "main_display".into();
        assert_eq!(rb.simulation_objects(), ["main_display"]);

        // A missing or `(null)` name falls back to the body's own object.
        rb.display_proxy = "(null)".into();
        assert_eq!(rb.simulation_objects(), ["body"]);
        rb.simulation_geometry = 4;
        rb.geometry_proxy = " a , b ,(null)".into();
        assert_eq!(rb.simulation_objects(), ["a", "b"]);

        assert!(!rb.is_plane());
        rb.geo_type = GEO_TYPE_PLANE;
        assert!(rb.is_plane());
    }

    #[test]
    fn attachment_payload_fields_split_on_wide_gaps() {
        // CRLF, mixed case keys, and an `Attach` list containing commas.
        let data = "Type = Attach\r\nID = 4\r\nAttach = Axle_M\r\nHost = Axle\r\n\r\nMass = 0.000000\r\nUnyielding = 0";
        let a = attachment("Point03", data);
        assert_eq!(a.id, 4);
        assert_eq!(a.types, ["axle_m"]);
        assert_eq!(a.host.as_deref(), Some("axle"));
        assert!(a.has_body);
    }

    #[test]
    fn attachment_keeps_multi_class_points_and_ignores_none_hosts() {
        let a = attachment(
            "Point01",
            "Type = Attach  ID = 1  Attach = Base_F, Generic_F",
        );
        assert_eq!(a.id, 1);
        assert_eq!(a.types, ["base_f", "generic_f"]);
        assert_eq!(a.host, None);
        assert!(!a.has_body);
        let b = attachment(
            "Point03",
            "type = attach  id = 3  attach = axle_m  host = <none>",
        );
        assert_eq!(b.host, None);
    }

    #[test]
    fn mount_classes_pair_across_gender_in_the_same_family() {
        let axle_f = attachment("Point01", "Type = Attach  ID = 1  Attach = Axle_F");
        assert!(axle_f.accepts("axle_m"));
        assert!(!axle_f.accepts("axle_f"));
        assert!(!axle_f.accepts("generic_m"));
        let base_f = attachment(
            "Point01",
            "Type = Attach  ID = 1  Attach = Base_F, Generic_F",
        );
        assert!(base_f.accepts("base_m"));
        assert!(base_f.accepts("generic_m"));
    }
}
