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
//! What is ours: the LAN refresh also queries the addresses in
//! `RA2_NET_HOSTS` (comma-separated `host:port`) besides the broadcast; the
//! remote-input key words name our drive axes, not a `.bot` control binding
//! ([`AXIS_THROTTLE`], [`AXIS_STEER`]).

use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

use ra2_shell::{Shell, Step};
use ra2fmt::gib::{Block, Gib, Widget, WidgetKind};
use ra2net::dpn;
use ra2net::game::{GameEvent, NetGame};
use ra2net::link::LinkConfig;
use ra2net::message::BotInput;
use ra2net::session::{Browser, HostConfig, HostEntry, JoinConfig, PlayerId};
use ra2net::sync::{BodySync, ConnectionSpeed, SyncSchema, SyncState};
use ra2sim::duel::Duel;
use ra2sim::sim::DriveInput;

pub const BROWSER_SCREEN: &str = "multiplayer/multiplayer.gib";
pub const CREATE_SCREEN: &str = "multiplayer/mp_create.gib";
pub const PREMATCH_SCREEN: &str = "multiplayer/prematch.gib";
pub const IP_WINDOW: &str = "multiplayer/ip_connect.gib";

/// `giRemoteInputMessage` key words for our drive axes.
pub const AXIS_THROTTLE: u32 = 0;
pub const AXIS_STEER: u32 = 1;

/// A body's id in a snapshot: robot index and part index.
pub fn body_id(robot: usize, part: usize) -> u32 {
    ((robot as u32) << 16) | part as u32
}

/// The snapshot of every robot part body (`giSyncMessage` bodies; the robot,
/// controller and device lists stay empty in this client, so the schema is
/// the empty one on both ends).
pub fn capture(duel: &Duel) -> SyncState {
    let mut bodies = Vec::new();
    for (r, robot) in duel.robots.iter().enumerate() {
        for (p, part) in robot.parts.iter().enumerate() {
            if !part.alive {
                continue;
            }
            let Some(state) = duel.physics.state(part.body) else {
                continue;
            };
            let q = state.rotation;
            bodies.push(BodySync {
                id: body_id(r, p),
                awake: true,
                position: state.position.to_array(),
                rotation: [q.w, q.x, q.y, q.z],
                linear: state.linear_velocity.to_array(),
                angular: state.angular_velocity.to_array(),
            });
        }
    }
    SyncState {
        clock: 0,
        clock2: 0,
        partial: false,
        bodies,
        ..SyncState::default()
    }
}

/// The schema [`capture`] writes against.
pub fn schema() -> SyncSchema {
    SyncSchema::default()
}

/// Put a snapshot's bodies into the duel. Returns how many were applied.
pub fn apply(duel: &mut Duel, state: &SyncState) -> usize {
    let mut applied = 0;
    for body in &state.bodies {
        let (r, p) = ((body.id >> 16) as usize, (body.id & 0xffff) as usize);
        let Some(part) = duel.robots.get(r).and_then(|robot| robot.parts.get(p)) else {
            continue;
        };
        if !part.alive {
            continue;
        }
        let handle = part.body;
        let [w, x, y, z] = body.rotation;
        let rotation = glam::Quat::from_xyzw(x, y, z, w).normalize();
        duel.physics
            .set_transform(handle, glam::Vec3::from_array(body.position), rotation);
        duel.physics.set_velocity(
            handle,
            glam::Vec3::from_array(body.linear),
            glam::Vec3::from_array(body.angular),
        );
        applied += 1;
    }
    applied
}

/// The two key events a drive becomes (values in percent, as a control's
/// value is).
pub fn drive_inputs(drive: DriveInput) -> [BotInput; 2] {
    let pct = |v: f32| (v.clamp(-1.0, 1.0) * 100.0).round() as i32 as u32;
    [
        BotInput {
            key: [AXIS_THROTTLE, 0, 0],
            value: pct(drive.throttle),
        },
        BotInput {
            key: [AXIS_STEER, 0, 0],
            value: pct(drive.steer),
        },
    ]
}

