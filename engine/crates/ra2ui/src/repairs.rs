//! `teamhq/repairs.gib`: the between-rounds repairs window.
//!
//! The screen is the game's own. Its constructor is `FUN_004d5de0` and it
//! binds three widgets (`docs/16-career.md` §5):
//!
//! | widget | the original's use |
//! |---|---|
//! | `repairs list` | one row per damaged component, with the `damage.tga` icon for a fractured one |
//! | `repair time` | the time left on the selected repair |
//! | `repair button` | advances the progress bar; full calls `FUN_0058b470`/`FUN_004d7010` |
//!
//! Repairing is **time, not money**: `FUN_004d6e10` advances a bar and
//! `FUN_005ff530` restores the components when it fills. There is no cost
//! field in the screen and no subtraction path in the binary.
//!
//! This module only composes and fills the screen. The career model owns the
//! repair list, the clock and the completion; the application routes clicks.
//! Like [`crate::hq`], it deliberately has no `ra2sim` dependency.

use std::path::Path;

use ra2fmt::gib::{Block, Gib, Widget, WidgetKind};

use crate::{Fonts, Frame, Grid, Placement};

/// The shipped repairs window.
pub const SCREEN: &str = "teamhq/repairs.gib";
/// The list the binary fills with the damaged components and times.
pub const LIST: &str = "repairs list";
/// The clock's text box (`display remaining repair time here`).
pub const TIME: &str = "repair time";
/// `click to repair the selected item`.
pub const REPAIR_BUTTON: &str = "repair button";
/// `go to event bracket for next round`.
pub const FINISHED_BUTTON: &str = "finished button";
/// The screen's own title (`Round 1 Repairs`).
pub const TITLE: &str = "repairs title";
/// Text face for a list row: the list's own `_Font System` is a tool font, and
/// the screens' small face is what the shipped cells use.
pub const ROW_FACE: &str = "fg_mcn12";

/// One line of the repairs list. `component` is what occupies the first
/// column; `time` is the second column's state string (remaining / total and
/// the bar's percentage). `progress` is the model's bar in `0.0..=1.0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub component: String,
    pub time: String,
    pub progress: f32,
}

impl Row {
    pub fn new(component: impl Into<String>, time: impl Into<String>, progress: f32) -> Row {
        Row {
            component: component.into(),
            time: time.into(),
            progress: progress.clamp(0.0, 1.0),
        }
    }
}

/// What a click on the repairs window did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// A list row was selected.
    Row(usize),
    /// `repair button`: advance the selected repair's clock.
    Repair,
    /// `finished button`: leave the screen.
    Finished,
}

/// The composed repairs screen and its rows.
pub struct Repairs {
    pub document: Gib,
    rows: Vec<Row>,
    selected: usize,
}

