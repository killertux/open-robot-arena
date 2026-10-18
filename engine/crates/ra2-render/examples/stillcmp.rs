//! Compare renders with an original capture.
//!
//! `stillcmp <original.png> <out-triptych.png> <render.png>...` prints each
//! render's mean absolute error (0..255, RGB) against the original over the
//! whole frame and over the frame minus the match HUD (the score boxes, the
//! camera label and the clock), and writes the original and the renders side by
//! side, each scaled to half size.

use image::{imageops, GenericImage, RgbaImage};

/// The HUD overlay rectangles of the match screens at 1024x768: x, y, w, h.
const HUD: [(u32, u32, u32, u32); 4] = [
    (0, 0, 110, 85),
    (920, 0, 104, 85),
    (110, 10, 160, 30),
    (460, 725, 110, 35),
];

fn masked(x: u32, y: u32) -> bool {
    HUD.iter()
        .any(|&(hx, hy, w, h)| x >= hx && x < hx + w && y >= hy && y < hy + h)
}

fn mae(a: &RgbaImage, b: &RgbaImage, mask: bool) -> f64 {
    let (mut sum, mut n) = (0f64, 0f64);
    for (x, y, p) in a.enumerate_pixels() {
        if mask && masked(x, y) {
            continue;
        }
        let q = b.get_pixel(x, y);
        for c in 0..3 {
            sum += (p[c] as f64 - q[c] as f64).abs();
        }
        n += 3.0;
    }
    sum / n
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: stillcmp <original.png> <out.png> <render.png>...");
        std::process::exit(2);
    }
    let orig = image::open(&args[0])?.to_rgba8();
    let renders: Vec<RgbaImage> = args[2..]
        .iter()
        .map(|p| image::open(p).map(|i| i.to_rgba8()))
        .collect::<Result<_, _>>()?;
    let (w, h) = orig.dimensions();
    for (name, img) in args[2..].iter().zip(&renders) {
        if img.dimensions() != (w, h) {
            return Err(format!("{name}: size differs from the original").into());
        }
        println!(
            "{name}: MAE {:.2} (full)  {:.2} (HUD masked)",
            mae(&orig, img, false),
            mae(&orig, img, true)
        );
    }
    // `STILLCMP_CROP=x,y,w,h` shows that region at full size instead.
    let crop: Option<Vec<u32>> = std::env::var("STILLCMP_CROP")
        .ok()
        .map(|c| c.split(',').filter_map(|v| v.trim().parse().ok()).collect());
    let (tw, th) = match &crop {
        Some(c) if c.len() == 4 => (c[2], c[3]),
        _ => (w / 2, h / 2),
    };
    let mut out = RgbaImage::new(tw * (1 + renders.len() as u32), th);
    for (i, img) in std::iter::once(&orig).chain(renders.iter()).enumerate() {
        let tile = match &crop {
            Some(c) if c.len() == 4 => imageops::crop_imm(img, c[0], c[1], c[2], c[3]).to_image(),
            _ => imageops::resize(img, tw, th, imageops::FilterType::Triangle),
        };
        out.copy_from(&tile, tw * i as u32, 0)?;
    }
    out.save(&args[1])?;
    Ok(())
}
