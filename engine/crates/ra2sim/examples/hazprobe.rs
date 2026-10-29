//! Diagnostic: an arena's hazards fired with the robots placed where a test
//! wants them (docs/29 section 12z), to compare with the original's
//! `hazard-*` captures (`tools/oracle/capture_hazard.py`).
//!
//! Usage: `hazprobe <arena.py> <botA> <botB> <steps> [cmd...]`
//!   `at <robot> <x> <z> <yaw deg>`   spawn the robot there (before the first step)
//!   `py <step> <python>`             run Python in the arena scripts at that step
//!                                    (`;` separates lines)
//!   `idle <n>`                       idle steps before step 0 (default 0)
//!
//! Prints, every step, `J <step> <joint> <angle or offset> w <x y z> v <x y z>
//! c <x y z>` for every hazard joint's moving body, `S <step> <robot> c <x y
//! z> v <x y z> w <x y z>` for each chassis, and `H <step> <att> <vic> <comp>
//! <amount> <speed> p <x y z> n <x y z>` for every priced hit (an arena hit
//! has att -1). `RA2_H17_DUMP` writes the capture rows with a `["T",0,<step>]`
//! row at the head of every step.
use glam::Vec3;
use ra2sim::{BotSetup, DriveInput, Duel, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 4 {
        return Err("hazprobe <arena.py> <botA> <botB> <steps> [cmd...]".into());
    }
    let steps: usize = a[3].parse()?;
    let mut setups: Vec<BotSetup> = a[1..3]
        .iter()
        .map(|p| BotSetup {
            path: p.clone(),
            yaw_deg: None,
            spawn: None,
        })
        .collect();
    let mut py: Vec<(usize, String)> = Vec::new();
    let mut idle_steps = 0usize;
    let mut i = 4;
    while i < a.len() {
        match a[i].as_str() {
            "at" => {
                let r: usize = a[i + 1].parse()?;
                setups[r].spawn = Some([a[i + 2].parse()?, a[i + 3].parse()?]);
                setups[r].yaw_deg = Some(a[i + 4].parse()?);
                i += 5;
            }
            "py" => {
                py.push((a[i + 1].parse()?, a[i + 2].replace(';', "\n")));
                i += 3;
            }
            "idle" => {
                idle_steps = a[i + 1].parse()?;
                i += 2;
            }
            w => return Err(format!("unknown command {w:?}").into()),
        }
    }
    let mut duel = Duel::new(&a[0], &setups)?;
    duel.record_impacts = true;
    let n = duel.robots.len();
    let idle = vec![DriveInput::default(); n];
    for _ in 0..idle_steps {
        duel.step_direct(&idle, DEFAULT_STEP);
    }
    duel.impacts.clear();
    for s in 0..steps {
        ra2sim::havok17::dump_line(format_args!("[\"T\",0,{s}]"));
        for (at, code) in &py {
            if *at == s {
                if let Err(e) = duel.run_script(code) {
                    eprintln!("py at {s}: {e}");
                }
            }
        }
        duel.step_direct(&idle, DEFAULT_STEP);
        for (k, (j, body, slider)) in duel.hazard_joints().into_iter().enumerate() {
            let x = if slider {
                duel.physics.slider_offset(j)
            } else {
                duel.physics.hinge_angle(j)
            };
            let st = duel.physics.state(body).unwrap();
            let c = duel.physics.center_of_mass(body).unwrap_or(st.position);
            let (w, v) = (st.angular_velocity, st.linear_velocity);
            println!(
                "J {s} {k} {x:.4} w {:.3} {:.3} {:.3} v {:.3} {:.3} {:.3} c {:.4} {:.4} {:.4}",
                w.x, w.y, w.z, v.x, v.y, v.z, c.x, c.y, c.z
            );
        }
        for (r, robot) in duel.robots.iter().enumerate() {
            let ch = robot.parts[0].body;
            let st = duel.physics.state(ch).unwrap();
            let c = duel.physics.center_of_mass(ch).unwrap_or(st.position);
            let (w, v) = (st.angular_velocity, st.linear_velocity);
            let up = st.rotation * Vec3::Y;
            println!(
                "S {s} {r} c {:.4} {:.4} {:.4} v {:.3} {:.3} {:.3} w {:.3} {:.3} {:.3} up {:.3} {:.3} {:.3}",
                c.x, c.y, c.z, v.x, v.y, v.z, w.x, w.y, w.z, up.x, up.y, up.z
            );
        }
        for h in std::mem::take(&mut duel.impacts) {
            let att = if h.attacker == ra2sim::combat::NO_ATTACKER {
                -1
            } else {
                h.attacker as i64
            };
            println!(
                "H {s} {att} {} {} {:.3} {:.3} p {:.3} {:.3} {:.3} n {:.3} {:.3} {:.3}",
                h.target,
                duel.robots[h.target].rig.components[h.component].name,
                h.damage,
                h.speed,
                h.point.x,
                h.point.y,
                h.point.z,
                h.normal.x,
                h.normal.y,
                h.normal.z
            );
        }
    }
    for l in &duel.log {
        eprintln!("log {l}");
    }
    Ok(())
}
