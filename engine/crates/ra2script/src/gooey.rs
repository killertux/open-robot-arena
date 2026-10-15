//! A stand-in for the game's `Gooey` debug-window module.
//!
//! Every `AI/*.py` opens with `import Gooey`, and the shipped `Scripts/Gooey.py`
//! reaches the engine's UI module (`giScript`), which is M5. The debug windows
//! are only *built* when `plus.isDebugMode()` — i.e. when `RA2_DEBUG` is set —
//! so a stub that accepts the calls keeps the import working without pretending
//! to have a UI. The calls are recorded, as everything else outside M4 is.

use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::PyModule;

/// `Gooey.Plain(name, x, y, w, h)` — a debug window.
#[pyclass(name = "Plain", module = "Gooey")]
#[derive(Default)]
pub struct DebugWindow {
    texts: BTreeMap<String, Py<TextBox>>,
}

#[pymethods]
impl DebugWindow {
    #[new]
    #[pyo3(signature = (_name, _x, _y, _w, _h))]
    fn new(_name: &str, _x: f32, _y: f32, _w: f32, _h: f32) -> Self {
        crate::state().defer("Gooey debug windows (M5)");
        DebugWindow::default()
    }

    #[pyo3(signature = (name, _x, _y, _w, _h))]
    fn addText(
        &mut self,
        py: Python<'_>,
        name: &str,
        _x: f32,
        _y: f32,
        _w: f32,
        _h: f32,
    ) -> Py<TextBox> {
        let box_ = Py::new(py, TextBox::default()).expect("a text box allocation");
        self.texts.insert(name.to_string(), box_.clone_ref(py));
        box_
    }

    fn get(&self, py: Python<'_>, name: &str) -> Py<TextBox> {
        match self.texts.get(name) {
            Some(t) => t.clone_ref(py),
            None => Py::new(py, TextBox::default()).expect("a text box allocation"),
        }
    }

    fn Remove(&self) {}
}

/// A line of debug text.
#[pyclass(name = "TextBox", module = "Gooey")]
#[derive(Default)]
pub struct TextBox {
    #[pyo3(get)]
    text: String,
}

#[pymethods]
impl TextBox {
    fn setText(&mut self, text: &str) {
        self.text = text.to_string();
    }

    fn GetText(&self) -> String {
        self.text.clone()
    }

    fn setColor(&self, _r: i32, _g: i32, _b: i32) {}
}

/// Install the stub, unless the real module is already present.
pub fn install(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if modules.contains("Gooey")? {
        return Ok(());
    }
    let m = PyModule::new(py, "Gooey")?;
    m.add_class::<DebugWindow>()?;
    m.add_class::<TextBox>()?;
    modules.set_item("Gooey", &m)?;
    Ok(())
}
