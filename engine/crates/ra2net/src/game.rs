//! Host and joiner drivers: a [`Session`] plus the [`Lobby`] and the match
//! sync, with the message flow of `RA2NetInterface` (dispatcher
//! `FUN_004ce940`).
//!
//! The flow, as the original runs it:
//!
//! 1. *Lobby.* A joiner that connects becomes a slot; the host describes every
//!    slot with `giGroupInfoMessage` (`0x71`) and its settings with
//!    `giChangeGameTypeMessage` / `giChangeWeightClassMessage` /
//!    `giArenaPreviewMessage`. A joiner picks a robot (its own slot's group
//!    info, with the design stream) and toggles ready (`giPlayerReadyMessage`,
//!    `0x12f`). Chat is `giNetwork` text (`0x65`) to everyone.
//! 2. *Start.* The host stops taking players (later joiners hear 901), sends
//!    `giPrepareMatchMessage` (`0x12e`), each peer loads and reports
//!    `giPlayerLoadingMessage` / `giPeerDataLoadedMessage`, the host sends
//!    `giBeginMatchMessage` (`0x130`) and `giStartCountDownMessage` (`0x13c`).
//! 3. *Match.* Joiners send `giRemoteInputMessage` (`0x73`); the host paces
//!    `giSyncMessage` snapshots (`0x74`) by the connection speed and sends the
//!    scoring messages; `giMatchWinMessage` (`0x137`) and `giEndMatchMessage`
//!    (`0x131`) close it.

use std::io;
use std::time::Duration;

use crate::lobby::{kind, Lobby, LobbyChange};
use crate::message::{BotInput, Message};
use crate::session::{
    Event, HostConfig, JoinConfig, LeaveReason, PlayerId, Refusal, Session, Target, HOST_ID,
};
use crate::sync::{ConnectionSpeed, Pacer, SyncMessage, SyncSchema, SyncState};

#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    /// Joiner: accepted; the lobby fills as group infos arrive.
    Joined {
        local: PlayerId,
    },
    Refused(Refusal),
    ConnectFailed,
    /// The host ended the session, or the link to it died.
    Terminated,
    PlayerJoined {
        id: PlayerId,
        name: String,
    },
    PlayerLeft {
        id: PlayerId,
        reason: LeaveReason,
    },
    Lobby(LobbyChange),
    /// `giPrepareMatchMessage`.
    Prepare {
        arena: String,
    },
    /// A peer reported loading / loaded.
    Loading {
        player: PlayerId,
    },
    Loaded {
        player: PlayerId,
    },
    /// `giBeginMatchMessage`.
    Begin,
    /// `giStartCountDownMessage`.
    CountDown,
    /// Host: a joiner's key event for its robot.
    RemoteInput {
        from: PlayerId,
        robot: u32,
        input: BotInput,
    },
    /// Joiner: a snapshot newer than the last one applied.
    Snapshot(SyncState),
    /// An in-match chat line (`0x76`, "second text class"; the sender has
    /// already put its `[name] ` in front, `FUN_00475240`).
    Chat {
        from: PlayerId,
        text: String,
    },
    /// Any other game message.
    Message {
        from: PlayerId,
        message: Message,
    },
}

pub struct NetGame {
    pub session: Session,
    pub lobby: Lobby,
    host: PlayerId,
    schema: Option<SyncSchema>,
    pacer: Pacer,
    clock: u32,
    last_clock: Option<u32>,
}

impl NetGame {
    pub fn host(config: HostConfig, speed: ConnectionSpeed) -> io::Result<NetGame> {
        let name = config.player_name.clone();
        let session = Session::host(config)?;
        let mut lobby = Lobby::default();
        lobby.join(HOST_ID, &name);
        Ok(NetGame {
            session,
            lobby,
            host: HOST_ID,
            schema: None,
            pacer: Pacer::new(speed),
            clock: 0,
            last_clock: None,
        })
    }

    pub fn join(config: JoinConfig) -> io::Result<NetGame> {
        Ok(NetGame {
            session: Session::join(config)?,
            lobby: Lobby::default(),
            host: HOST_ID,
            schema: None,
            pacer: Pacer::new(ConnectionSpeed::Lan),
            clock: 0,
            last_clock: None,
        })
    }

    pub fn is_host(&self) -> bool {
        self.session.is_host()
    }

    pub fn local(&self) -> Option<PlayerId> {
        self.session.local_id()
    }

    fn broadcast_lobby(&mut self) {
        for m in self.lobby.group_infos() {
            self.session.send_with(Target::All, &m, true);
        }
        self.session.set_host_data(self.lobby.host_data());
    }

