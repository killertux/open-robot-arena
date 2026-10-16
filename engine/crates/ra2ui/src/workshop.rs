//! The workshop: composing the bot lab's own screens.
//!
//! The bot lab is `botlab/botlabwin.gib` — a toolbar of six tabs, a content
//! area below it and the shell's navigation bar in its own 49-pixel strip — and
//! a *tab* is a fragment loaded into it. Nothing here authors a screen: the
//! window and every tab are the game's own `.gib` files, and what this module
//! adds is the composition the game's C++ does (`FUN_00437770`, reached from the
//! `BotLabWindow` constructor, which loads `botlab\botlabwin.gib` and then, per
//! tab, looks a slot up **by name**, loads the fragment and puts it there).
//!
//! ## The tab table is the data's own
//!
//! The six tabs are the `CheckBox`es of `botlabwin.gib`'s `blab option group`,
//! and five of them carry a `Notes` field that names the screen they show:
//! `overview button` says *"click to show 3dview.gib and overview.gib"*, and
//! `chassis`, `components` and `wiring` say the same with their own file.
//! `test robot button` says *"click to show testarena.gib"*. The sixth,
//! `paintshop button`, says *"go to chassis appearance effects"* — which is
//! `botlab/appearance.gib`, the file the binary pairs with the string
//! `Appearance` (`0x6efda0`, referenced from `0x423099`). The other two tabs'
//! pairings are the binary's too: `overview.gib` from `0x6f0f60` and
//! `components.gib` from `0x6f0468`. [`TABS`] is those mappings, and the test
//! beside it re-reads the `Notes` from the shipped file so the table cannot
//! drift from the data it came from.
//!
//! ## The slot is named, and the fragments say which
//!
//! Every tab fragment's root record carries `Notes load this into content slot`
//! — `overview.gib`, `chassis.gib`, `components.gib`, `wiring.gib`,
//! `appearance.gib` and `armor.gib` all say it — and the binary looks the string
//! `content slot` (`0x6f0f74`) up by name on the screen it just loaded. The
//! second fragment, the info bar, is `botlab/info_display.gib` (`0x6f0c88`)
//! loaded into `lowersplit` (`0x6f0ca0`): a `Widget "Info Display"` at 80% width
//! and 18 pixels tall, which lands on the pane above the content area and under
//! the shell's nav strip.
//!
//! ## What is *not* here
//!
//! The interaction. A tab's own buttons — the chassis tab's `Chassis 1` /
//! `Chassis 2` / `Armor` steps, the component list's selection, the drawing
//! tools — are the game's C++, not its data, and they are what the next slice
//! is for. What this gives them is the composed screen they act on.

use std::path::Path;

use ra2fmt::gib::{Block, Gib, Widget};

/// The bot lab window.
pub const SCREEN: &str = "botlab/botlabwin.gib";
/// The group a tab's screen is loaded into — the name the fragments' own `Notes`
/// give, and the name the binary looks up (`0x6f0f74`).
pub const CONTENT_SLOT: &str = "content slot";
/// The info bar fragment, loaded into [`INFO_SLOT`].
pub const INFO: &str = "botlab/info_display.gib";
/// The split the info bar goes into (`0x6f0ca0`).
pub const INFO_SLOT: &str = "lowersplit";

/// One tab of the workshop: the `CheckBox` that selects it, and the screen it
/// shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    /// The button's `Name` in `botlabwin.gib`.
    pub button: &'static str,
    /// The `.gib` loaded into [`CONTENT_SLOT`].
    pub fragment: &'static str,
}

/// The six tabs, in the order the toolbar lists them.
pub const TABS: [Tab; 6] = [
    Tab {
        button: "overview button",
        fragment: "botlab/overview.gib",
    },
    Tab {
        button: "chassis button",
        fragment: "botlab/chassis.gib",
    },
    Tab {
        button: "components button",
        fragment: "botlab/components.gib",
    },
    Tab {
        button: "wiring button",
        fragment: "botlab/wiring.gib",
    },
    Tab {
        button: "paintshop button",
        fragment: "botlab/appearance.gib",
    },
    Tab {
        button: "test robot button",
        fragment: "botlab/testarena.gib",
    },
];

