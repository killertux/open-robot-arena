//! Reader and writer for RA2 `.bot` robot files.
//!
//! `.bot` files are newline-separated text with two embedded uncompressed TGA
//! blobs (a 128×128 snapshot and, for the chassis, a 256×256 texture). Layout
//! follows the community PyRA2 analysis and is validated against the shipped
//! bots (formats 1.07, 1.08, 1.11 and 1.12).
//!
//! The writer reproduces the game's own text form, including the details the
//! binary's writer leaves on the page: `%g`-style numbers (6 significant
//! digits, three-digit exponents), a trailing space after each controller
//! binding and each top-corner row, and the file's own line ending. All 57
//! shipped bots round-trip byte-for-byte (`write(parse(file)) == file`).
//!
//! Versions differ in which sections exist; the writer gates on [`Bot::version`]
//! and a fresh [`Bot`] is 1.12 (the newest form the reader accepts).

use crate::Error;

/// A robot, as a `.bot` file records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Bot {
    /// The file-format version (`1.07`, `1.08`, `1.11`, `1.12`).
    pub version: String,
    /// The robot's name, without the `Name: ` label.
    pub name: String,
    /// 0 light, 1 medium, 2 heavy. The value the file stores, which some AI
    /// bots were hand-adjusted away from their true weight.
    pub class: i32,
    /// The 1.12 header's default-texture flag. Older files have no such line.
    pub default_texture: Option<bool>,
    /// Raw 128×128 uncompressed TGA snapshot, if the header carries one.
    pub snapshot: Option<Vec<u8>>,
    /// Components; the first is the chassis.
    pub components: Vec<Component>,
    /// The saved copy of the (partly) buckled chassis model, 1.11 onward.
    pub buckle_model: Option<BotMesh>,
    /// Forward heading in radians, 1.11 onward.
    pub heading: Option<f32>,
    /// The remote-control scheme.
    pub controllers: Vec<Controller>,
    /// Controller-action-to-component wiring.
    pub bindings: Vec<Binding>,
    /// The file's own line ending. The game reads either; the writer keeps the
    /// one the file was saved with.
    pub line_ending: LineEnding,
}

