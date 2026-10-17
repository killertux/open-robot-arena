fn main() {
    if let Err(error) = ra2_app::run(std::env::args().skip(1)) {
        eprintln!("ra2-app: {error}");
        std::process::exit(1);
    }
}
