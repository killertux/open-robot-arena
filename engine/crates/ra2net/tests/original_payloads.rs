//! The original's own game payloads, as its `SendTo` handed them to the
//! replacement `dpnet` (captures `mp-payloads-*` in `local/oracle/original`,
//! docs/38 §14): every one decodes and re-encodes byte for byte, and the
//! fields mean what the lobby and the match take them to mean. Skipped when
//! the captures are not on this machine.

use std::path::PathBuf;

use ra2net::lobby::{self, kind, Lobby};
use ra2net::message::Message;
use ra2net::sync::SyncSchema;

fn oracle() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/oracle/original");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// One `send` line: ms, to, flags, payload.
type Sent = (u64, u32, u32, Vec<u8>);

/// Every `send` line of a capture.
fn sends(name: &str) -> Option<Vec<Sent>> {
    let text = std::fs::read_to_string(oracle()?.join(name)).ok()?;
    let word = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
    Some(
        text.lines()
            .map(|l| {
                let f: Vec<&str> = l.split(' ').collect();
                assert_eq!(f[1], "send", "{l}");
                let hex = f[4];
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect();
                (f[0].parse().unwrap(), word(f[2]), word(f[3]), bytes)
            })
            .collect(),
    )
}

const CAPTURES: [&str; 3] = [
    "mp-payloads-host__live__000.txt",
    "mp-payloads-host__live__001.txt",
    "mp-payloads-joiner__live__000.txt",
];

#[test]
fn every_payload_the_original_sent_round_trips_byte_for_byte() {
    let mut total = 0;
    let mut types = std::collections::BTreeSet::new();
    for name in CAPTURES {
        let Some(lines) = sends(name) else { return };
        for (_, _, _, bytes) in lines {
            let m = Message::decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(m.encode(), bytes, "{name}: {m:?}");
            types.insert(m.id());
            total += 1;
            // Snapshots: the size word is the inflated length (a u32, not a
            // float), the snapshots of these matches carry no robot or body.
            if let Message::Sync(sync) = &m {
                let state = sync.unpack(&SyncSchema::default()).unwrap();
                assert!(state.bodies.is_empty() && state.robots.is_empty());
                assert_eq!(sync.raw_size, 17);
            }
        }
    }
    assert_eq!(total, 2411 + 4043 + 13);
    // 0x65 0x71 0x74 0x12d..0x13f as listed in docs/38 §14.
    assert_eq!(types.len(), 15, "{types:x?}");
}

#[test]
fn the_hosts_lobby_messages_mean_panels_owners_and_names() {
    let Some(lines) = sends("mp-payloads-host__live__000.txt") else {
        return;
    };
    let msgs: Vec<Message> = lines
        .iter()
        .map(|(_, _, _, b)| Message::decode(b).unwrap())
        .collect();
    let host = 0x10001;
    let mut view = Lobby::default();
    for m in &msgs {
        view.apply(host, host, m);
    }
    // Panel 0: the host's own robot; panel 1: a computer robot it set up.
    let p0 = view.panel(0).unwrap();
    assert_eq!(
        (p0.player, p0.kind, p0.team.as_str(), p0.team_logo.as_str()),
        (
            host,
            kind::HUMAN_WITH_ROBOT,
            "The Rookies",
            "Teams\\orange_skull.bmp"
        )
    );
    assert_eq!(
        (p0.robot.as_str(), p0.name.as_str()),
        ("C.R.U.S.H.", "Default Player")
    );
    let p1 = view.panel(1).unwrap();
    assert_eq!(
        (p1.player, p1.kind, p1.team.as_str(), p1.robot.as_str()),
        (host, kind::COMPUTER_WITH_ROBOT, "Red Zone", "Scout")
    );
    assert_eq!(lobby::arena_name(&view.arena), "Bridge of Doom");
    assert_eq!(view.game_type, "DEATHMATCH");
    assert_eq!(view.weight_class, "ALLWEIGHT");
    // The panels re-encode as the host wrote them.
    let written: Vec<&Vec<u8>> = lines
        .iter()
        .filter(|(_, _, _, b)| b[0] == 0x71)
        .map(|(_, _, _, b)| b)
        .collect();
    let ours: Vec<Vec<u8>> = view.group_infos().iter().map(Message::encode).collect();
    assert_eq!(ours.len(), 2);
    assert_eq!(&ours[0], written[0]);
    assert_eq!(&ours[1], written[1]);
    // The match: prepared by the arena's name, begun at once.
    assert!(msgs.contains(&Message::PrepareMatch {
        flag: true,
        arena: "Bridge of Doom".into()
    }));
    let order: Vec<u32> = msgs
        .iter()
        .map(Message::id)
        .filter(|&t| t == 0x12e || t == 0x130)
        .collect();
    assert_eq!(order, [0x12e, 0x130]);
}

#[test]
fn the_joiner_takes_a_panel_readies_by_it_and_sends_its_bot_on_prepare() {
    let Some(lines) = sends("mp-payloads-joiner__live__000.txt") else {
        return;
    };
    let me = 0x10002;
    let groups: Vec<(u32, u32, ra2net::message::GroupInfo)> = lines
        .iter()
        .filter_map(|(_, to, flags, b)| match Message::decode(b).unwrap() {
            Message::GroupInfo(g) => Some((*to, *flags, g)),
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 2);
    for (to, flags, g) in &groups {
        // To everyone, not guaranteed; its own id as the owner of panel 0.
        assert_eq!((*to, *flags, g.slot, g.owner), (0, 0x20, 0, me));
    }
    assert!(
        !groups[0].2.flag && groups[1].2.flag,
        "ready is the panel's flag"
    );
    // A port host takes the claim of its open panel 0.
    let mut host = Lobby::default();
    host.join(0x10001, "Host");
    host.join(me, "Default Player");
    for (_, _, g) in &groups {
        host.apply(me, 0x10001, &Message::GroupInfo(g.clone()));
    }
    let p0 = host.panel(0).unwrap();
    assert!(p0.player == me && p0.ready && p0.robot == "C.R.U.S.H.");
    // After PrepareMatch: its robot as `.bot` text in an AddArenaBot.
    let bot = lines
        .iter()
        .find_map(|(_, _, _, b)| match Message::decode(b).unwrap() {
            Message::AddArenaBot { words, design } => Some((words, design)),
            _ => None,
        })
        .unwrap();
    assert_eq!(bot.0, [1, 0, 0]);
    assert!(bot.1.starts_with(b"1.12\nName: C.R.U.S.H.\n"));
}
