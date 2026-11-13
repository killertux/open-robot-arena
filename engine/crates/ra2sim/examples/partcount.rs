//! Diagnostic (docs/36 9.40): per component `index class hitpoints` of a `.bot`'s rig, tab separated.
//! Usage: cargo run --release -p ra2sim --example partcount -- <bot>...
fn main() {
    for path in std::env::args().skip(1) {
        let rig = ra2sim::rig::load(&path).expect("load bot");
        println!("# {path}");
        for (i, c) in rig.components.iter().enumerate() {
            println!("{i}\t{}\t{}", c.class, c.hitpoints);
        }
    }
}
