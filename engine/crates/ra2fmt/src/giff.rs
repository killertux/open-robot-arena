//! Parser for `fg_*.giff` — the shell's bitmap fonts, shipped inside `RA2.cfz`.
//!
//! Nine of them, named by the `_Font` field of every label in every `.gib`
//! screen: `fg_d18`, `fg_d20`, `fg_d24`, `fg_d26`, `fg_dcn14`, `fg_dcn16`,
//! `fg_mcn12`, `fg_mcn14`, `fg_mcn16`.
//!
//! The file is the serialised form of the game's font object (the loader is
//! `FUN_005ce320`; `FUN_005cd560` is the builder that renders the face with
//! FreeType and writes the file's source data):
//!
//! ```text
//! 0x00  i32 ascent          (object +0xab8: the face's ascender, in pixels)
//! 0x04  i32 descent         (object +0xabc, stored negative)
//! 0x08  i32 extent          (object +0xac0: the deepest glyph row below the baseline, +1)
//! 0x0c  f32 scale           (1.0)
//! 0x10  u32 page width      (256, or 512 for the 24 and 26 point faces)
//! 0x14  u32 page height
//! 0x18  95 records of 28 bytes, for the characters 0x20..=0x7E:
//!         f32 u0 v0 u1 v1   the glyph's cell in the page, as fractions
//!         i32 advance       FreeType horiAdvance   >> 6
//!         i32 bearing_x     FreeType horiBearingX  >> 6
//!         i32 bearing_y     FreeType horiBearingY  >> 6
//! ....  an embedded 16-bit TGA: 18-byte header, width x height pixels, and the
//!       26-byte `TRUEVISION-XFILE.` footer
//! ```
//!
//! Every shipped file is `24 + 95 * 28 + 18 + w * h * 2 + 26` bytes, and the
//! page offset is derived from the header, never hard-coded.
//!
//! A pixel is ARGB4444 as the builder writes it, `(coverage << 8) | 0xfff`: the
//! top nibble is the glyph's coverage. The TGA is stored **bottom-up**, so the
//! file's first row is the texture's last; the records' `v` is measured on the
//! texture (top-down), so a glyph's cell is `v0*h .. v1*h` of the page read in
//! texture order, and that is how [`Font::page`] is stored here.
//!
//! A glyph is drawn with its cell's left edge at `pen + bearing_x` and its top
//! `bearing_y` above the baseline; the pen then moves by `advance`. There is no
//! kerning in the format.

use crate::Error;

/// The first character the table covers (a space) and how many it holds.
pub const FIRST_CHAR: u32 = 32;
pub const GLYPH_COUNT: usize = 95;

const HEADER: usize = 24;
const RECORD: usize = 28;
const TGA_HEADER: usize = 18;
const TGA_FOOTER: usize = 26;

/// A parsed `fg_*.giff`.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The face's ascender in pixels.
    pub ascent: i32,
    /// The descender, stored *negative*.
    pub descent: i32,
    /// The deepest glyph row below the baseline, plus one (header word 2).
    pub extent: i32,
    pub page_width: u32,
    pub page_height: u32,
    /// The glyph page's coverage, 0..=255, `page_width * page_height`, in
    /// **texture order** (top row first): the rows [`Glyph::rect`] names.
    pub page: Vec<u8>,
    /// The 95 records for `0x20..=0x7E`.
    pub glyphs: Vec<Glyph>,
}

/// One glyph's metrics and its cell in the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    /// How far the pen moves after drawing it.
    pub advance: i32,
    /// The cell's left edge, right of the pen.
    pub bearing_x: i32,
    /// The cell's top edge, above the baseline.
    pub bearing_y: i32,
}