impl Default for Bot {
    fn default() -> Self {
        Bot {
            version: "1.12".into(),
            name: String::new(),
            class: 0,
            default_texture: Some(true),
            snapshot: None,
            components: Vec::new(),
            buckle_model: None,
            heading: Some(0.0),
            controllers: Vec::new(),
            bindings: Vec::new(),
            line_ending: LineEnding::Lf,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Component {
    pub parent: i32,
    /// Attachment point **on the parent component** this one mounts to (`mP`).
    ///
    /// The file stores this *before* `own_mount`; the labels in the community
    /// bot-format document are the other way round, which is wrong — verified
    /// against PyRA2's reader and against bots whose components name it: a stock
    /// wheel declares `4` here and a stock angle motor has exactly one point 4,
    /// its `axle_M`, which mates with the wheel's own `axle_F`.
    pub parent_mount: i32,
    /// Attachment point **on this component** used for the mount (`mC`).
    pub own_mount: i32,
    pub translation: [f32; 3],
    /// Stored as `(x, y, z, w)` — PyRA2's `quatToAxisAndAngle` treats element 3
    /// as the scalar, and a chassis is exactly `[0, 0, 0, 1]`.
    pub rotation: [f32; 4],
    pub angle: f32,
    pub height: f32,
    pub class: String,
    pub file: String,
    pub chassis: Option<Chassis>,
    /// `BurstMotor` start/stop axle angles (radians), when present.
    pub burst_angles: Option<[f32; 2]>,
    /// `SmartZone` name line (the AI event zone), when present.
    pub smart_zone: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chassis {
    pub height: f32,
    /// The face count the height line carries (the model's triangle count).
    pub face_count: i32,
    pub mesh: Option<BotMesh>,
    /// Bottom-plate outline points (0..1000), the chassis footprint.
    pub bottom: Vec<[i32; 2]>,
    /// Top-plate outline points (0..1000).
    pub top: Vec<[i32; 2]>,
    /// Raw 256×256 uncompressed TGA bytes (chassis skin), if present.
    pub texture_tga: Option<Vec<u8>>,
    /// Armour type/weight/hardness from the bot's armour row.
    pub armour: Option<Armour>,
    /// The armour row's cosmetic twin (texture scale/shininess), present when a
    /// 3D model is. The reader ignores it; the writer keeps it.
    pub armour_material: Option<[f32; 5]>,
    /// Crush corners of the top plate: `(index, a, b, c)` rows. They are the
    /// points the game adds to the model, which is why the buckle points'
    /// vertex indices run past the mesh's own point list.
    pub top_corners: Vec<[i32; 4]>,
    /// The face groups that dress the chassis model (F10).
    pub face_groups: Vec<FaceGroup>,
    /// The group-dressing triangles, in the order the file lists them.
    ///
    /// `group` normally indexes [`Chassis::face_groups`], but 60 shipped rows
    /// name groups past the end of that list; those are kept verbatim because
    /// the reader (like the game) ignores them rather than dropping the lines.
    pub faces: Vec<Face>,
    /// The chassis' buckle points, from 1.11 onward (F9).
    ///
    /// The game subdivides the chassis' faces so its sides can cave in: each of
    /// these points is a vertex that appears several times in the mesh (once per
    /// face meeting at it), and a hit shifts them "down and in". `indices` are
    /// the duplicated vertices that have to move together.
    pub buckling: Vec<BucklePoint>,
}

/// One triangle of a chassis face group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    /// The face group this triangle dresses, as the file records it.
    pub group: i32,
    /// Mesh point indices, in winding order.
    pub points: [u32; 3],
}

/// One "face group" of the chassis: a patch of the chassis model together with
/// the part of the 256×256 skin that dresses it.
///
/// The bot file describes each side of the chassis (its sides, top and bottom)
/// as a group: a polygon on the texture map, a rectangle bounding that polygon,
/// and the triangles that belong to it (`docs`' "face groups"). Crucially the
/// group's texture coordinates are **per face vertex** (three per triangle),
/// not per mesh point: a mesh point shared by two groups needs a different
/// texture coordinate in each, and using the model's shared per-point UVs is
/// what makes a chassis skin come out rotated and mirrored (F10).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FaceGroup {
    /// `(u, v)` on the 256×256 skin, three per triangle of the group.
    pub coords: Vec<[f32; 2]>,
    /// `(x1, y1, x2, y2)` rectangle on the skin that bounds `coords`.
    pub bounds: [i32; 4],
}

/// One movable point of the chassis' buckle model.
#[derive(Debug, Clone, PartialEq)]
pub struct BucklePoint {
    /// Starting coordinates, as the file records them.
    pub position: [f32; 3],
    /// Vertices of the chassis mesh that meet at this point.
    pub indices: Vec<u32>,
}
/// Chassis armour, exactly as the bot file records it (see
/// `Armor_Definitions.txt`: type, weight per square unit, hardness).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Armour {
    /// 0 plastic, 1 aluminium, 2 steel, 3 titanium.
    pub type_id: i32,
    /// Weight per square unit of chassis surface.
    pub weight: f32,
    /// Hardness — the chassis' hitpoints. Stock bots store 2× the value the
    /// bot editor writes (e.g. steel 175 in the editor, 350 on AI bots).
    pub hardness: f32,
}

impl Default for Armour {
    fn default() -> Self {
        // Plastic: the lightest stock armour.
        Armour {
            type_id: 0,
            weight: 8.0,
            hardness: 80.0,
        }
    }
}

impl Armour {
    /// Stock name for this armour type.
    pub fn name(&self) -> &'static str {
        match self.type_id {
            0 => "Plastic",
            1 => "Aluminium",
            2 => "Steel",
            3 => "Titanium",
            _ => "Unknown",
        }
    }

    /// The cosmetic armour row the bot editor writes for this type: material
    /// scales, shininess and the editor's own hardness.
    fn material_row(&self) -> [f32; 5] {
        match self.type_id {
            0 => [0.2669, 0.2669, 0.19975, 0.85, 80.0],
            1 => [0.19975, 0.19975, 0.19975, 0.85, 20.0],
            2 => [0.3332, 0.30005, 0.30005, 0.85, 10.0],
            3 => [0.40035, 0.4998, 0.63325, 0.85, 60.0],
            _ => [1.0, 1.0, 1.0, 0.85, 0.0],
        }
    }
}

impl Chassis {
    /// Chassis mass in kg: armour weight per square unit × chassis surface area.
    /// (The bot format document states the rule: "a 3d face with an area of 1
    /// square unit using steel armouring, weighs 20 Kg".)
    pub fn mass(&self) -> f32 {
        let area = self.mesh.as_ref().map(|m| m.surface_area()).unwrap_or(0.0);
        area * self.armour.unwrap_or_default().weight
    }
}

/// A mesh embedded directly in a `.bot` (chassis shape or buckle model).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BotMesh {
    pub points: Vec<[f32; 3]>,
    /// One normal per point, as the 3D block records them.
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
}

impl BotMesh {
    /// Sum of the triangle areas (the unit the armour weight is quoted in).
    pub fn surface_area(&self) -> f32 {
        let mut area = 0.0;
        for tri in self.indices.as_chunks::<3>().0 {
            let (i, j, k) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            if i >= self.points.len() || j >= self.points.len() || k >= self.points.len() {
                continue;
            }
            let (a, b, c) = (self.points[i], self.points[j], self.points[k]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            area += 0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        }
        area
    }
}

/// One remote-control scheme (`Name: Forward`, its position and key bindings).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Controller {
    /// The control's name, without the `Name: ` label.
    pub name: String,
    /// `(x, y, type)` on the remote; the type is 1 analog, 2 switch, 4 button.
    pub position: [i32; 3],
    /// Keyboard/joystick codes. 1.07 writes three; newer files write four.
    pub bindings: Vec<[i32; 4]>,
}

