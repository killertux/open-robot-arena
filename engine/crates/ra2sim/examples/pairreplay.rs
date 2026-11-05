//! Replay a live stretch of two robots' meeting in ours, open loop (docs/29 12ay).
//!
//! ```text
//! pairreplay <arena.py> <botA> <botB> <state.tsv> <script.tsv> <seconds> <out.tsv>
//! ```
//!
//! `state.tsv`: one line per robot, `robot cx cy cz qx qy qz qw vx vy vz wx wy wz`: the chassis'
//! centre of mass, rotation and velocities as the live `S` rows of `frida/ra2_robotpair.js` give
//! them. The other parts of the robot are placed at their build pose relative to the chassis,
//! moving with it; nothing else is known of their state (wheels at rest, rods home, full health).
//!
//! `script.tsv`: `t thr0 turn0 thr1 turn1 flip0 fire1` (seconds from the state), the brains' own
//! commands (`Throttle`, `Turn`, the weapons' `Input`) at each tick, held until the next line;
//! the last two columns press `Flip` of robot 0 and `Fire` of robot 1 at that tick.
//!
//! `out.tsv`: every step, `t robot cx cy cz vx vy vz wx wy wz rot(9, column by column)`.
use std::io::Write;

use glam::{Mat3, Quat, Vec3};
use ra2sim::{DriveInput, GameType, Match, MatchConfig, RobotEntry, DEFAULT_STEP};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 8 {
        eprintln!("usage: pairreplay <arena.py> <botA> <botB> <state.tsv> <script.tsv> <seconds> <out.tsv>");
        std::process::exit(2);
    }
    let cfg = MatchConfig::new(
        a[1].clone(),
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, a[2].clone()),
            RobotEntry::ai(1, a[3].clone()),
        ],
    )
    .with_duration(Some(60.0));
    let mut m = Match::new(cfg)?;
    m.duel.hold_robots(false);
    // The build pose of every part relative to its chassis.
    struct Rel {
        robot: usize,
        part: usize,
        pos: Vec3,
        rot: Quat,
    }
    let mut rels: Vec<Rel> = Vec::new();
    let mut chassis: Vec<usize> = Vec::new();
    for (ri, r) in m.duel.robots.iter().enumerate() {
        let ci = (0..r.parts.len())
            .find(|&p| r.rig.parts[p].parent.is_none())
            .unwrap_or(0);
        chassis.push(ci);
        let cs = m
            .duel
            .physics
            .state(r.parts[ci].body)
            .ok_or("no chassis state")?;
        for (pi, p) in r.parts.iter().enumerate() {
            if !p.alive {
                continue;
            }
            let Some(s) = m.duel.physics.state(p.body) else {
                continue;
            };
            let inv = cs.rotation.inverse();
            rels.push(Rel {
                robot: ri,
                part: pi,
                pos: inv * (s.position - cs.position),
                rot: inv * s.rotation,
            });
        }
    }
    for line in std::fs::read_to_string(&a[4])?.lines() {
        let v: Vec<f32> = line
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if v.len() < 14 {
            continue;
        }
        let ri = v[0] as usize;
        let ci = chassis[ri];
        let body = m.duel.robots[ri].parts[ci].body;
        let (Some(st), Some(c)) = (
            m.duel.physics.state(body),
            m.duel.physics.center_of_mass(body),
        ) else {
            continue;
        };
        let local = st.rotation.inverse() * (c - st.position);
        let q = Quat::from_xyzw(v[4], v[5], v[6], v[7]).normalize();
        let com = Vec3::new(v[1], v[2], v[3]);
        let origin = com - q * local;
        let (lin, ang) = (Vec3::new(v[8], v[9], v[10]), Vec3::new(v[11], v[12], v[13]));
        for rel in rels.iter().filter(|r| r.robot == ri) {
            let pb = m.duel.robots[ri].parts[rel.part].body;
            let pos = origin + q * rel.pos;
            let rot = (q * rel.rot).normalize();
            m.duel.physics.set_transform(pb, pos, rot);
            m.duel
                .physics
                .set_velocity(pb, lin + ang.cross(pos - origin), ang);
        }
    }
    // `PAIRREPLAY_CHARGE=<f0>,<f1>`: each robot's batteries at that fraction of full when the replay
    // starts (a live stretch is 60-130 s into a fight; ours starts full, and the power scale
    // `clamp(supply / demand, 0.25, 1)` falls with the charge, docs/29 12bd).
    if let Ok(v) = std::env::var("PAIRREPLAY_CHARGE") {
        for (ri, f) in v
            .split(',')
            .filter_map(|x| x.parse::<f32>().ok())
            .enumerate()
        {
            if let Some(r) = m.duel.robots.get_mut(ri) {
                for s in r.energy.sources.iter_mut() {
                    s.current *= f;
                }
            }
        }
    }
    // The script.
    let mut script: Vec<[f32; 7]> = Vec::new();
    for line in std::fs::read_to_string(&a[5])?.lines() {
        let v: Vec<f32> = line
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if v.len() >= 5 {
            let mut r = [0.0f32; 7];
            for (i, x) in v.iter().take(7).enumerate() {
                r[i] = *x;
            }
            script.push(r);
        }
    }
    let secs: f32 = a[6].parse()?;
    let steps = (secs / DEFAULT_STEP) as usize;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[7])?);
    let mut k = 0usize;
    let inputs = [DriveInput::new(0.0, 0.0), DriveInput::new(0.0, 0.0)];
    for step in 0..steps {
        let t = step as f32 * DEFAULT_STEP;
        while k < script.len() && script[k][0] <= t + 1e-6 {
            let s = script[k];
            for (ri, (thr, turn)) in [(0usize, (s[1], s[2])), (1, (s[3], s[4]))] {
                if let Some(r) = m.duel.robots.get_mut(ri) {
                    r.control.input(&r.rig.wiring, "Forward", 0, thr);
                    r.control.input(&r.rig.wiring, "LeftRight", 0, -turn);
                    r.control.input(&r.rig.wiring, "LeftRight", 1, turn);
                }
            }
            if s[5] != 0.0 {
                let r = &mut m.duel.robots[0];
                r.control.input(&r.rig.wiring, "Flip", 0, 1.0);
            }
            if s[6] != 0.0 {
                let r = &mut m.duel.robots[1];
                r.control.input(&r.rig.wiring, "Fire", 0, 1.0);
            }
            k += 1;
        }
        m.duel.step_direct(&inputs, DEFAULT_STEP);
        let t1 = (step + 1) as f32 * DEFAULT_STEP;
        for (ri, r) in m.duel.robots.iter().enumerate() {
            let body = r.parts[chassis[ri]].body;
            if let (Some(st), Some(c)) = (
                m.duel.physics.state(body),
                m.duel.physics.center_of_mass(body),
            ) {
                let rot = Mat3::from_quat(st.rotation).to_cols_array();
                write!(
                    out,
                    "{t1:.4}\t{ri}\t{:.5}\t{:.5}\t{:.5}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}",
                    c.x,
                    c.y,
                    c.z,
                    st.linear_velocity.x,
                    st.linear_velocity.y,
                    st.linear_velocity.z,
                    st.angular_velocity.x,
                    st.angular_velocity.y,
                    st.angular_velocity.z
                )?;
                for x in rot {
                    write!(out, "\t{x:.4}")?;
                }
                writeln!(out)?;
            }
        }
    }
    Ok(())
}