    /// Pump the session.
    pub fn poll(&mut self) -> Vec<GameEvent> {
        let mut out = Vec::new();
        for event in self.session.poll() {
            match event {
                Event::Connected { local, host, .. } => {
                    self.host = host;
                    out.push(GameEvent::Joined { local });
                }
                Event::Refused(r) => out.push(GameEvent::Refused(r)),
                Event::ConnectFailed => out.push(GameEvent::ConnectFailed),
                Event::SessionTerminated => out.push(GameEvent::Terminated),
                Event::PlayerJoined(p) => {
                    if self.is_host() {
                        self.lobby.join(p.id, &p.name);
                        // The newcomer needs the settings and every slot;
                        // the others need the new slot.
                        for m in self.lobby.settings() {
                            self.session.send_with(Target::Player(p.id), &m, true);
                        }
                        self.broadcast_lobby();
                    }
                    out.push(GameEvent::PlayerJoined {
                        id: p.id,
                        name: p.name,
                    });
                }
                Event::PlayerLeft { id, reason } => {
                    self.lobby.leave(id);
                    if self.is_host() {
                        // A freed player panel goes to the first spectator.
                        if self.schema.is_none() {
                            self.lobby.promote_spectators();
                        }
                        self.broadcast_lobby();
                    }
                    out.push(GameEvent::PlayerLeft { id, reason });
                }
                Event::BadMessage { .. } => {}
                Event::Message { from, message } => self.on_message(from, message, &mut out),
            }
        }
        out
    }

    fn on_message(&mut self, from: PlayerId, message: Message, out: &mut Vec<GameEvent>) {
        let host = self.host;
        match &message {
            Message::GroupInfo(_)
            | Message::PlayerReady { .. }
            | Message::ChangeGameType(_)
            | Message::ChangeWeightClass(_)
            | Message::ArenaPreview(_)
            | Message::Text(_) => {
                if let Some(change) = self.lobby.apply(from, host, &message) {
                    if self.is_host() {
                        match &message {
                            Message::GroupInfo(_) => self.broadcast_lobby(),
                            Message::PlayerReady { .. } => {
                                self.session.send_with(Target::All, &message, true);
                            }
                            _ => {}
                        }
                    }
                    out.push(GameEvent::Lobby(change));
                }
            }
            Message::PrepareMatch { arena, .. } => out.push(GameEvent::Prepare {
                arena: arena.clone(),
            }),
            Message::PlayerLoading { player } => out.push(GameEvent::Loading { player: *player }),
            Message::PeerDataLoaded { player } => out.push(GameEvent::Loaded { player: *player }),
            Message::BeginMatch => out.push(GameEvent::Begin),
            Message::StartCountDown => out.push(GameEvent::CountDown),
            Message::RemoteInput { robot, input } => {
                if self.is_host() {
                    out.push(GameEvent::RemoteInput {
                        from,
                        robot: *robot,
                        input: *input,
                    });
                }
            }
            Message::Sync(sync) => {
                // "ERROR: Sync ignored - not in game yet".
                let Some(schema) = &self.schema else {
                    return;
                };
                if from != host {
                    return;
                }
                if let Ok(state) = sync.unpack(schema) {
                    // Non-guaranteed snapshots may arrive out of order: keep
                    // only newer ones.
                    if self.last_clock.is_some_and(|c| state.clock <= c) {
                        return;
                    }
                    self.last_clock = Some(state.clock);
                    out.push(GameEvent::Snapshot(state));
                }
            }
            Message::SystemText(text) => out.push(GameEvent::Chat {
                from,
                text: text.clone(),
            }),
            _ => out.push(GameEvent::Message { from, message }),
        }
    }

    /// Whether the match has started on this end (the sync schema is set).
    pub fn in_match(&self) -> bool {
        self.schema.is_some()
    }

    /// The local player's name.
    pub fn local_name(&self) -> String {
        self.local()
            .and_then(|id| self.lobby.slot(id))
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// Say something to everyone. Both chat boxes build the line the same way
    /// (`FUN_00475240` in the match, `0x492bc0` in the lobby): nothing for an
    /// empty entry, else `"[" + player name + "] " + text`, sent with flags 0
    /// to all. In the lobby it is a `0x65` text and shows in `chat messages`;
    /// in the match it is a `0x76` text and shows in `chat displaybox`. The
    /// original's own copy comes back through its loopback; here the line is
    /// returned (and kept in the lobby's chat) for the caller to show.
    pub fn chat(&mut self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }
        let me = self.local().unwrap_or(HOST_ID);
        let line = format!("[{}] {}", self.local_name(), text);
        if self.in_match() {
            self.session
                .send(Target::All, &Message::SystemText(line.clone()));
        } else {
            self.lobby.chat.push((me, line.clone()));
            self.session.send(Target::All, &Message::Text(line.clone()));
        }
        Some(line)
    }

