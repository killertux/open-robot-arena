//! Two sessions on 127.0.0.1 with ephemeral ports: discovery, join, lobby, a
//! short synced match, disconnects. Nothing leaves the loopback interface.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use ra2net::game::{GameEvent, NetGame};
use ra2net::link::LinkConfig;
use ra2net::lobby::LobbyChange;
use ra2net::message::{BotInput, Message};
use ra2net::session::{
    Browser, Event, HostConfig, JoinConfig, LeaveReason, Refusal, Session, Target, HOST_ID,
};
use ra2net::sync::{BodySync, ConnectionSpeed, SyncSchema, SyncState};

fn any() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

fn fast_link() -> LinkConfig {
    LinkConfig {
        rto: Duration::from_millis(50),
        keepalive: Duration::from_millis(100),
        timeout: Duration::from_millis(1500),
        ..LinkConfig::default()
    }
}

fn host_config(max_players: u32) -> HostConfig {
    HostConfig {
        bind: any(),
        session_name: "Loopback Arena".into(),
        player_name: "Host".into(),
        max_players,
        password: None,
        link: fast_link(),
    }
}

fn join_config(host: SocketAddr, name: &str) -> JoinConfig {
    JoinConfig {
        link: fast_link(),
        connect_timeout: Duration::from_secs(3),
        ..JoinConfig::new(any(), host, name)
    }
}

/// Poll both until `done` sees what it waits for (or fail after 5 s).
fn pump<F>(a: &mut NetGame, b: &mut NetGame, mut done: F) -> (Vec<GameEvent>, Vec<GameEvent>)
where
    F: FnMut(&[GameEvent], &[GameEvent]) -> bool,
{
    let (mut ea, mut eb) = (Vec::new(), Vec::new());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        ea.extend(a.poll());
        eb.extend(b.poll());
        if done(&ea, &eb) {
            return (ea, eb);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("timed out; host saw {ea:?}, joiner saw {eb:?}");
}

#[test]
fn a_browser_finds_the_host_by_enumeration() {
    let mut host = Session::host(host_config(4)).unwrap();
    host.set_host_data(ra2net::dpn::RaHostData {
        slots_23: 1,
        slots_01: 1,
    });
    let addr = host.local_addr().unwrap();
    let mut browser = Browser::new(any()).unwrap();
    browser.query(addr).unwrap();
    let start = Instant::now();
    while browser.hosts.is_empty() && start.elapsed() < Duration::from_secs(5) {
        host.poll();
        browser.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let row = browser.hosts.first().expect("the host answered");
    assert_eq!(row.addr, addr);
    assert_eq!(row.session_name, "Loopback Arena");
    assert_eq!((row.current_players, row.max_players), (1, 4));
    assert!(!row.password);
    assert_eq!(row.data.slots_23, 1);
}

#[test]
fn join_lobby_ready_and_chat() {
    let mut host = NetGame::host(host_config(4), ConnectionSpeed::Lan).unwrap();
    host.set_settings("DEATHMATCH", "Heavyweight", "Arenas/box.py");
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Guest")).unwrap();

    // Join: accepted, and the lobby arrives with the settings.
    let (eh, _) = pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Joined { .. }))
            && g.iter()
                .any(|e| matches!(e, GameEvent::Lobby(LobbyChange::Settings)))
    });
    let me = guest.local().unwrap();
    assert!(eh
        .iter()
        .any(|e| matches!(e, GameEvent::PlayerJoined { name, .. } if name == "Guest")));
    pump(&mut host, &mut guest, |_, _| true);
    let start = Instant::now();
    while guest.lobby.slots.len() < 2 && start.elapsed() < Duration::from_secs(3) {
        host.poll();
        guest.poll();
    }
    assert_eq!(guest.lobby.slots.len(), 2);
    assert_eq!(guest.lobby.slot(HOST_ID).unwrap().name, "Host");
    assert_eq!(guest.lobby.game_type, "DEATHMATCH");
    assert_eq!(guest.lobby.weight_class, "Heavyweight");

    // Robot choice (with a design stream bigger than one frame) and ready.
    let design: Vec<u8> = (0..3000u32).map(|i| (i * 7) as u8).collect();
    guest.choose_robot("Guests", "Razor", "razor.bot", design.clone());
    host.choose_robot("Hosts", "Tank", "tank.bot", vec![1, 2, 3]);
    guest.set_ready(true);
    host.set_ready(true);
    let start = Instant::now();
    while !(host.lobby.all_ready() && guest.lobby.all_ready())
        && start.elapsed() < Duration::from_secs(5)
    {
        host.poll();
        guest.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(host.lobby.all_ready(), "{:?}", host.lobby.slots);
    assert!(guest.lobby.all_ready(), "{:?}", guest.lobby.slots);
    assert_eq!(
        host.lobby.slot(me).unwrap().design.as_deref(),
        Some(&design[..])
    );
    assert_eq!(guest.lobby.slot(HOST_ID).unwrap().robot, "Tank");
    assert_eq!(
        host.lobby.host_data(),
        ra2net::dpn::RaHostData {
            slots_23: 0,
            slots_01: 2
        }
    );

    // Chat both ways.
    assert_eq!(
        guest.chat("good luck").as_deref(),
        Some("[Guest] good luck")
    );
    assert_eq!(host.chat("you too").as_deref(), Some("[Host] you too"));
    assert_eq!(guest.chat(""), None, "an empty entry sends nothing");
    let (eh, eg) = pump(&mut host, &mut guest, |h, g| {
        h.iter()
            .any(|e| matches!(e, GameEvent::Lobby(LobbyChange::Chat { .. })))
            && g.iter()
                .any(|e| matches!(e, GameEvent::Lobby(LobbyChange::Chat { .. })))
    });
    assert!(eh.contains(&GameEvent::Lobby(LobbyChange::Chat {
        from: me,
        text: "[Guest] good luck".into()
    })));
    assert!(eg.contains(&GameEvent::Lobby(LobbyChange::Chat {
        from: HOST_ID,
        text: "[Host] you too".into()
    })));
}

