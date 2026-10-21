//! Network play through the shipped screens and a real duel, host and joiner
//! in one process on 127.0.0.1 (ephemeral ports; nothing leaves loopback).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ra2_app::net_play::{self, NetPlay, BROWSER_SCREEN, PREMATCH_SCREEN};
use ra2_shell::Shell;
use ra2net::game::GameEvent;
use ra2net::link::LinkConfig;
use ra2net::session::HOST_ID;
use ra2net::sync::ConnectionSpeed;
use ra2sim::sim::DriveInput;

fn game_dir() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/game");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn loopback(name: &str) -> NetPlay {
    let mut net = NetPlay::new();
    net.bind_ip = "127.0.0.1".parse().unwrap();
    net.host_port = 0;
    net.extra_hosts.clear();
    net.player_name = name.to_string();
    net.link = LinkConfig {
        rto: Duration::from_millis(50),
        keepalive: Duration::from_millis(100),
        // Building a match takes seconds on a loaded machine and nothing polls
        // meanwhile; keep the links alive through it.
        timeout: Duration::from_secs(30),
        ..LinkConfig::default()
    };
    net
}

/// Refresh lists the host, Join reaches the pre-match lobby on both screens.
#[test]
fn refresh_join_and_the_prematch_lobby_through_the_screens() {
    let Some(game) = game_dir() else { return };
    let size = [800.0, 600.0];
    let mut host_shell = Shell::boot(&game, size).unwrap();
    let mut guest_shell = Shell::boot(&game, size).unwrap();
    let mut host = loopback("Host");
    let mut guest = loopback("Guest");

    // The host: Multiplayer -> Create Game -> Start Game.
    host_shell.click_named("Multiplayer button").unwrap();
    host_shell.click_named("Create Game button").unwrap();
    let screen = host_shell.screen_id();
    let step = host_shell.click_named("start game button").unwrap();
    host.on_step(&mut host_shell, &screen, &step).unwrap();
    assert!(host_shell
        .base_screen_id()
        .eq_ignore_ascii_case(PREMATCH_SCREEN));
    let addr = host.game.as_ref().unwrap().session.local_addr().unwrap();

    // The guest: Multiplayer -> Refresh (querying the host's port) -> the row.
    guest_shell.click_named("Multiplayer button").unwrap();
    guest.extra_hosts = vec![addr];
    let step = guest_shell.click_named("Refresh button").unwrap();
    guest
        .on_step(&mut guest_shell, BROWSER_SCREEN, &step)
        .unwrap();
    let start = Instant::now();
    while guest.rows.is_empty() && start.elapsed() < Duration::from_secs(5) {
        host.tick(&mut host_shell).unwrap();
        guest.tick(&mut guest_shell).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(guest.rows.len(), 1);
    // The create screen's own `server name textbox` default.
    assert_eq!(guest.rows[0].session_name, "New Exhibition Match");
    assert_eq!(guest.rows[0].addr, addr);

    // Select it and Join.
    let step = guest_shell.click_list_row("server listbox", 0).unwrap();
    guest
        .on_step(&mut guest_shell, BROWSER_SCREEN, &step)
        .unwrap();
    let step = guest_shell.click_named("Join game button").unwrap();
    guest
        .on_step(&mut guest_shell, BROWSER_SCREEN, &step)
        .unwrap();
    let start = Instant::now();
    while !(guest_shell
        .base_screen_id()
        .eq_ignore_ascii_case(PREMATCH_SCREEN)
        && guest
            .game
            .as_ref()
            .is_some_and(|g| g.lobby.slots.len() == 2))
        && start.elapsed() < Duration::from_secs(5)
    {
        host.tick(&mut host_shell).unwrap();
        guest.tick(&mut guest_shell).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(guest_shell
        .base_screen_id()
        .eq_ignore_ascii_case(PREMATCH_SCREEN));
    let lobby = &guest.game.as_ref().unwrap().lobby;
    assert_eq!(lobby.slots[0].player, HOST_ID);
    assert_eq!(lobby.slots[1].name, "Guest");
    assert_eq!(host.game.as_ref().unwrap().lobby.slots.len(), 2);

    // Ready on the guest's screen reaches the host.
    let step = ra2_shell::Step {
        command: String::new(),
        hit: "CheckBox \"ready checkbox\"".into(),
        note: None,
        screen: PREMATCH_SCREEN.into(),
        content: None,
        tab: None,
        step: None,
        point: [0.0, 0.0],
    };
    guest
        .on_step(&mut guest_shell, PREMATCH_SCREEN, &step)
        .unwrap();
    let start = Instant::now();
    let me = guest.game.as_ref().unwrap().local().unwrap();
    while !host.game.as_ref().unwrap().lobby.slot(me).unwrap().ready
        && start.elapsed() < Duration::from_secs(5)
    {
        host.tick(&mut host_shell).unwrap();
        guest.tick(&mut guest_shell).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(host.game.as_ref().unwrap().lobby.slot(me).unwrap().ready);

    // Disconnect: the guest is back on the server list, the host sees it go.
    let step = ra2_shell::Step {
        hit: "Button \"disconnect button\"".into(),
        ..step
    };
    guest
        .on_step(&mut guest_shell, PREMATCH_SCREEN, &step)
        .unwrap();
    assert!(guest_shell
        .base_screen_id()
        .eq_ignore_ascii_case(BROWSER_SCREEN));
    let start = Instant::now();
    while host.game.as_ref().unwrap().lobby.slots.len() > 1
        && start.elapsed() < Duration::from_secs(5)
    {
        host.tick(&mut host_shell).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.game.as_ref().unwrap().lobby.slots.len(), 1);
    host.leave();
}

/// A short synced match on the box arena: the joiner drives its robot, the
/// second slot, forward; the host simulates it from the remote input and the joiner's
/// duel follows the host's snapshots.
#[test]
fn a_synced_duel_follows_the_host() {
    let Some(game) = game_dir() else { return };
    let _scripts = ra2script::lock();
    let mut host = loopback("Host");
    let mut guest = loopback("Guest");
    host.host("Sync", None, ConnectionSpeed::Lan).unwrap();
    let addr = host.game.as_ref().unwrap().session.local_addr().unwrap();
    guest.join(addr).unwrap();
    let start = Instant::now();
    while guest.game.as_ref().unwrap().lobby.slots.len() < 2
        && start.elapsed() < Duration::from_secs(5)
    {
        host.game.as_mut().unwrap().poll();
        guest.game.as_mut().unwrap().poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    host.game
        .as_mut()
        .unwrap()
        .set_settings("DEATHMATCH", "Heavyweight", "Arenas/box.py");
    host.game.as_mut().unwrap().start_match("Arenas/box.py");
    host.game.as_mut().unwrap().set_schema(net_play::schema());
    for (i, slot) in host
        .game
        .as_ref()
        .unwrap()
        .lobby
        .slots
        .clone()
        .iter()
        .enumerate()
    {
        host.robot_of.insert(slot.player, i);
    }
    // The guest hears Prepare.
    let start = Instant::now();
    let mut prepared = false;
    while !prepared && start.elapsed() < Duration::from_secs(5) {
        host.game.as_mut().unwrap().poll();
        for e in guest.game.as_mut().unwrap().poll() {
            if let GameEvent::Prepare { .. } = e {
                prepared = true;
            }
        }
    }
    assert!(prepared);
    guest.game.as_mut().unwrap().set_schema(net_play::schema());
    guest
        .robot_of
        .insert(guest.game.as_ref().unwrap().local().unwrap(), 1);

    let mut host_match = host.match_session(&game, "Arenas/box.py").unwrap();
    let mut guest_match = guest.match_session(&game, "Arenas/box.py").unwrap();
    assert_eq!(host_match.player_robot, 0);
    assert_eq!(guest_match.player_robot, 1);
    host_match.skip_intro();
    guest_match.skip_intro();
    let start_pos = host_match.robot_position(1).unwrap();
    guest_match.set_drive(DriveInput::new(1.0, 0.0));
    let dt = host_match.matched.config.dt;
    for _ in 0..480 {
        guest.before_step(&mut guest_match);
        // Let the input arrive before the host steps.
        std::thread::sleep(Duration::from_millis(1));
        host.before_step(&mut host_match);
        host_match.step();
        host.after_step(&mut host_match, dt);
        guest_match.step();
        guest.after_step(&mut guest_match, dt);
    }
    // Let the last snapshots land, without stepping the guest further.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(200) {
        host.after_step(&mut host_match, 0.0);
        guest.after_step(&mut guest_match, 0.0);
    }
    // One last snapshot of the host's final state; wait until it has landed.
    host.after_step(&mut host_match, 1.0);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        host.after_step(&mut host_match, 0.0);
        guest.after_step(&mut guest_match, 0.0);
        let gap = host_match.robot_position(1).unwrap() - guest_match.robot_position(1).unwrap();
        if gap.length() < 0.05 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let moved = host_match.robot_position(1).unwrap() - start_pos;
    assert!(
        moved.length() > 0.5,
        "the remote robot moved on the host: {moved:?}"
    );
    let host_pos = host_match.robot_position(1).unwrap();
    let guest_pos = guest_match.robot_position(1).unwrap();
    assert!(
        (host_pos - guest_pos).length() < 0.05,
        "the joiner's robot is where the host's is: {host_pos:?} vs {guest_pos:?}"
    );
    let h0 = host_match.robot_position(0).unwrap();
    let g0 = guest_match.robot_position(0).unwrap();
    assert!((h0 - g0).length() < 0.05, "{h0:?} vs {g0:?}");
    host.leave();
    guest.leave();
}
