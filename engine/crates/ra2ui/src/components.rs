//! The component palette: the game's own component definitions, in the categories
//! the screen's own option group names.
//!
//! `botlab/components.gib`'s grid says what fills it — *"fill with components in
//! the current category, click to show preview"* — and the category is the
//! `category optiongroup`'s selected checkbox, whose own notes say what each one
//! shows: `power button` *"click to show power components in grid"*,
//! `mechanics button` *"click to show motors and pistons in grid"*,
//! `mobility button` *"click to show wheels and mobility in grid"*, and the same
//! for `extenders`, `weapons` and `extras`. Each note's word is the definition's
//! `type` field — `ztek2.txt` has `type = mechanics` — which is the field the
//! component factory switches on, so the palette and the sim agree by
//! construction about what a component *is* (`ra2sim::rig::category`).
//!
//! The definitions are the game's, inside `Components.cfz`: `name = Z-Tek Motor
//! (HP)`, `preview = ztek_preview.bmp`, `dir = ztek`, `type = mechanics`. A cell
//! is that preview art, **at its own size**, centred on the grid's cell: the art
//! is already cropped to one component (all 70 shipped previews are 96×72, with
//! their content inset about eight pixels, so 80 of the 96 columns carry the
//! icon), and the grid's cell is 78×72 with a 17-pixel column stride
//! (`ColWidths 78 17 78 17 78 17 78 17` — four columns inside a 320-pixel pane,
//! which is what rules the 17 out of being part of the layout: four columns at
//! 95 would be 363 pixels and would not fit). Drawing the art at its own size
//! with its icon centred therefore lands the 80-pixel icon in the 78-pixel cell,
//! and the 72-pixel height on the 72-pixel cell exactly.

use std::path::Path;

use ra2fmt::gib::{Field, Widget, WidgetKind};

use crate::Grid;

/// A component the palette can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    /// The definition's `name` — `Z-Tek Motor (HP)`.
    pub name: String,
    /// The definition's `type`, which is the palette's category.
    pub category: String,
    /// The preview art, as a path from the game's own root:
    /// `Components\ztek\ztek_preview.bmp`.
    pub art: String,
    /// The definition's own file in `Components.cfz` — the name a `.bot`
    /// writes in its component rows (`Components\ztek1.txt`).
    pub file: String,
}

/// The grid the palette fills, by name.
pub const GRID: &str = "components grid";
/// The option group whose selected member is the current category.
pub const GROUP: &str = "category optiongroup";

/// The palette's six categories, and the `category optiongroup` button that
/// selects each in `botlab/components.gib`.
pub const CATEGORIES: [(&str, &str); 6] = [
    ("power", "power button"),
    ("extenders", "extenders button"),
    ("mechanics", "mechanics button"),
    ("mobility", "mobility button"),
    ("weapons", "weapons button"),
    ("extras", "extras button"),
];

/// Every component the game ships, in archive order.
///
/// A definition counts when its `type` is one of the six, which is what leaves
/// the archives's style files out: they are definitions too, and they are not
/// components.
pub fn all(game: &Path) -> Result<Vec<Component>, String> {
    let cfl = ra2fmt::cfl::Cfl::open_path(&game.join("Components.cfz"))
        .map_err(|e| format!("Components.cfz: {e}"))?;
    let mut out = Vec::new();
    let mut order: Vec<i32> = Vec::new();
    for entry in cfl.entries() {
        if !entry.name.to_ascii_lowercase().ends_with(".txt") {
            continue;
        }
        let Some(text) = cfl.get_text(&entry.name) else {
            continue;
        };
        let def = ra2fmt::component::parse(&text);
        let Some(word) = def.get("type").and_then(|t| t.split_whitespace().next()) else {
            continue;
        };
        let category = word.to_ascii_lowercase();
        if !CATEGORIES.iter().any(|(name, _)| *name == category) || def.preview.is_empty() {
            continue;
        }
        // The palette lists the base definitions: a `hidden = 1` one (the Smart
        // Zone's size picker) and the `Styles\` variants it points at are
        // reached through the part's own style choice, not as palette cells.
        let hidden = def.get("hidden").is_some_and(|h| h.trim() == "1");
        if hidden
            || entry
                .name
                .replace('/', "\\")
                .to_ascii_lowercase()
                .contains("\\styles\\")
        {
            continue;
        }
        let sort = def
            .get("sort")
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(0);
        order.push(sort);
        out.push(Component {
            name: def.name.clone(),
            category,
            art: format!("Components\\{}\\{}", def.dir, def.preview),
            file: entry.name.clone(),
        });
    }
    // Palette order (capture, Power tab: control board, CO2 tank, large air
    // tank, Nifty, small pack, then SuperVolt): the definition's `sort` key
    // (the control board's is -10), then the file name, case-folded.
    let mut keyed: Vec<(i32, String, Component)> = out
        .into_iter()
        .zip(order)
        .map(|(c, sort)| (sort, c.file.to_ascii_lowercase(), c))
        .collect();
    keyed.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(keyed.into_iter().map(|(_, _, c)| c).collect())
}

