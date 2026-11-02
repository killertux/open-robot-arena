//! The screens an event click leads to: the join window (`teamhq/joinevent.gib`)
//! and the bracket (`teamhq/event_bracket.gib`).
//!
//! Recovered from `Robot Arena 2.exe`: the schedule's `event grid` is bound to
//! `FUN_00500c40` in the Team HQ constructor (`0x4fdc4e`). A click on an event
//! cell then decides by the career's current event (`FUN_00416940`, `-1` = none):
//!
//! * none: the modal `JoinEventWindow` (`FUN_00481b60`, `join event window`),
//!   filled for the clicked event and the roster, opens;
//! * the clicked event is the current one: the `EventBracketWindow`
//!   (`FUN_0046a500`, cached as `event bracket win`) is shown, or the repairs
//!   window (`FUN_004d5de0`, `repairs win`) when `FUN_005011e0` says the roster
//!   still has repairs due;
//! * another event is current: a message box `Robot Arena 2` / [`ONE_EVENT_AT_A_TIME`].
//!
//! The join window binds `cancel button` (closes), `enter event button`
//! (`FUN_00481f20`: with a robot chosen, close, build the bracket window for the
//! event and robot, hide the Team HQ and show the bracket) and `robot grid`
//! (`FUN_00482590`: the chosen robot's name goes to `robot name`; a robot that
//! `FUN_004fa3b0` rejects clears the name and disables the enter button).

use std::path::Path;

use ra2fmt::event::Event;
use ra2fmt::gib::{AttachmentKind, Block, Gib, Widget, WidgetKind};

use crate::hq::{BracketView, RobotRow};

/// The message box body (`0x6ff2f8`).
pub const ONE_EVENT_AT_A_TIME: &str =
    "Each team may enter one event at a time.  Finish your current event before beginning a new one.";
/// The message box caption (`0x6f0580`).
pub const CAPTION: &str = "Robot Arena 2";
/// The join window's robot cell template (a 90 x 90 preview cell, as the grid's
/// `RowHeight 90` / `ColWidths 90 17 ...` ask for).
pub const ROBOT_CELL: &str = "exhibition/choosebot_cell.gib";

fn find_mut_in<'a>(gib: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    gib.blocks.iter_mut().find_map(|b| match b {
        Block::Widget(w) => crate::workshop::find_mut(w, name),
        Block::Attachment(_) => None,
    })
}

/// [`crate::workshop::find_mut`] that also looks inside attachment bodies.
fn find_mut_attached<'a>(widget: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
    if widget.name() == name {
        return Some(widget);
    }
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = attachment.body.as_deref_mut() {
            if let Some(found) = find_mut_attached(body, name) {
                return Some(found);
            }
        }
    }
    for child in widget.children.iter_mut() {
        if let Some(found) = find_mut_attached(child, name) {
            return Some(found);
        }
    }
    None
}

fn set_value_in(gib: &mut Gib, name: &str, value: &str) {
    if let Some(widget) = find_mut_in(gib, name) {
        match widget
            .children
            .iter_mut()
            .find(|c| c.kind == WidgetKind::Text)
        {
            Some(text) => crate::input::set_field(text, "_Text", value),
            None => crate::input::set_field(widget, "_Text", value),
        }
    }
}

fn set_text_deep(widget: &mut Widget, text: &str) -> bool {
    if widget.kind == WidgetKind::Glyph {
        crate::input::set_field(widget, "_Text", text);
        return true;
    }
    widget.children.iter_mut().any(|c| set_text_deep(c, text))
}

fn set_display(widget: &mut Widget, text: &str) -> bool {
    if let Some(attachment) = widget
        .attachments
        .iter_mut()
        .find(|a| a.kind == AttachmentKind::DisplayBox)
    {
        if let Some(field) = attachment.field_mut("Display") {
            field.raw = crate::cells::markup_with_text(&field.raw, text);
        }
        if let Some(body) = &mut attachment.body {
            set_text_deep(body, text);
        }
        set_text_deep(widget, text);
        return true;
    }
    if set_text_deep(widget, text) {
        return true;
    }
    if widget.children.iter_mut().any(|c| set_display(c, text)) {
        return true;
    }
    widget
        .attachments
        .iter_mut()
        .filter_map(|a| a.body.as_mut())
        .any(|b| set_display(b, text))
}

/// Write `text` into the document's first `DisplayBox`.
pub fn set_first_display(gib: &mut Gib, text: &str) -> bool {
    gib.blocks.iter_mut().any(|b| match b {
        Block::Widget(w) => set_display(w, text),
        Block::Attachment(_) => false,
    })
}

/// The prose inside an `Events.txt` message: between the opening tag and `</text>`.
pub fn plain_text(markup: &str) -> String {
    let text = markup.find('>').map_or(markup, |at| &markup[at + 1..]);
    let text = text.strip_suffix("</text>").unwrap_or(text);
    text.replace("\\n", " ").trim().to_string()
}

