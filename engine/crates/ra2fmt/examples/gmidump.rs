use ra2fmt::gmi;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let s = gmi::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    println!("ambient {:?} bg {:?}", s.ambient, s.background);
    for o in s.lights() {
        println!(
            "LIGHT {} {:?} at {:?}",
            o.name,
            o.light.as_ref().unwrap(),
            o.transform[3]
        );
    }
    fn dm(m: &gmi::Material, ind: usize) {
        println!(
            "{:ind$}MAT {:?} amb{:?} dif{:?} spec{:?} shine{} str{} trans{} selfi{} xp{} two{}",
            "",
            m.name,
            m.ambient,
            m.diffuse,
            m.specular,
            m.shine,
            m.shine_strength,
            m.transparency,
            m.self_illum,
            m.xp_type,
            m.two_sided,
            ind = ind
        );
        for t in &m.textures {
            println!(
                "{:ind$}  TEX {} kind{} mt{} amt{} uv{:?} cm{} inv{} filt{} ch{}",
                "",
                t.bitmap,
                t.kind,
                t.map_type,
                t.amount,
                t.uv,
                t.colormap,
                t.invert,
                t.filter,
                t.channel,
                ind = ind
            );
        }
        for s in &m.sub_materials {
            dm(s, ind + 4);
        }
    }
    for m in &s.materials {
        dm(m, 0);
    }
    for rb in s.rigid_bodies() {
        println!(
            "RB {} mass{} unyield{} geom{} proxy{:?} usedisp{} disp{:?} nocoll{} inactive{} gt{}",
            rb.name,
            rb.mass,
            rb.unyielding,
            rb.simulation_geometry,
            rb.geometry_proxy,
            rb.use_display_proxy,
            rb.display_proxy,
            rb.disable_collisions,
            rb.inactive,
            rb.geo_type
        );
    }
    if std::env::var("GMI_PARENTS").is_ok() {
        for o in &s.objects {
            println!("NODE {:?} kind {:?} parent {:?} tm {:?}", o.name, o.kind, o.parent, o.transform);
        }
    }
    for o in s.meshes() {
        let m = o.mesh.as_ref().unwrap();
        let mut names: Vec<String> = m
            .faces
            .iter()
            .map(|f| {
                let t = s.materials.get(m.material_ref.max(0) as usize);
                t.map(|t| {
                    let e = t.resolve_face(f.material);
                    format!("{}({:.2})", e.name, e.transparency)
                })
                .unwrap_or("-".into())
            })
            .collect();
        names.sort();
        names.dedup();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for pt in &m.points {
            for k in 0..3 {
                lo[k] = lo[k].min(pt[k] + o.transform[3][k]);
                hi[k] = hi[k].max(pt[k] + o.transform[3][k]);
            }
        }
        println!("  bbox {:?}..{:?}", lo, hi);
        println!(
            "OBJ {} mats{:?} mat{} faces{} tc{} cols{} chans{:?} cull{}",
            o.name,
            names,
            m.material_ref,
            m.faces.len(),
            m.texcoords.len(),
            m.colors.len(),
            m.channels
                .iter()
                .map(|c| (c.index, c.texcoords.len(), c.texfaces.len()))
                .collect::<Vec<_>>(),
            m.backface_cull
        );
    }
}
#[allow(dead_code)]
fn unused() {}
