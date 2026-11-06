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
/// The fractured row's icon: the string `FUN_004d6480` loads at `0x6fd5b8`
/// when the `Team+0x13c` flag is 1.
pub const FRACTURED_ICON: &str = "teamhq\\damage.tga";

/// One line of the repairs list. `robot`, `component` and `class` are the
/// first columns; `time` is the second column's state string (remaining / total
/// and the bar's percentage). `progress` is the model's bar in `0.0..=1.0`.
/// `hp`/`total_hp` are the shipped health bar's current and maximum, and
/// `fractured` draws the `teamhq\damage.tga` icon (`FUN_004d6480`'s
/// `0x6fd5b8`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub robot: String,
    pub component: String,
    pub class: String,
    pub time: String,
    pub progress: f32,
    pub hp: f32,
    pub total_hp: f32,
    pub fractured: bool,
}

impl Row {
    /// The original three-field row: a component name, its time/state and the
    /// bar. Kept for callers that have no robot/class split.
    pub fn new(component: impl Into<String>, time: impl Into<String>, progress: f32) -> Row {
        Row {
            robot: String::new(),
            component: component.into(),
            class: String::new(),
            time: time.into(),
            progress: progress.clamp(0.0, 1.0),
            hp: 0.0,
            total_hp: 0.0,
            fractured: false,
        }
    }

    /// A component row: the robot and component names, the component class,
    /// the current/total hitpoints and the fractured state.
    #[allow(clippy::too_many_arguments)]
    pub fn component(
        robot: impl Into<String>,
        component: impl Into<String>,
        class: impl Into<String>,
        time: impl Into<String>,
        progress: f32,
        hp: f32,
        total_hp: f32,
        fractured: bool,
    ) -> Row {
        Row {
            robot: robot.into(),
            component: component.into(),
            class: class.into(),
            time: time.into(),
            progress: progress.clamp(0.0, 1.0),
            hp,
            total_hp,
            fractured,
        }
    }

