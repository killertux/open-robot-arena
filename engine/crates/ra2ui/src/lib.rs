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
//! ## What is not resolved yet
//!
//! A `Size` of zero is the editor's *automatic* size — a `Text` is zero-sized
//! and takes its extent from its font, which needs the `.giff` metrics. Those
//! widgets come back as zero-sized frames, and so do their children. Containers
//! that lay their children out themselves — `ListBox`, `Grid`, `OptionGroup` —
//! are treated as ordinary frames; their cells are not placed yet.

use ra2fmt::gib::{Block, Gib, Length, Widget, WidgetKind};

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

/// A widget with its resolved rectangle and its placed children.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement<'a> {
    pub widget: &'a Widget,
    pub frame: Frame,
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

/// Resolve every top-level block of a document against the screen.
pub fn resolve(gib: &Gib, screen: [f32; 2]) -> Vec<Placement<'_>> {
    let screen_frame = Frame::new(0.0, 0.0, screen[0], screen[1]);
    gib.blocks
        .iter()
        .filter_map(|b| match b {
            Block::Widget(w) => Some(place(w, screen_frame)),
            // A fragment has no frame of its own until a slot loads it.
            Block::Attachment(_) => None,
        })
        .collect()
}

/// Place one widget inside `parent`.
pub fn place(widget: &Widget, parent: Frame) -> Placement<'_> {
    let size = widget.size();
    let frame = sized(widget, parent, size);
    let children = match widget.kind {
        // A split frame hands its children panes rather than its own rectangle.
        WidgetKind::SplitFrame => split_children(widget, frame),
        _ => widget
            .children
            .iter()
            .map(|c| place(c, frame))
            .collect::<Vec<_>>(),
    };
    Placement {
        widget,
        frame,
        children,
    }
}

/// A widget's own rectangle: its `Size` against the parent's extent, with its
/// anchor point at its `Position`.
fn sized(widget: &Widget, parent: Frame, size: Option<ra2fmt::gib::Axes>) -> Frame {
    let w = size.map(|s| axis(s.first, parent.w)).unwrap_or(parent.w);
    let h = size.map(|s| axis(s.second, parent.h)).unwrap_or(parent.h);
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
fn split_children(widget: &Widget, frame: Frame) -> Vec<Placement<'_>> {
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
        .and_then(ra2fmt::gib::Axes::parse)
        .map(|a| axis(a.first, if horizontal { frame.w } else { frame.h }))
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
        .map(|(i, c)| place(c, panes[i.min(1)]))
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
        // It is a *top-level* sibling of the window, not a child of it — a
        // screen file is a forest, and every top-level block is placed against
        // the screen.
        let common = placed
            .iter()
            .find(|p| p.widget.name() == "common buts")
            .expect("common buts");
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

    /// Every shipped file, placed: 3590 widgets, all finite, and only 101 whose
    /// frame reaches past the screen. Those are of two kinds, both expected:
    /// *fragments* — files like `botlab/control_cell.gib` that are a content
    /// slot's payload and were never meant to be placed against the screen —
    /// and children whose declared `Size` is larger than the pane they sit in,
    /// which the game clips (`ClipMode`) where this resolver does not yet.
    /// Seventy-one of the 116 files place entirely on screen.
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
        assert_eq!(placed, 3590);
        assert_eq!(outside, 101);
        assert_eq!(perfect, 71);
        // The worst offender is a fragment, not a screen.
        assert!(worst.1.ends_with("botlab/control_cell.gib"), "{}", worst.1);
    }
}
