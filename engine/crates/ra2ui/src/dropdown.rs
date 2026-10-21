//! The `DropDown` control's open list.
//!
//! A shipped `DropDown` record (`DownButton 0`, `List 10`, `Text 11`) is a
//! 20-pixel field with a `Down Button`, a `TextBox` for the current value and a
//! `ListBox` that is empty in the file: the exe fills it when the owning window
//! is constructed (`FUN_004733f0` fills the match-time list, `FUN_004a00b0` the
//! resolution list, `FUN_004a0400` the camera list) and opens it when the field
//! or its arrow is clicked. Choosing a row copies its text into the field and
//! calls the control's selection handler (`FUN_00474330` for the match time).
//!
//! This module keeps the open state in the document itself (fields on the
//! dropdown and on the widgets it lifts), so any screen's click handler can
//! drive a dropdown without holding state of its own:
//!
//! * [`open`] fills the list, hangs it below the field and lifts the dropdown
//!   and every ancestor above their siblings with no clip (the list is
//!   100 pixels tall and the field's own groups are 18 pixels), remembering
//!   what it changed;
//! * [`close`] puts all of that back and hides the list;
//! * [`pick`] is [`close`] plus the chosen text in the field.

use ra2fmt::gib::{AttachmentKind, Block, Gib, Widget, WidgetKind};

use crate::input::{field_of, scroll_rows, select_cell, set_field, text_rows};
use crate::{Placement, ScrollBar};

/// The open dropdown's name, when one is open in this document.
pub fn open_name(gib: &Gib) -> Option<String> {
    gib.walk()
        .find(|w| w.kind == WidgetKind::DropDown && field_of(w, "_Open") == Some("true"))
        .map(|w| w.name().to_string())
}

/// Whether the dropdown called `name` is open.
pub fn is_open(gib: &Gib, name: &str) -> bool {
    open_name(gib).as_deref() == Some(name)
}

/// Call `f` on `name` and every widget above it, deepest first. Returns
/// whether the dropdown was found under `widget`.
fn on_chain(widget: &mut Widget, name: &str, f: &mut dyn FnMut(&mut Widget)) -> bool {
    if widget.kind == WidgetKind::DropDown && widget.name() == name {
        f(widget);
        return true;
    }
    let mut found = false;
    for child in widget.children.iter_mut() {
        found |= on_chain(child, name, f);
    }
    if found {
        f(widget);
    }
    found
}

fn on_gib_chain(gib: &mut Gib, name: &str, f: &mut dyn FnMut(&mut Widget)) -> bool {
    let mut found = false;
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(root) = block {
            found |= on_chain(root, name, f);
        }
    }
    found
}

fn list_of(dropdown: &mut Widget) -> Option<&mut Widget> {
    dropdown
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::ListBox)
}

/// The row text a dropdown shows in its field.
pub fn text(gib: &Gib, name: &str) -> Option<String> {
    let dropdown = gib
        .walk()
        .find(|w| w.kind == WidgetKind::DropDown && w.name() == name)?;
    first_text(dropdown)
        .and_then(|t| t.field("_Text"))
        .map(|f| f.text().to_string())
}

fn first_text(widget: &Widget) -> Option<&Widget> {
    // The list's own rows are `Text` too; the field's text is the one under
    // the dropdown's `TextBox`.
    widget
        .children
        .iter()
        .filter(|c| c.kind == WidgetKind::TextBox)
        .find_map(|b| b.children.iter().find(|c| c.kind == WidgetKind::Text))
}

/// Put `value` in the dropdown's field.
pub fn set_text(gib: &mut Gib, name: &str, value: &str) {
    on_gib_chain_only(gib, name, &mut |dropdown| {
        if let Some(text) = dropdown
            .children
            .iter_mut()
            .filter(|c| c.kind == WidgetKind::TextBox)
            .find_map(|b| b.children.iter_mut().find(|c| c.kind == WidgetKind::Text))
        {
            set_field(text, "_Text", value);
        }
    });
}

fn on_gib_chain_only(gib: &mut Gib, name: &str, f: &mut dyn FnMut(&mut Widget)) {
    let mut done = false;
    on_gib_chain(gib, name, &mut |w| {
        if !done && w.kind == WidgetKind::DropDown && w.name() == name {
            f(w);
            done = true;
        }
    });
}

/// Open the list: `items` as rows, `selected` highlighted, hung below the
/// field. Does nothing when the document has no such dropdown.
pub fn open(gib: &mut Gib, name: &str, items: &[String], selected: usize) -> bool {
    let mut height = 20.0;
    let mut opened = false;
    let found = on_gib_chain(gib, name, &mut |w| {
        let is_dropdown = w.kind == WidgetKind::DropDown && w.name() == name;
        let was_clip = field_of(w, "ClipMode").unwrap_or("0").to_string();
        let was_order = field_of(w, "ChildOrder").unwrap_or("-1").to_string();
        set_field(w, "_WasClip", &was_clip);
        set_field(w, "_WasOrder", &was_order);
        set_field(w, "ClipMode", "0");
        set_field(w, "ChildOrder", "1000");
        if is_dropdown {
            if let Some(h) = w.size().map(|s| s.second.amount) {
                height = h;
            }
            set_field(w, "_Open", "true");
            if let Some(list) = list_of(w) {
                set_field(list, "Position", &format!("0 0 {height} 0"));
                set_field(list, "ChildOrder", "100");
                set_field(list, "Visible", "true");
                text_rows(list, items);
                set_field(list, "_ScrollTop", "0");
                let row = selected.min(items.len().saturating_sub(1));
                select_cell(list, row, 0);
                // Keep the highlighted row on screen.
                let visible = crate::input::visible_rows(list).max(1);
                let top = (row + 1).saturating_sub(visible);
                scroll_rows(list, top);
            }
            opened = true;
        }
    });
    found && opened
}

