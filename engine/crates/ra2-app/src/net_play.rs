//! Network play in the client: [`ra2net`] behind the shipped multiplayer
//! screens, and the in-match sync of a [`Duel`] (docs/38).
//!
//! The screens, as the original binds them:
//!
//! * `multiplayer/multiplayer.gib` (`MultiplayerWindow`, ctor `FUN_004976a0`):
//!   `Refresh button` enumerates hosts (`FUN_00498460`; LAN: DirectPlay
//!   `EnumHosts`, [`ra2net::dpn`]) and fills `server listbox`; `Join game
//!   button` connects to the selected row; `status` shows the progress.
//! * `multiplayer/ip_connect.gib`: `connect button` connects to the
//!   `IP address` text (`hostname[:port]`).
//! * `multiplayer/mp_create.gib` + `mpc_settings.gib` (`MultiplayerCreateWnd`):
//!   `start game button` hosts (`FUN_0049a780`) with `server name textbox`,
//!   `password textbox` and the `connection dropdown` speed, then the
//!   pre-match lobby opens.
//! * `multiplayer/prematch.gib` (`MultiplayerPrematchWnd`): the four
//!   `playerN group` panels show `activeplayer.gib` / `noplayer.gib`;
//!   `ready checkbox` toggles ready, `disconnect button` leaves, the host's
//!   `begin match button` starts.
//!
//! In the match (docs/38 §6.1, §10, §11): the host prices every hit and awards
//! the points, and sends them as `giBotDamageMessage` / `giAddPointsMessage` /
//! `giBotImmobileMessage` / `giPlayerOutMessage` ([`event_message`]); a joiner
//! shows them ([`message_event`]) and prices nothing of its own. Peers beyond
//! the four player panels are spectators: they load the arena, follow the
//! snapshots and the scoring messages, and have no robot. `\` opens the chat
//! entry (multiplayer matches only) and a line goes to everyone as a `0x76`
//! text.
//!
//! What is ours: the LAN refresh also queries the addresses in
//! `RA2_NET_HOSTS` (comma-separated `host:port`) besides the broadcast. The match wire
//! (snapshots, ids, key events) is [`crate::net_match`]'s.

use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ra2_shell::{Shell, Step};
use ra2fmt::gib::{Block, Gib, Widget, WidgetKind};
use ra2net::dpn;
use ra2net::game::{GameEvent, NetGame};
use ra2net::link::LinkConfig;
use ra2net::lobby::{self, SESSION_MAX};
use ra2net::message::{AddPoints, BotDamage, BotImmobile, Message};
use ra2net::session::{Browser, HostConfig, HostEntry, JoinConfig, PlayerId};
use ra2net::sync::{ConnectionSpeed, SyncSchema};
use ra2sim::controls::KeySet;
use ra2sim::sim::DriveInput;
use ra2sim::{NetEvent, NetRole};

pub const BROWSER_SCREEN: &str = "multiplayer/multiplayer.gib";
pub const CREATE_SCREEN: &str = "multiplayer/mp_create.gib";
pub const PREMATCH_SCREEN: &str = "multiplayer/prematch.gib";
pub const IP_WINDOW: &str = "multiplayer/ip_connect.gib";

/// The schema before a match is built: no robots. The match's own is
/// [`crate::net_match::schema`], set once the duel exists ([`NetPlay::on_match_built`]).
pub fn schema() -> SyncSchema {
    SyncSchema::default()
}

fn find<'a>(gib: &'a Gib, name: &str) -> Option<&'a Widget> {
    gib.blocks.iter().find_map(|b| match b {
        Block::Widget(root) => ra2ui::workshop::find(root, name),
        _ => None,
    })
}

fn find_mut<'a>(gib: &'a mut Gib, name: &str) -> Option<&'a mut Widget> {
    gib.blocks.iter_mut().find_map(|b| match b {
        Block::Widget(root) => ra2ui::workshop::find_mut(root, name),
        _ => None,
    })
}

/// A text box's text (its `Text` child, or its own `_Text`).
fn text_of(gib: &Gib, name: &str) -> Option<String> {
    let widget = find(gib, name)?;
    for child in &widget.children {
        if child.kind == WidgetKind::Text {
            if let Some(t) = ra2ui::input::field_of(child, "_Text") {
                return Some(t.trim().to_string());
            }
        }
    }
    ra2ui::input::field_of(widget, "_Text").map(|t| t.trim().to_string())
}

fn set_text(gib: &mut Gib, name: &str, text: &str) -> bool {
    let Some(widget) = find_mut(gib, name) else {
        return false;
    };
    ra2ui::input::set_field(widget, "_Text", text);
    for child in widget.children.iter_mut() {
        if child.kind == WidgetKind::Text {
            ra2ui::input::set_field(child, "_Text", text);
        }
    }
    true
}

/// Parse `host[:port]` (the port defaults to the game's).
pub fn parse_address(text: &str) -> Option<SocketAddr> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let with_port = if text.contains(':') {
        text.to_string()
    } else {
        format!("{text}:{}", dpn::DEFAULT_PORT)
    };
    with_port.to_socket_addrs().ok()?.find(|a| a.is_ipv4())
}

/// A sound the network layer asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSound {
    /// `Sounds\spectator_joined.wav` (`0x492f11`): a peer was created
    /// (event `0x64`, handler `FUN_00492ef0`, registered by the prematch
    /// window).
    SpectatorJoined,
}

/// What the application should do after a network step.
#[derive(Debug, Clone, PartialEq)]
pub enum NetAction {
    None,
    /// Build and start the match (both ends): the arena and the robots of the
    /// lobby, in slot order.
    StartMatch {
        arena: String,
    },
    /// The session is gone; the shell is back on the server list.
    Ended(String),
}

/// Where the match start is (docs/38 section 15: the original's start, as an original host and joiner
/// do it): the host sends `giPrepareMatchMessage` and its robots as `giAddArenaBotMessage`s, waits for
/// the joiners' robots, sends `giBeginMatchMessage` and builds the match; a joiner answers the prepare
/// with its robots, and builds the match when the begin arrives with every other robot in.
#[derive(Debug, Clone, PartialEq)]
enum Start {
    Idle,
    Host { arena: String, since: Instant },
    Joiner { arena: String, since: Instant },
}

/// How long a start waits for a peer's robot before it builds without it.
const START_PATIENCE: Duration = Duration::from_secs(20);

/// The robot file a lobby panel's robot is: the team and robot names look it up in the install's
/// two team tables (the panel carries names and a picture, not the design: that travels in
/// `giAddArenaBotMessage`).
pub fn robot_file(game: &Path, team: &str, robot: &str) -> Option<PathBuf> {
    use ra2ui::teams::{bot_info, bot_path, rosters, Side};
    let mut fallback = None;
    for side in [Side::Player, Side::Computer] {
        for roster in rosters(game, side).unwrap_or_default() {
            for &bot in &roster.robots {
                let path = bot_path(game, side, roster.slot, bot);
                if bot_info(&path).is_some_and(|(name, _)| name == robot) {
                    if roster.name.eq_ignore_ascii_case(team) {
                        return Some(path);
                    }
                    fallback.get_or_insert(path);
                }
            }
        }
    }
    fallback
}

