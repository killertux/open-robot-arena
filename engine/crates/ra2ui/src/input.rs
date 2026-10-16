//! Input: what a click lands on, and what a click changes.
//!
//! The shell has had no input until now — screens were placed and drawn, and
//! nothing was ever *pressed*. This is the first half of the workshop's
//! interaction, and it is deliberately small: a click is hit-tested against the
//! placed tree, and the two widgets the shell's own screens use for selection —
//! a `CheckBox` and the `OptionGroup` that holds it — have the state machine the
//! shipped files imply.
//!
//! ## A click is hit-tested against the placed tree, front to back
//!
//! [`hit`] walks the tree the way it is *drawn*, in reverse: the last thing
//! drawn is the first thing a click lands on. Within a subtree the children come
//! before their parent, so a click on a button's caption finds the caption and
//! the caller can walk up to the button — which is what a GUI does, and what
//! [`hit`]'s path return lets it do. A widget outside its clip cannot be hit: a
//! scrolled list's cells stop at the trough.
//!
//! The `PassMouse` field is **not** consulted, and that is the data's doing: it
//! is `true` on all 1,298 widgets in the corpus that carry it, so it
//! distinguishes nothing. It is a base-class field the writer always emits, not
//! a flag the screens vary.
//!
//! ## The state machine is measured off the corpus
//!
//! A `CheckBox` carries up to four `Image` children — the states — and exactly
//! one of them is `Visible`. Across every `OptionGroup` member in all 116
//! screens, with no exceptions:
//!
//! | member | state images | visible |
//! |---|---|---|
//! | the group's `Selection` | 4 | 2 |
//! | the group's `Selection` | 3 | 1 |
//! | any other | 4 or 3 | 0 |
//!
//! So the *unselected* look is always the first image, and the *selected* look is
//! the second-to-last — the last being the disabled state the binary names
//! (`Normal`, `Rollover`, `Clicked`, `Disabled` sit together in its schema). That
//! is the whole rule: [`set_state`] shows `0` or `len - 2`, and [`select_option`]
//! applies it to a group by flipping `Checked` and the images' `Visible`.

use ra2fmt::gib::{Widget, WidgetKind};

use crate::{draw_order, Frame, Placement};

impl Frame {
    /// Whether a point is inside the rectangle. The right and bottom edges are
    /// outside it, so two rectangles that abut do not both claim the boundary.
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
}

/// The widget a click at `x, y` lands on, as a path from the roots.
///
/// `[]` is a root itself; `[2, 0]` is the first child of the third root. Only
/// widgets that are `Visible` and inside their clip can be hit.
pub fn hit(roots: &[Placement<'_>], x: f32, y: f32) -> Option<Vec<usize>> {
    for index in front_to_back(roots.iter().map(|p| p.widget.child_order())) {
        let mut path = vec![index];
        if let Some(found) = hit_at(&roots[index], x, y, &mut path) {
            return Some(found);
        }
    }
    None
}

/// Indices ordered front to back: `ChildOrder` ascending is back to front, and
/// ties keep file order, so reversing it gives what is drawn last first.
fn front_to_back(orders: impl Iterator<Item = i32>) -> Vec<usize> {
    let mut order: Vec<(i32, usize)> = orders.enumerate().map(|(i, o)| (o, i)).collect();
    // Higher `ChildOrder` is drawn later and so is in front; where two share it,
    // the **earlier** record is in front — see `draw_order_parented`.
    order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    order.into_iter().map(|(_, i)| i).collect()
}

fn hit_at(p: &Placement<'_>, x: f32, y: f32, path: &mut Vec<usize>) -> Option<Vec<usize>> {
    for index in front_to_back(p.children.iter().map(|c| c.widget.child_order())) {
        path.push(index);
        if let Some(found) = hit_at(&p.children[index], x, y, path) {
            return Some(found);
        }
        path.pop();
    }
    let inside = p.frame.contains_point(x, y) && p.clip.is_none_or(|c| c.contains_point(x, y));
    // A hit does not stop at the base `Widget`. It is the *abstract* class — the
    // one kind the writer leaves headerless, whose only fields are its note and
    // `PassMouse` — so a record of it is a container, with no class behaviour to
    // run. The data draws them over controls on purpose: `ch_stepbuttons.gib`'s
    // `step 1 button`, `step 2 button` and `snap checkbox` each have a nameless
    // `Widget` a pixel larger drawn over them, holding the `1` and the `2`, and
    // `chassis.gib` draws its two notes over its two buttons. Refusing to stop at
    // one lets the search fall through to what is behind it, which is what makes
    // those controls clickable at all.
    (p.widget.visible() && inside && can_take_a_click(&p.widget.kind)).then(|| path.clone())
}

/// Whether a class can be what a click *lands on*.
///
/// A layout container cannot: it has no behaviour of its own, so a hit passes
/// through it to whatever is behind. The **measured** case is the base `Widget`,
/// the class the writer leaves headerless — `botlab/ch_stepbuttons.gib` draws a
/// nameless one over each of its step buttons, `chassis.gib` draws its notes over
/// its two — and a hit that stopped there would make those controls unclickable.
fn can_take_a_click(kind: &WidgetKind) -> bool {
    !matches!(kind, WidgetKind::Widget)
}

/// The widget a [`hit`] path names, whose first element indexes the `roots`
/// slice rather than a widget's children.
pub fn at_roots<'a>(roots: &'a [Placement<'a>], path: &[usize]) -> Option<&'a Widget> {
    let first = *path.first()?;
    at(roots.get(first)?.widget, &path[1..])
}

/// The widget a path names, relative to one root's own children.
pub fn at<'a>(root: &'a Widget, path: &[usize]) -> Option<&'a Widget> {
    let mut widget = root;
    for step in path {
        widget = widget.children.get(*step)?;
    }
    Some(widget)
}

