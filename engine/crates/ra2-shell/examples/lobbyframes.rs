//! Diagnostic: the frames of the pre-match lobby's widgets at 1024x768.
use ra2_shell::{Shell, PREMATCH};
fn main() {
    ra2fmt::saves::isolate_for_tests();
    let game = std::env::args().nth(1).expect("game dir");
    let mut shell = Shell::boot(std::path::Path::new(&game), [1024.0, 768.0]).unwrap();
    shell.goto(PREMATCH).unwrap();
    fn go(p: &ra2ui::Placement, d: usize) {
        println!(
            "{:ind$}{} {:?} {:?}",
            "",
            p.widget.name(),
            p.widget.kind,
            p.frame,
            ind = d * 2
        );
        for c in &p.children {
            go(c, d + 1);
        }
    }
    for p in shell.placements().iter() {
        go(p, 0);
    }
}
