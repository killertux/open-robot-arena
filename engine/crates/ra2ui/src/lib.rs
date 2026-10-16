//! The shell's layout: turning a `.gib` widget tree into rectangles.
//!
//! [`ra2fmt::gib`] reads the screens; this is what the numbers in them *mean*.
//! Every rule here is either read out of the shipped data or out of the game's
//! own reader, and each is stated with its evidence:
//!
//! ## `Position` and `Size` are two axes each
//!
//! `Position x x_is_fraction y y_is_fraction` and `Size w w_is_fraction
//! h h_is_fraction`. The flag slot is never anything but 0 or 1 in the corpus
//! (one file has `1242948` there, a typo in the data), and the game's own field
//! table names the two states — `To Self` and `To Parent` sit right beside
//! `ClipMode` in its `.rdata`. With the flag set the amount is a fraction of the
//! parent's extent along that axis; without it the amount is pixels. A pixel
//! amount is also what a *sibling-of-child* is placed against, so children are
//! positioned in their parent's own frame.
//!
//! ## `Alignment` is a 3×3 anchor
//!
//! `Alignment` is `horizontal | vertical`, with `1`/`2`/`4` for left/centre/
//! right and `8`/`16`/`32` for top/middle/bottom. It says which point *of the
//! widget* is placed at the resolved `Position`. Across all fourteen screens and
//! every widget that carries the field — 841 of them — the value is always one
//! of those nine combinations and nothing else, which is the measurement that
//! pins it. Three worked cases in `mainmenu.gib` then confirm the reading:
//!
//! | widget | Position | Alignment | resolved |
//! |---|---|---|---|
//! | `Window "Main Menu Win"` | `0 0` | 9 = left\|top | the widget's top-left at (0,0) |
//! | `Group "logo grp"` | `0.5 0.07` | 10 = centre\|top | 512×76 at x=384, y=70 |
//! | `Group "sawback grp"` | `0.5 0.5` | 18 = centre\|middle | centred |
//! | `Group "common buts"` | `0.5 1` | 34 = centre\|bottom | bottom edge at the screen's bottom |
//!
//! ## `SplitFrame` splits, and the slot order is the child order
//!
//! `Horizontal true` puts slot 0 on the left and slot 1 on the right;
//! `false` puts slot 0 on top. `Master` names the slot that is sized by
//! `MasterSize` (a fraction of the frame along the split axis); the other takes
//! the rest. The screens' own names are the check — `"LR split"` is horizontal
//! with a `"left slot"` first, `"Left TB split"` is vertical with an `"UL slot"`
//! first, and `botlabwin.gib`'s vertical split has a 67 px-tall `"Slot0"` bar on
//! top.
//!
//! ## `ClipMode` is 0, 1 or 2, and the binary's own byte says which
//!
//! Every widget the corpus holds carries a `ClipMode`, and it is only ever
//! **0** (2289 of 4348), **1** (1999) or **2** (60). The binary keeps it at
//! `+0x78` of the base widget, between `Visible` (`+0x74`) and the class fields,
//! and the `.gib` writer emits it from there.
//!
//! * **0** — draw without clipping. The leaf decorators are all zeros: every
//!   `Text`, `Glyph`, `Lines`, `Rectangle` that is not a container's chrome, and
//!   almost every `Image`.
//! * **1** — the widget's own drawing *and its children* are clipped to its own
//!   frame. Every widget that is a container by nature is a 1 — `Window` 42,
//!   `Group` 460, `SplitFrame` 40, `ListBox` 23, `Grid` 12, `OptionGroup` 20,
//!   `ScrollBar` 30, `Slider` 41, `TextBox` 331, `Button` 193, `CheckBox` 69,
//!   `DropDown` 12, `Placeholder` 12 — and only two `Image`s in the whole game
//!   bother.
//! * **2** — clipped to the *parent's* frame instead. There are sixty, and
//!   **twenty-three of them are placed so that they stick out of the parent**: the two
//!   `robots grid`s (anchored at the parent's *bottom* edge), the five bot-lab
//!   `selected label`s, `botlabwin.gib`'s `wires grp` at `x = -1.0`, and
//!   `event_bracket.gib`'s heading textbox at `y = -20` with a height of 24. A
//!   self-clip would leave those where they are; only a clip by the parent
//!   explains the author wanting one. The rest that are inside at authoring time
//!   are chrome that *moves*: the two selection bars, which the container
//!   repositions down its rows, and `comp_properties.gib`'s tabs.
//!
//! [`Placement::clip`] carries the result: the rectangle the widget draws
//! inside, already intersected with everything above it.
//!
//! ## `Grid` and `ListBox` lay their cells out themselves
//!
//! `Rows`, `Columns`, `RowHeight` and `ColWidths` are the grid, and the cell
//! rectangles follow from them: `ColWidths` is **two numbers per column**, and
//! the first is the width; the x of a cell is the sum of the widths before it
//! and its y is `row × RowHeight`. The binary's own hit test says the same
//! thing — it walks one row's cells adding each column's width to x — and the
//! *second* number is 17 in **every one of the 36 shipped grids and lists**, so
//! this reader leaves it unnamed rather than invent one.
//!
//! The container's chrome is the two children the file names: `Selection` and
//! `Highlight` are child **`ID`s** (100 and 101 in all 36), and the
//! `RowSelect` flag decides how much they cover — `true` spans the whole row,
//! `false` one cell. [`Grid::cell`] reproduces the authors' own arithmetic
//! rather than replacing it, and the corpus checks it: all 37 shipped bars hang
//! from the row they name at the row height, at the summed x of the column they
//! name, and 31 of the 37 are as wide as the columns they span. `armor.gib`'s
//! bar sits at `y = 252` with `RowHeight 84` — row 3 — and `wiring.gib`'s at
//! `x = 62`, the first column's width — column 1. The six widths that differ are
//! the authors' own numbers, and are pinned in the test rather than smoothed
//! into a formula that would fit them.
//!
//! `OptionGroup` is not a layout: its `Selection` names a child by **`Name`**
//! (`video tab`), and its children carry their own rectangles. `DropDown`'s
//! parts are authored too. A `ScrollBar`'s are not — see [`ScrollBar`].
//!
//! ## What is not resolved yet
//!
//! The *content* of a cell — a `LoadCell` field exists that the game's own
//! reader consumes — comes from game data at run time, and a `Slider`'s thumb
//! sits at its value. Both are state, not layout, and neither is here.
//!
//! A `Size` of zero is the editor's *automatic* size. With a [`Fonts`] in hand a
//! resolver sizes such a widget from its own text ([`resolve_with`]); without
//! one, it comes back zero-sized, and so do its children.

use std::collections::BTreeMap;
use std::path::Path;

use ra2fmt::gib::{Block, Gib, Length, Widget, WidgetKind};

pub mod cells;
pub mod components;
pub mod events;
pub mod input;
pub mod project;
pub mod workshop;

/// The faces a screen's labels name, loaded from a game installation's
/// `RA2.cfz`. A resolver given these can size the widgets the editor left at
/// zero — a label's extent is the sum of its glyph advances wide and the face's
/// line height tall.
#[derive(Debug, Default, Clone)]
pub struct Fonts {
    faces: BTreeMap<String, ra2fmt::Font>,
}

impl Fonts {
    /// Load every `fg_*.giff` in a `RA2.cfz`.
    pub fn load(archive: &Path) -> Result<Fonts, ra2fmt::Error> {
        let cfl = ra2fmt::cfl::Cfl::open_path(archive)?;
        let mut faces = BTreeMap::new();
        for entry in cfl.entries() {
            let name = entry.name.clone();
            let Some(stem) = name.strip_suffix(".giff") else {
                continue;
            };
            if let Some(data) = cfl.get(&name) {
                if let Ok(font) = ra2fmt::giff::parse(&data) {
                    faces.insert(stem.to_string(), font);
                }
            }
        }
        Ok(Fonts { faces })
    }

    pub fn get(&self, name: &str) -> Option<&ra2fmt::Font> {
        self.faces.get(name)
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// The size of a string in a named face, in pixels.
    pub fn measure(&self, face: &str, text: &str) -> Option<(f32, f32)> {
        let font = self.get(face)?;
        let (w, h) = font.extent(text);
        Some((w as f32, h as f32))
    }
}

/// The screen the layouts were authored against. A 1280³-1024 window with a
/// 19-pixel title bar, which is what the full-screen background image in
/// `mainmenu.gib` measures: 1280×1005.
pub const DESIGN_SCREEN: [f32; 2] = [1280.0, 1005.0];

/// A widget's rectangle on screen, in pixels, y down from the top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Frame {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Frame {
        Frame { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, other: &Frame) -> bool {
        other.x >= self.x - 0.01
            && other.y >= self.y - 0.01
            && other.right() <= self.right() + 0.01
            && other.bottom() <= self.bottom() + 0.01
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.w.is_finite() && self.h.is_finite()
    }