#[test]
fn version_mismatch_in_progress_and_full_are_refused() {
    let mut host = Session::host(host_config(2)).unwrap();
    let addr = host.local_addr().unwrap();
    let wait = |host: &mut Session, guest: &mut Session| -> Vec<Event> {
        let start = Instant::now();
        loop {
            host.poll();
            let ev = guest.poll();
            if !ev.is_empty() || start.elapsed() > Duration::from_secs(5) {
                return ev;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    };

    let mut old = JoinConfig {
        connect_data: b"00.90\0".to_vec(),
        ..join_config(addr, "Old")
    };
    old.link = fast_link();
    let mut guest = Session::join(old).unwrap();
    assert_eq!(
        wait(&mut host, &mut guest),
        [Event::Refused(Refusal::IncompatibleVersion)]
    );
    assert_eq!(Refusal::IncompatibleVersion.reply_word(), Some(900));

    let mut first = Session::join(join_config(addr, "First")).unwrap();
    assert!(matches!(
        wait(&mut host, &mut first)[0],
        Event::Connected { .. }
    ));
    let mut third = Session::join(join_config(addr, "Third")).unwrap();
    assert_eq!(
        wait(&mut host, &mut third),
        [Event::Refused(Refusal::SessionFull)]
    );

    host.set_accepting(false);
    let mut late = Session::join(join_config(addr, "Late")).unwrap();
    assert_eq!(
        wait(&mut host, &mut late),
        [Event::Refused(Refusal::InProgress)]
    );
    assert_eq!(
        Refusal::InProgress.text(),
        "Connection refused.  Game already in progress."
    );
}

#[test]
fn nobody_home_fails_the_connect() {
    // Bind a socket and drop it: its port is closed.
    let port = std::net::UdpSocket::bind(any())
        .unwrap()
        .local_addr()
        .unwrap();
    let mut guest = Session::join(JoinConfig {
        connect_timeout: Duration::from_millis(500),
        ..join_config(port, "Lost")
    })
    .unwrap();
    let start = Instant::now();
    let mut events = Vec::new();
    while events.is_empty() && start.elapsed() < Duration::from_secs(5) {
        events = guest.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(events, [Event::ConnectFailed]);
}

/// A toy world the host owns: one body per player moving by its inputs.
struct World {
    bodies: Vec<BodySync>,
}

impl World {
    fn new() -> World {
        World {
            bodies: (0..2)
                .map(|i| BodySync {
                    id: i,
                    awake: true,
                    rotation: [1.0, 0.0, 0.0, 0.0],
                    ..BodySync::default()
                })
                .collect(),
        }
    }

    fn step(&mut self, throttle: &[f32; 2], dt: f32) {
        for (b, t) in self.bodies.iter_mut().zip(throttle) {
            b.linear[0] = *t;
            b.position[0] += t * dt;
        }
    }

    fn snapshot(&self) -> SyncState {
        SyncState {
            partial: true,
            bodies: self.bodies.clone(),
            ..SyncState::default()
        }
    }
}

#[test]
fn a_short_synced_match_then_disconnect() {
    let mut host = NetGame::host(host_config(4), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Guest")).unwrap();
    pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Joined { .. }))
    });
    let me = guest.local().unwrap();

    // Start: prepare, load, begin, count down.
    host.start_match("Arenas/box.py");
    let (_, eg) = pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Prepare { .. }))
    });
    assert!(eg.contains(&GameEvent::Prepare {
        arena: "Arenas/box.py".into()
    }));
    let schema = SyncSchema::default();
    host.set_schema(schema.clone());
    guest.set_schema(schema);
    guest.report_loaded();
    let (eh, _) = pump(&mut host, &mut guest, |h, _| {
        h.iter().any(|e| matches!(e, GameEvent::Loaded { .. }))
    });
    assert!(eh.contains(&GameEvent::Loaded { player: me }));
    host.begin_match();
    host.count_down();
    pump(&mut host, &mut guest, |_, g| {
        g.contains(&GameEvent::Begin) && g.contains(&GameEvent::CountDown)
    });

    // Late joiners are turned away now.
    let mut late = Session::join(join_config(addr, "Late")).unwrap();

    // Two simulated seconds at 60 Hz: the guest holds "forward" (value 100)
    // for its robot; the host applies it and syncs at the LAN rate.
    let mut world = World::new();
    let mut throttle = [0.0f32; 2];
    let mut last_seen: Option<SyncState> = None;
    let mut snapshots = 0;
    let mut late_events = Vec::new();
    let dt = 1.0 / 60.0;
    guest.send_input(
        1,
        BotInput {
            key: [0, 0, 0],
            value: 100,
        },
    );
    for _ in 0..120 {
        for e in host.poll() {
            if let GameEvent::RemoteInput { robot, input, from } = e {
                assert_eq!(from, me);
                throttle[robot as usize] = input.value as f32 / 100.0;
            }
        }
        world.step(&throttle, dt);
        host.tick_sync(dt, || world.snapshot());
        for e in guest.poll() {
            if let GameEvent::Snapshot(s) = e {
                snapshots += 1;
                last_seen = Some(s);
            }
        }
        late_events.extend(late.poll());
        std::thread::sleep(Duration::from_millis(2));
    }
    // Let the last snapshot land.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(300) {
        host.poll();
        for e in guest.poll() {
            if let GameEvent::Snapshot(s) = e {
                snapshots += 1;
                last_seen = Some(s);
            }
        }
        late_events.extend(late.poll());
    }
    assert!(
        snapshots >= 30,
        "LAN pacing is ~53 Hz; got {snapshots} in 2 s"
    );
    let seen = last_seen.expect("snapshots arrived");
    // The guest's robot moved by its own input, as the host simulated it; the
    // host's own robot stayed put.
    assert!(seen.bodies[1].position[0] > 1.5, "{:?}", seen.bodies[1]);
    assert_eq!(seen.bodies[0].position[0], 0.0);
    assert!((seen.bodies[1].position[0] - world.bodies[1].position[0]).abs() < 0.1);
    assert!(
        late_events.contains(&Event::Refused(Refusal::InProgress)),
        "{late_events:?}"
    );

    // The win, then the guest leaves; the host sees it go.
    host.broadcast(&Message::MatchWin {
        first: 1,
        second: u32::MAX,
    });
    let (_, eg) = pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| {
            matches!(
                e,
                GameEvent::Message {
                    message: Message::MatchWin { .. },
                    ..
                }
            )
        })
    });
    assert!(eg.iter().any(|e| matches!(e,
        GameEvent::Message { from, message: Message::MatchWin { first: 1, .. } } if *from == HOST_ID)));
    guest.close();
    let (eh, _) = pump(&mut host, &mut guest, |h, _| {
        h.iter().any(|e| matches!(e, GameEvent::PlayerLeft { .. }))
    });
    assert!(eh.contains(&GameEvent::PlayerLeft {
        id: me,
        reason: LeaveReason::Left
    }));
    assert_eq!(host.lobby.slots.len(), 1);
}