/// A wiring entry: what a controller position does to a component.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Binding {
    /// Index into [`Bot::components`].
    pub component: i32,
    /// The action (`Spin Clockwise`, `Fire`, ...).
    pub action: String,
    /// `(x, y, binding)` — the controller position and binding number.
    pub control: [i32; 3],
}

/// A file's line ending.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineEnding {
    #[default]
    Lf,
    Crlf,
}

impl LineEnding {
    pub(crate) fn s(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
        }
    }
}

/// Parse `.bot` bytes; only the header and component list are required.
pub fn parse(data: &[u8]) -> Result<Bot, Error> {
    let line_ending = if data
        .iter()
        .position(|&b| b == b'\n')
        .is_some_and(|i| i > 0 && data[i - 1] == b'\r')
    {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    let mut lines = Lines { d: data, p: 0 };
    let version = lines.str_line();
    let name = strip_label(&lines.str_line(), "Name: ");
    let class_line = String::from_utf8_lossy(lines.line()).into_owned();
    let class = class_line
        .rsplit([' ', '\t'])
        .next()
        .unwrap_or("0")
        .parse::<i32>()
        .unwrap_or(0);
    let ver = parse_version(&version);
    // 1.12 starts the body with the default-texture flag; older files put the
    // snapshot flag straight after the class line. Accept both.
    let x = lines.line();
    let mut default_texture = None;
    let snap = match x {
        b"true" => true,
        b"false" => false,
        _ => {
            default_texture = Some(!x.is_empty() && x != b"0");
            lines.line() == b"true"
        }
    };
    let snapshot = if snap {
        Some(lines.take_tga(Some((128, 128)))?.to_vec())
    } else {
        None
    };

    let mut bot = Bot {
        version,
        name,
        class,
        default_texture,
        snapshot,
        line_ending,
        ..Default::default()
    };

    let n = lines.ints()?.first().copied().unwrap_or(0);
    for _ in 0..n {
        let mut c = Component {
            parent: lines.i32()?,
            parent_mount: lines.i32()?,
            own_mount: lines.i32()?,
            translation: lines.vec3()?,
            rotation: lines.vec4()?,
            ..Default::default()
        };
        let ah = lines.floats()?;
        c.angle = ah.first().copied().unwrap_or(0.0);
        c.height = ah.get(1).copied().unwrap_or(0.0);
        c.class = lines.str_line();
        c.file = lines.str_line();
        match c.class.as_str() {
            "Chassis" => c.chassis = Some(parse_chassis(&mut lines, ver)?),
            "BurstMotor" => {
                let a = lines.floats().unwrap_or_default();
                c.burst_angles = Some([
                    a.first().copied().unwrap_or(0.0),
                    a.get(1).copied().unwrap_or(0.0),
                ]);
            }
            "SmartZone" => {
                c.smart_zone = Some(lines.str_line());
            }
            _ => {}
        }
        bot.components.push(c);
    }

    // 1.11 onward: the saved buckle model and the forward heading.
    if ver > (1, 8) {
        if lines.line() == b"true" {
            bot.buckle_model = Some(parse_3d_data(&mut lines)?);
        }
        let h = lines.floats()?;
        bot.heading = Some(h.first().copied().unwrap_or(0.0));
    }

    let nctrl = lines.i32()?;
    for _ in 0..nctrl {
        let name = strip_label(&lines.str_line(), "Name: ");
        let position = lines.ints()?;
        let position = [
            position.first().copied().unwrap_or(0),
            position.get(1).copied().unwrap_or(0),
            position.get(2).copied().unwrap_or(0),
        ];
        let nb = lines.i32()?;
        let mut bindings = Vec::new();
        for _ in 0..nb {
            let v = lines.ints()?;
            bindings.push([
                v.first().copied().unwrap_or(0),
                v.get(1).copied().unwrap_or(0),
                v.get(2).copied().unwrap_or(0),
                v.get(3).copied().unwrap_or(0),
            ]);
        }
        bot.controllers.push(Controller {
            name,
            position,
            bindings,
        });
    }

    let nbind = lines.i32()?;
    for _ in 0..nbind {
        let component = lines.i32()?;
        let action = lines.str_line();
        let v = lines.ints()?;
        bot.bindings.push(Binding {
            component,
            action,
            control: [
                v.first().copied().unwrap_or(0),
                v.get(1).copied().unwrap_or(0),
                v.get(2).copied().unwrap_or(0),
            ],
        });
    }
    Ok(bot)
}

fn parse_3d_data(lines: &mut Lines) -> Result<BotMesh, Error> {
    lines.line(); // "274" totalpts totalidx totalfaces
    let nmodel = lines.ints()?.first().copied().unwrap_or(0);
    let mut mesh = BotMesh::default();
    for _ in 0..nmodel {
        let hdr = lines.ints()?;
        let (idxc, ptc) = (
            hdr.first().copied().unwrap_or(0),
            hdr.get(2).copied().unwrap_or(0),
        );
        let raw = lines.line();
        if raw != b"RAW" {
            return Err(Error::Structure("expected RAW in 3D data".into()));
        }
        for _ in 0..ptc {
            mesh.normals.push(lines.vec3()?);
        }
        for _ in 0..ptc {
            let uv = lines.floats()?;
            mesh.uvs.push([
                uv.first().copied().unwrap_or(0.0),
                uv.get(1).copied().unwrap_or(0.0),
            ]);
        }
        for _ in 0..ptc {
            mesh.points.push(lines.vec3()?);
        }
        for _ in 0..idxc {
            mesh.indices.push(lines.i32()? as u32);
        }
    }
    Ok(mesh)
}

fn parse_chassis(lines: &mut Lines, ver: (i32, i32)) -> Result<Chassis, Error> {
    let bpc = lines.i32()?;
    let mut bottom = Vec::new();
    for _ in 0..bpc {
        let v = lines.ints()?;
        bottom.push([
            v.first().copied().unwrap_or(0),
            v.get(1).copied().unwrap_or(0),
        ]);
    }
    let tpc = lines.i32()?;
    let mut top = Vec::new();
    for _ in 0..tpc {
        let v = lines.ints()?;
        top.push([
            v.first().copied().unwrap_or(0),
            v.get(1).copied().unwrap_or(0),
        ]);
    }
    let hf = lines.floats()?;
    let mut ch = Chassis {
        height: hf.first().copied().unwrap_or(0.0),
        face_count: hf.get(1).copied().unwrap_or(0.0) as i32,
        bottom,
        top,
        ..Default::default()
    };

    let has_model = lines.line() == b"true";
    if has_model {
        ch.mesh = Some(parse_3d_data(lines)?);
        if lines.line() == b"true" {
            ch.texture_tga = Some(lines.take_tga(Some((256, 256)))?.to_vec());
        }
        // The "unknown section" flag exists from 1.08 onward (absent in 1.07).
        if ver > (1, 7) && lines.line() == b"true" {
            return Err(Error::Structure(
                "unknown chassis sub-section present".into(),
            ));
        }
        let ng = lines.i32()?;
        for _ in 0..ng {
            let npts = lines.i32()?;
            let mut coords = Vec::new();
            for _ in 0..npts {
                let uv = lines.floats()?;
                coords.push([
                    uv.first().copied().unwrap_or(0.0),
                    uv.get(1).copied().unwrap_or(0.0),
                ]);
            }
            ch.face_groups.push(FaceGroup {
                coords,
                ..Default::default()
            });
        }
        let ng2 = lines.i32()?;
        for i in 0..ng2 {
            // `group index, x1 y1, x2 y2` — the group's rectangle on the skin.
            let v = lines.ints()?;
            let group = v.first().copied().unwrap_or(i);
            let bounds = [
                v.get(1).copied().unwrap_or(0),
                v.get(2).copied().unwrap_or(0),
                v.get(3).copied().unwrap_or(0),
                v.get(4).copied().unwrap_or(0),
            ];
            if let Some(fg) = usize::try_from(group)
                .ok()
                .and_then(|g| ch.face_groups.get_mut(g))
            {
                fg.bounds = bounds;
            }
        }
        let nfe = lines.i32()?;
        for _ in 0..nfe {
            // `group index, then 3 point indices` — the group's triangles.
            let v = lines.ints()?;
            ch.faces.push(Face {
                group: v.first().copied().unwrap_or(0),
                points: [
                    v.get(1).copied().unwrap_or(0) as u32,
                    v.get(2).copied().unwrap_or(0) as u32,
                    v.get(3).copied().unwrap_or(0) as u32,
                ],
            });
        }
        let nce = lines.i32()?;
        for _ in 0..nce {
            // `index 3 a b c` — the top plate's crush corners.
            let v = lines.ints()?;
            ch.top_corners.push([
                v.first().copied().unwrap_or(0),
                v.get(2).copied().unwrap_or(0),
                v.get(3).copied().unwrap_or(0),
                v.get(4).copied().unwrap_or(0),
            ]);
        }
    }
    // Armour: a type row, plus an extra row when a 3D model is present.
    //
    //   <type> <weight per square unit> 0 <hardness> 0
    //
    // The extra row is cosmetic (texture scale/shininess).
    let armour_row = lines.floats()?;
    if armour_row.len() >= 4 {
        ch.armour = Some(Armour {
            type_id: armour_row[0] as i32,
            weight: armour_row[1],
            hardness: armour_row[3],
        });
    }
    if has_model {
        let row = lines.floats()?;
        if row.len() >= 5 {
            ch.armour_material = Some([row[0], row[1], row[2], row[3], row[4]]);
        }
    }
    // Buckling points exist from 1.11 onward (absent in 1.07/1.08). Each is a
    // position plus the mesh vertices that meet there (F9).
    if ver > (1, 8) {
        let bpn = lines.i32()?;
        for _ in 0..bpn {
            let position = lines.vec3()?;
            let ic = lines.i32()?;
            let mut indices = Vec::new();
            for _ in 0..ic {
                indices.push(lines.i32()? as u32);
            }
            ch.buckling.push(BucklePoint { position, indices });
        }
    }
    Ok(ch)
}

/// Parse a `major.minor` version string; defaults to 1.12 when unparseable.
fn parse_version(s: &str) -> (i32, i32) {
    let mut it = s.trim().split('.');
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => (
            a.trim().parse().unwrap_or(1),
            b.trim().parse().unwrap_or(12),
        ),
        _ => (1, 12),
    }
}

