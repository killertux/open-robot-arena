//! Regression tests for the recovered screen graph and the lists' contents:
//! every bound handler names a widget and a target that ship, the main menu
//! reaches each screen the exe's constructor (`FUN_00486960`) binds, and the
//! arena / select-team lists look as the captures do.

use std::path::PathBuf;

use ra2_shell::bindings::{self, Action, BINDINGS};
use ra2_shell::Shell;

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

fn load(ui: &std::path::Path, screen: &str) -> ra2fmt::gib::Gib {
    let text = std::fs::read_to_string(ui.join(screen.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .unwrap_or_else(|e| panic!("{screen}: {e}"));
    let mut gib = ra2fmt::gib::parse(&text).unwrap();
    ra2ui::inline_proxies_in(&mut gib, ui);
    gib
}

/// Every binding's screen ships, its widget exists on that screen, and a
/// `Goto`/`Window` target ships too.
#[test]
fn every_bound_handler_names_a_shipped_widget_and_target() {
    let Some(game) = game_dir() else { return };
    let ui = game.join("UI");
    for b in BINDINGS {
        let gib = load(&ui, b.screen);
        assert!(
            gib.walk().any(|w| w.name().eq_ignore_ascii_case(b.name)),
            "{} has no {:?}",
            b.screen,
            b.name
        );
        if let Action::Goto(t) | Action::Window(t) = b.action {
            assert!(ui.join(t).is_file(), "{t} does not ship");
        }
        assert!(bindings::lookup(b.screen, b.name).is_some());
    }
}

/// The main menu's graph: each bound button leads where the constructor says.
#[test]
fn the_main_menu_reaches_the_screens_its_constructor_binds() {
    let Some(game) = game_dir() else { return };
    for (button, screen, depth) in [
        ("exhibition button", "exhibition/exhib_setup.gib", 0),
        ("Team HQ button", "teamhq/select_team.gib", 0),
        ("Multiplayer button", "multiplayer/multiplayer.gib", 0),
        ("Options button", "mainmenu/mainmenu.gib", 1),
        ("credits button", "mainmenu/mainmenu.gib", 1),
    ] {
        let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
        shell.click_named(button).unwrap();
        assert_eq!(shell.base_screen_id(), screen, "{button}");
        assert_eq!(shell.window_depth(), depth, "{button}");
    }
}

/// The arena tab: all eight deathmatch arenas fit the 282 px list (15 rows), so
/// every row is visible, the first is selected and the viewport is measured.
#[test]
fn the_arena_list_shows_every_row_with_the_first_selected() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.click_named("exhibition button").unwrap();
    shell.click_named("arena button").unwrap();
    assert_eq!(shell.arena_row(), Some((0, 8)));
    let root = shell.document().widgets().next().unwrap();
    let list = ra2ui::workshop::find(root, ra2ui::arenas::LIST).unwrap();
    let view: usize = ra2ui::input::field_of(list, "_ViewRows")
        .and_then(|v| v.parse().ok())
        .expect("the viewport is measured");
    assert_eq!(view, 15);
    let rows: Vec<_> = list
        .children
        .iter()
        .filter(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
        .collect();
    assert_eq!(rows.len(), 8);
    assert!(rows.iter().all(|r| r.visible()));
    assert_eq!(rows[0].field("_Text").unwrap().text(), "Bridge of Doom");
}

/// Select team: the first team's row is selected (the capture's blue bar) and the
/// grid's scroll bar is hidden while the teams fit.
#[test]
fn select_team_opens_with_row_zero_selected_and_no_scroll_bar() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.click_named("Team HQ button").unwrap();
    let placed = shell.placements();
    let grid = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == ra2ui::teams::SELECT_GRID)
        .expect("select team grid");
    assert_eq!(
        ra2ui::input::field_of(grid.widget, "_SelectedRow"),
        Some("0")
    );
    let bar = grid
        .widget
        .children
        .iter()
        .find(|c| c.name() == "Sel Bar")
        .unwrap();
    assert!(bar.visible());
    let scroll = grid
        .attachments
        .iter()
        .flat_map(|a| a.body.walk())
        .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ScrollBar)
        .expect("the grid's scroll bar");
    assert!(!scroll.widget.visible(), "one team fits the grid");
}
