# 09 — Platforms: macOS and modern Windows (P3)

**Status: Linux x86_64 is built, tested and smoke-tested. macOS and Windows
are documented, not built and not tested.** Nothing in this file is a result on
those systems. It is the path, the configuration that was read, and the list of
what would have to be run before any claim could be made.

The only platform-motivated code change is a default output directory
(`std::env::temp_dir()` instead of the literal `/tmp`), because `/tmp` is a
drive-root path on Windows. Everything else in the engine is already
`cfg`-free at the points that matter: `wgpu::Backends::all()`, `Path::join`,
`RA2_GAME_DIR`, and a `.gib` lookup that normalises `\` and `/`. No untested
platform code was added.

## 1. The table

| | Linux x86_64 | macOS arm64 | macOS x86_64 | Windows x86_64 |
|---|---|---|---|---|
| **State** | **verified** (267 tests; tarball smoke) | not built | not built | not built |
| Rust target | `x86_64-unknown-linux-gnu` | `aarch64-apple-darwin` | `x86_64-apple-darwin` | `x86_64-pc-windows-msvc` |
| Toolchain | rustc/cargo 1.98.1 (measured) | stable + Xcode CLT | stable + Xcode CLT | stable MSVC + VS Build Tools |
| Jolt C++ (`oxijolt-sys` 1.1.0 + Jolt 5.6.0) | both paths run here: P2's build used the prebuilt archive, `check.sh` the vendored CMake build | **no archive for macOS in `prebuilt.txt` → CMake + vendored Jolt** | same | archive listed (MSVC 14.44, `MultiThreadedDLL`); CMake fallback |
| wgpu 30 backend | Vulkan (Mesa) | Metal | Metal | DX12 (Vulkan only if present) |
| Python 3.12 runtime | `libpython3.12.so.1.0` + stdlib | `libpython3.12.dylib` + stdlib | same | `python3.12.dll` + `python312.lib` + `Lib\` |
| Point `pyo3` at it | auto (`sysconfig`) | `PYO3_PYTHON` | `PYO3_PYTHON` | `PYO3_PYTHON` |
| Package | `.tar.gz` + `run.sh` (built) | `.app` skeleton + sign/notarise (**script written, never run**) | same | portable `.zip` layout (**script written, never run**) or MSI (**not written**) |
| Game found by | `--game`, `RA2_GAME_DIR`, launcher search | a launcher would pass `--game`/`RA2_GAME_DIR` | same | same |
| Verified here | build, tests, routes, boot-frame equality, missing-game errors, Python check, tarball | **nothing** | **nothing** | **nothing** |

Measured host for the verified column: Linux Mint 22.3 (Ubuntu 24.04 base),
x86_64, rustc 1.98.1, CPython 3.12.3, Mesa Vulkan. See
[`08-packaging.md`](08-packaging.md) §3 and §5.

Linux **package formats** beyond the tarball are recipes, not artifacts:
`packaging/flatpak/` (runtime 24.08, which carries CPython 3.12) and
`packaging/appimage/` (host CPython 3.12 required, not bundled). Both were
statically checked and neither was built — `flatpak-builder`,
`appimagetool`/`linuxdeploy` and the 24.08 SDK/runtime are absent here. See
[`08-packaging.md`](08-packaging.md) §6 for the exact gaps and commands.

## 2. What a build needs on each platform

Common to all three:

* **Rust stable.** The workspace pins no `rust-version`; 1.98.1 is the only
  version this project has been built with. `cargo build --release --offline`
  is the build.
* **CMake and a C++17 compiler** whenever `oxijolt-sys` does not use a prebuilt
  archive. It builds the vendored Jolt 5.6.0 through `vendor/joltc`'s
  `CMakeLists.txt`. Both paths have run on this Linux host: the original P2
  build used the prebuilt archive, and `tools/check.sh` (which exports
  `CARGO_NET_OFFLINE=true`) rebuilds the tarball from the vendored Jolt and
  passed the 267 tests and the smoke. That is Linux evidence for the CMake
  path, not macOS or Windows evidence. One AI assertion was re-windowed from 20 s to 30 s because the
  source build lands its first hit at 20.23 s; see `docs/08` §9.
* **A wgpu adapter, even offscreen.** `Renderer::new` asks for
  `Backends::all()` with no surface and `HighPerformance`. There is no window:
  `wgpu::InstanceDescriptor::new_without_display_handle()`.
* **CPython 3.12 exactly**, not "Python 3". `ra2script` depends on `pyo3` 0.23.5
  with `auto-initialize`, which links the shared library. The ABI is the minor
  version: a 3.13 runtime cannot satisfy a 3.12 link. `--no-python` skips
  booting `Scripts/Main.py`, not the link-time dependency.

### 2.1 Linux x86_64 (verified)

Ubuntu/Debian packages that were used: a Vulkan driver (`mesa-vulkan-drivers`
or the vendor driver), `cmake`, `build-essential`, `python3.12` and
`libpython3.12`. The linker needs `libpython3.12.so`; `ra2script/build.rs`
creates that name inside `OUT_DIR` from the versioned `libpython3.12.so.1.0`
when only the runtime is installed. `readelf -d` shows the runtime dependency:

```
$ readelf -d engine/target/release/ra2-app | grep NEEDED
  (NEEDED)  Shared library: [libpython3.12.so.1.0]
