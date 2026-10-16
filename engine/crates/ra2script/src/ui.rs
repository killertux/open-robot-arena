//! The UI a script builds.
//!
//! `Scripts/Gooey.py` — the shipped one — is a thin wrapper over three `plus`
//! calls, and the AI scripts build their debug windows with it:
//!
//! ```text
//! self.debug = Gooey.Plain("watch", 0, 75, 100, 75)     # plus.addWindow
//! tbox = self.debug.addText("line0", 0, 0, 100, 15)     # plus.addText
//! tbox.setText("Throttle")
//! ```
//!
//! What that builds here is a **real widget tree** — the same
//! [`ra2fmt::gib::Widget`] a `.gib` screen is made of — so `ra2ui` places it and
//! `ra2-render` draws it exactly as it draws a screen. A script's window is a
//! `Window` at its `Position`/`Size`, and each line is a `Text` inside it
//! carrying `_Font` `fg_mcn14`, `_Text` the string and `_ForeColor` what
//! `setColor` asked for — the same three fields a screen's label carries, which
//! is why the same renderer draws both.
//!
//! The scripts are Python 2.2 and this is not a re-implementation of them: the
//! calls arrive through the shipped `Gooey.py`, and this is only what it wraps.

use std::sync::Mutex;

use pyo3::prelude::*;
use ra2fmt::gib::{Field, Widget, WidgetKind};

/// The face the shell's own debug text uses. The engine's markup strings name
/// it too: `<text font fg_mcn14 color 0 0 0 1>`.
const DEBUG_FACE: &str = "fg_mcn14";

struct Node {
    widget: Widget,
    parent: Option<usize>,
    children: Vec<usize>,
}

struct Ui {
    nodes: Vec<Node>,
}

impl Ui {
    const fn new() -> Ui {
        Ui { nodes: Vec::new() }
    }

    fn push(&mut self, widget: Widget, parent: Option<usize>) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            widget,
            parent,
            children: Vec::new(),
        });
        if let Some(parent) = parent {
            self.nodes[parent].children.push(id);
        }
        id
    }

    /// The tree rooted at `id`, children nested.
    fn tree(&self, id: usize) -> Widget {
        let mut widget = self.nodes[id].widget.clone();
        widget.children = self.nodes[id]
            .children
            .iter()
            .map(|child| self.tree(*child))
            .collect();
        widget
    }
}

static UI: Mutex<Ui> = Mutex::new(Ui::new());

fn field(name: &str, raw: impl Into<String>) -> Field {
    Field {
        name: Some(name.to_string()),
        raw: raw.into(),
    }
}

