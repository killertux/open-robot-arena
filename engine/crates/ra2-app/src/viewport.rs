//! Window pixels to shell coordinates.
//!
//! The shell lays out at 1280×1005 and the window is whatever size the player
//! made it, so the scene is centred in the surface with its aspect ratio kept
//! ([`ra2_render::ui::fit`]). A click in a letterbox/pillarbox bar belongs to no
//! widget and is rejected rather than clamped onto an edge.

pub use ra2_render::ui::{fit, Viewport};

/// Map a point in window pixels to the shell's design coordinates.
///
/// `None` when the point is in a bar, which is what makes a click there a
/// no-op instead of a click on the nearest menu button.
pub fn to_shell(window: [u32; 2], shell: [f32; 2], point: [f32; 2]) -> Option<[f32; 2]> {
    fit(shell, [window[0] as f32, window[1] as f32]).to_scene(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHELL: [f32; 2] = [1280.0, 1005.0];

    /// A 1:1 target is the identity: the pixels the pixel-based routes use are
    /// the shell's own coordinates.
    #[test]
    fn a_1_to_1_window_maps_every_pixel_unchanged() {
        for point in [[0.0, 0.0], [639.0, 544.0], [1279.0, 1004.0]] {
            assert_eq!(to_shell([1280, 1005], SHELL, point), Some(point));
        }
        // The far edge is outside the half-open rectangle, not a widget.
        assert_eq!(to_shell([1280, 1005], SHELL, [1280.0, 10.0]), None);
        assert_eq!(to_shell([1280, 1005], SHELL, [10.0, 1005.0]), None);
    }

    /// A 16:9 window is pillarboxed: the shell keeps its 1280:1005 aspect and
    /// the bars are left and right. The mapping is the inverse of the scale.
    #[test]
    fn a_16_to_9_window_pillarboxes_and_maps_exactly() {
        let window = [1920, 1080];
        let viewport = fit(SHELL, [1920.0, 1080.0]);
        assert!((viewport.scale - 1080.0 / 1005.0).abs() < 1e-5);
        assert!(viewport.x > 0.0, "a pillarbox on the left");
        assert!(viewport.y.abs() < 1e-4, "the height is the fit");
        assert!((viewport.x * 2.0 + viewport.w - 1920.0).abs() < 1e-3);

        // Both corners and the centre are exact under the fit.
        for scene in [[0.0, 0.0], [640.0, 502.5], [1279.0, 1004.0]] {
            let pixel = viewport.map(scene);
            let back = to_shell(window, SHELL, pixel).expect("inside the scene");
            assert!((back[0] - scene[0]).abs() < 1e-3, "{scene:?} -> {pixel:?}");
            assert!((back[1] - scene[1]).abs() < 1e-3, "{scene:?} -> {pixel:?}");
        }
        // The shell's own route still works through the 16:9 mapping.
        let pixel = viewport.map([573.5, 534.8]);
        let back = to_shell(window, SHELL, pixel).unwrap();
        assert!((back[0] - 573.5).abs() < 1e-3);
        assert!((back[1] - 534.8).abs() < 1e-3);
    }

    /// Clicks in the bars are rejected in both shapes: pillarbox (a 16:9
    /// window) and letterbox (a window narrower than the shell's 1.27:1
    /// aspect, which leaves bars above and below).
    #[test]
    fn a_click_in_the_letterbox_bars_is_rejected() {
        let pillarbox = fit(SHELL, [1920.0, 1080.0]);
        assert_eq!(
            to_shell([1920, 1080], SHELL, [pillarbox.x - 1.0, 500.0]),
            None
        );
        assert_eq!(
            to_shell(
                [1920, 1080],
                SHELL,
                [pillarbox.x + pillarbox.w + 1.0, 500.0]
            ),
            None
        );
        assert!(
            to_shell([1920, 1080], SHELL, [pillarbox.x + 1.0, 500.0]).is_some(),
            "just inside the left edge is the shell"
        );

        let letterbox = fit(SHELL, [1100.0, 1000.0]);
        assert!(letterbox.y > 0.0, "a letterbox above and below");
        assert_eq!(
            to_shell([1100, 1000], SHELL, [550.0, letterbox.y - 1.0]),
            None
        );
        assert_eq!(
            to_shell(
                [1100, 1000],
                SHELL,
                [550.0, letterbox.y + letterbox.h + 1.0]
            ),
            None
        );
        assert!(
            to_shell([1100, 1000], SHELL, [550.0, letterbox.y + 1.0]).is_some(),
            "just inside the top edge is the shell"
        );
    }
}
