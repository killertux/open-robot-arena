//! Paint: the chassis skin and the face groups that dress it.
//!
//! A `.bot` carries **one** 256×256 uncompressed TGA for the chassis and, next
//! to it, the face groups (F10). A group is one side of the chassis: a polygon
//! on the skin, its bounding rectangle, and the triangles that belong to it.
//! There is no per-face texture name in the file, so "assign a texture to a face
//! group" means compositing that texture into the group's rectangle of the skin;
//! the file then stores the one image, exactly as the game's writer does.
//!
//! The three paint screens are the three operations:
//!
//! - `texturefill.gib` — *"Click a panel on your chassis to paint it with this
//!   texture"*: [`Paint::fill_group`] replaces a group's patch;
//! - `surfacelayer.gib` — *"Click a panel on your chassis to add this texture
//!   layer"*: [`Paint::layer_group`] alpha-blends over the patch;
//! - `decals.gib` — *"Position the decal with your mouse and click to apply
//!   it"*, with a `scale slider`: [`Paint::decal_group`] blends a decal at a
//!   normalised point inside the group.
//!
//! The rect coordinates are the file's own (`x1 y1 x2 y2` in 0..256) and are
//! treated as top-left-origin pixels, which is how the renderer samples the
//! TGA. The compositing rules themselves (how the game stretches a texture or
//! sizes a decal) are **not** recovered from the binary; they are named as our
//! rule in `docs/15-workshop-model.md`.

use ra2fmt::bot::{Chassis, Face, FaceGroup};

/// The 256×256 skin size the bot format requires.
pub const SKIN_SIZE: u32 = 256;

/// Why a paint operation could not be performed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaintError {
    /// A face group index past the chassis' group list.
    NoGroup { index: usize, count: usize },
    /// The group's bounding rectangle is empty or invalid.
    NoBounds { index: usize },
    /// An image could not be decoded.
    Image(String),
}

impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaintError::NoGroup { index, count } => {
                write!(f, "face group {index} does not exist ({count} groups)")
            }
            PaintError::NoBounds { index } => {
                write!(f, "face group {index} has no bounding rectangle")
            }
            PaintError::Image(e) => write!(f, "image: {e}"),
        }
    }
}

impl std::error::Error for PaintError {}

/// An RGBA image, row-major, top-left origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Texture {
    /// Decode a `.bmp`/`.tga`/`.jpg`/`.png` file.
    pub fn load(path: &std::path::Path) -> Result<Self, PaintError> {
        let img = image::open(path)
            .map_err(|e| PaintError::Image(format!("{}: {e}", path.display())))?
            .to_rgba8();
        Ok(Texture {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            width: img.width(),
            height: img.height(),
            pixels: img.into_raw(),
        })
    }

    /// Decode an image from memory (a custom texture in any supported format).
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Result<Self, PaintError> {
        let format = image::guess_format(bytes).unwrap_or(image::ImageFormat::Tga);
        let img = image::load_from_memory_with_format(bytes, format)
            .map_err(|e| PaintError::Image(format!("{name}: {e}")))?
            .to_rgba8();
        Ok(Texture {
            name: name.to_string(),
            width: img.width(),
            height: img.height(),
            pixels: img.into_raw(),
        })
    }

    /// A flat colour, for tests and defaults.
    pub fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        Texture {
            name: "solid".into(),
            width,
            height,
            pixels: rgba
                .iter()
                .copied()
                .cycle()
                .take((width * height * 4) as usize)
                .collect(),
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Nearest-neighbour sample with `u`, `v` in `0..1`, wrapping.
    fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 0];
        }
        let x = (u.rem_euclid(1.0) * self.width as f32) as u32;
        let y = (v.rem_euclid(1.0) * self.height as f32) as u32;
        self.pixel(x.min(self.width - 1), y.min(self.height - 1))
    }
}

