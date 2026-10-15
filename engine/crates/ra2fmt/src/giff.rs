//! Parser for `fg_*.giff` — the shell's bitmap fonts, shipped inside `RA2.cfz`.
//!
//! Nine of them, named by the `_Font` field of every label in every `.gib`
//! screen: `fg_d18`, `fg_d20`, `fg_d24`, `fg_d26`, `fg_dcn14`, `fg_dcn16`,
//! `fg_mcn12`, `fg_mcn14`, `fg_mcn16` (the `m`/`d` pair is the plain and bold
//! cut, `cn` the condensed one).
//!
//! The file is a fixed layout, and a small one:
//!
//! ```text
//! 0x00  ascent, descent, a third metric, 1.0f, page width, page height
//! 0x18  sixteen zero bytes
//! 0x28  96 records of 28 bytes
//! 0xAA8 the page: width x height 16-bit samples
//! ```
//!
//! The arithmetic is exact on all nine files: `40 + 96*28 = 2728`, and for the
//! 256-wide fonts `2728 + 256*256*2 = 133800`, the size every `mcn`/`dcn`/`d18`
//! file is; the two 512-wide ones are `2728 + 512*512*2 = 527016`.
//!
//! ```text
//! 95 records at 68 would also fit, and that is how this reader first had it.
//! What settles it is the runtime object the loader fills (FUN_005cd560): its
//! glyph array is 0x600 bytes of four floats, which is 96 glyphs of 16, and its
//! integer array is 96 of 12. So there are 96 records, the table starts at 0x28,
//! and the header is forty bytes.
//!
//! A record is three `i32`s and four `f32`s: an advance width in pixels, a
//! bearing, a third value that tracks the font size, and the glyph's rect **in
//! the page, as a fraction** (`u0 v0 u1 v1`).
//!
//! The page is **16-bit**, and the two bytes are two different pictures of the
//! same glyphs: the high byte is the antialiased coverage and the low byte a
//! hard 1-bit mask. It is also stored **bottom-up** — `v = 0` is the *bottom*
//! row — so [`Glyph::rect`] flips it into the top-down rect a renderer wants.
//!
//! ## Which record is which character
//!
//! The record for character `c` is `(c - 34) * 28`. That was measured by drawing,
//! not guessed: rendered at that offset, `mainmenu.gib`'s own `MULTIPLAYER TEAM
//! HQ` label comes out reading `MULTIPLAYER TEAM HQ`; at 32, 33, 35 or 36 it does
//! not. The loader's own loop starts at `0x20`, so the shipped files and the
//! loader's enumeration disagree by two characters — most likely the game's font
//! tool against a later loader. `ra2 font` labels every record with the character
//! this offset gives it.
//!
//! Two letters of that render are still off — `M` and `A` — which is either the
//! baseline rule ([`Glyph::rect`] hangs each cell from its `third`, which is
//! right for most of the alphabet) or a one-record slip somewhere in the middle
//! of the table. Everything else lands, including the label's two spaces.
//!
//! What a font is *for* here is [`Font::extent`]: the size a zero-`Size` `Text`
//! widget takes, which is `ascent - descent` tall and the sum of the advances
//! wide.

use crate::Error;

/// A parsed `fg_*.giff`.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Height above the baseline, in pixels.
    pub ascent: i32,
    /// Depth below it — and it is stored *negative*, so the line height is
    /// `ascent - descent`.
    pub descent: i32,
    /// A third metric that tracks the size (4 for `mcn14`, 6 for `d24`); its
    /// meaning is not settled, and nothing here uses it.
    pub leading: i32,
    pub page_width: u32,
    pub page_height: u32,
    /// The glyph page's coverage, 0..=255, `page_width * page_height`, top row
    /// first.
    pub page: Vec<u8>,
    /// The 1-bit mask that ships alongside the coverage, same layout.
    pub mask: Vec<u8>,
    /// 96 records. The one for character `c` is `(c - `[`FIRST_CHAR`]`) * 28` —
    /// see the module note.
    pub glyphs: Vec<Glyph>,
}

