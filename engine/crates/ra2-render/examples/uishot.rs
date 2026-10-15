//! Draw a `.gib` screen with the engine's own renderer.
//!
//! ```text
//! cargo run -q -p ra2-render --example uishot -- <screen.gib> <out.png> [w h]
//! ```
//!
//! The game root is found from the screen's path (`.../UI/...` → the directory
//! above `UI`); `RA2_GAME` overrides it. This is the check
//! `tools/ui_compose.py` makes, with the engine drawing instead of a script:
//! the two should agree.

use std::path::{Path, PathBuf};

use ra2_render::ui::{UiAssets, UiRenderer};
use ra2_render::Renderer;

/// The game root for a screen: the directory above `UI`.
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
    if args.len() < 2 {
        eprintln!("usage: uishot <screen.gib> <out.png> [w h]");
        std::process::exit(2);
    }
    let screen_path = PathBuf::from(&args[0]);
    let out = PathBuf::from(&args[1]);
    let width: f32 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(1280.0);
    let height: f32 = args.get(3).and_then(|a| a.parse().ok()).unwrap_or(1005.0);

    let game = game_root(&screen_path)
        .ok_or("could not find the game root (no UI directory above the screen)")?;
    let text = std::fs::read_to_string(&screen_path)?;
    let gib = ra2fmt::gib::parse(&text)?;

    let mut assets = UiAssets::open(&game)?;
    let placed = ra2ui::resolve_with(&gib, [width, height], Some(assets.faces()))
        .into_iter()
        .collect::<Vec<_>>();
    let scene = assets.scene(&placed, [width as u32, height as u32]);

    let renderer = Renderer::new()?;
    let ui = UiRenderer::new(renderer.device());
    let pixels = ui.render(renderer.device(), renderer.queue(), &scene)?;
    let image = image::RgbaImage::from_raw(scene.size[0], scene.size[1], pixels)
        .ok_or("the readback did not fill the image")?;
    image.save(&out)?;

    println!(
        "wrote {} ({}x{}), {} quads over {} textures",
        out.display(),
        scene.size[0],
        scene.size[1],
        scene.quads.len(),
        scene.textures.len()
    );
    if !scene.missing.is_empty() {
        println!(
            "  {} images the layout names are not in the game:",
            scene.missing.len()
        );
        for m in scene.missing.iter().take(6) {
            println!("     {m}");
        }
    }
    if !scene.tiles.is_empty() {
        let mut named: Vec<&String> = scene.tiles.iter().collect();
        named.sort();
        named.dedup();
        println!(
            "  {} tiles are not drawn (a pattern repeat): {}",
            scene.tiles.len(),
            named
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}