/// The chassis' 256×256 skin in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skin {
    pub width: u32,
    pub height: u32,
    /// RGBA, row-major, top-left origin.
    pub pixels: Vec<u8>,
}

impl Skin {
    pub fn new(width: u32, height: u32, rgba: [u8; 4]) -> Self {
        Skin {
            width,
            height,
            pixels: rgba
                .iter()
                .copied()
                .cycle()
                .take((width * height * 4) as usize)
                .collect(),
        }
    }

    /// Decode the bot's embedded TGA. The game writes 256×256 uncompressed
    /// 32-bit; anything the `image` crate reads is accepted.
    pub fn from_tga(bytes: &[u8]) -> Result<Self, PaintError> {
        // TGA has no magic number, so the format cannot be guessed from the
        // bytes the way PNG/JPEG can.
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Tga)
            .map_err(|e| PaintError::Image(format!("chassis texture: {e}")))?
            .to_rgba8();
        Ok(Skin {
            width: img.width(),
            height: img.height(),
            pixels: img.into_raw(),
        })
    }

    /// Encode an uncompressed 32-bit TGA with a top-left origin, the format the
    /// bot file embeds.
    pub fn to_tga(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(18 + (self.width * self.height * 4) as usize);
        out.extend_from_slice(&[
            0, // no id
            0, // no colour map
            2, // uncompressed true-colour
            0,
            0,
            0,
            0,
            0, // colour map spec
            0,
            0,
            0,
            0, // x/y origin
            (self.width & 0xff) as u8,
            (self.width >> 8) as u8,
            (self.height & 0xff) as u8,
            (self.height >> 8) as u8,
            32,   // bits per pixel
            0x28, // 8 alpha bits, top-left origin
        ]);
        for px in self.pixels.as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        out
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0, 0];
        }
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    /// Alpha-blend `src` over the pixel.
    fn blend_pixel(&mut self, x: u32, y: u32, src: [u8; 4]) {
        if src[3] == 0 {
            return;
        }
        if src[3] == 255 {
            self.set_pixel(x, y, src);
            return;
        }
        let dst = self.pixel(x, y);
        let a = src[3] as f32 / 255.0;
        let mix = |s: u8, d: u8| ((s as f32 * a) + (d as f32 * (1.0 - a))).round() as u8;
        self.set_pixel(
            x,
            y,
            [
                mix(src[0], dst[0]),
                mix(src[1], dst[1]),
                mix(src[2], dst[2]),
                255,
            ],
        );
    }
}

/// One face group, as the paint screen sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Index into the chassis' `face_groups` — the same index `Chassis.faces`
    /// names (the sim's flat, file-ordered list).
    pub index: usize,
    /// The group's rectangle on the skin: `x1, y1, x2, y2`.
    pub bounds: [i32; 4],
    /// How many of the file's triangles name this group.
    pub triangles: usize,
}

impl Group {
    fn rect(&self) -> Option<(i32, i32, i32, i32)> {
        let [x1, y1, x2, y2] = self.bounds;
        (x2 > x1 && y2 > y1).then_some((x1, y1, x2, y2))
    }
}

/// The chassis' paintable state: its skin, its groups and the face rows.
#[derive(Clone, Debug, Default)]
pub struct Paint {
    skin: Option<Skin>,
    /// The bytes the bot carried, kept so an untouched paint writes them back
    /// byte-for-byte.
    original: Option<Vec<u8>>,
    changed: bool,
    groups: Vec<Group>,
    faces: Vec<Face>,
    undo: Vec<Option<Skin>>,
    redo: Vec<Option<Skin>>,
}

