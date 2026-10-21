//! Test robot: a stock `.bot` with a Flamethrower on its chassis and two wires,
//! `Fire` (an analog control called `Flame`, key `F`) and `Fire burst` (a
//! button called `FlameBurst`, key `G`). Used by the live captures of
//! docs/37 section 6 and by the tests.
//!
//! Usage: flamebot <in.bot> <out.bot> [x z angle]  (FLAME_NO_TANK=1 leaves the air tank out)

use ra2fmt::bot::{self, Binding, Controller};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).expect("read bot");
    let mut b = bot::parse(&data).expect("parse bot");
    let num = |i: usize, d: f32| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let mut c = ra2fmt::mount::chassis_child([0.0, 0.0], num(5, 0.0), 0.0);
    c.translation = [num(3, 0.0), 0.0, num(4, 0.0)];
    c.parent_mount = 0;
    c.own_mount = 1;
    c.class = "Flamethrower".into();
    c.file = "Components\\flamethrower.txt".into();
    b.components.push(c);
    let idx = b.components.len() as i32 - 1;
    // the jet draws air as well as current: without a tank its emitter never starts
    // (`0x504790` wants both power scales above zero)
    if std::env::var("FLAME_NO_TANK").is_err() {
        let mut t = ra2fmt::mount::chassis_child([0.3, 0.1], 0.0, 0.0);
        t.own_mount = 1;
        t.class = "AirTank".into();
        t.file = "Components\\airtank.txt".into();
        b.components.push(t);
    }
    b.controllers.push(Controller {
        name: "Flame".into(),
        position: [5, 5, 1],
        bindings: vec![[0, 0, 33, 0]],
    });
    b.controllers.push(Controller {
        name: "FlameBurst".into(),
        position: [6, 5, 4],
        bindings: vec![[0, 0, 34, 0]],
    });
    b.bindings.push(Binding {
        component: idx,
        action: "Fire".into(),
        control: [5, 5, 0],
    });
    b.bindings.push(Binding {
        component: idx,
        action: "Fire burst".into(),
        control: [6, 5, 0],
    });
    b.write_to(std::path::Path::new(&a[2])).expect("write");
    println!("wrote {} ({} components)", a[2], b.components.len());
}
