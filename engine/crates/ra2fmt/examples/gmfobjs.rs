fn main() {
    for p in std::env::args().skip(1) {
        let d = std::fs::read(&p).unwrap();
        let s = ra2fmt::gmi::parse(&d).unwrap();
        println!("== {p}");
        for o in &s.objects {
            let bb = o.mesh.as_ref().map(|m| {
                let mut lo = [f32::MAX; 3];
                let mut hi = [f32::MIN; 3];
                for q in &m.points { for i in 0..3 { lo[i]=lo[i].min(q[i]); hi[i]=hi[i].max(q[i]); } }
                (m.points.len(), m.faces.len(), lo, hi)
            });
            println!("{:?} '{}' parent '{}' tm {:?} bb {:?}", o.kind, o.name, o.parent, o.transform, bb);
            if let Some(l)=&o.light { println!("   light {:?}", l); }
        }
    }
}