    /// The remaining hitpoint percentage, as the row's label prints it.
    pub fn hp_percent(&self) -> i32 {
        if self.total_hp > 0.0 {
            ((self.hp / self.total_hp).clamp(0.0, 1.0) * 100.0).round() as i32
        } else {
            0
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
        let document = load(ui_root, SCREEN)?;
        // The screen is placed as the file writes it: forcing every `SplitFrame` child to fill its pane
        // made the column header and the list as wide as the pane (394 px) where the file says 0.95 of it
        // (374; live `event-repairs`: the header ends at x 932 and the bar column is outside it).
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

    /// Fill the shipped `repairs list` with the rows. Live (`event-repairs-damaged`) the list draws
    /// **no text** in either column, only the cell lines of its rows (`BorderColor 0 0 0 1`,
    /// `BorderWidth 1`): a 1 px line above every row (also the row after the last), a line down the
    /// left edge, the column edge after 0.7 of the width (`Component` 261 px, `Time` 112 of 373) and
    /// the right edge, and the selection bar spans the selected row inside the lines. A bench with
    /// no rows (an undamaged robot, `event-repairs-atom`) draws none of it. The fractured icon of
    /// [`FRACTURED_ICON`] is kept (`FUN_004d6480`).
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
        // Rows are the widgets this module made; the file's own selection and
        // highlight bars stay.
        list.children
            .retain(|c| !c.name().starts_with("repair row") && !c.name().starts_with("row line"));
        let drawn = rows.len();
        for (index, row) in rows.iter().enumerate() {
            if row.fractured {
                list.children.push(icon_widget(
                    &format!("repair row {index} icon"),
                    FRACTURED_ICON,
                    1.0,
                    height * index as f32 + 1.0,
                    height - 1.0,
                    height - 1.0,
                ));
            }
        }
        let line = |name: String, pos: &str, size: &str| {
            let field = |name: &str, raw: &str| ra2fmt::gib::Field {
                name: Some(name.to_string()),
                raw: raw.to_string(),
            };
            Widget {
                kind: WidgetKind::Rectangle,
                implicit: false,
                fields: vec![
                    field("Filled", "true"),
                    field("FillColor", "0 0 0 1"),
                    field("BorderWidth", "0"),
                    field("BorderColor", "0 0 0 0"),
                    field("Name", &name),
                    field("ID", "-1"),
                    field("Position", pos),
                    field("Size", size),
                    field("Visible", "true"),
                    field("ChildOrder", "6"),
                    field("ClipMode", "0"),
                    field("NumChildren", "0"),
                ],
                attachments: Vec::new(),
                children: Vec::new(),
            }
        };
        for i in (0..=drawn).filter(|_| drawn > 0) {
            list.children.push(line(
                format!("row line {i}"),
                &format!("0 0 {} 0", height * i as f32),
                "1 1 1 0",
            ));
        }
        let down = format!("1 0 {} 0", height * drawn as f32 + 1.0);
        if drawn > 0 {
            list.children
                .push(line("row line left".into(), "0 0 0 0", &down));
            list.children
                .push(line("row line mid".into(), "0.7 1 0 0", &down));
        }
        // The right line is the list's last pixel column (live x = 931 of 558 + 374): its right
        // edge on the list's.
        if drawn > 0 {
            let mut right = line("row line right".into(), "1 1 0 0", &down);
            right.fields.push(ra2fmt::gib::Field {
                name: Some("Alignment".to_string()),
                raw: "12".to_string(),
            });
            list.children.push(right);
        }
        // The file's `Sel Bar` follows the selected row: inside the lines, the whole row wide.
        if let Some(bar) = list.children.iter_mut().find(|c| c.name() == "Sel Bar") {
            crate::input::set_field(
                bar,
                "Position",
                &format!(
                    "1 0 {} 0",
                    height * selected.min(drawn.saturating_sub(1)) as f32 + 1.0
                ),
            );
            crate::input::set_field(bar, "Size", &format!("1 1 {} 0", height - 1.0));
            crate::input::set_field(bar, "Visible", if drawn == 0 { "false" } else { "true" });
        }
        Ok(())
    }
}

/// The fractured row's `Image`: the same fields a shipped image carries, with
/// the `teamhq\damage.tga` file `FUN_004d6480` loads.
fn icon_widget(name: &str, file: &str, x: f32, y: f32, w: f32, h: f32) -> Widget {
    use ra2fmt::gib::Field;
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: WidgetKind::Image,
        implicit: false,
        fields: vec![
            field("_Tiling", "false"),
            field("_Filename", file),
            field("_BufferWidth", "0 0 0 0"),
            field("_Size", &format!("{w} 0 {h} 0")),
            field("_Alpha", "true"),
            field("_Filtering", "0"),
            field("_ForeColor", "1 1 1 1"),
            field("_Name", ""),
            field("_Position", "0 0 0 0"),
            field("Name", name),
            field("ID", "-1"),
            field("Position", &format!("{x} 0 {y} 0")),
            field("Size", &format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "1"),
            field("ClipMode", "0"),
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
        ra2fmt::saves::isolate_for_tests();
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
        // Live (`event-repairs-damaged`) the list draws no row text at all: only the cell lines
        // (one above each row and one under the last), the left, middle and right edges.
        assert!(texts.is_empty(), "{texts:?}");
        let lines: Vec<&str> = list
            .children
            .iter()
            .map(|c| c.widget.name())
            .filter(|name| name.starts_with("row line"))
            .collect();
        assert_eq!(lines.len(), 3 + 3, "{lines:?}");
        let bar = list
            .children
            .iter()
            .find(|c| c.widget.name() == "Sel Bar")
            .unwrap();
        assert_eq!(
            bar.widget.field("Visible").map(|f| f.raw.as_str()),
            Some("true")
        );
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
        // No rows: no cell lines and no selection bar (`event-repairs-atom`).
        assert!(!list
            .children
            .iter()
            .any(|c| c.widget.name().starts_with("row line")));
        let bar = list
            .children
            .iter()
            .find(|c| c.widget.name() == "Sel Bar")
            .unwrap();
        assert_eq!(
            bar.widget.field("Visible").map(|f| f.raw.as_str()),
            Some("false")
        );
    }

    #[test]
    fn a_component_row_names_the_robot_class_hp_and_fractured_icon() {
        let Some(ui) = ui_root() else { return };
        let rows = vec![
            Row::component(
                "C.R.U.S.H.",
                "Bent Axle",
                "Axle",
                "29:00",
                0.03,
                40.0,
                100.0,
                false,
            ),
            Row::component(
                "C.R.U.S.H.",
                "Torn Spinner",
                "SpinMotor",
                "30:00",
                0.0,
                0.0,
                80.0,
                true,
            ),
        ];
        let screen = Repairs::open(&ui, "Round 1 Repairs", rows, "30:00").unwrap();
        assert_eq!(screen.rows().len(), 2);
        assert_eq!(screen.rows()[0].hp_percent(), 40);
        assert_eq!(screen.rows()[1].hp_percent(), 0);
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
        // The rows draw no text (live); the model keeps the names and the times.
        assert!(texts.is_empty(), "{texts:?}");
        assert_eq!(screen.rows()[0].time, "29:00");
        assert_eq!(screen.rows()[0].component, "Bent Axle");
        assert_eq!(screen.rows()[1].component, "Torn Spinner");
        // Exactly one row is fractured, and its shipped icon is in the list.
        let icons: Vec<&str> = list
            .children
            .iter()
            .filter(|c| c.widget.kind == WidgetKind::Image)
            .filter_map(|c| c.widget.filename())
            .collect();
        assert_eq!(icons, vec![FRACTURED_ICON]);
        let icon = list
            .children
            .iter()
            .find(|c| c.widget.name() == "repair row 1 icon")
            .expect("the fractured row's icon");
        assert_eq!(icon.widget.filename(), Some(FRACTURED_ICON));
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