/// `teamhq/event_bracket.gib` with the event's own name, description, art and
/// the career's bracket: round heading, the field and the next opponent.
pub fn bracket_screen(
    ui_root: &Path,
    event: &Event,
    bracket: Option<&BracketView>,
) -> Result<Gib, String> {
    bracket_screen_at(ui_root, event, bracket, [1280.0, 1024.0])
}

/// [`bracket_screen`] with the bracket graphic laid out for a screen of
/// `screen` pixels (`crate::hq::fill_bracket_at`).
pub fn bracket_screen_at(
    ui_root: &Path,
    event: &Event,
    bracket: Option<&BracketView>,
    screen: [f32; 2],
) -> Result<Gib, String> {
    let mut gib = crate::hq::load(ui_root, "teamhq/event_bracket.gib")?;
    crate::hq::fit_splits_in(&mut gib);
    if let Some(image) = find_mut_in(&mut gib, "event graphic") {
        crate::input::set_field(image, "_Filename", &event.art);
    }
    // The arena's preview, not the shipped `sample_arena.bmp` placeholder.
    // (`background img` hangs in a `Background` attachment's body, which a
    // plain child search never enters.)
    for block in gib.blocks.iter_mut() {
        if let Block::Widget(root) = block {
            if let Some(image) = find_mut_attached(root, "background img") {
                crate::input::set_field(image, "_Filename", &event.arena.1);
            }
        }
    }
    set_value_in(&mut gib, "event title", &event.name);
    set_first_display(&mut gib, &plain_text(&event.description));
    if let Some(bracket) = bracket {
        set_value_in(&mut gib, "round heading textbox", &bracket.round_label);
        crate::hq::fill_bracket_at(&mut gib, bracket, screen);
        if let Some(button) = find_mut_in(&mut gib, "play button") {
            let visible = if bracket.can_play { "true" } else { "false" };
            crate::input::set_field(button, "Visible", visible);
        }
    } else {
        set_value_in(&mut gib, "round heading textbox", "ROUND 1");
    }
    Ok(gib)
}

/// Show or hide a grid's `Sel Bar`.
pub fn show_selection_bar(grid: &mut Widget, shown: bool) {
    if let Some(bar) = grid.children.iter_mut().find(|c| c.name() == "Sel Bar") {
        crate::input::set_field(bar, "Visible", if shown { "true" } else { "false" });
    }
}

/// Fill the join window for `event` and the roster, with robot `selected`
/// chosen (`FUN_00482590`: its name in `robot name`, the enter button live).
pub fn fill_join_window(
    ui_root: &Path,
    gib: &mut Gib,
    event: &Event,
    robots: &[RobotRow],
    selected: Option<usize>,
) -> Result<(), String> {
    set_value_in(gib, "event name", &event.name);
    set_first_display(gib, &plain_text(&event.description));
    if let Some(image) = find_mut_in(gib, "preview img") {
        crate::input::set_field(image, "_Filename", &event.art);
    }
    let template = crate::cells::template(ui_root, ROBOT_CELL)?;
    if let Some(grid) = find_mut_in(gib, "robot grid") {
        let bars = grid.children.len();
        let made = crate::cells::fill(grid, &template, robots.len().min(6));
        for (cell, row) in grid.children[bars..].iter_mut().zip(robots) {
            if !row.preview.is_empty() {
                crate::cells::bind_art(cell, "bot preview", &row.preview);
            }
            // The weight icon is the class badge (`teamhq-event-join`: L blue, M green, H red).
            if let Some(icon) = crate::workshop::find_mut(cell, "weight icon") {
                let class = row.class.to_ascii_lowercase();
                let file = if class.starts_with("light") {
                    "exhibition/light_icon.bmp"
                } else if class.starts_with("heavy") {
                    "exhibition/heavy_icon.bmp"
                } else {
                    "exhibition/medium_icon.bmp"
                };
                crate::input::set_field(icon, "_Filename", file);
            }
        }
        let _ = made;
        if let Some(row) = selected {
            crate::input::select_cell(grid, 0, row);
        }
        // Live (`teamhq-event-join`): with no robot chosen the grid's `Sel Bar` (a column-1
        // frame in the file) is not drawn; the first pick shows it (`show_selection_bar`).
        show_selection_bar(grid, selected.is_some());
    }
    let name = selected
        .and_then(|i| robots.get(i))
        .map(|r| r.name.as_str())
        .unwrap_or("");
    set_value_in(gib, "robot name", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_events_message_loses_its_markup() {
        assert_eq!(
            plain_text("<text font=1>First\\nsecond </text>"),
            "First second"
        );
    }

    #[test]
    fn the_one_event_message_is_the_exes_string() {
        assert!(ONE_EVENT_AT_A_TIME.starts_with("Each team may enter one event at a time."));
    }
}