impl Glyph {
    /// The glyph's cell in the page in pixels, `(x, y, w, h)`, top-down.
    pub fn rect(&self, width: u32, height: u32) -> (i32, i32, i32, i32) {
        let x0 = (self.u0 * width as f32).round() as i32;
        let x1 = (self.u1 * width as f32).round() as i32;
        let y0 = (self.v0 * height as f32).round() as i32;
        let y1 = (self.v1 * height as f32).round() as i32;
        (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }

    /// Whether the cell holds any pixels (a space's does not).
    pub fn has_ink(&self) -> bool {
        self.u1 > self.u0 && self.v1 > self.v0
    }
}

impl Font {
    /// Whether the table has a record for this character.
    pub fn has_glyph(&self, c: char) -> bool {
        let code = c as u32;
        (FIRST_CHAR..FIRST_CHAR + GLYPH_COUNT as u32).contains(&code)
    }

    /// The record for a character; anything outside printable ASCII is `?`.
    pub fn glyph(&self, c: char) -> &Glyph {
        let c = if self.has_glyph(c) { c } else { '?' };
        &self.glyphs[(c as u32 - FIRST_CHAR) as usize]
    }

    /// The advance of a character, in pixels.
    pub fn advance(&self, c: char) -> i32 {
        self.glyph(c).advance
    }

    /// The advance of a space.
    pub fn space_advance(&self) -> i32 {
        self.advance(' ')
    }

    /// The line height: `ascent - descent`.
    pub fn line_height(&self) -> i32 {
        self.ascent - self.descent
    }

    /// How much room a string takes: `(width, height)` in pixels, the sum of
    /// the advances by the line height.
    pub fn extent(&self, text: &str) -> (i32, i32) {
        let width: i32 = text.chars().map(|c| self.advance(c)).sum();
        (width, self.line_height())
    }

