//! The arena list: `exhibition/arena.gib`'s listbox and the arenas the game ships.
//!
//! The screen states the list's content in its own `Notes` — **"populate with
//! arena names"** — and the arenas are the game's, under `Arenas`: one `.py`
//! script each (`box.py`, `Bridge.py`, `clawtop.py`, …) whose class declares the
//! name the list shows (`name = "Combat Arena"`), the same name `Events.txt` uses
//! when it says which arena an event is fought in.
//!
//! ## What the data does not state
//!
//! A `ListBox` draws strings the C++ hands it, and there is no face written down
//! for them anywhere in the file — so the rows are drawn in `fg_mcn14`, **the face
//! the shell uses most** (212 widgets against the next face's 127). That is a
//! porting decision rather than a reading, and it is the only one in this module.
//!
//! The row **height** is not a guess: the list's own `Sel Bar` is `159 0 18 0`, so
//! a row is 18 pixels and the rows begin at the list's top.

use std::path::Path;

use ra2fmt::gib::Widget;

/// The list, by the name the screen gives it.
pub const LIST: &str = "arena listbox";

/// The one thing the file leaves open: the face the rows are drawn in.
pub const ROW_FACE: &str = "fg_mcn14";

/// An arena the game ships.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arena {
    /// The name the list shows — `Combat Arena`.
    pub name: String,
    /// The class docstring the picker shows under the preview.
    pub description: String,
    /// The art the picker hands the C++ beside the name, as the script writes it:
    /// `box/box_preview.bmp`.
    pub preview: String,
    /// The script it comes from, relative to `Arenas`.
    pub script: String,
    /// The game types the script declares (`game_types`), in its own spelling.
    pub game_types: Vec<String>,
}

impl Arena {
    /// Whether the arena offers `game_type`, compared case-insensitively
    /// against the script's own spelling (`KING OF THE HILL`).
    pub fn supports(&self, game_type: &str) -> bool {
        self.game_types
            .iter()
            .any(|declared| declared.eq_ignore_ascii_case(game_type.trim()))
    }
}

/// Every arena the game ships, by name.
///
/// The rule is the game's own, from `Arenas/__init__.py`:
///
/// ```python
/// def enumAvailable(type):
///     list = [x for x in arenas if x.in_list and type in x.game_types]
///     list.sort(sortByName)
/// ```
///
/// so an arena is a script that **registers** one (`Arenas.register(BoxArena)`)
/// and does not set `in_list = False`, and the picker sorts them by name. That is
/// what leaves `__init__.py` out (the package, no registration), `Hazards.py`
/// (a helper) and `event_results.py` (registered, but `in_list = False` — it is
/// the prize scene).
pub fn all(game: &Path) -> Result<Vec<Arena>, String> {
    let dir = game.join("Arenas");
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{e}"))?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("py") {
            continue;
        }
        let Ok(script) = ra2fmt::arena::parse_path(&path) else {
            continue;
        };
        if !script.registered || !script.in_list || script.name.is_empty() {
            continue;
        }
        out.push(Arena {
            name: script.name,
            description: script.description,
            preview: script.preview.unwrap_or_default(),
            script: path.file_name().unwrap().to_string_lossy().to_string(),
            game_types: script.game_types,
        });
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

/// The arenas the picker offers for `game_type`: the original's own filter,
/// `Arenas/__init__.py`'s `enumAvailable(type)` —
///
/// ```python
/// list = [x for x in arenas if x.in_list and type in x.game_types]
/// ```
///
/// `game_type` is the script's spelling (`KING OF THE HILL`).
pub fn available(game: &Path, game_type: &str) -> Result<Vec<Arena>, String> {
    Ok(all(game)?
        .into_iter()
        .filter(|arena| arena.supports(game_type))
        .collect())
}

