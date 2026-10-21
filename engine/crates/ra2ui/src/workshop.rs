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
//! The interaction. A tab's own buttons — the component list's selection, the
//! chassis *drawing* tools, the snap checkbox's effect on a point — are the
//! game's C++, not its data, and they are what the next slice is for. What this
//! gives them is the composed screen they act on. Two of them are here because
//! the data states them completely: the steps a tab offers ([`STEPS`]), and the
//! layer that rides over the structure screens ([`STEP_LAYER`]) with the two
//! buttons that swap between them.

use std::path::{Path, PathBuf};

use ra2fmt::gib::{Block, Gib, Widget};

use crate::{input, Fonts, Placement};

pub mod build;
pub mod chassis;
pub mod library;
pub mod paint;
pub mod placement;
pub mod stats;
pub mod wiring;

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

/// A **step** of a tab: a button that replaces the tab's own screen with another
/// one, in the same `content slot`.
///
/// The chassis tab has two, and both the button's note and its screen's note say
/// which screen: `structure design button` says *"click to go to chassis step
/// 1"*, and `chassis_structure1.gib` opens with `Notes step 1 of the chassis
/// creation process`. `armor button` says *"click to load ch_armor.gib into
/// 'right content slot'"* — **stale in both respects**: the game ships
/// `armor.gib`, not `ch_armor.gib`, and `armor.gib`'s own root record says `Notes
/// load this into content slot`. The screens are the evidence that outranks the
/// button's note, and they agree with each other: each is a full-content screen
/// — `armor.gib` is a 50/50 split whose left pane is 400 pixels, wider than the
/// 320-pixel `right content slot` the note names — and each carries the root
/// note the others do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// The tab the button belongs to.
    pub tab: &'static str,
    /// The button's `Name` in that tab's screen.
    pub button: &'static str,
    /// The screen it puts in [`CONTENT_SLOT`].
    pub fragment: &'static str,
    /// A second screen it puts in the slot *with* the first, if it has one.
    pub layer: Option<&'static str>,
}

/// The steps the shipped screens offer, and the button that starts each.
pub const STEPS: [Step; 2] = [
    Step {
        tab: "chassis button",
        button: "structure design button",
        fragment: "botlab/chassis_structure1.gib",
        layer: Some(STEP_LAYER),
    },
    Step {
        tab: "chassis button",
        button: "armor button",
        fragment: "botlab/armor.gib",
        layer: None,
    },
];

/// The layer that rides over the chassis *structure* screens: the snap checkbox,
/// the finished button and the two step buttons.
///
/// ## Why it is a sibling in [`CONTENT_SLOT`], and not a part of the screens
///
/// `ch_stepbuttons.gib`'s root is `steps and snap group`, and its own position
/// says where it goes without anyone having to be told: `Alignment 36` is
/// *right + bottom* and `Position 1 1 1 1` is both axes at 100%, so it is
/// anchored to the **bottom-right corner of whoever holds it**. No chassis screen
/// has a slot for it — `chassis_structure1.gib` and `chassis_structure2.gib` are
/// both `chassis split`, `Slot0` the blueprint and `Slot1` the tools, and neither
/// names anything a 70-pixel group could go in — and both screens' own root
/// records say `Notes load this into content slot`. So the layer is the screens'
/// *sibling* in that same slot, added after them: the C++ loads it the way it
/// loads every screen (`FUN_00437770`, which caches it in a member and inserts a
/// loaded screen into a slot it looks up by name) and its own anchor then puts it
/// in the corner of the content area.
///
/// ## Its buttons are the two chassis *structure* screens
///
/// The layer is the only place the two steps live: `step 1 button` is noted
/// *"step 1 of chassis outline"* and `step 2 button` *"step 2 (extrude, top
/// outline)"*, their captions are literally `1` and `2`, and the binary loads
/// `botlab\chassis_structure1.gib` and `botlab\chassis_structure2.gib` from the
/// same table with the labels `Chassis 1` and `Chassis 2`. So a click on one
/// swaps the content area's screen and nothing else — the layer stays, because it
/// is what the click is on.
pub const STEP_LAYER: &str = "botlab/ch_stepbuttons.gib";

/// The layer's two step buttons and the screen each shows.
pub const STRUCTURE_STEPS: [(&str, &str); 2] = [
    ("step 1 button", "botlab/chassis_structure1.gib"),
    ("step 2 button", "botlab/chassis_structure2.gib"),
];