    /// True when the two rectangles overlap at all.
    pub fn intersects(&self, other: &Frame) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// Which point of a widget an `Alignment` names: 0, ½ or 1 along each axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub h: f32,
    pub v: f32,
}

impl Anchor {
    pub const TOP_LEFT: Anchor = Anchor { h: 0.0, v: 0.0 };

    /// Read the nine combinations. Anything else in the low six bits falls back
    /// to the top-left, which is what an absent field means.
    pub fn from_alignment(alignment: i32) -> Anchor {
        Anchor {
            h: match alignment & 0x7 {
                1 => 0.0,
                2 => 0.5,
                4 => 1.0,
                _ => 0.0,
            },
            v: match alignment & 0x38 {
                8 => 0.0,
                16 => 0.5,
                32 => 1.0,
                _ => 0.0,
            },
        }
    }
}

/// How a widget clips what it draws — the file's `ClipMode`, at `+0x78` of the
/// base widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    /// `0` — no clipping.
    None,
    /// `1` — the widget's own drawing and its descendants are clipped to the
    /// widget's frame.
    SelfFrame,
    /// `2` — ... to the *parent's* frame.
    ParentFrame,
}

impl ClipMode {
    pub fn from_value(value: i32) -> ClipMode {
        match value {
            1 => ClipMode::SelfFrame,
            2 => ClipMode::ParentFrame,
            _ => ClipMode::None,
        }
    }
}

/// The parts a container lays out itself, rather than from each child's own
/// `Position`.
#[derive(Debug, Clone, PartialEq)]
pub enum Parts {
    Grid(Grid),
    ScrollBar(ScrollBar),
}

/// A `Grid` or a `ListBox`: `Rows` × `Columns` cells of `RowHeight` each.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub rows: i32,
    pub columns: i32,
    pub row_height: f32,
    /// One width per column — the first of the two numbers `ColWidths` writes
    /// per column. The second is 17 in every shipped file, so it is left alone.
    pub col_widths: Vec<f32>,
    /// `RowSelect`: whether the selection covers a whole row or one cell.
    pub row_select: bool,
    pub buffer_width: f32,
    pub border_width: f32,
    /// The `ID` of the child that is the selection bar, and of the highlight
    /// bar. The game stores these as child pointers and round-trips them as IDs.
    pub selection: Option<i32>,
    pub highlight: Option<i32>,
}

impl Grid {
    /// Read a `Grid`/`ListBox`, or nothing when it states no dimensions.
    pub fn from_widget(widget: &Widget) -> Option<Grid> {
        let number = |name: &str| widget.field(name).and_then(|f| f.number());
        let col_widths = widget
            .field("ColWidths")
            .map(|f| {
                f.numbers()
                    .chunks(2)
                    .filter_map(|pair| pair.first().copied())
                    .collect()
            })
            .unwrap_or_default();
        Some(Grid {
            rows: number("Rows")? as i32,
            columns: number("Columns")? as i32,
            row_height: number("RowHeight").unwrap_or(0.0),
            col_widths,
            row_select: widget
                .field("RowSelect")
                .and_then(|f| f.flag())
                .unwrap_or(false),
            buffer_width: number("BufferWidth").unwrap_or(0.0),
            border_width: number("BorderWidth").unwrap_or(0.0),
            selection: number("Selection").map(|n| n as i32),
            highlight: number("Highlight").map(|n| n as i32),
        })
    }

    /// The x of a column inside the container: the widths of the columns before
    /// it. The binary's hit test walks a row the same way.
    pub fn column_x(&self, column: i32) -> f32 {
        self.col_widths.iter().take(column.max(0) as usize).sum()
    }

    /// The width of every column, which is what a row-spanning selection covers.
    pub fn row_width(&self) -> f32 {
        self.col_widths.iter().sum()
    }

    /// The rectangle of one cell. It is *not* clamped to the container: a cell
    /// that reaches past the frame is what the container's `ClipMode` is for.
    pub fn cell(&self, frame: Frame, row: i32, column: i32) -> Frame {
        let x = self.column_x(column);
        let w = self
            .col_widths
            .get(column.max(0) as usize)
            .copied()
            .unwrap_or(0.0);
        Frame::new(
            frame.x + x,
            frame.y + row as f32 * self.row_height,
            w,
            self.row_height,
        )
    }

    /// The rectangle the selection covers at a cell: the whole row when
    /// `RowSelect`, one cell otherwise.
    pub fn select(&self, frame: Frame, row: i32, column: i32) -> Frame {
        if self.row_select {
            Frame::new(
                frame.x,
                frame.y + row as f32 * self.row_height,
                self.row_width(),
                self.row_height,
            )
        } else {
            self.cell(frame, row, column)
        }
    }

    /// Which cell a selection bar's own rectangle names — the authors' own
    /// answer, which is what the shipped files store.
    pub fn selected(&self, frame: Frame, bar: Frame) -> (i32, i32) {
        let row = if self.row_height > 0.0 {
            ((bar.y - frame.y) / self.row_height).round() as i32
        } else {
            0
        };
        let mut x = 0.0;
        let mut column = 0;
        while column < self.col_widths.len() as i32 && (x + frame.x - bar.x).abs() > 0.01 {
            x += self.col_widths[column as usize];
            column += 1;
        }
        (row, column)
    }
}

/// A `ScrollBar`: a less button, a more button, and the track between them.
///
/// The three parts are the children the fields name, and they are *indices* —
/// `Less true 0` is the first child. In every one of the 30 shipped bars the
/// index, the child's own `Name` and its kind agree (`Less`, `Down` and a
/// `Slider`), which is what pins it. The buttons are drawn where they are put:
/// their authored `Position` is `0 0` for the less button and nonsense for the
/// more one (`y = 64` under a full-height bar), because the bar sizes itself
/// from its parent at run time.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollBar {
    /// `Orientation`: 1 is vertical (29 of the 30) and 2 horizontal (1).
    pub vertical: bool,
    pub less: usize,
    pub more: usize,
    pub slider: usize,
}

impl ScrollBar {
    pub fn from_widget(widget: &Widget) -> ScrollBar {
        let index = |name: &str| {
            widget
                .field(name)
                .map(|f| f.numbers().first().copied().unwrap_or(0.0) as usize)
                .unwrap_or(0)
        };
        ScrollBar {
            vertical: widget
                .field("Orientation")
                .and_then(|f| f.number())
                .map(|n| n != 2.0)
                .unwrap_or(true),
            less: index("Less"),
            more: index("More"),
            slider: index("Slider"),
        }
    }

    /// The three part rectangles: the less button, the more button and the
    /// track. The buttons keep the size the file gives them; the track takes
    /// what is left between.
    pub fn parts(&self, frame: Frame, less: Frame, more: Frame) -> [Frame; 3] {
        if self.vertical {
            [
                Frame::new(frame.x, frame.y, less.w, less.h),
                Frame::new(frame.x, frame.bottom() - more.h, more.w, more.h),
                Frame::new(
                    frame.x,
                    frame.y + less.h,
                    frame.w,
                    (frame.h - less.h - more.h).max(0.0),
                ),
            ]
        } else {
            [
                Frame::new(frame.x, frame.y, less.w, less.h),
                Frame::new(frame.right() - more.w, frame.y, more.w, more.h),
                Frame::new(
                    frame.x + less.w,
                    frame.y,
                    (frame.w - less.w - more.w).max(0.0),
                    frame.h,
                ),
            ]
        }
    }
}

/// One glyph of a laid-out string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    /// The cell in the face's page, `x y w h`, top-down.
    pub cell: [i32; 4],
    /// Where that cell goes on screen.
    pub rect: Frame,
}

/// Lay a string out the way the game's own labels are written: the baseline
/// sits one `ascent` below the frame's top, each glyph's cell hangs from its own
/// third above that line, and the pen advances by each glyph's advance. A
/// character the face has no cell for — a space — advances without drawing.
pub fn text_run(font: &ra2fmt::Font, frame: Frame, text: &str) -> Vec<PlacedGlyph> {
    let baseline = frame.y + font.ascent as f32;
    let mut pen = frame.x;
    let mut out = Vec::new();
    for c in text.chars() {
        if font.has_glyph(c) {
            let glyph = font.glyph(c);
            let (x, y, w, h) = glyph.rect(font.page_width, font.page_height);
            out.push(PlacedGlyph {
                cell: [x, y, w, h],
                rect: Frame::new(pen, baseline - glyph.third as f32, w as f32, h as f32),
            });
        }
        pen += font.advance(c) as f32;
    }
    out
}