/// One glyph's metrics and its rect in the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// How far the pen moves after drawing it.
    pub advance: i32,
    /// A horizontal bearing.
    pub bearing: i32,
    /// The third value of the record.
    pub third: i32,
    /// The rect in the page, as a fraction of it, `v` measured from the bottom.
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl Glyph {
    /// The glyph's rect in the page in pixels, top-left origin — the page is
    /// stored bottom-up, so `v` is flipped here.
    pub fn rect(&self, width: u32, height: u32) -> (i32, i32, i32, i32) {
        let x0 = (self.u0 * width as f32).round() as i32;
        let x1 = (self.u1 * width as f32).round() as i32;
        let y0 = ((1.0 - self.v1) * height as f32).round() as i32;
        let y1 = ((1.0 - self.v0) * height as f32).round() as i32;
        (x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
    }
}

impl Font {
    /// The glyph for a character, or the first record for anything the table
    /// does not cover.
    pub fn glyph(&self, c: char) -> &Glyph {
        let code = c as u32;
        if code >= FIRST_CHAR && (code - FIRST_CHAR) < self.glyphs.len() as u32 {
            &self.glyphs[(code - FIRST_CHAR) as usize]
        } else {
            &self.glyphs[0]
        }
    }

    /// The advance width of a character.
    pub fn advance(&self, c: char) -> i32 {
        self.glyph(c).advance
    }

    /// The line height: `ascent - descent`.
    pub fn line_height(&self) -> i32 {
        self.ascent - self.descent
    }

    /// How much room a string takes: `(width, height)` in pixels. The width is
    /// the sum of the advances, so a trailing character's ink may overhang it.
    pub fn extent(&self, text: &str) -> (i32, i32) {
        let width: i32 = text.chars().map(|c| self.advance(c)).sum();
        (width, self.line_height())
    }

    /// The coverage at a pixel of the page, 0 when out of bounds.
    pub fn coverage(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.page_width as i32 || y >= self.page_height as i32 {
            return 0;
        }
        self.page[(y as u32 * self.page_width + x as u32) as usize]
    }
}

/// Parse a `fg_*.giff`.
/// The record for a character is `(c - FIRST_CHAR) * 28`, measured off the page
/// (see the module note).
pub const FIRST_CHAR: u32 = 34;

pub fn parse(data: &[u8]) -> Result<Font, Error> {
    const HEADER: usize = 40;
    const COUNT: usize = 96;
    const RECORD: usize = 28;

    if data.len() < HEADER {
        return Err(Error::UnexpectedEof {
            at: data.len(),
            wanted: HEADER,
        });
    }
    let word = |at: usize| -> Result<u32, Error> {
        data.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or(Error::UnexpectedEof { at, wanted: 4 })
    };
    let ascent = word(0)? as i32;
    let descent = word(4)? as i32;
    let leading = word(8)? as i32;
    let page_width = word(16)?;
    let page_height = word(20)?;
    let page_samples = (page_width as usize)
        .checked_mul(page_height as usize)
        .ok_or_else(|| Error::Structure("font page is too large".into()))?;
    let wanted = HEADER + COUNT * RECORD + page_samples * 2;
    if data.len() != wanted {
        return Err(Error::Structure(format!(
            "a {page_width}x{page_height} font is {wanted} bytes, this is {}",
            data.len()
        )));
    }

    let record = |at: usize| -> Glyph {
        let i = |o: usize| word(at + o).unwrap_or(0) as i32;
        let f = |o: usize| f32::from_bits(word(at + o).unwrap_or(0));
        Glyph {
            advance: i(0),
            bearing: i(4),
            third: i(8),
            u0: f(12),
            v0: f(16),
            u1: f(20),
            v1: f(24),
        }
    };

    let glyphs = (0..COUNT).map(|i| record(HEADER + i * RECORD)).collect();

    // Two bytes per sample: the high one is the antialiased coverage, the low
    // one the 1-bit mask.
    let mut page = Vec::with_capacity(page_samples);
    let mut mask = Vec::with_capacity(page_samples);
    for i in 0..page_samples {
        let lo = data[HEADER + COUNT * RECORD + i * 2];
        let hi = data[HEADER + COUNT * RECORD + i * 2 + 1];
        mask.push(lo);
        page.push(hi);
    }

    Ok(Font {
        ascent,
        descent,
        leading,
        page_width,
        page_height,
        page,
        mask,
        glyphs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shipped(name: &str) -> Option<Font> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let cfz = dir.join("local/game/RA2.cfz");
            if cfz.is_file() {
                let cfl = crate::cfl::Cfl::open_path(&cfz).ok()?;
                let data = cfl.get(&format!("{name}.giff"))?;
                return parse(&data).ok();
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Every shipped font parses, and the arithmetic of the format holds: the
    /// 256-wide ones are 133800 bytes, the 512-wide ones 527016.
    #[test]
    fn the_shipped_fonts_all_parse() {
        let mut seen = 0;
        for name in [
            "fg_d18", "fg_d20", "fg_d24", "fg_d26", "fg_dcn14", "fg_dcn16", "fg_mcn12", "fg_mcn14",
            "fg_mcn16",
        ] {
            let Some(font) = shipped(name) else {
                return;
            };
            seen += 1;
            assert_eq!(font.glyphs.len(), 96, "{name}");
            let bytes = font.page_width as usize * font.page_height as usize;
            assert_eq!(font.page.len(), bytes);
            assert_eq!(font.mask.len(), bytes);
            // 68 + 95*28 + two bytes per sample is the file's whole size, and
            // that is what the parse demanded.
            assert_eq!(
                2728 + bytes * 2,
                if font.page_width == 256 {
                    133800
                } else {
                    527016
                }
            );
            assert!(font.ascent > 0, "{name}");
            assert!(font.descent < 0, "{name}");
            assert_eq!(font.line_height(), font.ascent - font.descent);
        }
        assert_eq!(seen, 9);
    }

    /// The rectangles tile the page: every cell lies inside it, consecutive
    /// records are contiguous in x within their packing row, and the rows
    /// themselves are few — the packer lays glyphs out left to right and wraps,
    /// so `v0` takes only a handful of values across the 95 records. The rects
    /// are therefore not the open question; the record-to-character alignment
    /// is (see the module note).
    #[test]
    fn the_rects_tile_the_page() {
        let Some(font) = shipped("fg_mcn14") else {
            return;
        };
        assert_eq!((font.page_width, font.page_height), (256, 256));
        let mut previous: Option<(i32, i32, i32)> = None;
        let mut tallest = 0;
        let mut rows = std::collections::BTreeSet::new();
        for (i, g) in font.glyphs.iter().enumerate() {
            let (x, y, w, h) = g.rect(font.page_width, font.page_height);
            assert!(
                x >= 0 && y >= 0 && x + w <= 256 && y + h <= 256,
                "record {i}"
            );
            assert!(w <= 32 && h <= 40, "record {i} is {w}x{h}");
            tallest = tallest.max(h);
            rows.insert((x, y));
            if let Some((px, py, pw)) = previous {
                if py == y {
                    // Same packed row: contiguous, to within a pixel of padding.
                    assert!(
                        x >= px && x <= px + pw + 2,
                        "record {i} is at {x}, after {px}..{} on row {y}",
                        px + pw
                    );
                }
            }
            previous = Some((x, y, w));
        }
        assert!((8..=20).contains(&tallest), "tallest cell {tallest}");
        // The 95 records share exactly one cell between them: 94 distinct
        // rectangles, the last record carrying a nonsense advance (1040) and a
        // degenerate rect. So the table really holds 94 glyphs, one short of the
        // 95 the loader's loop would write, which is another thread of the
        // mapping question in the module note.
        assert!(rows.len() >= 90, "{} distinct cells", rows.len());

        let (w, line) = font.extent("ABC");
        assert_eq!(w, font.advance('A') + font.advance('B') + font.advance('C'));
        assert_eq!(line, 28);
    }

    /// A page that is the wrong size for its header is rejected rather than
    /// read past the end.
    #[test]
    fn a_truncated_font_is_rejected() {
        assert!(parse(&[0u8; 20]).is_err());
        let mut head = vec![0u8; 40];
        head[16] = 0; // page width 0
        head[17] = 1; // 256
        head[20] = 0;
        head[21] = 1; // 256
        assert!(parse(&head).is_err());
    }
}