fn strip_label(line: &str, label: &str) -> String {
    line.strip_prefix(label).unwrap_or(line).to_string()
}

// ---------------------------------------------------------------------------
// Writer

impl Bot {
    /// Write the robot in the game's own text form.
    ///
    /// The file-format version is [`Bot::version`]; a default `Bot` is 1.12.
    /// Sections that only exist in newer versions (the default-texture flag,
    /// buckle points, buckle model, heading) are emitted for the versions that
    /// have them, so an old file written back stays an old file.
    pub fn write(&self) -> Vec<u8> {
        let ver = parse_version(&self.version);
        let mut w = Writer::new(self.line_ending);

        w.line(&self.version);
        w.line(&format!("Name: {}", self.name));
        w.line(&format!("Class: {}", self.class));
        if let Some(default_texture) = self.default_texture {
            w.line(if default_texture { "1" } else { "0" });
        }
        w.line(if self.snapshot.is_some() {
            "true"
        } else {
            "false"
        });
        if let Some(snapshot) = &self.snapshot {
            w.bytes(snapshot);
        }
        w.line(&self.components.len().to_string());

        for c in &self.components {
            w.line(&c.parent.to_string());
            w.line(&c.parent_mount.to_string());
            w.line(&c.own_mount.to_string());
            w.floats(&c.translation);
            w.floats(&c.rotation);
            w.line(&format!("{} {}", num(c.angle), num(c.height)));
            w.line(&c.class);
            w.line(&c.file);
            match c.class.as_str() {
                "Chassis" => match c.chassis.as_ref() {
                    Some(ch) => w.chassis(ch, ver),
                    None => w.chassis(&Chassis::default(), ver),
                },
                "BurstMotor" => {
                    let a = c.burst_angles.unwrap_or([0.0, 0.0]);
                    w.line(&format!("{} {}", num(a[0]), num(a[1])));
                }
                "SmartZone" => w.line(c.smart_zone.as_deref().unwrap_or("")),
                _ => {}
            }
        }

        if ver > (1, 8) {
            match &self.buckle_model {
                Some(mesh) => {
                    w.line("true");
                    w.mesh(mesh);
                }
                None => w.line("false"),
            }
            w.line(&num(self.heading.unwrap_or(0.0)));
        }

        w.line(&self.controllers.len().to_string());
        for ctl in &self.controllers {
            w.line(&format!("Name: {}", ctl.name));
            w.line(&format!(
                "{} {} {}",
                ctl.position[0], ctl.position[1], ctl.position[2]
            ));
            w.line(&ctl.bindings.len().to_string());
            for b in &ctl.bindings {
                // The game's writer leaves a space after every code; 1.07 and
                // 1.08 store only the first three.
                if ver > (1, 8) {
                    w.line(&format!("{} {} {} {} ", b[0], b[1], b[2], b[3]));
                } else {
                    w.line(&format!("{} {} {} ", b[0], b[1], b[2]));
                }
            }
        }

        w.line(&self.bindings.len().to_string());
        for b in &self.bindings {
            w.line(&b.component.to_string());
            w.line(&b.action);
            w.line(&format!(
                "{} {} {}",
                b.control[0], b.control[1], b.control[2]
            ));
        }
        w.finish()
    }