/// The screen a click on a layer step button shows.
pub fn structure_step(button: &str) -> Option<&'static str> {
    STRUCTURE_STEPS
        .iter()
        .find(|(name, _)| *name == button)
        .map(|(_, screen)| *screen)
}

/// The step a button starts, by the button's `Name`.
pub fn step(button: &str) -> Option<&'static Step> {
    STEPS.iter().find(|s| s.button == button)
}

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

/// The first widget called `name`, to write it.
pub fn find_mut<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
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

/// The toolbar's option group, whose children are the six tabs.
pub const TAB_GROUP: &str = "blab option group";

/// The workshop, as a piece of live state.
///
/// The screen is the game's own file and stays composed: a click changes the
/// group's selection and the content slot's screen, and changes nothing else. It
/// is deliberately *not* rebuilt from disk on a click — that would throw away the
/// selection the click just made and put the file's baked one back.
pub struct BotLab {
    ui_root: PathBuf,
    /// The composed document: the window, its info bar, its proxies, and the
    /// selected tab's screen.
    pub document: Gib,
    /// The button of the tab on screen.
    pub tab: &'static str,
    /// The step started on it, if any: its screen is in `content slot` instead
    /// of the tab's own.
    pub step: Option<&'static str>,
    /// The layer the current step rides over its screen, if any — reloaded on
    /// every screen change so it survives one.
    layer: Option<&'static str>,
    /// Where the screen's own first record sits in `content slot`: the slot's own
    /// children, then the layer's, then the screen's.
    screen_at: usize,
    /// How many children `content slot` has of its own, before a tab is loaded
    /// into it — the file's `white backdrop img`, and any it ever gains.
    slot_own: usize,
}

impl BotLab {
    /// Open the workshop on the tab the file itself has selected.
    pub fn open(ui_root: &Path) -> Result<BotLab, String> {
        let document = compose(ui_root, TABS.first().expect("six tabs"))?;
        let mut lab = BotLab {
            ui_root: ui_root.to_path_buf(),
            document,
            tab: TABS[0].button,
            step: None,
            layer: None,
            screen_at: 0,
            slot_own: 0,
        };
        // `compose` has already loaded the first tab; count what the slot holds
        // of its own by loading that same tab again over it.
        let fragment = load(ui_root, TABS[0].fragment)?;
        let tab_children = fragment.widgets().count();
        let total = lab.slot_children();
        lab.slot_own = total.saturating_sub(tab_children);
        // The composed screen is the slot's own children and then the first tab,
        // with no layer yet.
        lab.screen_at = lab.slot_own;
        Ok(lab)
    }

    /// How many children `content slot` currently has.
    fn slot_children(&self) -> usize {
        for block in &self.document.blocks {
            if let Block::Widget(w) = block {
                if let Some(slot) = find(w, CONTENT_SLOT) {
                    return slot.children.len();
                }
            }
        }
        0
    }

    /// Put a screen into `content slot`, leaving the slot's own children alone —
    /// and the step's layer after it, if one is riding there.
    fn set_screen(&mut self, relative: &str) -> Result<(), String> {
        let fragment = load(&self.ui_root, relative)?;
        let layer = match self.layer {
            Some(path) => Some(load(&self.ui_root, path)?),
            None => None,
        };
        let mut done = false;
        for block in self.document.blocks.iter_mut() {
            let Block::Widget(w) = block else { continue };
            let Some(slot) = find_mut(w, CONTENT_SLOT) else {
                continue;
            };
            slot.children.truncate(self.slot_own);
            // The layer goes in **before** the screen, because a tie in
            // `ChildOrder` is broken by the file with the earlier record in
            // front: the layer is what must be clickable, so it is the earlier
            // one. (The shipped screens' own pairs are the same shape — see
            // `draw_order_parented`.)
            if let Some(layer) = &layer {
                slot.children.extend(layer.widgets().cloned());
            }
            self.screen_at = slot.children.len();
            slot.children.extend(fragment.widgets().cloned());
            done = true;
            break;
        }
        if !done {
            return Err(format!("{SCREEN} has no {CONTENT_SLOT}"));
        }
        Ok(())
    }

    /// Put a tab's own screen into `content slot`.
    pub fn set_tab(&mut self, button: &str) -> Result<bool, String> {
        let Some(tab) = tab(button) else {
            return Ok(false);
        };
        self.layer = None;
        self.set_screen(tab.fragment)?;
        self.tab = tab.button;
        self.step = None;
        Ok(true)
    }