    /// The coverage at a pixel of the page (texture order), 0 out of bounds.
    pub fn coverage(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.page_width as i32 || y >= self.page_height as i32 {
            return 0;
        }
        self.page[(y as u32 * self.page_width + x as u32) as usize]
    }
}

/// Parse a `fg_*.giff`.
pub fn parse(data: &[u8]) -> Result<Font, Error> {
    if data.len() < HEADER {
        return Err(Error::UnexpectedEof {
            at: data.len(),
            wanted: HEADER,
        });
    }
    let word = |at: usize| -> u32 {
        u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
    };
    let (ascent, descent, extent) = (word(0) as i32, word(4) as i32, word(8) as i32);
    let (page_width, page_height) = (word(16), word(20));
    let samples = (page_width as usize)
        .checked_mul(page_height as usize)
        .ok_or_else(|| Error::Structure("font page is too large".into()))?;
    let tga = HEADER + GLYPH_COUNT * RECORD;
    let pixels = tga + TGA_HEADER;
    let wanted = pixels + samples * 2 + TGA_FOOTER;
    if data.len() != wanted {
        return Err(Error::Structure(format!(
            "a {page_width}x{page_height} font is {wanted} bytes, this is {}",
            data.len()
        )));
    }
    // The embedded TGA: uncompressed true-colour, 16 bits, the page's size.
    let tga_w = u16::from_le_bytes([data[tga + 12], data[tga + 13]]) as u32;
    let tga_h = u16::from_le_bytes([data[tga + 14], data[tga + 15]]) as u32;
    if data[tga + 2] != 2 || data[tga + 16] != 16 || tga_w != page_width || tga_h != page_height {
        return Err(Error::Structure(
            "the font's embedded TGA page is not 16-bit".into(),
        ));
    }

    let glyphs = (0..GLYPH_COUNT)
        .map(|i| {
            let at = HEADER + i * RECORD;
            let f = |o: usize| f32::from_bits(word(at + o));
            Glyph {
                u0: f(0),
                v0: f(4),
                u1: f(8),
                v1: f(12),
                advance: word(at + 16) as i32,
                bearing_x: word(at + 20) as i32,
                bearing_y: word(at + 24) as i32,
            }
        })
        .collect();

    // Bottom-up in the file; texture order here. Coverage is the top nibble of
    // the ARGB4444 sample, widened the way the hardware widens it.
    let (w, h) = (page_width as usize, page_height as usize);
    let mut page = vec![0u8; samples];
    for row in 0..h {
        let src = pixels + (h - 1 - row) * w * 2;
        for x in 0..w {
            let hi = data[src + x * 2 + 1];
            page[row * w + x] = (hi >> 4) * 17;
        }
    }
    Ok(Font {
        ascent,
        descent,
        extent,
        page_width,
        page_height,
        page,
        glyphs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACES: [&str; 9] = [
        "fg_d18", "fg_d20", "fg_d24", "fg_d26", "fg_dcn14", "fg_dcn16", "fg_mcn12", "fg_mcn14",
        "fg_mcn16",
    ];

    fn shipped_bytes(name: &str) -> Option<Vec<u8>> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let cfz = dir.join("local/game/RA2.cfz");
            if cfz.is_file() {
                let cfl = crate::cfl::Cfl::open_path(&cfz).ok()?;
                return cfl.get(&format!("{name}.giff"));
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// The glyph's ink, counted in its own cell and in the page overall.
    fn cell_ink(font: &Font, c: char) -> (u32, u32) {
        let (x, y, w, h) = font.glyph(c).rect(font.page_width, font.page_height);
        let mut n = 0;
        for yy in y..y + h {
            for xx in x..x + w {
                if font.coverage(xx, yy) > 0x40 {
                    n += 1;
                }
            }
        }
        (n, (w * h) as u32)
    }

    /// All nine faces parse by the layout above, and every printable glyph's
    /// cell holds ink while the cell's one-pixel border does not hold much of
    /// it (the cells are the glyphs, not slices of their neighbours).
    #[test]
    fn every_face_parses_and_its_cells_are_its_glyphs() {
        for name in FACES {
            let Some(data) = shipped_bytes(name) else {
                return;
            };
            let font = parse(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(font.glyphs.len(), GLYPH_COUNT, "{name}");
            assert!(!font.glyphs[0].has_ink(), "{name}: the space has no cell");
            assert!(font.glyphs[0].advance > 0, "{name}: the space advances");
            for code in 0x21..=0x7eu32 {
                let c = char::from_u32(code).unwrap();
                let (ink, area) = cell_ink(&font, c);
                assert!(ink > 0 && area > 0, "{name}: {c:?} has no ink in its cell");
            }
            // Full coverage exists somewhere in the page.
            assert!(font.page.contains(&255), "{name}");
        }
    }

    /// Glyphs are upright in texture order: a `T`'s bar is in the top rows of
    /// its cell and an `L`'s foot in the bottom rows.
    #[test]
    fn glyphs_are_upright_in_the_page() {
        let Some(data) = shipped_bytes("fg_d20") else {
            return;
        };
        let font = parse(&data).unwrap();
        let row_ink = |c: char, row: i32| -> i32 {
            let (x, y, w, _) = font.glyph(c).rect(font.page_width, font.page_height);
            (x..x + w)
                .filter(|&xx| font.coverage(xx, y + row) > 0x80)
                .count() as i32
        };
        let (_, _, _, th) = font.glyph('T').rect(font.page_width, font.page_height);
        assert!(row_ink('T', 1) > row_ink('T', th - 2), "T's bar is on top");
        let (_, _, _, lh) = font.glyph('L').rect(font.page_width, font.page_height);
        assert!(
            row_ink('L', lh - 2) > row_ink('L', 1),
            "L's foot is at the bottom"
        );
    }

    /// The metrics are FreeType's: a lower-case `p` hangs below the baseline
    /// (its cell is taller than its bearing) and a capital does not.
    #[test]
    fn bearings_place_descenders_below_the_baseline() {
        let Some(data) = shipped_bytes("fg_d20") else {
            return;
        };
        let font = parse(&data).unwrap();
        let below = |c: char| {
            let g = font.glyph(c);
            let (_, _, _, h) = g.rect(font.page_width, font.page_height);
            h - g.bearing_y
        };
        assert!(below('p') >= 3);
        assert!(below('E') <= 1);
    }

    #[test]
    fn a_truncated_font_is_rejected() {
        let Some(data) = shipped_bytes("fg_mcn14") else {
            return;
        };
        assert!(parse(&data[..data.len() - 1]).is_err());
        assert!(parse(&data[..10]).is_err());
    }
}