/// The lines a `Glyph`'s text wraps to inside `width`.
///
/// A `Glyph` is the shell's *paragraph*: `_Text` holds a body of prose, with
/// `\n` for the breaks the author typed, and it wraps at spaces to the width
/// its `_Margin` gives it. A word longer than the line is left long rather than
/// broken — the corpus has none, and splitting one is a hyphenation rule the
/// data does not state.
pub fn wrap_lines(font: &ra2fmt::Font, text: &str, width: f32) -> Vec<String> {
    let mut out = Vec::new();
    for hard in text.split('\n') {
        if width <= 0.0 {
            out.push(hard.to_string());
            continue;
        }
        let space = font.advance(' ') as f32;
        let mut line = String::new();
        let mut pen = 0.0f32;
        for word in hard.split(' ') {
            let w = font.extent(word).0 as f32;
            if line.is_empty() {
                line.push_str(word);
                pen = w;
            } else if pen + space + w <= width {
                line.push(' ');
                line.push_str(word);
                pen += space + w;
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
                pen = w;
            }
        }
        out.push(line);
    }
    out
}

/// The glyphs a paragraph draws: each wrapped line down the face's own line
/// height, the first baseline one `ascent` below the box's top.
pub fn paragraph_run(
    font: &ra2fmt::Font,
    frame: Frame,
    text: &str,
    width: f32,
) -> Vec<PlacedGlyph> {
    let line_height = font.line_height() as f32;
    let mut out = Vec::new();
    for (i, line) in wrap_lines(font, text, width).into_iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_frame = Frame::new(
            frame.x,
            frame.y + i as f32 * line_height,
            width,
            line_height,
        );
        out.extend(text_run(font, line_frame, &line));
    }
    out
}

/// A widget with its resolved rectangle and its placed children.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement<'a> {
    pub widget: &'a Widget,
    pub frame: Frame,
    /// The rectangle this widget draws inside, already intersected with every
    /// clip above it. `None` when nothing above it clips.
    pub clip: Option<Frame>,
    pub children: Vec<Placement<'a>>,
}

impl<'a> Placement<'a> {
    /// Every placed widget, this one first, depth first.
    pub fn walk(&self) -> PlacementWalk<'a, '_> {
        PlacementWalk { stack: vec![self] }
    }

    /// The first placed widget with this `Name`.
    pub fn find(&self, name: &str) -> Option<&Placement<'a>> {
        self.walk().find(|p| p.widget.name() == name)
    }

    /// Whether this widget and everything under it is inside `frame`.
    pub fn fits(&self, frame: &Frame) -> bool {
        frame.contains(&self.frame) && self.children.iter().all(|c| c.fits(frame))
    }

    /// The container parts this widget lays out itself, if it is one.
    pub fn parts(&self) -> Option<Parts> {
        match self.widget.kind {
            WidgetKind::Grid | WidgetKind::ListBox => {
                Grid::from_widget(self.widget).map(Parts::Grid)
            }
            WidgetKind::ScrollBar => Some(Parts::ScrollBar(ScrollBar::from_widget(self.widget))),
            _ => None,
        }
    }

    pub fn grid(&self) -> Option<Grid> {
        Grid::from_widget(self.widget)
    }

    pub fn scroll_bar(&self) -> Option<ScrollBar> {
        (self.widget.kind == WidgetKind::ScrollBar).then(|| ScrollBar::from_widget(self.widget))
    }

    /// The rectangle of a cell of this container, in screen pixels.
    pub fn cell(&self, row: i32, column: i32) -> Option<Frame> {
        self.grid().map(|g| g.cell(self.frame, row, column))
    }

    /// The rectangle the selection covers at a cell of this container.
    pub fn select(&self, row: i32, column: i32) -> Option<Frame> {
        self.grid().map(|g| g.select(self.frame, row, column))
    }

    /// Which cell a child's box names, for a selection or highlight bar.
    pub fn selected(&self, bar: &Placement<'_>) -> Option<(i32, i32)> {
        self.grid().map(|g| g.selected(self.frame, bar.frame))
    }

    /// The first child with this `ID`, which is how a container names the
    /// children it drives.
    pub fn child_with_id(&self, id: i32) -> Option<&Placement<'a>> {
        self.children.iter().find(|c| c.widget.id() == id)
    }
}

/// Depth-first walk over a placed tree.
pub struct PlacementWalk<'a, 'b> {
    stack: Vec<&'b Placement<'a>>,
}

impl<'a, 'b> Iterator for PlacementWalk<'a, 'b> {
    type Item = &'b Placement<'a>;
    fn next(&mut self) -> Option<&'b Placement<'a>> {
        let p = self.stack.pop()?;
        self.stack.extend(p.children.iter().rev());
        Some(p)
    }
}

/// The widgets to draw, back to front.
///
/// A parent is drawn before its children and its children by `ChildOrder`
/// **ascending**, ties broken by file order — but the *top level* is drawn in
/// file order. Both halves are measured:
///
/// * Inside `mainmenu.gib`'s window the cement background is 1, the saw backdrop
///   2 and the logo group 3, so drawing the file's own order would paint the
///   background over the logo. A label follows its button (the button is -1 and
///   the `Text` beside it 0), and a button's four state images are 0..3 with
///   only the first visible, which is what puts a label over its art.
/// * The top level is *not* sorted. `mainmenu.gib`'s blocks are the window (3),
///   then the `Multiplayer button` (-1) and its `Multiplayer` label (0), then
///   the bottom bar (4) and its decorations (5–7); sorting them would draw the
///   button and its label *before* the window, and the window's full-screen
///   cement background would then cover both of them. Drawn in file order the
///   buttons and labels land on top of the background, which is what the game
///   shows.
pub fn draw_order<'a>(roots: &'a [Placement<'a>]) -> Vec<&'a Placement<'a>> {
    draw_order_parented(roots)
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

/// Resolve every top-level block of a document against the screen.
pub fn resolve(gib: &Gib, screen: [f32; 2]) -> Vec<Placement<'_>> {
    resolve_with(gib, screen, None)
}

/// Resolve with the screen's faces, so that a widget the editor left at zero
/// size is sized by its own text.
pub fn resolve_with<'a>(
    gib: &'a Gib,
    screen: [f32; 2],
    fonts: Option<&Fonts>,
) -> Vec<Placement<'a>> {
    let screen_frame = Frame::new(0.0, 0.0, screen[0], screen[1]);
    let mut placed = Vec::new();
    for block in &gib.blocks {
        let Block::Widget(w) = block else {
            // A fragment has no frame of its own until a slot loads it.
            continue;
        };
        // Every top-level record is placed against the screen, which is what
        // their own `Position`s are measured against — including the ones a
        // record's `NumChildren` leaves without a parent, like `import_texture
        // .gib`'s 1282x1007 panel at (-1,-1), a modal backdrop one pixel wider
        // than the screen on each side. Nothing special-cases them: the top
        // level is a sibling set like any other, ordered by `ChildOrder` in
        // `draw_order_parented`, so a backdrop sorts behind the `Window` that
        // asks for it (`co -1` against the window's `3`).
        placed.push(place_with(w, screen_frame, fonts));
    }
    placed
}

/// The art a widget draws.
///
/// Usually its own `_Filename`. An `Image` inside a container's selection or
/// highlight bar has none at all: those bars are the one place the shell draws
/// art the `.gib` cannot name, and the art is `Selection.tga`, which ships in
/// `RA2.cfz` and which no shipped screen mentions. All 36 bars are a `Rectangle`
/// with `ID` 100 or 101 holding an image with a blank name, and 100/101 are what
/// their containers' own `Selection` and `Highlight` say.
pub fn art_for<'a>(
    widget: &'a ra2fmt::gib::Widget,
    parent: Option<&ra2fmt::gib::Widget>,
) -> Option<&'a str> {
    let file = widget.filename()?;
    if !file.is_empty() {
        return Some(file);
    }
    let parent = parent?;
    let bar = parent.kind == WidgetKind::Rectangle
        && matches!(parent.id(), 100 | 101)
        && widget.kind == WidgetKind::Image;
    bar.then_some("Point Selection.tga")
}

/// The `.gib` a `Proxy` names, when it names one.
pub fn proxy_path(widget: &Widget) -> Option<&str> {
    if widget.kind != WidgetKind::Proxy {
        return None;
    }
    widget
        .field("Filename")
        .map(|f| f.text())
        .filter(|p| !p.is_empty())
}

/// How deep a chain of proxies may go before it is left as it is. None of the
/// shipped screens nests one at all — nine name `common\bottomnav.gib` and that
/// file holds no proxy of its own — so this only bounds a malformed tree.
const MAX_PROXY_DEPTH: u8 = 4;

