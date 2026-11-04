//! Cells: the widgets a list repeats, and what a row binds into them.
//!
//! A `Grid` or `ListBox` in the shell is a list of *cells*, and a cell is a
//! fragment the data names. `armor.gib`'s grid says so in its own `Notes` —
//! *"load this grid with the four armor cells"* — and the same goes for the
//! others: `hq_league.gib`'s *"fill with league_cell.gib with the teams in order
//! of rank"*, `hq_events.gib`'s *"fill with this season's events
//! (event_cell.gib)"*, `hq_robots.gib`'s *"fill grid with team robots
//! (robot_cell.gib)"*, `select_team.gib`'s *"load team_cell.gib's into these grid
//! cells for each team"*, `season_results.gib`'s *"fill with sresults_cell.gib
//! just like league standings"*.
//!
//! A four-row grid is not a `ListBox` with four children in the file: the cells
//! are *made*, one per row, at the geometry the list itself states. `armor.gib`
//! is `Rows 4`, `Columns 1`, `RowHeight 84`, `ColWidths 300 17`, so its four cells
//! are 300 by 84 at y = 0, 84, 168 and 252 — and that is what [`fill`] writes into
//! each instance's own `Position`/`Size`, since a cell template's root is
//! `Size 1 1 1 1` (it is written to be *placed*, not to know where it goes).
//!
//! ## What a row binds, and how the template says it
//!
//! A template names its binding points in its own `Notes`, and they are its
//! children's names:
//!
//! | template | its notes | the children they name |
//! |---|---|---|
//! | `armor_cell.gib` | *"'armor img' child should be the preview for each armor type"*, *"armor description displayed here"*, *"show weight value here"* | `armor img`, the description, `weight value` |
//! | `bot_cell.gib` | *"load bot image into 'bot snapshot img' child of this group"*, *"robot name here"*, *"'Lightweight' or 'Heavyweight' here"* | `bot snapshot img`, the name, the class |
//! | `result_cell.gib` | *"set 'team logo' and 'bot preview' images"*, *"display the bot's total points here"* | `team logo`, `bot preview`, the points |
//!
//! [`bind_art`] writes a row's file into a child by name. Prose is not named: the
//! template's own `Glyph` *is* the prose — `armor_cell.gib` has exactly one, and
//! it carries the text twice, as `_Text` and inside its `DisplayBox`'s markup
//! (`<text font fg_mcn14 color .8 .8 .8 1>Plastic armor is …</text>`) — so
//! [`bind_prose`] writes both and leaves the markup's font and colour alone.

use ra2fmt::gib::{Attachment, Gib, Widget, WidgetKind};

use crate::Grid;

/// Read a cell template out of a game's `UI` directory.
pub fn template(ui_root: &std::path::Path, relative: &str) -> Result<Gib, String> {
    crate::workshop::load(ui_root, relative)
}

/// Fill a grid or list with `cells` copies of a template, one per cell of the
/// list's own `Grid`, in row-major order.
///
/// The geometry is the list's: `Columns` decides how many go in a row — the
/// armour grid is `Columns 1` and so is one per row, the event grid is
/// `Columns 3` — and `RowHeight` with `ColWidths` is the cell's own size.
/// Returns how many cells were filled, nothing when the list states no
/// dimensions.
pub fn fill(list: &mut Widget, template: &Gib, cells: usize) -> usize {
    let Some(grid) = Grid::from_widget(list) else {
        return 0;
    };
    let columns = grid.columns.max(1) as usize;
    let width = grid.col_widths.first().copied().unwrap_or(0.0);
    let roots: Vec<Widget> = template.widgets().cloned().collect();
    let full_width = columns == 1
        && grid.buffer_width == 0.0
        && list
            .attachments
            .iter()
            .any(|a| a.kind == ra2fmt::gib::AttachmentKind::Scroller);
    for index in 0..cells {
        let (row, column) = ((index / columns) as i32, (index % columns) as i32);
        let x = grid.column_x(column);
        let y = grid.row_height * row as f32;
        for root in &roots {
            let mut cell = root.clone();
            // `BufferWidth` insets the cell inside its slot; the border is drawn
            // around the inset frame.
            let b = grid.buffer_width + grid.border_width;
            crate::input::set_field(&mut cell, "Position", &format!("{} 0 {} 0", x + b, y + b));
            // A one-column grid inside a `Scroller` runs its rows the whole way
            // to the bar (the standings capture: the cell rules reach the
            // scroll bar although `ColWidths` says 388 of 630 px).
            let size = if full_width {
                format!("1 1 {} 0", grid.row_height - 2.0 * b)
            } else {
                format!("{} 0 {} 0", width - 2.0 * b, grid.row_height - 2.0 * b)
            };
            crate::input::set_field(&mut cell, "Size", &size);
            add_border(list, &mut cell, grid.border_width);
            // The list's cell lines are a bar along the top and one down the left of every slot; a
            // neighbour's top is this cell's bottom (docs/34 11.12).
            if let Some(border) = cell
                .attachments
                .last_mut()
                .filter(|_| grid.border_width > 0.0)
            {
                border.fields.push(ra2fmt::gib::Field {
                    name: Some("Sides".to_string()),
                    raw: "top left".to_string(),
                });
            }
            list.children.push(cell);
        }
    }
    cells
}

