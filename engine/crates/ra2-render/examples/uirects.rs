//! Print a `.gib` screen's resolved widget rectangles.
//!
//! ```text
//! cargo run -q -p ra2-render --example uirects -- <screen.gib> [w h] [name...]
//! ```
//!
//! The capture harness (`tools/capture/window_smoke.sh`) clicks the windowed
//! client through XTEST, which needs root-window pixels, not widget names.  This
//! example asks the same resolver the renderer uses for those rectangles, so the
//! harness never hard-codes a layout the shipped screen can change.
//!
//! Output is one tab-separated line per placed widget:
//!
//! ```text
//! name<TAB>x<TAB>y<TAB>width<TAB>height
//! ```
//!
//! With names, only exact matches are printed (a name may be printed more than
//! once: the shell composes proxies and reuses names).  Without names, every
//! widget is printed, depth first in draw order.

use std::path::{Path, PathBuf};

use ra2_render::ui::UiAssets;

fn game_root(screen: &Path) -> Option<PathBuf> {
    if let Ok(root) = std::env::var("RA2_GAME") {
        return Some(PathBuf::from(root));
    }
    let screen = screen.canonicalize().ok()?;
    let mut dir = screen.parent()?;
    loop {
        if dir.file_name().is_some_and(|n| n == "UI") {
            return dir.parent().map(|p| p.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: uirects <screen.gib> [w h] [name...]");
        std::process::exit(2);
    }
    let screen_path = PathBuf::from(&args[0]);
    let width: f32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(1280.0);
    let height: f32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1005.0);
    let wanted: Vec<&str> = args.iter().skip(3).map(String::as_str).collect();

    let game = game_root(&screen_path)
        .ok_or("could not find the game root (no UI directory above the screen)")?;
    let text = std::fs::read_to_string(&screen_path)?;
    let mut gib = ra2fmt::gib::parse(&text)?;
    let fragments = ra2ui::inline_proxies_in(&mut gib, &game.join("UI"));
    let assets = UiAssets::open(&game)?;
    let roots = ra2ui::resolve_with(&gib, [width, height], Some(assets.faces()));

    for root in &roots {
        for placed in root.walk() {
            let name = placed.widget.name();
            if !wanted.is_empty() && !wanted.iter().any(|w| *w == name) {
                continue;
            }
            let f = placed.frame;
            println!("{name}\t{:.3}\t{:.3}\t{:.3}\t{:.3}", f.x, f.y, f.w, f.h);
        }
    }
    if !fragments.is_empty() {
        eprintln!(
            "{} proxies loaded: {}",
            fragments.len(),
            fragments.join(", ")
        );
    }
    Ok(())
}
