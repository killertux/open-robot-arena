//! The robot controls reminder (`F1` in a match).
//!
//! Recovered from the match window's key handler `0x475ee0`: key `0x70`
//! (`F1`), when the arena intro is not playing (`+0x1d4` clear), finds or
//! builds the `controls reminder` window (`KeyControlRemindWindow`, 0x174
//! bytes, constructor `FUN_004828b0` on `common\controls_reminder.gib`, which
//! binds `control grid` and `bot name`), picks the first robot whose
//! "is local" virtual (`+0x18`) is true, fills the window for it
//! (`FUN_00482b20` -> `FUN_00482b60`: the robot's name into `bot name`, the
//! grid set to 3 x 3 cells (`FUN_005fd510(3, 3)`), one `botlab\control_cell.gib`
//! per control of the robot's board (`FUN_00482bd0`, the same filler as the
//! workshop's wiring tab: `control name`, the keys joined with `" - "` in
//! `control input`, `control img`) at the control's own cell), and shows it
//! modally (`FUN_005e5620`). Outside a network game the match is paused for as
//! long as the window is up (`FUN_004190f0(1)` before, `(0)` after).
//!
//! The window's own key handler (`0x482a10`, vtable `0x6a7f74` slot 63):
//! `Esc` closes it (result 1); `F1` moves on to the next local robot not
//! shown yet (the window keeps the shown ones in a list at `+0x160`) and
//! closes it when there is none, so with one player robot `F1` toggles it.

use std::path::Path;

use ra2fmt::gib::{Block, Gib, Widget};
use ra2ui::workshop::wiring::{ControlBoard, ControlKind};

/// The reminder window's file.
pub const SCREEN: &str = "common/controls_reminder.gib";
/// The control cell the grid is filled with (shared with the wiring tab).
pub const CELL: &str = "botlab/control_cell.gib";
/// The board is three cells across and three down (`FUN_005fd510(3, 3)`).
pub const GRID: [i32; 2] = [3, 3];

/// The window's pieces for one robot: the shipped window, the cell template,
/// and the robot's name and board.
#[derive(Clone)]
pub struct Reminder {
    window: Gib,
    template: Gib,
    bot_name: String,
    board: ControlBoard,
}

impl Reminder {
    /// The reminder for the `.bot` at `bot_path` (absolute, or relative to the
    /// install).
    pub fn open(game: &Path, bot_path: &str) -> Result<Reminder, String> {
        let ui = game.join("UI");
        let window = ra2ui::workshop::load(&ui, SCREEN)?;
        let template = ra2ui::cells::template(&ui, CELL)?;
        let path = {
            let p = Path::new(bot_path);
            if p.is_absolute() || p.is_file() {
                p.to_path_buf()
            } else {
                game.join(p)
            }
        };
        let data = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let bot = ra2fmt::bot::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
        let board =
            ControlBoard::from_bot(&bot).map_err(|e| format!("{}: {e:?}", path.display()))?;
        Ok(Reminder {
            window,
            template,
            bot_name: bot.name.clone(),
            board,
        })
    }

    pub fn bot_name(&self) -> &str {
        &self.bot_name
    }

    pub fn controls(&self) -> usize {
        self.board.controls.len()
    }

