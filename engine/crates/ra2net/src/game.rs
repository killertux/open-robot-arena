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
    /// A player changed its name (the original's `0x72`, PEER_INFO).
    PlayerRenamed {
        id: PlayerId,
        name: String,
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
    /// `giAddArenaBotMessage` (`0x12d`): a peer's robot for one panel as `.bot`
    /// text. `words` are the robot's kind (1 human, 3 computer), the panel and
    /// the team (docs/38 section 15).
    ArenaBot {
        from: PlayerId,
        words: [u32; 3],
        design: Vec<u8>,
    },
    /// `giPlayerReadyMessage` (`0x12f`) during the match start: a panel's robot
    /// is loaded (`flag` false: the bots are in) or the match is loaded (`flag`
    /// true).
    PanelReport {
        from: PlayerId,
        panel: u32,
        flag: bool,
    },
    /// `giTimeSyncMessage` (`0x138`): the host's match time left, in seconds.
    TimeSync {
        left: f32,
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
    /// When set, every `giSyncMessage` that arrives is also kept here, whole
    /// (the tools that record the original's snapshots read it).
    pub tap_sync: Option<Vec<SyncMessage>>,
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
            tap_sync: None,
        })
    }

    pub fn join(config: JoinConfig) -> io::Result<NetGame> {
        Ok(NetGame {
            session: Session::join(config)?,
            lobby: Lobby::default(),
            host: HOST_ID,
            schema: None,
            pacer: Pacer::new(ConnectionSpeed::Lan),
            tap_sync: None,
        })
    }

    pub fn is_host(&self) -> bool {
        self.session.is_host()
    }

    pub fn local(&self) -> Option<PlayerId> {
        self.session.local_id()
    }

    /// Host: tell the peers the panels that changed since `before`.
    /// DirectPlay's mesh delivers a peer's message to the others directly and
    /// never back to its sender (`DPNSEND_NOLOOPBACK`, flags `0x20` on the
    /// original's `giGroupInfoMessage`); this host relays, so a panel the
    /// `claimant` itself just described does not go back to it. A peer is the
    /// authority of its own panel: the original takes a `giGroupInfoMessage`
    /// that names its own panel for a remote one (the panel loses its Eject,
    /// Ready and change-robot controls; live, docs/38 section 15).
    fn publish_lobby(&mut self, before: &[crate::lobby::Slot], claimant: Option<PlayerId>) {
        let me = self.local();
        let peers: Vec<PlayerId> = self
            .session
            .players()
            .into_iter()
            .map(|p| p.id)
            .filter(|id| Some(*id) != me)
            .collect();
        for slot in self.lobby.slots.clone() {
            if slot.is_spectator() || before.contains(&slot) {
                continue;
            }
            let m = Message::GroupInfo(slot.to_group_info());
            for p in &peers {
                if Some(*p) != claimant || slot.player != *p {
                    self.session.send_with(Target::Player(*p), &m, true);
                }
            }
        }
        self.session.set_host_data(self.lobby.host_data());
    }

    /// Host: a newcomer is told every panel, its own default one included.
    fn send_lobby_to(&mut self, to: PlayerId) {
        for m in self.lobby.group_infos() {
            self.session.send_with(Target::Player(to), &m, true);
        }
    }

    /// Pump the session.
    pub fn poll(&mut self) -> Vec<GameEvent> {
        let mut out = Vec::new();
        for event in self.session.poll() {
            match event {
                Event::Connected {
                    local,
                    host,
                    players,
                } => {
                    self.host = host;
                    for p in &players {
                        self.lobby.add_watcher(p.id, &p.name);
                    }
                    out.push(GameEvent::Joined { local });
                }
                Event::Refused(r) => out.push(GameEvent::Refused(r)),
                Event::ConnectFailed => out.push(GameEvent::ConnectFailed),
                Event::SessionTerminated => out.push(GameEvent::Terminated),
                Event::PlayerJoined(p) => {
                    if !self.is_host() {
                        self.lobby.add_watcher(p.id, &p.name);
                    }
                    if self.is_host() {
                        let before = self.lobby.slots.clone();
                        self.lobby.join(p.id, &p.name);
                        // The newcomer needs the settings and every slot;
                        // the others need the new slot.
                        for m in self.lobby.settings() {
                            self.session.send_with(Target::Player(p.id), &m, true);
                        }
                        self.send_lobby_to(p.id);
                        self.publish_lobby(&before, Some(p.id));
                    }
                    out.push(GameEvent::PlayerJoined {
                        id: p.id,
                        name: p.name,
                    });
                }
                Event::PlayerRenamed(p) => {
                    for slot in self.lobby.slots.iter_mut().filter(|s| s.player == p.id) {
                        slot.name = p.name.clone();
                    }
                    self.lobby.add_member(p.id, &p.name);
                    out.push(GameEvent::PlayerRenamed {
                        id: p.id,
                        name: p.name,
                    });
                }
                Event::PlayerLeft { id, reason } => {
                    let before = self.lobby.slots.clone();
                    self.lobby.leave(id);
                    if self.is_host() {
                        // A freed player panel goes to the first spectator.
                        if self.schema.is_none() {
                            self.lobby.promote_spectators();
                        }
                        self.publish_lobby(&before, None);
                    }
                    out.push(GameEvent::PlayerLeft { id, reason });
                }
                // Bridge mode only (the `dpnet` replacement); a NetGame never sets it.
                Event::BadMessage { .. }
                | Event::Raw { .. }
                | Event::ConnectRequest { .. }
                | Event::EnumQuery { .. } => {}
                Event::Message { from, message } => self.on_message(from, message, &mut out),
            }
        }
        out
    }

    fn on_message(&mut self, from: PlayerId, message: Message, out: &mut Vec<GameEvent>) {
        let host = self.host;
        match &message {
            Message::GroupInfo(_)
            | Message::ChangeGameType(_)
            | Message::ChangeWeightClass(_)
            | Message::ArenaPreview(_)
            | Message::Text(_) => {
                let before = self.lobby.slots.clone();
                if let Some(change) = self.lobby.apply(from, host, &message) {
                    if self.is_host() {
                        match &message {
                            Message::GroupInfo(_) => self.publish_lobby(&before, Some(from)),
                            Message::PlayerReady { .. } => {
                                // Not back to the sender (see `publish_lobby`).
                                let me = self.local();
                                let peers: Vec<PlayerId> = self
                                    .session
                                    .players()
                                    .into_iter()
                                    .map(|p| p.id)
                                    .filter(|id| Some(*id) != me && *id != from)
                                    .collect();
                                for p in peers {
                                    self.session.send_with(Target::Player(p), &message, true);
                                }
                            }
                            _ => {}
                        }
                    }
                    out.push(GameEvent::Lobby(change));
                }
            }
            Message::AddArenaBot { words, design } => out.push(GameEvent::ArenaBot {
                from,
                words: *words,
                design: design.clone(),
            }),
            Message::PlayerReady { player, ready } => out.push(GameEvent::PanelReport {
                from,
                panel: *player,
                flag: *ready,
            }),
            Message::TimeSync { word, .. } => out.push(GameEvent::TimeSync {
                left: f32::from_bits(*word),
            }),
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
                if let Some(tap) = &mut self.tap_sync {
                    tap.push(sync.clone());
                }
                // "ERROR: Sync ignored - not in game yet".
                let Some(schema) = &self.schema else {
                    return;
                };
                if from != host {
                    return;
                }
                if let Ok(state) = sync.unpack(schema) {
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

    /// The lobby's `screen name` box was edited (`FUN_004931a0`): set our name
    /// (`FUN_00622660`), tell the other players (`FUN_00623580`, `SetPeerInfo`)
    /// and update our own lobby entry (`FUN_0047f8a0`).
    pub fn rename(&mut self, name: &str) {
        let Some(me) = self.local() else { return };
        if let Some(slot) = self.lobby.slot_mut(me) {
            slot.name = name.to_string();
        }
        self.session.set_name(name);
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

    /// Choose the local player's robot (team, team logo, robot name and
    /// design stream). A player without a panel takes the first free one (as
    /// a joiner of the original's lobby, where everyone starts without one);
    /// with none free it stays a spectator.
    pub fn choose_robot(&mut self, team: &str, robot: &str, logo: &str, design: Vec<u8>) {
        self.claim_panel(None, team, robot, logo, design);
    }

    /// [`NetGame::choose_robot`] for a panel the player picked: the original's
    /// lobby offers each open panel's join button (`noplayer.gib`), and the
    /// claim names that panel.
    pub fn claim_panel(
        &mut self,
        panel: Option<u32>,
        team: &str,
        robot: &str,
        logo: &str,
        design: Vec<u8>,
    ) {
        let Some(me) = self.local() else { return };
        let name = self
            .lobby
            .members
            .iter()
            .find(|(id, _)| *id == me)
            .map(|(_, n)| n.clone())
            .or_else(|| self.lobby.slot(me).map(|s| s.name.clone()))
            .unwrap_or_default();
        let index = match (panel, self.lobby.slot(me)) {
            (Some(i), _) => i,
            (None, Some(s)) if s.is_human() => s.index,
            _ => match self.lobby.free_panel() {
                Some(i) => i,
                None => return,
            },
        };
        let slot = crate::lobby::Slot {
            index,
            player: me,
            kind: kind::HUMAN_WITH_ROBOT,
            name,
            team: team.to_string(),
            team_logo: logo.to_string(),
            robot: robot.to_string(),
            ready: self.lobby.slot(me).is_some_and(|s| s.is_human() && s.ready),
            design: Some(design),
        };
        let before = self.lobby.slots.clone();
        self.lobby.set_panel(slot.clone());
        let slot = &slot;
        let info = Message::GroupInfo(slot.to_group_info());
        if self.is_host() {
            self.publish_lobby(&before, None);
        } else {
            self.session
                .send_with(Target::Player(self.host), &info, true);
        }
    }

    /// Toggle the local player's ready flag: it is the panel's `giGroupInfoMessage`
    /// flag, sent again without the picture (the original's 95-byte message).
    pub fn set_ready(&mut self, ready: bool) {
        let Some(me) = self.local() else { return };
        let before = self.lobby.slots.clone();
        let Some(slot) = self.lobby.slot_mut(me) else {
            return;
        };
        if slot.is_spectator() {
            return;
        }
        slot.ready = ready;
        let mut info = slot.to_group_info();
        info.robot = None;
        if self.is_host() {
            self.publish_lobby(&before, None);
        } else {
            self.session
                .send_with(Target::Player(self.host), &Message::GroupInfo(info), true);
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

    /// Host: the robots are in; the peers load the match (`giBeginMatchMessage`).
    pub fn begin_match(&mut self) {
        self.session
            .send_with(Target::All, &Message::BeginMatch, true);
    }

    /// Host: every peer reported its match loaded (`giPeerDataLoadedMessage`):
    /// the count-down starts (`giStartCountDownMessage`).
    pub fn count_down(&mut self) {
        self.session
            .send_with(Target::All, &Message::StartCountDown, true);
    }

    /// Everyone: this end's robot for one panel, as `.bot` text, to the others
    /// (`giAddArenaBotMessage`, flags `0x38`: guaranteed, no loopback).
    pub fn send_arena_bot(&mut self, kind: u32, panel: u32, team: u32, design: Vec<u8>) {
        let m = Message::AddArenaBot {
            words: [kind, panel, team],
            design,
        };
        self.session.send_with(Target::All, &m, true);
    }

    /// Everyone: a panel's report during the start (`giPlayerReadyMessage`):
    /// `false` once the bots are in, `true` once the match is loaded.
    pub fn report_panel(&mut self, panel: u32, loaded: bool) {
        let m = Message::PlayerReady {
            player: panel,
            ready: loaded,
        };
        self.session.send_with(Target::All, &m, false);
    }

    /// Host: the match time left, once a second (`giTimeSyncMessage`; the other
    /// four words of the message are stack garbage in the original).
    pub fn send_time_sync(&mut self, left: f32) {
        self.session.send(
            Target::All,
            &Message::TimeSync {
                word: left.to_bits(),
                times: [0; 4],
            },
        );
    }

    /// Both ends: the shared lists the snapshots are written against (known
    /// once the match is loaded).
    pub fn set_schema(&mut self, schema: SyncSchema) {
        self.schema = Some(schema);
    }

    /// Both ends: the match is over and the players are back in the lobby.
    /// Snapshots stop (`in_match` is false again, so a lobby chat line goes out
    /// as a lobby text), and a host takes joiners and spectators again.
    pub fn end_match(&mut self) {
        self.schema = None;
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
        let state = capture();
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