/// Fold one key event into a remote robot's drive.
pub fn apply_input(drive: &mut DriveInput, input: &BotInput) {
    let v = (input.value as i32) as f32 / 100.0;
    match input.key[0] {
        AXIS_THROTTLE => drive.throttle = v,
        AXIS_STEER => drive.steer = v,
        _ => {}
    }
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
    /// Host side: which robot each player drives, and their last drives.
    pub robot_of: BTreeMap<PlayerId, usize>,
    remote: BTreeMap<usize, DriveInput>,
    last_sent: Option<(DriveInput, Instant)>,
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
            player_name: "Player".to_string(),
            link: LinkConfig::default(),
            robot_of: BTreeMap::new(),
            remote: BTreeMap::new(),
            last_sent: None,
        }
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
        self.status = "Connecting to Server     10".to_string();
        Ok(())
    }

    /// Host a game.
    pub fn host(
        &mut self,
        name: &str,
        password: Option<String>,
        speed: ConnectionSpeed,
    ) -> Result<(), String> {
        let config = HostConfig {
            bind: self.bind(self.host_port),
            session_name: if name.is_empty() {
                "Default Server".to_string()
            } else {
                name.to_string()
            },
            player_name: self.player_name.clone(),
            max_players: 4,
            password,
            link: self.link,
        };
        self.game = Some(NetGame::host(config, speed).map_err(|e| format!("host: {e}"))?);
        self.robot_of.clear();
        self.robot_of.insert(ra2net::session::HOST_ID, 0);
        Ok(())
    }

    pub fn leave(&mut self) {
        if let Some(game) = &mut self.game {
            game.close();
        }
        self.game = None;
        self.connecting = None;
        self.robot_of.clear();
        self.remote.clear();
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
                self.host(&name, password, speed)?;
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
                        let arena = if game.lobby.arena.is_empty() {
                            "Arenas/box.py".to_string()
                        } else {
                            game.lobby.arena.clone()
                        };
                        game.start_match(&arena);
                        game.set_schema(schema());
                        for (i, slot) in game.lobby.slots.iter().enumerate() {
                            self.robot_of.insert(slot.player, i);
                        }
                        return Ok(NetAction::StartMatch { arena });
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
        if let Some(game) = &mut self.game {
            for event in game.poll() {
                match event {
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
                        // The robots are the lobby's, in slot order.
                        self.robot_of = game
                            .lobby
                            .slots
                            .iter()
                            .enumerate()
                            .map(|(i, s)| (s.player, i))
                            .collect();
                        game.set_schema(schema());
                        game.report_loaded();
                        action = NetAction::StartMatch { arena };
                    }
                    GameEvent::RemoteInput { from, input, .. } => {
                        if let Some(&robot) = self.robot_of.get(&from) {
                            apply_input(self.remote.entry(robot).or_default(), &input);
                        }
                    }
                    _ => {}
                }
            }
            if let Some(start) = self.connecting {
                let left = 10u64.saturating_sub(start.elapsed().as_secs());
                self.status = format!("Connecting to Server     {left}");
            }
        }
        if let NetAction::Ended(_) = action {
            self.game = None;
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
                    format!(
                        "{}{}  {}/{}  {} ms",
                        if h.password { "* " } else { "" },
                        h.session_name,
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
        let Some(doc) = shell.document_mut() else {
            return;
        };
        set_text(doc, "arena name", &lobby.arena);
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
        for i in 0..4 {
            let panel = format!("player{} group", i + 1);
            let cell = match lobby.slots.get(i) {
                Some(slot) => active
                    .as_ref()
                    .and_then(|g| g.widgets().next().cloned())
                    .map(|mut cell| {
                        ra2ui::cells::bind_value(&mut cell, "team name", &slot.name);
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

    /// The match a lobby starts: the arena, one robot per slot in slot order
    /// (the slot's robot file when it names a design in the install, else a
    /// shipped AI robot), the local player's robot driven from the keyboard.
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
        for (i, slot) in net.lobby.slots.iter().take(4).enumerate() {
            let file = game.join(&slot.robot_file);
            let path = if !slot.robot_file.is_empty() && file.is_file() {
                file
            } else {
                game.join(FALLBACK[i])
            };
            entries.push(ra2sim::r#match::RobotEntry::ai(
                i,
                path.to_string_lossy().to_string(),
            ));
        }
        while entries.len() < 2 {
            let i = entries.len();
            entries.push(ra2sim::r#match::RobotEntry::ai(
                i,
                game.join(FALLBACK[i]).to_string_lossy().to_string(),
            ));
        }
        let arena_path = game.join(arena);
        let mut session = crate::match_mode::Session::exhibition(
            game,
            &arena_path,
            ra2sim::r#match::GameType::Deathmatch,
            entries,
            None,
            None,
        )?;
        let me = net.local().unwrap_or(ra2net::session::HOST_ID);
        let robot = net
            .lobby
            .slots
            .iter()
            .position(|s| s.player == me)
            .unwrap_or(0);
        session.player_robot = robot;
        session.matched.duel.human_robot = Some(robot);
        Ok(session)
    }

    /// In the match, before each fixed step: the host drives the remote
    /// humans' robots from their inputs; a joiner sends its own drive.
    pub fn before_step(&mut self, session: &mut crate::match_mode::Session) {
        let Some(game) = &mut self.game else { return };
        if game.is_host() {
            for (&robot, &drive) in &self.remote {
                session.set_remote_drive(robot, Some(drive));
            }
        } else {
            let drive = session.drive();
            // Inputs are not guaranteed (flags 0x20); resend a held drive
            // every quarter second so a lost one does not stick.
            let due = match self.last_sent {
                Some((last, at)) => {
                    (last.throttle, last.steer) != (drive.throttle, drive.steer)
                        || at.elapsed() >= Duration::from_millis(250)
                }
                None => true,
            };
            if due {
                let robot = self
                    .robot_of
                    .get(&game.local().unwrap_or_default())
                    .copied()
                    .unwrap_or(session.player_robot) as u32;
                for input in drive_inputs(drive) {
                    game.send_input(robot, input);
                }
                self.last_sent = Some((drive, Instant::now()));
            }
        }
    }

    /// After each fixed step: the host paces snapshots; a joiner applies the
    /// newest snapshot it has.
    pub fn after_step(&mut self, session: &mut crate::match_mode::Session, dt: f32) {
        let Some(game) = &mut self.game else { return };
        if game.is_host() {
            let duel = &session.matched.duel;
            game.tick_sync(dt, || capture(duel));
            for event in game.poll() {
                if let GameEvent::RemoteInput { from, input, .. } = event {
                    if let Some(&robot) = self.robot_of.get(&from) {
                        apply_input(self.remote.entry(robot).or_default(), &input);
                    }
                }
            }
        } else {
            let mut newest = None;
            for event in game.poll() {
                if let GameEvent::Snapshot(state) = event {
                    newest = Some(state);
                }
            }
            if let Some(state) = newest {
                apply(&mut session.matched.duel, &state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_become_percent_key_events_and_back() {
        let drive = DriveInput::new(1.0, -0.5);
        let mut back = DriveInput::default();
        for input in drive_inputs(drive) {
            apply_input(&mut back, &input);
        }
        assert_eq!((back.throttle, back.steer), (drive.throttle, drive.steer));
        assert_eq!(drive_inputs(drive)[1].value as i32, -50);
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
        assert_eq!(body_id(2, 5), 0x0002_0005);
    }
}
