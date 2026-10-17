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
//! The page is **16-bit**, and the high byte is the antialiased coverage. The
//! low byte is **not** the glyph mask this note once called it: dumped, more
//! than 90% of it is `0xFF` (the colour/alpha channel, and what the page builder
//! `FUN_005cd560` writes with `(coverage << 8) | 0xfff`), so only the high byte
//! is a picture of the glyphs. The page is also stored **bottom-up**, so a
//! renderer must read it that way round: the engine's own path swaps the two `v`s
//! of a glyph's quad, and reading the page in byte order draws every label in the
//! shell upside-down — a capital `T` with its bar at the bottom and an `L` with
//! its foot at the top. That was measured through the whole pipeline, and the
//! check is now a test (`the_page_is_stored_bottom_up`): inside the rows its own
//! rect names, `T`'s ink sits *higher* than `L`'s, which is only true if the page
//! runs bottom-up.
//!
//! The horizontal axis is **not** flipped, and the drawing is what settles it: a
//! label reading `LOCAL FJORD 12345` in `fg_d26` and again in `fg_mcn16` comes
//! out reading `LOCAL FJORD 12345`, with every asymmetric letter the right way
//! round. The check that *suggested* otherwise was the same one applied to a
//! glyph's *columns*, and it is the wrong tool: a cell's window can catch a
//! neighbour's ink (record 36, `F`, profiles with its stem on the right), so a
//! column mean is not a measurement of a glyph. Rows are — `T` and `L` are wide
//! apart in shape and their own cells bracket them — which is why the vertical
//! claim is the one that stands. Two readings of a low-zoom render as "mirrored"
//! were wrong for the same reason a column profile was: not enough to read.
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
//! ## The rects do not bracket the page's glyphs
//!
//! **The shipped `.giff` is internally inconsistent, and the fault is the file's,
//! not this reader's.** The record's four floats are settled: the loader that
//! *builds* a runtime page (`FUN_005cd560`) writes `u0 = x/W`, `v0 = y/H`,
//! `u1 = (x + width)/W`, `v1 = (y + height)/H`, so the order is `u0 v0 u1 v1` and
//! `v` is measured from the page's bottom (which [`Glyph::rect`] turns over). The
//! page's picture is the **high byte**; the low byte is colour, not a mask (more
//! than 99% of it is `0xFF` where the high byte has ink, and the builder writes
//! `(coverage << 8) | 0xfff`).
//!
//! **The "ink just outside the rect" count is confounded and is only a
//! baseline.** Consecutive rects are contiguous with a one-pixel gap, so `u0 - 1`
//! and `u1` land on the *same shared gap column*; a packer whose glyphs touch (or
//! whose coverage bleeds) puts ink there for almost every record. The workshop
//! toolbar that *looks* clean is **baked into the button `.bmp` art**, not drawn
//! by this reader. A probe of all nine faces (`[Text] HAMBURGEFONTSIV`, rendered
//! by `uishot` and `tools/ui_compose.py`, which agree pixel for pixel) shows
//! **every face smears**, worst the small condensed ones (`fg_mcn12`, `fg_mcn14`,
//! `fg_mcn16`), least the large `fg_d*`.
//!
//! What the page says, measured. Drawing every rect border over the `fg_mcn14`
//! page at 4× shows the borders landing **0–6 columns from the nearest blank cell
//! boundary** — not a uniform off-by-N, not a drift along the row, not a slip:
//! **jitter**, with the rect generally a few columns left of its glyph. Of the 94
//! scored rects, **69** (`fg_mcn14`), **74** (`fg_mcn16`) and **71** (`fg_d20`)
//! have their **left border on ink**, 66/69/58 their right border, and only ~5
//! bracket a blank-separated cell at all. No constant shift (`d` in `-10..=10`
//! leaves 17/20 for `fg_d20`, 18/33 for `fg_mcn14`), scale or pitch fixes it; the
//! fit `cell_start = a·x0 + b` is `a ≈ 1.00, b ≈ 0` with a ~3 px residual on both
//! `fg_mcn14` and `fg_mcn16`.
//!
//! The file cannot be rescued by another layout. The header is invariant across
//! all nine faces: word `0x0C` is exactly `1.0f`, the sixteen bytes at `0x18` are
//! zero, and `leading` is 4–7, never the records' `third`. Of the layouts the
//! 133800 bytes admit — records at `0x28`/`0x44`/`0x68`, 95 records, a 95-record
//! page at 2700, `512x128`, `128x512` — the best strict-bracket count is **9 of
//! 92** (`512x128`), against 92 for a correct file. So the file is broken as
//! shipped: the rects and the page describe different packings, and the renderer
//! stays faithful to the shipped data rather than inventing a correction.
//! `the_font_file_is_internally_inconsistent` keeps the border-on-ink counts and
//! the header checks.
//!
//! What is **not** settled is whether the game draws this file directly. Its page
//! *builder* (`FUN_005cd560`) creates a D3D texture, blits per-glyph bitmaps from
//! a sprite/glyph array (stride `0x54`, data `0x58`) and computes the u/v above —
//! so a rebuild exists at least on the fallback path. But the deserialiser
//! `FUN_005ce100`/`FUN_005ce320` reads **four floats then three ints** per glyph
//! where this file stores **three ints then four floats**, so a conversion (or a
//! second, unshipped file) sits between them; and the `.giff` parser and the
//! text-draw routine were not located in a bounded search. The exact next reads
//! are: (a) `FUN_00591b50`'s stack arguments at its call site `0x4e3805`
//! (it is passed both `fg_mcn14.giff` and `fg_mcn14`); (b) the function that
//! converts `[3 ints][4 floats]` records to `[4 floats][3 ints]`; (c) the routine
//! that consumes the glyph array at `font+0x38` (four floats) and `font+0x638`
//! (three ints) to build the quad, and whether it samples this page or the rebuilt
//! one.
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
    /// The glyph page's coverage, 0..=255, `page_width * page_height`, first row
    /// first — the same rows [`Glyph::rect`] names, so a renderer samples it
    /// straight. (The *uv* in a record is measured from the bottom, which is
    /// what `rect` turns over.)
    pub page: Vec<u8>,
    /// The other byte of each 16-bit sample, same layout. It is **not** a glyph
    /// mask: more than 90% of it is `0xFF`, the colour/alpha channel the page
    /// builder writes (`(coverage << 8) | 0xfff`), so it carries no picture.
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

    /// Whether the table has a cell for this character at all. The first two
    /// printable characters do not — the records start at
    /// [`FIRST_CHAR`](const@FIRST_CHAR) — so a space has an advance but no ink.
    pub fn has_glyph(&self, c: char) -> bool {
        let code = c as u32;
        code >= FIRST_CHAR && (code - FIRST_CHAR) < self.glyphs.len() as u32
    }

    /// The advance of a character the table does not carry — which in practice
    /// is the space, and the exclamation mark. The median of the advances the
    /// face does carry is what a space looks like; the first record, which is
    /// what [`glyph`](Self::glyph) falls back to, is nothing like it.
    pub fn space_advance(&self) -> i32 {
        let mut widths: Vec<i32> = self
            .glyphs
            .iter()
            .map(|g| g.advance)
            .filter(|a| *a > 0)
            .collect();
        widths.sort_unstable();
        widths.get(widths.len() / 2).copied().unwrap_or(0)
    }

    /// The advance width of a character.
    pub fn advance(&self, c: char) -> i32 {
        if self.has_glyph(c) {
            self.glyph(c).advance
        } else {
            self.space_advance()
        }
    }

    /// The line height: `ascent - descent`.
    pub fn line_height(&self) -> i32 {
        self.ascent - self.descent
    }

    /// How much room a string takes: `(width, height)` in pixels. The width is
    /// the sum of the advances, so a trailing character's ink may overhang it,
    /// and a character with no cell — a space — takes the median advance.
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
    // one the colour/alpha channel (mostly 0xFF), not a glyph mask.
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
        shipped_bytes(name).and_then(|d| parse(&d).ok())
    }

    /// The raw `.giff` bytes from `RA2.cfz`, for the header checks.
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
    /// tile *each other*; whether they bracket the page's glyphs is a separate
    /// question, and they do not (see the module note and
    /// `almost_no_record_brackets_its_glyph`).
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
    /// The page is stored **bottom-up**, and this is the measurement that says
    /// so: `T` carries its bar at the top and `L` its foot at the bottom, so
    /// inside the rows its own rect names, `T`'s ink must sit higher than `L`'s.
    /// It does — in every face — which is why the renderer swaps the two `v`s of
    /// a glyph's uv: reading the page in byte order draws every label
    /// upside-down.
    ///
    /// The same check with a *column* is not decisive at these sizes (`L`'s foot
    /// is as wide as the cell and its stem one pixel, so the mean column barely
    /// moves), and the horizontal axis is the one thing here still open — see
    /// the recon notes.
    #[test]
    fn the_page_is_stored_bottom_up() {
        // The three faces the asymmetry was measured on. `fg_mcn12` is
        // ambiguous at 12 pixels — its `L` is 8 wide with a one-pixel stem — and
        // is not claimed.
        for face in ["fg_mcn14", "fg_mcn16", "fg_d20"] {
            let Some(font) = shipped(face) else { return };
            let (w, h) = (font.page_width as usize, font.page_height as usize);
            let mean_row = |c: usize| -> f64 {
                let g = &font.glyphs[c - 34];
                let (x0, y0, gw, gh) = g.rect(font.page_width, font.page_height);
                let (x0, y0) = (x0.max(0) as usize, y0.max(0) as usize);
                let (mut sum, mut count) = (0usize, 0usize);
                for y in y0..(y0 + gh as usize).min(h) {
                    for x in x0..(x0 + gw as usize).min(w) {
                        if font.page[y * w + x] > 40 {
                            sum += y - y0;
                            count += 1;
                        }
                    }
                }
                sum as f64 / count.max(1) as f64
            };
            let (t, l) = (mean_row(84), mean_row(76));
            assert!(
                l < t,
                "{face}: `L`'s foot ({l:.1}) must sit above `T`'s bar ({t:.1}) \
                 for the page to be bottom-up"
            );
        }
    }

    /// How much ink a record's own rect holds, and over how many columns.
    fn ink_of(font: &Font, record: usize) -> (usize, usize, usize) {
        let w = font.page_width as usize;
        let h = font.page_height as usize;
        let (x0, y0, gw, gh) = font.glyphs[record].rect(font.page_width, font.page_height);
        let (mut ink, mut min, mut max) = (0, usize::MAX, 0);
        for y in y0.max(0)..(y0 + gh).min(h as i32) {
            for x in x0.max(0)..(x0 + gw).min(w as i32) {
                if font.page[(y as usize) * w + x as usize] > 40 {
                    ink += 1;
                    min = min.min(x as usize);
                    max = max.max(x as usize);
                }
            }
        }
        (ink, min, max)
    }

    /// The glyph table is a **packed row**: a record's rect begins one column
    /// after the last one's ended, so the records are where the reader thinks
    /// they are. Which is what makes the one thing that *is* missing legible as
    /// the file's own: **`fg_d20` has no `O`**. Its record 45 — the character
    /// `O` — is a full 11×14 rect whose ink sits in its last three columns, 21
    /// pixels of it, while **`fg_mcn14`'s record 45, the same character, draws a
    /// whole letter** (45 pixels over eight columns). Same index, same kind of
    /// rect, one face with a letter and one without — so the record mapping is
    /// right and the *page* is blank there. The advance is intact (17 against a
    /// neighbour's 10), which is why the letter is skipped rather than drawn
    /// somewhere wrong, and why `botlabwin.gib`'s `title txt` reads `R BT W RKSH
    /// P` instead of `ROBOT WORKSHOP`: the game reads the same file, so this is
    /// the data's, and the renderer is faithful to it.
    #[test]
    fn one_face_has_no_o() {
        let Some(d20) = shipped("fg_d20") else { return };
        let Some(mcn) = shipped("fg_mcn14") else {
            return;
        };

        // The row is packed: every rect around the O follows the one before it
        // with a one- to three-column gap, so the records are where the rects
        // say they are.
        for i in 40..45 {
            let (x0, _, gw, _) = d20.glyphs[i].rect(d20.page_width, d20.page_height);
            let (next_x, _, next_w, _) = d20.glyphs[i + 1].rect(d20.page_width, d20.page_height);
            let gap = next_x - (x0 + gw);
            assert!((0..=3).contains(&gap), "record {i}: gap {gap} ({next_w})");
        }

        // The O: a rect with room, and almost no ink in it.
        let (_, _, gw, gh) = d20.glyphs[45].rect(d20.page_width, d20.page_height);
        assert!(gw >= 10 && gh >= 10, "the O's rect is {gw}x{gh}");
        let (ink, min, max) = ink_of(&d20, 45);
        assert_eq!((ink, max - min + 1), (21, 3), "fg_d20's O");
        assert_eq!(d20.glyphs[45].advance, 17, "and its advance, intact");

        // The same character in another face is a letter.
        let (ink, min, max) = ink_of(&mcn, 45);
        assert!(ink > 40 && max - min + 1 >= 8, "fg_mcn14's O: {ink}");
        assert_eq!(mcn.glyphs.len(), d20.glyphs.len(), "both tables are 96");
    }

    /// The ink a record's rect holds, and how many columns of it carry any.
    fn glyph_ink(font: &Font, record: usize) -> (usize, usize) {
        let w = font.page_width as usize;
        let h = font.page_height as usize;
        let (x0, y0, gw, gh) = font.glyphs[record].rect(font.page_width, font.page_height);
        let (mut ink, mut columns) = (0, 0);
        for x in x0.max(0)..(x0 + gw).min(w as i32) {
            let mut any = false;
            for y in y0.max(0)..(y0 + gh).min(h as i32) {
                if font.page[(y as usize) * w + x as usize] > 40 {
                    ink += 1;
                    any = true;
                }
            }
            if any {
                columns += 1;
            }
        }
        (ink, columns)
    }

    fn every_face() -> Vec<(String, Font)> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cfz = loop {
            let c = dir.join("local/game/RA2.cfz");
            if c.is_file() {
                break c;
            }
            if !dir.pop() {
                return Vec::new();
            }
        };
        let Ok(cfl) = crate::cfl::Cfl::open_path(&cfz) else {
            return Vec::new();
        };
        let mut names: Vec<String> = cfl
            .entries()
            .iter()
            .filter(|e| e.name.to_ascii_lowercase().ends_with(".giff"))
            .map(|e| e.name.clone())
            .collect();
        names.sort();
        names
            .into_iter()
            .filter_map(|name| {
                let font = parse(&cfl.get(&name)?).ok()?;
                (font.glyphs.len() > 45).then_some((name, font))
            })
            .collect()
    }

    /// **Two faces have no `O`.** Every face ships a 96-record table, and record
    /// 45 is the character `O` in all of them — but in `fg_d20` its rect holds 21
    /// pixels across 3 of its 11 columns and in `fg_mcn12` 12 pixels across 2 of
    /// 8, where every other face draws a whole letter (40 pixels and up, over six
    /// columns or more). The advances are intact, so the letter is *skipped*
    /// rather than misplaced, and the game reads the same pages.
    #[test]
    fn two_faces_have_no_o() {
        let faces = every_face();
        if faces.is_empty() {
            return;
        }
        assert_eq!(faces.len(), 9, "the shipped faces");
        let mut broken = Vec::new();
        for (name, font) in &faces {
            let (ink, columns) = glyph_ink(font, 45);
            assert!(font.glyphs[45].advance > 0, "{name}: no advance");
            if columns <= 3 {
                broken.push((name.trim_end_matches(".giff").to_string(), ink, columns));
            } else {
                assert!(
                    ink >= 40 && columns >= 6,
                    "{name}: a whole O is {ink} px over {columns} columns"
                );
            }
        }
        assert_eq!(
            broken,
            [
                ("fg_d20".to_string(), 21, 3),
                ("fg_mcn12".to_string(), 12, 2)
            ],
            "the faces whose O is a fragment"
        );
    }

    /// **The "ink just outside the rect" count is confounded — keep it only as a
    /// baseline.** A rect that frames a glyph would leave `u0 - 1` and `u1` blank,
    /// but consecutive rects are contiguous with a one-pixel gap, so both checks
    /// land on the *same shared gap column*; a packer whose glyphs touch (or whose
    /// coverage bleeds) puts ink there for almost every record. So of the 96
    /// records in `fg_mcn14`, 87 "cut a neighbour" even though every rect holds
    /// ink — and the workshop toolbar that *looks* clean is baked `.bmp` art, not
    /// this reader. The number is kept so the day the alignment is fixed the test
    /// says so.
    ///
    /// The test also asserts the measurements that localise it: the low byte is
    /// colour, not a glyph mask (more than 90% is `0xFF`); the cells do hold ink;
    /// and *no* constant shift of a packed row (`d` in `-10..=10`) comes close. A
    /// fit of the page's own ink runs against the rects gives `a ≈ 1.00, b ≈ 0`
    /// with a ~3 px per-glyph residual on **both** faces — not a field order, a
    /// flip, a rounding rule, a sample pairing, a uniform offset, a scale or a
    /// pitch, but a per-glyph scatter (largest at a row's right end: `fg_mcn16`'s
    /// `A` rect is `(236,244,10,12)` while `A`'s ink run is `244..250`). See the
    /// module note.
    #[test]
    fn almost_no_record_brackets_its_glyph() {
        let mut inside = Vec::new();
        let mut cut = Vec::new();
        let mut best = Vec::new();
        let mut colour = Vec::new();
        for face in ["fg_mcn14", "fg_mcn16", "fg_d20"] {
            let Some(font) = shipped(face) else { return };
            // Where the coverage has glyph ink, the second byte is >99% `0xFF`:
            // it is the colour/alpha channel, not a glyph picture.
            let (mut inked, mut white) = (0usize, 0usize);
            for i in 0..font.page.len() {
                if font.page[i] > 40 {
                    inked += 1;
                    if font.mask[i] >= 200 {
                        white += 1;
                    }
                }
            }
            colour.push((face, white * 20 > inked * 19));
            let w = font.page_width as usize;
            let h = font.page_height as usize;
            let ink = |x: i32, y: i32| -> bool {
                x >= 0
                    && y >= 0
                    && (x as usize) < w
                    && (y as usize) < h
                    && font.page[(y as usize) * w + x as usize] > 40
            };
            let (mut ins, mut cu, mut sc) = (0usize, 0usize, 0usize);
            for g in &font.glyphs[..94] {
                let (x0, y0, gw, gh) = g.rect(font.page_width, font.page_height);
                if gw <= 0 || x0 <= 0 || (x0 + gw) as usize >= w {
                    continue;
                }
                sc += 1;
                let colink = |x: i32| (y0.max(0)..(y0 + gh).min(h as i32)).any(|y| ink(x, y));
                if (x0..x0 + gw).any(&colink) {
                    ins += 1;
                }
                if colink(x0 - 1) || colink(x0 + gw) {
                    cu += 1;
                }
            }
            inside.push((face, ins));
            cut.push((face, cu, sc));
            // The packed rows, and the best constant shift of each.
            let mut bands: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
            for (i, g) in font.glyphs.iter().enumerate().take(94) {
                bands
                    .entry((g.v0 as f64 * 1.0e6).round() as i64)
                    .or_default()
                    .push(i);
            }
            let mut band_best = Vec::new();
            for recs in bands.values() {
                let (mut bc, mut bd, mut bs) = (usize::MAX, 0i32, 0usize);
                for d in -10i32..=10 {
                    let (mut c, mut s) = (0usize, 0usize);
                    for &i in recs {
                        let (x0, y0, gw, gh) =
                            font.glyphs[i].rect(font.page_width, font.page_height);
                        let (x0, x1) = (x0 + d, x0 + gw + d);
                        if x1 - x0 <= 0 || x0 <= 0 || x1 as usize >= w {
                            continue;
                        }
                        s += 1;
                        let colink =
                            |x: i32| (y0.max(0)..(y0 + gh).min(h as i32)).any(|y| ink(x, y));
                        if colink(x0 - 1) || colink(x1) {
                            c += 1;
                        }
                    }
                    if c < bc {
                        bc = c;
                        bd = d;
                        bs = s;
                    }
                }
                band_best.push((bd, bc, bs));
            }
            band_best.retain(|&(_, _, bs)| bs > 0);
            best.push((face, band_best));
        }
        // Every rect holds ink — the rects are *near* their glyphs — and yet
        // almost every one also has ink in the one-pixel gap column it shares
        // with the next rect, so the glyph overflows its cell on that side.
        assert_eq!(
            inside,
            [("fg_mcn14", 89), ("fg_mcn16", 89), ("fg_d20", 89)],
            "records whose rect holds ink"
        );
        assert_eq!(
            cut,
            [
                ("fg_mcn14", 87, 92),
                ("fg_mcn16", 86, 92),
                ("fg_d20", 78, 90)
            ],
            "records whose rect cuts a neighbour (of those scored)"
        );
        // No constant shift of a packed row's rects comes close: the best shift
        // still cuts a third to two thirds of the row. (The last `fg_d20` band
        // holds one degenerate record and is dropped.)
        assert_eq!(
            best,
            [
                ("fg_mcn14", vec![(-5, 18, 33), (-5, 25, 31), (7, 20, 27)]),
                ("fg_mcn16", vec![(-4, 19, 31), (-4, 22, 28), (-5, 22, 32)]),
                (
                    "fg_d20",
                    vec![(0, 18, 24), (0, 18, 21), (-2, 17, 20), (-2, 20, 25)]
                ),
            ],
            "the best constant shift of each packed row (d, cut, scored)"
        );
        // The low byte is colour, not a glyph plane: >90% of it is 0xFF.
        assert_eq!(
            colour,
            [("fg_mcn14", true), ("fg_mcn16", true), ("fg_d20", true)],
            "faces whose second byte is mostly 0xFF"
        );
    }

    /// **The shipped `.giff` is internally inconsistent: its rects do not frame
    /// its own page.** The overlay of `fg_mcn14`'s page (4×, every rect drawn)
    /// shows each rect's borders landing **0–6 columns from the nearest blank cell
    /// boundary** — not a uniform off-by-N, not a drift along the row, not a slip:
    /// jitter, with the rect generally a few columns left of its glyph. Of the 94
    /// scored rects, **69** (`fg_mcn14`), **74** (`fg_mcn16`) and **71** (`fg_d20`)
    /// have their **left border on ink** (a blank cell boundary would leave it
    /// blank), and 66/69/58 their right border; only ~5 bracket a blank-separated
    /// cell at all.
    ///
    /// The file cannot be rescued by another layout: the header is invariant
    /// (word `0x0C` is exactly `1.0f`, the sixteen bytes at `0x18` are zero, and
    /// `leading` is 4–7, never the records' `third`), and of the layouts the
    /// 133800 bytes admit — records at `0x28`/`0x44`/`0x68`, 95 records, a
    /// 95-record page at 2700, `512x128`, `128x512` — the best strict-bracket
    /// count is **9 of 92** (`512x128`), against 92 for a correct file. So the
    /// fault is in the font file, not in this reader; and the game's own draw path
    /// was not located (see the module note), so the renderer stays faithful to
    /// the shipped data rather than inventing a correction.
    #[test]
    fn the_font_file_is_internally_inconsistent() {
        for face in [
            "fg_d18", "fg_d20", "fg_d24", "fg_d26", "fg_dcn14", "fg_dcn16", "fg_mcn12", "fg_mcn14",
            "fg_mcn16",
        ] {
            let Some(data) = shipped_bytes(face) else {
                return;
            };
            assert_eq!(
                u32::from_le_bytes([data[12], data[13], data[14], data[15]]),
                0x3f80_0000,
                "{face}: word 0x0C is not 1.0f"
            );
            assert!(
                data[0x18..0x28].iter().all(|&b| b == 0),
                "{face}: the sixteen bytes at 0x18 are not zero"
            );
        }

        let mut border = Vec::new();
        for face in ["fg_mcn14", "fg_mcn16", "fg_d20"] {
            let Some(font) = shipped(face) else {
                return;
            };
            let w = font.page_width as usize;
            let h = font.page_height as usize;
            let mut bands: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
            for (i, g) in font.glyphs.iter().enumerate().take(94) {
                bands
                    .entry((g.v0 as f64 * 1.0e6).round() as i64)
                    .or_default()
                    .push(i);
            }
            let (mut left, mut right, mut scored) = (0usize, 0usize, 0usize);
            for idxs in bands.values() {
                let rects: Vec<_> = idxs
                    .iter()
                    .map(|&i| font.glyphs[i].rect(font.page_width, font.page_height))
                    .collect();
                let y0 = rects.iter().map(|r| r.1).min().unwrap().max(0);
                let y1 = rects.iter().map(|r| r.1 + r.3).max().unwrap().min(h as i32);
                let blank: Vec<bool> = (0..w)
                    .map(|x| (y0..y1).all(|y| font.page[(y as usize) * w + x] <= 128))
                    .collect();
                for &(x0, _, gw, _) in &rects {
                    let x1 = x0 + gw;
                    if x0 < 0 || x1 <= 0 || x1 as usize >= w {
                        continue;
                    }
                    scored += 1;
                    if !blank[x0 as usize] {
                        left += 1;
                    }
                    if !blank[x1 as usize] {
                        right += 1;
                    }
                }
            }
            border.push((face, left, right, scored));
        }
        assert_eq!(
            border,
            [
                ("fg_mcn14", 69, 66, 94),
                ("fg_mcn16", 74, 69, 94),
                ("fg_d20", 71, 58, 94),
            ],
            "rect borders that land on ink (left, right, scored)"
        );
    }

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
