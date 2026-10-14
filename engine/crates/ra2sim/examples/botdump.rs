//! Diagnostic: dump a `.bot`'s component table and each component's attachment
//! points (`ID`, mount classes, host, frame) — the data a mount placement needs.
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

    let (bot, _root, loaded) = ra2_render::load_bot_components(&path).expect("load bot");
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
            "  {i:2} p={:2} onParent={} onSelf={} t=({:7.4},{:7.4},{:7.4}) q=({:6.3},{:6.3},{:6.3},{:6.3}) angle={:7.4} height={:7.4} {:12} {}",
            c.parent,
            c.parent_mount,
            c.own_mount,
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
        if let Some(lc) = loaded.get(i).and_then(|l| l.as_ref()) {
            let table = lc.scene.attachment_table();
            if table.is_empty() && !lc.scene.attachments().next().is_some() {
                // nothing to show
            }
            for (id, a) in &table {
                let frame = &a.transform;
                println!(
                    "        #{id:<2} {:<9} types=[{}]{} body={} pos=({:7.4},{:7.4},{:7.4})",
                    a.name,
                    a.types.join(", "),
                    match &a.host {
                        Some(h) => format!(" host={h}"),
                        None => String::new(),
                    },
                    a.has_body,
                    frame[3][0],
                    frame[3][1],
                    frame[3][2],
                );
            }
        }
    }

    // Which attachment each component actually names on itself and on its parent.
    println!("\nmounts used (component -> parent):");
    for (i, c) in bot.components.iter().enumerate() {
        if c.parent < 0 {
            continue;
        }
        let p = c.parent as usize;
        println!(
            "  {i:2} self({}) -> parent {p} ({})  [{} -> {}]",
            c.own_mount,
            c.parent_mount,
            bot.components[i].class,
            bot.components
                .get(p)
                .map(|q| q.class.as_str())
                .unwrap_or("?"),
        );
    }
}
