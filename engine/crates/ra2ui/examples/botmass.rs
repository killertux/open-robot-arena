//! Diagnostic: per-part mass of each given `.bot` against the game folder (first argument).
use ra2ui::workshop::build::Build;
use ra2ui::workshop::library::GameLibrary;
fn main() {
    let mut a = std::env::args().skip(1);
    let root = a.next().expect("game dir");
    let lib = GameLibrary::open(std::path::Path::new(&root)).expect("lib");
    for p in a {
        let build = Build::load(&lib, std::path::Path::new(&p)).expect("build");
        let s = build.stats();
        println!("{p}: mass {:.3}", s.mass);
        for (i, ps) in s.parts.iter().enumerate() {
            let info = build.component_info(i);
            println!(
                "  {i} {} {} mass {:.3} hull {:?}",
                ps.class,
                ps.name,
                ps.mass,
                info.map(|x| x.has_hull)
            );
        }
    }
}