/// The name a player has until `ra2.cfg` or the lobby's `screen name` box says otherwise: the
/// network object's constructor copies `Default Player` (`0x709644`, `0x621ec9`) into its name
/// string (`+0x244`), and the prematch constructor shows it in `screen name` (`0x49105a`-`0x491085`).
/// `FUN_006229e0` (`0x622941`) replaces it with `PlayerName=` of `ra2.cfg`'s
/// `[Begin Multiplayer Config]` section (`FUN_00622660` sets it) and the game port, 9791 (`0x263f`),
/// with `Port=`.
pub const DEFAULT_PLAYER_NAME: &str = "Default Player";

/// The multiplayer screens' network state.
pub struct NetPlay {
    pub game: Option<NetGame>,
    browser: Option<Browser>,
    pub rows: Vec<HostEntry>,
    pub status: String,
    connecting: Option<Instant>,
    /// Bind address for sessions and the browser (tests use loopback).
    pub bind_ip: std::net::IpAddr,
    /// The game port a host binds (0 for an ephemeral one).
    pub host_port: u16,
    /// Extra addresses a refresh queries.
    pub extra_hosts: Vec<SocketAddr>,
    pub player_name: String,
    pub link: LinkConfig,
    /// Which robot each human player drives (host side), and the keys each remote player holds.
    pub robot_of: BTreeMap<PlayerId, usize>,
    remote: BTreeMap<usize, KeySet>,
    /// Joiner: the keys last sent for our robot.
    last_keys: KeySet,
    /// Robot index to panel: the original's robot id (the order of the lobby's player panels).
    panels: Vec<u32>,
    /// The `.bot` of each panel's robot as the peers sent it, written to the save folder.
    bots: BTreeMap<u32, PathBuf>,
    /// Where the match start is (see [`Start`]).
    start: Start,
    /// Joiner: when the next `giPeerDataLoadedMessage` is due.
    next_loaded: Option<Instant>,
    /// Host: the fight clock of the next `giTimeSyncMessage`.
    next_time_sync: f32,
    sounds: Vec<NetSound>,
    /// Host: `giMatchWinMessage` has been sent.
    win_sent: bool,
    /// Host: `giEndMatchMessage` has been sent.
    end_sent: bool,
    /// This end has finished loading the match (a joiner has sent its
    /// `giPeerDataLoadedMessage`).
    loaded_sent: bool,
    /// Host: the peers that reported their match loaded.
    loaded: std::collections::BTreeSet<PlayerId>,
    /// The count-down has started (`giStartCountDownMessage`): the host has every peer's
    /// `giPeerDataLoadedMessage` and sent it; a joiner heard it.
    begun: bool,
    /// `giBeginMatchMessage` arrived: the peers load the match.
    begin_seen: bool,
    /// Events polled from the session and not yet consumed. `tick` (screens,
    /// lobby, match state) and `after_step` (snapshots, chat, scoring
    /// messages, which need the match) both run every frame in the windowed
    /// app; each takes only what it owns, so no event is dropped or seen twice.
    inbox: Vec<GameEvent>,
    /// Joiner: the host's `giEndMatchMessage` (`0x131`) arrived while this end's match was still
    /// running; the window leaves it for the lobby ([`NetPlay::take_leave_match`]).
    leave_match: bool,
}

impl Default for NetPlay {
    fn default() -> Self {
        NetPlay::new()
    }
}

impl NetPlay {
    pub fn new() -> NetPlay {
        let extra_hosts = std::env::var("RA2_NET_HOSTS")
            .ok()
            .map(|v| v.split(',').filter_map(parse_address).collect())
            .unwrap_or_default();
        NetPlay {
            game: None,
            browser: None,
            rows: Vec::new(),
            status: String::new(),
            connecting: None,
            bind_ip: "0.0.0.0".parse().expect("an address"),
            host_port: dpn::DEFAULT_PORT,
            extra_hosts,
            player_name: DEFAULT_PLAYER_NAME.to_string(),
            link: LinkConfig::default(),
            robot_of: BTreeMap::new(),
            remote: BTreeMap::new(),
            last_keys: KeySet::default(),
            panels: Vec::new(),
            bots: BTreeMap::new(),
            start: Start::Idle,
            next_loaded: None,
            next_time_sync: 0.0,
            sounds: Vec::new(),
            win_sent: false,
            end_sent: false,
            loaded_sent: false,
            loaded: Default::default(),
            begun: false,
            begin_seen: false,
            inbox: Vec::new(),
            leave_match: false,
        }
    }

    /// Read the install's `ra2.cfg` multiplayer keys: `PlayerName=` (a name that is not empty)
    /// and `Port=` (`FUN_006229e0`).
    pub fn load_config(&mut self, game: &std::path::Path) {
        let cfg = crate::options::read_cfg(game);
        self.apply_config(&cfg);
    }