impl Repairs {
    /// Compose the screen and fill its list and clock.
    pub fn open(
        ui_root: &Path,
        title: &str,
        rows: Vec<Row>,
        time: &str,
    ) -> Result<Repairs, String> {
        let mut document = load(ui_root, SCREEN)?;
        fit_splits_in(&mut document);
        // The window is authored against an 800×600 canvas; the arena image
        // behind it (`background img`'s own note says so) is stretched to the
        // shell's screen so the window is not floating over black.
        if let Some(image) = find_mut_in_gib(&mut document, "background img") {
            crate::input::set_field(image, "_Size", "1280 0 1005 0");
        }
        let mut repairs = Repairs {
            document,
            rows: Vec::new(),
            selected: 0,
        };
        repairs.set_rows(rows);
        repairs.set_title(title);
        repairs.set_time(time);
        repairs.fill()?;
        Ok(repairs)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
            self.fill().ok();
        }
    }

    /// Replace the list (and keep the selection on a valid row).
    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        if self.selected >= self.rows.len() {
            self.selected = 0;
        }
    }

    /// Re-draw the list after [`Repairs::set_rows`]/[`Repairs::set_time`].
    pub fn refresh(&mut self) -> Result<(), String> {
        self.fill()
    }

    pub fn set_title(&mut self, title: &str) {
        if let Some(widget) = find_mut_in_gib(&mut self.document, TITLE) {
            set_value(widget, title);
        }
    }

    /// The `repair time` box, `m:ss` in the screen's own format.
    pub fn set_time(&mut self, time: &str) {
        if let Some(widget) = find_mut_in_gib(&mut self.document, TIME) {
            set_value(widget, time);
        }
    }

    pub fn placements(&self, screen: [f32; 2], fonts: Option<&Fonts>) -> Vec<Placement<'_>> {
        crate::resolve_with(&self.document, screen, fonts)
    }

    /// Which repair row a point is on, by the list's own geometry.
    pub fn row_at(
        &self,
        point: [f32; 2],
        screen: [f32; 2],
        fonts: Option<&Fonts>,
    ) -> Option<usize> {
        let placed = self.placements(screen, fonts);
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)?;
        if !list.frame.contains_point(point[0], point[1]) {
            return None;
        }
        let grid = Grid::from_widget(list.widget)?;
        if grid.row_height <= 0.0 {
            return None;
        }
        let row = ((point[1] - list.frame.y) / grid.row_height).floor() as usize;
        (row < self.rows.len()).then_some(row)
    }

    /// Route a click: a row, the repair button, the finished button, or none.
    pub fn click(&self, point: [f32; 2], screen: [f32; 2], fonts: Option<&Fonts>) -> Option<Click> {
        let placed = self.placements(screen, fonts);
        let path = crate::input::hit_control(&placed, point[0], point[1])?;
        let mut names = Vec::new();
        let mut current = &placed[path[0]];
        names.push(current.widget.name().to_string());
        for index in &path[1..] {
            current = &current.children[*index];
            names.push(current.widget.name().to_string());
        }
        if names.iter().any(|n| n == REPAIR_BUTTON) {
            return Some(Click::Repair);
        }
        if names.iter().any(|n| n == FINISHED_BUTTON) {
            return Some(Click::Finished);
        }
        if names.iter().any(|n| n == LIST) {
            return self.row_at(point, screen, fonts).map(Click::Row);
        }
        None
    }

    /// Fill the shipped `repairs list` with the rows. Two `Text` widgets per
    /// row: the component in the first column and the time/state in the second,
    /// which are the list's own `ColWidths 116 17 116 17`.
    fn fill(&mut self) -> Result<(), String> {
        let rows = self.rows.clone();
        let selected = self.selected;
        let Some(list) = find_mut_in_gib(&mut self.document, LIST) else {
            return Err(format!("{SCREEN} has no {LIST}"));
        };
        let Some(grid) = Grid::from_widget(list) else {
            return Err(format!("{LIST} has no Rows/Columns"));
        };
        let height = grid.row_height.max(18.0);
        let width = grid.col_widths.first().copied().unwrap_or(116.0);
        let second_x = width;
        // Rows are the text children this module made; the file's own selection
        // and highlight bars stay.
        list.children.retain(|c| {
            !(c.kind == WidgetKind::Text
                && c.field("_Font").is_some()
                && c.name().starts_with("repair row"))
        });
        for (index, row) in rows.iter().enumerate() {
            let y = height * index as f32;
            let mut left = row_widget(
                &format!("repair row {index} component"),
                &row.component,
                0.0,
                y,
                width,
                height,
            );
            let mut right = row_widget(
                &format!("repair row {index} time"),
                &row.time,
                second_x,
                y,
                width,
                height,
            );
            if index == selected {
                crate::input::set_field(&mut left, "_ForeColor", "1 1 0.6 1");
                crate::input::set_field(&mut right, "_ForeColor", "1 1 0.6 1");
            }
            list.children.push(left);
            list.children.push(right);
        }
        // The file's `Sel Bar` follows the selected row (the authors' own bar).
        let rows_len = rows.len();
        if let Some(bar) = list.children.iter_mut().find(|c| c.name() == "Sel Bar") {
            crate::input::set_field(
                bar,
                "Position",
                &format!(
                    "0 0 {} 0",
                    height * selected.min(rows_len.saturating_sub(1)) as f32
                ),
            );
            crate::input::set_field(bar, "Visible", if rows_len == 0 { "false" } else { "true" });
        }
        Ok(())
    }
}

fn row_widget(name: &str, text: &str, x: f32, y: f32, w: f32, h: f32) -> Widget {
    use ra2fmt::gib::Field;
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: WidgetKind::Text,
        implicit: false,
        fields: vec![
            field("_Font", ROW_FACE),
            field("_Text", text),
            field("_ForeColor", "1 1 1 1"),
            field("_Name", ""),
            field("_Position", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", &format!("{x} 0 {y} 0")),
            field("Size", &format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "0"),
            field("ClipMode", "1"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

/// A `.gib` under a game's `UI` directory, with its proxies inlined.
pub fn load(ui_root: &Path, relative: &str) -> Result<Gib, String> {
    let path = ra2fmt::find_ci(ui_root, relative)
        .ok_or_else(|| format!("{relative} is not in {}", ui_root.display()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut gib = ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::inline_proxies_in(&mut gib, ui_root);
    Ok(gib)
}

/// The first widget called `name`, depth first.
pub fn find<'a>(widget: &'a Widget, name: &str) -> Option<&'a Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    widget.children.iter().find_map(|c| find(c, name))
}

fn find_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    widget.children.iter_mut().find_map(|c| find_mut(c, name))
}

fn find_mut_in_gib<'a>(document: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    document.blocks.iter_mut().find_map(|b| match b {
        Block::Widget(w) => find_mut(w, name),
        Block::Attachment(_) => None,
    })
}