/// The same, mutably.
pub fn at_mut<'a>(root: &'a mut Widget, path: &[usize]) -> Option<&'a mut Widget> {
    let mut widget = root;
    for step in path {
        widget = widget.children.get_mut(*step)?;
    }
    Some(widget)
}

/// Set a `CheckBox`'s state: `Checked` for the record, and the state image that
/// matches (see the module notes: `0` unselected, `len - 2` selected).
///
/// Returns whether anything changed.
pub fn set_state(widget: &mut Widget, selected: bool) -> bool {
    if widget.kind != WidgetKind::CheckBox {
        return false;
    }
    let count = widget.children.len();
    if count == 0 {
        return false;
    }
    let want = if selected { count - 2 } else { 0 };
    let mut changed = false;
    for (index, child) in widget.children.iter_mut().enumerate() {
        let show = index == want;
        if child.visible() != show {
            set_field(child, "Visible", if show { "true" } else { "false" });
            changed = true;
        }
    }
    let checked = selected.to_string();
    if field_of(widget, "Checked").is_none_or(|c| c != checked) {
        set_field(widget, "Checked", &checked);
        changed = true;
    }
    changed
}

/// Select `child` in the option group called `group`: the group's `Selection`
/// is set, and every `CheckBox` under it is put into its selected or unselected
/// state.
///
/// Returns the names that changed.
pub fn select_option(root: &mut Widget, group: &str, child: &str) -> Vec<String> {
    let mut changed = Vec::new();
    if !select_in(root, group, child, &mut changed) {
        return Vec::new();
    }
    changed
}

fn select_in(widget: &mut Widget, group: &str, child: &str, changed: &mut Vec<String>) -> bool {
    if widget.name() == group && widget.kind == WidgetKind::OptionGroup {
        set_field(widget, "Selection", child);
        // A `CheckBox`'s own children are its state images, so collect the
        // indices first and then touch each one.
        let targets: Vec<usize> = (0..widget.children.len())
            .filter(|i| widget.children[*i].kind == WidgetKind::CheckBox)
            .collect();
        for index in targets {
            let selected = widget.children[index].name() == child;
            let name = widget.children[index].name().to_string();
            if set_state(&mut widget.children[index], selected) {
                changed.push(name);
            }
        }
        return true;
    }
    widget
        .children
        .iter_mut()
        .any(|c| select_in(c, group, child, changed))
}

/// A widget's field as text.
pub fn field_of<'a>(widget: &'a Widget, name: &str) -> Option<&'a str> {
    widget.field(name).map(|f| f.text())
}

/// Replace a field's value, adding the field if the record does not carry it.
pub fn set_field(widget: &mut Widget, name: &str, value: &str) {
    if let Some(field) = widget.field_mut(name) {
        field.raw = value.to_string();
        return;
    }
    widget.fields.push(ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw: value.to_string(),
    });
}

