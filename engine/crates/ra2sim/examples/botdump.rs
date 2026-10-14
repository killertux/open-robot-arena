//! Diagnostic: dump a `.bot`'s raw component table.
//!
//! Usage: cargo run --release -p ra2sim --example botdump -- <bot> [from] [to]

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bot> [from] [to]");
    let from: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let to: usize = std::env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let data = std::fs::read(&path).expect("read");
    let bot = ra2fmt::bot::parse(&data).expect("parse");
    println!(
        "{} v{}: {} components",
        bot.name,
        bot.version,
        bot.components.len()
    );
    for (i, c) in bot.components.iter().enumerate() {
        if i < from || i > to {
            continue;
        }
        println!(
            "  {i:2} p={:2} mC={} mP={} t=({:7.4},{:7.4},{:7.4}) q=({:6.3},{:6.3},{:6.3},{:6.3}) angle={:7.4} height={:7.4} {:12} {}",
            c.parent,
            c.mount_on_component,
            c.mount_on_parent,
            c.translation[0],
            c.translation[1],
            c.translation[2],
            c.rotation[0],
            c.rotation[1],
            c.rotation[2],
            c.rotation[3],
            c.angle,
            c.height,
            c.class,
            c.file,
        );
    }
}
