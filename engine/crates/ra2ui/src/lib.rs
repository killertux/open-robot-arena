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
//! ## `Alignment` is a 3×3 anchor (except on a `TextBox`)
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
//! A `TextBox` is the exception: its `Alignment` is the text's alignment inside
//! the box, so the box itself is placed at its own `Position` (top-left). The
//! data settles it — `matchresults.gib`'s title is `Position 0 0`,
//! `Size 1 1 28 0`, `Alignment 18`, and reading the field as a box anchor put
//! the title half a box above its panel and clipped it away. The `Text` child of
//! a `TextBox`/`Group` is placed by its **baseline**: `statusmeters.gib`'s name
//! is a 12-pixel box whose `Text` is at y 8, and a 12-pixel face's ink only lands
//! inside that box when 8 is the baseline.
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

use ra2fmt::gib::{Attachment, AttachmentKind, Block, Gib, Length, Widget, WidgetKind};

pub mod arenas;
pub mod cells;
pub mod components;
pub mod events;
pub mod hq;
pub mod input;
pub mod preview;
pub mod project;
pub mod repairs;
pub mod results;
pub mod standings;
pub mod teams;
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

/// Where a run's pen starts relative to its frame, in pixels. Measured, not
/// derived: in the original's 1280x1024 capture every label — `EXHIBITION`,
/// `TEAM HQ`, `MULTIPLAYER`, `Options`, `Credits`, `Quit` — has its ink exactly
/// one pixel right of and (for the big buttons, whose boxes line up with the
/// capture to the pixel) one pixel below where `pen + bearing_x` and
/// `baseline - bearing_y` put it, while the ink's width is identical. The font
/// object's own extents are `+1` after its build loop (`FUN_005cd560`).
pub const TEXT_ORIGIN: [f32; 2] = [1.0, 1.0];

/// Lay a string out the way the game's own labels are written: the baseline
/// sits one `ascent` below the frame's top, each glyph's cell starts
/// `bearing_x` right of the pen and `bearing_y` above that line, and the pen
/// advances by each glyph's advance. A character with no cell — a space —
/// advances without drawing.
pub fn text_run(font: &ra2fmt::Font, frame: Frame, text: &str) -> Vec<PlacedGlyph> {
    let baseline = frame.y + font.ascent as f32 + TEXT_ORIGIN[1];
    let mut pen = frame.x + TEXT_ORIGIN[0];
    let mut out = Vec::new();
    for c in text.chars() {
        let glyph = font.glyph(c);
        if glyph.has_ink() {
            let (x, y, w, h) = glyph.rect(font.page_width, font.page_height);
            out.push(PlacedGlyph {
                cell: [x, y, w, h],
                rect: Frame::new(
                    pen + glyph.bearing_x as f32,
                    baseline - glyph.bearing_y as f32,
                    w as f32,
                    h as f32,
                ),
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
    // The `.gib` writer stores a line break as the two characters `\n`
    // (`credits.gib`'s paragraphs); the engine reads it as a newline.
    let text = text.replace("\\n", "\n");
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
    // The exe's text layout (FUN_00602210) advances each line by the font's
    // ascent (`FUN_00602bf0` = font+0xab8), not by ascent - descent.
    let pitch = font.ascent as f32;
    let mut out = Vec::new();
    for (i, line) in wrap_lines(font, text, width).into_iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_frame = Frame::new(frame.x, frame.y + i as f32 * pitch, width, line_height);
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
    /// The widgets its attachments carry: a `Background`'s image, a `Caption`'s
    /// text, a `Scroller`'s bar. They are placed against the widget's **parent**
    /// frame, because the attachment's own `Position`/`Size` are measured there
    /// (the decorated widget is a child of the same parent), and they are drawn
    /// around the widget: a background behind it, the rest in front.
    pub attachments: Vec<PlacedAttachment<'a>>,
}

/// One attachment's body, placed around the widget it dresses.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedAttachment<'a> {
    pub attachment: &'a Attachment,
    pub body: Placement<'a>,
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
        frame.contains(&self.frame)
            && self.children.iter().all(|c| c.fits(frame))
            && self.attachments.iter().all(|a| a.body.fits(frame))
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
        for a in p.attachments.iter().rev() {
            self.stack.push(&a.body);
        }
        self.stack.extend(p.children.iter().rev());
        Some(p)
    }
}

/// Whether a class is one a player operates.
///
/// This is the vocabulary a click can *land on*: a `Button`, a `CheckBox` (a tab
/// or an option), a `ListBox`/`Grid` (a list), a `DropDown`, a `ScrollBar`, a
/// `Slider`, a `TextBox`. Everything else — a `Window`, `Group`, `SplitFrame`,
/// `Image`, `Text`, `Glyph`, `Lines`, `Rectangle`, `Proxy`, `Placeholder` — is
/// the screen's chrome.
pub fn is_control(kind: &WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Button
            | WidgetKind::CheckBox
            | WidgetKind::ListBox
            | WidgetKind::Grid
            | WidgetKind::DropDown
            | WidgetKind::ScrollBar
            | WidgetKind::Slider
            | WidgetKind::TextBox
    )
}

/// Whether a negative `ChildOrder` on this class draws it **last**.
///
/// The class is a control the player presses: a `Button`, a `CheckBox` (a tab or
/// an option), a `TextBox`, a `Slider` or a `DropDown`. A `Grid`/`ListBox` is a
/// control too — `input::hit_control` stops on it — but it is a *content area*,
/// and `botlab/components.gib` is the case that pins the difference: its palette
/// grid is -1 and its category bar is -1, the bar is written first, and the
/// screen's own note says the buttons filter the grid, so the bar has to stay in
/// front of the grid. Promoting every negative control put the grid over the
/// buttons.
pub fn draws_last_when_negative(kind: &WidgetKind) -> bool {
    matches!(
        kind,
        WidgetKind::Button
            | WidgetKind::CheckBox
            | WidgetKind::TextBox
            | WidgetKind::Slider
            | WidgetKind::DropDown
    )
}

/// The draw key of a sibling.
///
/// A non-negative `ChildOrder` is itself and draws ascending. **A negative order
/// on a pressed control draws last** ([`draws_last_when_negative`]): 613 shipped
/// children carry a negative order, and 73 of them overlap a non-negative
/// sibling. The pressed controls among those are the ones the screens' own notes
/// require to be operable — the main menu's three buttons under the saw plate,
/// `bottomnav.gib`'s bar buttons under their panel, the credits window's close
/// button, the options' dropdowns and sliders.
///
/// A negative order on chrome still draws first: `select_team.gib`'s full-screen
/// `cement img` is -1 and must stay behind the dialog; the credits backdrop is an
/// `Attachment`, drawn explicitly behind its widget; and the palette's grid keeps
/// the tie with the category bar, where the earlier record is in front.
///
/// So the sign does not mean "front" or "behind" by itself; it means *no order*,
/// and what "no order" resolves to depends on what the record is. The earlier
/// reading — negative is always behind — is what left the main menu without
/// `EXHIBITION`, `TEAM HQ` and `MULTIPLAYER`.
pub fn draw_key(widget: &Widget) -> i32 {
    let order = widget.child_order();
    if order < 0 && draws_last_when_negative(&widget.kind) {
        i32::MAX
    } else {
        order
    }
}

/// The widgets to draw, back to front.
///
/// A parent is drawn before its children and its children by `ChildOrder`
/// **ascending**, with a negative order last ([`draw_key`]), ties broken by file
/// order. The top level is a sibling set and sorts the same way.
///
/// * Inside `mainmenu.gib`'s window the cement background is 1, the saw backdrop
///   2 and the logo group 3, so drawing the file's own order would paint the
///   background over the logo. A button's four state images are 0..3 with only
///   the first visible, which is what puts a label over its art.
/// * The main menu's three buttons are -1 inside `sawback grp` while the plate
///   is 6; drawing them first would hide `EXHIBITION`, `TEAM HQ` and
///   `MULTIPLAYER` behind the plate. Drawn last they land on it, which is what
///   the game shows and what the screens' own notes require (the notes say the
///   buttons link somewhere, so a click has to reach them).
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
        // A hidden widget takes its whole subtree with it. The scene builder
        // checks visibility per widget too, but a hidden group's children are
        // still `Visible true` in the file — `gamewindow.gib`'s countdown text
        // and `result_cell.gib`'s `Spectator` are exactly that — so the walk has
        // to stop here rather than rely on the leaf's own flag.
        if !p.widget.visible() {
            return;
        }
        // A `Background` is the one attachment that draws *behind* what it
        // dresses; every other body (a caption's text, a scroller's bar, a
        // border) draws in front. That is what the class means: the body's frame
        // is the panel under the widget, and the corpus makes the point on
        // `hq_league.gib`, whose grid has a 607×319 light panel behind it and a
        // black text row on top.
        for a in &p.attachments {
            if p.widget.visible() && a.attachment.kind == AttachmentKind::Background {
                visit(&a.body, Some(p), out);
            }
        }
        out.push((p, parent));
        // Higher `ChildOrder` is drawn later, and so is in front. Where two
        // children share one, the **earlier record is in front**: the read is
        // taken backwards and the sort is stable. That is the direction
        // `botlab/components.gib` forces — its palette grid shares the category
        // bar's order and comes later in the file, and the screen's own note
        // (*"grid is filtered based on the category selected in this
        // optiongroup"*) says those buttons are what the player clicks, so they
        // have to be the ones in front of the cells.
        let mut children: Vec<&Placement> = p.children.iter().rev().collect();
        children.sort_by_key(|p| draw_key(p.widget));
        for c in children {
            visit(c, Some(p), out);
        }
        for a in &p.attachments {
            if p.widget.visible() && a.attachment.kind != AttachmentKind::Background {
                visit(&a.body, Some(p), out);
            }
        }
    }
    // The top level is a sibling set too: a file's own records are ordered by
    // `ChildOrder` exactly as a widget's children are.
    let mut top: Vec<&Placement> = roots.iter().rev().collect();
    top.sort_by_key(|p| draw_key(p.widget));
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
    place_at(widget, parent, None, fonts, false)
}