/// The components one category holds.
pub fn in_category<'a>(all: &'a [Component], category: &str) -> Vec<&'a Component> {
    all.iter().filter(|c| c.category == category).collect()
}

/// A component's preview art in the game's tree, matched the way the game's own
/// filesystem does — `universalT/T_preview.bmp` is on disk as `t_preview.bmp`.
pub fn art_path(game: &Path, component: &Component) -> Option<std::path::PathBuf> {
    ra2fmt::find_ci(game, &component.art.replace('\\', "/"))
}

/// The pixel size a 24-bit BMP states in its header.
pub fn bmp_size(path: &Path) -> Option<[f32; 2]> {
    let header = std::fs::read(path).ok()?;
    if header.len() < 26 {
        return None;
    }
    let number = |at: usize| {
        let bytes: [u8; 4] = header.get(at..at + 4)?.try_into().ok()?;
        Some(i32::from_le_bytes(bytes) as f32)
    };
    Some([number(18)?, number(22)?])
}

/// The category the screen has selected: the `category optiongroup`'s own
/// `CheckBox` whose `Checked` is true.
///
/// The file bakes one, and a click on a button moves it ([`crate::input`]'s
/// `set_state` writes `Checked` and shows the checkbox's `len - 2` state image,
/// which for these six is the `co_*_s.bmp` art).
pub fn selected_category(root: &ra2fmt::gib::Widget) -> Option<(&'static str, &'static str)> {
    let group = crate::workshop::find(root, GROUP)?;
    CATEGORIES.iter().copied().find(|(_, button)| {
        crate::workshop::find(group, button)
            .and_then(|w| crate::input::field_of(w, "Checked"))
            .is_some_and(|checked| checked.eq_ignore_ascii_case("true"))
    })
}

/// Fill the palette for the category the screen has selected, and say which one
/// that was and how many cells it took.
///
/// This is the whole of the grid's own instruction — *"fill with components in
/// the current category"* — in one call: the screen says which category it is in
/// the option group, the definitions say which components are in it, and the grid
/// says where the cells go.
pub fn fill_selected(
    root: &mut ra2fmt::gib::Widget,
    game: &Path,
    all: &[Component],
) -> Option<(&'static str, usize)> {
    // Read the selection before borrowing the tree to write it: the option group
    // is the *same* widget tree the grid is in.
    let (category, _) = selected_category(root)?;
    let shown = in_category(all, category);
    let grid = crate::workshop::find_mut(root, GRID)?;
    let filled = fill_grid(grid, game, &shown);
    Some((category, filled))
}

