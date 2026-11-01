//! `livebots2 <game> <in.bot> <out.bot>`: load in the lab model and save again.
use ra2ui::workshop::build::Build;
use ra2ui::workshop::library::GameLibrary;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = std::path::PathBuf::from(&a[1]);
    std::env::set_var("RA2_GAME", &game);
    let library = GameLibrary::open(&game).unwrap();
    let build = Build::load(&library, std::path::Path::new(&a[2])).unwrap();
    build.save_to(std::path::Path::new(&a[3])).unwrap();
}