/// Put a hand-made cell into the slot at (`x`, `y`) of `w` x `h` pixels of a grid, the way [`fill`] does: the
/// cell is inset by the grid's `BufferWidth` + `BorderWidth` and dressed with the cell lines (the picker's
/// `robots grid`, which builds its cells itself).
pub fn place_in_slot(list: &Widget, cell: &mut Widget, x: f32, y: f32, w: f32, h: f32) {
    let Some(grid) = Grid::from_widget(list) else {
        return;
    };
    let b = grid.buffer_width + grid.border_width;
    crate::input::set_field(cell, "Position", &format!("{} 0 {} 0", x + b, y + b));
    crate::input::set_field(
        cell,
        "Size",
        &format!("{} 0 {} 0", w - 2.0 * b, h - 2.0 * b),
    );
    add_border(list, cell, grid.border_width);
    if let Some(border) = cell
        .attachments
        .last_mut()
        .filter(|_| grid.border_width > 0.0)
    {
        border.fields.push(ra2fmt::gib::Field {
            name: Some("Sides".to_string()),
            raw: "top left".to_string(),
        });
    }
}

/// Dress a cell with the list's own `BorderColor` / `BorderWidth` ring (the grid
/// lines the original draws around each cell). Does nothing for width 0.
pub fn add_border(list: &Widget, cell: &mut Widget, width: f32) {
    if width <= 0.0 {
        return;
    }
    let color = list
        .field("BorderColor")
        .map_or("0 0 0 1", |f| f.raw.trim())
        .to_string();
    let field = |name: &str, raw: String| ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw,
    };
    cell.attachments.push(Attachment {
        kind: ra2fmt::gib::AttachmentKind::Border,
        fields: vec![
            field("Width", format!("{width}")),
            field("Inner", color.clone()),
            field("Outer", color),
        ],
        body: None,
    });
}

/// Write a row's art file into the child the template names.
///
/// The file replaces `_Filename` alone: a cell's art is the whole file (the
/// renderer samples a cell `0..1`, the rect only says whether it *is* a cell).
pub fn bind_art(root: &mut Widget, child: &str, file: &str) -> bool {
    let Some(widget) = crate::workshop::find_mut(root, child) else {
        return false;
    };
    crate::input::set_field(widget, "_Filename", file);
    true
}

/// Write a row's number into the child the template names — a `TextBox` holds
/// its value in its own `Text` child, so that is where it goes.
pub fn bind_value(root: &mut Widget, child: &str, value: &str) -> bool {
    let Some(widget) = crate::workshop::find_mut(root, child) else {
        return false;
    };
    if let Some(text) = widget
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Text)
    {
        crate::input::set_field(text, "_Text", value);
        return true;
    }
    crate::input::set_field(widget, "_Text", value);
    true
}