/// Fill the list with one text row per arena.
///
/// Returns how many rows were written — nothing when the list is not there.
pub fn fill_list(list: &mut Widget, arenas: &[Arena]) -> usize {
    let names: Vec<String> = arenas.iter().map(|a| a.name.clone()).collect();
    crate::input::text_rows(list, &names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::gib::Block;

    fn game_dir() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
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

    /// `enumAvailable(type)` is the scripts' own `game_types`: eight arenas for
    /// deathmatch, the two tabletop tops, the two King-of-the-Hill hills, and
    /// the box arena declares the three fight types it can host.
    #[test]
    fn the_available_list_is_filtered_by_the_scripts_game_types() {
        let Some(game) = game_dir() else { return };
        let deathmatch = available(&game, "DEATHMATCH").unwrap();
        assert_eq!(deathmatch.len(), 8);
        assert!(deathmatch.iter().all(|arena| arena.supports("DEATHMATCH")));
        assert_eq!(available(&game, "TABLETOP").unwrap().len(), 2);
        assert_eq!(available(&game, "KING OF THE HILL").unwrap().len(), 2);
        assert_eq!(available(&game, "BATTLE ROYAL").unwrap().len(), 8);
        assert_eq!(available(&game, "TEAM MATCH").unwrap().len(), 8);
        let box_ = all(&game)
            .unwrap()
            .into_iter()
            .find(|arena| arena.script == "box.py")
            .unwrap();
        assert!(box_.supports("DEATHMATCH"));
        assert!(box_.supports("BATTLE ROYAL"));
        assert!(box_.supports("TEAM MATCH"));
        assert!(!box_.supports("TABLETOP"));
        assert!(!box_.supports("KING OF THE HILL"));
        assert!(available(&game, "NONSENSE").unwrap().is_empty());
    }

    /// The arenas are the scripts that name one, and the names are the game's
    /// own — `Combat Arena` among them, the name `Events.txt` uses for the arena
    /// `box.py` builds.
    #[test]
    fn the_arenas_are_the_scripts_that_name_one() {
        let Some(game) = game_dir() else { return };
        let arenas = all(&game).unwrap();
        assert_eq!(arenas.len(), 12, "the arenas the picker offers");
        assert!(arenas
            .iter()
            .any(|a| a.script == "box.py" && a.name == "Combat Arena"));
        // The picker's own `preview`, which it hands the C++ beside the name.
        assert_eq!(
            arenas
                .iter()
                .find(|a| a.name == "Combat Arena")
                .unwrap()
                .preview,
            "box/box_preview.bmp"
        );
        // The package, a helper, and the prize scene: not offered.
        assert!(!arenas.iter().any(|a| a.script == "__init__.py"));
        assert!(!arenas.iter().any(|a| a.script == "Hazards.py"));
        assert!(!arenas.iter().any(|a| a.name == "Event Results"));
        assert!(!arenas.iter().any(|a| a.name == "No Name"));
        // And they are in name order, which is the order the list shows.
        let mut sorted = arenas.clone();
        sorted.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.name.cmp(&b.name))
        });
        assert_eq!(arenas, sorted);
    }

    /// Filling the screen's own list gives one row per arena, 18 pixels apart —
    /// the height its own `Sel Bar` states — with the names in order.
    #[test]
    fn the_list_takes_a_row_per_arena() {
        let Some(game) = game_dir() else { return };
        let ui = game.join("UI");
        let mut gib = crate::workshop::load(&ui, "exhibition/arena.gib").unwrap();
        let arenas = all(&game).unwrap();
        let bars = {
            let root = gib
                .blocks
                .iter_mut()
                .find_map(|b| match b {
                    Block::Widget(w) => Some(w),
                    _ => None,
                })
                .unwrap();
            let list = crate::workshop::find_mut(root, LIST).expect("the arena listbox");
            assert!(
                list.notes().contains("populate with arena names"),
                "the list's note: {:?}",
                list.notes()
            );
            let bars = list.children.len();
            assert_eq!(bars, 2, "the list's own two bars");
            assert_eq!(fill_list(list, &arenas), arenas.len());
            bars
        };

        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let list = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LIST)
            .unwrap();
        assert_eq!(list.children.len(), bars + arenas.len());
        for (i, row) in list.children.iter().skip(bars).enumerate() {
            // Rows sit 18 px apart, text inset by the measured row-text offset.
            assert_eq!(
                row.frame.y,
                list.frame.y + 18.0 * i as f32 + crate::input::ROW_TEXT_Y,
                "row {i}"
            );
            assert_eq!(
                row.frame.x,
                list.frame.x + crate::input::ROW_TEXT_X,
                "row {i}"
            );
            assert_eq!(row.frame.h, 18.0, "row {i}");
            assert_eq!(row.widget.text(), Some(arenas[i].name.as_str()), "row {i}");
        }
        // A refill replaces the rows rather than adding to them.
        for block in gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            let list = crate::workshop::find_mut(root, LIST).unwrap();
            assert_eq!(fill_list(list, &arenas[..3]), 3);
            assert_eq!(list.children.len(), bars + 3);
            break;
        }
    }
}