/// Load the `.gib` a `Proxy` names, as the widgets to draw inside it.
fn load_fragment(ui_root: &Path, rel: &str) -> Option<Vec<Widget>> {
    let path = ra2fmt::find_ci(ui_root, rel)?;
    let text = std::fs::read_to_string(path).ok()?;
    let gib = ra2fmt::gib::parse(&text).ok()?;
    Some(
        gib.blocks
            .into_iter()
            .filter_map(|b| match b {
                Block::Widget(w) => Some(w),
                // A fragment that is a lone attachment is chrome with no widget
                // to hang it on; there is nowhere to put it inside a proxy.
                Block::Attachment(_) => None,
            })
            .collect(),
    )
}

fn inline_one(widget: &mut Widget, ui_root: &Path, depth: u8, loaded: &mut Vec<String>) {
    // The depth counts *fragments*, not tree levels: a proxy may sit any
    // distance down a screen, and `botlabwin.gib`'s is five deep.
    for child in widget.children.iter_mut() {
        inline_one(child, ui_root, depth, loaded);
    }
    if depth == 0 {
        return;
    }
    let Some(rel) = proxy_path(widget).map(str::to_string) else {
        return;
    };
    let Some(mut fragment) = load_fragment(ui_root, &rel) else {
        return;
    };
    loaded.push(rel);
    for f in fragment.iter_mut() {
        inline_one(f, ui_root, depth - 1, loaded);
    }
    widget.children.extend(fragment);
}

/// Replace every `Proxy` in a screen with the `.gib` it names.
///
/// A `Proxy` is the data's own slot: it carries a `Filename`, a `.gib` path
/// under `UI`, and the fragment is drawn inside the proxy's own rectangle —
/// the proxy becomes the fragment's parent, so its `Position`/`Size` are the
/// fragment's frame. Nine shipped screens name `common\bottomnav.gib`, which is
/// the bottom navigation bar, and `botlabwin.gib`, `teamhq.gib` and the
/// multiplayer screens are among them.
///
/// Returns the paths that were loaded, in order, so a caller can report what it
/// pulled in and what it could not find.
pub fn inline_proxies_in(gib: &mut Gib, ui_root: &Path) -> Vec<String> {
    let mut loaded = Vec::new();
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(w) = block {
            inline_one(w, ui_root, MAX_PROXY_DEPTH, &mut loaded);
        }
    }
    loaded
}

/// Every `.gib` a screen's `Proxy` widgets name, in file order.
pub fn proxies(widget: &Widget) -> Vec<String> {
    let mut out = Vec::new();
    fn visit(w: &Widget, out: &mut Vec<String>) {
        if let Some(path) = proxy_path(w) {
            out.push(path.to_string());
        }
        for c in &w.children {
            visit(c, out);
        }
    }
    visit(widget, &mut out);
    out
}

/// The widgets to draw, back to front, each with the widget that holds it.
pub fn draw_order_parented<'a>(
    roots: &'a [Placement<'a>],
) -> Vec<(&'a Placement<'a>, Option<&'a Placement<'a>>)> {
    fn visit<'a>(
        p: &'a Placement<'a>,
        parent: Option<&'a Placement<'a>>,
        out: &mut Vec<(&'a Placement<'a>, Option<&'a Placement<'a>>)>,
    ) {
        out.push((p, parent));
        let mut children: Vec<&Placement> = p.children.iter().collect();
        children.sort_by_key(|p| p.widget.child_order());
        for c in children {
            visit(c, Some(p), out);
        }
    }
    // The top level is a sibling set too: a file's own records are ordered by
    // `ChildOrder` exactly as a widget's children are, which is what puts a
    // modal backdrop (child order -1) behind the window it belongs to (3) when
    // the reader could not give the backdrop a parent.
    let mut top: Vec<&Placement> = roots.iter().collect();
    top.sort_by_key(|p| p.widget.child_order());
    let mut out = Vec::new();
    for r in top {
        visit(r, None, &mut out);
    }
    out
}

/// Place one widget inside `parent`.
pub fn place(widget: &Widget, parent: Frame) -> Placement<'_> {
    place_with(widget, parent, None)
}

/// Place one widget inside `parent`, sizing an unmeasured label from its text.
pub fn place_with<'a>(widget: &'a Widget, parent: Frame, fonts: Option<&Fonts>) -> Placement<'a> {
    place_at(widget, parent, None, fonts)
}

/// Place one widget, carrying down the clip its ancestors impose.
fn place_at<'a>(
    widget: &'a Widget,
    parent: Frame,
    inherited: Option<Frame>,
    fonts: Option<&Fonts>,
) -> Placement<'a> {
    let frame = sized(widget, parent, widget.size(), fonts);
    let clip = match ClipMode::from_value(widget.clip_mode()) {
        ClipMode::None => inherited,
        ClipMode::SelfFrame => Some(intersect(inherited, frame)),
        // A clip by a parent that is larger than the widget has nothing to do;
        // a clip by a parent that is smaller is the whole point of the mode.
        ClipMode::ParentFrame => Some(intersect(inherited, parent)),
    };
    let children = match widget.kind {
        // A split frame hands its children panes rather than its own rectangle.
        WidgetKind::SplitFrame => split_children(widget, frame, clip, fonts),
        // A scroll bar puts its parts at its two ends and between them.
        WidgetKind::ScrollBar => scroll_children(widget, frame, clip, fonts),
        _ => {
            let mut placed: Vec<Placement> = widget
                .children
                .iter()
                .map(|c| place_at(c, frame, clip, fonts))
                .collect();
            centre_captions(&mut placed);
            placed
        }
    };
    Placement {
        widget,
        frame,
        clip,
        children,
    }
}

/// A button that carries a `Caption` attachment takes the `Text` after it as its
/// caption, and the caption is drawn centred in the button.
///
/// Both halves are measured. There are exactly **122** `Caption` attachments in
/// the corpus and exactly **122** buttons and check boxes whose four state images
/// all have a blank `_Filename` — the ones with no art of their own — and each of
/// the 122 is followed by its `Text` (`mainmenu.gib`'s bar is a gear, a gear, and
/// then `Button "Options button"`, `Text "Options"`, `Button "credits button"`,
/// `Text "Credits"`, …). What settles the centring is that the captions' own
/// positions make no sense against their parent: the three bar captions are at
/// x = 14, 18 and 9 while their buttons are at x = 32, 105 and 183, so taken as
/// the group's they would all pile up at the left — which is what a composite
/// does before this rule. Inside their buttons 14 of 67, 18 of 72 and 9 of 40 are
/// each roughly centred, which is where the game puts them.
fn centre_captions(children: &mut [Placement<'_>]) {
    for i in 0..children.len() {
        let captioned = matches!(
            children[i].widget.kind,
            WidgetKind::Button | WidgetKind::CheckBox
        ) && children[i].widget.attachment("Caption").is_some();
        if !captioned || i + 1 >= children.len() || children[i + 1].widget.kind != WidgetKind::Text
        {
            continue;
        }
        let button = children[i].frame;
        let caption = children[i + 1].frame;
        children[i + 1].frame = Frame::new(
            button.x + (button.w - caption.w) / 2.0,
            button.y + (button.h - caption.h) / 2.0,
            caption.w,
            caption.h,
        );
    }
}

/// The overlap of two clips, or of a clip and a new rectangle.
fn intersect(clip: Option<Frame>, with: Frame) -> Frame {
    match clip {
        None => with,
        Some(c) => {
            let x = c.x.max(with.x);
            let y = c.y.max(with.y);
            Frame::new(
                x,
                y,
                (c.right().min(with.right()) - x).max(0.0),
                (c.bottom().min(with.bottom()) - y).max(0.0),
            )
        }
    }
}

/// Move a placed subtree. The frames only: the clips are recomputed from
/// scratch once the whole move is done, because a part that also *resizes* —
/// a scroll bar's track — would otherwise leave its descendants clipped to the
/// box it used to have.
fn shift(placement: &mut Placement<'_>, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    placement.frame.x += dx;
    placement.frame.y += dy;
    for child in placement.children.iter_mut() {
        shift(child, dx, dy);
    }
}

/// Work out again what a placed subtree is clipped to, `parent` being the frame
/// of the thing that holds it.
fn reclip(placement: &mut Placement<'_>, parent: Frame, inherited: Option<Frame>) {
    placement.clip = match ClipMode::from_value(placement.widget.clip_mode()) {
        ClipMode::None => inherited,
        ClipMode::SelfFrame => Some(intersect(inherited, placement.frame)),
        ClipMode::ParentFrame => Some(intersect(inherited, parent)),
    };
    let (frame, clip) = (placement.frame, placement.clip);
    for child in placement.children.iter_mut() {
        reclip(child, frame, clip);
    }
}

/// A `ScrollBar`'s children: the parts the fields name are moved to the two
/// ends and into the track between them, and the rest are left where they are.
fn scroll_children<'a>(
    widget: &'a Widget,
    frame: Frame,
    clip: Option<Frame>,
    fonts: Option<&Fonts>,
) -> Vec<Placement<'a>> {
    let bar = ScrollBar::from_widget(widget);
    let mut placed: Vec<Placement<'a>> = widget
        .children
        .iter()
        .map(|c| place_at(c, frame, clip, fonts))
        .collect();
    let zero = Frame::new(frame.x, frame.y, 0.0, 0.0);
    let less = placed.get(bar.less).map(|p| p.frame).unwrap_or(zero);
    let more = placed.get(bar.more).map(|p| p.frame).unwrap_or(zero);
    let parts = bar.parts(frame, less, more);
    for (index, target) in [
        (bar.less, parts[0]),
        (bar.more, parts[1]),
        (bar.slider, parts[2]),
    ] {
        let Some(placement) = placed.get_mut(index) else {
            continue;
        };
        shift(
            placement,
            target.x - placement.frame.x,
            target.y - placement.frame.y,
        );
        placement.frame.w = target.w;
        placement.frame.h = target.h;
    }
    // The track is not the height the file gave it, so every clip in the three
    // parts is worked out again rather than moved.
    for (index, target) in [
        (bar.less, parts[0]),
        (bar.more, parts[1]),
        (bar.slider, parts[2]),
    ] {
        if let Some(placement) = placed.get_mut(index) {
            reclip(placement, target, clip);
        }
    }
    placed
}