```

### 2.2 macOS

What is known from the sources, not from a run:

* **Two architectures, two builds.** Build arm64 on Apple Silicon and x86_64 on
  an Intel Mac (or with a matching cross toolchain). A `lipo` universal binary
  would need a universal `libpython` and a universal Jolt; nothing here has
  been tested as universal, so prefer one artifact per architecture.
* **Xcode Command Line Tools** provide clang and the SDK; **CMake** is needed
  because `oxijolt-sys`'s `prebuilt.txt` has no macOS entries (it lists
  `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` only). The vendored
  Jolt source is in the crate.
* **Python.** Use a CPython 3.12 whose architecture matches the Rust target.
  python.org's framework installer is universal2; Homebrew's `python@3.12` is
  per-prefix (`/opt/homebrew` arm64, `/usr/local` x86_64). Point `pyo3` at the
  executable:

  ```sh
  export PYO3_PYTHON=/Library/Frameworks/Python.framework/Versions/3.12/bin/python3.12
  cd engine && cargo build --release --offline
  otool -L target/release/ra2-app | grep libpython
  ```

  The expected dependency is `libpython3.12.dylib`. The 3.12 standard library
  must be present too. A framework Python records an absolute install name, so
  no `DYLD_LIBRARY_PATH` should be needed; a relocatable Python may need
  `DYLD_LIBRARY_PATH` and `PYTHONHOME`. Neither has been measured.
* **`ra2script/build.rs`** is a Linux-soname workaround: it looks for the
  `sysconfig` `LDLIBRARY` name and, if only `INSTSONAME` exists, symlinks it.
  On a normal macOS framework/Homebrew install `LDLIBRARY`
  (`libpython3.12.dylib`) exists and the script returns without touching
  anything. If it did not, the symlink branch is harmless on macOS. It is not a
  macOS-specific code path and was not needed there.
* **wgpu/Metal.** `Backends::all()` includes Metal; the renderer uses no
  surface and reads pixels back into a PNG. Whether a headless Metal device is
  available in every macOS session (SSH, CI) is unverified.
* **Game discovery.** The binary accepts `--game` and `RA2_GAME_DIR`, then
  walks up from the working directory and the build-time crate directory
  looking for `local/game`. A `.app` has neither a source checkout nor a
  stable working directory, so a macOS launcher must pass `--game` (or export
  `RA2_GAME_DIR`). `ra2script` then `chdir`s to the game root, exactly as on
  Linux; Python accepts `/` separators on macOS.
* **Package.** The intended shape is a `.app` bundle with
  `Contents/Info.plist`, `Contents/MacOS/` (the client plus a `bin/` of the
  tools), `Contents/Resources/icon.icns`, and the game **not** inside.
  `packaging/macos/bundle.sh` now writes that skeleton: it refuses non-Darwin
  hosts, requires a release build, copies `ra2-app` and the five tools, writes
  `Info.plist`, and puts a README in `Contents/Resources/` naming the
  `libpython3.12.dylib`, Jolt, Metal and signing gaps. **The script has never
  been executed on macOS; no `.app` from it exists.** It does not build, copy
  the dylib, create an `.icns`, sign or notarise. Distribution outside the
  developer's machine needs a Developer ID signature with the hardened runtime
  and notarisation:

  ```sh
  codesign --force --deep --options runtime --sign "Developer ID Application: …" "Open Robot Arena.app"
  xcrun notarytool submit "Open Robot Arena.zip" --wait --keychain-profile …
  xcrun stapler staple "Open Robot Arena.app"
  ```

  These commands are the documented path, **not** something this repository
  has run. `tools/package.sh` refuses non-Linux hosts (`uname -s`), so there is
  no macOS artifact to sign.

### 2.3 Windows x86_64

What is known from the sources, not from a run:

* **MSVC toolchain.** `rustup default stable-msvc` plus Visual Studio Build
  Tools with "Desktop development with C++" and a Windows 10/11 SDK. The
  GNU target was not considered: `oxijolt-sys`'s prebuilt Windows archives are
  built for `x86_64-pc-windows-msvc`, CRT `MultiThreadedDLL`.
* **Jolt.** `prebuilt.txt` lists x86_64 MSVC archives (toolset 14.44). With
  network, `oxijolt-sys` may fetch one with `curl`; with
  `CARGO_NET_OFFLINE=true` (what `tools/package.sh` and `tools/check.sh` now
  export) it builds the vendored Jolt with CMake instead. Both paths are the
  crate's; neither was run on Windows here. Windows arm64 has no prebuilt
  entry and would take the CMake path.
* **Python.** Install the **x86_64** CPython 3.12 from python.org (not the
  Microsoft Store alias), then:

  ```powershell
  $env:PYO3_PYTHON = "C:\Program Files\Python312\python.exe"
  cd engine
  cargo build --release --offline
  dumpbin /dependents target\release\ra2-app.exe | Select-String python
  ```

  Expected: `python312.dll` resolved from
  `%LOCALAPPDATA%\Programs\Python\Python312\` or `C:\Program Files\Python312\`,
  with `python312.lib` in `libs\` for the link and `Lib\` for the standard
  library. Whether the embedded interpreter locates `Lib` without
  `PYTHONHOME` in a copied/relocated package is **unverified**. Do not mix a
  32-bit Python with an x86_64 build.
* **wgpu/DX12.** `Backends::all()` includes DX12; an offscreen texture needs a
  DX12-capable adapter and driver. A software/WARP adapter was not tested.
  The original game's `D3D8.dll`, `D3D9.dll` and `dgVoodoo` are never loaded
  or shipped.
* **Console vs windowed subsystem.** The tools are console programs (default
  Rust subsystem on Windows). `ra2-app` opens a `winit` window but keeps that
  console subsystem, so a Windows launch would show a console alongside the
  game window; whether that is acceptable, and whether a
  `windows_subsystem = "windows"` attribute or a GUI launcher is wanted, is
  **unverified and undecided**. No attribute was added.
* **Game discovery.** `--game` and `RA2_GAME_DIR` work unchanged. A portable
  launcher would also try `%LOCALAPPDATA%\ra2-open-robot-arena\game`,
  `game\` beside the executable and `%ProgramFiles(x86)%\Robot Arena 2`; only
  the first two are the same idea as `run.sh`. Paths in `.gib` notes are
  normalised (`\` → `/`) by `ra2fmt::find_ci`, and the resolver walks a
  directory case-insensitively when a direct `Path::join` misses. This has
  been exercised on Linux's case-sensitive filesystem, not on NTFS.
* **Package.** The intended shape is a portable `.zip` containing
  `ra2-app.exe` and the five tools, a `run.cmd`/`run.ps1` that performs the
  game search and the Python check, `README.txt`, and the licences; or an MSI
  built with WiX. `packaging/windows/zip.sh` now writes the binary/layout half:
  it must run under Git Bash/MSYS/Cygwin on Windows, requires the MSVC `.exe`
  files, and emits a README naming the `python3.12.dll`, Jolt and DX12 gaps.
  **It has never been executed; no Windows zip exists.** It does not write a
  `run.cmd`/`run.ps1`: that launcher is still open work, and writing it here
  would be untested code presented as a path. The `.desktop` file and
  `run.sh`/`install.sh` are Linux only and are not part of a Windows package.

## 3. What is unverified, and what would verify it

For each platform, the same acceptance the Linux tarball has (docs/08 §5):

| Unverified | The evidence that would close it |
|---|---|
| It compiles and links | `cargo build --release --offline` from a clean `engine/target` |
| The test suite passes | `cargo test --release --offline --workspace` (267 tests on Linux; corpus tests skip without the game) |
| The shell boots the data and walks both routes | `cargo run -q --release -p ra2-app -- --game "$GAME" --offscreen out.png`; and the CLI routes `cargo run -q --release -p ra2-shell -- --route botlab --out out --game "$GAME"` / `--route exhibition`; 7 and 3 PNGs, 1280×1005 |
| The boot frame is the main menu | `cargo run -q --release -p ra2-render --example uishot -- "$GAME/UI/mainmenu/mainmenu.gib" ref.png 1280 1005`; `cmp out.png ref.png` |
| The Python link is discoverable | `otool -L` (macOS) / `dumpbin /dependents` (Windows) shows the 3.12 library, and the 3.12 stdlib is found at runtime |
| The game search and its error | run with `--game` pointing at a directory without `RA2.cfz`, `UI/` and `Scripts/`; the error must name all three |
| The packaged artifact runs from a clean tree | unpack the `.app`/`.zip` on a machine with no Rust toolchain and no game inside; run the same commands |
| A Linux Flatpak runs | `flatpak-builder --user --install`, then `flatpak run --command=ra2-app org.openrobotarena.OpenRobotArena --game "$GAME" --offscreen out.png` on a host without the Rust toolchain; the first-run portal picker must return a readable path |
| A Linux AppImage runs | `packaging/appimage/build-appimage.sh`, then the resulting file with `--offscreen` on a clean host, with and without CPython 3.12 installed (the latter must print the launcher's message and exit non-zero) |
| Signing, notarisation, installer | `codesign --verify --deep --strict` + `spctl -a -vv` + `xcrun stapler validate`; or an installed MSI that launches |
| arm64 Windows, universal macOS, aarch64 Linux | one build/test/smoke run per target; none exists |

### 3.1 The three unknowns that decide the build

These are the ones to check first; a failure in any of them blocks the artifact
regardless of packaging:

1. **Jolt prebuilt availability.** `oxijolt-sys`'s `prebuilt.txt` lists
   `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` archives and **no
   macOS entry**. macOS must therefore compile vendored Jolt 5.6.0 with CMake
   and a C++17 compiler; Windows may use the MSVC 14.44 archive or fall back
   to CMake. **Unknown:** whether the macOS CMake build succeeds with the
   current Xcode SDK and whether the Windows archive is present, downloadable,
   and ABI-compatible with the installed toolset. Neither path was run here.
2. **The Python DLL/dylib.** pyo3 links the 3.12 shared library at build time
   and the loader resolves it before `main()`. **Unknown:** whether a
   relocated package can find `libpython3.12.dylib`/`python3.12.dll` without
   `DYLD_LIBRARY_PATH`/`PYTHONHOME`/`PATH` changes, where the 3.12 standard
   library is found from a copied tree, and whether `install_name_tool` or a
   bundled interpreter tree is needed. Nothing was bundled; the Linux decision
   (require the system 3.12) is the only measured one.
3. **The wgpu backend.** `Backends::all()` picks Metal on macOS and DX12 on
   Windows. **Unknown:** whether a Metal device exists in every session
   (including SSH/CI) and whether DX12 (or WARP) provides an adapter for the
   offscreen and windowed paths. Only Vulkan/Mesa was exercised here.

### 3.2 The scripts

`packaging/macos/bundle.sh` and `packaging/windows/zip.sh` are the M12.1/M12.2
artifacts. Both header-mark themselves `WRITTEN, NEVER RUN`; each refuses to
stage on the wrong host rather than pretending to cross-package. They write a
layout and a README, not a tested application: the macOS script does not copy
or repair the dylib, build an `.icns`, sign or notarise, and the Windows
script does not write `run.cmd`/`run.ps1`. Their output must not be published
as a release until the table above passes on that platform.

The Linux Flatpak and AppImage recipes (M12.3) are in the same state: written,
statically checked, never built. They are described in
[`08-packaging.md`](08-packaging.md) §6; the missing tools are
`flatpak-builder`, the 24.08 SDK/runtime and `appimagetool`/`linuxdeploy`.

The honest summary: **the platform table is a plan with one verified row.**
Linux is the artifact this repository ships today. macOS and Windows have a
named toolchain, library and packaging path, and now a reviewable packaging
script; they do not have a build, a test run, a screenshot or a notarised
bundle behind them, and this document does not pretend otherwise.

## 4. Why no more Rust code was added

The cheap, real configuration was checked:

* `cfg` use in the workspace is limited to `ra2script/build.rs`'s
  `#[cfg(unix)]` symlink and its `not(unix)` copy fallback. The shim returns
  before that branch when `LDLIBRARY` exists, and also when `INSTSONAME` is
  absent from `LIBDIR`; on Windows, `pyo3`'s own import-library link is the
  part that matters, and the shim was neither needed nor tested there.
* No Rust code reads `/tmp` any more: `ra2-shell`'s `--out` default and the
  `workshop` example both use `std::env::temp_dir()`.
* No code branches on `.desktop`, `XDG_*`, `ldd`, `LD_LIBRARY_PATH` or a
  Linux-only path; those live in `packaging/run.sh` and `tools/`, which are
  Linux launchers by construction.
* The `.gib` reader normalises separators and matches case-insensitively; the
  game search uses `Path::join` and `RA2_GAME_DIR`; the script layer uses
  `PathBuf::join` and lets CPython handle Windows paths.

What remains is measurement on the target OS, not invention here.
