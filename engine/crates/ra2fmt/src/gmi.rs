//! Parser for RA2 `.gmf` files (the GMI binary mesh format).
//!
//! `.gmf` is a binary form of the 3ds Max `.ase` export, extended with RA2
//! attachment points and embedded Havok physics nodes. Field order follows the
//! community PyRA2 analysis; this is an independent implementation.

use crate::Error;

const MAGIC: &[u8; 3] = b"GMI";

/// A parsed `.gmf` scene.
#[derive(Debug, Clone, Default)]
pub struct GmiScene {
    pub materials: Vec<Material>,
    pub objects: Vec<SceneObject>,
}

impl GmiScene {
    /// Geometric objects that carry a mesh.
    pub fn meshes(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter().filter(|o| o.mesh.is_some())
    }

    /// Attachment points (`Point01`, `Base_F`, ...).
    pub fn attachments(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter().filter(|o| o.kind == ObjectKind::Attachment)
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
}

#[derive(Debug, Clone, Default)]
pub struct Material {
    pub name: String,
    pub class: String,
    pub diffuse: [f32; 3],
    pub textures: Vec<Texture>,
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
    pub normals: Vec<[f32; 3]>,
    pub backface_cull: i32,
}

impl Mesh {
    /// A collision mesh is tagged with material `-1` (used for Havok proxies).
    pub fn is_collision(&self) -> bool {
        self.material_ref < 0 || self.faces.iter().any(|f| f.material < 0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct RigidBody {
    pub name: String,
    pub mass: f32,
    pub elasticity: f32,
    pub friction: f32,
    pub geo_type: i32,
    pub children: Vec<RigidBody>,
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
    pub rigid_bodies: Vec<RigidBody>,
}

/// Parse the bytes of a `.gmf` file.
pub fn parse(data: &[u8]) -> Result<GmiScene, Error> {
    Parser { r: Reader::new(data) }.parse()
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
            return Err(Error::UnexpectedEof { at: self.p, wanted: n });
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
        Ok([b[0] as f32 / 255.0, b[1] as f32 / 255.0, b[2] as f32 / 255.0])
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
        let mut scene = GmiScene::default();
        self.read_scene()?;
        scene.materials = self.read_material_list()?;
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

    fn read_scene(&mut self) -> Result<(), Error> {
        self.expect_node(1, 2, "scene")?;
        self.r.string()?; // source
        self.r.i32()?; // first frame
        self.r.i32()?; // last frame
        self.r.i32()?; // speed
        self.r.i32()?; // ticks
        self.r.color()?; // static background
        self.r.color()?; // static ambient
        Ok(())
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
        self.r.color()?; // ambient
        m.diffuse = self.r.color()?;
        self.r.color()?; // specular
        self.r.f32()?; // shine
        self.r.f32()?; // shine strength
        self.r.f32()?; // transparency
        self.r.f32()?; // wire size
        self.r.i32()?; // shading type
        self.r.f32()?; // xp falloff
        self.r.f32()?; // self illumination
        self.r.i32()?; // two sided
        self.r.i32()?; // xp type
        let texcount = self.r.i32()?;
        if texcount != 0 {
            m.textures = self.read_texture_list()?;
        }
        let subcount = self.r.i32()?;
        if subcount != 0 {
            let _ = self.read_material_list()?;
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
        let t = Texture {
            name: self.r.string()?,
            class: self.r.string()?,
            bitmap: self.r.string()?,
        };
        self.r.f32()?; // amount
        self.r.i32()?; // type
        self.r.i32()?; // map type
        for _ in 0..8 {
            self.r.f32()?; // u/v offset + tiling, angle, blur, blur offset, noise
        }
        self.r.f32()?; // noise size
        self.r.i32()?; // noise level
        self.r.f32()?; // noise phase
        self.r.i32()?; // invert
        self.r.i32()?; // unknown
        self.r.i32()?; // filter
        self.r.i32()?; // channel
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
                30 => scene.objects.push(self.read_simulation()?),
                42 => scene.objects.push(self.read_constraint_solver()?),
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
        let mut o = SceneObject { kind: ObjectKind::Geometric, ..Default::default() };
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
            m.faces.push(Face { v: [a as u32, b as u32, c as u32], material: mat });
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
            self.read_texture_channels()?;
        }
        for _ in 0..ccount {
            let _ = self.r.vec3()?;
        }
        if ccount != 0 {
            for _ in 0..fcount {
                let _ = self.r.triplet()?;
            }
        }
        for _ in 0..(fcount.max(0) * 4) {
            m.normals.push(self.r.vec3()?);
        }
        m.backface_cull = self.r.i32()?;
        Ok(m)
    }

    fn read_texture_channels(&mut self) -> Result<(), Error> {
        let n = self.r.i32()?;
        for _ in 0..n {
            self.r.i32()?; // channel index
            let tcount = self.r.i32()?;
            self.r.i32()?;
            let fcount = self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            self.r.i32()?;
            for _ in 0..tcount {
                let _ = self.r.vec3()?;
            }
            if tcount != 0 {
                for _ in 0..fcount {
                    let _ = self.r.triplet()?;
                }
            }
        }
        Ok(())
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
        self.r.i32()?;
        self.r.i32()?;
        self.r.i32()?;
        self.r.color()?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.take(8)?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.u8()?;
        self.r.take(3)?;
        Ok(SceneObject { kind: ObjectKind::Light, name, transform, ..Default::default() })
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
        Ok(SceneObject { kind: ObjectKind::Camera, name, transform, ..Default::default() })
    }

    fn read_simulation(&mut self) -> Result<SceneObject, Error> {
        let name = self.r.string()?;
        let _ = self.r.vec3()?; // gravity
        self.r.f32()?; // world scale
        self.r.f32()?; // sim tolerance
        self.r.i32()?; // resolver
        self.r.u8()?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.u8()?;
        self.r.f32()?;
        self.r.f32()?;
        self.r.u8()?;
        self.r.f32()?;
        self.r.i32()?;
        Ok(SceneObject { kind: ObjectKind::Simulation, name, ..Default::default() })
    }

    fn read_rigidbody_collection(&mut self, flags: i32) -> Result<SceneObject, Error> {
        if flags != 4 {
            return Err(Error::Structure(format!("rigid body collection flags {flags}")));
        }
        let name = self.r.string()?;
        let paircount = self.r.i32()?;
        self.r.i32()?; // solver type
        let rigid_bodies = self.read_rigidbody_list()?;
        if paircount != 0 {
            self.read_disabled_pairs()?;
        }
        Ok(SceneObject {
            kind: ObjectKind::RigidBodies,
            name,
            rigid_bodies,
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
        self.r.f32()?; // optimization level
        self.r.i32()?; // unyielding
        self.r.i32()?; // simulation geometry
        self.r.string()?; // geometry proxy name
        self.r.u8()?; // use display proxy
        self.r.u8()?; // disable collisions
        self.r.u8()?; // inactive
        self.r.string()?; // display proxy name
        let _tm = self.read_tm_node()?;
        let geo_type = self.r.i32()?;
        let childcount = self.r.i32()?;
        let mut children = Vec::new();
        if childcount != 0 {
            children = self.read_rigidbody_list()?;
        }
        Ok(RigidBody { name, mass, elasticity, friction, geo_type, children })
    }

    fn read_disabled_pairs(&mut self) -> Result<(), Error> {
        self.expect_node(51, 2, "disabled collision pairs")?;
        let count = self.r.i32()?;
        for _ in 0..count {
            let _ = self.r.string()?;
            let _ = self.r.string()?;
        }
        Ok(())
    }

    fn read_constraint_solver(&mut self) -> Result<SceneObject, Error> {
        let name = self.r.string()?;
        self.r.f32()?; // threshold
        self.r.string()?; // rigid body collection name
        let n = self.r.i32()?;
        if n != 0 {
            // List is documented but not needed for the first milestones; skip by structure.
            self.read_constraint_list()?;
        }
        Ok(SceneObject { kind: ObjectKind::Constraints, name, ..Default::default() })
    }

    fn read_constraint_list(&mut self) -> Result<(), Error> {
        self.expect_node(44, 2, "constraint list")?;
        let count = self.r.i32()?;
        for _ in 0..count {
            let (cid, _cflags, _) = self.r.node()?;
            match cid {
                46 => {
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
                    let _ = self.r.string()?;
                    let _ = self.read_tm_node()?;
                    let _ = self.r.string()?;
                    let _ = self.r.string()?;
                    let _ = self.r.vec3()?;
                    let _ = self.r.vec3()?;
                    self.r.u8()?;
                    self.r.f32()?;
                    self.r.f32()?;
                    self.r.f32()?;
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
        Ok(SceneObject { kind: ObjectKind::Dashpot, name, ..Default::default() })
    }
}