impl Paint {
    /// Read a chassis' skin and face groups.
    pub fn from_chassis(chassis: &Chassis) -> Self {
        let original = chassis.texture_tga.clone();
        let skin = original.as_deref().and_then(|b| Skin::from_tga(b).ok());
        let groups = chassis
            .face_groups
            .iter()
            .enumerate()
            .map(|(index, g)| group(index, g, &chassis.faces))
            .collect();
        Paint {
            skin,
            original,
            changed: false,
            groups,
            faces: chassis.faces.clone(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The groups, in file order. `Chassis.faces[i].group` indexes this list.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Faces that name an existing group — and those the shipped files leave
    /// past the end (60 rows across the corpus), which the reader keeps and the
    /// renderer ignores.
    pub fn face_range_counts(&self) -> (usize, usize) {
        let in_range = self
            .faces
            .iter()
            .filter(|f| f.group >= 0 && (f.group as usize) < self.groups.len())
            .count();
        (in_range, self.faces.len() - in_range)
    }

    pub fn skin(&self) -> Option<&Skin> {
        self.skin.as_ref()
    }

    pub fn is_changed(&self) -> bool {
        self.changed
    }

    /// The group's rectangle, clamped to the skin.
    pub fn group_rect(&self, index: usize) -> Option<(i32, i32, i32, i32)> {
        let group = self.groups.get(index)?;
        let (x1, y1, x2, y2) = group.rect()?;
        let w = self
            .skin
            .as_ref()
            .map(|s| s.width as i32)
            .unwrap_or(SKIN_SIZE as i32);
        let h = self
            .skin
            .as_ref()
            .map(|s| s.height as i32)
            .unwrap_or(SKIN_SIZE as i32);
        Some((
            x1.clamp(0, w),
            y1.clamp(0, h),
            x2.clamp(0, w),
            y2.clamp(0, h),
        ))
    }

    fn ensure_skin(&mut self) -> &mut Skin {
        self.skin.get_or_insert_with(|| {
            // A neutral grey: a bot whose chassis carries no texture has no
            // original patch to preserve, and a decal needs something opaque.
            Skin::new(SKIN_SIZE, SKIN_SIZE, [128, 128, 128, 255])
        })
    }

    fn snapshot(&mut self) {
        self.undo.push(self.skin.clone());
        if self.undo.len() > 16 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Replace a group's patch with `texture`, stretched to the group rectangle.
    pub fn fill_group(&mut self, index: usize, texture: &Texture) -> Result<(), PaintError> {
        self.paint_group(index, texture, false)
    }

    /// Apply one texture to every group ("texture entire").
    pub fn fill_all(&mut self, texture: &Texture) -> Result<(), PaintError> {
        for index in 0..self.groups.len() {
            self.fill_group(index, texture)?;
        }
        Ok(())
    }

    /// Alpha-blend `texture` over a group's patch, stretched, without clearing
    /// what is there ("add this texture layer").
    pub fn layer_group(&mut self, index: usize, texture: &Texture) -> Result<(), PaintError> {
        self.paint_group(index, texture, true)
    }

    /// Stretch `texture` into a group. `blend` keeps the existing pixels under
    /// transparent texels; without it the texture replaces the patch (the game's
    /// *"paint it with this texture"*).
    fn paint_group(
        &mut self,
        index: usize,
        texture: &Texture,
        blend: bool,
    ) -> Result<(), PaintError> {
        self.check_group(index)?;
        let (x1, y1, x2, y2) = self
            .group_rect(index)
            .ok_or(PaintError::NoBounds { index })?;
        if x2 <= x1 || y2 <= y1 {
            return Err(PaintError::NoBounds { index });
        }
        self.snapshot();
        let skin = self.ensure_skin();
        for y in y1 as u32..y2 as u32 {
            for x in x1 as u32..x2 as u32 {
                let u = (x as f32 - x1 as f32) / (x2 - x1) as f32;
                let v = (y as f32 - y1 as f32) / (y2 - y1) as f32;
                let px = texture.sample(u, v);
                if blend {
                    skin.blend_pixel(x, y, px);
                } else {
                    skin.set_pixel(x, y, px);
                }
            }
        }
        self.changed = true;
        Ok(())
    }

    /// Draw a decal centred at `at` (normalised `0..1` inside the group).
    ///
    /// `scale` is the decal's width as a fraction of the group's width; its
    /// height follows the image's aspect. Pixels outside the group are clipped.
    pub fn decal_group(
        &mut self,
        index: usize,
        decal: &Texture,
        at: [f32; 2],
        scale: f32,
    ) -> Result<(), PaintError> {
        self.check_group(index)?;
        let (x1, y1, x2, y2) = self
            .group_rect(index)
            .ok_or(PaintError::NoBounds { index })?;
        if x2 <= x1 || y2 <= y1 {
            return Err(PaintError::NoBounds { index });
        }
        if decal.width == 0 || decal.height == 0 {
            return Err(PaintError::Image("empty decal".into()));
        }
        self.snapshot();
        let rect_w = (x2 - x1) as f32;
        let rect_h = (y2 - y1) as f32;
        let dw = (scale.clamp(0.01, 4.0) * rect_w).max(1.0);
        let dh = (dw * decal.height as f32 / decal.width as f32).max(1.0);
        let cx = x1 as f32 + at[0].clamp(0.0, 1.0) * rect_w;
        let cy = y1 as f32 + at[1].clamp(0.0, 1.0) * rect_h;
        let left = (cx - dw * 0.5).round() as i32;
        let top = (cy - dh * 0.5).round() as i32;
        let skin = self.ensure_skin();
        for py in 0..dh.round().max(1.0) as i32 {
            for px in 0..dw.round().max(1.0) as i32 {
                let x = left + px;
                let y = top + py;
                if x < x1 || x >= x2 || y < y1 || y >= y2 {
                    continue;
                }
                let u = px as f32 / dw;
                let v = py as f32 / dh;
                let texel = decal.sample(u, v);
                skin.blend_pixel(x as u32, y as u32, texel);
            }
        }
        self.changed = true;
        Ok(())
    }

    fn check_group(&self, index: usize) -> Result<(), PaintError> {
        if index >= self.groups.len() {
            return Err(PaintError::NoGroup {
                index,
                count: self.groups.len(),
            });
        }
        Ok(())
    }

    /// Write the skin back to a chassis. An untouched paint restores the
    /// original bytes exactly; a painted one writes a fresh 256×256 TGA.
    pub fn apply_to(&self, chassis: &mut Chassis) {
        if !self.changed {
            chassis.texture_tga = self.original.clone();
            return;
        }
        if let Some(skin) = &self.skin {
            chassis.texture_tga = Some(skin.to_tga());
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.skin.clone());
        self.skin = previous;
        self.changed = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.skin.clone());
        self.skin = next;
        self.changed = true;
        true
    }

    /// Test helper: the current skin, if any.
    pub fn skin_mut(&mut self) -> Option<&mut Skin> {
        self.skin.as_mut()
    }
}

fn group(index: usize, g: &FaceGroup, faces: &[Face]) -> Group {
    Group {
        index,
        bounds: g.bounds,
        triangles: faces.iter().filter(|f| f.group == index as i32).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::bot::{Armour, Bot, BotMesh, Component};

    fn mesh() -> BotMesh {
        BotMesh {
            points: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    fn chassis() -> Chassis {
        let skin = Skin::new(SKIN_SIZE, SKIN_SIZE, [10, 20, 30, 255]);
        Chassis {
            height: 0.5,
            face_count: 2,
            mesh: Some(mesh()),
            bottom: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            top: vec![[0, 0], [1000, 0], [1000, 1000], [0, 1000]],
            texture_tga: Some(skin.to_tga()),
            armour: Some(Armour::default()),
            face_groups: vec![
                FaceGroup {
                    coords: vec![[0.0, 0.0], [64.0, 0.0], [64.0, 64.0]],
                    bounds: [0, 0, 64, 64],
                },
                FaceGroup {
                    coords: vec![[64.0, 0.0], [128.0, 0.0], [128.0, 64.0]],
                    bounds: [64, 0, 128, 64],
                },
            ],
            faces: vec![
                Face {
                    group: 0,
                    points: [0, 1, 2],
                },
                Face {
                    group: 1,
                    points: [0, 2, 3],
                },
                // Kept verbatim by the reader and ignored by the renderer.
                Face {
                    group: 9,
                    points: [3, 2, 1],
                },
            ],
            ..Default::default()
        }
    }

    fn bot(chassis: Chassis) -> Bot {
        Bot {
            name: "Painted".into(),
            components: vec![Component {
                parent: -1,
                class: "Chassis".into(),
                rotation: [0.0, 0.0, 0.0, 1.0],
                chassis: Some(chassis),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_tga_round_trips_through_the_skin() {
        let skin = Skin::new(4, 3, [1, 2, 3, 4]);
        let bytes = skin.to_tga();
        assert_eq!(bytes.len(), 18 + 4 * 3 * 4);
        let back = Skin::from_tga(&bytes).unwrap();
        assert_eq!(back.width, 4);
        assert_eq!(back.height, 3);
        assert_eq!(back.pixels, skin.pixels);
    }

    #[test]
    fn painting_a_group_touches_only_that_groups_rectangle() {
        let ch = chassis();
        let mut paint = Paint::from_chassis(&ch);
        assert_eq!(paint.group_count(), 2);
        paint
            .fill_group(0, &Texture::solid(8, 8, [200, 10, 10, 255]))
            .unwrap();
        paint
            .fill_group(1, &Texture::solid(8, 8, [10, 10, 200, 255]))
            .unwrap();
        let mut ch2 = ch.clone();
        paint.apply_to(&mut ch2);
        let skin = Skin::from_tga(ch2.texture_tga.as_deref().unwrap()).unwrap();
        for y in 0..64 {
            for x in 0..64 {
                let p = skin.pixel(x, y);
                assert_eq!((p[0], p[1], p[2]), (200, 10, 10), "group 0 at {x},{y}");
            }
            for x in 64..128 {
                let p = skin.pixel(x, y);
                assert_eq!((p[0], p[1], p[2]), (10, 10, 200), "group 1 at {x},{y}");
            }
            for x in 128..256 {
                let p = skin.pixel(x, y);
                assert_eq!((p[0], p[1], p[2]), (10, 20, 30), "outside at {x},{y}");
            }
        }
    }

    #[test]
    fn a_decal_is_clipped_to_its_group() {
        let ch = chassis();
        let mut paint = Paint::from_chassis(&ch);
        let decal = Texture::solid(4, 4, [255, 0, 255, 255]);
        paint.decal_group(1, &decal, [0.5, 0.5], 0.5).unwrap();
        let skin = paint.skin().unwrap();
        // Centre of group 1 is (96, 32).
        let p = skin.pixel(96, 32);
        assert_eq!((p[0], p[1], p[2]), (255, 0, 255));
        // Outside the group's rectangle, nothing changed.
        assert_eq!(skin.pixel(40, 32), [10, 20, 30, 255]);
        assert_eq!(skin.pixel(80, 200), [10, 20, 30, 255]);
    }

    #[test]
    fn an_untouched_paint_writes_the_original_bytes_back() {
        let ch = chassis();
        let original = ch.texture_tga.clone().unwrap();
        let paint = Paint::from_chassis(&ch);
        let mut out = ch.clone();
        paint.apply_to(&mut out);
        assert_eq!(out.texture_tga.as_deref(), Some(original.as_slice()));
    }

    #[test]
    fn paint_undo_restores_the_previous_skin() {
        let ch = chassis();
        let mut paint = Paint::from_chassis(&ch);
        paint
            .fill_group(0, &Texture::solid(2, 2, [255, 255, 255, 255]))
            .unwrap();
        assert_eq!(paint.skin().unwrap().pixel(1, 1), [255, 255, 255, 255]);
        assert!(paint.undo());
        assert_eq!(paint.skin().unwrap().pixel(1, 1), [10, 20, 30, 255]);
        assert!(paint.redo());
        assert_eq!(paint.skin().unwrap().pixel(1, 1), [255, 255, 255, 255]);
    }

    /// A painted chassis survives the game's own writer: the face groups, the
    /// face rows and the skin's pixels are the same after `write`/`parse`.
    #[test]
    fn a_painted_chassis_round_trips_through_the_writer() {
        let ch = chassis();
        let mut paint = Paint::from_chassis(&ch);
        paint
            .fill_group(0, &Texture::solid(4, 4, [1, 2, 3, 255]))
            .unwrap();
        paint
            .decal_group(1, &Texture::solid(2, 2, [9, 9, 9, 255]), [0.25, 0.75], 0.5)
            .unwrap();
        let mut bot = bot(ch);
        paint.apply_to(bot.components[0].chassis.as_mut().unwrap());
        let bytes = bot.write();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        let ch2 = back.components[0].chassis.as_ref().unwrap();
        let original = bot.components[0].chassis.as_ref().unwrap();
        assert_eq!(ch2.face_groups, original.face_groups);
        assert_eq!(ch2.faces, original.faces);
        assert_eq!(ch2.texture_tga, original.texture_tga);
        let skin = Skin::from_tga(ch2.texture_tga.as_deref().unwrap()).unwrap();
        assert_eq!(skin.pixel(0, 0), [1, 2, 3, 255]);
        assert_eq!(skin.pixel(64, 0), [10, 20, 30, 255]);
    }

    fn game_root() -> Option<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = std::path::PathBuf::from(dir);
            return dir.join("Components").is_dir().then_some(dir);
        }
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.join("Components").is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    fn bots(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                bots(&path, out);
            } else if path.extension().is_some_and(|e| e == "bot") {
                out.push(path);
            }
        }
    }

    /// The model's group indices are the file's flat, file-ordered list — the
    /// same list the sim's chassis renderer walks (`Chassis.faces`'s `group`
    /// indexes `Chassis.face_groups`, in file order). The corpus leaves 60 face
    /// rows past the end; both keep them and both ignore them when drawing.
    #[test]
    fn face_groups_are_the_sims_flat_file_ordered_list() {
        let Some(root) = game_root() else { return };
        let mut files = Vec::new();
        for sub in ["Robot Designs", "AI", "Teams"] {
            let dir = root.join(sub);
            if dir.is_dir() {
                bots(&dir, &mut files);
            }
        }
        files.sort();
        assert!(files.len() >= 57);
        let mut groups = 0usize;
        let mut faces = 0usize;
        let mut out_of_range = 0usize;
        for path in &files {
            let data = std::fs::read(path).unwrap();
            let bot = ra2fmt::bot::parse(&data).unwrap();
            let Some(ch) = bot.components[0].chassis.as_ref() else {
                continue;
            };
            let paint = Paint::from_chassis(ch);
            assert_eq!(paint.group_count(), ch.face_groups.len());
            assert_eq!(paint.faces(), ch.faces.as_slice());
            for (i, g) in paint.groups().iter().enumerate() {
                assert_eq!(g.bounds, ch.face_groups[i].bounds);
                assert_eq!(g.index, i);
            }
            let (in_range, out) = paint.face_range_counts();
            assert_eq!(in_range + out, ch.faces.len());
            groups += paint.group_count();
            faces += ch.faces.len();
            out_of_range += out;
        }
        assert_eq!(files.len(), 57);
        assert!(
            out_of_range >= 60,
            "the corpus' out-of-range face rows were {out_of_range}"
        );
        eprintln!(
            "{} bots: {groups} face groups, {faces} face rows, {out_of_range} past the last group",
            files.len()
        );
    }
}
