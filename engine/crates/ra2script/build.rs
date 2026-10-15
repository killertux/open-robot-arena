//! Build glue for embedding CPython.
//!
//! `pyo3` asks the linker for `-lpython3.12`, which needs the `libpython3.12.so`
//! *development* symlink. Several distributions (Ubuntu's `libpython3.12t64`
//! without `python3.12-dev`, for instance) ship only the versioned soname
//! `libpython3.12.so.1.0`, and the link then fails with
//! `unable to find library -lpython3.12`.
//!
//! Rather than require a dev package, point the linker at a directory holding
//! the missing symlink. Nothing is written outside the build directory, and on a
//! system that does have the dev package this does nothing.

use std::path::{Path, PathBuf};
use std::process::Command;

fn sysconfig(key: &str) -> Option<String> {
    let exe = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let out = Command::new(exe)
        .args([
            "-c",
            &format!("import sysconfig; print(sysconfig.get_config_var({key:?}) or '')"),
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let Some(dir) = sysconfig("LIBDIR") else {
        return;
    };
    if sysconfig("Py_ENABLE_SHARED").as_deref() != Some("1") {
        // Static Python: pyo3's own flags stand.
        return;
    }
    let want = match sysconfig("LDLIBRARY") {
        Some(l) => l, // e.g. libpython3.12.so
        None => return,
    };
    let dir = PathBuf::from(dir);
    if dir.join(&want).exists() {
        return; // a dev install; nothing to do
    }
    let Some(soname) = sysconfig("INSTSONAME") else {
        return;
    };
    let real = dir.join(&soname);
    if !real.exists() {
        return;
    }

    let Ok(out) = std::env::var("OUT_DIR") else {
        return;
    };
    let link_dir = Path::new(&out).join("pylink");
    if std::fs::create_dir_all(&link_dir).is_err() {
        return;
    }
    let link = link_dir.join(&want);
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    if std::os::unix::fs::symlink(&real, &link).is_err() {
        return;
    }
    #[cfg(not(unix))]
    let _ = std::fs::copy(&real, &link);

    println!("cargo:rustc-link-search=native={}", link_dir.display());
}
