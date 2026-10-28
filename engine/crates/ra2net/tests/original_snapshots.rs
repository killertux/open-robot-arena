//! The original host's own `giSyncMessage` snapshots (capture `mp-snapshots-host__live__000` in
//! `local/oracle/original`, docs/38 section 15): 119 inflated snapshots of matches of the host's
//! C.R.U.S.H. against twelve other robots, each one decoded against the two designs' component counts
//! and written back byte for byte. Skipped when the capture or the game files are not on this machine.

use std::path::PathBuf;

use ra2net::sync::{DeviceSync, SyncSchema, SyncState};

fn up_to(name: &str) -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn components(game: &std::path::Path, rel: &str) -> usize {
    let bytes = std::fs::read(game.join(rel)).expect("a shipped robot");
    ra2fmt::bot::parse(&bytes).expect("a bot").components.len()
}

#[test]
fn every_snapshot_the_original_wrote_decodes_and_writes_back_byte_for_byte() {
    let (Some(file), Some(game)) = (
        up_to("local/oracle/original/mp-snapshots-host__live__000.txt"),
        up_to("local/game"),
    ) else {
        return;
    };
    let text = std::fs::read_to_string(file).unwrap();
    let mut seen = 0;
    let mut kinds = std::collections::BTreeSet::new();
    let mut partial = [0, 0];
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        assert_eq!(f[1], "snapshot", "{line}");
        let raw: Vec<u8> = (0..f[4].len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&f[4][i..i + 2], 16).unwrap())
            .collect();
        let schema = SyncSchema {
            robot_components: vec![components(&game, f[2]), components(&game, f[3])],
        };
        let state =
            SyncState::from_bytes(&raw, &schema).unwrap_or_else(|e| panic!("{line}: {e:?}"));
        assert!(state.to_bytes() == raw, "{f:?} does not write back");
        // The robots are the two panels, and a body id names its robot and component.
        assert_eq!(
            state.robots.iter().map(|r| r.id).collect::<Vec<_>>(),
            [0, 1]
        );
        for body in &state.bodies {
            assert!(matches!(body.id >> 10, 1 | 2), "{:#x}", body.id);
            assert_eq!(body.id & 3 < 2, true);
        }
        for (id, _) in &state.controllers {
            assert!(matches!(id >> 10, 1 | 2) && id & 3 == 0, "{id:#x}");
        }
        for d in &state.devices {
            kinds.insert(match d {
                DeviceSync::One { .. } => 1,
                DeviceSync::Four { .. } => 4,
                DeviceSync::Five { .. } => 5,
                DeviceSync::Other { kind, .. } => *kind,
            });
        }
        partial[usize::from(state.partial)] += 1;
        // The chassis bodies are in every snapshot, awake, in full.
        let chassis: Vec<_> = state.bodies.iter().filter(|b| b.id & 0x3ff == 0).collect();
        assert!(chassis.iter().all(|b| b.awake));
        seen += 1;
    }
    assert!(seen >= 100, "{seen} snapshots");
    assert_eq!(kinds.into_iter().collect::<Vec<_>>(), [1, 4, 5]);
    assert!(
        partial[0] > 0 && partial[1] > 0,
        "both snapshot forms: {partial:?}"
    );
}