/// `plus.addWindow(name, x, y, w, h)` — a debug window. Returns its id.
pub fn add_window(name: &str, x: f32, y: f32, w: f32, h: f32) -> usize {
    let widget = Widget {
        kind: WidgetKind::Window,
        fields: vec![
            field("Name", name),
            field("Position", format!("{x} 0 {y} 0")),
            field("Size", format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "0"),
            // A window clips what its children draw, as every window in the
            // corpus does.
            field("ClipMode", "1"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    };
    UI.lock().expect("the UI lock").push(widget, None)
}

/// `plus.addText(window, x, y, w, h)` — one line inside a window. Returns its id.
pub fn add_text(window: usize, x: f32, y: f32, w: f32, h: f32) -> usize {
    let widget = Widget {
        kind: WidgetKind::Text,
        fields: vec![
            field("_Font", DEBUG_FACE),
            field("_Text", ""),
            field("_ForeColor", "1 1 1 1"),
            field("Position", format!("{x} 0 {y} 0")),
            field("Size", format!("{w} 0 {h} 0")),
            field("Visible", "true"),
            field("ChildOrder", "0"),
            field("ClipMode", "0"),
        ],
        attachments: Vec::new(),
        children: Vec::new(),
    };
    UI.lock().expect("the UI lock").push(widget, Some(window))
}

/// Set a line's text — what `tbox.setText("Throttle")` does.
pub fn set_text(id: usize, text: &str) {
    let mut ui = UI.lock().expect("the UI lock");
    if let Some(node) = ui.nodes.get_mut(id) {
        if let Some(f) = node
            .widget
            .fields
            .iter_mut()
            .find(|f| f.name.as_deref() == Some("_Text"))
        {
            f.raw = text.to_string();
        }
    }
}

/// Set a line's colour — `setColor(r, g, b)`, in the 0..1 units the layout uses.
pub fn set_color(id: usize, r: f32, g: f32, b: f32) {
    let mut ui = UI.lock().expect("the UI lock");
    if let Some(node) = ui.nodes.get_mut(id) {
        if let Some(f) = node
            .widget
            .fields
            .iter_mut()
            .find(|f| f.name.as_deref() == Some("_ForeColor"))
        {
            f.raw = format!("{r} {g} {b} 1");
        }
    }
}

/// A line's text, as `GetText` reports it.
pub fn text_of(id: usize) -> Option<String> {
    let ui = UI.lock().expect("the UI lock");
    let node = ui.nodes.get(id)?;
    node.widget.field("_Text").map(|f| f.text().to_string())
}

/// Every window a script has made, newest last, as widget trees.
pub fn windows() -> Vec<Widget> {
    let ui = UI.lock().expect("the UI lock");
    (0..ui.nodes.len())
        .filter(|id| ui.nodes[*id].parent.is_none())
        .map(|id| ui.tree(id))
        .collect()
}

/// Build a debug window the way the game's own scripts do — through the shipped
/// `Gooey.Plain` — with one line per string.
///
/// This is the round trip in one call, and it keeps `pyo3` out of the caller:
/// the game's Python builds the widgets (via `plus.addWindow`/`addText`), and
/// [`windows`] then hands back the tree to place and draw. The shell has to have
/// booted.
pub fn debug_window(name: &str, x: f32, y: f32, w: f32, h: f32, lines: &[&str]) -> PyResult<()> {
    Python::with_gil(|py| {
        let gooey = PyModule::import(py, "Gooey")?;
        let window = gooey.call_method1("Plain", (name, x, y, w, h))?;
        for (i, text) in lines.iter().enumerate() {
            let line = window.call_method1(
                "addText",
                (
                    format!("line{i}"),
                    8.0,
                    6.0 + 20.0 * i as f32,
                    w - 16.0,
                    18.0,
                ),
            )?;
            line.call_method1("setText", (*text,))?;
        }
        Ok(())
    })
}

/// Forget everything, for the next run.
pub fn clear() {
    UI.lock().expect("the UI lock").nodes.clear();
}

// ---------------------------------------------------------------------------
// what `plus` hands back
// ---------------------------------------------------------------------------

/// `plus.addWindow` — the handle `Gooey.Plain` keeps as `self.window`.
#[pyclass(name = "Window", module = "plus")]
pub struct Window {
    pub id: usize,
}

#[pymethods]
impl Window {
    fn Remove(&self) {
        crate::ui::remove(self.id);
    }

    #[getter]
    fn name(&self) -> String {
        String::new()
    }
}

/// `plus.addText` — the handle the AI scripts call `setText` on.
#[pyclass(name = "Text", module = "plus")]
pub struct Text {
    pub id: usize,
}

#[pymethods]
impl Text {
    fn setText(&mut self, text: &str) {
        crate::ui::set_text(self.id, text);
    }

    fn GetText(&self) -> String {
        crate::ui::text_of(self.id).unwrap_or_default()
    }

    fn setColor(&mut self, r: f32, g: f32, b: f32) {
        crate::ui::set_color(self.id, r, g, b);
    }
}

/// Take a widget out of the tree, its children with it.
pub fn remove(id: usize) {
    let mut ui = UI.lock().expect("the UI lock");
    fn drop_at(ui: &mut Ui, id: usize) {
        let children = ui.nodes[id].children.clone();
        for child in children {
            drop_at(ui, child);
        }
        ui.nodes[id].children.clear();
    }
    if id < ui.nodes.len() {
        drop_at(&mut ui, id);
        let parent = ui.nodes[id].parent;
        if let Some(parent) = parent {
            ui.nodes[parent].children.retain(|c| *c != id);
        }
        ui.nodes[id].parent = None;
    }
}