/// Fill a palette grid with one cell per component.
///
/// The cell is the preview art at its own size, centred on the grid's cell, and
/// placed with the grid's own geometry: `Rows 1, Columns 4, RowHeight 72` mean a
/// one-row window of four — the file's `PermScrolls true` is what says the rest
/// scroll past it — with the art of column `n` at `n` times the column width.
///
/// Returns how many cells were written, which is the number whose art was found.
pub fn fill_grid(grid: &mut Widget, game: &Path, components: &[&Component]) -> usize {
    let Some(geometry) = Grid::from_widget(grid) else {
        return 0;
    };
    // A refill replaces the cells: they are the grid's children that are images
    // of a component's art, which is what leaves the file's own two selection
    // bars alone.
    grid.children.retain(|c| {
        !(c.kind == WidgetKind::Image && c.filename().is_some_and(|f| f.starts_with("Components")))
    });
    let columns = geometry.columns.max(1);
    let cell_width = geometry.col_widths.first().copied().unwrap_or(0.0);
    let mut made = 0;
    for (i, component) in components.iter().enumerate() {
        let Some([art_width, art_height]) = art_path(game, component).and_then(|p| bmp_size(&p))
        else {
            continue;
        };
        // `Rows 1` under a horizontal `Scroller` is one long row the bar scrolls
        // (the capture's Power tab runs five cells to the panel's edge).
        let one_row = geometry.rows == 1
            && grid
                .attachments
                .iter()
                .any(|a| a.kind == ra2fmt::gib::AttachmentKind::Scroller);
        let (row, column) = if one_row {
            (0, i as i32)
        } else {
            ((i as i32) / columns, (i as i32) % columns)
        };
        // Centred on the cell, which is where the art's own inset content lands
        // in it.
        let (x, y) = if one_row {
            // The capture's cells sit edge to edge at the art's own width
            // (96 px, 1 px black rules between), not at `ColWidths`' 78.
            (
                art_width * column as f32,
                -(art_height - geometry.row_height) / 2.0,
            )
        } else {
            (
                geometry.column_x(column) - (art_width - cell_width) / 2.0,
                geometry.row_height * row as f32 - (art_height - geometry.row_height) / 2.0,
            )
        };
        let mut cell = image_widget();
        crate::input::set_field(&mut cell, "Position", &format!("{x} 0 {y} 0"));
        crate::input::set_field(&mut cell, "Size", &format!("{art_width} 0 {art_height} 0"));
        crate::input::set_field(&mut cell, "_Filename", &component.art);
        if one_row {
            // Capture: unselected cells are the art at 0.6 grey ((155,155,155)
            // over the white), the selected one full white, with a black rule.
            let tint = if i == 0 { "1 1 1 1" } else { "0.6 0.6 0.6 1" };
            crate::input::set_field(&mut cell, "_ForeColor", tint);
            // In front of the grid's own selection bar (`ChildOrder` 0): the
            // capture shows only the bar's under-line past the cell's edge.
            crate::input::set_field(&mut cell, "ChildOrder", "10");
            crate::cells::add_border(grid, &mut cell, 1.0);
        }
        grid.children.push(cell);
        made += 1;
    }
    made
}

