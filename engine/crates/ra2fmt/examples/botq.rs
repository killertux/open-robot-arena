//! Diagnostic: version, forward heading and component placements of each given `.bot`.
fn main() {
    for p in std::env::args().skip(1) {
        let b = ra2fmt::bot::parse(&std::fs::read(&p).expect("read")).expect("parse");
        println!("{} {} v{} heading {:?}", p, b.name, b.version, b.heading);
        for (i, c) in b.components.iter().enumerate() {
            println!(
                "  {i} {} {} t {:?} q {:?} angle {}",
                c.class, c.file, c.translation, c.rotation, c.angle
            );
        }
    }
}
