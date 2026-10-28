//! Network play through the shipped screens and a real duel, host and joiner
//! in one process on 127.0.0.1 (ephemeral ports; nothing leaves loopback).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ra2_app::net_play::{self, MatchArena, NetPlay, BROWSER_SCREEN, PREMATCH_SCREEN};
use ra2_shell::Shell;
use ra2net::game::GameEvent;
use ra2net::link::LinkConfig;
use ra2net::session::HOST_ID;
use ra2net::sync::ConnectionSpeed;
use ra2sim::sim::DriveInput;

fn game_dir() -> Option<PathBuf> {
    ra2fmt::saves::isolate_for_tests();
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
    // The create screen's own `server name textbox` default, in the
    // original's six-field session name (docs/38 §14).
    assert_eq!(
        guest.rows[0].session_name,
        "New Exhibition Match:DEATHMATCH:Combat Arena:ALLWEIGHT:1.20:512K LAN:"
    );
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

    // Lobby chat from the guest's entry (Enter): `[Guest] hello` reaches the
    // host's `chat messages` box (a DisplayBox: its markup carries the text),
    // and the box's sample lines are gone.
    guest_shell.click_named("chat textbox").unwrap();
    for c in "hello".chars() {
        guest_shell.type_char(c);
    }
    guest.commit_lobby_chat(&mut guest_shell);
    let shown = |shell: &Shell| display_text(shell.document(), "chat messages");
    let start = Instant::now();
    while !shown(&host_shell).contains("[Guest] hello") && start.elapsed() < Duration::from_secs(5)
    {
        host.tick(&mut host_shell).unwrap();
        guest.tick(&mut guest_shell).unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        shown(&host_shell).contains("[Guest] hello"),
        "{}",
        shown(&host_shell)
    );
    assert!(!shown(&host_shell).contains("Perry"));
    assert!(shown(&guest_shell).contains("[Guest] hello"));

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
    host.host("Sync", None, ConnectionSpeed::Lan, &box_arena())
        .unwrap();
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
    host.on_match_built(&mut host_match);
    guest.on_match_built(&mut guest_match);
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
/// The same match, each process pumping the network the way the windowed app
/// does every frame: `tick`, `before_step`, the step, `after_step`.
const WINDOW_TEST: &str = "the_windowed_frame_order_delivers_every_event_once";

fn report(line: String) {
    println!("NETTEST {line}");
}

fn child(role: &str, window: bool) {
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
        net.host("Scripted", None, ConnectionSpeed::Lan, &box_arena())
            .unwrap();
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
    net.on_match_built(&mut session);
    session.skip_intro();
    let dt = session.matched.config.dt;
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut shell = Shell::boot(&game, [800.0, 600.0]).unwrap();
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
        // The windowed app's frame (`about_to_wait`): the network tick first.
        if window {
            net.tick(&mut shell).unwrap();
        }
        net.before_step(&mut session);
        std::thread::sleep(Duration::from_millis(3));
        // Loaded is reported here; nobody's countdown runs until the host
        // has heard every peer and begun the match (docs/38 §5).
        if net.match_begun() {
            session.step();
        }
        net.after_step(&mut session, dt);
        if window {
            // The event loop idles until the next update; packets land here,
            // where the next frame's `tick` meets them before `after_step`.
            std::thread::sleep(Duration::from_millis(8));
        }
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
    five_processes(SCRIPT_TEST, false);
}

#[test]
fn the_windowed_frame_order_delivers_every_event_once() {
    five_processes(WINDOW_TEST, true);
}

fn five_processes(test: &str, window: bool) {
    if let Ok(role) = std::env::var("RA2_NETTEST_ROLE") {
        child(&role, window);
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
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
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

/// Lobby -> match -> results -> lobby -> a second match: the begin gate, the
/// loaded set and the sync switch start clean each time (docs/38 §12).
#[test]
fn a_rematch_in_the_same_lobby_starts_from_a_clean_gate() {
    use ra2_app::net_play::NetAction;
    let Some(game) = game_dir() else { return };
    let size = [800.0, 600.0];
    let mut host_shell = Shell::boot(&game, size).unwrap();
    let mut guest_shell = Shell::boot(&game, size).unwrap();
    let mut host = loopback("Host");
    let mut guest = loopback("Guest");
    host.host("Rematch", None, ConnectionSpeed::Lan, &box_arena())
        .unwrap();
    host_shell.goto(PREMATCH_SCREEN).unwrap();
    let addr = host.game.as_ref().unwrap().session.local_addr().unwrap();
    guest.join(addr).unwrap();
    let pump = |host: &mut NetPlay, guest: &mut NetPlay, hs: &mut Shell, gs: &mut Shell| {
        host.tick(hs).unwrap();
        let action = guest.tick(gs).unwrap();
        std::thread::sleep(Duration::from_millis(3));
        action
    };
    let start = Instant::now();
    while host.game.as_ref().unwrap().lobby.slots.len() < 2
        && start.elapsed() < Duration::from_secs(5)
    {
        pump(&mut host, &mut guest, &mut host_shell, &mut guest_shell);
    }
    assert_eq!(host.game.as_ref().unwrap().lobby.slots.len(), 2);
    let begin = ra2_shell::Step {
        command: String::new(),
        hit: "Button \"begin match button\"".into(),
        note: None,
        screen: PREMATCH_SCREEN.into(),
        content: None,
        tab: None,
        step: None,
        point: [0.0, 0.0],
    };

    for round in 1..=2 {
        let action = host
            .on_step(&mut host_shell, PREMATCH_SCREEN, &begin)
            .unwrap();
        assert!(matches!(action, NetAction::StartMatch { .. }), "{round}");
        // The joiner's tick hears Prepare (and keeps the in-match events for
        // `after_step`).
        let start = Instant::now();
        let mut prepared = false;
        while !prepared && start.elapsed() < Duration::from_secs(5) {
            prepared = matches!(
                pump(&mut host, &mut guest, &mut host_shell, &mut guest_shell),
                NetAction::StartMatch { .. }
            );
        }
        assert!(prepared, "round {round}: no Prepare");
        // Neither end may run before the host has heard the joiner loaded.
        assert!(
            !host.match_begun(),
            "round {round}: the host began before the joiner loaded"
        );
        assert!(
            !guest.match_begun(),
            "round {round}: the joiner began before the host's go"
        );
        let start = Instant::now();
        while !(host.match_begun() && guest.match_begun())
            && start.elapsed() < Duration::from_secs(5)
        {
            pump(&mut host, &mut guest, &mut host_shell, &mut guest_shell);
        }
        assert!(host.match_begun() && guest.match_begun(), "round {round}");
        assert!(guest.game.as_ref().unwrap().in_match());
        // The results are closed: back in the lobby.
        host.end_match();
        guest.end_match();
        assert!(!guest.game.as_ref().unwrap().in_match(), "round {round}");
        assert!(!host.game.as_ref().unwrap().in_match(), "round {round}");
    }
    host.leave();
    guest.leave();
}

// ---------------------------------------------------------------------------------------------
// Two processes, one host and one joiner, for what happens at the edges of a match (docs/38 §13):
// who drives and who is followed, a joiner that leaves, a host that ends the match.
// ---------------------------------------------------------------------------------------------

const EDGE_TESTS: [&str; 4] = [
    "each_player_drives_and_follows_their_own_robot",
    "a_remote_human_who_leaves_leaves_the_robot_standing",
    "a_joiner_leaves_the_match_when_the_host_ends_it_early",
    "a_finished_match_keeps_the_joiners_results_after_the_end_message",
];

/// Run `test` as a host process and a joiner process; return what each reported on its `RESULT` line.
fn edge_processes(test: &str) -> Option<(String, String)> {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    let _game = game_dir()?;
    let port = {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.local_addr().unwrap().port()
    };
    let save = |role: &str| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("netplay-edge-{role}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    };
    let exe = std::env::current_exe().unwrap();
    let (tx, rx) = mpsc::channel::<(String, String)>();
    let mut children = Vec::new();
    let mut backlog = Vec::new();
    for role in ["host", "guest"] {
        let mut c = Command::new(&exe)
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env("RA2_NETTEST_ROLE", role)
            .env("RA2_NETTEST_PORT", port.to_string())
            .env("ORA_SAVE_DIR", save(role))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let out = c.stdout.take().unwrap();
        let tx = tx.clone();
        let who = role.to_string();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some((_, rest)) = line.split_once("NETTEST ") {
                    let _ = tx.send((who.clone(), rest.to_string()));
                }
            }
        });
        children.push(c);
        if role == "host" {
            // The joiner dials a listening host.
            loop {
                let (r, line) = rx
                    .recv_timeout(Duration::from_secs(60))
                    .expect("host listens");
                if r == "host" && line.starts_with("LISTENING") {
                    break;
                }
                backlog.push((r, line));
            }
        }
    }
    let mut results = std::collections::BTreeMap::new();
    let mut started = false;
    let end = Instant::now() + Duration::from_secs(150);
    while results.len() < 2 {
        let (role, line) = match backlog.pop() {
            Some(x) => x,
            None => {
                let left = end.saturating_duration_since(Instant::now());
                rx.recv_timeout(left).unwrap_or_else(|_| {
                    for c in children.iter_mut() {
                        let _ = c.kill();
                    }
                    panic!("timed out; got {results:?}");
                })
            }
        };
        if line.starts_with("JOINED guest") && !started {
            started = true;
            children[0]
                .stdin
                .as_mut()
                .unwrap()
                .write_all(b"START\n")
                .unwrap();
        }
        if let Some(rest) = line.strip_prefix("RESULT ") {
            results.insert(role, rest.to_string());
        }
    }
    for c in children.iter_mut() {
        let _ = c.kill();
        let _ = c.wait();
    }
    Some((results["host"].clone(), results["guest"].clone()))
}

/// What one end of an edge scenario does; the parent reads its `RESULT` line.
fn edge_child(test: &str, role: &str) {
    use ra2sim::r#match::MatchState;
    use std::io::BufRead;
    let game = game_dir().expect("the game install");
    let _scripts = ra2script::lock();
    let port: u16 = std::env::var("RA2_NETTEST_PORT").unwrap().parse().unwrap();
    let host = role == "host";
    let mut net = loopback(if host { "Host" } else { "Guest" });
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    if host {
        net.host_port = port;
        net.host("Edges", None, ConnectionSpeed::Lan, &box_arena())
            .unwrap();
        report("LISTENING".into());
    } else {
        net.join(addr).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let lobby_len = |net: &NetPlay| net.game.as_ref().map_or(0, |g| g.lobby.slots.len());
    while lobby_len(&net) < 2 && Instant::now() < deadline {
        net.game.as_mut().unwrap().poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(lobby_len(&net), 2, "{role}: never joined");
    report(format!("JOINED {role}"));
    let arena = "Arenas/box.py";
    if host {
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
        let g = net.game.as_mut().unwrap();
        g.set_settings("DEATHMATCH", "Heavyweight", arena);
        g.start_match(arena);
        g.set_schema(net_play::schema());
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
    let mut session = net.match_session(&game, arena).unwrap();
    net.on_match_built(&mut session);
    session.skip_intro();
    let dt = session.matched.config.dt;
    let mut shell = Shell::boot(&game, [800.0, 600.0]).unwrap();
    let mine = if host { 0 } else { 1 };

    // What this end follows and drives (before anything moves).
    let chase = match session.camera {
        ra2sim::DuelCamera::Chase { robot, .. } => robot as i64,
        _ => -1,
    };
    let human = session.matched.duel.human_robot.map_or(-1, |r| r as i64);
    let driven = session.player_robot;
    let mut fight = 0u32;
    let mut quit_at: Option<u32> = None;
    let mut start_pos = [glam::Vec3::ZERO; 2];
    let mut left_at = None;
    let mut keys_from = 0u32;
    let mut over_at = None;
    let mut frames = 0u32;
    // The windowed app's frame order (`about_to_wait`), with its idle.
    let out: String = loop {
        frames += 1;
        assert!(
            Instant::now() < deadline + Duration::from_secs(80),
            "{role}: the scenario did not finish (fight {fight})"
        );
        let fighting = session.matched.state() == MatchState::Fight;
        if fighting {
            fight += 1;
        }
        if fighting && fight == 1 {
            for (i, p) in start_pos.iter_mut().enumerate() {
                *p = session.robot_position(i).unwrap();
            }
        }
        match (test, role) {
            (t, _) if t == EDGE_TESTS[0] && fighting && fight == 20 => {
                // Both hold forward.
                keys_from = fight;
                session.set_drive(DriveInput::new(1.0, 0.0));
            }
            (t, "guest") if t == EDGE_TESTS[1] && fighting && fight == 40 => {
                net.leave();
                break "LEFT".to_string();
            }
            (t, "host") if t == EDGE_TESTS[2] && fighting && fight == 40 => {
                // The host quits its match (the pause dialog's `End Match`) and is back in the
                // lobby, still running its session.
                net.end_match();
                quit_at = Some(frames);
            }
            (t, "host") if t == EDGE_TESTS[3] && fighting && fight == 40 => {
                assert!(session.matched.end_by_judges());
            }
            _ => {}
        }
        net.tick(&mut shell).unwrap();
        if let Some(at) = quit_at {
            // The host's lobby: only the session runs.
            std::thread::sleep(Duration::from_millis(10));
            if frames > at + 150 {
                break format!("QUIT host_state={:?}", session.matched.state());
            }
            continue;
        }
        net.before_step(&mut session);
        std::thread::sleep(Duration::from_millis(3));
        if net.match_begun() {
            session.step();
        }
        net.after_step(&mut session, dt);
        std::thread::sleep(Duration::from_millis(8));
        let over = matches!(
            session.matched.state(),
            MatchState::Over | MatchState::Results
        );
        if !host && (test == EDGE_TESTS[2] || test == EDGE_TESTS[3]) {
            let in_match = net.game.as_ref().is_some_and(|g| g.in_match());
            if net.take_leave_match() {
                break format!(
                    "LEAVE state={:?} in_match={in_match}",
                    session.matched.state()
                );
            }
            if test == EDGE_TESTS[3] && over {
                let first = *over_at.get_or_insert(frames);
                if !in_match && frames > first + 30 {
                    break format!(
                        "OVER state={:?} in_match={in_match} winners={:?}",
                        session.matched.state(),
                        session.matched.report().winners
                    );
                }
            }
        }
        if host && test == EDGE_TESTS[3] && over {
            let first = *over_at.get_or_insert(frames);
            if frames > first + 200 {
                break format!("HOST over state={:?}", session.matched.state());
            }
        }
        if test == EDGE_TESTS[0] && keys_from > 0 && fight >= keys_from + 150 {
            let moved: Vec<String> = (0..2)
                .map(|i| {
                    format!(
                        "{:.2}",
                        (session.robot_position(i).unwrap() - start_pos[i]).length()
                    )
                })
                .collect();
            break format!(
                "DRIVE chase={chase} human={human} driven={driven} mine={mine} moved={}",
                moved.join(",")
            );
        }
        if host && test == EDGE_TESTS[1] {
            match left_at {
                None => {
                    if net.game.as_ref().is_some_and(|g| g.lobby.slots.len() == 1) {
                        left_at = Some(fight);
                        start_pos[1] = session.robot_position(1).unwrap();
                    }
                }
                Some(at) if fight >= at + 400 => {
                    let moved = (session.robot_position(1).unwrap() - start_pos[1]).length();
                    let d = &session.matched.duel;
                    break format!(
                        "STANDING slots=1 moved={moved:.2} state={:?} out={},{} outside={}",
                        session.matched.state(),
                        d.robots[1].immobilized,
                        d.robots[1].destroyed || d.robots[1].eliminated,
                        session.is_driven_from_outside(1),
                    );
                }
                Some(_) => {}
            }
        }
    };
    report(format!("RESULT {out}"));
    net.leave();
}

fn edge_role(test: &str) -> bool {
    match std::env::var("RA2_NETTEST_ROLE") {
        Ok(role) => {
            edge_child(test, &role);
            true
        }
        Err(_) => false,
    }
}

/// The host follows and drives robot 0, the joiner robot 1: its chase camera, the human player the
/// watch and orbit cameras follow, the robot its keys drive, and the keys it sent move that robot
/// on the host.
#[test]
fn each_player_drives_and_follows_their_own_robot() {
    if edge_role(EDGE_TESTS[0]) {
        return;
    }
    let Some((host, guest)) = edge_processes(EDGE_TESTS[0]) else {
        return;
    };
    println!("host: {host}\nguest: {guest}");
    assert!(host.contains("chase=0 human=0 driven=0 mine=0"), "{host}");
    assert!(guest.contains("chase=1 human=1 driven=1 mine=1"), "{guest}");
    let moved = |s: &str| -> Vec<f32> {
        s.split("moved=")
            .nth(1)
            .unwrap()
            .split(',')
            .map(|v| v.trim().parse().unwrap())
            .collect()
    };
    let h = moved(&host);
    assert!(h[0] > 1.0, "the host's robot drives: {host}");
    assert!(
        h[1] > 1.0,
        "the joiner's keys drive robot 1 on the host: {host}"
    );
    let g = moved(&guest);
    assert!(g[1] > 1.0, "the joiner's own robot drives: {guest}");
}

/// A joiner that goes mid-match: the host's lobby loses the slot, the robot stays where it was
/// (no AI takes it over, no `PlayerOut` is sent): the original's `DESTROY_PLAYER` is a lobby event
/// (`0x4930a0`, docs/38 section 13).
#[test]
fn a_remote_human_who_leaves_leaves_the_robot_standing() {
    if edge_role(EDGE_TESTS[1]) {
        return;
    }
    let Some((host, guest)) = edge_processes(EDGE_TESTS[1]) else {
        return;
    };
    println!("host: {host}\nguest: {guest}");
    assert!(guest.starts_with("LEFT"), "{guest}");
    assert!(host.starts_with("STANDING slots=1"), "{host}");
    let moved: f32 = host
        .split("moved=")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(moved < 0.3, "the robot was driven off: {host}");
    assert!(host.contains("out=false,false"), "{host}");
    assert!(host.contains("outside=true"), "{host}");
}

/// The host quits its match early: `giEndMatchMessage` sends the joiner back to the lobby.
#[test]
fn a_joiner_leaves_the_match_when_the_host_ends_it_early() {
    if edge_role(EDGE_TESTS[2]) {
        return;
    }
    let Some((host, guest)) = edge_processes(EDGE_TESTS[2]) else {
        return;
    };
    println!("host: {host}\nguest: {guest}");
    assert!(host.starts_with("QUIT"), "{host}");
    assert!(guest.starts_with("LEAVE"), "{guest}");
    assert!(guest.contains("in_match=false"), "{guest}");
}

/// The normal end: `0x137` then `0x131`; the joiner's match is over with the host's winners, its
/// session is back in the lobby phase, and the window is not asked to leave (the results stay).
#[test]
fn a_finished_match_keeps_the_joiners_results_after_the_end_message() {
    if edge_role(EDGE_TESTS[3]) {
        return;
    }
    let Some((_host, guest)) = edge_processes(EDGE_TESTS[3]) else {
        return;
    };
    println!("guest: {guest}");
    assert!(guest.starts_with("OVER"), "{guest}");
    assert!(guest.contains("in_match=false"), "{guest}");
}

/// The name a player has by default is the original's (`Default Player`) and `ra2.cfg`'s
/// `[Multiplayer Config]` may replace it and the port.
#[test]
fn the_default_player_name_is_the_originals_and_ra2_cfg_replaces_it() {
    let mut net = NetPlay::new();
    assert_eq!(net.player_name, "Default Player");
    net.apply_config(&ra2fmt::cfg::Cfg::parse(
        "[Begin Multiplayer Config]\r\nPlayerName=Wedge\r\nPort=4242\r\n[End Multiplayer Config]\r\n",
    ));
    assert_eq!(net.player_name, "Wedge");
    assert_eq!(net.host_port, 4242);
    // An empty name keeps the default.
    let mut net = NetPlay::new();
    net.apply_config(&ra2fmt::cfg::Cfg::parse("PlayerName=\r\n"));
    assert_eq!(net.player_name, "Default Player");
}

/// Combat Arena as the messages name it (without reading an install).
fn box_arena() -> MatchArena {
    MatchArena {
        script: net_play::DEFAULT_ARENA.to_string(),
        name: "Combat Arena".to_string(),
        preview: "box/box_preview.bmp".to_string(),
    }
}

/// The markup of the `DisplayBox` whose body is `name`.
fn display_text(gib: &ra2fmt::gib::Gib, name: &str) -> String {
    let mut out = String::new();
    for w in gib.walk() {
        for a in &w.attachments {
            if a.body.as_ref().is_some_and(|b| b.name() == name) {
                if let Some(f) = a.field("Display") {
                    out.push_str(&f.raw);
                }
            }
        }
    }
    out
}

/// The snapshot of a match is the one the original's host wrote for the same two robots (docs/38
/// section 15; recorded live, C.R.U.S.H. as panel 0 against Sting Ray as panel 1): the same bodies
/// under the same ids, hitpoints, batteries and motors.
#[test]
fn the_snapshot_of_two_robots_has_the_ids_the_originals_has() {
    use ra2net::sync::DeviceSync;
    let Some(game) = game_dir() else { return };
    let _scripts = ra2script::lock();
    let entries = vec![
        ra2sim::r#match::RobotEntry::human(
            0,
            game.join("Teams/Team0/Bot0.bot")
                .to_string_lossy()
                .to_string(),
        ),
        ra2sim::r#match::RobotEntry::human(
            1,
            game.join("Teams/Team0/Bot1.bot")
                .to_string_lossy()
                .to_string(),
        ),
    ];
    let session = ra2_app::match_mode::Session::exhibition(
        &game,
        &game.join("Arenas/Bridge.py"),
        ra2sim::r#match::GameType::Deathmatch,
        entries,
        None,
        None,
    );
    let Ok(session) = session else { return };
    let state = ra2_app::net_match::capture(&session.matched.duel, &[0, 1]);
    let mut ids: Vec<u32> = state.bodies.iter().map(|b| b.id).collect();
    ids.sort();
    assert_eq!(
        ids,
        [0x400, 0x404, 0x408, 0x40c, 0x410, 0x424, 0x800, 0x805, 0x809]
    );
    assert_eq!(state.robots[0].components.len(), 14);
    assert_eq!(state.robots[1].components.len(), 15);
    assert_eq!(state.robots[0].components[0], 1226.0);
    assert_eq!(state.robots[1].components[0], 926.0);
    let controllers: Vec<u32> = state.controllers.iter().map(|c| c.0).collect();
    assert_eq!(controllers, [0x42c, 0x824, 0x828, 0x82c]);
    assert_eq!(state.controllers[0].1, 52000.0);
    assert_eq!(state.controllers[1].1, 7000.0);
    let devices: Vec<u32> = state
        .devices
        .iter()
        .map(|d| match d {
            DeviceSync::One { id, .. }
            | DeviceSync::Four { id, .. }
            | DeviceSync::Five { id, .. } => *id,
            DeviceSync::Other { id, .. } => *id,
        })
        .collect();
    assert_eq!(devices, [0x404, 0x408, 0x40c, 0x410, 0x424, 0x804, 0x808]);
    // The chassis are awake in full, the other bodies by position and rotation.
    assert!(state.partial);
    assert!(state.bodies.iter().all(|b| b.awake == (b.id & 0x3ff == 0)));
}
