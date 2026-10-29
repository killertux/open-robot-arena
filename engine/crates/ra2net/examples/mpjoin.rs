//! `mpjoin`: a headless port joiner for recording the original's host
//! (docs/38 section 15). It joins `127.0.0.1:9791`, takes a panel with a robot
//! on command, answers the match start the way the original's joiner does and
//! writes every `giSyncMessage` the host sends (inflated) and every other
//! message to `--out DIR`.
//!
//! `mpjoin --bot "Teams/Team0/Bot0.bot" --out DIR [--host 127.0.0.1:9791]`
//!
//! Commands on standard input: `claim N` (take panel N), `ready`, `eject N`, `chat TEXT`, `key CODE 1|0`, `quit`.

use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

use ra2net::game::{GameEvent, NetGame};
use ra2net::lobby::kind;
use ra2net::message::Message;
use ra2net::session::{JoinConfig, Target};

fn main() {
    let mut bot_path = None;
    let mut out = None;
    let mut host = "127.0.0.1:9791".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--bot" => bot_path = args.next(),
            "--out" => out = args.next(),
            "--host" => host = args.next().unwrap_or(host),
            _ => {}
        }
    }
    let bot = ra2fmt::bot::parse(&std::fs::read(bot_path.expect("--bot")).expect("the bot"))
        .expect("a bot");
    let out = std::path::PathBuf::from(out.expect("--out DIR"));
    std::fs::create_dir_all(&out).expect("the output directory");
    let mut matches = 0u32;
    let mut raw = std::fs::File::create(out.join("sync0.bin")).expect("sync0.bin");
    let mut log = std::fs::File::create(out.join("messages.txt")).expect("messages.txt");
    let config = JoinConfig::new(
        ([127, 0, 0, 1], 0).into(),
        host.parse().expect("host:port"),
        "Port Joiner",
    );
    let mut game = NetGame::join(config).expect("join");
    game.tap_sync = Some(Vec::new());
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_self = tx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let start = Instant::now();
    let mut say = |s: String| {
        let line = format!("[{:8.3}] {s}", start.elapsed().as_secs_f64());
        eprintln!("{line}");
        let _ = writeln!(log, "{line}");
    };
    let mut panel = 1u32;
    let mut begun_at: Option<Instant> = None;
    let mut next_loaded = Instant::now();
    let mut ready_again: Option<Instant> = None;
    loop {
        for event in game.poll() {
            match &event {
                GameEvent::Snapshot(_) => {}
                GameEvent::Prepare { arena } => {
                    say(format!("Prepare {arena}"));
                    matches += 1;
                    raw = std::fs::File::create(out.join(format!("sync{matches}.bin")))
                        .expect("sync file");
                    let m = Message::AddArenaBot {
                        words: [1, panel, 0],
                        design: bot.write_wire(),
                    };
                    game.session.send_with(Target::All, &m, true);
                    // Bots loaded (the original's joiner: `0x12f {panel, 0}`).
                    game.session.send_with(
                        Target::All,
                        &Message::PlayerReady {
                            player: panel,
                            ready: false,
                        },
                        false,
                    );
                    say("AddArenaBot, ready 0 sent".into());
                }
                GameEvent::Begin => {
                    say("Begin".into());
                    begun_at = Some(Instant::now());
                }
                GameEvent::Message { from, message } => {
                    let text = format!("{message:?}");
                    say(format!("from {from:#x}: {}", &text[..text.len().min(160)]));
                    if matches!(message, Message::EndMatch) {
                        begun_at = None;
                        ready_again = Some(Instant::now() + Duration::from_secs(2));
                    }
                    if let Message::AddArenaBot { words, design } = message {
                        let name = format!("bot_{}_{}_{}.bot", matches, from, words[1]);
                        let _ = std::fs::write(out.join(name), design);
                    }
                }
                other => say(format!("{other:?}")),
            }
        }
        if let Some(tap) = &mut game.tap_sync {
            for m in tap.drain(..) {
                let mut body = Vec::new();
                let _ = std::io::Read::read_to_end(
                    &mut flate2::read::ZlibDecoder::new(&m.packed[..]),
                    &mut body,
                );
                let ms = start.elapsed().as_millis() as u32;
                let _ = raw.write_all(&ms.to_le_bytes());
                let _ = raw.write_all(&(body.len() as u32).to_le_bytes());
                let _ = raw.write_all(&body);
            }
        }
        if begun_at.is_some_and(|t| t.elapsed() > Duration::from_secs(3))
            && Instant::now() >= next_loaded
        {
            next_loaded = Instant::now() + Duration::from_millis(300);
            let me = game.local().unwrap();
            game.session
                .send_with(Target::All, &Message::PeerDataLoaded { player: me }, true);
            game.session.send_with(
                Target::All,
                &Message::PlayerReady {
                    player: panel,
                    ready: true,
                },
                false,
            );
        }
        if ready_again.is_some_and(|t| Instant::now() >= t) {
            ready_again = None;
            let _ = tx_self.send("ready".to_string());
        }
        while let Ok(cmd) = rx.try_recv() {
            let w: Vec<&str> = cmd.split_whitespace().collect();
            match w.as_slice() {
                ["claim", n] => {
                    panel = n.parse().unwrap_or(1);
                    game.claim_panel(
                        Some(panel),
                        "The Rookies",
                        &bot.name,
                        "Teams\\orange_skull.bmp",
                        bot.snapshot_wire().expect("a preview"),
                    );
                    say(format!("claimed panel {panel}"));
                }
                ["ready"] => {
                    game.set_ready(true);
                    if let Some(me) = game.local() {
                        if let Some(slot) = game.lobby.slot(me).cloned() {
                            let mut s = slot;
                            s.ready = true;
                            s.design = None;
                            let _ = kind::HUMAN;
                            game.session.send_with(
                                Target::All,
                                &Message::GroupInfo(s.to_group_info()),
                                false,
                            );
                        }
                    }
                    say("ready".into());
                }
                ["key", code, value] => {
                    // A key event of the original's remote input: the binding's
                    // first three words and 1 (down) or 0 (up).
                    let m = Message::RemoteInput {
                        robot: panel,
                        input: ra2net::message::BotInput {
                            key: [0, 0, code.parse().unwrap_or(0)],
                            value: value.parse().unwrap_or(0),
                        },
                    };
                    game.session.send_with(Target::All, &m, false);
                }
                ["eject", n] => {
                    let ok = game.eject_panel(n.parse().unwrap_or(panel));
                    say(format!("eject {n}: {ok}"));
                }
                _ if cmd.starts_with("chat ") => {
                    game.chat(cmd["chat ".len()..].trim());
                    say(format!("chat: {}", cmd["chat ".len()..].trim()));
                }
                ["quit"] => return,
                _ => say(format!("? {cmd}")),
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
