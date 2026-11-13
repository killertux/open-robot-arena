fn main() {
    for p in std::env::args().skip(1) {
        let b = ra2fmt::bot::parse(&std::fs::read(&p).unwrap()).unwrap();
        let c = b.components[0].chassis.as_ref().unwrap();
        let sz: Vec<usize> = b
            .components
            .iter()
            .enumerate()
            .filter(|(_, x)| x.class == "SmartZone")
            .map(|(i, _)| i)
            .collect();
        println!(
            "{} | {} | {} comps | ver {} | hp {:?} | smartzones {:?}",
            p.rsplit('/').next().unwrap(),
            b.name,
            b.components.len(),
            b.version,
            c.hitpoints(),
            sz
        );
        if std::env::var("V").is_ok() {
            for (i, x) in b.components.iter().enumerate() {
                println!("  {i} {} {} parent={}", x.class, x.file, x.parent);
            }
        }
    }
}
