//! Diagnostic: gmf files whose stored normals are not unit length.
use ra2fmt::gmi;
fn main() {
    for p in std::env::args().skip(1) {
        let Ok(d) = std::fs::read(&p) else { continue };
        let Ok(s) = gmi::parse(&d) else { continue };
        let mut worst: Option<(f32, f32, usize)> = None;
        for o in &s.objects {
            let Some(m) = &o.mesh else { continue };
            let mut n = 0;
            let (mut lo, mut hi) = (f32::MAX, 0f32);
            for v in &m.normals {
                let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if l > 1e-6 && (l - 1.0).abs() > 0.01 {
                    n += 1;
                }
                if l > 1e-6 {
                    lo = lo.min(l);
                    hi = hi.max(l);
                }
            }
            if n > 0 {
                worst = Some((lo, hi, n));
                println!(
                    "{p} {} non-unit {n} of {} len {lo:.3}..{hi:.3}",
                    o.name,
                    m.normals.len()
                );
            }
        }
        let _ = worst;
    }
}