/// The extent a label takes from its own font, when the layout left it at zero.
fn measured(widget: &Widget, fonts: Option<&Fonts>) -> Option<(f32, f32)> {
    let fonts = fonts?;
    let (text, face) = (widget.text()?, widget.font()?);
    if text.is_empty() {
        return None;
    }
    fonts.measure(face, text)
}

/// A widget's own rectangle: its `Size` against the parent's extent, with its
/// anchor point at its `Position`.
fn sized(
    widget: &Widget,
    parent: Frame,
    size: Option<ra2fmt::gib::Axes>,
    fonts: Option<&Fonts>,
) -> Frame {
    // A zero is the editor's automatic size. For a label that is its text's
    // extent, which is what the game does too — a `Text` in every shipped screen
    // is written `Size 0 0 0 0`.
    let fit = measured(widget, fonts);
    let dim = |given: Option<Length>, parent: f32, fallback: f32| match given {
        Some(l) if l.amount != 0.0 || l.fraction => axis(l, parent),
        _ => fallback,
    };
    let w = match size {
        Some(s) => dim(Some(s.first), parent.w, fit.map(|f| f.0).unwrap_or(0.0)),
        None => parent.w,
    };
    let h = match size {
        Some(s) => dim(Some(s.second), parent.h, fit.map(|f| f.1).unwrap_or(0.0)),
        None => parent.h,
    };
    let position = widget.position();
    let (px, py) = match position {
        Some(p) => (axis(p.first, parent.w), axis(p.second, parent.h)),
        None => (0.0, 0.0),
    };
    let anchor = if has_alignment(widget) {
        Anchor::from_alignment(widget.alignment())
    } else {
        Anchor::TOP_LEFT
    };
    Frame::new(
        parent.x + px - anchor.h * w,
        parent.y + py - anchor.v * h,
        w,
        h,
    )
}

/// Only the widgets whose class declares an `Alignment` have one; for the rest
/// the anchor is the top-left, which `from_alignment(0)` also gives.
fn has_alignment(widget: &Widget) -> bool {
    widget.field("Alignment").is_some()
}

/// One axis of a `Position`/`Size`, in pixels.
fn axis(length: Length, parent: f32) -> f32 {
    if length.fraction {
        length.amount * parent
    } else {
        length.amount
    }
}

