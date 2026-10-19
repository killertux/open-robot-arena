//! Boot the script layer with an arena and run a snippet of Python against it.
//!
//! ```text
//! pyprobe <game root> <arena.py> <python code>
//! ```
//!
//! A diagnostic for the AI's navigation queries (`Arenas.currentArena.GetPath`,
//! `IsStraightPathClear`, ...): the snippet runs after the arena and the AI
//! modules are loaded, exactly as a match would have them.

use pyo3::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 3 {
        eprintln!("usage: pyprobe <game root> <arena.py> <python code>");
        std::process::exit(2);
    }
    let _guard = ra2script::lock();
    let _host = ra2script::ScriptHost::boot_with_scripts(&a[0], Some(std::path::Path::new(&a[1])))?;
    Python::with_gil(|py| -> PyResult<()> {
        let code = std::ffi::CString::new(a[2].clone()).expect("no NUL in the snippet");
        py.run(&code, None, None)
    })?;
    Ok(())
}