    /// Write the robot to `path`.
    pub fn write_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.write())
    }
}

impl Writer<'_> {
    fn chassis(&mut self, ch: &Chassis, ver: (i32, i32)) {
        self.line(&ch.bottom.len().to_string());
        for p in &ch.bottom {
            self.line(&format!("{} {}", p[0], p[1]));
        }
        self.line(&ch.top.len().to_string());
        for p in &ch.top {
            self.line(&format!("{} {}", p[0], p[1]));
        }
        self.line(&format!("{} {}", num(ch.height), ch.face_count));
        self.line(if ch.mesh.is_some() { "true" } else { "false" });
        if let Some(mesh) = &ch.mesh {
            self.mesh(mesh);
            self.line(if ch.texture_tga.is_some() {
                "true"
            } else {
                "false"
            });
            if let Some(tex) = &ch.texture_tga {
                self.bytes(tex);
            }
            if ver > (1, 7) {
                // The reader cannot decode a present unknown section, and no
                // shipped file sets the flag, so the writer always says no.
                self.line("false");
            }
            self.line(&ch.face_groups.len().to_string());
            for g in &ch.face_groups {
                self.line(&g.coords.len().to_string());
                for uv in &g.coords {
                    self.line(&format!("{} {}", num(uv[0]), num(uv[1])));
                }
            }
            self.line(&ch.face_groups.len().to_string());
            for (i, g) in ch.face_groups.iter().enumerate() {
                self.line(&format!(
                    "{} {} {} {} {}",
                    i, g.bounds[0], g.bounds[1], g.bounds[2], g.bounds[3]
                ));
            }
            self.line(&ch.faces.len().to_string());
            for f in &ch.faces {
                self.line(&format!(
                    "{} {} {} {}",
                    f.group, f.points[0], f.points[1], f.points[2]
                ));
            }
            self.line(&ch.top_corners.len().to_string());
            for c in &ch.top_corners {
                // The `3` is the corner's point count; the writer pads the
                // line with a space, as the game does.
                self.line(&format!("{} 3 {} {} {} ", c[0], c[1], c[2], c[3]));
            }
        }
        let armour = ch.armour.unwrap_or_default();
        self.line(&format!(
            "{} {} 0 {} 0",
            armour.type_id,
            num(armour.weight),
            num(armour.hardness)
        ));
        if ch.mesh.is_some() {
            let row = ch.armour_material.unwrap_or_else(|| armour.material_row());
            self.line(&row.iter().map(|v| num(*v)).collect::<Vec<_>>().join(" "));
        }
        if ver > (1, 8) {
            self.line(&ch.buckling.len().to_string());
            for b in &ch.buckling {
                self.floats(&b.position);
                self.line(&b.indices.len().to_string());
                for i in &b.indices {
                    self.line(&i.to_string());
                }
            }
        }
    }

    /// A 3D block: header, model header, `RAW`, normals, UVs, points, indices.
    fn mesh(&mut self, mesh: &BotMesh) {
        let points = mesh.points.len();
        let indices = mesh.indices.len();
        self.line(&format!("274 {points} {indices} {}", indices / 3));
        self.line("1");
        self.line(&format!("{indices} {} {points} 0", indices / 3));
        self.line("RAW");
        for i in 0..points {
            let n = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            self.floats(&n);
        }
        for i in 0..points {
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            self.floats(&uv);
        }
        for p in &mesh.points {
            self.floats(p);
        }
        for i in &mesh.indices {
            self.line(&i.to_string());
        }
    }
}

