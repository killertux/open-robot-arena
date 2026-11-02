//! `zoneprobe <arena.py> <bot0> <bot1> < queries` (docs/29 section 12ao): the
//! smallest margin by which an arena zone must be grown for a robot, moved
//! rigidly to a live chassis pose, to touch it. One query per line, tab
//! separated: `robot_index zone_index com_x com_y com_z r00 r01 r02 r10 r11
//! r12 r20 r21 r22 transposed`; one answer per line (`-` for none up to 1 m).
use glam::{Mat3, Vec3};
use ra2sim::{GameType, Match, MatchConfig, RobotEntry};
use std::io::BufRead;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cfg = MatchConfig::new(
        a[0].clone(),
        GameType::Deathmatch,
        vec![
            RobotEntry::ai(0, a[1].clone()),
            RobotEntry::ai(1, a[2].clone()),
        ],
    );
    let m = Match::new(cfg)?;
    let margins: Vec<f32> = (0..=40).map(|i| i as f32 * 0.025).collect();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let c: Vec<f32> = line.split('\t').map(|v| v.parse().unwrap()).collect();
        let rows = Mat3::from_cols(
            Vec3::new(c[5], c[6], c[7]),
            Vec3::new(c[8], c[9], c[10]),
            Vec3::new(c[11], c[12], c[13]),
        );
        // The cols above are the file's rows: a row-major matrix is the
        // transpose of the column reading.
        let rot = if c[14] > 0.5 { rows } else { rows.transpose() };
        let got = m.duel.zone_probe_margin(
            c[0] as usize,
            Vec3::new(c[2], c[3], c[4]),
            rot,
            c[1] as usize,
            &margins,
        );
        println!("{}", got.map_or("-".to_string(), |g| format!("{g:.3}")));
    }
    Ok(())
}
