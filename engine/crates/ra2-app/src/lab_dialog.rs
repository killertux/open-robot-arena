//! The Bot Lab's wiring windows (docs/36 section 9.27).
//!
//! Live, the Wiring tab works in two steps and both open a modal window over the lab:
//!
//! * dragging `SWITCH`, `BUTTON` or `ANALOG` from the strip above the controller onto a free cell
//!   opens that kind's properties window (`botlab/ctrl_<kind>_props.gib`): a `Control Name` box
//!   (`New Analog`, `New Switch`, `New Button`) and one or two input boxes that take the next key
//!   pressed (`Positive Axis` / `Negative Axis`, `Switch on/off:`, `Button on/off:`); OK adds the
//!   control to the cell, Cancel does not;
//! * with a control selected, a click on a component in the 3D view opens its connection window
//!   (`botlab/wiring_connection_<kind>.gib`): the component's name and picture, the control's
//!   keys and one action dropdown per input (`<none>` first, then the actions the control kind
//!   can give that component class); OK writes the bindings.
//!
//! This module holds the window's state and its edits; the click routing lives in
//! `build_mode`, which owns the lab.

use ra2fmt::gib::{Gib, Widget, WidgetKind};
use ra2ui::workshop::wiring::{Action, ControlKind};

/// Which window is open.
#[derive(Clone, Debug)]
pub enum DialogKind {
    /// A new control's properties.
    Props {
        kind: ControlKind,
        cell: [i32; 2],
        /// The name box's text.
        name: String,
        /// The DirectInput code of each input box.
        keys: [Option<i32>; 2],
        /// A double click on a control opened it: OK renames and re-keys that control.
        editing: bool,
    },
    /// `Delete Control`'s box (`Delete this input control?`, live a `Wiring` window whose body does
    /// not draw): Yes removes the control in `cell`, No leaves it.
    ConfirmDelete { cell: [i32; 2] },
    /// `Export Paint Template` (`botlab/export_texture.gib`): the file's name and whether the
    /// chassis' current texture is exported instead of a blank template.
    ExportTexture { name: String, current: bool },
    /// A control wired to a component.
    Wire {
        kind: ControlKind,
        cell: [i32; 2],
        component: usize,
        /// The actions the dropdowns offer (after `<none>`).
        actions: Vec<Action>,
        /// The chosen row of each input's dropdown (`0` is `<none>`).
        picks: [usize; 2],
    },
}

/// Where typing goes in a properties window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    None,
    Name,
    Key(usize),
}

/// A window and its state.
#[derive(Clone, Debug)]
pub struct LabDialog {
    pub gib: Gib,
    pub kind: DialogKind,
    pub focus: Focus,
}

/// The fragment of a properties window.
pub fn props_file(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Analog => "botlab/ctrl_analog_props.gib",
        ControlKind::Switch => "botlab/ctrl_switch_props.gib",
        ControlKind::Button => "botlab/ctrl_button_props.gib",
    }
}

/// The fragment of a connection window.
pub fn wire_file(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Analog => "botlab/wiring_connection_analog.gib",
        ControlKind::Switch => "botlab/wiring_connection_switch.gib",
        ControlKind::Button => "botlab/wiring_connection_button.gib",
    }
}

/// The name a new control of `kind` starts with (the window's own text).
pub fn default_name(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Analog => "New Analog",
        ControlKind::Switch => "New Switch",
        ControlKind::Button => "New Button",
    }
}

/// The names of the two dropdowns of a connection window.
pub const DROPDOWNS: [&str; 2] = ["input1 map", "input2 map"];

/// Write `text` into the widget called `name` and its text children.
pub fn set_text(gib: &mut Gib, name: &str, text: &str) {
    fn visit(widget: &mut Widget, name: &str, text: &str) -> bool {
        if widget.name() == name {
            fill(widget, text);
            return true;
        }
        widget.children.iter_mut().any(|c| visit(c, name, text))
    }
    fn fill(widget: &mut Widget, text: &str) {
        if matches!(widget.kind, WidgetKind::Text | WidgetKind::Glyph) {
            ra2ui::input::set_field(widget, "_Text", text);
        }
        for child in widget.children.iter_mut() {
            fill(child, text);
        }
    }
    for block in gib.blocks.iter_mut() {
        if let ra2fmt::gib::Block::Widget(root) = block {
            if visit(root, name, text) {
                return;
            }
        }
    }
}

/// The text of the widget called `name`.
pub fn text_of(gib: &Gib, name: &str) -> Option<String> {
    fn visit(widget: &Widget, name: &str) -> Option<String> {
        if widget.name() == name {
            return first_text(widget);
        }
        widget.children.iter().find_map(|c| visit(c, name))
    }
    fn first_text(widget: &Widget) -> Option<String> {
        if matches!(widget.kind, WidgetKind::Text | WidgetKind::Glyph) {
            if let Some(t) = ra2ui::input::field_of(widget, "_Text") {
                return Some(t.to_string());
            }
        }
        widget.children.iter().find_map(first_text)
    }
    gib.blocks.iter().find_map(|b| match b {
        ra2fmt::gib::Block::Widget(root) => visit(root, name),
        _ => None,
    })
}

/// The DirectInput scan code of a typed character, for the keys the game's controls use.
pub fn dik_for_char(c: char) -> Option<i32> {
    let c = c.to_ascii_uppercase();
    Some(match c {
        'Q' => 16,
        'W' => 17,
        'E' => 18,
        'R' => 19,
        'T' => 20,
        'Y' => 21,
        'U' => 22,
        'I' => 23,
        'O' => 24,
        'P' => 25,
        'A' => 30,
        'S' => 31,
        'D' => 32,
        'F' => 33,
        'G' => 34,
        'H' => 35,
        'J' => 36,
        'K' => 37,
        'L' => 38,
        'Z' => 44,
        'X' => 45,
        'C' => 46,
        'V' => 47,
        'B' => 48,
        'N' => 49,
        'M' => 50,
        ' ' => 57,
        '1'..='9' => 2 + (c as i32 - '1' as i32),
        '0' => 11,
        _ => return None,
    })
}

/// The scan code of a named non-character key (`Up`, `Down`, `Left`, `Right`, `Tab`, ...).
pub fn dik_for_name(name: &str) -> Option<i32> {
    Some(match name {
        "Up" => 200,
        "Down" => 208,
        "Left" => 203,
        "Right" => 205,
        "Tab" => 15,
        "LShift" => 42,
        "RShift" => 54,
        "LCtrl" => 29,
        _ => return None,
    })
}