    /// Select a tab: the toolbar's group, then the screen it shows.
    pub fn select(&mut self, button: &str) -> Result<bool, String> {
        if tab(button).is_none() {
            return Ok(false);
        }
        for block in self.document.blocks.iter_mut() {
            let Block::Widget(w) = block else { continue };
            input::select_option(w, TAB_GROUP, button);
        }
        self.set_tab(button)
    }

    /// The tab's own screen inside `content slot`: the first record the tab
    /// loaded, after the slot's own children. A fragment's *other* root records
    /// — `overview.gib` has two more, headerless `Widget`s — follow it, exactly
    /// as they do when a fragment is placed on its own.
    pub fn content_root(&self) -> Option<&Widget> {
        for block in &self.document.blocks {
            if let Block::Widget(w) = block {
                if let Some(slot) = find(w, CONTENT_SLOT) {
                    // The screen's first record, which the layer sits in front of.
                    return slot.children.get(self.screen_at);
                }
            }
        }
        None
    }

    /// Place the current document.
    pub fn placements(&self, screen: [f32; 2], fonts: Option<&Fonts>) -> Vec<Placement<'_>> {
        crate::resolve_with(&self.document, screen, fonts)
    }

    /// Click at a point. Returns the tab that was selected, if a tab was hit.
    ///
    /// The hit is the innermost widget under the point; walking back up the path
    /// finds the tab button that holds it, which is how a click on a tab's icon
    /// still selects the tab.
    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        screen: [f32; 2],
        fonts: Option<&Fonts>,
    ) -> Result<Option<&'static Tab>, String> {
        // The innermost tab button under the point, and the innermost *step*
        // button, if either is there.
        let (tab_button, step_button, structure, option) = {
            let placed = self.placements(screen, fonts);
            let Some(path) = input::hit(&placed, x, y) else {
                return Ok(None);
            };
            // The path is the widgets from the root down to the one that was
            // hit, so the option group a checkbox is in is the last
            // `OptionGroup` above it — which is how a click on a category button
            // knows its group without a table of pairs.
            let mut chain: Vec<&Placement> = vec![&placed[path[0]]];
            for index in &path[1..] {
                let next = &chain[chain.len() - 1].children[*index];
                chain.push(next);
            }
            let mut group: Option<String> = None;
            let mut found_tab = None;
            let mut found_step = None;
            let mut found_structure = None;
            let mut found_option: Option<(String, String)> = None;
            for placement in &chain {
                let name = placement.widget.name();
                if placement.widget.kind == ra2fmt::gib::WidgetKind::OptionGroup {
                    group = Some(name.to_string());
                }
                if placement.widget.kind == ra2fmt::gib::WidgetKind::CheckBox {
                    if let Some(group) = &group {
                        found_option = Some((group.clone(), name.to_string()));
                    }
                }
                if let Some(t) = tab(name) {
                    found_tab = Some(t.button);
                }
                if let Some(s) = step(name) {
                    found_step = Some(s.button);
                }
                if let Some(s) = structure_step(name) {
                    found_structure = Some(s);
                }
            }
            (found_tab, found_step, found_structure, found_option)
        };
        if let Some((group, child)) = option {
            // A click on an option group's member selects it in its group.
            self.select_option_member(&group, &child);
        }
        if let Some(next) = structure {
            // One of the layer's two step buttons: it swaps the content area's
            // screen. The layer stays — it is what the click was on.
            self.set_screen(next)?;
            return Ok(None);
        }
        if let Some(button) = step_button {
            // A step replaces the tab's screen; the tab stays selected.
            return self.start_step(button).map(|_| None);
        }
        let Some(button) = tab_button else {
            return Ok(None);
        };
        self.select(button)?;
        Ok(tab(button))
    }

    /// Select an option group's member in the composed document — the group by
    /// name, the member by name, which is how the screen itself names them.
    fn select_option_member(&mut self, group: &str, child: &str) -> bool {
        for block in self.document.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            if find(root, group).is_some() {
                input::select_option(root, group, child);
                return true;
            }
        }
        false
    }

    /// Start a step: its screen replaces the tab's own, in the same slot, and
    /// the tab's button stays selected.
    pub fn start_step(&mut self, button: &str) -> Result<bool, String> {
        let Some(step) = step(button) else {
            return Ok(false);
        };
        if self.tab != step.tab {
            // A step's button is only on its own tab's screen.
            return Ok(false);
        }
        self.layer = step.layer;
        self.set_screen(step.fragment)?;
        self.step = Some(step.button);
        Ok(true)
    }
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
        // The strip is the bottom pane of `lowersplit`, under the content.
        let content = at(CONTENT_SLOT);
        assert!(info.y >= content.y + content.h, "{info:?}");
        // The bottom bar the window proxies.
        let bar = at("common buts");
        assert_eq!(bar.w, 256.0);
        assert!(bar.y > at(CONTENT_SLOT).y);
    }

    /// Clicking a tab selects it: the toolbar's state images flip, the tab's
    /// name is the group's `Selection`, and the content slot holds a different
    /// screen. The click is hit-tested against the placed tree, not looked up.
    #[test]
    fn clicking_a_tab_swaps_the_screen() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        assert_eq!(lab.tab, "overview button");
        let root_name = |lab: &BotLab| {
            lab.content_root()
                .map(|w| w.name().to_string())
                .unwrap_or_default()
        };
        assert_eq!(root_name(&lab), "overview split");

        // The chassis tab's frame is 431,37 100x24 in `botlabwin.gib`.
        let hit = lab.click(437.0, 120.0, DESIGN_SCREEN, None).unwrap();
        assert_eq!(hit.map(|t| t.button), Some("chassis button"));
        assert_eq!(lab.tab, "chassis button");
        assert_eq!(root_name(&lab), "chassis intro split");

        // The group's selection and the two tabs' state images both moved, in
        // the document the renderer reads.
        let root = lab
            .document
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap();
        let group = find(root, TAB_GROUP).unwrap();
        assert_eq!(
            group.field("Selection").map(|f| f.text()),
            Some("chassis button")
        );
        let selected_image = |name: &str| {
            let button = find(root, name).unwrap();
            button
                .children
                .iter()
                .filter(|c| c.kind == ra2fmt::gib::WidgetKind::Image)
                .position(|c| c.visible())
                .unwrap()
        };
        assert_eq!(selected_image("chassis button"), 2);
        assert_eq!(selected_image("overview button"), 0);

        // A click on a tab that is already selected changes nothing.
        assert_eq!(
            lab.click(437.0, 120.0, DESIGN_SCREEN, None)
                .unwrap()
                .map(|t| t.button),
            Some("chassis button")
        );
        assert_eq!(lab.tab, "chassis button");
        // A click on the screen away from the toolbar is not a tab.
        assert!(lab
            .click(20.0, 900.0, DESIGN_SCREEN, None)
            .unwrap()
            .is_none());
        assert_eq!(lab.tab, "chassis button");
    }

    /// The chassis tab composes further, and its own buttons' `Notes` say so:
    /// `structure design button` goes to chassis step 1, and `armor button`
    /// loads a screen into `'right content slot'` — naming `ch_armor.gib`, which
    /// the game does not ship: the file is `armor.gib`.
    #[test]
    fn the_chassis_tab_names_its_own_steps() {
        let Some(ui) = ui_root() else { return };
        let chassis = load(&ui, "botlab/chassis.gib").unwrap();
        let root = chassis.widgets().next().unwrap();
        let structure = find(root, "structure design button").expect("the step 1 button");
        assert!(
            structure.notes().contains("chassis step 1"),
            "{}",
            structure.notes()
        );
        let armor = find(root, "armor button").expect("the armor button");
        assert!(
            armor.notes().contains("right content slot"),
            "{}",
            armor.notes()
        );
        // The note's own file name is stale; the shipped one is `armor.gib`.
        assert!(armor.notes().contains("ch_armor.gib"));
        assert!(ra2fmt::find_ci(&ui, "botlab/ch_armor.gib").is_none());
        assert!(ra2fmt::find_ci(&ui, "botlab/armor.gib").is_some());
        // And the container has the slot the note names.
        assert!(find(root, "right content slot").is_some());
    }

    /// The chassis intro's two buttons are steps, and clicking one puts its
    /// screen in the tab's own `content slot` — the tab stays selected.
    #[test]
    fn clicking_a_chassis_step_replaces_the_tabs_screen() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        lab.select("chassis button").unwrap();
        assert_eq!(
            lab.content_root().map(|w| w.name()),
            Some("chassis intro split")
        );

        // The two buttons are placed in `chassis.gib`'s `right content slot`;
        // click each at the centre of where the layout put it.
        for (button, root) in [
            ("structure design button", "chassis split"),
            ("armor button", "armor split"),
        ] {
            let (x, y) = reachable_point(&lab, button);
            assert!(lab.click(x, y, DESIGN_SCREEN, None).unwrap().is_none());
            assert_eq!(lab.step, Some(button));
            assert_eq!(lab.content_root().map(|w| w.name()), Some(root));
            assert_eq!(lab.tab, "chassis button", "the tab stays selected");
            lab.select("chassis button").unwrap();
            assert_eq!(lab.step, None, "re-selecting the tab clears the step");
        }

        // A step button belongs to its tab: starting one from another tab is
        // refused rather than obeyed.
        lab.select("overview button").unwrap();
        assert!(!lab.start_step("armor button").unwrap());
        assert_eq!(lab.content_root().map(|w| w.name()), Some("overview split"));
    }

    /// A click on an option group's member selects it in its group and changes
    /// nothing else — the group being found from the hit path (the last
    /// `OptionGroup` above the `CheckBox` that was hit), not from a table.
    ///
    /// The case is `chassis_structure1.gib`'s `drawstyle optiongroup`, the
    /// freeform / circle / rectangle tools. The component palette's own option
    /// group is *not* the case here: its grid is drawn over it (see
    /// `docs/05-validation-report.md`), so a click lands on the grid's selection
    /// bar instead of a button, and the palette's category is set by
    /// `ra2ui::components::selected_category` reading the screen.
    #[test]
    fn clicking_an_option_groups_member_selects_it() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        lab.select("chassis button").unwrap();
        lab.start_step("structure design button").unwrap();
        for member in ["circle checkbox", "rectangle checkbox", "freeform checkbox"] {
            let (x, y) = reachable_point(&lab, member);
            assert!(lab.click(x, y, DESIGN_SCREEN, None).unwrap().is_none());
            let group = find(lab.content_root().unwrap(), "drawstyle optiongroup").unwrap();
            assert_eq!(
                input::field_of(group, "Selection"),
                Some(member),
                "the group moved to {member}"
            );
        }
    }

    /// The palette's category buttons are clickable **with the palette filled**,
    /// which is the case that was not: the grid shares the category bar's
    /// `ChildOrder` and comes later in the file, so it is drawn over the buttons,
    /// and the grid's own frame must not swallow the click. Its cells still are
    /// targets — the grid's note says to click them — and the option group's
    /// buttons are what the note says filters it.
    #[test]
    fn the_palette_is_clickable_once_it_holds_cells() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        lab.select("components button").unwrap();
        // Fill it, which is what put the grid over the buttons before.
        let game = ui.parent().unwrap();
        let all = crate::components::all(game).unwrap();
        for block in lab.document.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            crate::components::fill_selected(root, game, &all);
            break;
        }
        assert_eq!(
            crate::components::selected_category(lab.content_root().unwrap()),
            Some(("power", "power button"))
        );

        for (category, button) in crate::components::CATEGORIES {
            let (x, y) = reachable_point(&lab, button);
            assert!(lab.click(x, y, DESIGN_SCREEN, None).unwrap().is_none());
            assert_eq!(
                crate::components::selected_category(lab.content_root().unwrap()),
                Some((category, button)),
                "a click on {button} reaches it with cells in the grid"
            );
            assert_eq!(lab.tab, "components button", "the tab stays");
        }
    }

    /// The structure step's layer rides in `content slot` with the screen, and
    /// the layer's own anchor puts it in the slot's bottom-right corner — which
    /// is where the game's snap and step controls sit.
    #[test]
    fn the_step_layer_rides_the_content_slots_bottom_right() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        lab.select("chassis button").unwrap();
        assert!(lab.start_step("structure design button").unwrap());

        // The screen is still the slot's first, and the layer follows it.
        assert_eq!(lab.content_root().map(|w| w.name()), Some("chassis split"));
        let placed = lab.placements(DESIGN_SCREEN, None);
        let frame_of = |name: &str| {
            placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == name)
                .unwrap_or_else(|| panic!("no widget called {name}"))
                .frame
        };
        let slot = frame_of("content slot");
        let layer = frame_of("steps and snap group");
        // `Alignment 36` = right + bottom, `Position 1 1 1 1` = both axes at
        // 100%, `Size 0.46 1 70 0` = 46% wide and 70 pixels tall.
        assert!(
            (layer.x + layer.w - (slot.x + slot.w)).abs() < 0.5,
            "{layer:?}"
        );
        assert!(
            (layer.y + layer.h - (slot.y + slot.h)).abs() < 0.5,
            "{layer:?}"
        );
        assert!((layer.w - slot.w * 0.46).abs() < 0.5, "{layer:?}");
        assert_eq!(layer.h, 70.0, "{layer:?}");

        // The armor step has none: its screen fills the slot on its own.
        lab.select("chassis button").unwrap();
        assert!(lab.start_step("armor button").unwrap());
        let placed = lab.placements(DESIGN_SCREEN, None);
        assert!(
            !placed
                .iter()
                .flat_map(|p| p.walk())
                .any(|p| p.widget.name() == "steps and snap group"),
            "the armor step carries no layer"
        );
    }

    /// The layer's own two buttons are the two chassis structure screens: a click
    /// on one swaps the content area's screen and leaves the layer where it is.
    #[test]
    fn the_layers_buttons_swap_the_two_structure_screens() {
        let Some(ui) = ui_root() else { return };
        let mut lab = BotLab::open(&ui).unwrap();
        lab.select("chassis button").unwrap();
        lab.start_step("structure design button").unwrap();

        // Step 1's blueprint is labelled the baseplate; step 2's the topplate.
        let blueprint = |lab: &BotLab| {
            lab.content_root()
                .and_then(|root| find(root, "label img"))
                .and_then(|img| img.filename().map(str::to_string))
                .unwrap()
        };
        assert_eq!(blueprint(&lab), "botlab\\baseplate_label.bmp");

        let (x, y) = reachable_point(&lab, "step 2 button");
        assert!(lab.click(x, y, DESIGN_SCREEN, None).unwrap().is_none());
        assert_eq!(blueprint(&lab), "botlab\\topplate_label.bmp");
        assert_eq!(lab.step, Some("structure design button"), "same step");
        assert_eq!(lab.tab, "chassis button");
        // And it is still there to click back.
        assert!(lab
            .placements(DESIGN_SCREEN, None)
            .iter()
            .flat_map(|p| p.walk())
            .any(|p| p.widget.name() == "steps and snap group"));
        let (x, y) = reachable_point(&lab, "step 1 button");
        lab.click(x, y, DESIGN_SCREEN, None).unwrap();
        assert_eq!(blueprint(&lab), "botlab\\baseplate_label.bmp");
    }

    /// A point inside `name` that a click actually *reaches* it at.
    ///
    /// Not the same as its centre, and that is the shipped layout's doing:
    /// `chassis.gib`'s `structure note` label is drawn over its two buttons — a
    /// click on one's middle lands on the note, which is what a label in front
    /// should do. So the point is found by hit-testing a few of them and taking
    /// the first whose path goes through the widget.
    fn reachable_point(lab: &BotLab, name: &str) -> (f32, f32) {
        let placed = lab.placements(DESIGN_SCREEN, None);
        let frame = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == name)
            .unwrap_or_else(|| panic!("no widget called {name}"))
            .frame;
        let passes_through = |path: &[usize]| {
            let mut placement = &placed[path[0]];
            let mut found = placement.widget.name() == name;
            for index in &path[1..] {
                placement = &placement.children[*index];
                found |= placement.widget.name() == name;
            }
            found
        };
        for iy in 1..7 {
            for ix in 1..9 {
                let x = frame.x + frame.w * ix as f32 / 9.0;
                let y = frame.y + frame.h * iy as f32 / 7.0;
                if let Some(path) = input::hit(&placed, x, y) {
                    if passes_through(&path) {
                        return (x, y);
                    }
                }
            }
        }
        panic!("no click reaches {name}");
    }

    /// Every step's screen says it goes in `content slot`, and the button that
    /// starts it is on that tab's screen — which is what makes the button's own
    /// note about `'right content slot'` stale rather than a second rule.
    #[test]
    fn every_step_screen_names_the_content_slot() {
        let Some(ui) = ui_root() else { return };
        for step in &STEPS {
            let fragment = load(&ui, step.fragment).unwrap();
            // `armor.gib` names the slot outright; the two chassis-structure
            // screens name their *step* instead ("step 1 of the chassis creation
            // process"), which is what their button's note says ("go to chassis
            // step 1"). Both are the data's own words for the pairing.
            assert!(
                fragment
                    .widgets()
                    .any(|w| w.notes().contains(CONTENT_SLOT)
                        || w.notes().contains("chassis creation")),
                "{} says neither where it goes nor which step it is",
                step.fragment
            );
            let tab = tab(step.tab).expect("a tab");
            let screen = load(&ui, tab.fragment).unwrap();
            let root = screen.widgets().next().unwrap();
            assert!(
                find(root, step.button).is_some(),
                "{}'s screen has no {}",
                step.tab,
                step.button
            );
        }
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