/// Write a row's prose into the template's own `Glyph`: its `_Text`, and the
/// text inside its `DisplayBox`'s markup, leaving the markup's own attributes.
pub fn bind_prose(root: &mut Widget, text: &str) -> bool {
    let Some(glyph) = find_kind_mut(root, &WidgetKind::Glyph) else {
        return false;
    };
    crate::input::set_field(glyph, "_Text", text);
    if let Some(display) = glyph
        .attachments
        .iter_mut()
        .find(|a| a.kind == ra2fmt::gib::AttachmentKind::DisplayBox)
    {
        if let Some(field) = display.field_mut("Display") {
            field.raw = markup_with_text(&field.raw, text);
        }
    }
    true
}

/// The markup a `DisplayBox` carries, with its text replaced and its own
/// attributes — `<text font fg_mcn14 color .8 .8 .8 1>` — left as they are.
pub fn markup_with_text(markup: &str, text: &str) -> String {
    let Some(open) = markup.find("<text") else {
        return text.to_string();
    };
    let Some(tag_end) = markup[open..].find('>').map(|i| open + i + 1) else {
        return text.to_string();
    };
    match markup[tag_end..].find("</text>") {
        Some(close) => format!(
            "{}{}{}",
            &markup[..tag_end],
            text,
            &markup[tag_end + close..]
        ),
        None => format!("{}{}", &markup[..tag_end], text),
    }
}

fn find_kind_mut<'a>(widget: &'a mut Widget, kind: &WidgetKind) -> Option<&'a mut Widget> {
    if &widget.kind == kind {
        return Some(widget);
    }
    widget
        .children
        .iter_mut()
        .find_map(|c| find_kind_mut(c, kind))
}

