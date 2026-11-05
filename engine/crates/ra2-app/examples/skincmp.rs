//! Compare the default appearance we draw with the skin of a live `.bot`.
//! `skincmp GAME BOT TYPE [OUT.png]`
use ra2ui::workshop::skinfx;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bot = ra2fmt::bot::parse(&std::fs::read(&a[2]).unwrap()).unwrap();
    let ch = bot.components[0].chassis.as_ref().unwrap();
    let live = ra2ui::workshop::paint::Skin::from_tga(ch.texture_tga.as_ref().unwrap()).unwrap();
    let polys: Vec<Vec<[f32; 2]>> = ch.face_groups.iter().map(|g| g.coords.clone()).collect();
    let white = ra2ui::workshop::paint::Skin::new(256, 256, [255; 4]);
    let ours = skinfx::default_appearance(
        std::path::Path::new(&a[1]),
        a[3].parse().unwrap(),
        &polys,
        &white,
    );
    let mut sum = 0u64;
    for (x, y) in ours.pixels.iter().zip(&live.pixels) {
        sum += (*x as i32 - *y as i32).unsigned_abs() as u64;
    }
    let mut cnt = 0;
    let mut mx = 0;
    for (x, y) in ours.pixels.iter().zip(&live.pixels) {
        let d = (*x as i32 - *y as i32).abs();
        if d > 0 {
            cnt += 1;
        }
        mx = mx.max(d);
    }
    println!("differing channels {cnt} max {mx}");
    println!("MAE {:.3}", sum as f64 / ours.pixels.len() as f64);
    if let Ok(row) = std::env::var("ROW") {
        let (y, x0, x1): (usize, usize, usize) = {
            let v: Vec<usize> = row.split(',').map(|v| v.parse().unwrap()).collect();
            (v[0], v[1], v[2])
        };
        for x in x0..x1 {
            let i = (y * 256 + x) * 4;
            println!(
                "x{} live {:?} ours {:?}",
                x,
                &live.pixels[i..i + 3],
                &ours.pixels[i..i + 3]
            );
        }
    }
    if let Some(out) = a.get(4) {
        let mut img = image::RgbaImage::new(768, 256);
        for y in 0..256u32 {
            for x in 0..256u32 {
                let i = ((y * 256 + x) * 4) as usize;
                let o = &ours.pixels[i..i + 4];
                let l = &live.pixels[i..i + 4];
                img.put_pixel(x, y, image::Rgba([l[0], l[1], l[2], 255]));
                img.put_pixel(256 + x, y, image::Rgba([o[0], o[1], o[2], 255]));
                let d = |k: usize| (o[k] as i32 - l[k] as i32).unsigned_abs().min(63) as u8 * 4;
                img.put_pixel(512 + x, y, image::Rgba([d(0), d(1), d(2), 255]));
            }
        }
        img.save(out).unwrap();
    }
}