/// Place one widget, carrying down the clip its ancestors impose.
///
/// `baseline` says that this widget is a `Text` whose `_Position.y` is the
/// **glyph baseline**, not the top of its line box. That is the origin the game
/// writes: `statusmeters.gib`'s name is a 12 px box with its `Text` at y 8, and
/// a 12-pixel font's ink only lands inside that box when 8 is the baseline.
/// Direct children of a `TextBox`/`Group` are placed that way; a `Caption`'s
/// body is already centred by [`centre_captions`] and keeps its top-left frame.
fn place_at<'a>(
    widget: &'a Widget,
    parent: Frame,
    inherited: Option<Frame>,
    fonts: Option<&Fonts>,
    baseline: bool,
) -> Placement<'a> {
    place_core(widget, parent, inherited, fonts, baseline, None, None)
}

/// [`place_at`], with the widget's frame optionally forced along a split's axis:
/// a split frame gives each slot's child the slot's extent *along the axis it
/// divides* whatever the child's `Position`/`Size` say there (`exhib_setup.gib`'s
/// `top slot` states 418 px of height and is the whole 87% pane), while the
/// other axis keeps the child's own rules (`content slot`'s `0.9` of the height
/// of a horizontal split).
fn place_core<'a>(
    widget: &'a Widget,
    parent: Frame,
    inherited: Option<Frame>,
    fonts: Option<&Fonts>,
    baseline: bool,
    forced: Option<(Frame, bool)>,
    fill: Option<Frame>,
) -> Placement<'a> {
    let mut frame = sized(widget, parent, widget.size(), fonts, baseline);
    if let Some(owner) = fill {
        frame = owner;
    }
    // A list a `Scroller` wraps fills the scroller's own rectangle: the list's
    // `Position`/`Size` are the editor's leftovers (`arena.gib`'s listbox says
    // 159 x 175 px at (1, 1); the scroller body says 0.89 x 0.585 at (0.1, 0.08)
    // and the capture's black list is 258 x 282 there).
    if matches!(widget.kind, WidgetKind::ListBox) {
        if let Some(body) = widget
            .attachments
            .iter()
            .find(|a| a.kind == AttachmentKind::Scroller)
            .and_then(|a| a.body.as_deref())
        {
            frame = sized(body, parent, body.size(), fonts, false);
        }
    }
    if let Some((pane, horizontal)) = forced {
        if horizontal {
            frame.x = pane.x;
            frame.w = pane.w;
        } else {
            frame.y = pane.y;
            frame.h = pane.h;
        }
    }
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
            // Explicit text inside a box or a group is positioned by its
            // baseline, and so is a caption's: a button's `Caption` body is a
            // bare `Widget` the size of the button, and its `Text` is authored
            // at the pen's x and the baseline's y inside it (`Options` at
            // `14 0 13 0` in a 17-pixel button; measured against the original).
            let baseline_text = matches!(
                widget.kind,
                WidgetKind::TextBox | WidgetKind::Group | WidgetKind::Widget
            );
            let placed: Vec<Placement> = widget
                .children
                .iter()
                .map(|c| place_at(c, frame, clip, fonts, baseline_text))
                .collect();
            placed
        }
    };
    // An attachment's own record is measured against the widget's *parent*, not
    // against the widget: a `Background` may be larger than what it dresses (the
    // 607×319 panel behind `hq_league.gib`'s 591-pixel grid) and a `Caption`
    // hangs off the same button. It inherits the parent's clip, then applies its
    // own `ClipMode`.
    // A widget that clips to itself clips its attachments too: botlab's
    // `3d grp` has a stray 296-px dark Background at (12, 24), which the capture
    // never shows outside the 3D view's square.
    let attachment_clip = if widget.kind == WidgetKind::Group
        && ClipMode::from_value(widget.clip_mode()) == ClipMode::SelfFrame
        && widget
            .field("AspectSetting")
            .is_some_and(|f| f.text().trim() != "0")
    {
        clip
    } else {
        inherited
    };
    let attachments: Vec<PlacedAttachment> = widget
        .attachments
        .iter()
        .filter_map(|a| {
            a.body.as_deref().map(|body| PlacedAttachment {
                attachment: a,
                body: place_core(
                    body,
                    parent,
                    attachment_clip,
                    fonts,
                    false,
                    None,
                    // A `Background` dresses its owner: the body's own pixel
                    // size is the editor's canvas, the game stretches it over
                    // the widget (the capture's HQ panels are tinted edge to
                    // edge).
                    (a.kind == AttachmentKind::Background).then_some(frame),
                ),
            })
        })
        .collect();
    // A `Glyph` dressed by a `DisplayBox` is the box's paragraph: the widget
    // itself is `Position 0 0 0 0` (it hangs from its parent's corner) and the
    // box it fills is the attachment's body. `credits.gib`'s two name lists are
    // both at the window's origin until each moves to its own box.
    if widget.kind == WidgetKind::Glyph {
        if let Some(boxed) = attachments
            .iter()
            .find(|a| a.attachment.kind == AttachmentKind::DisplayBox)
        {
            frame = boxed.body.frame;
        }
    }
    Placement {
        widget,
        frame,
        clip,
        children,
        attachments,
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
    for a in placement.attachments.iter_mut() {
        shift(&mut a.body, dx, dy);
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
    // An attachment body hangs off the same parent the widget does, so it is
    // re-clipped against that parent and the same inherited clip.
    for a in placement.attachments.iter_mut() {
        reclip(&mut a.body, parent, inherited);
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
        .map(|c| place_at(c, frame, clip, fonts, false))
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
    baseline: bool,
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
    // `AspectSetting`/`AspectRatio` on a group (width / height): 1 derives the
    // height from the width (botlab's `3d grp`: 0.92 of the left pane wide, as
    // tall as wide), 2 derives the width from the height (the inventory's
    // `bot img group`: 455 px asked, a square of the cell's height drawn).
    let (w, h) = aspect(widget, w, h);
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
    // A `Text` whose `_Position.y` is a baseline shifts its frame up by the
    // face's ascent, so `text_run`'s `frame.y + ascent` lands on that baseline.
    let ascent = if baseline {
        fonts
            .and_then(|fonts| widget.font().and_then(|name| fonts.get(name)))
            .map(|font| font.ascent as f32)
            .unwrap_or(0.0)
    } else {
        0.0
    };
    Frame::new(
        parent.x + px - anchor.h * w,
        parent.y + py - anchor.v * h - ascent,
        w,
        h,
    )
}

fn aspect(widget: &Widget, w: f32, h: f32) -> (f32, f32) {
    if widget.kind != WidgetKind::Group {
        return (w, h);
    }
    let number =
        |name: &str| input::field_of(widget, name).and_then(|v| v.trim().parse::<f32>().ok());
    let (Some(setting), Some(ratio)) = (number("AspectSetting"), number("AspectRatio")) else {
        return (w, h);
    };
    if ratio <= 0.0 {
        return (w, h);
    }
    match setting as i32 {
        1 => (w, w * ratio),
        2 => (h * ratio, h),
        _ => (w, h),
    }
}

/// Whether a widget's `Alignment` anchors its **box**.
///
/// `Group` and `Window` do. A `TextBox` does not: its `Alignment` is the text
/// alignment inside the box, and anchoring the box by it moves the record half
/// its own width/height out of its parent. The shipped records settle it —
/// `common/matchresults.gib`'s title is `Position 0 0`, `Size 1 1 28 0`,
/// `Alignment 18` and its `Text` child is authored at `_Position 186 19`; read
/// as an anchor the box starts at y = −14 and the title is clipped away, read as
/// a top-left box at 0,0 the child lands where the designer put it.
/// `statusmeters.gib`'s `points` is the same: `Position 46 51`, `Alignment 17`,
/// and the number under `POINTS:` only when the position is the box's own.
fn has_alignment(widget: &Widget) -> bool {
    widget.field("Alignment").is_some() && widget.kind != WidgetKind::TextBox
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

    // `Master` names the slot MasterSize sizes. Slot 0 is always the first
    // (top/left) pane and slot 1 the second, so a master of 1 is the pane at
    // the far end (`exhib_setup.gib`'s 13% bottom bar) and slot 0 takes the rest.
    let total = if horizontal { frame.w } else { frame.h };
    let first = if master == 1 {
        total - master_size
    } else {
        master_size
    };
    let (a, b) = if horizontal {
        (
            Frame::new(frame.x, frame.y, first, frame.h),
            Frame::new(frame.x + first, frame.y, total - first, frame.h),
        )
    } else {
        (
            Frame::new(frame.x, frame.y, frame.w, first),
            Frame::new(frame.x, frame.y + first, frame.w, total - first),
        )
    };
    let panes = [a, b];

    widget
        .children
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let pane = panes[i.min(1)];
            place_core(c, pane, clip, fonts, false, Some((pane, horizontal)), None)
        })
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

    /// What makes `ClipMode 2` readable as a clip *by the parent*: of the sixty
    /// widgets that carry it, forty-seven are placed so that they stick out of
    /// the parent, where a self-clip would do nothing at all — and the rest are
    /// chrome that moves afterwards, the selection bars and two of
    /// `comp_properties.gib`'s tabs. Every one of them clips inside the parent.
    /// (The count was 32 before the attachment bodies were read: 28 of the 60
    /// are bodies, which used to be placed as siblings of the wrong parent.)
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
                walk_clip_two(
                    &root,
                    Frame::new(0.0, 0.0, DESIGN_SCREEN[0], DESIGN_SCREEN[1]),
                    &mut seen,
                    &mut outside,
                );
            }
        }
        assert_eq!(seen, 60, "ClipMode 2 widgets shipped");
        assert_eq!(outside, 46, "of them sticking out of the parent");
    }

    fn walk_clip_two(
        placement: &Placement<'_>,
        parent: Frame,
        seen: &mut usize,
        outside: &mut usize,
    ) {
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
                if !parent.contains(&child.frame) {
                    *outside += 1;
                }
            }
            walk_clip_two(child, placement.frame, seen, outside);
        }
        // An attachment body is measured against the decorated widget's parent,
        // which is the `parent` this call was given — not against the widget it
        // dresses.
        for a in &placement.attachments {
            let body = &a.body;
            if body.widget.clip_mode() == 2 {
                *seen += 1;
                let clip = body.clip.expect("a clipped widget has a clip");
                assert!(
                    parent.contains(&clip) || placement.clip.is_some(),
                    "the clip of an attached {} is {}, which is not inside its parent {}",
                    body.widget.kind.name(),
                    format_frame(&clip),
                    format_frame(&parent),
                );
                if !parent.contains(&body.frame) {
                    *outside += 1;
                }
            }
            walk_clip_two(body, parent, seen, outside);
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

    /// **The help screen's first body reads from its page runs.** The line is
    /// `fg_mcn14`'s long `At the top of your screen...`; the first glyph must be
    /// the page's `A` (not `B`, which is what the order-only mapping gave), the
    /// run's width is the sum of the face's advances, and every sampled cell has
    /// ink. This is the face the one-glyph shift was reported on.
    #[test]
    fn the_help_line_reads_from_its_page_runs() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).expect("the shipped faces");
        let text = std::fs::read_to_string(game.join("UI/help/help_ws_overview.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));
        let body = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| {
                p.widget
                    .text()
                    .is_some_and(|t| t.starts_with("At the top of your screen"))
            })
            .expect("the help body");
        let label = body.widget.text().unwrap();
        let font = fonts.get("fg_mcn14").expect("fg_mcn14");
        let run = text_run(font, body.frame, label);
        assert_eq!(
            run.len(),
            label.chars().filter(|c| *c != ' ').count(),
            "one glyph per non-space character"
        );
        let (ax, ay, aw, ah) = font.glyph('A').rect(font.page_width, font.page_height);
        assert_eq!(run[0].cell, [ax, ay, aw, ah], "the first glyph is A");
        let (tx, ty, tw, th) = font.glyph('t').rect(font.page_width, font.page_height);
        assert_eq!(run[1].cell, [tx, ty, tw, th], "the second glyph is t");

        let sum: i32 = label.chars().map(|c| font.advance(c)).sum();
        assert_eq!(body.frame.w, sum as f32, "the run's width");
        for glyph in &run {
            assert!(glyph.cell[2] > 0 && glyph.cell[3] > 0, "a cell");
            let ink = (glyph.cell[1]..glyph.cell[1] + glyph.cell[3])
                .flat_map(|y| {
                    (glyph.cell[0]..glyph.cell[0] + glyph.cell[2]).map(move |x| font.coverage(x, y))
                })
                .filter(|&v| v > 40)
                .count();
            assert!(ink > 0, "a cell with no ink: {glyph:?}");
        }
    }

    /// **The pangram and the alphabet read in every face.** For each of the nine
    /// shipped faces, the `The quick brown fox...` run has one glyph per
    /// non-space character, its width is the sum of the advances, every cell has
    /// ink, and the upper/lower case alphabet and the digits all have real page
    /// cells. This is the per-face check the one-glyph shift failed.
    #[test]
    fn the_pangram_reads_in_every_face() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).expect("the shipped faces");
        let faces = [
            "fg_d18", "fg_d20", "fg_d24", "fg_d26", "fg_dcn14", "fg_dcn16", "fg_mcn12", "fg_mcn14",
            "fg_mcn16",
        ];
        let pangram = "The quick brown fox jumps over the lazy dog. 0123456789";
        for face in faces {
            let font = fonts.get(face).expect(face);
            let frame = Frame::new(
                0.0,
                0.0,
                font.extent(pangram).0 as f32,
                font.line_height() as f32,
            );
            let run = text_run(font, frame, pangram);
            assert_eq!(
                run.len(),
                pangram.chars().filter(|c| *c != ' ').count(),
                "{face}: one glyph per non-space character"
            );
            let sum: i32 = pangram.chars().map(|c| font.advance(c)).sum();
            assert_eq!(font.extent(pangram).0, sum, "{face}: the run's width");
            let (tx, ty, tw, th) = font.glyph('T').rect(font.page_width, font.page_height);
            assert_eq!(run[0].cell, [tx, ty, tw, th], "{face}: the first glyph");
            for glyph in &run {
                assert!(
                    glyph.cell[2] > 0 && glyph.cell[3] > 0,
                    "{face}: a zero cell"
                );
                let ink = (glyph.cell[1]..glyph.cell[1] + glyph.cell[3])
                    .flat_map(|y| {
                        (glyph.cell[0]..glyph.cell[0] + glyph.cell[2])
                            .map(move |x| font.coverage(x, y))
                    })
                    .filter(|&v| v > 40)
                    .count();
                assert!(ink > 0, "{face}: a cell with no ink: {glyph:?}");
            }
            for c in ('A'..='Z').chain('a'..='z').chain('0'..='9') {
                let (x, y, w, h) = font.glyph(c).rect(font.page_width, font.page_height);
                assert!(w > 0 && h > 0, "{face}: {c:?} has no cell");
                let ink = (y..y + h)
                    .flat_map(|yy| (x..x + w).map(move |xx| font.coverage(xx, yy)))
                    .filter(|&v| v > 40)
                    .count();
                assert!(ink > 0, "{face}: {c:?} has no ink");
            }
        }
    }

    /// The bot lab's title and the team picker's title are `fg_d20` runs whose first cell is
    /// their first letter, whose width is the advance sum, and whose cells hold
    /// ink.
    #[test]
    fn the_bot_lab_and_team_picker_titles_still_read() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).expect("the shipped faces");
        for (screen, title) in [
            ("botlab/botlabwin.gib", "ROBOT WORKSHOP"),
            ("teamhq/select_team.gib", "SELECT YOUR ROBOTICS TEAM"),
        ] {
            let text = std::fs::read_to_string(game.join("UI").join(screen)).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let placed = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));
            let body = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.text() == Some(title))
                .unwrap_or_else(|| panic!("{screen}: {title}"));
            let font = fonts.get("fg_d20").expect("fg_d20");
            let run = text_run(font, body.frame, title);
            let first = title.chars().next().unwrap();
            let (x, y, w, h) = font.glyph(first).rect(font.page_width, font.page_height);
            assert_eq!(run[0].cell, [x, y, w, h], "{screen}: the first glyph");
            let sum: i32 = title.chars().map(|c| font.advance(c)).sum();
            assert_eq!(
                font.extent(title).0,
                sum,
                "{screen}: the run's width is the advance sum"
            );
            for glyph in &run {
                let ink = (glyph.cell[1]..glyph.cell[1] + glyph.cell[3])
                    .flat_map(|yy| {
                        (glyph.cell[0]..glyph.cell[0] + glyph.cell[2])
                            .map(move |xx| font.coverage(xx, yy))
                    })
                    .filter(|&v| v > 40)
                    .count();
                assert!(ink > 0, "{screen}: a cell with no ink: {glyph:?}");
            }
        }
    }

    /// **The corpus's captions are all one `Text` body.** 122 `Caption`
    /// attachments in 53 of the 116 files (the 758 attachments also hold 295
    /// `Background`s), and each caption is a body with exactly one non-empty
    /// `Text` — the shape `centre_captions` reads. The same page/advance model
    /// that fixed the main menu is what every one of them is drawn with; there
    /// is no face- or screen-specific caption path.
    #[test]
    fn every_caption_in_the_corpus_is_one_text_body() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        assert_eq!(files.len(), 116, "the shipped screens");

        let (mut captions, mut backgrounds, mut attachments, mut files_with) = (0, 0, 0, 0);
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let mut here = 0;
            for widget in gib.walk() {
                attachments += widget.attachments.len();
                for attachment in &widget.attachments {
                    match attachment.kind {
                        AttachmentKind::Caption => {
                            here += 1;
                            captions += 1;
                            let body = attachment.body.as_deref().expect("a caption body");
                            assert_eq!(
                                body.children.len(),
                                1,
                                "{}: a caption body with {} children",
                                path.display(),
                                body.children.len()
                            );
                            let child = &body.children[0];
                            assert_eq!(child.kind, WidgetKind::Text, "{}", path.display());
                            assert!(
                                child.text().is_some_and(|t| !t.is_empty()),
                                "{}: an empty caption",
                                path.display()
                            );
                        }
                        AttachmentKind::Background => backgrounds += 1,
                        _ => {}
                    }
                }
            }
            if here > 0 {
                files_with += 1;
            }
        }
        assert_eq!(
            (captions, backgrounds, attachments, files_with),
            (122, 295, 758, 53),
            "the corpus's caption/background/attachment counts"
        );
    }

    /// **Each caption is drawn once, at the frame the data gives.** A button's
    /// `Caption` attachment carries a body with one `Text`; the body is the
    /// button's own frame and the text sits at its authored position inside it. The draw order must contain that text exactly once. The run is
    /// spaced by the face's advances, its width is their sum, and every glyph
    /// samples a page cell with ink — the three properties the old readings got
    /// wrong when a caption looked like two overlapping runs.
    #[test]
    fn each_caption_is_drawn_once_at_its_buttons_frame() {
        let Some(game) = game_dir() else { return };
        let fonts = Fonts::load(&game.join("RA2.cfz")).expect("the shipped faces");
        let text = std::fs::read_to_string(game.join("UI/mainmenu/mainmenu.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));

        for (button_name, caption, face) in [
            ("exhibition button", "EXHIBITION", "fg_d20"),
            ("Team HQ button", "TEAM HQ", "fg_d20"),
            ("Multiplayer button", "MULTIPLAYER", "fg_d20"),
            ("Options button", "Options", "fg_mcn16"),
            ("credits button", "Credits", "fg_mcn16"),
            ("Quit button", "Quit", "fg_mcn16"),
        ] {
            let button = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == button_name)
                .unwrap_or_else(|| panic!("{button_name}"));
            let placed_caption = button
                .attachments
                .iter()
                .find(|a| a.attachment.kind == AttachmentKind::Caption)
                .unwrap_or_else(|| panic!("{button_name}: Caption"));
            let texts: Vec<&Placement> = placed_caption
                .body
                .walk()
                .filter(|p| p.widget.kind == WidgetKind::Text)
                .collect();
            assert_eq!(texts.len(), 1, "{button_name}: one caption Text");
            let caption_text = texts[0].widget.text().expect("the caption's text");
            assert_eq!(caption_text, caption, "{button_name}");

            // Drawn exactly once in the whole screen's draw order.
            let draws: Vec<&Placement> = draw_order_parented(&placed)
                .into_iter()
                .filter(|(p, _)| p.widget.text() == Some(caption))
                .map(|(p, _)| p)
                .collect();
            assert_eq!(draws.len(), 1, "{button_name}: drawn once");

            // Flush left at the authored x inside the button, baseline at the
            // authored y: not centred (the original's captions are not).
            let t = texts[0].frame;
            let at = texts[0].widget.position().expect("the caption's Position");
            let ascent = fonts.get(face).expect("the caption's face").ascent as f32;
            let body = placed_caption.body.frame;
            assert!(
                (t.x - (body.x + at.first.amount)).abs() < 0.01,
                "{button_name}: x {} vs {}",
                t.x,
                body.x + at.first.amount
            );
            assert!(
                (t.y + ascent - (body.y + at.second.amount)).abs() < 0.01,
                "{button_name}: baseline {} vs {}",
                t.y + ascent,
                body.y + at.second.amount
            );

            // The run's width is the sum of the advances, and its cells are the
            // page's own runs: every one has ink, and no two overlap.
            let font = fonts.get(face).expect("the caption's face");
            let sum: i32 = caption.chars().map(|c| font.advance(c)).sum();
            assert_eq!(t.w, sum as f32, "{button_name}: run width");
            let run = text_run(font, t, caption);
            assert_eq!(
                run.len(),
                caption.chars().filter(|c| *c != ' ').count(),
                "{button_name}: one glyph per non-space character"
            );
            let mut cells: Vec<(i32, i32, i32)> = Vec::new();
            for glyph in &run {
                assert!(
                    glyph.cell[2] > 0 && glyph.cell[3] > 0,
                    "{button_name}: a cell"
                );
                let (w, h) = (font.page_width as i32, font.page_height as i32);
                let ink = (glyph.cell[1]..glyph.cell[1] + glyph.cell[3])
                    .flat_map(|y| {
                        (glyph.cell[0]..glyph.cell[0] + glyph.cell[2])
                            .map(move |x| font.coverage(x, y))
                    })
                    .filter(|&v| v > 40)
                    .count();
                assert!(ink > 0, "{button_name}: a cell with no ink {glyph:?}");
                assert!(
                    glyph.cell[0] + glyph.cell[2] <= w && glyph.cell[1] + glyph.cell[3] <= h,
                    "{button_name}: a cell outside the page"
                );
                cells.push((glyph.cell[1], glyph.cell[0], glyph.cell[0] + glyph.cell[2]));
            }
            cells.sort_unstable();
            cells.dedup();
            for pair in cells.windows(2) {
                if pair[0].0 == pair[1].0 {
                    assert!(
                        pair[0].2 <= pair[1].1 || pair[1].2 <= pair[0].1,
                        "{button_name}: two glyphs sample overlapping page cells"
                    );
                }
            }
            // The drawn quads advance in pen order and only touch by the ink's
            // one- or two-pixel overhang: no glyph is drawn twice and no run is
            // laid out at two different x's.
            for pair in run.windows(2) {
                let overlap = pair[0].rect.x + pair[0].rect.w - pair[1].rect.x;
                assert!(
                    overlap <= 2.0,
                    "{button_name}: quads overlap by {overlap} px at {:?}",
                    pair[0].rect
                );
            }
        }
    }

    /// The places where two siblings share a `ChildOrder` **and** their frames
    /// overlap: **13 pairs**, which is where the tie-break matters at all, and
    /// they are kept as a baseline. The older count of 328 was mostly a control
    /// followed by the headerless record that draws its caption; that record is
    /// the `Caption` attachment's body, not a sibling, which is the same reason
    /// the corpus's 758 headerless records nest (see `Attachment::body`).
    ///
    /// What they do *not* do is decide which way the tie goes. The largest is
    /// `multiplayer/multiplayer.gib`'s server list against its heading grid, and
    /// the file order says the earlier record is in front; the rule is the one
    /// `draw_order_parented` implements.
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
            14,
            "equal-ChildOrder overlapping sibling pairs (Backgrounds now fill their owner)"
        );
        // The 328 older pairs were mostly a control followed by the caption
        // holder that draws its text. That holder is the `Caption` attachment's
        // *body*, not a sibling, so it is no longer part of this count: the
        // caption is drawn in front because of what the attachment is, not
        // because of a `ChildOrder` tie. What is left is 12 true sibling
        // overlaps; the largest is `multiplayer.gib`'s list against its heading
        // grid, and the file order still says the earlier one is in front.
        for button in ["Options button", "MainMenu button", "Help button"] {
            let gib = std::fs::read_to_string(ui.join("common/bottomnav.gib")).unwrap();
            let gib = ra2fmt::gib::parse(&gib).unwrap();
            let found = gib.walk().find(|w| w.name() == button).unwrap();
            let caption = found.attachment("Caption").expect("a Caption attachment");
            let body = caption.body.as_deref().expect("the caption's body");
            assert_eq!(body.children[0].kind, WidgetKind::Text, "{button}");
        }
        assert!(pairs.iter().any(|p| p.0 == "multiplayer/multiplayer.gib"
            && p.1.starts_with("server listbox")
            && p.3 == "list grp"));
    }

    /// Every shipped file, placed: 4,348 widgets, all finite, and 155 whose
    /// frame reaches past the screen. Those are of two kinds, both expected:
    /// *fragments* — files like `botlab/control_cell.gib` that are a content
    /// slot's payload and were never meant to be placed against the screen —
    /// and children whose declared `Size` is larger than the pane they sit in,
    /// which the game clips (`ClipMode`) where this resolver does not yet.
    /// Fifty-seven of the 116 files place entirely on screen.
    ///
    /// Every piece of art a shipped screen names, resolved the way the renderer
    /// resolves it — the loose tree under `UI`, then the game's own root (a
    /// component's preview lives there), then `RA2.cfz` by stem — and the ones
    /// that do not resolve, which are the game's own gaps rather than ours.
    #[test]
    fn every_art_a_screen_names_ships() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let archive = ra2fmt::cfl::Cfl::open_path(&game.join("RA2.cfz")).ok();
        let mut files = Vec::new();
        collect_gib(&ui, &mut files);

        let mut unresolved: Vec<(String, String)> = Vec::new();
        let mut named = 0;
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let screen = path
                .strip_prefix(&ui)
                .unwrap()
                .to_string_lossy()
                .to_string();
            for widget in gib.walk() {
                // The art a widget names, and the face it asks for: a `Glyph`'s
                // art is a font page, so the two are the same question.
                let mut names: Vec<(String, bool)> = Vec::new();
                if let Some(art) = widget.filename().filter(|f| !f.is_empty()) {
                    names.push((art.to_string(), false));
                }
                for field in &widget.fields {
                    if field.name.as_deref() == Some("_Font") {
                        let face = field.text();
                        if !face.is_empty() {
                            names.push((format!("{face}.giff"), true));
                        }
                    }
                }
                for (art, face) in names {
                    named += 1;
                    let key = art.replace('\\', "/");
                    let key = key.trim_start_matches("./").trim_start_matches('/');
                    let loose = ra2fmt::find_ci(&ui, key).is_some()
                        || ra2fmt::find_ci(&game, key).is_some();
                    let stem = key.rsplit('/').next().unwrap_or(key);
                    let archived = archive.as_ref().is_some_and(|a| a.entry(stem).is_some());
                    if !(loose || archived) {
                        unresolved.push((
                            screen.clone(),
                            if face { format!("face {art}") } else { art },
                        ));
                    }
                }
            }
        }
        // The shell's own inventory: 1,303 names and nine that do not resolve.
        assert_eq!(named, 1303, "art names across the shipped screens");
        assert_eq!(unresolved.len(), 9, "{unresolved:?}");

        // Seven of the nine are `_Font System`, which is not a gap at all: those
        // widgets carry **no text either** (`_Text` is empty on every one), and
        // `System` is nowhere in the game — no `.giff` by that name in either
        // archive and no such file — so it is the editor's default for a widget
        // that has neither a face nor words. There is nothing to draw there.
        let mut faces: Vec<&String> = unresolved
            .iter()
            .filter(|(_, what)| what == "face System.giff")
            .map(|(screen, _)| screen)
            .collect();
        faces.sort();
        assert_eq!(
            faces,
            [
                "botlab/import_texture.gib",
                "botlab/importbot.gib",
                "botlab/info_display.gib",
                "botlab/surfacelayer.gib",
                "common/credits.gib",
                "teamhq/editteam.gib",
                "teamhq/repairs.gib"
            ],
            "the screens that ask for the system font"
        );

        // And two are art the game does **not ship**: `teamhq/xout.tga`, the
        // event cell's overlay, and `teamhq/event_bba.tga`, the BBA event's
        // winner art. So those two draw nothing, in the game and here.
        let mut missing: Vec<&String> = unresolved
            .iter()
            .filter(|(_, what)| what.ends_with(".tga"))
            .map(|(_, what)| what)
            .collect();
        missing.sort();
        assert_eq!(
            missing,
            ["teamhq\\event_bba.tga", "teamhq\\xout.tga"],
            "art the game names and does not ship"
        );
    }

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
        // first slot. The corpus is 4,348 widgets, and since every attachment
        // takes its body there are no ownerless records left to place against
        // the screen: the 54 that still reach past it are fragments and
        // authored oversize frames, not nesting failures. Eighty-two of the
        // 116 files place entirely on screen. (A `TextBox` keeps its authored
        // rectangle now — its `Alignment` is text alignment, not a box anchor —
        // and a `Text` child is positioned by its baseline, so fewer frames are
        // pushed past the edge than the earlier reading had.)
        assert_eq!(placed, 4348);
        assert_eq!(
            outside, 54,
            "off screen (a Background fills its owner, so fewer frames overshoot)"
        );
        assert_eq!(perfect, 82, "perfect files");
        // The worst offender is a fragment, not a screen.
        assert!(
            worst.1.ends_with("botlab/armor.gib")
                || worst.1.ends_with("botlab/adjust_none.gib")
                || worst.1.ends_with("botlab/comp_adjust.gib")
                || worst.1.ends_with("exhibition/gametype.gib"),
            "{} {}",
            worst.1,
            worst.0
        );
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

        // `botlabwin.gib`'s proxy sits in the strip at the bottom of the
        // window: `lowersplit`'s slot 1 is the master (`Master 1`) and takes
        // the last 12%, so the bar is flush with the screen's bottom edge
        // (1005 - 36), as in the original.
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
        assert_eq!(bar.frame, Frame::new(512.0, 969.0, 256.0, 36.0));
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
        assert!((frame("lowersplit").y - 140.7).abs() < 0.1);
        assert!(
            (frame("content slot").y - 140.7).abs() < 0.1,
            "{}",
            frame("content slot").y
        );
        assert!((frame("content slot").h - 760.6).abs() < 0.1);
        let split = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "lowersplit")
            .unwrap();
        assert_eq!(split.children.len(), 2);
        // 12% of the 864.3 it splits, at the far end of the split.
        assert!((split.children[1].frame.h - 103.7).abs() < 0.1);
        assert!((split.children[1].frame.y - 901.3).abs() < 0.1);
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
        // label does, and a wrapped second line sits one ascent lower (the exe's line advance).
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
            font.ascent as f32,
            "the second line is one ascent below the first (FUN_00602210)"
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

    /// **Closed by the attachment body.** A record's `NumChildren` counts its
    /// *named* children only, and the headerless records — 758 of them, the base
    /// `Widget` class — are the bodies of the attachments. Once each attachment
    /// takes its body (written after the decorated widget's subtree), every one
    /// of the 116 shipped files nests to exactly **one root**.
    ///
    /// Before the body rule this test recorded the opposite — 49 files closed to
    /// one root and `import_texture.gib` left six records at the top level —
    /// because the bodies had no owner and were placed as siblings. The counts
    /// are the data's: `sum(NumChildren)` plus the 758 headerless records is
    /// `records - 1` in every file, and each headerless record belongs to the
    /// attachment written before the widget it wraps.
    #[test]
    fn every_file_nests_to_one_root() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let mut worst = (0usize, String::new());
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let roots = gib.widgets().count();
            if roots > worst.0 {
                worst = (roots, path.display().to_string());
            }
        }
        assert_eq!(files.len(), 116);
        assert_eq!(worst.0, 1, "worst roots: {}", worst.1);
    }

    /// A negative `ChildOrder` means *no order*, not "behind": a negative order
    /// on a **control** draws last, and on **chrome** it still draws first. The
    /// corpus has 613 negative orders and 73 overlap a non-negative sibling. The
    /// two worked cases are the main menu's buttons (-1 under a plate whose
    /// images are 1..7) and `select_team.gib`'s full-screen `cement img` (-1,
    /// which must stay behind its dialog).
    #[test]
    fn a_negative_child_order_draws_last_only_on_a_control() {
        let Some(game) = game_dir() else { return };
        let mut files = Vec::new();
        collect_gib(&game.join("UI"), &mut files);
        let (mut negatives, mut overlapping, mut controls) = (0usize, 0usize, 0usize);
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = ra2fmt::gib::parse(&text).unwrap();
            let placed = resolve(&gib, DESIGN_SCREEN);
            for parent in placed.iter().flat_map(|p| p.walk()) {
                for child in &parent.children {
                    if child.widget.child_order() >= 0 {
                        continue;
                    }
                    negatives += 1;
                    controls += usize::from(draws_last_when_negative(&child.widget.kind));
                    if parent
                        .children
                        .iter()
                        .any(|c| c.widget.child_order() >= 0 && c.frame.intersects(&child.frame))
                    {
                        overlapping += 1;
                    }
                }
            }
        }
        assert_eq!(negatives, 613, "children with a negative ChildOrder");
        assert_eq!(overlapping, 72, "those overlapping a non-negative sibling");
        assert_eq!(controls, 256, "negative orders on pressed controls");

        let text = std::fs::read_to_string(game.join("UI/mainmenu/mainmenu.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let order = draw_order(&placed);
        let at = |name: &str| {
            order
                .iter()
                .position(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("{name} is not drawn"))
        };
        let plate = order
            .iter()
            .position(|p| {
                p.widget
                    .filename()
                    .is_some_and(|f| f.replace('\\', "/").ends_with("main_backdrop.tga"))
            })
            .expect("the saw plate");
        for button in ["exhibition button", "Team HQ button", "Multiplayer button"] {
            assert!(plate < at(button), "{button} is behind the saw plate");
        }

        // The chrome with a negative order stays behind: `select_team.gib`'s
        // cement is -1 and its `continue button` is the control a click must
        // reach, at 779,443 82x20.
        let text = std::fs::read_to_string(game.join("UI/teamhq/select_team.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let order = draw_order(&placed);
        let at = |name: &str| {
            order
                .iter()
                .position(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("{name} is not drawn"))
        };
        assert!(
            at("cement img") < at("continue button"),
            "the cement is the screen behind the dialog"
        );
        let path = crate::input::hit_control(&placed, 820.0, 883.0).expect("the continue button");
        let mut placement = &placed[path[0]];
        let mut names = vec![placement.widget.name()];
        for index in &path[1..] {
            placement = &placement.children[*index];
            names.push(placement.widget.name());
        }
        assert!(
            names.contains(&"continue button"),
            "the click lands on the continue button, path {names:?}"
        );
    }

    /// Draw order is `ChildOrder` ascending, ties by file order — which is what
    /// puts the full-screen background first and the logo after it.
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
        // Each cell starts at the pen plus its glyph's left bearing, and sits
        // `bearing_y` above the baseline `ascent` under the frame's top.
        let mut pen = frame.x + TEXT_ORIGIN[0];
        for (glyph, placed) in "MULTIPLAYER".chars().zip(&run) {
            let g = font.glyph(glyph);
            assert_eq!(placed.rect.x, pen + g.bearing_x as f32, "{glyph}");
            assert_eq!(
                placed.rect.y,
                frame.y + TEXT_ORIGIN[1] + font.ascent as f32 - g.bearing_y as f32,
                "{glyph}"
            );
            pen += font.advance(glyph) as f32;
        }
        let (width, height) = font.extent("MULTIPLAYER");
        assert_eq!(
            pen - frame.x - TEXT_ORIGIN[0],
            width as f32,
            "the pen ends at the extent"
        );
        assert_eq!(height, font.line_height());
        // A space has no cell but does advance.
        let spaced = text_run(font, frame, "A B");
        assert_eq!(spaced.len(), 2);
        assert_eq!(
            spaced[1].rect.x,
            frame.x
                + TEXT_ORIGIN[0]
                + font.advance('A') as f32
                + font.space_advance() as f32
                + font.glyph('B').bearing_x as f32
        );
    }

    /// A `TextBox` keeps its authored rectangle (its `Alignment` is the text's,
    /// not the box's), and its `Text` child is placed by the baseline. Two
    /// shipped cases pin both halves: the match-results title inside its panel
    /// and `statusmeters.gib`'s points under `POINTS:`.
    #[test]
    fn a_textbox_is_a_box_and_its_text_is_a_baseline() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let fonts = Fonts::load(&game.join("RA2.cfz")).unwrap();
        let text = std::fs::read_to_string(ui.join("common/matchresults.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));
        let title = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "New [TextBox]" && p.widget.kind == WidgetKind::TextBox)
            .expect("the title's TextBox");
        assert_eq!(title.frame.x, 380.0);
        assert_eq!(title.frame.y, 289.5);
        assert_eq!(title.frame.w, 520.0);
        assert_eq!(title.frame.h, 28.0);
        let child = title
            .children
            .iter()
            .find(|p| p.widget.kind == WidgetKind::Text)
            .expect("the title's Text");
        assert_eq!(child.frame.x, 380.0 + 186.0);
        // The glyphs' ink is inside the box, not clipped below it.
        let font = fonts.get("fg_d24").expect("fg_d24");
        let run = text_run(font, child.frame, "Match Results");
        assert!(run[0].rect.y >= title.frame.y - 4.0, "title above its box");
        assert!(
            run[0].rect.bottom() <= title.frame.bottom(),
            "title below its box"
        );

        let text = std::fs::read_to_string(ui.join("common/statusmeters.gib")).unwrap();
        let gib = ra2fmt::gib::parse(&text).unwrap();
        let placed = resolve_with(&gib, DESIGN_SCREEN, Some(&fonts));
        let points = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "points")
            .expect("the points TextBox");
        assert_eq!(points.frame.x, 46.0);
        assert_eq!(points.frame.y, 51.0);
        let child = points
            .children
            .iter()
            .find(|p| p.widget.kind == WidgetKind::Text)
            .expect("the points Text");
        let font = fonts.get("fg_dcn14").expect("fg_dcn14");
        let run = text_run(font, child.frame, "8888");
        assert!(run[0].rect.y >= points.frame.y - 5.0);
        assert!(run[0].rect.bottom() <= points.frame.bottom() + 5.0);
    }
}