/// Make every `SplitFrame`'s children fill their pane, as the HQ does: the
/// fragment is authored at 1280×1005 and the game hands it the content frame.
pub fn fit_splits_in(gib: &mut Gib) {
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(widget) = block {
            fit_splits(widget);
        }
    }
}

fn fit_splits(widget: &mut Widget) {
    if widget.kind == WidgetKind::SplitFrame {
        for child in widget.children.iter_mut() {
            crate::input::set_field(child, "Position", "0 0 0 0");
            crate::input::set_field(child, "Size", "1 1 1 1");
        }
    }
    for child in widget.children.iter_mut() {
        fit_splits(child);
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = &mut attachment.body {
            fit_splits(body);
        }
    }
}

/// Write a value into a label: a `TextBox` keeps it in its `Text` child, a
/// `Text` in its own `_Text`.
fn set_value(widget: &mut Widget, value: &str) {
    if let Some(text) = widget
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Text)
    {
        crate::input::set_field(text, "_Text", value);
    } else {
        crate::input::set_field(widget, "_Text", value);
    }
}

/// The screen's own time format: minutes and seconds (`24:30` in the file).
pub fn format_time(seconds: f32) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".to_string();
    }
    let whole = seconds.ceil() as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// The 3D placeholder's name; the original shows the robot needing repairs
/// there. The app leaves it as the screen's own dark backing.
pub const PLACEHOLDER: &str = "repairs 3d placeholder";

/// The rectangle of the repairs 3D placeholder, for a caller that wants to
/// composite a preview later.
pub fn placeholder_frame(placed: &[Placement<'_>]) -> Option<Frame> {
    placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == PLACEHOLDER)
        .map(|p| p.frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DESIGN_SCREEN;
    use std::path::PathBuf;

    fn ui_root() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game/UI");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if let Some(game) = std::env::var_os("RA2_GAME") {
                let candidate = PathBuf::from(game).join("UI");
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    #[test]
    fn the_list_is_the_careers_rows_and_the_empty_state_is_empty() {
        let Some(ui) = ui_root() else { return };
        let rows = vec![
            Row::new("Damaged Bot", "0:18 / 0:30 (40%)", 0.4),
            Row::new("Bent Axle", "0:06 / 0:30 (80%)", 0.8),
        ];
        let screen = Repairs::open(&ui, "Round 1 Repairs", rows, "0:18").unwrap();
        let placed = screen.placements(DESIGN_SCREEN, None);
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)
            .unwrap();
        let texts: Vec<&str> = list
            .children
            .iter()
            .filter(|c| c.widget.kind == WidgetKind::Text)
            .filter_map(|c| c.widget.text())
            .collect();
        assert!(texts.contains(&"Damaged Bot"));
        assert!(texts.contains(&"0:18 / 0:30 (40%)"));
        assert!(texts.contains(&"Bent Axle"));
        let time = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == TIME)
            .unwrap();
        let label = time
            .children
            .iter()
            .find(|c| c.widget.kind == WidgetKind::Text)
            .and_then(|c| c.widget.text())
            .or_else(|| time.widget.text());
        assert_eq!(label, Some("0:18"));

        // The screen's own empty state: no rows, the clock at zero.
        let empty = Repairs::open(&ui, "Round 1 Repairs", Vec::new(), "0:00").unwrap();
        let placed = empty.placements(DESIGN_SCREEN, None);
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)
            .unwrap();
        assert_eq!(
            list.children
                .iter()
                .filter(|c| c.widget.kind == WidgetKind::Text)
                .count(),
            0
        );
        assert_eq!(empty.rows().len(), 0);
    }

    #[test]
    fn the_repair_button_and_a_row_are_hit_tested_by_the_shipped_widgets() {
        let Some(ui) = ui_root() else { return };
        let rows = vec![Row::new("Damaged Bot", "0:30 / 0:30 (0%)", 0.0)];
        let screen = Repairs::open(&ui, "Round 1 Repairs", rows, "0:30").unwrap();
        let placed = screen.placements(DESIGN_SCREEN, None);
        let button = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == REPAIR_BUTTON)
            .unwrap();
        let point = [
            button.frame.x + button.frame.w / 2.0,
            button.frame.y + button.frame.h / 2.0,
        ];
        assert_eq!(
            screen.click(point, DESIGN_SCREEN, None),
            Some(Click::Repair)
        );
        let row = screen.row_at(
            [list_x(&placed) + 10.0, list_y(&placed) + 9.0],
            DESIGN_SCREEN,
            None,
        );
        assert_eq!(row, Some(0));
        assert_eq!(format_time(30.0), "0:30");
        assert_eq!(format_time(0.0), "0:00");
    }

    fn list_x(placed: &[Placement<'_>]) -> f32 {
        placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)
            .map(|p| p.frame.x)
            .unwrap_or(0.0)
    }

    fn list_y(placed: &[Placement<'_>]) -> f32 {
        placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)
            .map(|p| p.frame.y)
            .unwrap_or(0.0)
    }
}
