//! `exportcmp <source.bot> <exported.bot>`: does a 1.12 re-write of the source equal the file the original's
//! Export Bot (Team HQ inventory) wrote? (docs/36 section 9.25)
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read(&a[1]).unwrap();
    let exp = std::fs::read(&a[2]).unwrap();
    let bot = ra2fmt::bot::parse(&src).unwrap();
    println!(
        "source {} default_texture {:?}",
        bot.version, bot.default_texture
    );
    let out = bot.upgraded().write();
    println!(
        "ours {} bytes, original's export {} bytes, equal {}",
        out.len(),
        exp.len(),
        out == exp
    );
    if let Some(i) = out.iter().zip(&exp).position(|(x, y)| x != y) {
        println!("first difference at byte {i}");
        let lo = i.saturating_sub(40);
        println!(
            "ours  {:?}",
            String::from_utf8_lossy(&out[lo..(i + 60).min(out.len())])
        );
        println!(
            "theirs {:?}",
            String::from_utf8_lossy(&exp[lo..(i + 60).min(exp.len())])
        );
    }
}