    /// Choose the local player's robot (name, file and design stream).
    pub fn choose_robot(&mut self, team: &str, robot: &str, file: &str, design: Vec<u8>) {
        let Some(me) = self.local() else { return };
        let Some(index) = self.lobby.slots.iter().position(|s| s.player == me) else {
            return;
        };
        let slot = &mut self.lobby.slots[index];
        if slot.is_spectator() {
            return;
        }
        slot.kind = kind::HUMAN_WITH_ROBOT;
        slot.team = team.to_string();
        slot.robot = robot.to_string();
        slot.robot_file = file.to_string();
        slot.design = Some(design);
        let info = Message::GroupInfo(slot.to_group_info(index));
        if self.is_host() {
            self.broadcast_lobby();
        } else {
            self.session
                .send_with(Target::Player(self.host), &info, true);
        }
    }

    /// Toggle the local player's ready flag.
    pub fn set_ready(&mut self, ready: bool) {
        let Some(me) = self.local() else { return };
        if let Some(slot) = self.lobby.slot_mut(me) {
            if slot.is_spectator() {
                return;
            }
            slot.ready = ready;
        }
        let m = Message::PlayerReady { player: me, ready };
        if self.is_host() {
            self.session.send_with(Target::All, &m, true);
        } else {
            self.session.send_with(Target::Player(self.host), &m, true);
        }
    }

    /// Host: the settings dropdowns.
    pub fn set_settings(&mut self, game_type: &str, weight_class: &str, arena: &str) {
        self.lobby.game_type = game_type.to_string();
        self.lobby.weight_class = weight_class.to_string();
        self.lobby.arena = arena.to_string();
        for m in self.lobby.settings() {
            self.session.send_with(Target::All, &m, true);
        }
    }

    /// Host: close the lobby and start loading the match.
    pub fn start_match(&mut self, arena: &str) {
        self.session.set_accepting(false);
        self.lobby.arena = arena.to_string();
        let m = Message::PrepareMatch {
            flag: true,
            arena: arena.to_string(),
        };
        self.session.send_with(Target::All, &m, true);
    }

    /// Host: everyone is loaded; go.
    pub fn begin_match(&mut self) {
        self.session
            .send_with(Target::All, &Message::BeginMatch, true);
        self.session
            .send_with(Target::All, &Message::StartCountDown, true);
    }

    /// Both ends: the shared lists the snapshots are written against (known
    /// once the match is loaded).
    pub fn set_schema(&mut self, schema: SyncSchema) {
        self.schema = Some(schema);
        self.last_clock = None;
    }

    /// Both ends: the match is over and the players are back in the lobby.
    /// Snapshots stop (`in_match` is false again, so a lobby chat line goes out
    /// as a lobby text), and a host takes joiners and spectators again.
    pub fn end_match(&mut self) {
        self.schema = None;
        self.last_clock = None;
        if self.is_host() {
            self.session.set_accepting(true);
        }
    }

    /// Joiner: report loading progress to the host.
    pub fn report_loaded(&mut self) {
        let Some(me) = self.local() else { return };
        self.session.send_with(
            Target::Player(self.host),
            &Message::PeerDataLoaded { player: me },
            true,
        );
    }

    /// Joiner: one key event for our robot.
    pub fn send_input(&mut self, robot: u32, input: BotInput) {
        self.session.send(
            Target::Player(self.host),
            &Message::RemoteInput { robot, input },
        );
    }

    /// Host: advance the synchroniser by `dt`; when a snapshot is due, `capture`
    /// builds it and it goes to everyone. Returns whether one was sent.
    pub fn tick_sync(&mut self, dt: f32, capture: impl FnOnce() -> SyncState) -> bool {
        if !self.is_host() || !self.pacer.tick(dt) {
            return false;
        }
        let mut state = capture();
        self.clock = self.clock.wrapping_add(1).max(state.clock);
        state.clock = self.clock;
        let message = Message::Sync(SyncMessage::pack(&state));
        self.session.send(Target::All, &message);
        true
    }

    /// Host: send a message (scoring, win, end) to everyone.
    pub fn broadcast(&mut self, message: &Message) {
        self.session.send(Target::All, message);
    }

    /// Host: send a message that must arrive (the scoring messages: a lost
    /// `giBotDamageMessage` would leave the joiners' scores behind the host's;
    /// the original sends them with flags `0x28`, guaranteed, while its
    /// guarantee switch is on).
    pub fn broadcast_guaranteed(&mut self, message: &Message) {
        self.session.send_with(Target::All, message, true);
    }

    /// The pacing interval.
    pub fn sync_interval(&self) -> Duration {
        Duration::from_secs_f32(self.pacer.interval)
    }

    pub fn close(&mut self) {
        self.session.close();
    }
}