#[test]
fn the_host_ending_the_session_and_a_silent_peer() {
    // Host closes: the guest hears the session end.
    let mut host = NetGame::host(host_config(4), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Guest")).unwrap();
    pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Joined { .. }))
    });
    host.close();
    pump(&mut host, &mut guest, |_, g| {
        g.contains(&GameEvent::Terminated)
    });

    // A guest that vanishes without a word times out on the host.
    let mut host = NetGame::host(host_config(4), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Ghost")).unwrap();
    pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Joined { .. }))
    });
    let id = guest.local().unwrap();
    drop(guest);
    let start = Instant::now();
    let mut events = Vec::new();
    while start.elapsed() < Duration::from_secs(5) {
        events.extend(host.poll());
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::PlayerLeft { .. }))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(events.contains(&GameEvent::PlayerLeft {
        id,
        reason: LeaveReason::TimedOut
    }));
}

#[test]
fn a_joiner_can_address_another_joiner_through_the_host() {
    let mut host = Session::host(host_config(4)).unwrap();
    let addr = host.local_addr().unwrap();
    let mut a = Session::join(join_config(addr, "A")).unwrap();
    let mut b = Session::join(join_config(addr, "B")).unwrap();
    let start = Instant::now();
    let (mut ida, mut idb) = (None, None);
    while (ida.is_none() || idb.is_none()) && start.elapsed() < Duration::from_secs(5) {
        host.poll();
        for e in a.poll() {
            if let Event::Connected { local, .. } = e {
                ida = Some(local);
            }
        }
        for e in b.poll() {
            if let Event::Connected { local, .. } = e {
                idb = Some(local);
            }
        }
    }
    let (ida, idb) = (ida.unwrap(), idb.unwrap());
    a.send_with(
        Target::Player(idb),
        &Message::CriticalHit { robot: 3 },
        true,
    );
    let start = Instant::now();
    let mut got = Vec::new();
    while got.is_empty() && start.elapsed() < Duration::from_secs(5) {
        host.poll();
        a.poll();
        got.extend(
            b.poll()
                .into_iter()
                .filter(|e| matches!(e, Event::Message { .. })),
        );
    }
    assert_eq!(
        got,
        [Event::Message {
            from: ida,
            message: Message::CriticalHit { robot: 3 }
        }]
    );
}