/// Read and parse one `.gib` under a game's `UI` directory.
pub fn load(ui_root: &Path, relative: &str) -> Result<Gib, String> {
    let path = ra2fmt::find_ci(ui_root, relative)
        .ok_or_else(|| format!("{relative} is not in {}", ui_root.display()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    ra2fmt::gib::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
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

/// Load a fragment's records into the widget called `slot`.
///
/// The fragment's **root records** become the slot's children — an attachment a
/// fragment begins with is not a widget and has nothing to hang on, so it is
/// skipped, exactly as a `Proxy`'s payload skips one. A fragment's root is
/// `Size 1 1 1 1` in every case that ships (`overview.gib`'s `overview split`,
/// `appearance.gib`'s `appearance split`, …) so it fills the slot.
pub fn load_into(root: &mut Widget, fragment: &Gib, slot: &str) -> bool {
    let Some(target) = find_mut(root, slot) else {
        return false;
    };
    target
        .children
        .extend(fragment.blocks.iter().filter_map(|b| match b {
            Block::Widget(w) => Some(w.clone()),
            Block::Attachment(_) => None,
        }));
    true
}

/// Compose the workshop as the game does: the window, its info bar, the tab's
/// screen, and the fragments any of them `Proxy`.
///
/// Returns the composed document, ready for `ra2ui::resolve_with`.
pub fn compose(ui_root: &Path, tab: &Tab) -> Result<Gib, String> {
    let mut gib = load(ui_root, SCREEN)?;
    let info = load(ui_root, INFO)?;
    let fragment = load(ui_root, tab.fragment)?;

    let mut loaded_info = false;
    let mut loaded_tab = false;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(w) = block else { continue };
        loaded_info |= load_into(w, &info, INFO_SLOT);
        loaded_tab |= load_into(w, &fragment, CONTENT_SLOT);
    }
    if !loaded_info {
        return Err(format!("{SCREEN} has no {INFO_SLOT}"));
    }
    if !loaded_tab {
        return Err(format!("{SCREEN} has no {CONTENT_SLOT}"));
    }
    // The nav bar arrives through a `Proxy` in the window's own strip.
    crate::inline_proxies_in(&mut gib, ui_root);
    Ok(gib)
}

/// The tab a button selects, by the button's `Name`.
pub fn tab(button: &str) -> Option<&'static Tab> {
    TABS.iter().find(|t| t.button == button)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve, DESIGN_SCREEN};

    fn ui_root() -> Option<std::path::PathBuf> {
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

    /// The `Notes` the shipped toolbar writes on its own tab buttons name the
    /// screens they show, so the table above can be checked against the file it
    /// came from rather than trusted.
    #[test]
    fn the_toolbar_names_the_screen_each_tab_shows() {
        let Some(ui) = ui_root() else { return };
        let screen = load(&ui, SCREEN).unwrap();
        let root = screen.widgets().next().expect("the bot lab window");

        for tab in &TABS {
            let button = find(root, tab.button)
                .unwrap_or_else(|| panic!("no tab button called {}", tab.button));
            let notes = button.notes();
            let file = tab.fragment.rsplit('/').next().unwrap();
            // The file stem, without the `.gib`: "3dview" is a different screen
            // and must not satisfy "view".
            assert!(
                notes.contains(file.trim_end_matches(".gib")),
                "{}'s notes do not name {}: {notes:?}",
                tab.button,
                tab.fragment
            );
            assert!(
                ui.join(tab.fragment).exists() || ra2fmt::find_ci(&ui, tab.fragment).is_some(),
                "{} is not shipped",
                tab.fragment
            );
        }
        // The sixth button's note names the *effect*, not the file, and the
        // binary is what pairs the two.
        assert!(find(root, "paintshop button")
            .unwrap()
            .notes()
            .contains("appearance"));
    }

    /// Five of the six tab fragments name the slot on their **own root
    /// record** — `overview.gib`, `chassis.gib`, `components.gib`,
    /// `wiring.gib` and `appearance.gib` all open with `Notes load this into
    /// content slot` — and `testarena.gib` is the sixth: its root is `Group
    /// "test arena main group"` with no note, and the file names the slot
    /// further in, on the placeholder the arena itself fills (*"load the test
    /// arena here (put placeholder into content slot)"*). So the rule is the
    /// data's and the exception is the data's too, and both are asserted.
    #[test]
    fn the_fragment_says_which_slot_it_loads_into() {
        let Some(ui) = ui_root() else { return };
        for tab in &TABS {
            let fragment = load(&ui, tab.fragment).unwrap();
            assert!(
                fragment.walk().any(|w| w.notes().contains(CONTENT_SLOT)),
                "{} never names {CONTENT_SLOT}",
                tab.fragment
            );
            if tab.fragment != "botlab/testarena.gib" {
                assert!(
                    fragment.widgets().any(|w| w.notes().contains(CONTENT_SLOT)),
                    "{} does not name {CONTENT_SLOT} on its root record",
                    tab.fragment
                );
            }
        }
    }

    /// Composing a tab puts its screen in `content slot`, at the slot's own
    /// frame — the fragment's root is `Size 1 1 1 1` in all six.
    #[test]
    fn a_tab_fills_the_content_slot() {
        let Some(ui) = ui_root() else { return };
        let tabs = [
            ("overview button", "overview split"),
            ("chassis button", "chassis intro split"),
            ("components button", "components split"),
            ("wiring button", "wiring split"),
            ("paintshop button", "appearance split"),
        ];
        for (button, root_name) in tabs {
            let tab = tab(button).unwrap_or_else(|| panic!("no tab for {button}"));
            let gib = compose(&ui, tab).unwrap();
            let placed = resolve(&gib, DESIGN_SCREEN);
            let slot = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == CONTENT_SLOT)
                .expect("the content slot");
            let inside = slot
                .children
                .iter()
                .find(|c| c.widget.name() == root_name)
                .unwrap_or_else(|| {
                    panic!("{button}: {root_name} is not in {CONTENT_SLOT}");
                });
            assert_eq!(
                (
                    inside.frame.x,
                    inside.frame.y,
                    inside.frame.w,
                    inside.frame.h
                ),
                (slot.frame.x, slot.frame.y, slot.frame.w, slot.frame.h),
                "{button}: {root_name} fills the slot"
            );
        }
    }

    /// The info bar is a thin strip on the pane above the content area, and the
    /// shell's nav bar arrives through the window's own `Proxy`.
    #[test]
    fn the_info_bar_and_the_nav_bar_are_both_there() {
        let Some(ui) = ui_root() else { return };
        let gib = compose(&ui, tab("overview button").unwrap()).unwrap();
        let placed = resolve(&gib, DESIGN_SCREEN);
        let at = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("no widget called {name}"))
                .frame
        };
        // `Widget "Info Display"` is 80% of its parent wide and 18 tall, 2
        // pixels down, and the pane it sits on is the one above `content slot`.
        let info = at("Info Display");
        assert_eq!(info.h, 18.0);
        assert_eq!(info.w, 1024.0);
        assert!(info.y + info.h <= at(CONTENT_SLOT).y, "{info:?}");
        // The bottom bar the window proxies.
        let bar = at("common buts");
        assert_eq!(bar.w, 256.0);
        assert!(bar.y > at(CONTENT_SLOT).y);
    }

    /// The binary pairs `paintshop button` with `appearance.gib`, whose own
    /// title is "Appearance" — the same string the binary carries.
    #[test]
    fn the_paintshop_tab_is_the_appearance_screen() {
        let Some(ui) = ui_root() else { return };
        let tab = tab("paintshop button").unwrap();
        assert_eq!(tab.fragment, "botlab/appearance.gib");
        let fragment = load(&ui, tab.fragment).unwrap();
        assert!(
            fragment
                .walk()
                .any(|w| w.text().is_some_and(|t| t.contains("Appearance"))),
            "the appearance screen names itself"
        );
    }
}
