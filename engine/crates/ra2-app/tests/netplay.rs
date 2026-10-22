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

// ---------------------------------------------------------------------------
// A scripted match across five processes (docs/38 §6.1, §10, §11).
//
// The script layer is process-global, so the host, three players and a
// spectator each run in their own process (this test binary again, told its
// role by RA2_NETTEST_ROLE) and talk over loopback. The host lands scripted
// blows; a joiner lands one of its own that must count for nothing; chat goes
// out from a player, the host and the spectator. Every process reports what it
// ended with, and they must agree.
// ---------------------------------------------------------------------------

const SCRIPT_TEST: &str = "a_scripted_match_across_five_processes_counts_every_hit_once";

fn report(line: String) {
    println!("NETTEST {line}");
}

fn child(role: &str) {
    use std::io::BufRead;
    let game = game_dir().expect("the game install");
    let _scripts = ra2script::lock();
    let port: u16 = std::env::var("RA2_NETTEST_PORT").unwrap().parse().unwrap();
    let (name, expect) = match role {
        "host" => ("Host", 1),
        "p1" => ("Guest1", 2),
        "p2" => ("Guest2", 3),
        "p3" => ("Guest3", 4),
        _ => ("Watcher", 5),
    };
    let mut net = loopback(name);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    if role == "host" {
        net.host_port = port;
        net.host("Scripted", None, ConnectionSpeed::Lan).unwrap();
        report("LISTENING".into());
    } else {
        net.join(addr).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let lobby_len = |net: &NetPlay| net.game.as_ref().map_or(0, |g| g.lobby.slots.len());
    while lobby_len(&net) < expect && Instant::now() < deadline {
        net.game.as_mut().unwrap().poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(lobby_len(&net) >= expect, "{role}: never joined");
    report(format!("JOINED {role}"));

    // The host waits for the parent's START; everyone else for Prepare.
    let arena = "Arenas/box.py";
    if role == "host" {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = std::io::stdin().lock().read_line(&mut line);
            let _ = tx.send(());
        });
        while rx.try_recv().is_err() && Instant::now() < deadline {
            net.game.as_mut().unwrap().poll();
            std::thread::sleep(Duration::from_millis(2));
        }
        let game_net = net.game.as_mut().unwrap();
        game_net.set_settings("DEATHMATCH", "Heavyweight", arena);
        game_net.start_match(arena);
        game_net.set_schema(net_play::schema());
        net.assign_robots();
    } else {
        let mut prepared = false;
        while !prepared && Instant::now() < deadline {
            for e in net.game.as_mut().unwrap().poll() {
                if let GameEvent::Prepare { .. } = e {
                    prepared = true;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(prepared, "{role}: no Prepare");
        net.game.as_mut().unwrap().set_schema(net_play::schema());
        net.assign_robots();
    }
    let spectating = net.is_spectator();
    assert_eq!(spectating, role == "watch", "{role}");
    let mut session = net.match_session(&game, arena).unwrap();
    session.skip_intro();
    let dt = session.matched.config.dt;
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut fight = 0u32;
    let mut after_end = 0u32;
    let mut ended = false;
    while Instant::now() < deadline + Duration::from_secs(60) {
        let fighting = session.matched.state() == ra2sim::r#match::MatchState::Fight;
        if fighting {
            fight += 1;
        }
        // The script, by fight step.
        match (role, fight, fighting) {
            ("host", 30, true) => session.matched.hit(0, 1, 0, 30.0),
            ("host", 60, true) => session.matched.hit(1, 0, 0, 45.5),
            ("host", 90, true) => session.matched.hit(0, 1, 0, 12.7),
            ("host", 100, true) => {
                session.toggle_chat();
                "good luck".chars().for_each(|c| session.chat_type(c));
                session.chat_submit();
            }
            ("host", 120, true) => {
                // A hazard's blow: no attacker, no points.
                let at = glam::Vec3::ZERO;
                session.matched.duel.apply_hit(None, 2, 0, 25.0, at);
            }
            // A joiner's own pricing of a blow: it must count for nothing.
            ("p2", 40, true) => session.matched.hit(2, 0, 0, 100.0),
            ("p1", 45, true) => {
                session.toggle_chat();
                "gg".chars().for_each(|c| session.chat_type(c));
                session.chat_submit();
            }
            ("watch", 70, true) => {
                session.toggle_chat();
                "nice fight".chars().for_each(|c| session.chat_type(c));
                session.chat_submit();
            }
            ("host", 400, true) => {
                assert!(session.matched.end_by_judges());
            }
            _ => {}
        }
        net.before_step(&mut session);
        std::thread::sleep(Duration::from_millis(3));
        // Loaded is reported here; nobody's countdown runs until the host
        // has heard every peer and begun the match (docs/38 §5).
        if net.match_begun() {
            session.step();
        }
        net.after_step(&mut session, dt);
        for line in session.chat_lines() {
            seen.insert(line.to_string());
        }
        let over = matches!(
            session.matched.state(),
            ra2sim::r#match::MatchState::Over | ra2sim::r#match::MatchState::Results
        );
        if over {
            ended = true;
        }
        if ended {
            after_end += 1;
            if after_end > 150 {
                break;
            }
        }
    }
    let report_data = session.matched.report();
    let points: Vec<String> = (0..4).map(|i| ra2script::points(i).to_string()).collect();
    let taken: Vec<String> = session
        .matched
        .duel
        .damage_taken
        .iter()
        .map(|d| format!("{d:.2}"))
        .collect();
    report(format!(
        "FINAL {role} ended={ended} points={} taken={} winners={:?} chat={}",
        points.join(","),
        taken.join(","),
        report_data.winners,
        seen.into_iter().collect::<Vec<_>>().join("|")
    ));
    net.leave();
}

#[test]
fn a_scripted_match_across_five_processes_counts_every_hit_once() {
    if let Ok(role) = std::env::var("RA2_NETTEST_ROLE") {
        child(&role);
        return;
    }
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    let Some(_game) = game_dir() else { return };
    let port = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    };
    let save = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("netplay-save");
    std::fs::create_dir_all(&save).unwrap();
    let exe = std::env::current_exe().unwrap();
    let (tx, rx) = mpsc::channel::<(String, String)>();
    let mut children = Vec::new();
    let spawn = |role: &str, children: &mut Vec<std::process::Child>| {
        let mut c = Command::new(&exe)
            .args(["--exact", SCRIPT_TEST, "--nocapture", "--test-threads=1"])
            .env("RA2_NETTEST_ROLE", role)
            .env("RA2_NETTEST_PORT", port.to_string())
            .env("ORA_SAVE_DIR", &save)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let out = c.stdout.take().unwrap();
        let tx = tx.clone();
        let role = role.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                // libtest may have started the line with "test name ... ".
                if let Some((_, rest)) = line.split_once("NETTEST ") {
                    let _ = tx.send((role.clone(), rest.to_string()));
                }
            }
        });
        children.push(c);
    };
    let wait_for = |what: &str, role: &str| {
        let end = Instant::now() + Duration::from_secs(90);
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let (r, line) = rx.recv_timeout(left).unwrap_or_else(|_| {
                panic!("timed out waiting for {role} {what}");
            });
            if r == role && line.starts_with(what) {
                return line;
            }
        }
    };
    spawn("host", &mut children);
    wait_for("LISTENING", "host");
    for role in ["p1", "p2", "p3", "watch"] {
        spawn(role, &mut children);
        wait_for("JOINED", role);
    }
    children[0]
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"START\n")
        .unwrap();
    let mut finals = std::collections::BTreeMap::new();
    let end = Instant::now() + Duration::from_secs(150);
    while finals.len() < 5 {
        let left = end.saturating_duration_since(Instant::now());
        let (role, line) = rx.recv_timeout(left).expect("every process reports");
        if let Some(rest) = line.strip_prefix("FINAL ") {
            finals.insert(role, rest.to_string());
        }
    }
    for c in children.iter_mut() {
        let _ = c.kill();
        let _ = c.wait();
    }
    // Everything after "FINAL <role> " must agree across the five.
    let body = |s: &String| s.split_once(' ').unwrap().1.to_string();
    let host = body(&finals["host"]);
    println!("host ended with: {host}");
    for (role, line) in &finals {
        assert_eq!(body(line), host, "{role} disagrees with the host");
    }
    assert!(host.contains("ended=true"), "{host}");
    // 30 + 12 (12.7 truncated) for robot 0, 45 (45.5 truncated) for robot 1;
    // the hazard scores nobody; the joiner's own 100 counts for nothing.
    assert!(host.contains("points=42,45,0,0"), "{host}");
    assert!(host.contains("taken=45.50,42.70,25.00,0.00"), "{host}");
    assert!(host.contains("winners=[1]"), "{host}");
    for line in ["[Guest1] gg", "[Host] good luck", "[Watcher] nice fight"] {
        assert!(host.contains(line), "{line} missing: {host}");
    }
}