struct Writer<'a> {
    out: Vec<u8>,
    e: &'a str,
}

impl<'a> Writer<'a> {
    fn new(line_ending: LineEnding) -> Self {
        Writer {
            out: Vec::new(),
            e: line_ending.s(),
        }
    }

    fn line(&mut self, s: &str) {
        self.out.extend_from_slice(s.as_bytes());
        self.out.extend_from_slice(self.e.as_bytes());
    }

    fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    fn floats(&mut self, v: &[f32]) {
        let s = v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" ");
        self.line(&s);
    }

    fn finish(self) -> Vec<u8> {
        self.out
    }
}

/// The game's number format: 6 significant digits (`%g`), Microsoft-style
/// three-digit exponents (`-2.38419e-007`), when that is lossless for the f32.
/// Falling back to Rust's shortest form keeps a hand-edited float exact.
fn num(v: f32) -> String {
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let candidate = g6(v);
    match candidate.parse::<f32>() {
        Ok(back) if back.to_bits() == v.to_bits() => candidate,
        _ => v.to_string(),
    }
}

/// `%g` with precision 6, minus the `printf` locale.
fn g6(v: f32) -> String {
    let sci = format!("{:.5e}", v.abs());
    let exp = sci
        .rsplit_once('e')
        .and_then(|(_, e)| e.parse::<i32>().ok())
        .unwrap_or(0);
    if !(-4..6).contains(&exp) {
        let (mant, _) = sci.split_once('e').unwrap_or((&sci, ""));
        let mant = mant.trim_end_matches('0').trim_end_matches('.');
        format!(
            "{}{}e{}{:03}",
            if v < 0.0 { "-" } else { "" },
            mant,
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        )
    } else {
        let decimals = (5 - exp).max(0) as usize;
        let s = format!("{v:.decimals$}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() || s == "-" {
            "0".into()
        } else {
            s.to_string()
        }
    }
}