    /// [`NetPlay::load_config`] on a parsed file.
    pub fn apply_config(&mut self, cfg: &ra2fmt::cfg::Cfg) {
        if let Some(name) = cfg
            .get("PlayerName")
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            self.player_name = name.to_string();
        }
        if let Some(port) = cfg.get("Port").and_then(|p| p.trim().parse::<u16>().ok()) {
            self.host_port = port;
        }
    }

    /// Poll the session once into the inbox. Snapshots are superseded by the
    /// newest one (they carry whole states), everything else keeps its order.
    fn pump(&mut self) {
        let Some(game) = &mut self.game else { return };
        for event in game.poll() {
            if matches!(event, GameEvent::Snapshot(_)) {
                self.inbox.retain(|e| !matches!(e, GameEvent::Snapshot(_)));
            }
            self.inbox.push(event);
        }
    }

    /// Take the inbox events `owns` accepts, in order; the rest stay.
    fn take_events(&mut self, owns: impl Fn(&GameEvent) -> bool) -> Vec<GameEvent> {
        let (taken, kept) = std::mem::take(&mut self.inbox)
            .into_iter()
            .partition(|e| owns(e));
        self.inbox = kept;
        taken
    }

    /// The match-state events, which need no session: a joiner's inputs, a
    /// peer loaded, the begin gate.
    fn absorb_state(&mut self, event: &GameEvent) {
        match event {
            GameEvent::Loaded { player } => {
                self.loaded.insert(*player);
            }
            GameEvent::Begin => self.begin_seen = true,
            GameEvent::CountDown => self.begun = true,
            GameEvent::RemoteInput { from, input, .. } => {
                if let Some(&robot) = self.robot_of.get(from) {
                    crate::net_match::apply_key(self.remote.entry(robot).or_default(), input);
                }
            }
            _ => {}
        }
    }

    /// The sounds asked for since the last call.
    pub fn take_sounds(&mut self) -> Vec<NetSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Whether the local player is a spectator (no player panel).
    pub fn is_spectator(&self) -> bool {
        self.game.as_ref().is_some_and(|g| {
            g.local()
                .and_then(|me| g.lobby.slot(me))
                .is_some_and(|s| s.is_spectator())
        })
    }

    /// Which robot each player drives: the place among the player panels.
    pub fn assign_robots(&mut self) {
        let Some(game) = &self.game else { return };
        self.robot_of = game
            .lobby
            .players()
            .enumerate()
            .filter(|(_, s)| s.is_human())
            .map(|(i, s)| (s.player, i))
            .collect();
        self.panels = game.lobby.players().map(|s| s.index).collect();
    }

    fn bind(&self, port: u16) -> SocketAddr {
        SocketAddr::new(self.bind_ip, port)
    }

    /// `Refresh button`: query the LAN broadcast and the extra hosts.
    pub fn refresh(&mut self) -> Result<(), String> {
        let mut browser = Browser::new(self.bind(0)).map_err(|e| format!("refresh: {e}"))?;
        if !self.bind_ip.is_loopback() {
            let broadcast = SocketAddr::from(([255, 255, 255, 255], dpn::ENUM_PORT));
            let _ = browser.query(broadcast);
            let game = SocketAddr::from(([255, 255, 255, 255], dpn::DEFAULT_PORT));
            let _ = browser.query(game);
        }
        for addr in self.extra_hosts.clone() {
            let _ = browser.query(addr);
        }
        self.rows.clear();
        self.browser = Some(browser);
        self.status = "Searching for games...".to_string();
        Ok(())
    }

    /// Join a host.
    pub fn join(&mut self, host: SocketAddr) -> Result<(), String> {
        let config = JoinConfig {
            link: self.link,
            ..JoinConfig::new(self.bind(0), host, &self.player_name)
        };
        self.game = Some(NetGame::join(config).map_err(|e| format!("join: {e}"))?);
        self.connecting = Some(Instant::now());
        self.reset_match();
        self.status = "Connecting to Server     10".to_string();
        Ok(())
    }

    /// Host a game.
    pub fn host(
        &mut self,
        name: &str,
        password: Option<String>,
        speed: ConnectionSpeed,
        arena: &MatchArena,
    ) -> Result<(), String> {
        // The original's six-field session name: its joiners read the
        // columns and the version from it (docs/38 §14).
        let session_name = dpn::SessionName {
            server: if name.is_empty() {
                "Default Server".to_string()
            } else {
                name.to_string()
            },
            game_type: DEFAULT_GAME_TYPE.to_string(),
            arena: arena.name.clone(),
            weight_class: DEFAULT_WEIGHT_CLASS.to_string(),
            version: dpn::VERSION.to_string(),
            speed: speed.session_tag().to_string(),
        };
        let config = HostConfig {
            bind: self.bind(self.host_port),
            session_name: session_name.encode(),
            player_name: self.player_name.clone(),
            max_players: SESSION_MAX,
            password,
            link: self.link,
        };
        let mut game = NetGame::host(config, speed).map_err(|e| format!("host: {e}"))?;
        // What the original's host tells each newcomer (live: the arena
        // preview, the game type and the weight class).
        game.set_settings(
            DEFAULT_GAME_TYPE,
            DEFAULT_WEIGHT_CLASS,
            &arena.preview_text(),
        );
        self.game = Some(game);
        self.robot_of.clear();
        self.robot_of.insert(ra2net::session::HOST_ID, 0);
        self.reset_match();
        Ok(())
    }

    fn reset_match(&mut self) {
        self.leave_match = false;
        self.win_sent = false;
        self.end_sent = false;
        self.loaded_sent = false;
        self.loaded.clear();
        self.begun = false;
        self.begin_seen = false;
        self.start = Start::Idle;
        self.next_loaded = None;
        self.next_time_sync = 0.0;
        self.bots.clear();
        self.inbox.clear();
    }

    /// Whether the match may run, asked before each step once the match is
    /// loaded (docs/38 §5): the first call reports this end loaded (a joiner
    /// sends `giPeerDataLoadedMessage`, `0x13f`); the host begins the match
    /// (`giBeginMatchMessage` `0x130`, `giStartCountDownMessage` `0x13c`) once
    /// every peer in the lobby has reported, and a joiner runs once it hears
    /// them. Until then the match holds before its countdown, so no peer's
    /// fight starts while another is still loading.
    pub fn match_begun(&mut self) -> bool {
        let Some(game) = &mut self.game else {
            return true;
        };
        if self.begun {
            return true;
        }
        if game.is_host() {
            // Every other peer in the session reports its match loaded (`0x13f`, spectators
            // too); then the count-down starts.
            let me = game.local();
            let everyone = game
                .session
                .players()
                .iter()
                .all(|p| Some(p.id) == me || self.loaded.contains(&p.id));
            if everyone {
                game.count_down();
                self.begun = true;
            }
        } else {
            // A joiner repeats its report about every 300 ms until the count-down comes
            // (live, from the original's joiner), and says each of its panels is loaded.
            let now = Instant::now();
            if self.next_loaded.is_none_or(|t| now >= t) {
                self.next_loaded = Some(now + Duration::from_millis(300));
                game.report_loaded();
                let me = game.local();
                let own: Vec<u32> = game
                    .lobby
                    .players()
                    .filter(|s| Some(s.player) == me)
                    .map(|s| s.index)
                    .collect();
                for panel in own {
                    game.report_panel(panel, true);
                }
            }
        }
        self.begun
    }

    /// This end's robots for the peers: each panel it owns with a robot, as `.bot` text
    /// (`giAddArenaBotMessage`: kind 1 human / 3 computer, the panel, the team), and `0x12f` that
    /// they are in. The file is the install's robot the panel names.
    fn send_own_bots(&mut self, game_dir: &Path) {
        let Some(game) = &mut self.game else { return };
        let me = game.local();
        let own: Vec<_> = game
            .lobby
            .players()
            .filter(|s| s.has_robot() && Some(s.player) == me)
            .cloned()
            .collect();
        for slot in own {
            let Some(path) = robot_file(game_dir, &slot.team, &slot.robot) else {
                eprintln!("network: no robot file for {} / {}", slot.team, slot.robot);
                continue;
            };
            let Some(bot) = std::fs::read(&path)
                .ok()
                .and_then(|bytes| ra2fmt::bot::parse(&bytes).ok())
            else {
                continue;
            };
            let kind = if slot.is_human() { 1 } else { 3 };
            game.send_arena_bot(kind, slot.index, 0, bot.write_wire());
            game.report_panel(slot.index, false);
            self.bots.insert(slot.index, path);
        }
    }

    /// A peer's robot arrived (`giAddArenaBotMessage`): the `.bot` goes to the save folder and
    /// the panel's match robot is that file. A joiner may only send its own panel's.
    fn take_bot(&mut self, game_dir: &Path, from: PlayerId, words: [u32; 3], design: &[u8]) {
        let Some(game) = &self.game else { return };
        let panel = words[1];
        let owner = game.lobby.panel(panel).map(|s| s.player);
        if from != ra2net::session::HOST_ID && owner != Some(from) {
            return;
        }
        let Ok(bot) = ra2fmt::bot::parse(design) else {
            eprintln!("network: a robot for panel {panel} that does not read");
            return;
        };
        let rel = Path::new("Net").join(format!("panel{panel}.bot"));
        match ra2fmt::saves::write(game_dir, &game_dir.join(rel), bot.write()) {
            Ok(path) => {
                self.bots.insert(panel, path);
            }
            Err(error) => eprintln!("network: cannot keep the robot of panel {panel}: {error}"),
        }
    }

    /// The start is complete when every panel with a robot has its file; the host then sends the
    /// begin and both ends build the match.
    fn finish_start(&mut self) -> NetAction {
        let Some(game) = &mut self.game else {
            return NetAction::None;
        };
        let missing = game
            .lobby
            .players()
            .filter(|s| s.has_robot())
            .any(|s| !self.bots.contains_key(&s.index));
        let (arena, since, host) = match &self.start {
            Start::Host { arena, since } => (arena.clone(), *since, true),
            Start::Joiner { arena, since } => (arena.clone(), *since, false),
            Start::Idle => return NetAction::None,
        };
        let waited = since.elapsed() >= START_PATIENCE;
        let ready = !missing && (host || self.begin_seen);
        if !ready && !(waited && (host || self.begin_seen)) {
            return NetAction::None;
        }
        if host {
            game.begin_match();
        }
        self.start = Start::Idle;
        self.assign_robots();
        NetAction::StartMatch { arena }
    }

    /// The match exists: the snapshots' lists are its robots'.
    pub fn on_match_built(&mut self, session: &mut crate::match_mode::Session) {
        if self.panels.is_empty() {
            self.panels = (0..session.matched.duel.robots.len() as u32).collect();
        }
        if let Some(game) = &mut self.game {
            game.set_schema(crate::net_match::schema(&session.matched.duel));
        }
        // The original starts the count-down at once; there is no arena intro in a network
        // match.
        session.skip_intro();
    }

    /// Joiner: the host ended the match while this end's was still on (an `End Match` from the
    /// pause dialog, a host that gave up): the window leaves the match for the lobby, as the
    /// original's `Game Window` `Show(0)` does (docs/38 section 13). Taken once.
    pub fn take_leave_match(&mut self) -> bool {
        std::mem::take(&mut self.leave_match)
    }

    /// The local match is over (results closed) and this end is back in the
    /// lobby: a rematch starts from a clean gate, the session stops syncing,
    /// and match events nobody consumed are dropped.
    pub fn end_match(&mut self) {
        // A host that leaves its match before the end was sent ends it for the joiners too
        // (`giEndMatchMessage`): they return to the lobby (`take_leave_match`).
        if let Some(game) = &mut self.game {
            if game.is_host() && game.in_match() && !self.end_sent {
                game.broadcast_guaranteed(&Message::EndMatch);
            }
        }
        self.reset_match();
        self.remote.clear();
        self.last_keys = KeySet::default();
        if let Some(game) = &mut self.game {
            game.end_match();
        }
    }

    /// The lobby's `screen name` box was committed (Enter; the same box event the chat entry
    /// uses, `0x5ea170(3)` at `0x490c67`): the handler `FUN_004931a0` sets the network name,
    /// sends it to the other players (`SetPeerInfo`) and renames our lobby entry. An empty
    /// box is ignored here (the original would clear the name).
    /// Enter in the lobby's `chat textbox`: the line goes to everyone as
    /// `[name] text` (a `0x65` text, flags 0) and the entry empties.
    pub fn commit_lobby_chat(&mut self, shell: &mut Shell) {
        let text = shell.focused_text().unwrap_or_default();
        if let Some(game) = &mut self.game {
            game.chat(text.trim());
        }
        if let Some(doc) = shell.document_mut() {
            set_text(doc, "chat textbox", "");
        }
        self.show(shell);
    }

    pub fn commit_screen_name(&mut self, text: &str) {
        let name = text.trim();
        if name.is_empty() {
            return;
        }
        self.player_name = name.to_string();
        if let Some(game) = &mut self.game {
            game.rename(name);
        }
    }

    pub fn leave(&mut self) {
        if let Some(game) = &mut self.game {
            game.close();
        }
        self.game = None;
        self.connecting = None;
        self.robot_of.clear();
        self.remote.clear();
        self.reset_match();
    }

    /// React to a shell click on a multiplayer screen. `screen` is the screen
    /// (or window) the click landed on.
    pub fn on_step(
        &mut self,
        shell: &mut Shell,
        screen: &str,
        step: &Step,
    ) -> Result<NetAction, String> {
        let hit = step.hit_widget().unwrap_or("");
        match (screen, hit) {
            (s, "Refresh button") if s.eq_ignore_ascii_case(BROWSER_SCREEN) => self.refresh()?,
            (s, "Join game button") if s.eq_ignore_ascii_case(BROWSER_SCREEN) => {
                let row =
                    find(shell.document(), "server listbox").and_then(ra2ui::input::selected_row);
                match row.and_then(|r| self.rows.get(r)).map(|h| h.addr) {
                    Some(addr) => self.join(addr)?,
                    None => self.status = "Select a game to join.".to_string(),
                }
            }
            (s, "connect button") if s.eq_ignore_ascii_case(IP_WINDOW) => {
                let text = text_of(shell.document(), "IP address").unwrap_or_default();
                shell.close_window();
                match parse_address(&text) {
                    Some(addr) => self.join(addr)?,
                    None => self.status = format!("Can't find {text}."),
                }
            }
            (s, "start game button") if s.eq_ignore_ascii_case(CREATE_SCREEN) => {
                let doc = shell.document();
                let name = text_of(doc, "server name textbox").unwrap_or_default();
                let password = text_of(doc, "password textbox").filter(|p| !p.is_empty());
                let speed = text_of(doc, "connection dropdown")
                    .and_then(|t| ConnectionSpeed::from_label(&t))
                    .unwrap_or(ConnectionSpeed::Lan);
                let arena = MatchArena::find(shell.game(), DEFAULT_ARENA);
                self.host(&name, password, speed, &arena)?;
                shell.goto(PREMATCH_SCREEN)?;
            }
            (s, "disconnect button") if s.eq_ignore_ascii_case(PREMATCH_SCREEN) => {
                self.leave();
                shell.goto(BROWSER_SCREEN)?;
                return Ok(NetAction::Ended("left".to_string()));
            }
            (s, "ready checkbox") if s.eq_ignore_ascii_case(PREMATCH_SCREEN) => {
                if let Some(game) = &mut self.game {
                    let me = game.local();
                    let ready = me
                        .and_then(|id| game.lobby.slot(id))
                        .is_some_and(|s| s.ready);
                    game.set_ready(!ready);
                }
            }
            (s, "begin match button") if s.eq_ignore_ascii_case(PREMATCH_SCREEN) => {
                if let Some(game) = &mut self.game {
                    if game.is_host() {
                        let picked = MatchArena::find(
                            shell.game(),
                            ra2net::lobby::arena_name(&game.lobby.arena),
                        );
                        let arena = picked.script.clone();
                        // PrepareMatch names the arena (`Bridge of Doom`, live), then the host's
                        // robots follow; the match is built once the joiners' are in.
                        self.reset_match();
                        if let Some(game) = &mut self.game {
                            game.start_match(&picked.name);
                        }
                        self.send_own_bots(shell.game());
                        self.start = Start::Host {
                            arena,
                            since: Instant::now(),
                        };
                    }
                }
            }
            _ => {}
        }
        self.show(shell);
        Ok(NetAction::None)
    }

    /// Pump the network; update the screen on show.
    pub fn tick(&mut self, shell: &mut Shell) -> Result<NetAction, String> {
        let mut action = NetAction::None;
        if let Some(browser) = &mut self.browser {
            if browser.poll() > 0 {
                self.rows = browser.hosts.clone();
                self.status = format!("{} game(s) found.", self.rows.len());
            }
        }
        self.pump();
        // The scoring, snapshot and chat events wait for `after_step`.
        let events = self.take_events(|e| {
            !matches!(
                e,
                GameEvent::Snapshot(_)
                    | GameEvent::Chat { .. }
                    | GameEvent::Message { .. }
                    | GameEvent::TimeSync { .. }
            )
        });
        if self.game.is_some() {
            for event in events {
                // In order: a `Begin` that follows its `Prepare` in one batch
                // must survive the new match's reset.
                if let GameEvent::Prepare { .. } = event {
                    self.reset_match();
                }
                self.absorb_state(&event);
                match event {
                    GameEvent::PlayerJoined { .. } => self.sounds.push(NetSound::SpectatorJoined),
                    GameEvent::Joined { .. } => {
                        self.connecting = None;
                        self.status.clear();
                        shell.goto(PREMATCH_SCREEN)?;
                    }
                    GameEvent::Refused(r) => {
                        self.connecting = None;
                        self.status = r.text().to_string();
                        action = NetAction::Ended(r.text().to_string());
                    }
                    GameEvent::ConnectFailed => {
                        self.connecting = None;
                        self.status = "Connection failed.".to_string();
                        action = NetAction::Ended(self.status.clone());
                    }
                    GameEvent::Terminated => {
                        action = NetAction::Ended("The host ended the session.".to_string());
                    }
                    GameEvent::Prepare { arena } => {
                        let arena = MatchArena::find(shell.game(), &arena).script;
                        // Answer with this end's robots (a spectator has none); the match is
                        // built when the begin arrives and every robot is in.
                        self.send_own_bots(shell.game());
                        self.start = Start::Joiner {
                            arena,
                            since: Instant::now(),
                        };
                    }
                    GameEvent::ArenaBot {
                        from,
                        words,
                        design,
                    } => self.take_bot(shell.game(), from, words, &design),
                    _ => {}
                }
            }
            if action == NetAction::None {
                action = self.finish_start();
            }
            if let Some(start) = self.connecting {
                let left = 10u64.saturating_sub(start.elapsed().as_secs());
                self.status = format!("Connecting to Server     {left}");
            }
        }
        if let NetAction::Ended(_) = action {
            self.game = None;
            self.reset_match();
            if !shell.base_screen_id().eq_ignore_ascii_case(BROWSER_SCREEN) {
                shell.goto(BROWSER_SCREEN)?;
            }
        }
        self.show(shell);
        Ok(action)
    }

    /// Draw the state into the screen on show.
    fn show(&mut self, shell: &mut Shell) {
        let screen = shell.base_screen_id();
        if screen.eq_ignore_ascii_case(BROWSER_SCREEN) && shell.window_depth() == 0 {
            let rows: Vec<String> = self
                .rows
                .iter()
                .map(|h| {
                    // The original's name carries the columns (docs/38 §14).
                    let (server, detail) = match dpn::SessionName::parse(&h.session_name) {
                        Some(n) => (
                            n.server,
                            format!("  {}  {}  {}", n.game_type, n.arena, n.speed),
                        ),
                        None => (h.session_name.clone(), String::new()),
                    };
                    format!(
                        "{}{}{}  {}/{}  {} ms",
                        if h.password { "* " } else { "" },
                        server,
                        detail,
                        h.current_players,
                        h.max_players,
                        h.ping.as_millis()
                    )
                })
                .collect();
            let status = self.status.clone();
            if let Some(doc) = shell.document_mut() {
                if let Some(list) = find_mut(doc, "server listbox") {
                    ra2ui::input::text_rows(list, &rows);
                }
                set_text(doc, "status", &status);
            }
        }
        if screen.eq_ignore_ascii_case(PREMATCH_SCREEN) && shell.window_depth() == 0 {
            if let Some(game) = &self.game {
                self.show_lobby(shell, game.lobby.clone(), game.local());
            }
        }
    }

    fn show_lobby(&self, shell: &mut Shell, lobby: ra2net::lobby::Lobby, me: Option<PlayerId>) {
        let ui = shell.game().join("UI");
        let active = ra2ui::workshop::load(&ui, "multiplayer/activeplayer.gib").ok();
        let empty = ra2ui::workshop::load(&ui, "multiplayer/noplayer.gib").ok();
        let editing_name = shell.text_focus() == Some("screen name");
        let editing_chat = shell.text_focus() == Some("chat textbox");
        let Some(doc) = shell.document_mut() else {
            return;
        };
        // The prematch constructor puts the network name in `screen name` (the box's own sample
        // text is "Mister Roboto").
        // (Not while the player is typing in it.)
        let my_name = me.and_then(|m| {
            lobby
                .members
                .iter()
                .find(|(id, _)| *id == m)
                .map(|(_, n)| n.clone())
                .or_else(|| lobby.slot(m).map(|s| s.name.clone()))
        });
        if let Some(name) = my_name {
            if !editing_name {
                set_text(doc, "screen name", &name);
            }
        }
        // The box's sample line is not shown by the original (an empty entry).
        if !editing_chat && text_of(doc, "chat textbox").as_deref() == Some(CHAT_SAMPLE) {
            set_text(doc, "chat textbox", "");
        }
        set_text(doc, "arena name", ra2net::lobby::arena_name(&lobby.arena));
        set_text(doc, "game type", &lobby.game_type);
        let chat: Vec<String> = lobby
            .chat
            .iter()
            .rev()
            .take(6)
            .rev()
            .map(|(_, t)| t.clone())
            .collect();
        set_text(doc, "chat messages", &chat.join("\n"));
        // The box is the body of a `DisplayBox` attachment: the lines live in
        // the attachment's markup, on the widget that carries it.
        set_display_box(doc, "chat messages", &chat.join("\n"));
        // The peers beyond the four panels, in the order they joined.
        // The original lists everyone in the session there (live).
        let spectators: Vec<String> = if lobby.members.is_empty() {
            lobby.spectators().map(|s| s.name.clone()).collect()
        } else {
            lobby.members.iter().map(|(_, n)| n.clone()).collect()
        };
        if let Some(list) = find_mut(doc, "spectator list") {
            ra2ui::input::text_rows(list, &spectators);
        }
        for i in 0..lobby::PLAYER_SLOTS {
            let panel = format!("player{} group", i + 1);
            // Panels are positions (`GroupInfo::slot`).
            let cell = match lobby.panel(i as u32) {
                Some(slot) => active
                    .as_ref()
                    .and_then(|g| g.widgets().next().cloned())
                    .map(|mut cell| {
                        // The original writes the team there (`The Rookies`).
                        let team = if slot.team.is_empty() {
                            &slot.name
                        } else {
                            &slot.team
                        };
                        ra2ui::cells::bind_value(&mut cell, "team name", team);
                        ra2ui::cells::bind_value(&mut cell, "bot name", &slot.robot);
                        ra2ui::cells::bind_art(
                            &mut cell,
                            "player icon img",
                            if Some(slot.player) == me {
                                "multiplayer\\local_player_icon.bmp"
                            } else {
                                "multiplayer\\remote_player_icon.bmp"
                            },
                        );
                        if let Some(check) = ra2ui::workshop::find_mut(&mut cell, "ready checkbox")
                        {
                            ra2ui::input::set_state(check, slot.ready);
                        }
                        cell
                    }),
                None => empty.as_ref().and_then(|g| g.widgets().next().cloned()),
            };
            if let (Some(cell), Some(group)) = (cell, find_mut(doc, &panel)) {
                group.children.truncate(1);
                group.children.push(cell);
            }
        }
    }

    /// The match a lobby starts: the arena, one robot per player panel in
    /// order (the slot's robot file when it names a design in the install,
    /// else a shipped AI robot), the local player's robot driven from the
    /// keyboard. A spectator drives nothing and watches (`Watch` camera). Both
    /// ends know their part in the scoring ([`NetRole`]) and have the chat key.
    pub fn match_session(
        &self,
        game: &std::path::Path,
        arena: &str,
    ) -> Result<crate::match_mode::Session, String> {
        let Some(net) = &self.game else {
            return Err("no session".to_string());
        };
        const FALLBACK: [&str; 4] = [
            "AI/Team0/Bot0.bot",
            "AI/Team5/Bot1.bot",
            "AI/Team1/Bot0.bot",
            "AI/Team2/Bot0.bot",
        ];
        let mut entries = Vec::new();
        for (i, slot) in net.lobby.players().take(4).enumerate() {
            // The robot each peer sent (`giAddArenaBotMessage`, written to the save folder), or
            // this end's own file; a shipped robot stands in for one that never came.
            let path = match self.bots.get(&slot.index) {
                Some(path) => path.to_string_lossy().to_string(),
                None => game.join(FALLBACK[i]).to_string_lossy().to_string(),
            };
            // A human panel is a person, this end's or a remote one; a
            // computer panel (the original's host sets those up) is the AI.
            entries.push(if slot.is_human() {
                ra2sim::r#match::RobotEntry::human(i, path)
            } else {
                ra2sim::r#match::RobotEntry::ai(i, path)
            });
        }
        while entries.len() < 2 {
            let i = entries.len();
            entries.push(ra2sim::r#match::RobotEntry::ai(
                i,
                game.join(FALLBACK[i]).to_string_lossy().to_string(),
            ));
        }
        let arena_path = game.join(arena);
        let me = net.local().unwrap_or(ra2net::session::HOST_ID);
        let robot = net.lobby.robot_index(me);
        // The robots the peers sent live in the save folder: their components are the install's.
        let had_game = std::env::var_os("RA2_GAME");
        std::env::set_var("RA2_GAME", game);
        let built = (|| -> Result<crate::match_mode::Session, String> {
            if robot.is_some() {
                crate::match_mode::Session::exhibition(
                    game,
                    &arena_path,
                    ra2sim::r#match::GameType::Deathmatch,
                    entries,
                    None,
                    None,
                )
                .map_err(|e| e.to_string())
            } else {
                crate::match_mode::Session::spectate(
                    game,
                    &arena_path,
                    ra2sim::r#match::GameType::Deathmatch,
                    entries,
                )
                .map_err(|e| e.to_string())
            }
        })();
        match had_game {
            Some(value) => std::env::set_var("RA2_GAME", value),
            None => std::env::remove_var("RA2_GAME"),
        }
        let mut session = built?;
        if let Some(robot) = robot {
            session.set_player_robot(game, robot);
        }
        session.matched.set_net_role(if net.is_host() {
            NetRole::Host
        } else {
            NetRole::Joiner
        });
        session.enable_chat();
        Ok(session)
    }

    /// In the match, before each fixed step: the host drives the remote
    /// humans' robots from their inputs; a joiner sends its own drive (a
    /// spectator has none); a chat line the player entered goes out.
    pub fn before_step(&mut self, session: &mut crate::match_mode::Session) {
        let Some(game) = &mut self.game else { return };
        for text in session.take_chat_outbox() {
            if let Some(line) = game.chat(&text) {
                session.push_chat_line(line);
            }
        }
        if game.is_host() {
            // A remote human's robot is driven by its inputs alone (the
            // host's `HumanRemote`): it stands still until the first one
            // arrives, never handed to the AI.
            let me = game.local();
            for (&player, &robot) in &self.robot_of {
                if Some(player) != me {
                    let keys = self.remote.get(&robot).copied().unwrap_or_default();
                    session.set_remote_drive(
                        robot,
                        Some(DriveInput {
                            keys,
                            ..DriveInput::default()
                        }),
                    );
                }
            }
        } else if let Some(&robot) = self.robot_of.get(&game.local().unwrap_or_default()) {
            // One key event per key that changed (`giRemoteInputMessage`: the binding's first
            // three words, 1 down / 0 up), for the robot of our panel.
            let keys = session.drive().keys;
            let panel = self.panels.get(robot).copied().unwrap_or(robot as u32);
            for event in crate::net_match::key_events(&self.last_keys, &keys) {
                game.send_input(panel, event);
            }
            self.last_keys = keys;
        }
    }

    /// After each fixed step: the host paces snapshots and sends what it
    /// decided (hits, points, immobility, who is out, the end); a joiner or a
    /// spectator applies the newest snapshot it has and shows the host's
    /// scoring messages.
    pub fn after_step(&mut self, session: &mut crate::match_mode::Session, dt: f32) {
        self.pump();
        // Match events, and the session-free match state `tick` may not have
        // reached yet; screen and lobby events stay for `tick`.
        let events = self.take_events(|e| {
            matches!(
                e,
                GameEvent::Snapshot(_)
                    | GameEvent::Chat { .. }
                    | GameEvent::Message { .. }
                    | GameEvent::TimeSync { .. }
                    | GameEvent::Loaded { .. }
                    | GameEvent::Begin
                    | GameEvent::CountDown
                    | GameEvent::RemoteInput { .. }
            )
        });
        for event in &events {
            self.absorb_state(event);
        }
        let Some(game) = &mut self.game else { return };
        if game.is_host() {
            let duel = &session.matched.duel;
            let panels = self.panels.clone();
            game.tick_sync(dt, || crate::net_match::capture(duel, &panels));
            // The match time left, about once a second of the fight (`giTimeSyncMessage`).
            if session.matched.state() == ra2sim::r#match::MatchState::Fight {
                let clock = session.matched.clock();
                if let Some(limit) = session.matched.config.duration {
                    if clock >= self.next_time_sync {
                        game.send_time_sync(limit - clock);
                        self.next_time_sync = clock + 1.0;
                    }
                }
            }
            let decided = session.matched.take_net_events();
            for event in &decided {
                let message = event_message(event, &self.panels);
                game.broadcast_guaranteed(&message);
            }
            if !self.win_sent
                && matches!(
                    session.matched.state(),
                    ra2sim::r#match::MatchState::Over | ra2sim::r#match::MatchState::Results
                )
            {
                // The win goes out at once; `giEndMatchMessage` when the host closes its results
                // window ([`NetPlay::end_match`]), as the original's host does (live): the
                // joiners show their results until then.
                self.win_sent = true;
                let winners = session.matched.report().winners;
                let panels = &self.panels;
                let id = |i: usize| {
                    winners
                        .get(i)
                        .map_or(u32::MAX, |w| panels.get(*w).copied().unwrap_or(*w as u32))
                };
                game.broadcast_guaranteed(&Message::MatchWin {
                    first: id(0),
                    second: id(1),
                });
            }
            for polled in events {
                if let GameEvent::Chat { text, .. } = polled {
                    session.push_chat_line(text);
                }
            }
        } else {
            let mut newest = None;
            for event in events {
                match event {
                    GameEvent::Snapshot(state) => newest = Some(state),
                    GameEvent::TimeSync { left } => session.matched.net_time_left(left),
                    GameEvent::Chat { text, .. } => session.push_chat_line(text),
                    GameEvent::Message { from, message } if from == ra2net::session::HOST_ID => {
                        match message {
                            // `giEndMatchMessage` -> the prematch window's `0x495250`: when the
                            // match is still on, the in-game popup closes and the game window
                            // hides (back to the lobby, `FUN_00475c30(0)`); either way the game
                            // phase is back at 1 (`FUN_004d0200(1)`), the lobby's again: the
                            // session stops syncing and accepts joiners. A finished match keeps
                            // its results window until it is closed.
                            Message::EndMatch => {
                                use ra2sim::r#match::MatchState;
                                if !matches!(
                                    session.matched.state(),
                                    MatchState::Over | MatchState::Results
                                ) {
                                    self.leave_match = true;
                                }
                                game.end_match();
                            }
                            Message::MatchWin { first, second } => {
                                let winners = [first, second]
                                    .into_iter()
                                    .filter(|w| *w != u32::MAX)
                                    .map(|w| {
                                        self.panels
                                            .iter()
                                            .position(|p| *p == w)
                                            .unwrap_or(w as usize)
                                    })
                                    .collect();
                                session.matched.net_finish(winners);
                            }
                            other => {
                                if let Some(event) = message_event(&other, &self.panels) {
                                    session.matched.apply_net_event(&event);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let Some(state) = newest {
                crate::net_match::apply(&mut session.matched.duel, &self.panels, &state);
            }
        }
    }
}

/// The wire form of a host decision (`FUN_00417330` and friends): a hit is a
/// `giBotDamageMessage`, points a `giAddPointsMessage`, the immobility warning
/// a `giBotImmobileMessage` (with where the robot sits), a robot going out a
/// `giPlayerOutMessage`.
pub fn event_message(event: &NetEvent, panels: &[u32]) -> Message {
    // A robot is named by its panel on the wire.
    let id = |robot: usize| panels.get(robot).copied().unwrap_or(robot as u32);
    match event {
        NetEvent::Hit {
            attacker,
            victim,
            component,
            damage,
            point,
            normal,
        } => Message::BotDamage(BotDamage::new(
            attacker.map(|(a, by)| (id(a), by as u32)),
            (id(*victim), *component as u32),
            *damage,
            point.to_array(),
            normal.to_array(),
        )),
        NetEvent::Points {
            player,
            points,
            script,
        } => Message::AddPoints(AddPoints::new(id(*player), *points, [0.0; 3], *script)),
        NetEvent::Immobile {
            robot,
            on,
            at,
            seconds,
        } => Message::BotImmobile(BotImmobile::new(id(*robot), *at, *on, *seconds)),
        NetEvent::Out { robot, reason } => Message::PlayerOut {
            player: id(*robot),
            reason: *reason,
        },
    }
}

/// What a joiner makes of a host's scoring message (the handlers of
/// `FUN_004ce940`: `0x135` -> `FUN_00417ae0`, `0x139` -> `FUN_004178a0`,
/// `0x13b` -> `FUN_004120e0`, `0x13a` -> `FUN_00418040`).
pub fn message_event(message: &Message, panels: &[u32]) -> Option<NetEvent> {
    use glam::Vec3;
    // The robot of a panel id.
    let robot = |id: u32| panels.iter().position(|p| *p == id).unwrap_or(id as usize);
    Some(match message {
        Message::BotDamage(d) => {
            let (victim, component) = d.victim();
            NetEvent::Hit {
                attacker: d.attacker().map(|(a, by)| (robot(a), by as usize)),
                victim: robot(victim),
                component: component as usize,
                damage: d.damage(),
                point: Vec3::from_array(d.a),
                normal: Vec3::from_array(d.b),
            }
        }
        Message::AddPoints(p) => NetEvent::Points {
            player: robot(p.player),
            points: p.points_f32(),
            script: p.flags[1],
        },
        Message::BotImmobile(b) => NetEvent::Immobile {
            robot: robot(b.robot),
            on: b.immobile,
            at: b.at,
            seconds: b.seconds(),
        },
        Message::PlayerOut { player, reason } => NetEvent::Out {
            robot: robot(*player),
            reason: *reason,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key is one event down and one up (`giRemoteInputMessage`: the binding's words, 1/0), and
    /// the receiving end holds what it was told.
    #[test]
    fn key_changes_travel_as_events_and_the_far_end_holds_them() {
        use crate::net_match::{apply_key, key_events};
        let mut now = KeySet::default();
        let mut far = KeySet::default();
        let mut last = KeySet::default();
        for code in [17, 30] {
            now.set(code, true);
        }
        let events = key_events(&last, &now);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, [0, 0, 17]);
        assert_eq!(events[0].value, 1);
        for e in &events {
            apply_key(&mut far, e);
        }
        last = now;
        assert!(far.contains(17) && far.contains(30));
        now.set(17, false);
        let events = key_events(&last, &now);
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].key, events[0].value), ([0, 0, 17], 0));
        for e in &events {
            apply_key(&mut far, e);
        }
        assert!(!far.contains(17) && far.contains(30));
        assert!(
            key_events(&now, &now).is_empty(),
            "nothing changed: nothing sent"
        );
    }

    /// The host's decisions survive the wire: an event becomes the message
    /// the exe's constructor builds and a joiner reads the same event back.
    #[test]
    fn scoring_events_round_trip_through_their_messages() {
        use glam::Vec3;
        let hit = NetEvent::Hit {
            attacker: Some((2, 1)),
            victim: 0,
            component: 3,
            damage: 33.25,
            point: Vec3::new(1.0, 2.0, 3.0),
            normal: Vec3::Y,
        };
        let hazard = NetEvent::Hit {
            attacker: None,
            victim: 1,
            component: 0,
            damage: 25.0,
            point: Vec3::ZERO,
            normal: Vec3::ZERO,
        };
        let points = NetEvent::Points {
            player: 2,
            points: 33.25,
            script: false,
        };
        let script = NetEvent::Points {
            player: 1,
            points: 5.0,
            script: true,
        };
        let out = NetEvent::Out {
            robot: 3,
            reason: ra2sim::netevent::out::IMMOBILIZED,
        };
        let immobile = NetEvent::Immobile {
            robot: 1,
            on: true,
            at: [1.0, 0.0, 2.0],
            seconds: 10.0,
        };
        for event in [hit, hazard, points, script, out] {
            let message = match &event {
                NetEvent::Hit {
                    attacker,
                    victim,
                    component,
                    damage,
                    point,
                    normal,
                } => Message::BotDamage(BotDamage::new(
                    attacker.map(|(a, b)| (a as u32, b as u32)),
                    (*victim as u32, *component as u32),
                    *damage,
                    point.to_array(),
                    normal.to_array(),
                )),
                NetEvent::Points {
                    player,
                    points,
                    script,
                } => Message::AddPoints(AddPoints::new(*player as u32, *points, [0.0; 3], *script)),
                NetEvent::Out { robot, reason } => Message::PlayerOut {
                    player: *robot as u32,
                    reason: *reason,
                },
                NetEvent::Immobile { .. } => unreachable!(),
            };
            let wire = Message::decode(&message.encode()).unwrap();
            assert_eq!(message_event(&wire, &[0, 1, 2, 3]), Some(event));
        }
        let wire = Message::BotImmobile(BotImmobile::new(1, [1.0, 0.0, 2.0], true, 10.0));
        assert_eq!(message_event(&wire, &[0, 1, 2, 3]), Some(immobile));
        assert_eq!(message_event(&Message::StartCountDown, &[0, 1]), None);
        // The wire names a robot by its panel: the robots of panels 1 and 3 are 0 and 1.
        let hit = NetEvent::Immobile {
            robot: 1,
            on: true,
            at: [0.0; 3],
            seconds: 1.0,
        };
        let Message::BotImmobile(b) = event_message(&hit, &[1, 3]) else {
            panic!("an immobile message");
        };
        assert_eq!(b.robot, 3);
        assert_eq!(message_event(&Message::BotImmobile(b), &[1, 3]), Some(hit));
    }

    #[test]
    fn addresses_take_the_game_port_by_default() {
        assert_eq!(
            parse_address("127.0.0.1"),
            Some("127.0.0.1:9791".parse().unwrap())
        );
        assert_eq!(
            parse_address(" 127.0.0.1:1234 "),
            Some("127.0.0.1:1234".parse().unwrap())
        );
        assert_eq!(parse_address(""), None);
    }
}

/// The arena a new multiplayer game starts on (`Combat Arena`, the box).
pub const DEFAULT_ARENA: &str = "Arenas/box.py";
/// The game type and weight class a new game announces (the original's
/// defaults, live: `DEATHMATCH`, `ALLWEIGHT`).
pub const DEFAULT_GAME_TYPE: &str = "DEATHMATCH";
pub const DEFAULT_WEIGHT_CLASS: &str = "ALLWEIGHT";

/// An arena as the multiplayer messages name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArena {
    /// The script, relative to the install (`Arenas/box.py`).
    pub script: String,
    /// The display name (`Combat Arena`), what `PrepareMatch` and the session
    /// name carry.
    pub name: String,
    /// The preview art relative to `Arenas` (`box/box_preview.bmp`).
    pub preview: String,
}

impl MatchArena {
    /// The `ArenaPreview` text: `Arenas/<preview>:<name>` (live:
    /// `Arenas/bridge/bridge_preview.bmp:Bridge of Doom`).
    pub fn preview_text(&self) -> String {
        format!("Arenas/{}:{}", self.preview, self.name)
    }

    /// The arena a script path or a display name names, case-insensitively;
    /// the box when nothing matches.
    pub fn find(game: &std::path::Path, key: &str) -> MatchArena {
        let all = ra2ui::arenas::all(game).unwrap_or_default();
        let key = key.trim();
        let script_of = |a: &ra2ui::arenas::Arena| format!("Arenas/{}", a.script);
        let hit = all
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(key) || script_of(a).eq_ignore_ascii_case(key));
        let hit = hit.or_else(|| {
            all.iter()
                .find(|a| script_of(a).eq_ignore_ascii_case(DEFAULT_ARENA))
        });
        match hit {
            Some(a) => MatchArena {
                script: script_of(a),
                name: a.name.clone(),
                preview: a.preview.clone(),
            },
            None => MatchArena {
                script: DEFAULT_ARENA.to_string(),
                name: "Combat Arena".to_string(),
                preview: "box/box_preview.bmp".to_string(),
            },
        }
    }
}

/// `prematch.gib`'s sample text in `chat textbox`.
const CHAT_SAMPLE: &str = "This is a chat message.";

/// Set the text of the `DisplayBox` whose body is named `name` (the
/// attachment sits on another widget, so the search looks for the widget that
/// carries it).
fn set_display_box(gib: &mut Gib, name: &str, text: &str) -> bool {
    fn owner<'a>(w: &'a mut Widget, name: &str) -> Option<&'a mut Widget> {
        let carries = w
            .attachments
            .iter()
            .any(|a| a.body.as_ref().is_some_and(|b| b.name() == name));
        if carries {
            return Some(w);
        }
        for c in w.children.iter_mut() {
            if let Some(hit) = owner(c, name) {
                return Some(hit);
            }
        }
        for a in w.attachments.iter_mut() {
            if let Some(b) = &mut a.body {
                if let Some(hit) = owner(b, name) {
                    return Some(hit);
                }
            }
        }
        None
    }
    for b in gib.blocks.iter_mut() {
        if let Block::Widget(root) = b {
            if let Some(w) = owner(root, name) {
                return crate::hq_mode::set_display(w, text);
            }
        }
    }
    false
}