/// The `DisplayBox` an attachment list holds, if any — for a reader that wants
/// the markup rather than the plain text.
pub fn display_box(attachments: &[Attachment]) -> Option<&Attachment> {
    attachments
        .iter()
        .find(|a| a.kind == ra2fmt::gib::AttachmentKind::DisplayBox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve, DESIGN_SCREEN};
    use ra2fmt::gib::Block;

    fn ui_root() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game/UI");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn game_dir() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
        ui_root().and_then(|ui| ui.parent().map(|p| p.to_path_buf()))
    }

    /// The armour grid's own `Notes` says four cells, and the four armour types
    /// ship in `RA2.cfz` — so the grid is filled from the game's data, not from
    /// a list written here.
    #[test]
    fn the_armour_grid_is_filled_from_the_shipped_armour_table() {
        let Some(ui) = ui_root() else { return };
        let Some(game) = game_dir() else { return };
        let mut gib = crate::workshop::load(&ui, "botlab/armor.gib").unwrap();
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            Block::Widget(w) => Some(w),
            _ => None,
        });
        let root = root.unwrap();
        let grid = crate::workshop::find_mut(root, "armor grid").expect("the armour grid");
        assert!(
            grid.notes().contains("four armor cells"),
            "the grid's note: {:?}",
            grid.notes()
        );

        // The armour table, read from the archive the same way the sim reads it.
        let cfl = ra2fmt::cfl::Cfl::open_path(&game.join("RA2.cfz")).unwrap();
        let text = cfl.get("Armor_Definitions.txt").expect("the armour table");
        let armour = ra2fmt::armor::parse(std::str::from_utf8(&text).unwrap());
        assert_eq!(armour.len(), 4, "four armour types");

        let template = template(&ui, "botlab/armor_cell.gib").unwrap();
        assert_eq!(fill(grid, &template, armour.len()), 4);
        // The grid's own children are its two selection bars (`Sel Bar` and
        // `High Bar`, the `ID`s its `Selection`/`Highlight` name); the four
        // filled cells follow them, one per row.
        let bars = grid.children.len() - armour.len();
        assert_eq!(bars, 2, "the file's own bars");
        let mut cells = grid.children[bars..].iter_mut().collect::<Vec<_>>();
        for (cell, def) in cells.iter_mut().zip(&armour) {
            assert!(bind_art(cell, "armor img", &def.texture));
            assert!(bind_prose(cell, &def.description));
            assert!(bind_value(
                cell,
                "weight value",
                &format!("{}", def.weight as i32)
            ));
        }

        // Placed, the four cells are one row height apart and as wide as the
        // grid's first column: `Rows 4`, `RowHeight 84`, `ColWidths 300 17`.
        let placed = resolve(&gib, DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "armor grid")
            .unwrap();
        assert_eq!(grid.children.len(), 6, "two bars and four cells");
        for (row, cell) in grid.children.iter().skip(2).enumerate() {
            // `BorderWidth 1` insets the cell inside its slot (live standings and join window).
            let y = grid.frame.y + 84.0 * row as f32 + 1.0;
            assert_eq!(cell.frame.y, y, "row {row}");
            assert_eq!(cell.frame.x, grid.frame.x + 1.0, "row {row} is one column");
            // A scroller-wrapped single column runs to the grid's own width
            // (the file's `ColWidths 300` is the editor's canvas).
            assert_eq!(cell.frame.w, grid.frame.w);
            assert_eq!(cell.frame.h, 82.0);
        }

        // The bindings reached the widgets the notes name.
        let first = &grid.children[2];
        let art = first
            .walk()
            .find(|p| p.widget.name() == "armor img")
            .expect("armor img");
        assert_eq!(
            art.widget.filename(),
            Some("botlab\\armor_plastic.tga"),
            "the first armour type's texture"
        );
        let glyph = first
            .walk()
            .find(|p| p.widget.kind == WidgetKind::Glyph)
            .expect("the description glyph");
        assert_eq!(glyph.widget.text(), Some(armour[0].description.as_str()));
        let display = display_box(&glyph.widget.attachments).expect("its DisplayBox");
        // The markup keeps its own attributes and takes the new text.
        let raw = display.field("Display").unwrap().text();
        assert!(
            raw.starts_with("<text font fg_mcn14 color .8 .8 .8 1>"),
            "{raw}"
        );
        assert!(raw.contains(&armour[0].description));
        assert!(raw.ends_with("</text>"), "{raw}");
    }

    /// Binding a cell leaves the markup's attributes alone and replaces only the
    /// text between them.
    #[test]
    fn the_markup_keeps_its_attributes() {
        assert_eq!(
            markup_with_text("<text font fg_mcn14 color .8 .8 .8 1>old</text>", "new"),
            "<text font fg_mcn14 color .8 .8 .8 1>new</text>"
        );
        // No markup at all: the text is the whole value.
        assert_eq!(markup_with_text("old", "new"), "new");
        // An unterminated tag still gets the text.
        assert_eq!(
            markup_with_text("<text font fg_x>old", "new"),
            "<text font fg_x>new"
        );
    }

    /// `fill` on a list that states no dimensions writes nothing rather than
    /// guessing a geometry.
    #[test]
    fn a_list_without_dimensions_is_left_alone() {
        let w = Widget {
            kind: WidgetKind::ListBox,
            implicit: false,
            fields: Vec::new(),
            attachments: Vec::new(),
            children: Vec::new(),
        };
        let Some(ui) = ui_root() else { return };
        let template = template(&ui, "botlab/armor_cell.gib").unwrap();
        let mut list = w;
        assert_eq!(fill(&mut list, &template, 4), 0);
        assert!(list.children.is_empty());
    }

    /// `hq_robots.gib`'s own note names `robot_cell.gib`; that file does not
    /// ship, and the data's cell is `bot_cell.gib`. The data wins, and this test
    /// names the difference rather than smoothing the note over.
    #[test]
    fn the_robot_grid_note_names_a_file_the_game_does_not_ship() {
        let Some(ui) = ui_root() else { return };
        let mut gib = crate::workshop::load(&ui, "teamhq/hq_robots.gib").unwrap();
        let root = gib
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let grid = crate::workshop::find_mut(root, "robot grid").expect("the robot grid");
        assert!(
            grid.notes().contains("robot_cell.gib"),
            "the grid's note: {:?}",
            grid.notes()
        );
        assert!(
            template(&ui, "teamhq/robot_cell.gib").is_err(),
            "the note's template does not ship"
        );
        assert!(
            template(&ui, "teamhq/bot_cell.gib").is_ok(),
            "the cell the fill uses (FUN_004ff570) does ship"
        );
    }
}
