//! Print every line of a CFL archive's text entries that contains a needle.
use ra2fmt::cfl::Cfl;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let cfl = Cfl::open_path(std::path::Path::new(&a[1])).expect("archive");
    let needle = a[2].to_lowercase();
    for e in cfl.entries() {
        if let Some(t) = cfl.get_text(&e.name) {
            for l in t.lines() {
                if l.to_lowercase().contains(&needle) {
                    println!("{}: {}", e.name, l.trim());
                }
            }
        }
    }
}
