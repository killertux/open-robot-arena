//! Diagnostic: version, forward heading and component placements of each given `.bot`
//! (parent, parent mount, own mount, translation, rotation, SHIFT angle, CTRL height).
fn main() {
    for p in std::env::args().skip(1) {
        let b = ra2fmt::bot::parse(&std::fs::read(&p).expect("read")).expect("parse");
        println!("{} {} v{} heading {:?}", p, b.name, b.version, b.heading);
        for (i, c) in b.components.iter().enumerate() {
            println!(
                "  {i} {} {} parent {} pm {} om {} t {:?} q {:?} angle {} height {}",
                c.class,
                c.file,
                c.parent,
                c.parent_mount,
                c.own_mount,
                c.translation,
                c.rotation,
                c.angle,
                c.height
            );
        }
    }
}
