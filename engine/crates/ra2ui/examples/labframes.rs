//! Diagnostic: the frames of the lab's overview widgets at a screen size.
use ra2ui::workshop::BotLab;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let w: f32 = a[2].parse().unwrap();
    let h: f32 = a[3].parse().unwrap();
    let lab = BotLab::open(std::path::Path::new(&a[1]).join("UI").as_path()).unwrap();
    fn dump(w: &ra2fmt::gib::Widget, d: usize) {
        for a in &w.attachments {
            println!(
                "DOC {:ind$}  attach {:?} body {:?} fields {:?}",
                "",
                a.kind,
                a.body
                    .as_ref()
                    .map(|b| (b.name().to_string(), b.kind.clone())),
                a.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                ind = d * 2
            );
        }
        if d < 5 {
            println!(
                "DOC {:ind$}{} {:?} kids {}",
                "",
                w.name(),
                w.kind,
                w.children.len(),
                ind = d * 2
            );
        }
        for c in &w.children {
            dump(c, d + 1);
        }
    }
    for b in &lab.document.blocks {
        if let ra2fmt::gib::Block::Widget(w) = b {
            dump(w, 0);
        }
    }
    let placed = lab.placements([w, h], None);
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
    for p in &placed {
        go(p, 0);
    }
    for p in placed.iter().flat_map(|p| p.walk()) {
        if p.widget.name().contains("Info") {
            println!(
                "WALK {} {:?} kids {}",
                p.widget.name(),
                p.frame,
                p.children.len()
            );
        }
    }
}
