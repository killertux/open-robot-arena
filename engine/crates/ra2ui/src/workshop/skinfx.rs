//! The Paint Shop's skin renderer: the original draws every appearance effect as textured quads
//! into a 256 x 256 render target (`FUN_005b2ce0` opens it, `FUN_005b2e70` draws one quad,
//! `FUN_005b2cb0` closes it) and keeps the target as the chassis texture. This module is the same
//! drawing on the CPU.
//!
//! * [`Canvas::quad`] is `FUN_005b2e70`: four corners (screen pixels) with four texture coordinates,
//!   drawn as the triangles (A, B, C) and (B, D, C), the texture sampled bilinearly at the pixel
//!   centre and blended `SRCALPHA / INVSRCALPHA` over the target.
//! * [`Canvas::fill`] is the full-skin quad of the armour default: corners (0,0) (256,0) (0,256)
//!   (256,256) with the uv square.
//! * [`Canvas::edges`] is `FUN_0044f690` / `FUN_0044f6f0`: for every panel polygon of the skin and
//!   every edge of it from `P0` to `P1` a strip centred on the edge, `width` pixels to each side,
//!   whose texture coordinate runs `0 .. length x ratio` along the edge and 0 .. 1 across it.
//! * [`default_appearance`] is the armour pick (`FUN_0043ca70` / `0x43ccc5`): per armour type the
//!   fill texture(s) and the edge strips the original draws.

use std::path::Path;

use super::paint::Skin;

/// An RGBA image in floats, row-major, top-left origin: a texture or the render target.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub px: Vec<[f32; 4]>,
}

impl Image {
    pub fn load(path: &Path) -> Option<Self> {
        let img = image::open(path).ok()?.to_rgba8();
        Some(Image {
            width: img.width() as usize,
            height: img.height() as usize,
            px: img
                .pixels()
                .map(|p| {
                    [
                        p[0] as f32 / 255.0,
                        p[1] as f32 / 255.0,
                        p[2] as f32 / 255.0,
                        p[3] as f32 / 255.0,
                    ]
                })
                .collect(),
        })
    }

    pub fn from_skin(skin: &Skin) -> Self {
        Image {
            width: skin.width as usize,
            height: skin.height as usize,
            px: skin
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .map(|p| {
                    [
                        p[0] as f32 / 255.0,
                        p[1] as f32 / 255.0,
                        p[2] as f32 / 255.0,
                        p[3] as f32 / 255.0,
                    ]
                })
                .collect(),
        }
    }