    /// The window filled for a screen of `layout` pixels: the cells are sized
    /// from the laid-out `control grid` (a third of it each way).
    pub fn filled(&self, layout: [f32; 2]) -> Gib {
        let mut gib = self.window.clone();
        let grid_frame = ra2ui::resolve(&gib, layout)
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "control grid")
            .map(|p| p.frame);
        let (cell_w, cell_h) = grid_frame
            // Whole pixels: the capture's lines are exactly 83 px apart.
            .map(|f| {
                (
                    (f.w / GRID[0] as f32).floor(),
                    (f.h / GRID[1] as f32).floor(),
                )
            })
            .unwrap_or((62.0, 70.0));
        for block in gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            if let Some(name) = ra2ui::workshop::find_mut(root, "bot name") {
                set_text(name, &self.bot_name);
            }
            let Some(grid) = ra2ui::workshop::find_mut(root, "control grid") else {
                continue;
            };
            // Nothing is selected or highlighted in the reminder.
            for bar in grid
                .children
                .iter_mut()
                .filter(|c| c.name() == "Sel Bar" || c.name() == "High Bar")
            {
                ra2ui::input::set_field(bar, "Visible", "false");
            }
            // The grid draws its own cell lines (`BorderColor 0.4 0.4 0.4 1`,
            // `BorderWidth 1`, `BufferWidth 1`) around all nine cells, empty
            // ones included (capture `match-f1`).
            for row in 0..GRID[1] {
                for column in 0..GRID[0] {
                    // One pixel line between neighbours, not two: the ring is
                    // drawn outside the frame, so the frame spans from one
                    // line to the next (capture: lines at x 388, 471, 554,
                    // 637, a 83 px pitch).
                    let mut cell = empty_cell(
                        cell_w * column as f32 + 1.0,
                        cell_h * row as f32 + 1.0,
                        cell_w - 1.0,
                        cell_h - 1.0,
                    );
                    ra2ui::cells::add_border(grid, &mut cell, 1.0);
                    grid.children.push(cell);
                }
            }
            for control in &self.board.controls {
                // `BufferWidth 1`: a control's cell sits one pixel inside the
                // line (the capture's key bars are 80 px wide in an 83 px
                // pitch, one grey pixel either side).
                let x = cell_w * control.cell[0] as f32 + 2.0;
                let y = cell_h * control.cell[1] as f32 + 1.0;
                let (cell_w, cell_h) = (cell_w - 3.0, cell_h - 3.0);
                for template in self.template.widgets() {
                    let mut cell = template.clone();
                    ra2ui::input::set_field(
                        &mut cell,
                        "Name",
                        &format!("reminder:ctl:{}:{}", control.cell[0], control.cell[1]),
                    );
                    ra2ui::input::set_field(&mut cell, "Position", &format!("{x} 0 {y} 0"));
                    ra2ui::input::set_field(&mut cell, "Size", &format!("{cell_w} 0 {cell_h} 0"));
                    let art = match control.kind {
                        ControlKind::Analog => "botlab\\ctrl_analog.bmp",
                        ControlKind::Switch => "botlab\\ctrl_switch.bmp",
                        ControlKind::Button => "botlab\\ctrl_button.bmp",
                    };
                    ra2ui::cells::bind_art(&mut cell, "control img", art);
                    ra2ui::cells::bind_value(&mut cell, "control name", &control.name);
                    ra2ui::cells::bind_value(&mut cell, "control input", &control.key_label());
                    grid.children.push(cell);
                }
            }
        }
        gib
    }
}

/// A cell with nothing in it: only the grid's ring is drawn around it.
fn empty_cell(x: f32, y: f32, w: f32, h: f32) -> Widget {
    let field = |name: &str, raw: &str| ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: ra2fmt::gib::WidgetKind::Rectangle,
        implicit: false,
        fields: vec![
            field("Filled", "false"),
            field("FillColor", "0 0 0 0"),
            field("BorderWidth", "0"),
            field("BorderColor", "0 0 0 0"),
            field("Name", "reminder:empty"),
            field("ID", "-1"),
            field("Position", &format!("{x} 0 {y} 0")),
            field("Size", &format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "2"),
            field("ClipMode", "0"),
            field("NumChildren", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

/// Write a text box's `Text` child (the first `_Text` under it).
fn set_text(widget: &mut Widget, text: &str) {
    for child in widget.children.iter_mut() {
        if child.kind == ra2fmt::gib::WidgetKind::Text {
            ra2ui::input::set_field(child, "_Text", text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        crate::find_game(None).ok()
    }

    /// The reminder lists the player robot's own controls, one shipped
    /// control cell each, and names the robot.
    #[test]
    fn the_reminder_lists_the_robots_controls() {
        let Some(game) = game() else { return };
        let reminder = Reminder::open(&game, "Teams/Team0/Bot0.bot").unwrap();
        assert!(reminder.controls() > 0);
        let gib = reminder.filled([1280.0, 1024.0]);
        let root = gib.widgets().next().unwrap();
        assert_eq!(root.name(), "controls reminder");
        let grid = ra2ui::workshop::find(root, "control grid").unwrap();
        let cells = grid
            .children
            .iter()
            .filter(|c| c.name().starts_with("reminder:ctl:"))
            .count();
        assert_eq!(cells, reminder.controls());
        let name = ra2ui::workshop::find(root, "bot name").unwrap();
        let text = name
            .children
            .iter()
            .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
            .and_then(|c| c.field("_Text"))
            .map(|f| f.raw.trim().to_string());
        assert_eq!(text.as_deref(), Some(reminder.bot_name()));
    }
}
