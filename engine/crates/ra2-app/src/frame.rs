//! One frame of the shell, from either way of presenting it.
//!
//! The window and `--offscreen` both ask [`FrameRenderer::frame`] for the same
//! thing: the placed tree's draw list at the shell's own 1280×1005 design size,
//! plus where that rectangle lands in the target. The renderer letterboxes it
//! with [`ra2_render::ui::fit`], which is also the rectangle clicks are mapped
//! through, so a window pixel means the same point in both paths.

use std::path::Path;

use ra2_render::ui::{fit, UiAssets, UiScene, Viewport};
use ra2_shell::Shell;

/// A shell frame: the draw list, and the target rectangle it fills.
pub struct Frame {
    pub scene: UiScene,
    /// Where the scene lands in the requested target. The renderer recomputes
    /// the same value from the scene's size and the target; this is here for the
    /// input side, so a click and a quad cannot disagree about the bars.
    pub viewport: Viewport,
}

/// The shell's art, loaded once and drawn frame after frame.
pub struct FrameRenderer {
    assets: UiAssets,
}

impl FrameRenderer {
    pub fn open(game: &Path) -> Result<FrameRenderer, String> {
        UiAssets::open(game)
            .map(|assets| FrameRenderer { assets })
            .map_err(|e| e.to_string())
    }

    /// The current screen's draw list. `target` is the window or PNG size; the
    /// scene itself stays in the shell's coordinates and the renderer scales it.
    pub fn frame(&mut self, shell: &mut Shell, target: [u32; 2]) -> Frame {
        let [width, height] = shell.size();
        let placed = shell.placements();
        let scene = self.assets.scene(&placed, [width as u32, height as u32]);
        let viewport = fit([width, height], [target[0] as f32, target[1] as f32]);
        Frame { scene, viewport }
    }
}
