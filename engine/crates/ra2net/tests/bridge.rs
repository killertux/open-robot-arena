//! The bridge (the session side of `tools/dpnet`, docs/38 §14) against
//! ordinary port sessions on 127.0.0.1: the original hosting and the port
//! joining, and the reverse. The DLL's part is played by the test: it answers
//! the bridge's questions the way the game's DirectPlay callback does.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ra2net::bridge::{self, Bridge, Note, Request};
use ra2net::dpn;
use ra2net::link::LinkConfig;
use ra2net::message::Message;
use ra2net::session::{Browser, Event, HostConfig, JoinConfig, Refusal, Session, Target};

fn fast_link() -> LinkConfig {
    LinkConfig {
        rto: Duration::from_millis(50),
        keepalive: Duration::from_millis(100),
        timeout: Duration::from_millis(1500),
        ..LinkConfig::default()
    }
}

fn loopback_bridge() -> Bridge {
    let mut b = Bridge::new();
    b.bind_ip = "127.0.0.1".parse().unwrap();
    b.link = fast_link();
    b
}

fn any() -> SocketAddr {
    "127.0.0.1:0".parse().unwrap()
}

/// Poll the bridge and the sessions until `done` (or fail after 5 s).
fn until<F>(
    bridge: &mut Bridge,
    sessions: &mut [&mut Session],
    mut done: F,
) -> (Vec<Note>, Vec<Vec<Event>>)
where
    F: FnMut(&[Note], &[Vec<Event>]) -> bool,
{
    let mut notes = Vec::new();
    let mut events = vec![Vec::new(); sessions.len()];
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        notes.extend(bridge.poll());
        for (s, e) in sessions.iter_mut().zip(events.iter_mut()) {
            e.extend(s.poll());
        }
        if done(&notes, &events) {
            return (notes, events);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("timed out; bridge {notes:?}, sessions {events:?}");
}

fn host_request() -> Request {
    Request::Host {
        port: 0,
        max_players: 8,
        session: "Default Server".into(),
        password: String::new(),
        player: "Original".into(),
    }
}

#[test]
fn the_original_hosts_and_a_port_player_joins_through_its_verdict() {
    let mut bridge = loopback_bridge();
    let notes = bridge.request(host_request());
    // The local player comes first (CREATE_PLAYER during Host), then the result.
    assert!(matches!(&notes[0], Note::Player { flags, name, .. }
        if *flags == bridge::PLAYER_LOCAL | bridge::PLAYER_HOST && name == "Original"));
    let Note::Hosted { hr: 0, local } = notes[1] else {
        panic!("{notes:?}")
    };
    let addr = bridge.session.as_ref().unwrap().local_addr().unwrap();

    // The server list: the query goes to the game, its 8 bytes come back.
    let mut browser = Browser::new(any()).unwrap();
    browser.query(addr).unwrap();
    let (notes, _) = until(&mut bridge, &mut [], |n, _| {
        n.iter().any(|n| matches!(n, Note::EnumAsk { .. }))
    });
    let Some(Note::EnumAsk { token, .. }) =
        notes.iter().find(|n| matches!(n, Note::EnumAsk { .. }))
    else {
        unreachable!()
    };
    let reply = [1u8, 0, 0, 0, 2, 0, 0, 0];
    bridge.request(Request::EnumAnswer {
        token: *token,
        answer: true,
        data: reply.to_vec(),
    });
    let start = Instant::now();
    while browser.hosts.is_empty() && start.elapsed() < Duration::from_secs(3) {
        browser.poll();
        std::thread::sleep(Duration::from_millis(2));
    }
    let row = &browser.hosts[0];
    assert_eq!(
        (
            row.session_name.as_str(),
            row.current_players,
            row.max_players
        ),
        ("Default Server", 1, 8)
    );
    assert_eq!(row.reply, reply);

    // A port joiner: the game's INDICATE_CONNECT sees the connect data and
    // accepts.
    let mut joiner = Session::join(JoinConfig {
        link: fast_link(),
        ..JoinConfig::new(any(), addr, "Port")
    })
    .unwrap();
    let (notes, _) = until(&mut bridge, &mut [&mut joiner], |n, _| {
        n.iter().any(|n| matches!(n, Note::AdmitAsk { .. }))
    });
    let Some(Note::AdmitAsk {
        token,
        name,
        connect_data,
        ..
    }) = notes.iter().find(|n| matches!(n, Note::AdmitAsk { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (name.as_str(), connect_data.as_slice()),
        ("Port", dpn::CONNECT_DATA)
    );
    bridge.request(Request::Admit {
        token: *token,
        accept: true,
        reply: Vec::new(),
    });
    let (notes, events) = until(&mut bridge, &mut [&mut joiner], |n, e| {
        n.iter()
            .any(|n| matches!(n, Note::Player { name, .. } if name == "Port"))
            && e[0].iter().any(|e| matches!(e, Event::Connected { .. }))
    });
    let joiner_id = notes
        .iter()
        .find_map(|n| match n {
            Note::Player {
                id, name, flags: 0, ..
            } if name == "Port" => Some(*id),
            _ => None,
        })
        .unwrap();
    assert!(events[0]
        .iter()
        .any(|e| matches!(e, Event::Connected { host, players, .. }
        if *host == local && players.iter().any(|p| p.name == "Original"))));

    // Game payloads both ways, untouched.
    let slot = Message::Text("hello from the port".into());
    joiner.send(Target::All, &slot);
    let (notes, _) = until(&mut bridge, &mut [&mut joiner], |n, _| {
        n.iter().any(|n| matches!(n, Note::Receive { .. }))
    });
    assert!(notes.contains(&Note::Receive {
        from: joiner_id,
        data: slot.encode()
    }));
    // Chat goes with flags 0: DirectPlay loops it back to the sender.
    let chat = Message::Text("[Original] hi".into()).encode();
    let notes = bridge.request(Request::Send {
        handle: 5,
        to: 0,
        flags: 0,
        data: chat.clone(),
    });
    assert_eq!(
        notes,
        [
            Note::Receive {
                from: local,
                data: chat.clone()
            },
            Note::Sent { handle: 5, hr: 0 }
        ]
    );
    let (_, events) = until(&mut bridge, &mut [&mut joiner], |_, e| {
        e[0].iter().any(|e| matches!(e, Event::Message { .. }))
    });
    assert!(events[0].contains(&Event::Message {
        from: local,
        message: Message::Text("[Original] hi".into())
    }));

    // A late joiner: the game answers 901 and the port shows "in progress".
    let mut late = Session::join(JoinConfig {
        link: fast_link(),
        ..JoinConfig::new(any(), addr, "Late")
    })
    .unwrap();
    let (notes, _) = until(&mut bridge, &mut [&mut late], |n, _| {
        n.iter().any(|n| matches!(n, Note::AdmitAsk { .. }))
    });
    let Some(Note::AdmitAsk { token, .. }) =
        notes.iter().find(|n| matches!(n, Note::AdmitAsk { .. }))
    else {
        unreachable!()
    };
    bridge.request(Request::Admit {
        token: *token,
        accept: false,
        reply: dpn::REFUSE_IN_PROGRESS.to_le_bytes().to_vec(),
    });
    let (_, events) = until(&mut bridge, &mut [&mut late], |_, e| !e[0].is_empty());
    assert_eq!(events[0], [Event::Refused(Refusal::InProgress)]);

    // The original leaves: the joiner's session ends.
    bridge.request(Request::Close);
    let start = Instant::now();
    let mut ended = false;
    while !ended && start.elapsed() < Duration::from_secs(3) {
        ended = joiner.poll().contains(&Event::SessionTerminated);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(ended);
}

#[test]
fn the_port_hosts_and_the_original_finds_and_joins_it() {
    let mut host = Session::host(HostConfig {
        bind: any(),
        session_name: "Port Arena".into(),
        player_name: "PortHost".into(),
        max_players: 4,
        password: None,
        link: fast_link(),
    })
    .unwrap();
    let addr = host.local_addr().unwrap();
    let mut bridge = loopback_bridge();

    // Refresh with IP Address: one row, keyed by the instance GUID.
    bridge.request(Request::Enum {
        host: "127.0.0.1".into(),
        port: addr.port(),
    });
    let (notes, _) = until(&mut bridge, &mut [&mut host], |n, _| {
        n.iter().any(|n| matches!(n, Note::EnumFound { .. }))
    });
    let Some(Note::EnumFound {
        ip,
        port,
        session,
        current_players,
        app,
        reply,
        ..
    }) = notes.iter().find(|n| matches!(n, Note::EnumFound { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (ip.as_str(), *port, session.as_str(), *current_players),
        ("127.0.0.1", addr.port(), "Port Arena", 1)
    );
    assert_eq!(*app, dpn::APP_GUID);
    assert_eq!(reply.len(), 8);
    bridge.request(Request::EnumStop);

    // Connect: every player is created (local and host flagged), then the
    // connect completes.
    let notes = bridge.request(Request::Connect {
        host: "127.0.0.1".into(),
        port: addr.port(),
        local_port: 0,
        player: "Original".into(),
        password: String::new(),
        connect_data: dpn::CONNECT_DATA.to_vec(),
    });
    assert!(notes.is_empty());
    let (notes, events) = until(&mut bridge, &mut [&mut host], |n, _| {
        n.iter().any(|n| matches!(n, Note::Connected { .. }))
    });
    let at = notes
        .iter()
        .position(|n| matches!(n, Note::Connected { .. }))
        .unwrap();
    let Note::Connected {
        hr: 0,
        local,
        host: host_id,
        ..
    } = notes[at]
    else {
        panic!("{notes:?}")
    };
    let players: Vec<_> = notes[..at]
        .iter()
        .filter_map(|n| match n {
            Note::Player {
                id, flags, name, ..
            } => Some((*id, *flags, name.clone())),
            _ => None,
        })
        .collect();
    assert!(players.contains(&(local, bridge::PLAYER_LOCAL, "Original".into())));
    assert!(players.contains(&(host_id, bridge::PLAYER_HOST, "PortHost".into())));
    let _ = events;

    // The original's chat (flags 0) reaches the host decoded.
    let chat = Message::Text("[Original] hello".into());
    bridge.request(Request::Send {
        handle: 1,
        to: host_id,
        flags: 0x28,
        data: chat.encode(),
    });
    let (_, events) = until(&mut bridge, &mut [&mut host], |_, e| {
        e[0].iter().any(|e| matches!(e, Event::Message { .. }))
    });
    assert!(events[0].contains(&Event::Message {
        from: local,
        message: chat
    }));

    // The host's message arrives raw.
    let ready = Message::PlayerReady {
        player: local,
        ready: true,
    };
    host.send(Target::All, &ready);
    let (notes, _) = until(&mut bridge, &mut [&mut host], |n, _| {
        n.iter().any(|n| matches!(n, Note::Receive { .. }))
    });
    assert!(notes.contains(&Note::Receive {
        from: host_id,
        data: ready.encode()
    }));
}

#[test]
fn a_refused_original_hears_the_reply_word() {
    let mut host = Session::host(HostConfig {
        bind: any(),
        session_name: "Busy".into(),
        player_name: "PortHost".into(),
        max_players: 4,
        password: None,
        link: fast_link(),
    })
    .unwrap();
    host.set_accepting(false);
    let addr = host.local_addr().unwrap();
    let mut bridge = loopback_bridge();
    bridge.request(Request::Connect {
        host: "127.0.0.1".into(),
        port: addr.port(),
        local_port: 0,
        player: "Original".into(),
        password: String::new(),
        connect_data: dpn::CONNECT_DATA.to_vec(),
    });
    let (notes, _) = until(&mut bridge, &mut [&mut host], |n, _| {
        n.iter().any(|n| matches!(n, Note::Connected { .. }))
    });
    // CONNECT_COMPLETE with DPNERR_HOSTREJECTEDCONNECTION and 901: the
    // original's "Connection refused.  Game already in progress."
    assert!(notes.contains(&Note::Connected {
        hr: bridge::HR_HOST_REJECTED,
        local: 0,
        host: 0,
        reply: dpn::REFUSE_IN_PROGRESS.to_le_bytes().to_vec(),
    }));
}

#[test]
fn the_pipe_carries_requests_and_notes_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let server = std::thread::spawn(move || {
        bridge::serve(
            listener,
            loopback_bridge,
            |_| {},
            move || stop2.load(Ordering::Relaxed),
        )
    });
    let mut dll = TcpStream::connect(at).unwrap();
    dll.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    dll.write_all(&host_request().encode()).unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut notes = Vec::new();
    while notes.len() < 2 {
        let n = dll.read(&mut chunk).unwrap();
        assert!(n > 0);
        buf.extend(&chunk[..n]);
        notes.extend(
            bridge::take_frames(&mut buf)
                .iter()
                .map(|b| Note::decode(b).unwrap()),
        );
    }
    assert!(matches!(notes[0], Note::Player { .. }));
    assert!(matches!(notes[1], Note::Hosted { hr: 0, .. }));
    dll.write_all(&Request::Close.encode()).unwrap();
    drop(dll);
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap().unwrap();
}
