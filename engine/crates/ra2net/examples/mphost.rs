//! `mphost`: a headless port host for trying an original joiner against
//! (docs/38 section 15). It hosts on loopback with one human panel holding a
//! robot (`--bot FILE`), logs every event, and runs the start of a match on
//! commands read from standard input: `prepare`, `begin`, `bots`, `count`,
//! `end`, `quit`.
//!
//! `mphost --bot "Teams/Team0/Bot1.bot" [--port 9791]`

use std::io::BufRead;
use std::time::{Duration, Instant};

use ra2net::dpn;
use ra2net::game::{GameEvent, NetGame};
use ra2net::message::Message;
use ra2net::session::{HostConfig, Target};
use ra2net::sync::ConnectionSpeed;

fn main() {
    let mut bot_path = None;
    let mut host_robot = true;
    let mut port = dpn::DEFAULT_PORT;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bot" => bot_path = args.next(),
            "--no-host-robot" => host_robot = false,
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(port),
            _ => {}
        }
    }
    let bot_path = bot_path.expect("--bot FILE");
    let bot = ra2fmt::bot::parse(&std::fs::read(&bot_path).expect("the bot")).expect("a bot");
    let session_name = dpn::SessionName {
        server: "mphost".into(),
        game_type: "DEATHMATCH".into(),
        arena: "Bridge of Doom".into(),
        weight_class: "ALLWEIGHT".into(),
        version: dpn::VERSION.into(),
        speed: ConnectionSpeed::Lan.session_tag().into(),
    };
    let config = HostConfig {
        bind: ([127, 0, 0, 1], port).into(),
        session_name: session_name.encode(),
        player_name: std::env::var("MPHOST_NAME").unwrap_or("Port Host".into()),
        max_players: 8,
        password: None,
        link: Default::default(),
    };
    let mut game = NetGame::host(config, ConnectionSpeed::Lan).expect("host");
    game.set_settings(
        "DEATHMATCH",
        "ALLWEIGHT",
        "Arenas/bridge/bridge_preview.bmp:Bridge of Doom",
    );
    if host_robot {
        game.choose_robot(
            "The Rookies",
            &bot.name,
            "Teams\\orange_skull.bmp",
            bot.snapshot_wire().expect("a preview"),
        );
    }
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let start = Instant::now();
    let mut timesync: Option<(Instant, f32)> = None;
    let mut next_sync = Instant::now();
    let log = |s: String| eprintln!("[{:8.3}] {s}", start.elapsed().as_secs_f64());
    log(format!("hosting on 127.0.0.1:{port} with {}", bot.name));
    loop {
        for event in game.poll() {
            match &event {
                GameEvent::Message { from, message } => {
                    let text = format!("{message:?}");
                    log(format!("from {from:#x}: {}", &text[..text.len().min(200)]));
                }
                GameEvent::Snapshot(_) => {}
                other => log(format!("{other:?}")),
            }
        }
        while let Ok(cmd) = rx.try_recv() {
            let words: Vec<&str> = cmd.split_whitespace().collect();
            match words.as_slice() {
                ["prepare"] => game.start_match("Bridge of Doom"),
                ["begin"] => game
                    .session
                    .send_with(Target::All, &Message::BeginMatch, true),
                ["bots"] => {
                    let m = Message::AddArenaBot {
                        words: [1, 0, 0],
                        design: bot.write_wire(),
                    };
                    game.session.send_with(Target::All, &m, true);
                    log("AddArenaBot sent".into());
                }
                ["loaded"] => {
                    let me = game.local().unwrap();
                    game.session.send_with(
                        Target::All,
                        &Message::PeerDataLoaded { player: me },
                        true,
                    );
                }
                ["count"] => game
                    .session
                    .send_with(Target::All, &Message::StartCountDown, true),
                ["time", secs] => {
                    timesync = Some((Instant::now(), secs.parse().unwrap_or(180.0)));
                }
                ["end"] => game.broadcast_guaranteed(&Message::EndMatch),
                ["quit"] => return,
                _ => log(format!("? {cmd}")),
            }
        }
        if let Some((t0, secs)) = timesync {
            if Instant::now() >= next_sync {
                next_sync = Instant::now() + Duration::from_millis(1000);
                let left = secs - t0.elapsed().as_secs_f32();
                game.session.send(
                    Target::All,
                    &Message::TimeSync {
                        word: left.to_bits(),
                        times: [0; 4],
                    },
                );
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