    pub fn to_skin(&self) -> Skin {
        let mut pixels = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            for c in p {
                pixels.push((c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
            }
        }
        Skin {
            width: self.width as u32,
            height: self.height as u32,
            pixels,
        }
    }

    /// Bilinear sample at texture coordinates `u`, `v`, wrapping (texel centres at `(i + 0.5) / n`).
    fn sample(&self, u: f32, v: f32, wrap: bool) -> [f32; 4] {
        if !wrap {
            let xi = ((u * self.width as f32).floor() as i64).clamp(0, self.width as i64 - 1);
            let yi = ((v * self.height as f32).floor() as i64).clamp(0, self.height as i64 - 1);
            return self.px[yi as usize * self.width + xi as usize];
        }
        let x = u * self.width as f32 - 0.5;
        let y = v * self.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let at = |xi: f32, yi: f32| {
            let (w, h) = (self.width as i64, self.height as i64);
            let (mut xi, mut yi) = (xi as i64, yi as i64);
            if wrap {
                xi = xi.rem_euclid(w);
                yi = yi.rem_euclid(h);
            } else {
                xi = xi.clamp(0, w - 1);
                yi = yi.clamp(0, h - 1);
            }
            self.px[(yi * w + xi) as usize]
        };
        let (a, b, c, d) = (
            at(x0, y0),
            at(x0 + 1.0, y0),
            at(x0, y0 + 1.0),
            at(x0 + 1.0, y0 + 1.0),
        );
        let mut out = [0.0; 4];
        for i in 0..4 {
            out[i] = a[i] * (1.0 - fx) * (1.0 - fy)
                + b[i] * fx * (1.0 - fy)
                + c[i] * (1.0 - fx) * fy
                + d[i] * fx * fy;
        }
        out
    }
}

/// The render target.
pub struct Canvas {
    pub img: Image,
}

type P = [f32; 2];

fn edge_fn(a: P, b: P, p: P) -> f32 {
    (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
}

impl Canvas {
    pub fn new(skin: &Skin) -> Self {
        Canvas {
            img: Image::from_skin(skin),
        }
    }

    pub fn finish(self) -> Skin {
        self.img.to_skin()
    }

    /// One textured triangle, alpha blended.
    fn triangle(&mut self, tex: &Image, wrap: bool, pos: [P; 3], uv: [P; 3]) {
        let mut area = edge_fn(pos[0], pos[1], pos[2]);
        if area == 0.0 {
            return;
        }
        // a pixel centre exactly on a shared edge belongs to one triangle only (the top-left rule)
        let mut pos = pos;
        let mut uv = uv;
        if area < 0.0 {
            pos.swap(1, 2);
            uv.swap(1, 2);
            area = -area;
        }
        let owns = |a: P, b: P| b[1] > a[1] || (b[1] == a[1] && b[0] < a[0]);
        let own = [
            owns(pos[1], pos[2]),
            owns(pos[2], pos[0]),
            owns(pos[0], pos[1]),
        ];
        let min_x = pos
            .iter()
            .map(|p| p[0])
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_x = (pos
            .iter()
            .map(|p| p[0])
            .fold(f32::MIN, f32::max)
            .ceil()
            .max(0.0) as usize)
            .min(self.img.width);
        let min_y = pos
            .iter()
            .map(|p| p[1])
            .fold(f32::MAX, f32::min)
            .floor()
            .max(0.0) as usize;
        let max_y = (pos
            .iter()
            .map(|p| p[1])
            .fold(f32::MIN, f32::max)
            .ceil()
            .max(0.0) as usize)
            .min(self.img.height);
        for y in min_y..max_y {
            for x in min_x..max_x {
                // a clamped stage (flag 0 of `FUN_005b1e60`) is point sampled at the pixel's corner (live
                // `Export Paint Template` of a texture fill: `floor(x * 256 / width)` exactly), a wrapped one
                // (flag 1) bilinear at the pixel centre
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let w0 = edge_fn(pos[1], pos[2], p) / area;
                let w1 = edge_fn(pos[2], pos[0], p) / area;
                let w2 = edge_fn(pos[0], pos[1], p) / area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                if (w0 == 0.0 && !own[0]) || (w1 == 0.0 && !own[1]) || (w2 == 0.0 && !own[2]) {
                    continue;
                }
                let (w0, w1, w2) = if wrap {
                    (w0, w1, w2)
                } else {
                    let q = [x as f32, y as f32];
                    (
                        edge_fn(pos[1], pos[2], q) / area,
                        edge_fn(pos[2], pos[0], q) / area,
                        edge_fn(pos[0], pos[1], q) / area,
                    )
                };
                let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
                let v = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
                let s = tex.sample(u, v, wrap);
                let d = &mut self.img.px[y * self.img.width + x];
                let a = s[3];
                for i in 0..3 {
                    d[i] = ((s[i] * a + d[i] * (1.0 - a)) * 255.0 + 0.5).floor() / 255.0;
                }
            }
        }
    }

    /// `FUN_005b2e70`: the quad A B / C D (screen pixels, uv per corner), two triangles.
    pub fn quad(&mut self, tex: &Image, wrap: bool, pos: [P; 4], uv: [P; 4]) {
        self.triangle(tex, wrap, [pos[0], pos[1], pos[2]], [uv[0], uv[1], uv[2]]);
        self.triangle(tex, wrap, [pos[1], pos[3], pos[2]], [uv[1], uv[3], uv[2]]);
    }

    /// The texture over the whole skin (uv 0..1).
    pub fn fill(&mut self, tex: &Image) {
        let (w, h) = (self.img.width as f32, self.img.height as f32);
        self.quad(
            tex,
            false,
            [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        );
    }

    /// `FUN_0044f690`: the edge strips of every panel polygon (the *Edge Entire Chassis* button and
    /// the armour default).
    pub fn edges(&mut self, tex: &Image, polys: &[Vec<P>], width: f32, ratio: Option<f32>) {
        for (index, poly) in polys.iter().enumerate() {
            self.edge_strips(tex, poly, index, width, ratio);
        }
    }

    /// `FUN_0044f6f0`: the strips along every edge of panel polygon number `index`. The original
    /// walks a polygon's vertices backwards (edge `i + 1` to `i`) and works in pixel-centre
    /// coordinates (the polygon is shifted by half a pixel onto the skin's grid). A strip is
    /// centred on the edge, `width` pixels to each side; its texture coordinate runs
    /// `0 .. length x ratio` along the edge (`0 .. 1` when `ratio` is `None`) and 0 .. 1 across it,
    /// the across direction flipped for every panel but the first.
    pub fn edge_strips(
        &mut self,
        tex: &Image,
        poly: &[P],
        index: usize,
        width: f32,
        ratio: Option<f32>,
    ) {
        let n = poly.len();
        for i in 0..n {
            let at = |k: usize| [poly[k][0] + 0.5, poly[k][1] + 0.5];
            let (p0, p1) = (at((i + 1) % n), at(i));
            let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
            let len = dx.hypot(dy);
            if len == 0.0 {
                continue;
            }
            let (nx, ny) = (-dy / len * width, dx / len * width);
            let a = [p0[0] - nx, p0[1] - ny];
            let b = [p1[0] - nx, p1[1] - ny];
            let c = [p0[0] + nx, p0[1] + ny];
            let d = [p1[0] + nx, p1[1] + ny];
            let ul = ratio.map_or(1.0, |r| len * r);
            let uv = if index == 0 {
                [[0.0, 0.0], [ul, 0.0], [0.0, 1.0], [ul, 1.0]]
            } else {
                [[0.0, 1.0], [ul, 1.0], [0.0, 0.0], [ul, 0.0]]
            };
            self.quad(tex, true, [a, b, c, d], uv);
        }
    }

    /// `FUN_00424850` with a colour: the rectangle `x1 y1 x2 y2` in one opaque colour.
    pub fn solid_rect(&mut self, r: [i32; 4], rgb: [u8; 3]) {
        let c = [
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
        ];
        let w = self.img.width as i32;
        let h = self.img.height as i32;
        for y in r[1].max(0)..r[3].min(h) {
            for x in r[0].max(0)..r[2].min(w) {
                let d = &mut self.img.px[(y * w + x) as usize];
                d[..3].copy_from_slice(&c);
            }
        }
    }

    /// `FUN_00424850` with a texture: the rectangle's four corners with the uv square (the
    /// texture stretched over it, alpha blended).
    pub fn texture_rect(&mut self, tex: &Image, r: [i32; 4]) {
        let (x1, y1, x2, y2) = (r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32);
        self.quad(
            tex,
            false,
            [[x1, y1], [x2, y1], [x1, y2], [x2, y2]],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        );
    }

    /// `FUN_00426130`: a decal, a square of `side` pixels turned by `angle` about `centre`, drawn
    /// with the texture's own alpha and kept only inside the panel's rectangle `clip` (the original
    /// copies the four bands around the rectangle back from the skin).
    pub fn decal(&mut self, tex: &Image, centre: P, side: f32, angle: f32, clip: [i32; 4]) {
        let before = self.img.px.clone();
        let h = side * 0.5;
        let (s, c) = angle.sin_cos();
        let corner = |x: f32, y: f32| [centre[0] + x * c - y * s, centre[1] + x * s + y * c];
        self.quad(
            tex,
            true,
            [corner(-h, -h), corner(h, -h), corner(-h, h), corner(h, h)],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        );
        let w = self.img.width as i32;
        for y in 0..self.img.height as i32 {
            for x in 0..w {
                if x < clip[0] || x >= clip[2] || y < clip[1] || y >= clip[3] {
                    self.img.px[(y * w + x) as usize] = before[(y * w + x) as usize];
                }
            }
        }
    }
}

/// The 24-bit bottom-up `.bmp` the original writes for an exported skin (`Export Paint Template`):
/// the 54-byte header, the rows as blue, green, red, and two zero bytes after the last row (the file
/// is 196 664 bytes for 256 x 256).
pub fn to_bmp(skin: &Skin) -> Vec<u8> {
    let (w, h) = (skin.width as usize, skin.height as usize);
    let data = w * h * 3;
    let mut out = Vec::with_capacity(54 + data + 2);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + data + 2) as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&2834i32.to_le_bytes());
    out.extend_from_slice(&2834i32.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    for y in (0..h).rev() {
        for x in 0..w {
            let p = &skin.pixels[(y * w + x) * 4..];
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// The blank paint template: red, with every panel's polygon outlined in green four pixels wide
/// (live `botlab-e2e` export: 6751 green pixels; this strip rule agrees on 93 % of the line pixels).
pub fn template(polys: &[Vec<P>]) -> Skin {
    let mut skin = Skin::new(256, 256, [255, 0, 0, 255]);
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (p0, p1) = (poly[i], poly[(i + 1) % n]);
            let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
            let len = dx.hypot(dy);
            if len == 0.0 {
                continue;
            }
            let (ux, uy) = (dx / len, dy / len);
            for y in 0..256i32 {
                for x in 0..256i32 {
                    let (qx, qy) = (x as f32 + 0.5 - p0[0], y as f32 + 0.5 - p0[1]);
                    let (a, b) = (qx * ux + qy * uy, -qx * uy + qy * ux);
                    if (0.0..=len).contains(&a) && b.abs() <= 2.0 {
                        let i = ((y * 256 + x) * 4) as usize;
                        skin.pixels[i..i + 4].copy_from_slice(&[0, 255, 0, 255]);
                    }
                }
            }
        }
    }
    skin
}

/// One drawing step of an armour's default appearance.
pub enum Step {
    /// The texture over the whole skin: a file under `Texture Library/`.
    Fill(&'static str),
    /// An edge strip: a file under `Texture Library/edges/`, half width, uv per pixel.
    Edge(&'static str, f32, f32),
}

/// The default appearance the armour pick draws (`0x43ccc5`, one case per armour type).
pub fn default_steps(type_id: i32) -> &'static [Step] {
    match type_id {
        0 => &[
            Step::Fill("red_plastic.bmp"),
            Step::Edge("light_edge.tga", 3.0, 0.5),
        ],
        1 => &[
            Step::Fill("aluminum_horiz.bmp"),
            Step::Edge("dirty_edge.tga", 14.0, 0.015),
            Step::Edge("light_edge.tga", 3.0, 0.5),
            Step::Edge("rivets.tga", 5.0, 0.1),
        ],
        2 => &[
            Step::Fill("gray_metal.bmp"),
            Step::Fill("diamondplate.tga"),
            Step::Edge("shiny_edge.tga", 5.0, 0.5),
        ],
        3 => &[
            Step::Fill("light_metal.bmp"),
            Step::Edge("rim_line.tga", 4.0, 0.25),
        ],
        _ => &[],
    }
}

/// The skin an armour pick paints: the steps drawn onto the target (the render target starts as the
/// skin it replaces; every first step is a full opaque fill). `polys` are the panels' polygons in
/// skin pixels.
pub fn default_appearance(root: &Path, type_id: i32, polys: &[Vec<P>], skin: &Skin) -> Skin {
    let mut canvas = Canvas::new(skin);
    for step in default_steps(type_id) {
        match step {
            Step::Fill(f) => {
                if let Some(t) = Image::load(&root.join("Texture Library").join(f)) {
                    canvas.fill(&t);
                }
            }
            Step::Edge(f, w, r) => {
                if let Some(t) = Image::load(&root.join("Texture Library/edges").join(f)) {
                    canvas.edges(&t, polys, *w, Some(*r));
                }
            }
        }
    }
    canvas.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn local_dir(sub: &str) -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join(sub);
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn solid(w: usize, h: usize, c: [f32; 4]) -> Image {
        Image {
            width: w,
            height: h,
            px: vec![c; w * h],
        }
    }

    #[test]
    fn a_full_quad_of_the_same_size_copies_the_texture_exactly() {
        let mut tex = solid(8, 8, [0.0, 0.0, 0.0, 1.0]);
        for (i, p) in tex.px.iter_mut().enumerate() {
            let v = (i * 4) as f32 / 255.0;
            *p = [v, 128.0 / 255.0, 1.0 - v, 1.0];
        }
        let mut canvas = Canvas {
            img: solid(8, 8, [1.0, 1.0, 1.0, 1.0]),
        };
        canvas.fill(&tex);
        for (a, b) in canvas.img.px.iter().zip(&tex.px) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn the_shared_diagonal_of_a_quad_is_blended_once() {
        // half transparent white over black: once is 0.5, twice would be 0.75
        let tex = solid(4, 4, [1.0, 1.0, 1.0, 0.5]);
        let mut canvas = Canvas {
            img: solid(4, 4, [0.0, 0.0, 0.0, 1.0]),
        };
        canvas.fill(&tex);
        for p in &canvas.img.px {
            assert!((p[0] - 0.5).abs() < 0.01, "{p:?}");
        }
    }

    #[test]
    fn a_decal_stays_inside_the_clip_rectangle() {
        let mut canvas = Canvas {
            img: solid(32, 32, [0.0, 0.0, 0.0, 1.0]),
        };
        canvas.decal(
            &solid(4, 4, [1.0, 0.0, 0.0, 1.0]),
            [16.0, 16.0],
            40.0,
            0.3,
            [8, 8, 24, 24],
        );
        assert_eq!(canvas.img.px[16 * 32 + 16][0], 1.0);
        assert_eq!(canvas.img.px[2 * 32 + 2][0], 0.0);
        assert_eq!(canvas.img.px[30 * 32 + 30][0], 0.0);
    }

    /// The four armour appearances against the skins the original wrote
    /// (`botlab-e2e-armor-*`: type 0 plastic, 1 aluminium, 2 steel, 3 titanium).
    #[test]
    fn the_armour_defaults_match_the_originals_skins() {
        let (Some(game), Some(oracle)) =
            (local_dir("local/game"), local_dir("local/oracle/original"))
        else {
            return;
        };
        for (file, type_id) in [
            ("plastic", 0),
            ("aluminum", 1),
            ("titanium", 2),
            ("steel", 3),
        ] {
            let path = oracle.join(format!("botlab-e2e-armor-{file}__live__000.bot"));
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let bot = ra2fmt::bot::parse(&bytes).unwrap();
            let chassis = bot.components[0].chassis.as_ref().unwrap();
            let live = Skin::from_tga(chassis.texture_tga.as_ref().unwrap()).unwrap();
            let polys: Vec<Vec<P>> = chassis
                .face_groups
                .iter()
                .map(|g| g.coords.clone())
                .collect();
            let white = Skin::new(256, 256, [255; 4]);
            let ours = default_appearance(&game, type_id, &polys, &white);
            let sum: u64 = ours
                .pixels
                .iter()
                .zip(&live.pixels)
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64)
                .sum();
            let mae = sum as f64 / ours.pixels.len() as f64;
            assert!(mae < 0.2, "{file}: mean abs difference {mae}");
        }
    }

    /// The live decal against the live armour skin (`decalband-export-*`): one `bolt.tga` decal at size
    /// 1 (80 px) on the top face's panel `R = (95, 165, 179, 249)`. The original's four `CopyRects`
    /// (`FUN_005b30a0`) did not take the pixels outside `R` back in the live run: the nut runs past
    /// `R` on three sides (docs/36 section 9.33).
    #[test]
    fn the_decal_matches_the_live_export_without_the_band_restore() {
        let (Some(game), Some(oracle)) =
            (local_dir("local/game"), local_dir("local/oracle/original"))
        else {
            return;
        };
        let load = |name: &str| Image::load(&oracle.join(format!("{name}__live__000.bmp")));
        let (Some(base), Some(live), Some(bolt)) = (
            load("decalband-export-armour"),
            load("decalband-export-decal"),
            Image::load(&game.join("Texture Library/decals/bolt.tga")),
        ) else {
            return;
        };
        let mae = |clip: [i32; 4]| {
            let mut canvas = Canvas::new(&base.to_skin());
            canvas.decal(&bolt, [153.5, 183.0], 80.0, 0.0, clip);
            let ours = canvas.finish();
            let live = live.to_skin();
            let sum: u64 = ours
                .pixels
                .iter()
                .zip(&live.pixels)
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64)
                .sum();
            sum as f64 / ours.pixels.len() as f64
        };
        let (clipped, free) = (mae([95, 165, 179, 249]), mae([0, 0, 256, 256]));
        assert!(free < 0.2, "{free}");
        assert!(clipped > 5.0 * free, "{clipped} {free}");
    }
}