/// An empty `Image` record, with the base fields every widget carries — a cell
/// is made, not cloned, because the grid's cells are not in the file.
fn image_widget() -> Widget {
    let field = |name: &str, raw: &str| Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    Widget {
        kind: WidgetKind::Image,
        implicit: false,
        fields: vec![
            field("Name", ""),
            field("ID", "-1"),
            field("Position", "0 0 0 0"),
            field("Size", "1 1 1 1"),
            field("Visible", "true"),
            field("ChildOrder", "-1"),
            field("ClipMode", "1"),
            field("NumChildren", "0"),
            field("_Filename", ""),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::gib::Block;

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

    /// The palette is the game's own definitions: every one classified, every
    /// preview on disk, and every preview the same 96×72 tile.
    #[test]
    fn the_palette_is_the_games_own_components() {
        let Some(game) = game_dir() else { return };
        let all = all(&game).unwrap();
        // 131 definitions in the archive; the palette lists the 70 base ones: the
        // 61 `Styles\` variants and `hidden = 1` size pickers are reached through
        // a part's own style choice (capture: the Power tab runs control board,
        // CO2 tank, large air tank, Nifty, small pack, SuperVolt, no Smart Zone).
        assert_eq!(all.len(), 70, "the base components the palette lists");
        for component in &all {
            assert!(
                CATEGORIES.iter().any(|(c, _)| *c == component.category),
                "{}: category {:?}",
                component.name,
                component.category
            );
            let path = art_path(&game, component)
                .unwrap_or_else(|| panic!("{}: no art at {}", component.name, component.art));
            assert_eq!(bmp_size(&path), Some([96.0, 72.0]), "{}", component.name);
        }
        // The names are the definitions' own, not the directory names.
        assert!(all.iter().any(|c| c.name.starts_with("Z-Tek Motor")));
        // One cell per preview picture: the 70 shipped 96x72 previews.
        let mut arts: Vec<&str> = all.iter().map(|c| c.art.as_str()).collect();
        arts.sort_unstable();
        arts.dedup();
        assert_eq!(
            arts.len(),
            all.len(),
            "no two palette cells share a picture"
        );
    }

    /// The six categories are a partition: every component is in one, and the
    /// parts add up to the whole.
    #[test]
    fn every_component_is_in_exactly_one_category() {
        let Some(game) = game_dir() else { return };
        let all = all(&game).unwrap();
        // The screen's own tagged buttons, once.
        let ui = game.join("UI");
        let document = std::fs::read_to_string(crate::project::find(&ui).unwrap()).unwrap();
        let tagged: Vec<String> = crate::project::parse(&document)
            .get("botlab/components.gib")
            .map(|f| f.children.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();

        let mut total = 0;
        for (category, button) in CATEGORIES {
            let in_category = in_category(&all, category);
            println!("  {category:10} {:3} components", in_category.len());
            assert!(!in_category.is_empty(), "{category} is empty");
            total += in_category.len();
            assert!(tagged.iter().any(|n| n == button), "{button} is tagged");
        }
        assert_eq!(total, all.len(), "the six parts are the whole");
    }

    /// The screen bakes its own category — `power button`'s `Checked` is true —
    /// and a click moves it, after which the palette is the other category's.
    #[test]
    fn the_palette_follows_the_screens_own_selection() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let all = all(&game).unwrap();
        let mut gib = crate::workshop::load(&ui, "botlab/components.gib").unwrap();
        let root = gib
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();

        // The file's own selection, and the count it fills.
        assert_eq!(
            selected_category(root),
            Some(("power", "power button")),
            "the screen loads with power selected"
        );
        let (category, filled) = fill_selected(root, &game, &all).unwrap();
        assert_eq!(category, "power");
        assert_eq!(filled, in_category(&all, "power").len());
        let bars = crate::workshop::find(root, GRID).unwrap().children.len() - filled;
        assert_eq!(bars, 2, "the grid's own two selection bars");

        // A click on another category moves the selection, and a refill is that
        // category's — with the old cells gone, not added to.
        crate::input::select_option(root, GROUP, "mechanics button");
        assert_eq!(
            selected_category(root),
            Some(("mechanics", "mechanics button"))
        );
        let (category, filled) = fill_selected(root, &game, &all).unwrap();
        assert_eq!(category, "mechanics");
        assert_eq!(filled, 15, "mechanics is the base motors and pistons");
        assert_eq!(
            crate::workshop::find(root, GRID).unwrap().children.len(),
            bars + filled,
            "a refill replaces the cells"
        );
    }

    /// Filling `botlab/components.gib`'s own grid puts a cell per component at
    /// the grid's geometry: the art at its own size, centred on the cell, one
    /// column stride apart.
    #[test]
    fn the_grid_takes_a_cell_per_component_at_the_grids_geometry() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut gib = crate::workshop::load(&ui, "botlab/components.gib").unwrap();
        let root = gib
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let grid = crate::workshop::find_mut(root, "components grid").expect("the palette grid");
        assert!(grid
            .notes()
            .contains("fill with components in the current category"));

        let all = all(&game).unwrap();
        let power = in_category(&all, "power");
        let bars = grid.children.len();
        assert_eq!(bars, 2, "the grid's own two selection bars");
        let made = fill_grid(grid, &game, &power);
        assert_eq!(made, power.len());

        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == "components grid")
            .unwrap();
        assert_eq!(grid.children.len(), bars + power.len());
        for (i, cell) in grid.children.iter().skip(bars).enumerate() {
            // `Rows 1` under a horizontal scroller is one row: each cell is
            // 96 wide and 72 tall, edge to edge (capture: black rules every
            // 96 px from the grid's left edge).
            assert_eq!(cell.frame.w, 96.0, "cell {i}");
            assert_eq!(cell.frame.h, 72.0, "cell {i}");
            assert_eq!(cell.frame.x, grid.frame.x + 96.0 * i as f32, "cell {i}");
            assert_eq!(cell.frame.y, grid.frame.y, "cell {i}");
        }
    }
}