/// Close the list and give back the clip and draw order [`open`] changed.
pub fn close(gib: &mut Gib, name: &str) {
    on_gib_chain(gib, name, &mut |w| {
        if let Some(clip) = field_of(w, "_WasClip").map(str::to_string) {
            set_field(w, "ClipMode", &clip);
        }
        if let Some(order) = field_of(w, "_WasOrder").map(str::to_string) {
            set_field(w, "ChildOrder", &order);
        }
        if w.kind == WidgetKind::DropDown && w.name() == name {
            set_field(w, "_Open", "false");
            if let Some(list) = list_of(w) {
                set_field(list, "Visible", "false");
                // The rows go with the list; the exe fills it again on open.
                text_rows(list, &[]);
            }
        }
    });
}

/// Choose row `row` of the open list: its text goes into the field and the
/// list closes. Returns the text.
pub fn pick(gib: &mut Gib, name: &str, items: &[String], row: usize) -> Option<String> {
    let value = items.get(row)?.clone();
    close(gib, name);
    set_text(gib, name, &value);
    Some(value)
}

/// What a click means to a dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// The field or its arrow: toggles the list.
    Field,
    /// A row of the open list (counting from the first item).
    Row(usize),
    /// The list's scroll bar asked for this top row.
    Scroll(usize),
    /// Elsewhere while the list is open: closes it (the click is consumed).
    Outside,
}

/// The dropdown a click at `x, y` operates, and which part of it.
///
/// `hit` is the control path [`crate::input::hit_control`] found for the
/// point. An open list's own scroll bar is an attachment, which a control hit
/// never reports, so it is looked for first.
pub fn click_part(
    placed: &[Placement<'_>],
    hit: Option<&[usize]>,
    x: f32,
    y: f32,
) -> Option<(String, Part)> {
    // The open dropdown, if any.
    let open = placed.iter().flat_map(|p| p.walk()).find(|p| {
        p.widget.kind == WidgetKind::DropDown && field_of(p.widget, "_Open") == Some("true")
    });
    if let Some(open) = open {
        let name = open.widget.name().to_string();
        let list = open
            .children
            .iter()
            .find(|c| c.widget.kind == WidgetKind::ListBox);
        if let Some(list) = list {
            let count = crate::input::content_count(list.widget);
            let visible = crate::input::visible_rows(list.widget).max(1);
            let top = field_of(list.widget, "_ScrollTop")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            if let Some(target) = scrollbar_target(list, x, y, top, count.saturating_sub(visible)) {
                return Some((name, Part::Scroll(target)));
            }
            let on_list =
                list.frame.contains_point(x, y) && list.clip.is_none_or(|c| c.contains_point(x, y));
            if on_list {
                let height = crate::Grid::from_widget(list.widget)
                    .map(|g| g.row_height)
                    .filter(|h| *h > 0.0)
                    .unwrap_or(18.0);
                let row = ((y - list.frame.y) / height).floor().max(0.0) as usize + top;
                return if row < count {
                    Some((name, Part::Row(row)))
                } else {
                    Some((name, Part::Outside))
                };
            }
        }
        let on_field = hit.is_some_and(|path| {
            chain(placed, path)
                .iter()
                .any(|p| p.widget.kind == WidgetKind::DropDown && p.widget.name() == name)
        });
        return Some((name, if on_field { Part::Field } else { Part::Outside }));
    }
    let path = hit?;
    let dropdown = chain(placed, path)
        .into_iter()
        .rev()
        .find(|p| p.widget.kind == WidgetKind::DropDown)?;
    Some((dropdown.widget.name().to_string(), Part::Field))
}

fn chain<'a>(roots: &'a [Placement<'a>], path: &[usize]) -> Vec<&'a Placement<'a>> {
    let mut out = Vec::new();
    let Some(first) = path.first() else {
        return out;
    };
    out.push(&roots[*first]);
    for index in &path[1..] {
        let next = &out.last().expect("a root").children[*index];
        out.push(next);
    }
    out
}

/// Where a click on a list's own scroll bar asks the list to scroll.
///
/// The bar is the list's `+Scroller` attachment: its first and last parts are
/// the arrows and the track between them the third. A click on an arrow moves
/// one row; a click on the track jumps to that fraction of the content.
/// `None` when the point is not on this list's bar.
pub fn scrollbar_target(
    list: &Placement<'_>,
    x: f32,
    y: f32,
    top: usize,
    max: usize,
) -> Option<usize> {
    let bar = list
        .attachments
        .iter()
        .find(|a| a.attachment.kind == AttachmentKind::Scroller)?
        .body
        .walk()
        .find(|p| p.widget.kind == WidgetKind::ScrollBar)?;
    if !bar.frame.contains_point(x, y) || !bar.widget.visible() {
        return None;
    }
    let model = ScrollBar::from_widget(bar.widget);
    let less = bar.children.get(model.less)?.frame;
    let more = bar.children.get(model.more)?.frame;
    let parts = model.parts(bar.frame, less, more);
    let track = parts[2];
    if y < track.y {
        return Some(top.saturating_sub(1));
    }
    if y >= track.bottom() {
        return Some((top + 1).min(max));
    }
    if track.h <= 0.0 {
        return Some(top);
    }
    let fraction = ((y - track.y) / track.h).clamp(0.0, 1.0);
    Some((fraction * max as f32).round() as usize)
}

/// Scroll the open list to `top`.
pub fn scroll(gib: &mut Gib, name: &str, top: usize) {
    on_gib_chain_only(gib, name, &mut |dropdown| {
        if let Some(list) = list_of(dropdown) {
            scroll_rows(list, top);
        }
    });
}
