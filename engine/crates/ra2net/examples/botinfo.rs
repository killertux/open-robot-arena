//! `botinfo FILE...`: the components and controls of a `.bot` (for reading snapshots).
fn main() {
    for path in std::env::args().skip(1) {
        let bot = ra2fmt::bot::parse(&std::fs::read(&path).expect("a file")).expect("a bot");
        println!(
            "== {} ({}) components {} controllers {} bindings {}",
            bot.name,
            path,
            bot.components.len(),
            bot.controllers.len(),
            bot.bindings.len()
        );
        for (i, c) in bot.components.iter().enumerate() {
            println!(
                "  {i:2} parent {:2} {} {} at {:?}",
                c.parent, c.class, c.file, c.translation
            );
        }
        for c in &bot.controllers {
            println!("  ctl {} {:?} {:?}", c.name, c.position, c.bindings);
        }
    }
}
