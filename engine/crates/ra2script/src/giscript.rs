//! The `giScript` host module — the engine's side of the shell's own Python.
//!
//! `Scripts/Main.py` opens by hooking `sys.stdout` and `sys.stderr` into it and
//! closes by registering `dispatchEvent` with it:
//!
//! ```text
//! import giScript
//! class giScript_stdout:
//!     def write(self, s): giScript.debug_stdout(s)
//! ...
//! giScript.register_function(dispatchEvent)
//! ```
//!
//! So everything the shell's Python prints comes through here — the game puts it
//! in its console; this keeps it in order, so a run can report what the shell
//! said — and the function it registers is how the engine calls back into that
//! Python for UI events. Both are recorded rather than simulated, and
//! [`dispatch_event`] calls the registered function the way the game's C++ does.

use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::PyModule;

struct Host {
    output: String,
    /// The function `giScript.register_function` was given.
    callback: Option<Py<PyAny>>,
}

impl Host {
    const fn new() -> Host {
        Host {
            output: String::new(),
            callback: None,
        }
    }
}

static HOST: Mutex<Host> = Mutex::new(Host::new());

/// Register the module, if it is not already in `sys.modules`.
pub fn register(py: Python<'_>) -> PyResult<()> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    if modules.contains("giScript")? {
        return Ok(());
    }
    let m = PyModule::new(py, "giScript")?;
    m.add(
        "__doc__",
        "The Robot Arena 2 shell's host module, as its own Python sees it.",
    )?;
    m.add_function(wrap_pyfunction!(debug_stdout, &m)?)?;
    m.add_function(wrap_pyfunction!(debug_stderr, &m)?)?;
    m.add_function(wrap_pyfunction!(register_function, &m)?)?;
    modules.set_item("giScript", &m)?;
    Ok(())
}

fn append(s: &str) {
    HOST.lock().expect("the host lock").output.push_str(s);
}

/// `giScript.debug_stdout(s)` — where the shell's `print` goes.
#[pyfunction]
fn debug_stdout(s: &str) {
    append(s);
}

/// `giScript.debug_stderr(s)` — where the shell's tracebacks go.
#[pyfunction]
fn debug_stderr(s: &str) {
    append(s);
}

/// `giScript.register_function(fn)` — the callback the engine calls for events.
#[pyfunction]
fn register_function(callback: Py<PyAny>) {
    HOST.lock().expect("the host lock").callback = Some(callback);
}

/// Everything the shell has printed, in order.
pub fn output() -> String {
    HOST.lock().expect("the host lock").output.clone()
}

/// Take the output so far, for a report.
pub fn take_output() -> String {
    std::mem::take(&mut HOST.lock().expect("the host lock").output)
}

/// Whether the shell registered its event callback yet.
pub fn has_callback() -> bool {
    HOST.lock().expect("the host lock").callback.is_some()
}

/// Call the shell's registered `dispatchEvent(event_name, data)`, the way the
/// engine's C++ does.
pub fn dispatch_event(py: Python<'_>, event_name: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
    let callback = HOST
        .lock()
        .expect("the host lock")
        .callback
        .as_ref()
        .map(|cb| cb.clone_ref(py));
    match callback {
        Some(cb) => {
            cb.bind(py).call1((event_name, data))?;
            Ok(())
        }
        None => Err(pyo3::exceptions::PyRuntimeError::new_err(
            "the shell has not registered its event callback",
        )),
    }
}

/// Forget everything, for the next run.
pub fn clear() {
    let mut host = HOST.lock().expect("the host lock");
    host.output.clear();
    host.callback = None;
}
