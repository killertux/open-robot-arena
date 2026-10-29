//! The pre-match lobby's panel buttons reach the application through the
//! shell, and the shipped robot picker opens over the lobby (docs/38 §15.9).

use std::path::PathBuf;

use ra2_shell::{PanelButton, PanelRequest, Shell, PREMATCH};
use ra2ui::teams::Side;

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

/// Put a shipped cell into a panel group, as the application draws the lobby.
fn fill(shell: &mut Shell, panel: usize, cell: &str) {
    let ui = shell.game().join("UI");
    let gib = ra2ui::workshop::load(&ui, cell).unwrap();
    let cell = gib.widgets().next().cloned().unwrap();
    let group = format!("player{} group", panel + 1);
    let doc = shell.document_mut().unwrap();
    for block in doc.blocks.iter_mut() {
        if let ra2fmt::gib::Block::Widget(root) = block {
            if let Some(g) = ra2ui::workshop::find_mut(root, &group) {
                g.children.truncate(1);
                g.children.push(cell);
                return;
            }
        }
    }
    panic!("no {group}");
}

#[test]
fn panel_buttons_are_reported_and_the_picker_opens_over_the_lobby() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [800.0, 600.0]).unwrap();
    shell.goto(PREMATCH).unwrap();
    fill(&mut shell, 0, "multiplayer/noplayer.gib");
    fill(&mut shell, 1, "multiplayer/noplayer.gib");
    fill(&mut shell, 2, "multiplayer/activeplayer.gib");

    // An open panel's two buttons: reported, nothing else happens.
    let step = shell.click_panel_button(0, "human player button").unwrap();
    assert_eq!(step.hit_widget(), Some("human player button"));
    assert_eq!(
        shell.take_panel_request(),
        Some(PanelRequest {
            slot: 0,
            button: PanelButton::Join
        })
    );
    assert_eq!(shell.take_panel_request(), None, "taken once");
    assert!(shell.popup_id().is_none(), "the application opens it");
    shell
        .click_panel_button(1, "computer player button")
        .unwrap();
    assert_eq!(
        shell.take_panel_request(),
        Some(PanelRequest {
            slot: 1,
            button: PanelButton::AddComputer
        })
    );

    // A panel with a robot: change, eject and ready.
    for (name, button) in [
        ("change robot button", PanelButton::Change),
        ("eject button", PanelButton::Eject),
        ("ready checkbox", PanelButton::Ready),
    ] {
        shell.click_panel_button(2, name).unwrap();
        assert_eq!(
            shell.take_panel_request(),
            Some(PanelRequest { slot: 2, button }),
            "{name}"
        );
    }
    assert!(
        shell.slot_pick(2).is_none(),
        "the exhibition state is untouched"
    );

    // Select Player Robot over the lobby: pick the second robot, OK.
    shell.open_lobby_picker(0, Side::Player).unwrap();
    assert_eq!(shell.popup_id(), Some("exhibition/selectbot_player.gib"));
    assert!(shell.base_screen_id().eq_ignore_ascii_case(PREMATCH));
    shell.click_popup_robot(1).unwrap();
    shell.click_named("ok button").unwrap();
    assert!(shell.popup_id().is_none());
    let pick = shell.take_lobby_pick().expect("the accepted robot");
    assert_eq!((pick.slot, pick.side), (0, Side::Player));
    let rosters = ra2ui::teams::rosters(&game, Side::Player).unwrap();
    assert_eq!(pick.pick.bot, rosters[pick.pick.team].robots[1]);
    assert!(ra2ui::teams::bot_path(&game, Side::Player, pick.pick.team, pick.pick.bot).is_file());
    assert_eq!(shell.take_lobby_pick(), None);
    assert!(shell.slot_pick(0).is_none(), "no exhibition panel art");

    // Select Computer Robot, then Cancel: nothing is picked.
    shell.open_lobby_picker(1, Side::Computer).unwrap();
    assert_eq!(shell.popup_id(), Some("exhibition/selectbot_computer.gib"));
    shell.click_popup_robot(0).unwrap();
    shell.click_named("cancel button").unwrap();
    assert!(shell.popup_id().is_none() && shell.take_lobby_pick().is_none());
    shell.open_lobby_picker(1, Side::Computer).unwrap();
    shell.click_popup_robot(0).unwrap();
    shell.click_named("ok button").unwrap();
    let pick = shell.take_lobby_pick().unwrap();
    assert_eq!((pick.slot, pick.side), (1, Side::Computer));
    assert!(shell.open_lobby_picker(4, Side::Player).is_err());
}