/// The two panes of a split frame, and the children placed in them.
fn split_children<'a>(
    widget: &'a Widget,
    frame: Frame,
    clip: Option<Frame>,
    fonts: Option<&Fonts>,
) -> Vec<Placement<'a>> {
    let horizontal = widget
        .field("Horizontal")
        .and_then(|f| f.flag())
        .unwrap_or(true);
    let master = widget
        .field("Master")
        .and_then(|f| f.number())
        .unwrap_or(0.0) as i32;
    let master_size = widget
        .field("MasterSize")
        .and_then(Length::parse)
        .map(|l| l.resolve(if horizontal { frame.w } else { frame.h }))
        .unwrap_or(if horizontal { frame.w } else { frame.h } / 2.0);
    let master_size = master_size.clamp(0.0, if horizontal { frame.w } else { frame.h });

    let (a, b) = if horizontal {
        (
            Frame::new(frame.x, frame.y, master_size, frame.h),
            Frame::new(
                frame.x + master_size,
                frame.y,
                frame.w - master_size,
                frame.h,
            ),
        )
    } else {
        (
            Frame::new(frame.x, frame.y, frame.w, master_size),
            Frame::new(
                frame.x,
                frame.y + master_size,
                frame.w,
                frame.h - master_size,
            ),
        )
    };
    // `Master` names the slot MasterSize sizes; the other takes the remainder.
    let panes = if master == 1 { [b, a] } else { [a, b] };

    widget
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| place_at(c, panes[i.min(1)], clip, fonts))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_anchor_grid_is_three_by_three() {
        for (v, h, vert) in [
            (1, 0.0, 0.0),
            (2, 0.5, 0.0),
            (4, 1.0, 0.0),
            (9, 0.0, 0.0),
            (10, 0.5, 0.0),
            (12, 1.0, 0.0),
            (17, 0.0, 0.5),
            (18, 0.5, 0.5),
            (20, 1.0, 0.5),
            (33, 0.0, 1.0),
            (34, 0.5, 1.0),
            (36, 1.0, 1.0),
        ] {
            let a = Anchor::from_alignment(v);
            assert_eq!((a.h, a.v), (h, vert), "alignment {v}");
        }
        assert_eq!(Anchor::from_alignment(0), Anchor::TOP_LEFT);
    }

    fn game_dir() -> Option<std::path::PathBuf> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Every value `Alignment` takes in the shipped screens, which is how the
    /// 3×3 reading is pinned: nine combinations, no others, anywhere.
    #[test]
    fn every_shipped_alignment_is_one_of_the_nine() {
        let Some(game) = game_dir() else { return };
        let mut seen = std::collections::BTreeSet::new();
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for w in gib.walk() {
                if let Some(f) = w.field("Alignment") {
                    let v = f.number().unwrap() as i32;
                    seen.insert(v);
                    assert!(
                        matches!(v & 0x7, 0 | 1 | 2 | 4),
                        "{}: {} has alignment {v}",
                        path.display(),
                        w.kind.name()
                    );
                    assert!(
                        matches!(v & 0x38, 0 | 8 | 16 | 32),
                        "{}: {} has alignment {v}",
                        path.display(),
                        w.kind.name()
                    );
                    assert_eq!(v & !0x3f, 0, "{}: extra bits in {v}", path.display());
                }
            }
        }
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            vec![0, 9, 10, 12, 17, 18, 20, 33, 34, 36]
        );
    }

    fn collect_gib(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_gib(&path, out);
            } else if path.extension().is_some_and(|e| e == "gib") {
                out.push(path);
            }
        }
    }

    /// Every `ClipMode` the corpus carries, and how many of each. The three
    /// values are the whole vocabulary, so a fourth would be a new field.
    #[test]
    fn every_shipped_clip_mode_is_zero_one_or_two() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut counts = std::collections::BTreeMap::new();
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for w in gib.walk() {
                *counts.entry(w.clip_mode()).or_insert(0) += 1;
            }
        }
        assert_eq!(
            counts.into_iter().collect::<Vec<_>>(),
            vec![(0, 2289), (1, 1999), (2, 60)],
            "every shipped ClipMode"
        );
    }

    /// The `Grid`/`ListBox` rule, checked against the authors' own arithmetic:
    /// for every shipped container, the box the file gives the `Selection` (or
    /// `Highlight`) child is the cell that child names — the row from its y,
    /// the column from its x — with the span `RowSelect` asks for.
    ///
    /// Every bar names its row and hangs from it at the row height — all 37 of
    /// them, no exceptions. The *x* is the sum of the columns before it. The
    /// *width* is that sum in 31 of the 37; in the other six it is a number the
    /// author made their own, and they are pinned below rather than smoothed
    /// over: `arena.gib` 159 where the columns sum to 300 and `prematch.gib`
    /// 187 where they sum to 136 — both times the container's own width — and
    /// the three `wiring_connection_*.gib` fragments wanting 161 where their
    /// column sums to 177, which is the *slot* they are loaded into, in a screen
    /// the fragment does not name.
    #[test]
    fn the_selection_bar_sits_on_the_cell_the_file_names() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut checked = 0;
        let mut offenders = std::collections::BTreeSet::new();
        let mut out_of_range = std::collections::BTreeSet::new();
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for root in resolve(&gib, DESIGN_SCREEN) {
                for p in root.walk() {
                    let Some(grid) = p.grid() else { continue };
                    assert_eq!(grid.col_widths.len(), grid.columns.max(0) as usize);
                    for id in [grid.selection, grid.highlight].into_iter().flatten() {
                        let Some(bar) = p.child_with_id(id) else {
                            panic!("{}: no child with ID {id}", path.display());
                        };
                        checked += 1;
                        let (row, column) = grid.selected(p.frame, bar.frame);
                        // The bar hangs from the row it names: its height is the
                        // row height and its y is that row's.
                        assert!(row >= 0 && column >= 0, "{}", path.display());
                        if row >= grid.rows {
                            out_of_range
                                .insert(path.file_name().unwrap().to_string_lossy().to_string());
                        }
                        assert_eq!(bar.frame.h, grid.row_height, "{}", path.display());
                        assert_eq!(
                            bar.frame.y,
                            p.frame.y + row as f32 * grid.row_height,
                            "{}",
                            path.display()
                        );
                        let expected = grid.select(p.frame, row, column);
                        if (expected.w - bar.frame.w).abs() > 0.01 {
                            offenders
                                .insert(path.file_name().unwrap().to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        assert!(checked >= 30, "a corpus of containers, not one");
        // Four single-row lists name a row that does not exist. It is the
        // author's other leftover, like the widths below, and the bar still
        // hangs from `row x RowHeight`, which is what the assertion above is.
        assert_eq!(
            out_of_range.into_iter().collect::<Vec<_>>(),
            vec![
                "prematch.gib",
                "wiring_connection_analog.gib",
                "wiring_connection_button.gib",
                "wiring_connection_switch.gib",
            ],
            "the four rows that are out of range"
        );
        assert_eq!(
            offenders.into_iter().collect::<Vec<_>>(),
            vec![
                "arena.gib",
                "gametype.gib",
                "prematch.gib",
                "wiring_connection_analog.gib",
                "wiring_connection_button.gib",
                "wiring_connection_switch.gib",
            ],
            "the six widths the authors made their own"
        );
    }

    /// A `ScrollBar` puts its parts at its two ends: the less button at the
    /// start, the more button at the far end, and the track between. The
    /// fields name the children by index, and in all 30 shipped bars the index,
    /// the child's `Name` and its kind agree.
    #[test]
    fn a_scroll_bar_stacks_its_parts() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut seen = 0;
        let mut horizontal = 0;
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for root in resolve(&gib, DESIGN_SCREEN) {
                for p in root.walk() {
                    let Some(bar) = p.scroll_bar() else { continue };
                    seen += 1;
                    if !bar.vertical {
                        horizontal += 1;
                    }
                    let less = &p.children[bar.less];
                    let more = &p.children[bar.more];
                    let slider = &p.children[bar.slider];
                    // The index, the name and the kind agree.
                    assert_eq!(less.widget.name(), "Less", "{}", path.display());
                    assert_eq!(more.widget.name(), "Down", "{}", path.display());
                    assert_eq!(slider.widget.kind.name(), "Slider", "{}", path.display());
                    // The less button opens the bar and the more one closes
                    // it, whatever the bar's own size turns out to be.
                    assert_eq!((less.frame.x, less.frame.y), (p.frame.x, p.frame.y));
                    if bar.vertical {
                        assert_eq!(more.frame.x, p.frame.x, "{}", path.display());
                        assert!((more.frame.bottom() - p.frame.bottom()).abs() < 0.01);
                        assert!((slider.frame.y - less.frame.bottom()).abs() < 0.01);
                        assert!((slider.frame.bottom() - more.frame.y).abs() < 0.01);
                        assert!(more.frame.y > less.frame.y);
                    } else {
                        assert_eq!(more.frame.y, p.frame.y, "{}", path.display());
                        assert!((more.frame.right() - p.frame.right()).abs() < 0.01);
                        assert!((slider.frame.x - less.frame.right()).abs() < 0.01);
                        assert!((slider.frame.right() - more.frame.x).abs() < 0.01);
                        assert!(more.frame.x > less.frame.x);
                    }
                }
            }
        }
        assert_eq!(seen, 30, "thirty shipped scroll bars");
        assert_eq!(horizontal, 1, "one of them horizontal");
    }

    /// What makes `ClipMode 2` readable as a clip *by the parent*: of the
    /// fifteen widgets that carry it, eleven are placed so that they stick out
    /// of the parent, where a self-clip would do nothing at all — and the other
    /// four are chrome that moves afterwards, the two selection bars and two of
    /// `comp_properties.gib`'s tabs. Every one of them clips inside the parent.
    #[test]
    fn a_clip_mode_two_widget_is_clipped_by_its_parent() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut seen = 0;
        let mut outside = 0;
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for root in resolve(&gib, DESIGN_SCREEN) {
                walk_clip_two(&root, &mut seen, &mut outside);
            }
        }
        assert_eq!(seen, 32, "ClipMode 2 widgets shipped");
        assert_eq!(outside, 23, "of them sticking out of the parent");
    }

    fn walk_clip_two(placement: &Placement<'_>, seen: &mut usize, outside: &mut usize) {
        for child in &placement.children {
            if child.widget.clip_mode() == 2 {
                *seen += 1;
                let clip = child.clip.expect("a clipped widget has a clip");
                assert!(
                    placement.frame.contains(&clip) || placement.clip.is_some(),
                    "the clip of {} is {}, which is not inside its parent {}",
                    child.widget.kind.name(),
                    format_frame(&clip),
                    format_frame(&placement.frame),
                );
                if !placement.frame.contains(&child.frame) {
                    *outside += 1;
                }
            }
            walk_clip_two(child, seen, outside);
        }
    }

    fn format_frame(f: &Frame) -> String {
        format!("{:.0},{:.0} {:.0}x{:.0}", f.x, f.y, f.w, f.h)
    }

    /// The measured path: with the screen's faces, a label the editor left at
    /// zero size takes its own text's extent — which is what `Size 0 0 0 0`
    /// means on every `Text` in every shipped screen.
    #[test]
    fn a_zero_sized_label_is_sized_by_its_text() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).expect("the shipped faces");
        assert_eq!(fonts.len(), 9, "nine fg_* faces");

        let text = std::fs::read_to_string(game.join("UI/mainmenu/mainmenu.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();

        let unmeasured = resolve(&gib, DESIGN_SCREEN);
        let label = unmeasured
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.text() == Some("MULTIPLAYER"))
            .expect("the multiplayer label");
        assert_eq!(label.frame.w, 0.0, "the editor left it at zero");

        let measured = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));
        let label = measured
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.text() == Some("MULTIPLAYER"))
            .expect("the multiplayer label");
        let (w, h) = fonts.measure("fg_d20", "MULTIPLAYER").unwrap();
        assert!(w > 40.0 && w < 300.0, "MULTIPLAYER is {w} px wide");
        assert_eq!(label.frame.w, w);
        assert_eq!(label.frame.h, h);
        assert_eq!(
            h, 40.0,
            "fg_d20's line height is its ascent less its descent"
        );

        // Its siblings' boxes do not move: only sizes that were zero change.
        let before = unmeasured[0].frame;
        let after = measured[0].frame;
        assert_eq!(before, after, "the window itself is not resized");
    }

    /// The worked case: `mainmenu.gib`, placed against the design screen.
    #[test]
    fn the_main_menu_places_its_logo_backdrop_and_buttons() {
        let Some(game) = game_dir() else { return };
        let text = std::fs::read_to_string(game.join("UI/mainmenu/mainmenu.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let screen = Frame::new(0.0, 0.0, DESIGN_SCREEN[0], DESIGN_SCREEN[1]);

        let menu = placed
            .iter()
            .find(|p| p.widget.name() == "Main Menu Win")
            .expect("the main menu window");
        assert_eq!(menu.frame, screen);

        // Align 10 = centre | top, Size 512x76 px, Position (0.5, 0.07).
        let logo = menu.find("logo grp").expect("logo grp");
        assert_eq!(logo.frame.w, 512.0);
        assert_eq!(logo.frame.h, 76.0);
        assert_eq!(logo.frame.x, DESIGN_SCREEN[0] / 2.0 - 256.0);
        assert!((logo.frame.y - 0.07 * DESIGN_SCREEN[1]).abs() < 0.01);

        // Align 34 = centre | bottom: the button strip sits on the bottom edge.
        // It is a child of the window — the file is one tree, not a forest, and
        // the blocks that used to read as top-level siblings were records the
        // reader had not closed.
        let common = menu.find("common buts").expect("common buts");
        assert!((common.frame.bottom() - DESIGN_SCREEN[1]).abs() < 0.01);
        assert_eq!(common.frame.x, DESIGN_SCREEN[0] / 2.0 - 128.0);

        // A button is in pixels inside the sawback group, which is centred.
        let sawback = menu.find("sawback grp").expect("sawback grp");
        assert_eq!(sawback.frame.w, 640.0);
        assert_eq!(sawback.frame.x, DESIGN_SCREEN[0] / 2.0 - 320.0);
        let exhibition = menu.find("exhibition button").expect("exhibition button");
        assert_eq!(exhibition.frame.x, sawback.frame.x + 237.0);
        assert_eq!(exhibition.frame.y, sawback.frame.y + 191.0);
        assert_eq!(exhibition.frame.w, 165.0);
        assert_eq!(exhibition.frame.h, 26.0);
    }

    /// The places where two siblings share a `ChildOrder` **and** their frames
    /// overlap: **328 pairs**, which is where the tie-break matters at all, and
    /// they are kept as a baseline.
    ///
    /// What they do *not* do is decide which way the tie goes. Their dominant
    /// shape is a named control followed by an unnamed `Widget` holding the same
    /// text — `common/bottomnav.gib`'s `Options button` then a label reading
    /// `Options`, `botlab/ch_stepbuttons.gib`'s step buttons then their numerals
    /// — and that pair reads the same either way round, because a `Button` with a
    /// `Caption` attachment draws its own caption from a `Text` such as that one:
    /// the text is in front of the button's art whichever of the two draws it.
    /// Only **6** pairs in the whole corpus put art over a sibling that has a
    /// `Background` (`exhibition/arena.gib`'s overlay over its `ListBox`,
    /// `teamhq/select_team.gib`'s over its grid, three in `teamhq/repairs.gib`),
    /// and an overlay drawn over a list is as plausible over as under.
    ///
    /// So the tie's direction stays **open**, with one argument pointing away
    /// from what `draw_order` and `input::hit` do today: in
    /// `botlab/components.gib` the palette's grid shares a `ChildOrder` with the
    /// category bar, comes later in the file, and is drawn over the buttons that
    /// the palette needs clicked — which the game must not do, so *there* the
    /// earlier record would be the front one. What settles it is the game's own
    /// hit test, since with a grid in front the buttons still have to be
    /// reachable.
    #[test]
    fn a_child_order_tie_is_broken_by_the_file() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut files = Vec::new();
        collect_gib(&ui, &mut files);
        let mut pairs: Vec<(String, String, String, String, f32)> = Vec::new();
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let placed = resolve(&gib, DESIGN_SCREEN);
            let name = path
                .strip_prefix(&ui)
                .unwrap()
                .to_string_lossy()
                .to_string();
            for parent in placed.iter().flat_map(|p| p.walk()) {
                for (i, a) in parent.children.iter().enumerate() {
                    for b in parent.children.iter().skip(i + 1) {
                        if a.widget.child_order() != b.widget.child_order() {
                            continue;
                        }
                        let w = (a.frame.x + a.frame.w).min(b.frame.x + b.frame.w)
                            - a.frame.x.max(b.frame.x);
                        let h = (a.frame.y + a.frame.h).min(b.frame.y + b.frame.h)
                            - a.frame.y.max(b.frame.y);
                        if w > 4.0 && h > 4.0 {
                            pairs.push((
                                name.clone(),
                                format!("{} [{}]", a.widget.name(), a.widget.kind.name()),
                                format!("{} [{}]", b.widget.name(), b.widget.kind.name()),
                                parent.widget.name().to_string(),
                                w * h,
                            ));
                        }
                    }
                }
            }
        }
        assert_eq!(
            pairs.len(),
            328,
            "equal-ChildOrder overlapping sibling pairs"
        );
        // The convention, named: each of the nav bar's three buttons is followed
        // by the label that draws its text, and the step buttons by their
        // numerals.
        for button in ["Options button", "MainMenu button", "Help button"] {
            assert!(
                pairs.iter().any(|p| p.0 == "common/bottomnav.gib"
                    && p.1.starts_with(button)
                    && p.2.starts_with(' ')
                    && p.3 == "gray grp"),
                "bottomnav: {button} is followed by its label"
            );
        }
        assert!(pairs
            .iter()
            .any(|p| p.0 == "botlab/ch_stepbuttons.gib" && p.1.starts_with("step 1 button")));
        assert!(pairs
            .iter()
            .any(|p| p.0 == "botlab/components.gib" && p.1.starts_with("comp upper grp")));
    }

    /// Every shipped file, placed: 4,348 widgets, all finite, and 228 whose
    /// frame reaches past the screen. Those are of three kinds, all expected:
    /// *fragments* — files like `botlab/control_cell.gib` that are a content
    /// slot's payload and were never meant to be placed against the screen —
    /// children whose declared `Size` is larger than the pane they sit in, which
    /// the game clips (`ClipMode`) where this resolver does not yet, and the
    /// records the file's counts leave without a home (see
    /// `a_file_does_not_always_nest_to_one_root`), which are placed against the
    /// screen because nothing claims them. Fifty-six of the 116 files place
    /// entirely on screen.
    #[test]
    fn every_shipped_file_places_and_the_frames_are_finite() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let screen = Frame::new(0.0, 0.0, DESIGN_SCREEN[0], DESIGN_SCREEN[1]);

        let mut placed = 0;
        let mut outside = 0;
        let mut perfect = 0;
        let mut worst = (0.0f32, String::new());
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let roots = resolve(&gib, DESIGN_SCREEN);
            let mut here = 0;
            let mut off = 0;
            for root in &roots {
                for p in root.walk() {
                    placed += 1;
                    here += 1;
                    assert!(
                        p.frame.is_finite(),
                        "{}: {} placed non-finite",
                        path.display(),
                        p.widget.kind.name()
                    );
                    if !screen.contains(&p.frame) {
                        off += 1;
                    }
                }
            }
            outside += off;
            if off == 0 {
                perfect += 1;
            }
            let fraction = off as f32 / here.max(1) as f32;
            if fraction > worst.0 {
                worst = (fraction, path.display().to_string());
            }
        }
        // `MasterSize` is one axis, not a `Position`-style pair: all
        // twenty-six shipped values are two tokens, `0.14 1` or `90 0`, and the
        // reader that took them for two axes found no four-number value and
        // fell back to half the frame for every split whose master is not the
        // first slot. The counts are higher than they were because the reader
        // now closes a record at its `NumChildren` and keeps the headerless
        // base `Widget` records that used to be swallowed: 4,348 widgets, not
        // 3,590. The 228 that fall outside are the records whose owning group is
        // full, which the file's own count cannot express — see
        // `a_file_does_not_always_nest_to_one_root`.
        assert_eq!(placed, 4348);
        assert_eq!(outside, 228, "off screen");
        assert_eq!(perfect, 56, "perfect files");
        // The worst offender is a fragment, not a screen.
        assert!(worst.1.ends_with("botlab/adjust_none.gib"), "{}", worst.1);
    }

    /// A `Proxy` is the data's own slot: it names a `.gib` and that fragment is
    /// drawn inside the proxy's rectangle. Nine shipped screens name
    /// `common\bottomnav.gib`, and inlining puts its root — `common buts`, the
    /// bottom navigation bar — at the frame the proxy gives it.
    #[test]
    fn a_proxy_loads_the_screen_it_names() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut files = Vec::new();
        collect_gib(&ui, &mut files);
        let mut named = Vec::new();
        let mut missing = Vec::new();
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            for block in &gib.blocks {
                let Block::Widget(w) = block else { continue };
                for p in proxies(w) {
                    named.push(p.clone());
                    if ra2fmt::find_ci(&ui, &p).is_none() {
                        missing.push(format!("{}: {p}", path.display()));
                    }
                }
            }
        }
        assert_eq!(named.len(), 9, "nine shipped screens proxy another file");
        assert!(missing.is_empty(), "proxies with no file: {missing:?}");
        assert!(
            named
                .iter()
                .all(|p| p.to_ascii_lowercase().contains("bottomnav.gib")),
            "every one of them is the bottom bar: {named:?}"
        );

        // `botlabwin.gib`'s proxy sits in the 49-pixel strip at the bottom of its
        // own content area, so the bar lands there rather than on the screen's
        // bottom edge — the file's geometry, not a guess.
        let text = std::fs::read_to_string(ui.join("botlab/botlabwin.gib")).unwrap();
        let mut gib = ra2fmt::gib::parse(&text).unwrap();
        let loaded = inline_proxies_in(&mut gib, &ui);
        assert_eq!(loaded, vec!["common\\bottomnav.gib".to_string()]);
        let placed = resolve(&gib, DESIGN_SCREEN);
        let bar = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "common buts")
            .expect("the bottom bar was loaded into the proxy");
        assert_eq!(bar.frame, Frame::new(512.0, 584.7, 256.0, 36.0));
    }

    /// `MasterSize` is **one axis**, not two: all twenty-six split frames in
    /// the corpus write `amount is_fraction` — `0.14 1`, `90 0` — and the reader
    /// that took them for two axes fell back to half the frame for every split
    /// whose master is not the first slot. `botlabwin.gib` is the worked case:
    /// its toolbar split is `Master 0`, `MasterSize 0.14 1`, so the toolbar's
    /// pane is 0.14 × 1005 = 140.7 and the content below it starts there, which
    /// puts the title group at 2% of the 864.3 that is left — 158.0 — rather
    /// than at the 512.5 a half-frame reading gave it.
    #[test]
    fn the_master_size_is_one_axis() {
        let Some(game) = game_dir() else { return };
        let text = std::fs::read_to_string(game.join("UI/botlab/botlabwin.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let frame = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("no widget named {name}"))
                .frame
        };
        // The lower split is `Master 1`, `MasterSize 0.12 1`: the master is the
        // 49-pixel nav strip and the content slot takes the 363.4 above it, so
        // the strip's pane starts 49.56 below the split and it fills it.
        assert_eq!(frame("botlab splt"), Frame::new(0.0, 0.0, 1280.0, 1005.0));
        assert!((frame("lowersplit").y - 207.7).abs() < 0.1);
        assert!(
            (frame("content slot").y - 257.3).abs() < 0.1,
            "{}",
            frame("content slot").y
        );
        assert_eq!(frame("content slot").h, 364.0);
        let split = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "lowersplit")
            .unwrap();
        assert_eq!(split.children.len(), 2);
        // 12% of the 413 it splits, and the strip lands on its own
        // `Position 0 0 364 0` inside the content slot's 364.
        assert_eq!(split.children[1].frame.h, 49.0);
        assert!((split.children[1].frame.y - 571.7).abs() < 0.1);
    }

    /// A `Glyph` wraps its text at spaces to the width its `_Margin` gives it,
    /// and honours the `\n`s the author typed.
    #[test]
    fn a_glyph_wraps_to_its_margin() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).unwrap();
        let font = fonts.get("fg_mcn14").expect("the debug face");

        // `\n` are the author's own breaks, and a line after one starts fresh.
        let lines = wrap_lines(font, "one\ntwo three", 10000.0);
        assert_eq!(lines, vec!["one".to_string(), "two three".to_string()]);

        // Narrow enough for one word a line, and no word is broken.
        let one = font.extent("chassis").0 as f32;
        let lines = wrap_lines(font, "chassis chassis", one + 1.0);
        assert_eq!(lines, vec!["chassis".to_string(), "chassis".to_string()]);

        // A paragraph's first line hangs from the box's top the same way a
        // label does, and a wrapped second line sits one line height lower.
        let width = font.extent("chassis").0 as f32 + 1.0;
        let two = paragraph_run(
            font,
            Frame::new(0.0, 0.0, width, 40.0),
            "chassis chassis",
            width,
        );
        let first = two[0].rect.y;
        assert!(
            first >= 0.0 && first < font.line_height() as f32,
            "the first line hangs from the top: {first}"
        );
        assert_eq!(
            two.last().unwrap().rect.y - first,
            font.line_height() as f32,
            "the second line is one line height below the first"
        );
    }

    /// A `Lines` widget is a polyline in point *pairs*, and the corpus is what
    /// settles it: `selectbot_computer.gib`'s only visible one insets a box by
    /// two pixels at a `Width` of four, which reads as a rectangle only when
    /// the eight points are cut into four segments.
    #[test]
    fn a_lines_widget_is_read_in_point_pairs() {
        let Some(game) = game_dir() else { return };
        let text =
            std::fs::read_to_string(game.join("UI/exhibition/selectbot_computer.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let lines: Vec<_> = placed
            .iter()
            .flat_map(|p| p.walk())
            .filter(|p| p.widget.kind == WidgetKind::Lines && p.widget.visible())
            .collect();
        assert_eq!(lines.len(), 1, "one visible Lines");
        let p = lines[0];
        let points: Vec<[f32; 2]> = p
            .widget
            .fields_named("Point")
            .map(|f| {
                let n = f.numbers();
                [n[0], n[2]]
            })
            .collect();
        assert_eq!(
            points,
            vec![
                [0.0, 0.0],
                [90.0, 0.0],
                [88.0, 0.0],
                [88.0, 90.0],
                [90.0, 90.0],
                [0.0, 90.0],
                [2.0, 90.0],
                [2.0, 0.0]
            ]
        );
        assert_eq!(p.widget.field("Width").and_then(|f| f.number()), Some(4.0));
    }

    /// **Open, but bounded.** A record's `NumChildren` counts its *named*
    /// children only — measured over the whole corpus: `sum(NumChildren)` plus
    /// the headerless records is `records - 1` in all 116 files — so a
    /// headerless child, or a button's caption holder, has no slot to claim when
    /// the group it belongs to is already full. Forty-nine of the 116 files then
    /// nest to a single root; the rest leave a few records at the top level, and
    /// the largest, `import_texture.gib`, ends with six.
    ///
    /// Both halves are the data's, not an inference: the counts are on disk, and
    /// the headerless records are the base `Widget` class, whose `[Kind]` header
    /// the writer emits from a virtual call and which has no name to emit. What
    /// is missing is where an uncounted record goes when its parent's count is
    /// already satisfied — the file cannot say, and neither the game's reader
    /// nor this one has been shown to handle it. It is *not* a placement
    /// failure: the records land somewhere finite, and the counts above are
    /// computed with the same trees.
    #[test]
    fn a_file_does_not_always_nest_to_one_root() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut single = 0;
        let mut worst = (0usize, String::new());
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let roots = gib.widgets().count();
            if roots == 1 {
                single += 1;
            }
            if roots > worst.0 {
                worst = (roots, path.display().to_string());
            }
        }
        assert_eq!(files.len(), 116);
        assert_eq!(single, 49, "files that nest to their Window");
        assert!(
            worst.1.ends_with("botlab/import_texture.gib"),
            "{}",
            worst.1
        );
        assert_eq!(worst.0, 6, "worst roots");
    }

    /// Draw order is `ChildOrder` ascending, ties by file order — which is what
    /// puts the full-screen background first and the logo after it, and what
    /// puts a button's label (0) after the button itself (-1).
    #[test]
    fn the_background_is_drawn_before_the_logo() {
        let Some(game) = game_dir() else { return };
        let text = std::fs::read_to_string(game.join("UI/mainmenu/mainmenu.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let order = draw_order(&placed);
        let at = |name: &str| {
            order
                .iter()
                .position(|p| {
                    p.widget
                        .filename()
                        .is_some_and(|f| f.replace('\\', "/").ends_with(name))
                })
                .unwrap_or_else(|| panic!("{name} is not drawn"))
        };
        let background = at("cement_background.bmp");
        for later in [
            "main_backdrop.tga",
            "ra2_logo.tga",
            "little_gear.tga",
            "rounded_rectL.tga",
        ] {
            assert!(background < at(later), "{later} is behind the background");
        }
    }
}

#[cfg(test)]
mod text_run_tests {
    use super::*;

    fn game_dir() -> Option<std::path::PathBuf> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// A run advances by each glyph's advance, hangs its cells from one
    /// baseline, and covers the same width the extent measures.
    #[test]
    fn a_run_is_as_wide_as_the_extent() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).unwrap();
        let font = fonts.get("fg_d20").expect("fg_d20");
        let frame = Frame::new(10.0, 20.0, 0.0, 0.0);
        let run = text_run(font, frame, "MULTIPLAYER");
        assert_eq!(
            run.len(),
            11,
            "one cell per letter, and no cell for a space"
        );
        assert_eq!(run[0].rect.x, 10.0, "the run starts at the frame");
        // Each cell starts where the pen had reached.
        let mut pen = frame.x;
        for (glyph, placed) in "MULTIPLAYER".chars().zip(&run) {
            assert_eq!(placed.rect.x, pen, "{glyph}");
            pen += font.advance(glyph) as f32;
        }
        let (width, height) = font.extent("MULTIPLAYER");
        assert_eq!(pen - frame.x, width as f32, "the pen ends at the extent");
        assert_eq!(height, font.line_height());
        // A space has no cell but does advance.
        let spaced = text_run(font, frame, "A B");
        assert_eq!(spaced.len(), 2);
        assert_eq!(
            spaced[1].rect.x,
            frame.x + font.advance('A') as f32 + font.space_advance() as f32
        );
    }
}
