//! Render every tab of the robot workshop.
//!
//! ```text
//! cargo run -q -p ra2-render --example workshop -- [out_dir]
//! ```
//!
//! The workshop is `botlabwin.gib` plus the fragment a tab loads into its
//! `content slot`, composed by `ra2ui::workshop` exactly as the game's C++ does
//! — no screen here is authored, and every one of them is the game's own file.

use std::path::PathBuf;

use ra2_render::ui::{UiAssets, UiRenderer};
use ra2_render::Renderer;
use ra2ui::workshop::{self, TABS};

fn game_root() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("RA2_GAME") {
        return Some(PathBuf::from(root));
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/game");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or("/tmp/workshop".into()));
    std::fs::create_dir_all(&out)?;
    let game = game_root().ok_or("no local/game (set RA2_GAME)")?;
    let ui = game.join("UI");
    let (width, height) = (1280u32, 1005u32);

    for tab in &TABS {
        let gib = workshop::compose(&ui, tab)?;
        let mut assets = UiAssets::open(&game)?;
        let placed = ra2ui::resolve_with(&gib, [width as f32, height as f32], Some(assets.faces()))
            .into_iter()
            .collect::<Vec<_>>();
        let scene = assets.scene(&placed, [width, height]);

        let renderer = Renderer::new()?;
        let ui_renderer = UiRenderer::new(renderer.device());
        let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
        let name = tab.button.trim_end_matches(" button").replace(' ', "_");
        let path = out.join(format!("{name}.png"));
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!(
            "{:<22} -> {:<44} {} widgets, {} quads, {} proxies{}",
            tab.button,
            tab.fragment,
            placed.iter().map(|p| p.walk().count()).sum::<usize>(),
            scene.quads.len(),
            scene.tiles.len(),
            if scene.missing.is_empty() {
                String::new()
            } else {
                format!(", {} images missing", scene.missing.len())
            }
        );
    }
    println!("wrote the workshop's tabs to {}", out.display());
    Ok(())
}
