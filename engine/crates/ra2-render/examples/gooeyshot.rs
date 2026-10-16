//! Draw a debug window that a *script* built.
//!
//! The shipped `Scripts/Gooey.py` is a wrapper over `plus.addWindow`/`addText`,
//! and the AI scripts build their debug windows with it. This boots the shell,
//! asks the shipped wrapper for a window and two lines, and draws the result
//! with the same renderer a `.gib` screen goes through:
//!
//! ```text
//! cargo run -q -p ra2-render --example gooeyshot -- <out.png>
//! ```
//!
//! It is the whole loop in one command: the game's own Python → `plus` → a
//! widget tree → `ra2ui` → the GPU.

use std::path::PathBuf;

use ra2_render::ui::{UiAssets, UiRenderer};
use ra2_render::Renderer;

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
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or("gooey.png".into()));
    let game = game_root().ok_or("no local/game (set RA2_GAME)")?;
    let (width, height) = (1280u32, 1005u32);

    let _guard = ra2script::lock();
    let _host = ra2script::ScriptHost::boot(&game)?;
    // The shipped `Gooey.Plain` builds the window, not this example.
    ra2script::ui::debug_window(
        "watch",
        20.0,
        20.0,
        260.0,
        86.0,
        &["Throttle  87", "Turning  -12", "Range  4.6 m"],
    )?;

    // A script's window is a widget tree; the resolver places it and the UI
    // renderer draws it, exactly as it does a screen.
    let widgets = ra2script::ui::windows();
    let screen = ra2ui::Frame::new(0.0, 0.0, width as f32, height as f32);
    let placed: Vec<_> = widgets.iter().map(|w| ra2ui::place(w, screen)).collect();

    let mut assets = UiAssets::open(&game)?;
    let scene = assets.scene(&placed, [width, height]);
    let renderer = Renderer::new()?;
    let ui = UiRenderer::new(renderer.device());
    let pixels = ui.render(renderer.device(), renderer.queue(), &scene)?;
    image::RgbaImage::from_raw(width, height, pixels)
        .ok_or("readback")?
        .save(&out)?;
    println!(
        "wrote {} — {} windows, {} quads",
        out.display(),
        widgets.len(),
        scene.quads.len()
    );
    if !scene.missing.is_empty() {
        println!("  no font? {:?}", scene.missing);
    }
    Ok(())
}
