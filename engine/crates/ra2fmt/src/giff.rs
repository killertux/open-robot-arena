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
//! The page is stored **bottom-up only**. `v` is measured from the bottom, so a
//! renderer samples the cell bottom-up; `u` is **not** flipped. Flipped
//! vertically and read left to right, the page's row at y 233 is `E F G H I J K
//! L M N O P Q R S T U V` — upright letters in reading order. This corrects an
//! earlier reading: rotating the page 180° both reverses the row and mirrors
//! each glyph, and that was mistaken for the cells being mirrored in `u`.
//! Swapping `u` as well mirrors every letter — a rendered `WATER` comes out
//! `WATƎЯ` — so both renderers swap `v` only. The workshop toolbar is not
//! evidence either way: its words are baked into the button `.bmp`.
//!
//! ## The rects are not the page's cells (S8)
//!
//! The record's four rect floats and the page are **two different packings**.
//! The page lays the glyphs out in packed rows (bottom-up, in record order), with
//! a right slot holding the records that did not fit at the end of the previous
//! row (one or two of them); the row is its record block rotated left by that
//! many records. The records' rects are a cumulative layout that drifts from
//! those runs. Drawing the rects
//! therefore samples the neighbours — `fg_d20`'s `EXHIBITION` read
//! `E/XIUBITIU !N`, and its `O` looked absent because the rect pointed at the
//! row below the page's `O`. `Font::repack_from_page` now derives every cell
//! from the page's ink runs at parse time, and [`Font::advance`] reads the
//! advance table two records on (the loader starts its loop at `0x20`). The
//! measurement below is what led there; where it says a renderer must stay
//! faithful to the record's rect, read "to the page's own runs".
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
//! Under the record rects two letters of that render were still off — `M` and
//! `A` — and S8 showed why: the rects are a different packing from the page
//! (below). With the page's runs the whole label reads, including the two
//! spaces.
//!
//! ## Why the rects looked like clipped cells (the measurement S8 settled)
//!
//! With the vertical flip in place the labels are upright, but some glyphs are
//! still struck through — worst on the small condensed faces. The cause is
//! **not** another reader bug: in the condensed faces the rect samples a cell of
//! the page that is shifted a few columns from the glyph it names, and in every
//! face the glyph's ink overhangs the advance-sized rect. This is a property of
//! the shipped file (the page's glyph positions and the rects disagree), not of
//! `parse`, which reads `u0`/`u1` straight. The record's four floats are settled: the
//! loader that *builds* a runtime page (`FUN_005cd560`) writes `u0 = x/W`,
//! `v0 = y/H`, `u1 = (x + width)/W`, `v1 = (y + height)/H`, so the order is
//! `u0 v0 u1 v1` and `v` is measured from the page's bottom (which
//! [`Glyph::rect`] turns over). The page's picture is the **high byte**; the low
//! byte is colour, not a mask.
//!
//! **The "ink just outside the rect" count is confounded and is only a
//! baseline.** Consecutive rects are contiguous with a one-pixel gap, so `u0 - 1`
//! and `u1` land on the *same shared gap column*; a packer whose glyphs touch (or
//! whose coverage bleeds) puts ink there for almost every record. The workshop
//! toolbar that *looks* clean is **baked into the button `.bmp` art**, not drawn
//! by this reader. A probe of all nine faces (`[Text] HAMBURGEFONTSIV`, rendered
//! by `uishot` and `tools/ui_compose.py`, which agree) shows the large `fg_d*`
//! faces reading and the small condensed ones (`fg_mcn12`, `fg_mcn14`,
//! `fg_mcn16`) smearing: their rects are the ones offset from the page's
//! glyphs.
//!
//! What the page says, measured. Overlaying every rect on the page shows the
//! borders landing **0–6 columns from the nearest blank cell boundary**, and the
//! shift **grows along each row** (page glyph cell start − rect x0, monotone
//! pairing): `fg_mcn14` row 1 is `+0 +3 +5 +5 −1 −2 +3 +3 +5 +7 … +8 +9`, row 0
//! grows to `+12…+17`, row 2 sits at `0` until its last three (the far-right
//! anomaly). That is a small pitch excess of the page over the rects — not a
//! uniform off-by-N, not a single slip. Across all nine faces **58–82 of 94**
//! rects have their left border on ink and **58–77** their right border, and only
//! **0–5** bracket a blank-separated glyph (`fg_mcn14` 69/66/0, `fg_mcn16`
//! 74/69/0, `fg_d20` 71/58/3). So the advance-sized cells clip the overhanging
//! glyph in every face; the small condensed faces look worse because their
//! rects are also offset, and a few columns is a larger fraction of a small
//! glyph.
//!
//! The probe for that is **shape, not reading**: one `[Text]` per face holding
//! `F`, `L`, `J`, rendered by `uishot`, then each glyph's ink bbox and density
//! (ink / bbox area). Every bbox fills its cell (the renderer samples exactly the
//! rect), the letters are **upright** (the vertical flip is right), and the `F` is the
//! sparse one — density **0.43** in `fg_mcn14`, **0.49** in `fg_mcn16`, **0.50**
//! in `fg_d20`, against the `L`'s 0.57/0.48/0.59 — because the cell keeps the
//! `F`'s stem and clips a bar. The rendered ink count equals the cell's ink count
//! exactly, so the renderer is faithful and it is the file's rect that clips.
//! A long label is not a different case: a character's cell and pen are the same
//! in a two-character label and in the 44-character help line
//! (`ra2-render`'s `a_glyphs_cell_does_not_depend_on_the_label_length`), the two
//! renderers agree on the words and layout, and the alphabet maps in order. The lower-case
//! help body reads worse only because the same tail is a larger fraction of a
//! 5–9 px cell. Three rescues were tried and rejected — widening the pen to the
//! cell, squeezing the cell onto the advance, and trimming to the ink bbox — all
//! because the tail is *inside* the shipped cell. The faithful rescue is the
//! game's own re-pack, whose cut is the open read above.
//!
//! **The page is not advance-packed.** Placing each record at
//! `x = Σ advance` within its row (the hypothesis that the page follows the pen)
//! was measured against the record rects. It loses: `fg_mcn14`'s row at y 233
//! holds 32 records whose advances sum to **197 px**, while that row's page ink
//! reaches the page's right edge (**255 px**) and the rects span **250 px** — the page runs at roughly the
//! rect pitch, not the advance pitch. Rendered, the probe `WATER MOW EfGbDqp`
//! reads under the record rects (cleanly on the regular faces, the upper case
//! on the condensed ones) and is unreadable under the advance mapping.
//! The same holds on every face; the condensed rows are the ones whose rects
//! are a few columns off their glyphs (the next paragraph), and the advance
//! mapping is off in the opposite direction.
//!
//! The file cannot be rescued by another layout. The header is invariant across
//! all nine faces: word `0x0C` is exactly `1.0f`, the sixteen bytes at `0x18` are
//! zero, and `leading` is 4–7, never the records' `third`. Of the layouts the
//! 133800 bytes admit — records at `0x28`/`0x44`/`0x68`, 95 records, a 95-record
//! page at 2700, `512x128`, `128x512` — the best strict-bracket count is **9 of
//! 92** (`512x128`), against 92 for a correct file. So the file's rects and page
//! describe different packings. S8 settles it by reading the **page's runs**
//! (`repack_from_page`), which is the shipped data, not by re-authoring a glyph.
//! `the_cells_are_the_pages_glyph_runs`, `every_face_maps_its_page_runs` and
//! `the_font_page_supplies_the_o_the_records_misplace` keep the reconciled
//! state measurable; the header is checked by `the_font_header_is_invariant`.
//!
//! What the binary says. `FUN_00591b50` (the `.giff` → runtime path) constructs
//! the font object, loads the resource by name, deserialises it with
//! `FUN_005ce320`, then calls `FUN_005cd560`. `FUN_005cd560` **cuts the glyphs out
//! and re-packs them**: it creates a D3D texture, blits each per-glyph bitmap from
//! a sprite/glyph array (stride `0x54`, data `0x58`), and writes fresh
//! `u0 = x/W … v1 = (y+h)/H` — it does not use the file's floats as the quad's
//! uv. So the runtime's page is a **rebuild**, and the file's page/rects are its
//! source. What is **not** settled is **how the cut is located**: the deserialiser
//! `FUN_005ce320` reads **four floats then three ints** per glyph where this file
//! stores **three ints then four floats**, so a conversion (or a second, unshipped
//! file) sits between them, and the `.giff` parser itself was not located in a
//! bounded search. The exact next reads are: (a) `FUN_00591b50`'s stack arguments
//! at its call site `0x4e3805` (it is passed both `fg_mcn14.giff` and
//! `fg_mcn14`); (b) the function that converts `[3 ints][4 floats]` records to
//! `[4 floats][3 ints]`; (c) whether the cut uses the record's `u0/u1` (our rect),
//! the `advance`, or the mask plane.
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
    /// The glyph's rect in the page in pixels, top-left origin. The page's rows
    /// are stored **bottom-up** — `v` is measured from the bottom — so `v` is
    /// turned over here. `u` is **not** flipped: the cells run left to right in
    /// reading order, and the letters inside them are upright when the page is
    /// turned over. A renderer therefore swaps the glyph quad's `v` pair only;
    /// swapping `u` mirrors every letter.
    ///
    /// The rect is the record's cell, not necessarily the page's glyph: in the
    /// condensed faces the page's glyph sits a few columns from it, and the
    /// glyph's ink can overhang the advance-sized rect. See the module note.
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
    ///
    /// The advance table is **two records ahead of the page table**: the record
    /// for the glyph `A` is `65 - 34`, but the advance that belongs with it is
    /// `65 - 32` (the loader's own loop starts at `0x20`, see the module note).
    /// The shift is visible in the metrics — `fg_mcn16`'s `i` (record 71) carries
    /// 7 where its `m` carries 11, but record 73 carries 3 and record 77 carries
    /// 11, which are `i`'s and `m`'s; `n` at record 76 carries 3 where 78 carries
    /// 7. Spacing a run by the glyph table's advances makes `Options` read
    /// `Opti o is`; by this table it reads `Options`. The cells are still taken
    /// from the page (see `repack_from_page`), so the two tables are read
    /// where each is right.
    pub fn advance(&self, c: char) -> i32 {
        if !self.has_glyph(c) {
            return self.space_advance();
        }
        let shifted = self.glyphs.get(c as usize - 32);
        match shifted {
            Some(g) if g.advance > 0 && g.advance <= 200 => g.advance,
            _ => self.glyph(c).advance,
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

fn read(data: &[u8]) -> Result<Font, Error> {
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

    let font = Font {
        ascent,
        descent,
        leading,
        page_width,
        page_height,
        page,
        mask,
        glyphs,
    };
    Ok(font)
}

/// Parse a `fg_*.giff` and re-pack its page cells from the page's ink.
pub fn parse(data: &[u8]) -> Result<Font, Error> {
    let mut font = read(data)?;
    font.repack_from_page();
    Ok(font)
}

/// Parse a `fg_*.giff` without re-packing its page cells. Tests use this to see
/// the file's two packings; the renderer always goes through [`parse`].
#[cfg(test)]
fn parse_raw(data: &[u8]) -> Result<Font, Error> {
    read(data)
}

impl Font {
    /// Re-derive every glyph's cell from the page's own ink.
    ///
    /// The records' `u0..v1` and the page are **two different packings** (the
    /// module note has the measurement): the page lays the glyphs out in reading
    /// order, one packed row per `v0`, while the records' rects are a cumulative
    /// layout whose cells drift from the glyphs they name. Drawing the record's
    /// rect therefore samples a slice of its neighbours — `fg_d20`'s
    /// `EXHIBITION` reads `E/XIUBITIU !N`, and its `O` is a three-column piece of
    /// the `9` that follows it. The game does not use the file's rects either: its
    /// loader cuts the glyphs out and re-packs them (`FUN_005cd560`, see the module
    /// note), so the page's ink is the authority here too.
    ///
    /// The page's rows are packed **bottom-up**, in the same order as the record
    /// rows (`v0` ascending). Each row is a run of glyphs separated by blank
    /// columns, plus a **right slot** (the last ~13 columns) where the packer
    /// puts the records that did not fit at the end of the previous row. The
    /// page is therefore the row's own records **rotated left by the number of
    /// right-slot records**: on `fg_mcn14` the first row reads
    /// `# ... C "`, the second `E ... c D`, and the third `f ... ~ d e` — one,
    /// one and two records moved to the slot. `fg_mcn16`'s first row has a blank
    /// quote, so its slot holds only the `A` that its next row also carries.
    /// A width/advance alignment (below) finds that rotation and the blank
    /// records without hard-coding a face, a character or a string.
    ///
    /// The left/right widths are the **next record's** raw cell width: the file's
    /// rects are one record ahead of the page's runs (the same packing
    /// disagreement), which is the measured prior the alignment scores against.
    /// The advance is **not** touched: it belongs to the advance table, read two
    /// records on (see [`Font::advance`]).
    fn repack_from_page(&mut self) {
        let rows = page_rows(self);
        let mut mapped = vec![false; self.glyphs.len()];
        for row in rows {
            for &(record, run) in &row.matches {
                let (x0, x1) = row.runs[run];
                self.set_cell(record, x0, x1, row.top, row.bottom);
                mapped[record] = true;
            }
            // A record in the block with no run is blank in this face (the quote
            // in `fg_mcn16`, a `DEL`, the leading space-like records). Give it an
            // empty cell unless another row already supplied it.
            for &record in &row.block {
                if !mapped[record] {
                    self.clear_cell(record);
                }
            }
        }
    }

    /// Point one record at a page run: the horizontal cell is the run and the
    /// vertical cell the packed row.
    fn set_cell(&mut self, record: usize, x0: i32, x1: i32, top: i32, bottom: i32) {
        let (w, h) = (self.page_width as f32, self.page_height as f32);
        let height = (bottom - top).max(1);
        let glyph = &mut self.glyphs[record];
        glyph.u0 = x0 as f32 / w;
        glyph.u1 = (x1 + 1) as f32 / w;
        glyph.v0 = 1.0 - bottom as f32 / h;
        glyph.v1 = 1.0 - top as f32 / h;
        // The baseline sits one pixel above the packed row's bottom, so the
        // cell's top is `height - 1` above it.
        glyph.third = height - 1;
        // The advance is **not** rewritten: it belongs to the shifted advance
        // table (see `Font::advance`), and the packed pitch is only the glyph's
        // width, not its spacing.
    }

    /// An empty cell: a record the page has no glyph for in this face.
    fn clear_cell(&mut self, record: usize) {
        let glyph = &mut self.glyphs[record];
        glyph.u0 = 0.0;
        glyph.u1 = 0.0;
        glyph.v0 = 0.0;
        glyph.v1 = 0.0;
        glyph.third = 0;
    }
}

/// One page row's run/record alignment, as measured from the raw file.
#[derive(Debug, Clone)]
struct RowAlignment {
    top: i32,
    bottom: i32,
    /// The page's ink runs in visual order (regular flow then right slot).
    runs: Vec<(i32, i32)>,
    /// The row's record block: `previous row's last` + this row's records but
    /// its last.
    block: Vec<usize>,
    /// The chosen left rotation: the page's first run is `block[k]`.
    #[allow(dead_code)]
    k: usize,
    #[allow(dead_code)]
    cost: i32,
    /// `(record, run)` pairs, in run order.
    matches: Vec<(usize, usize)>,
}

/// Measure every packed page row: its band, ink runs, record block and the
/// rotation/blank alignment that best matches the run widths to the records'
/// raw cell widths. This is the rule `repack_from_page` applies.
fn page_rows(font: &Font) -> Vec<RowAlignment> {
    use std::collections::BTreeMap;

    let (w, h) = (font.page_width as i32, font.page_height as i32);
    if w <= 0 || h <= 0 {
        return Vec::new();
    }
    let raw_width: Vec<i32> = font
        .glyphs
        .iter()
        .map(|g| ((g.u1 - g.u0) * w as f32).round() as i32)
        .collect();
    let raw_top: Vec<i32> = font
        .glyphs
        .iter()
        .map(|g| ((1.0 - g.v1) * h as f32).round() as i32)
        .collect();
    let raw_bottom: Vec<i32> = font
        .glyphs
        .iter()
        .map(|g| ((1.0 - g.v0) * h as f32).round() as i32)
        .collect();

    // The records that describe a real cell: a positive advance and a
    // non-degenerate rect. The last record in every face is a sentinel
    // (`advance 1040`, an empty rect) and must not join a row.
    let mut rows: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (i, g) in font.glyphs.iter().enumerate() {
        if g.advance <= 0 || g.advance > 200 {
            continue;
        }
        let (_, _, gw, gh) = g.rect(font.page_width, font.page_height);
        if gw <= 0 || gh <= 0 {
            continue;
        }
        rows.entry((g.v0 * h as f32).round() as i32)
            .or_default()
            .push(i);
    }

    let mut out = Vec::new();
    let mut previous_last: Option<usize> = None;
    for records in rows.values() {
        let top = records
            .iter()
            .map(|&i| raw_top[i])
            .min()
            .unwrap_or(0)
            .clamp(0, h);
        let bottom = records
            .iter()
            .map(|&i| raw_bottom[i])
            .max()
            .unwrap_or(h)
            .clamp(0, h);
        if bottom <= top {
            previous_last = records.last().copied();
            continue;
        }

        // One packed run per blank column gap. A truncated duplicate at the
        // page's right edge (the row's first glyph copied into the slot and cut
        // by the edge) is dropped when a real slot run is already there.
        let mut ink = vec![false; w as usize];
        for (x, any) in ink.iter_mut().enumerate() {
            *any = (top..bottom).any(|y| font.page[(y as usize) * w as usize + x] > 30);
        }
        let mut runs: Vec<(i32, i32)> = Vec::new();
        let mut start = None;
        for x in 0..=w {
            let lit = x < w && ink[x as usize];
            match (lit, start) {
                (true, None) => start = Some(x),
                (false, Some(a)) => {
                    runs.push((a, x - 1));
                    start = None;
                }
                _ => {}
            }
        }
        let mut kept: Vec<(i32, i32)> = Vec::new();
        for (a, b) in runs {
            // A 1-px run at either page edge whose ink is only a row or two
            // tall is bleed, not a glyph; a real 1-px bracket at the edge spans
            // the band. A truncated duplicate at the right edge is the next
            // row's first glyph copied there.
            let height = (top..bottom)
                .filter(|&y| {
                    (a..=b).any(|x| font.page[(y as usize) * w as usize + x as usize] > 30)
                })
                .count();
            let edge_speck =
                b == a && (a == 0 || b >= w - 1) && height * 2 < (bottom - top) as usize;
            let truncated = b >= w - 1 && b - a < 4 && kept.iter().any(|&(x, _)| x >= w - 13);
            if !edge_speck && !truncated {
                kept.push((a, b));
            }
        }
        let mut runs = kept;

        let mut block: Vec<usize> = Vec::with_capacity(records.len());
        if let Some(last) = previous_last {
            block.push(last);
        }
        block.extend_from_slice(&records[..records.len().saturating_sub(1)]);

        // The slot holds only the records the block still needs. Extra runs at
        // the page edge are the next row's first glyph copied there (truncated),
        // or a speck; they are not glyphs of this row. `fg_d26`'s first row had
        // 36 block records, 36 regular runs and a 37th duplicate `E` at the
        // edge; keeping it mapped the real `E` onto the duplicate.
        let regular = runs.iter().filter(|(a, _)| *a < w - 13).count();
        let wanted = block.len().saturating_sub(regular);
        if runs.len() > regular + wanted {
            runs.truncate(regular + wanted);
        }

        let (cost, k, matches) = match best_alignment(&block, &runs, &raw_width) {
            Some((cost, k, matches)) => (cost, k, matches),
            None => (0, 0, Vec::new()),
        };
        out.push(RowAlignment {
            top,
            bottom,
            runs,
            block,
            k,
            cost,
            matches,
        });
        previous_last = records.last().copied();
    }
    out
}

/// The best row rotation for a page row: the block rotated left by `k`, with
/// blank records skipped. Costs are in pixels; a skipped block record is a blank
/// glyph (`BLANK`) and a skipped run is a duplicate or speck (`SKIP_RUN`).
const BLANK: i32 = 10;
const SKIP_RUN: i32 = 15;

type Match = (usize, usize);
type Alignment = (i32, usize, Vec<Match>);

fn best_alignment(block: &[usize], runs: &[(i32, i32)], raw_width: &[i32]) -> Option<Alignment> {
    let n = block.len();
    let r = runs.len();
    if n == 0 || r == 0 {
        return None;
    }
    let mut best: Option<Alignment> = None;
    for k in 0..=r.min(3) {
        if let Some((cost, matches)) = align(block, runs, raw_width, k) {
            let better = match &best {
                None => true,
                Some((best_cost, _, _)) => cost < *best_cost,
            };
            if better {
                best = Some((cost, k, matches));
            }
        }
    }
    best
}

/// Align the page's runs to `block` rotated left by `k`, in order, allowing
/// blank block records and extra runs. Returns the cost and `(record, run)`.
fn align(
    block: &[usize],
    runs: &[(i32, i32)],
    raw_width: &[i32],
    k: usize,
) -> Option<(i32, Vec<Match>)> {
    let n = block.len();
    let r = runs.len();
    let inf = i32::MAX / 4;
    let mut dp = vec![vec![inf; n + 1]; r + 1];
    // 0 = unchecked, 1 = blank block record, 2 = skip run, 3 = match.
    let mut par = vec![vec![0u8; n + 1]; r + 1];
    dp[0][0] = 0;
    for i in 0..=r {
        for j in 0..=n {
            let cur = dp[i][j];
            if cur >= inf {
                continue;
            }
            if j < n {
                let c = cur + BLANK;
                if c < dp[i][j + 1] {
                    dp[i][j + 1] = c;
                    par[i][j + 1] = 1;
                }
            }
            if i < r {
                let c = cur + SKIP_RUN;
                if c < dp[i + 1][j] {
                    dp[i + 1][j] = c;
                    par[i + 1][j] = 2;
                }
                if j < n {
                    let next = (j + k + 1) % n;
                    let width = runs[i].1 - runs[i].0 + 1;
                    let c = cur + (width - raw_width[block[next]]).abs();
                    if c < dp[i + 1][j + 1] {
                        dp[i + 1][j + 1] = c;
                        par[i + 1][j + 1] = 3;
                    }
                }
            }
        }
    }
    if dp[r][n] >= inf {
        return None;
    }
    let mut i = r;
    let mut j = n;
    let mut matches = Vec::new();
    while i > 0 || j > 0 {
        match par[i][j] {
            1 => j -= 1,
            2 => i -= 1,
            3 => {
                matches.push((block[(j - 1 + k) % n], i - 1));
                i -= 1;
                j -= 1;
            }
            _ => return None,
        }
    }
    matches.sort_unstable_by_key(|&(_, run)| run);
    Some((dp[r][n], matches))
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

    /// The cells are the page's own glyph runs: every cell lies inside the page,
    /// cells that share a packed row are ordered and **do not overlap**, and a
    /// cell is never wider or taller than the packed row's pitch. This replaces
    /// the older `the_rects_tile_the_page`: the file's records do not tile the
    /// page (their packing is a different one), but the cells the parser derives
    /// from the page now do.
    #[test]
    fn the_cells_are_the_pages_glyph_runs() {
        for face in ["fg_d20", "fg_mcn14", "fg_mcn16"] {
            let Some(font) = shipped(face) else {
                return;
            };
            let (w, h) = (font.page_width as i32, font.page_height as i32);
            let mut rows: std::collections::BTreeMap<i32, Vec<(i32, i32)>> = Default::default();
            let mut cells = 0;
            for (i, g) in font.glyphs.iter().enumerate() {
                let (x, y, gw, gh) = g.rect(font.page_width, font.page_height);
                assert!(
                    x >= 0 && y >= 0 && x + gw <= w && y + gh <= h,
                    "{face} record {i}"
                );
                if g.advance <= 0 || g.advance > 200 || gw <= 0 || gh <= 0 {
                    continue;
                }
                cells += 1;
                assert!(gw <= 32 && gh <= 24, "{face} record {i} is {gw}x{gh}");
                rows.entry(y).or_default().push((x, x + gw));
            }
            assert!(cells >= 90, "{face}: only {cells} cells");
            for (y, mut runs) in rows {
                runs.sort_unstable();
                for pair in runs.windows(2) {
                    assert!(
                        pair[0].1 <= pair[1].0,
                        "{face}: cells {}..{} and {}..{} overlap on row {y}",
                        pair[0].0,
                        pair[0].1,
                        pair[1].0,
                        pair[1].1
                    );
                }
            }

            let (w, line) = font.extent("ABC");
            assert_eq!(w, font.advance('A') + font.advance('B') + font.advance('C'));
            assert_eq!(line, font.line_height());
        }
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

    /// **The page holds the `O`; the record's rect was looking at its
    /// neighbour.** Under the file's rects `fg_d20`'s `O` held 21 pixels over 3
    /// columns — the old reading called the letter absent — because the rect
    /// spans 235..246 on the row *below* the page's `O`. Repacked from the page,
    /// the same record is the whole `O` at 243..255 on the `P..c` row: 116 pixels
    /// over 12 columns, and `EXHIBITION` reads. `fg_mcn16`'s `O` is whole too.
    #[test]
    fn the_font_page_supplies_the_o_the_records_misplace() {
        let Some(d20) = shipped("fg_d20") else { return };
        let (x, y, w, h) = d20.glyphs[45].rect(d20.page_width, d20.page_height);
        let (ink, columns) = glyph_ink(&d20, 45);
        assert_eq!((x, y, w, h), (243, 205, 13, 18), "fg_d20's O cell");
        assert!(
            ink >= 60 && columns >= 8,
            "fg_d20's O is {ink} px over {columns} columns"
        );

        let Some(mcn) = shipped("fg_mcn16") else {
            return;
        };
        let (ink, columns) = glyph_ink(&mcn, 45);
        assert!(
            ink >= 40 && columns >= 6,
            "fg_mcn16's O is {ink} px over {columns} columns"
        );
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

    /// **The page rows are measured, and the last record of a row is carried
    /// into the next.** Each row is a record block (`previous row's last` + this
    /// row's records but its last) drawn as the page's runs rotated left by the
    /// number of records in the right slot. The table below is the measurement:
    /// `(block records, ink runs, rotation k, matched runs, cost)`. `fg_mcn14`
    /// is the face the help screen uses; its second row is `E..c` then `D` and
    /// its third `f..~` then `d`, `e` — the one-glyph shift the old order-only
    /// mapping produced. `fg_mcn16` is the working face beside it.
    #[test]
    fn the_page_rows_are_measured_and_rotated() {
        type RowSummary = (usize, usize, usize, usize, i32);
        let expected: &[(&str, &[RowSummary])] = &[
            (
                "fg_mcn14",
                &[(34, 34, 1, 34, 23), (32, 32, 1, 32, 11), (27, 27, 2, 27, 8)],
            ),
            (
                "fg_mcn16",
                &[
                    (31, 31, 1, 31, 23),
                    (29, 29, 1, 29, 21),
                    (33, 33, 1, 33, 25),
                ],
            ),
            (
                "fg_d20",
                &[
                    (23, 23, 1, 23, 18),
                    (22, 22, 1, 22, 4),
                    (21, 21, 1, 21, 6),
                    (26, 26, 1, 26, 10),
                    (1, 1, 0, 1, 4),
                ],
            ),
        ];
        for (face, table) in expected {
            let Some(data) = shipped_bytes(face) else {
                return;
            };
            let raw = parse_raw(&data).expect("a raw font");
            let rows = page_rows(&raw);
            let summary: Vec<RowSummary> = rows
                .iter()
                .map(|r| (r.block.len(), r.runs.len(), r.k, r.matches.len(), r.cost))
                .collect();
            assert_eq!(&summary, table, "{face}: (block, runs, k, matches, cost)");
        }

        // The run-width sequences are the other half of the measurement: they
        // are what the alignment matches against `next record's cell width`.
        let widths = |face: &str, row: usize| -> Vec<i32> {
            let data = shipped_bytes(face).unwrap();
            let raw = parse_raw(&data).unwrap();
            page_rows(&raw)[row]
                .runs
                .iter()
                .map(|(a, b)| b - a + 1)
                .collect()
        };
        assert_eq!(
            widths("fg_mcn14", 1),
            [
                3, 7, 8, 7, 3, 5, 8, 6, 10, 7, 8, 7, 8, 7, 7, 7, 7, 7, 10, 7, 7, 6, 3, 7, 3, 7, 7,
                4, 6, 7, 6, 8
            ]
        );
        assert_eq!(
            widths("fg_mcn16", 1),
            [
                4, 8, 8, 7, 6, 9, 7, 2, 5, 8, 6, 10, 7, 9, 7, 9, 7, 8, 8, 7, 8, 11, 8, 8, 7, 3, 8,
                3, 8
            ]
        );

        // The rotation carries the record the page puts in the right slot: the
        // second row of `fg_mcn14` starts at `E` and ends with the previous
        // row's `D`; its third starts at `f` and ends `d`, `e`.
        let rows_of = |face: &str| {
            let data = shipped_bytes(face).unwrap();
            let raw = parse_raw(&data).unwrap();
            page_rows(&raw)
        };
        let first_last = |rows: &[RowAlignment], row: usize| -> (u32, u32) {
            let r = &rows[row];
            (
                r.matches.first().unwrap().0 as u32 + FIRST_CHAR,
                r.matches.last().unwrap().0 as u32 + FIRST_CHAR,
            )
        };
        let m14 = rows_of("fg_mcn14");
        assert_eq!(first_last(&m14, 0), (35, 34), "fg_mcn14 row 0");
        assert_eq!(first_last(&m14, 1), (69, 68), "fg_mcn14 row 1");
        assert_eq!(first_last(&m14, 2), (102, 101), "fg_mcn14 row 2");
        let m16 = rows_of("fg_mcn16");
        assert_eq!(first_last(&m16, 0).0, 35, "fg_mcn16 row 0 starts at #");
        assert_eq!(first_last(&m16, 1), (66, 65), "fg_mcn16 row 1");
        assert_eq!(first_last(&m16, 2), (95, 94), "fg_mcn16 row 2");
    }

    /// Every face's forty-odd packed records are mapped onto real page runs:
    /// **94 cells** per face, all but a sentinel or two holding ink. This is the
    /// cross-face check that the right-slot rotation (a row's first records live
    /// at the end of the previous row's slot) is not specific to one face.
    #[test]
    fn every_face_maps_its_page_runs() {
        let faces = every_face();
        if faces.is_empty() {
            return;
        }
        assert_eq!(faces.len(), 9, "the shipped faces");
        for (name, font) in &faces {
            let mut mapped = 0;
            let mut with_ink = 0;
            for (i, g) in font.glyphs.iter().enumerate() {
                if g.advance <= 0 || g.advance > 200 {
                    continue;
                }
                let (_, _, w, h) = g.rect(font.page_width, font.page_height);
                if w <= 0 || h <= 0 {
                    continue;
                }
                mapped += 1;
                if glyph_ink(font, i).0 > 0 {
                    with_ink += 1;
                }
            }
            assert_eq!(mapped, 94, "{name}: mapped cells");
            assert!(with_ink >= 92, "{name}: only {with_ink} cells hold ink");
        }
    }

    /// The header is invariant across all nine faces: word `0x0C` is exactly
    /// `1.0f` and the sixteen bytes at `0x18` are zero. The glyph table and the
    /// page are reconciled by `repack_from_page`; the header is what
    /// says the file has not been re-laid-out underneath the reader.
    #[test]
    fn the_font_header_is_invariant() {
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
    }

    /// **The advance table is two records ahead of the glyph table in every
    /// face**, not a side effect of the page's two leading characters. The
    /// numbers: the glyph record for `i` carries 9/11/13/14/6/7/6/6/7 across the
    /// nine faces (`fg_d18`…`fg_mcn16`), while the record two on carries
    /// 4/5/6/6/3/3/3/3/3 — the page's `i` is 3–6 columns. `m` is the mirror
    /// (glyph 5–14, two on 9–21); `n` and `o` likewise. The page's first visible
    /// run is `#` (the quote is in the right slot) and the two characters before
    /// record 34 are space-like and have no run; that is the **page** layout, and
    /// it is not what shifts the advances. The glyph table stays at `c - 34`;
    /// only the advance is read at `c - 32`.
    #[test]
    fn the_advance_table_is_two_records_ahead_of_the_page() {
        for face in [
            "fg_d18", "fg_d20", "fg_d24", "fg_d26", "fg_dcn14", "fg_dcn16", "fg_mcn12", "fg_mcn14",
            "fg_mcn16",
        ] {
            let Some(font) = shipped(face) else { return };
            assert!(
                font.advance('i') <= 6 && font.advance('i') < font.glyph('i').advance,
                "{face}: i is {}/{} (effective/glyph)",
                font.advance('i'),
                font.glyph('i').advance
            );
            assert!(
                font.advance('m') > font.glyph('m').advance,
                "{face}: m is {}/{} (effective/glyph)",
                font.advance('m'),
                font.glyph('m').advance
            );
            assert!(
                font.advance('n') > font.glyph('n').advance,
                "{face}: n is {}/{} (effective/glyph)",
                font.advance('n'),
                font.glyph('n').advance
            );
            assert!(
                font.advance('o') < font.glyph('o').advance,
                "{face}: o is {}/{} (effective/glyph)",
                font.advance('o'),
                font.glyph('o').advance
            );
        }

        let Some(font) = shipped("fg_mcn16") else {
            return;
        };
        assert_eq!(font.advance('i'), 3, "i's true advance");
        assert_eq!(font.advance('m'), 11, "m's true advance");
        assert_eq!(font.advance('n'), 7, "n's true advance");
        assert_eq!(font.advance('o'), 7, "o's true advance");
        // The glyph record still carries the old values, which is why the shift
        // is visible at all.
        assert_eq!(font.glyph('i').advance, 7);
        assert_eq!(font.glyph('n').advance, 3);
        // The cell is the page's `i`: three columns, not the advance.
        let (_, _, w, _) = font.glyph('i').rect(font.page_width, font.page_height);
        assert_eq!(w, 3);

        // A run's width is exactly the sum of its advances.
        let sum: i32 = "Options".chars().map(|c| font.advance(c)).sum();
        assert_eq!(font.extent("Options"), (sum, font.line_height()));
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