// ---------------------------------------------------------------------------

struct Lines<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Lines<'a> {
    fn line(&mut self) -> &'a [u8] {
        let start = self.p;
        let end = match self.d[self.p..].iter().position(|&b| b == b'\n') {
            Some(off) => self.p + off,
            None => self.d.len(),
        };
        self.p = (end + 1).min(self.d.len());
        let mut s = &self.d[start..end];
        if s.last() == Some(&b'\r') {
            s = &s[..s.len() - 1];
        }
        s
    }

    fn str_line(&mut self) -> String {
        String::from_utf8_lossy(self.line()).into_owned()
    }

    fn i32(&mut self) -> Result<i32, Error> {
        let l = self.line();
        let tok = l
            .split(|b| *b == b' ')
            .find(|s| !s.is_empty())
            .unwrap_or(b"0");
        let s = std::str::from_utf8(tok).map_err(|_| Error::Structure("bad int token".into()))?;
        s.parse::<f32>()
            .map(|f| f as i32)
            .map_err(|e| Error::Structure(format!("bad int {s:?}: {e}")))
    }

    fn ints(&mut self) -> Result<Vec<i32>, Error> {
        let l = self.line();
        l.split(|b| *b == b' ')
            .filter(|s| !s.is_empty())
            .map(|tok| {
                let s = std::str::from_utf8(tok).unwrap_or("0");
                s.parse::<f32>()
                    .map(|f| f as i32)
                    .map_err(|e| Error::Structure(format!("bad int {s:?}: {e}")))
            })
            .collect()
    }

    fn floats(&mut self) -> Result<Vec<f32>, Error> {
        let l = self.line();
        l.split(|b| *b == b' ')
            .filter(|s| !s.is_empty())
            .map(|tok| {
                let s = std::str::from_utf8(tok).unwrap_or("0");
                s.parse::<f32>()
                    .map_err(|e| Error::Structure(format!("bad float {s:?}: {e}")))
            })
            .collect()
    }

    fn vec3(&mut self) -> Result<[f32; 3], Error> {
        let v = self.floats()?;
        Ok([
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
            v.get(2).copied().unwrap_or(0.0),
        ])
    }

    fn vec4(&mut self) -> Result<[f32; 4], Error> {
        let v = self.floats()?;
        Ok([
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
            v.get(2).copied().unwrap_or(0.0),
            v.get(3).copied().unwrap_or(0.0),
        ])
    }

    /// Return the raw bytes of a TGA at the cursor, advancing past it.
    fn take_tga(&mut self, expect: Option<(u16, u16)>) -> Result<&'a [u8], Error> {
        let h = self
            .d
            .get(self.p..self.p + 18)
            .ok_or(Error::UnexpectedEof {
                at: self.p,
                wanted: 18,
            })?;
        let idlen = h[0] as usize;
        let itype = h[2];
        let w = u16::from_le_bytes([h[12], h[13]]);
        let ht = u16::from_le_bytes([h[14], h[15]]);
        let bpp = h[16] as usize;
        if itype != 2 && itype != 3 {
            return Err(Error::Structure(format!("unsupported TGA type {itype}")));
        }
        if let Some((ew, eh)) = expect {
            if (w, ht) != (ew, eh) {
                return Err(Error::Structure(format!("TGA {w}x{ht} != {ew}x{eh}")));
            }
        }
        let size = 18 + idlen + (w as usize) * (ht as usize) * (bpp / 8);
        let bytes = self
            .d
            .get(self.p..self.p + size)
            .ok_or(Error::UnexpectedEof {
                at: self.p,
                wanted: size,
            })?;
        self.p += size;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game files live outside the repository; CI has none.
    fn game_dir() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = std::path::PathBuf::from(dir);
            return dir.is_dir().then_some(dir);
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    fn bots_in(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                bots_in(&path, out);
            } else if path.extension().is_some_and(|e| e == "bot") {
                out.push(path);
            }
        }
    }

    /// Every shipped `.bot` — the designs, the AI teams' rosters and the
    /// player's team — parses, writes and parses back to the same model, and
    /// the bytes are the original's own.
    #[test]
    fn shipped_bots_round_trip_semantically_and_byte_for_byte() {
        let Some(game) = game_dir() else {
            eprintln!("skipping .bot corpus test: no local/game (set RA2_GAME to run it)");
            return;
        };
        let mut files = Vec::new();
        for sub in ["Robot Designs", "AI", "Teams"] {
            let dir = game.join(sub);
            if dir.is_dir() {
                bots_in(&dir, &mut files);
            }
        }
        files.sort();
        assert!(
            files.len() >= 57,
            "expected the shipped .bot corpus, found {} files",
            files.len()
        );
        let mut identical = Vec::new();
        let mut differing = Vec::new();
        for path in &files {
            let data = std::fs::read(path).unwrap();
            let bot = parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let written = bot.write();
            let back = parse(&written)
                .unwrap_or_else(|e| panic!("{}: written form does not parse: {e}", path.display()));
            assert_eq!(back, bot, "semantic round-trip of {}", path.display());
            if written == data {
                identical.push(path.clone());
            } else {
                differing.push(path.clone());
            }
        }
        assert!(
            differing.is_empty(),
            "{}/{} bots differ after write: {:?}",
            differing.len(),
            files.len(),
            differing
                .iter()
                .take(3)
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        );
        eprintln!(
            "{} shipped bots: {} semantic round-trips, {}/{} byte-identical",
            files.len(),
            identical.len() + differing.len(),
            identical.len(),
            files.len()
        );
    }

    /// A bot built from nothing writes a file the reader accepts, in 1.12.
    #[test]
    fn a_hand_built_bot_writes_and_reads_back() {
        let mut bot = Bot {
            name: "Test Bot".into(),
            class: 0,
            ..Default::default()
        };
        bot.components.push(Component {
            parent: -1,
            rotation: [0.0, 0.0, 0.0, 1.0],
            class: "Chassis".into(),
            chassis: Some(Chassis {
                height: 0.5,
                bottom: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
                top: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
                armour: Some(Armour::default()),
                ..Default::default()
            }),
            ..Default::default()
        });
        bot.components.push(Component {
            parent: 0,
            parent_mount: 1,
            own_mount: 1,
            translation: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            class: "ControlBoard".into(),
            file: "Components\\controlboard.txt".into(),
            ..Default::default()
        });
        bot.controllers.push(Controller {
            name: "Forward".into(),
            position: [0, 0, 4],
            bindings: vec![[0, 0, 57, 0]],
        });
        bot.bindings.push(Binding {
            component: 1,
            action: "Fire".into(),
            control: [0, 0, 0],
        });

        let written = bot.write();
        let text = String::from_utf8_lossy(&written);
        assert!(text.starts_with("1.12\nName: Test Bot\nClass: 0\n1\nfalse\n"));
        let back = parse(&written).unwrap();
        assert_eq!(back, bot);
        assert_eq!(back.components.len(), 2);
        assert_eq!(back.components[0].class, "Chassis");
        assert_eq!(back.controllers[0].name, "Forward");
        assert_eq!(back.bindings[0].action, "Fire");
    }

    /// Rows the reader accepts but does not use (the cosmetic armour row, a
    /// face entry naming a group that does not exist) survive a write.
    #[test]
    fn ignored_rows_survive_a_round_trip() {
        let mut ch = Chassis {
            height: 0.6,
            face_count: 1,
            mesh: Some(BotMesh {
                points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                normals: vec![[0.0, 1.0, 0.0]; 3],
                uvs: vec![[0.0, 0.0]; 3],
                indices: vec![0, 1, 2],
            }),
            face_groups: vec![FaceGroup {
                coords: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
                bounds: [0, 0, 256, 256],
            }],
            faces: vec![
                Face {
                    group: 0,
                    points: [0, 1, 2],
                },
                // Past the end of `face_groups`: the reader ignores it, the
                // writer must keep it.
                Face {
                    group: 7,
                    points: [2, 1, 0],
                },
            ],
            armour: Some(Armour {
                type_id: 2,
                weight: 20.0,
                hardness: 175.0,
            }),
            armour_material: Some([0.3332, 0.30005, 0.30005, 0.85, 10.0]),
            ..Default::default()
        };
        ch.top_corners.push([0, 0, 1, 2]);

        let bot = Bot {
            name: "Rows".into(),
            components: vec![Component {
                parent: -1,
                class: "Chassis".into(),
                chassis: Some(ch),
                ..Default::default()
            }],
            ..Default::default()
        };
        let written = bot.write();
        // The corner line's trailing space and the out-of-range face row are
        // in the bytes, not just the model.
        let text = String::from_utf8_lossy(&written);
        assert!(text.contains("\n0 3 0 1 2 \n"), "{text}");
        assert!(text.contains("\n7 2 1 0\n"), "{text}");
        assert!(
            text.contains("\n0.3332 0.30005 0.30005 0.85 10\n"),
            "{text}"
        );

        let back = parse(&written).unwrap();
        let ch = back.components[0].chassis.as_ref().unwrap();
        assert_eq!(ch.faces, bot.components[0].chassis.as_ref().unwrap().faces);
        assert_eq!(
            ch.armour_material,
            Some([0.3332, 0.30005, 0.30005, 0.85, 10.0])
        );
    }
}
