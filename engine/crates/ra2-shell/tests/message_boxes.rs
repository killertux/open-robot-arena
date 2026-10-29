//! The buttons of the message boxes stand in the middle of the box (live `teamhq-scrap-box`,
//! `mp-confirm-begin-unready`, `teamhq-events-one-event`, `mp-connection-termination`; docs/38 §16):
//! a lone OK's border at x 477, Yes and No at 438 and 516 in a box from 342.

use std::path::PathBuf;

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

/// The frame of the border attachment that draws a button, and of the button's caption text.
fn drawn(shell: &Shell, name: &str) -> Option<(f32, f32)> {
    for p in shell.window_placements() {
        let Some(button) = p.find(name) else { continue };
        let border = button.attachments.iter().find_map(|a| {
            (a.attachment.kind == ra2fmt::gib::AttachmentKind::Border).then_some(a.body.frame.x)
        })?;
        let text = button
            .attachments
            .iter()
            .flat_map(|a| a.body.walk())
            .find(|w| w.widget.kind.name() == "Text")?
            .frame
            .x;
        return Some((border, text));
    }
    None
}

#[test]
fn a_lone_ok_and_a_yes_no_pair_stand_in_the_middle_of_the_box() {
    let Some(game) = game_dir() else { return };
    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.show_message("Select Bots", "x").unwrap();
    let (border, text) = drawn(&shell, "ok button").expect("an OK button");
    assert_eq!(border.floor(), 477.0);
    // The label moved with it: 25 px into the button.
    assert!(
        (text - (border + 1.0 + 25.0)).abs() < 1.0,
        "{border} {text}"
    );

    let mut shell = Shell::boot(&game, [1024.0, 768.0]).unwrap();
    shell.ask_begin_match().unwrap();
    let (yes, yes_text) = drawn(&shell, "yes button").expect("a Yes button");
    let (no, _) = drawn(&shell, "no button").expect("a No button");
    assert_eq!((yes.floor(), no.floor()), (438.0, 516.0));
    assert!(yes_text > yes && yes_text < yes + 70.0, "{yes_text}");
}
