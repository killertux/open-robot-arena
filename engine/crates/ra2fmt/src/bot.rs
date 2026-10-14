//! Parser for RA2 `.bot` robot files.
//!
//! `.bot` files are newline-separated text with two embedded uncompressed TGA
//! blobs (a 128×128 snapshot and, for the chassis, a 256×256 texture). Layout
//! follows the community PyRA2 analysis and is validated against the shipped
//! bots (format v1.09+; the older 1.07/1.08 layout is not yet handled).

use crate::Error;

#[derive(Debug, Clone, Default)]
pub struct Bot {
    pub version: String,
    pub name: String,
    pub class: i32,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Default)]
pub struct Component {
    pub parent: i32,
    pub mount_on_component: i32,
    pub mount_on_parent: i32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4], // w x y z
    pub angle: f32,
    pub height: f32,
    pub class: String,
    pub file: String,
    pub chassis: Option<Chassis>,
    /// `BurstMotor` start/stop axle angles (radians), when present.
    pub burst_angles: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Default)]
pub struct Chassis {
    pub height: f32,
    pub mesh: Option<BotMesh>,
    /// Bottom-plate outline points (0..1000), the chassis footprint.
    pub bottom: Vec<[i32; 2]>,
    /// Top-plate outline points (0..1000).
    pub top: Vec<[i32; 2]>,
    /// Raw 256×256 uncompressed TGA bytes (chassis skin), if present.
    pub texture_tga: Option<Vec<u8>>,
}

/// A mesh embedded directly in a `.bot` (chassis shape).
#[derive(Debug, Clone, Default)]
pub struct BotMesh {
    pub points: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Triangle indices.
    pub indices: Vec<u32>,
}

/// Parse `.bot` bytes; only the header and component list are required.
pub fn parse(data: &[u8]) -> Result<Bot, Error> {
    let mut lines = Lines { d: data, p: 0 };
    let version = lines.str_line();
    let name = lines.str_line();
    let class_line = String::from_utf8_lossy(lines.line()).into_owned();
    let class = class_line
        .rsplit([' ', '\t'])
        .next()
        .unwrap_or("0")
        .parse::<i32>()
        .unwrap_or(0);
    let ver = parse_version(&version);
    let mut bot = Bot {
        version,
        name,
        class,
        ..Default::default()
    };
    // optional default-texture flag (absent in older formats)
    let x = lines.line();
    let snap = match x {
        b"true" => true,
        b"false" => false,
        _ => lines.line() == b"true",
    };
    if snap {
        lines.skip_tga(Some((128, 128)))?;
    }

    let n = lines.ints()?.first().copied().unwrap_or(0);
    for _ in 0..n {
        let mut c = Component {
            parent: lines.i32()?,
            mount_on_component: lines.i32()?,
            mount_on_parent: lines.i32()?,
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
                lines.line();
            }
            _ => {}
        }
        bot.components.push(c);
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
            lines.line(); // normal
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
            for _ in 0..npts {
                lines.line();
            }
        }
        let ng2 = lines.i32()?;
        for _ in 0..ng2 {
            lines.line();
        }
        let nfe = lines.i32()?;
        for _ in 0..nfe {
            lines.line();
        }
        let nce = lines.i32()?;
        for _ in 0..nce {
            lines.line();
        }
    }
    // Armour: a type row, plus an extra row when a 3D model is present.
    lines.line(); // armour row
    if has_model {
        lines.line(); // armour extra row
    }
    // Buckling points exist from 1.11 onward (absent in 1.07/1.08).
    if ver > (1, 8) {
        let bpn = lines.i32()?;
        for _ in 0..bpn {
            lines.line();
            let ic = lines.i32()?;
            for _ in 0..ic {
                lines.line();
            }
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

    fn skip_tga(&mut self, expect: Option<(u16, u16)>) -> Result<(), Error> {
        self.take_tga(expect).map(|_| ())
    }
}