/// Every widget of a screen in draw order, back to front — the order a click
/// tests in reverse.
pub fn draw_order_of<'a>(roots: &'a [Placement<'a>]) -> Vec<&'a Placement<'a>> {
    draw_order(roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve, DESIGN_SCREEN};
    use ra2fmt::gib::Gib;

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

    fn screen(relative: &str) -> Option<Gib> {
        let ui = ui_root()?;
        std::fs::read_to_string(ui.join(relative))
            .ok()
            .and_then(|t| ra2fmt::gib::parse(&t).ok())
    }

    /// The state rule, re-derived from the file rather than trusted: the
    /// group's `Selection` shows the second-to-last state image and every other
    /// member shows the first.
    #[test]
    fn the_shipped_selection_is_the_second_to_last_state_image() {
        let Some(gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let mut groups = 0;
        for w in gib.walk() {
            if w.kind != WidgetKind::OptionGroup {
                continue;
            }
            groups += 1;
            let selected = field_of(w, "Selection").unwrap_or("").to_string();
            for child in w.children.iter().filter(|c| c.kind == WidgetKind::CheckBox) {
                let images: Vec<&Widget> = child
                    .children
                    .iter()
                    .filter(|c| c.kind == WidgetKind::Image)
                    .collect();
                let visible: Vec<usize> = images
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.visible())
                    .map(|(n, _)| n)
                    .collect();
                let want = if child.name() == selected {
                    images.len() - 2
                } else {
                    0
                };
                assert_eq!(
                    visible,
                    vec![want],
                    "{} should show image {want}",
                    child.name()
                );
            }
        }
        assert_eq!(groups, 1, "the toolbar's own option group");
    }

    /// A click lands on the widget drawn last, and on the innermost one under
    /// the point — the caption inside a button rather than the button itself.
    #[test]
    fn a_click_hits_the_innermost_widget_drawn_under_it() {
        let Some(gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let roots = resolve(&gib, DESIGN_SCREEN);
        // `chassis button` is the second tab, placed at 431,37 100x24.
        let path = hit(&roots, 437.0, 45.0).expect("a click on the chassis tab");
        let widget = at_roots(&roots, &path).unwrap();
        assert!(
            widget.kind == WidgetKind::Image || widget.kind == WidgetKind::CheckBox,
            "hit {} {}",
            widget.kind.name(),
            widget.name()
        );
        assert!(
            path.len() >= 2,
            "the innermost widget under the point, not the root: {path:?}"
        );
        // Away from the toolbar the hit is the screen behind it, never a tab.
        let elsewhere = hit(&roots, 20.0, 900.0).expect("the screen itself");
        assert!(!at_roots(&roots, &elsewhere)
            .unwrap()
            .name()
            .ends_with("button"));
        // Right of the screen: nothing at all.
        assert!(hit(&roots, 2000.0, 900.0).is_none());
    }

    /// Clicking a tab makes it the group's selection and flips the two images —
    /// the change is in the *document*, which is where the renderer reads it.
    #[test]
    fn selecting_a_tab_flips_the_state_images() {
        let Some(mut gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            ra2fmt::gib::Block::Widget(w) => Some(w),
            _ => None,
        });
        let root = root.unwrap();

        let changed = select_option(root, "blab option group", "wiring button");
        assert_eq!(changed.len(), 2, "overview and wiring both change");
        // The group says so, and the images do.
        let group = crate::workshop::find(root, "blab option group").unwrap();
        assert_eq!(field_of(group, "Selection"), Some("wiring button"));
        let state = |name: &str| {
            let button = crate::workshop::find(root, name).unwrap();
            let images: Vec<&Widget> = button
                .children
                .iter()
                .filter(|c| c.kind == WidgetKind::Image)
                .collect();
            let visible = images.iter().position(|i| i.visible()).unwrap();
            (field_of(button, "Checked").unwrap().to_string(), visible)
        };
        assert_eq!(state("wiring button"), ("true".to_string(), 2));
        assert_eq!(state("overview button"), ("false".to_string(), 0));
        // Untouched members keep the state they had.
        assert_eq!(state("chassis button"), ("false".to_string(), 0));
    }

    /// `set_state` leaves a widget that is not a `CheckBox` alone.
    #[test]
    fn only_check_boxes_have_states() {
        let Some(mut gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            ra2fmt::gib::Block::Widget(w) => Some(w),
            _ => None,
        });
        assert!(!set_state(root.unwrap(), true));
    }
}
