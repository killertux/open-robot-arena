//! Shared helpers for writing frame sequences from the headless runners.

use std::path::{Path, PathBuf};

/// Write an RGBA8 buffer to a PNG.
pub fn save_png(
    path: &Path,
    rgba: &[u8],
    w: u32,
    h: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    image::save_buffer(path, rgba, w, h, image::ColorType::Rgba8)?;
    Ok(())
}

/// Tile the frames into a single contact sheet for quick inspection.
pub fn montage(
    frames: &[PathBuf],
    out: &Path,
    cols: usize,
    scale: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    if frames.is_empty() {
        return Ok(());
    }
    let cols = cols.max(1);
    let first = image::open(&frames[0])?.to_rgba8();
    let (tw, th) = (
        (first.width() / scale).max(1),
        (first.height() / scale).max(1),
    );
    let rows = frames.len().div_ceil(cols);
    let mut canvas = image::RgbaImage::from_pixel(
        tw * cols as u32,
        th * rows as u32,
        image::Rgba([18, 18, 22, 255]),
    );
    for (i, f) in frames.iter().enumerate() {
        let img = image::open(f)?.to_rgba8();
        let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
        let (x, y) = ((i % cols) as u32 * tw, (i / cols) as u32 * th);
        image::imageops::overlay(&mut canvas, &small, x as i64, y as i64);
    }
    canvas.save(out)?;
    Ok(())
}