/// Spectators (docs/38 §10): peers beyond the four player panels sit in the
/// `spectator list`, cannot take a robot or a ready flag, are told everything a
/// player is told, and take a panel when one frees up.
#[test]
fn a_fifth_peer_is_a_spectator_who_takes_a_freed_panel() {
    use ra2net::lobby::{kind, SESSION_MAX};
    let mut host = NetGame::host(host_config(SESSION_MAX), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guests: Vec<NetGame> = Vec::new();
    let start = Instant::now();
    // One at a time, so the join order is the panel order.
    for name in ["A", "B", "C", "Watcher"] {
        guests.push(NetGame::join(join_config(addr, name)).unwrap());
        let want = guests.len() + 1;
        while host.lobby.slots.len() < want && start.elapsed() < Duration::from_secs(8) {
            host.poll();
            for g in guests.iter_mut() {
                g.poll();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let settled = |host: &NetGame, guests: &[NetGame]| {
        host.lobby.slots.len() == 5 && guests.iter().all(|g| g.lobby.slots.len() == 5)
    };
    while !settled(&host, &guests) && start.elapsed() < Duration::from_secs(8) {
        host.poll();
        for g in guests.iter_mut() {
            g.poll();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(settled(&host, &guests), "{:?}", host.lobby.slots);
    let watcher = guests[3].local().unwrap();
    for lobby in std::iter::once(&host.lobby).chain(guests.iter().map(|g| &g.lobby)) {
        assert_eq!(lobby.players().count(), 4);
        let spectators: Vec<_> = lobby.spectators().map(|s| s.name.as_str()).collect();
        assert_eq!(spectators, ["Watcher"]);
        assert_eq!(lobby.robot_index(watcher), None);
        assert_eq!(lobby.robot_index(guests[2].local().unwrap()), Some(3));
    }

    // A spectator cannot take a robot or be ready.
    guests[3].choose_robot("T", "Razor", "razor.bot", vec![1]);
    guests[3].set_ready(true);
    for _ in 0..50 {
        host.poll();
        for g in guests.iter_mut() {
            g.poll();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let slot = host.lobby.slot(watcher).unwrap();
    assert!(slot.is_spectator() && !slot.ready && !slot.has_robot());
    assert_eq!(slot.kind, kind::SPECTATOR);

    // A player leaves: the spectator takes the panel on every end.
    guests[0].close();
    let start = Instant::now();
    while !(host.lobby.slot(watcher).is_some_and(|s| !s.is_spectator())
        && guests[3]
            .lobby
            .slot(watcher)
            .is_some_and(|s| !s.is_spectator()))
        && start.elapsed() < Duration::from_secs(8)
    {
        host.poll();
        for g in guests.iter_mut().skip(1) {
            g.poll();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(host.lobby.spectators().count(), 0);
    assert_eq!(host.lobby.players().count(), 4);
    // Panels are positions (`GroupInfo::slot`): the watcher takes the panel A
    // left, the second one.
    assert_eq!(guests[3].lobby.robot_index(watcher), Some(1));
    assert_eq!(host.lobby.slot(watcher).unwrap().index, 1);
}

/// The in-match chat line: `"[name] text"` as a `0x76` text to everyone,
/// nothing for an empty entry (`FUN_00475240`).
#[test]
fn match_chat_reaches_the_host_the_joiner_and_a_spectator() {
    let mut host = NetGame::host(host_config(8), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Guest")).unwrap();
    let mut watcher = NetGame::join(join_config(addr, "Watcher")).unwrap();
    let start = Instant::now();
    while !(guest.lobby.slots.len() == 3 && watcher.lobby.slots.len() == 3)
        && start.elapsed() < Duration::from_secs(5)
    {
        host.poll();
        guest.poll();
        watcher.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    for g in [&mut host, &mut guest, &mut watcher] {
        g.set_schema(SyncSchema::default());
    }
    assert_eq!(guest.chat("gg").as_deref(), Some("[Guest] gg"));
    assert_eq!(host.chat(""), None);
    let mut seen = (false, false);
    let start = Instant::now();
    while seen != (true, true) && start.elapsed() < Duration::from_secs(5) {
        for e in host.poll() {
            if let GameEvent::Chat { text, .. } = e {
                assert_eq!(text, "[Guest] gg");
                seen.0 = true;
            }
        }
        guest.poll();
        for e in watcher.poll() {
            if let GameEvent::Chat { text, .. } = e {
                assert_eq!(text, "[Guest] gg");
                seen.1 = true;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(seen, (true, true));
}

#[test]
fn renaming_a_player_reaches_the_other_end() {
    let mut host = NetGame::host(host_config(4), ConnectionSpeed::Lan).unwrap();
    let addr = host.session.local_addr().unwrap();
    let mut guest = NetGame::join(join_config(addr, "Guest")).unwrap();
    pump(&mut host, &mut guest, |_, g| {
        g.iter().any(|e| matches!(e, GameEvent::Joined { .. }))
    });
    let start = Instant::now();
    while guest.lobby.slots.len() < 2 && start.elapsed() < Duration::from_secs(3) {
        host.poll();
        guest.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let me = guest.local().unwrap();

    // The joiner edits its `screen name` box: the host's lobby entry follows.
    guest.rename("Mister Roboto");
    let (eh, _) = pump(&mut host, &mut guest, |h, _| {
        h.iter()
            .any(|e| matches!(e, GameEvent::PlayerRenamed { .. }))
    });
    assert!(eh.contains(&GameEvent::PlayerRenamed {
        id: me,
        name: "Mister Roboto".into()
    }));
    assert_eq!(host.lobby.slot(me).unwrap().name, "Mister Roboto");
    assert_eq!(guest.local_name(), "Mister Roboto");
    assert!(host
        .session
        .players()
        .iter()
        .any(|p| p.id == me && p.name == "Mister Roboto"));

    // And the host's edit reaches the joiner.
    host.rename("Boss");
    let (_, eg) = pump(&mut host, &mut guest, |_, g| {
        g.iter()
            .any(|e| matches!(e, GameEvent::PlayerRenamed { .. }))
    });
    assert!(eg.contains(&GameEvent::PlayerRenamed {
        id: HOST_ID,
        name: "Boss".into()
    }));
    assert_eq!(guest.lobby.slot(HOST_ID).unwrap().name, "Boss");
}
