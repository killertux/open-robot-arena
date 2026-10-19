//! The event list as `hq_events.gib` shows it.
//!
//! The screen's own grid says what fills it — *"fill with this season's events
//! (event_cell.gib)"* — and the template says what a cell is: `event_cell.gib`'s
//! only note is on its first child, *"'preview img' child is the event preview
//! image"*, and its `xout` image is the overlay the game shows over an event that
//! has not been entered. So a cell is one event's art.
//!
//! The events are the game's `Events.txt` ([`ra2fmt::event`]): twelve of them,
//! each with its own art under `UI\teamhq\events\`. Which of the twelve a season
//! holds is the save's business, not the shell's — the shell's job is to draw the
//! list it is given, and the list the game ships is the one this can draw.

use std::path::Path;

use ra2fmt::gib::{Gib, Widget};

/// The grid, by name.
pub const GRID: &str = "event grid";
/// The cell template, from the grid's own note.
pub const TEMPLATE: &str = "teamhq/event_cell.gib";
/// The child of a cell that takes the event's art, by the name the template's own
/// note gives it.
pub const ART: &str = "preview img";

/// The events, in the game's own order.
pub fn all(game: &Path) -> Result<Vec<ra2fmt::event::Event>, String> {
    ra2fmt::event::parse(&ra2fmt::event::read(&game.join("Events.txt"))?)
}

/// Fill an event grid with one cell per event, at the grid's own geometry.
pub fn fill_grid(
    grid: &mut Widget,
    game: &Path,
    template: &Gib,
    events: &[ra2fmt::event::Event],
) -> usize {
    let made = crate::cells::fill(grid, template, events.len());
    let bars = grid.children.len() - made;
    for (cell, event) in grid.children[bars..].iter_mut().zip(events) {
        crate::cells::bind_art(cell, ART, &event.art);
        let _ = game;
    }
    made
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

    /// Filling `hq_events.gib`'s own grid puts a cell per event at the grid's
    /// geometry — `Rows 3, Columns 3, RowHeight 64, ColWidths 114 17 …` — with
    /// each cell's own art.
    #[test]
    fn the_event_grid_takes_a_cell_per_event() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut gib = crate::workshop::load(&ui, "teamhq/hq_events.gib").unwrap();
        let root = gib
            .blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let grid = crate::workshop::find_mut(root, GRID).expect("the event grid");
        assert!(
            grid.notes()
                .contains("this season's events (event_cell.gib)"),
            "the grid's note: {:?}",
            grid.notes()
        );

        let template = crate::workshop::load(&ui, TEMPLATE).unwrap();
        let events = all(&game).unwrap();
        let bars = grid.children.len();
        assert_eq!(bars, 2, "the grid's own two selection bars");
        assert_eq!(fill_grid(grid, &game, &template, &events), events.len());

        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == GRID)
            .unwrap();
        assert_eq!(grid.children.len(), bars + events.len());
        for (i, cell) in grid.children.iter().skip(bars).enumerate() {
            let (row, column) = ((i / 3) as f32, (i % 3) as f32);
            // `BufferWidth 2` insets each cell inside its 114 x 64 slot.
            assert_eq!(
                cell.frame.x,
                grid.frame.x + 114.0 * column + 2.0,
                "cell {i}"
            );
            assert_eq!(cell.frame.y, grid.frame.y + 64.0 * row + 2.0, "cell {i}");
            assert_eq!(cell.frame.w, 110.0, "cell {i}");
            assert_eq!(cell.frame.h, 60.0, "cell {i}");
            // And the art is the event's own.
            let art = cell
                .walk()
                .find(|p| p.widget.name() == ART)
                .expect("preview img");
            assert_eq!(art.widget.filename(), Some(events[i].art.as_str()));
        }
    }
}
