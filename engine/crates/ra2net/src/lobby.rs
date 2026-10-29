//! The pre-match lobby (`prematch.gib`, `MultiplayerPrematchWnd`) as both ends
//! see it: one slot per player, kept in step with `giGroupInfoMessage`s, plus
//! the ready flags, the host's game settings and the chat.
//!
//! The original's slot kinds (`slot+0x54`, counted by `FUN_0047ba70` for the
//! enumeration reply): 0 and 1 are humans, 2 and 3 computer players; the odd
//! kinds have a robot loaded (`FUN_0047d120` draws the robot preview for 1 and
//! 3) and 4 is a slot whose player left. The mapping of kinds to "human" and
//! "computer" is inferred from the two counts the host reports (docs/38 §5).
//!
//! **Spectators** (docs/38 §10, inferred): `prematch.gib` has a `spectator
//! list` ("list of the spectators (joined but not in the match)"), and a new
//! peer plays `spectator_joined.wav` (event `0x64`, handler `0x492ef0`). A
//! peer without a player panel is such a spectator here: kind
//! [`kind::SPECTATOR`], no robot, no ready flag, no robot index in the match.
//! Every peer, the host included, arrives as one and takes a panel by
//! claiming it (live: nobody has a panel on arrival).

use crate::dpn::RaHostData;
use crate::message::{GroupInfo, Message};
use crate::session::PlayerId;

/// `GroupInfo::state` values.
pub mod kind {
    pub const HUMAN: u32 = 0;
    pub const HUMAN_WITH_ROBOT: u32 = 1;
    pub const COMPUTER: u32 = 2;
    pub const COMPUTER_WITH_ROBOT: u32 = 3;
    pub const LEFT: u32 = 4;
    /// Ours: a connected peer with no player panel, who watches the match.
    pub const SPECTATOR: u32 = 5;
}

/// The player panels of `prematch.gib` (`player1 group`..`player4 group`).
pub const PLAYER_SLOTS: usize = 4;
/// Spectators a host takes besides its four players (library default for
/// other game types; the original admits none, see [`session_max`]).
pub const MAX_SPECTATORS: usize = 4;
/// Peers a session admits: the players and their spectators (library default).
pub const SESSION_MAX: u32 = (PLAYER_SLOTS + MAX_SPECTATORS) as u32;

/// `dwMaxPlayers` the original's host (`FUN_0048eca0`) passes to `Host`
/// (`FUN_006268b0`): 2 for a DEATHMATCH, 4 for every other game type. DirectPlay
/// refuses a peer beyond it, spectators included (docs/38 section 16).
pub fn session_max(game_type: &str) -> u32 {
    if game_type == "DEATHMATCH" {
        2
    } else {
        4
    }
}

/// The `GroupInfo` of an eject as the original sends it (live, docs/38 section 16): the
/// panel `old` in state `LEFT` with its team and robot names, an empty logo, the sending end's
/// own player name, no ready flag, the human value and an empty design.
pub fn eject_info(old: &Slot, sender_name: &str) -> GroupInfo {
    GroupInfo {
        slot: old.index,
        owner: old.player,
        state: kind::LEFT,
        strings: [
            old.team.clone(),
            String::new(),
            old.robot.clone(),
            sender_name.to_string(),
        ],
        flag: false,
        value: u32::from(old.is_human()),
        // The design flag is set with nothing behind it (live: the message ends on `01`).
        robot: Some(Vec::new()),
    }
}

/// [`Slot::index`] of a spectator: no player panel.
pub const NO_PANEL: u32 = u32::MAX;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slot {
    /// The player panel (`GroupInfo::slot`), [`NO_PANEL`] for a spectator.
    pub index: u32,
    /// Who controls it (`GroupInfo::owner`).
    pub player: PlayerId,
    pub kind: u32,
    /// The owner's player name.
    pub name: String,
    pub team: String,
    /// The team logo (`Teams\orange_skull.bmp`).
    pub team_logo: String,
    pub robot: String,
    pub ready: bool,
    /// The robot design stream, when one is chosen.
    pub design: Option<Vec<u8>>,
}

impl Slot {
    pub fn is_spectator(&self) -> bool {
        self.kind == kind::SPECTATOR
    }

    pub fn has_robot(&self) -> bool {
        self.kind == kind::HUMAN_WITH_ROBOT || self.kind == kind::COMPUTER_WITH_ROBOT
    }

    pub fn is_human(&self) -> bool {
        self.kind == kind::HUMAN || self.kind == kind::HUMAN_WITH_ROBOT
    }

    /// The panel as the original writes it (field order from the live
    /// messages, docs/38 §14): panel, owner, state, team, team logo, robot,
    /// owner name, ready, 1 for a human panel, the design.
    pub fn to_group_info(&self) -> GroupInfo {
        GroupInfo {
            slot: self.index,
            owner: self.player,
            state: self.kind,
            strings: [
                self.team.clone(),
                self.team_logo.clone(),
                self.robot.clone(),
                self.name.clone(),
            ],
            flag: self.ready,
            value: self.is_human() as u32,
            robot: self.design.clone(),
        }
    }

    fn from_group_info(g: &GroupInfo) -> Slot {
        let [team, team_logo, robot, name] = g.strings.clone();
        Slot {
            index: g.slot,
            player: g.owner,
            kind: g.state,
            name,
            team,
            team_logo,
            robot,
            ready: g.flag,
            design: g.robot.clone(),
        }
    }
}

/// What changed when a message was applied.
#[derive(Debug, Clone, PartialEq)]
pub enum LobbyChange {
    Slots,
    Ready { player: PlayerId, ready: bool },
    Settings,
    Chat { from: PlayerId, text: String },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lobby {
    pub slots: Vec<Slot>,
    pub game_type: String,
    pub weight_class: String,
    /// The `ArenaPreview` text: `Arenas/<preview>:<name>` from the
    /// original (`Arenas/bridge/bridge_preview.bmp:Bridge of Doom`, live).
    pub arena: String,
    pub chat: Vec<(PlayerId, String)>,
    /// Everyone in the session, in join order (the prematch `Spectators:`
    /// list shows them all: the original host listed itself and the joiner
    /// while both also held panels, live).
    pub members: Vec<(PlayerId, String)>,
}

/// The arena name of an `ArenaPreview` text (after the last `:`), or the
/// text itself.
pub fn arena_name(preview: &str) -> &str {
    preview.rsplit_once(':').map_or(preview, |(_, name)| name)
}

impl Lobby {
    /// The player's slot: its human panel, else its spectator entry, else a
    /// computer panel it set up.
    pub fn slot(&self, player: PlayerId) -> Option<&Slot> {
        let i = self.slot_position(player)?;
        Some(&self.slots[i])
    }

    pub fn slot_mut(&mut self, player: PlayerId) -> Option<&mut Slot> {
        let i = self.slot_position(player)?;
        Some(&mut self.slots[i])
    }

    fn slot_position(&self, player: PlayerId) -> Option<usize> {
        let owned =
            |f: &dyn Fn(&Slot) -> bool| self.slots.iter().position(|s| s.player == player && f(s));
        owned(&|s| s.is_human())
            .or_else(|| owned(&|s| s.is_spectator()))
            .or_else(|| owned(&|_| true))
    }

    /// A panel's message without the picture (the ready flag toggling, 95
    /// bytes in the original) keeps the picture its owner sent before.
    fn with_kept_design(&self, mut slot: Slot) -> Slot {
        if slot.design.is_none() {
            if let Some(old) = self.panel(slot.index).filter(|o| o.player == slot.player) {
                slot.design = old.design.clone();
            }
        }
        slot
    }

    /// The panel at `index`.
    pub fn panel(&self, index: u32) -> Option<&Slot> {
        self.slots
            .iter()
            .find(|s| s.index == index && !s.is_spectator())
    }

    /// The lowest panel nobody holds.
    pub fn free_panel(&self) -> Option<u32> {
        (0..PLAYER_SLOTS as u32).find(|&i| self.panel(i).is_none())
    }

    /// Put `slot` in its panel (replacing what was there).
    pub fn set_panel(&mut self, slot: Slot) {
        self.put_panel(slot);
    }

    /// Put `slot` in its panel (replacing what was there), panels in index
    /// order before the spectators.
    fn put_panel(&mut self, slot: Slot) {
        self.slots
            .retain(|s| s.is_spectator() || s.index != slot.index);
        if slot.is_human() {
            // The owner is no longer waiting without a panel.
            self.slots
                .retain(|s| !(s.is_spectator() && s.player == slot.player));
        }
        let at = self
            .slots
            .iter()
            .position(|s| s.is_spectator() || s.index > slot.index)
            .unwrap_or(self.slots.len());
        self.slots.insert(at, slot);
    }

    /// A joiner's view of someone in the session: a member, and a spectator
    /// until the host's `GroupInfo` gives it a panel.
    pub fn add_watcher(&mut self, player: PlayerId, name: &str) {
        self.add_member(player, name);
        if self.slot(player).is_none() {
            self.slots.push(Slot {
                index: NO_PANEL,
                player,
                kind: kind::SPECTATOR,
                name: name.to_string(),
                ..Slot::default()
            });
        }
    }

    /// Someone is in the session (both ends; the host also gives a panel
    /// with [`Lobby::join`]).
    pub fn add_member(&mut self, player: PlayerId, name: &str) {
        match self.members.iter_mut().find(|(id, _)| *id == player) {
            Some(m) => m.1 = name.to_string(),
            None => self.members.push((player, name.to_string())),
        }
    }

    /// Someone joined the session (host side, on the session's player-joined).
    /// Nobody has a panel on arrival (the original's lobby lists the host and
    /// every peer under `Spectators:` until they take one, live): the peer is
    /// a spectator until [`Lobby::apply`] or the local claim gives it a panel.
    pub fn join(&mut self, player: PlayerId, name: &str) {
        self.add_watcher(player, name);
    }

    /// Take the panel at `index` away (an eject, a player leaving its panel).
    /// A human who is still in the session goes back to the spectators.
    pub fn remove_panel(&mut self, index: u32) -> Option<Slot> {
        let at = self
            .slots
            .iter()
            .position(|s| !s.is_spectator() && s.index == index)?;
        let old = self.slots.remove(at);
        if old.is_human()
            && self.members.iter().any(|(id, _)| *id == old.player)
            && !self
                .slots
                .iter()
                .any(|s| s.player == old.player && s.is_human())
            && !self
                .slots
                .iter()
                .any(|s| s.player == old.player && s.is_spectator())
        {
            self.slots.push(Slot {
                index: NO_PANEL,
                player: old.player,
                kind: kind::SPECTATOR,
                name: old.name.clone(),
                ..Slot::default()
            });
        }
        Some(old)
    }

    /// The slots with a player panel, in panel order (the robots of the
    /// match).
    pub fn players(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(|s| !s.is_spectator())
    }

    /// The panels that hold a robot, in panel order: the robots of the match.
    pub fn contestants(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(|s| s.has_robot())
    }

    /// The spectators, in the order they joined (`spectator list`).
    pub fn spectators(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(|s| s.is_spectator())
    }

    /// The match robot index of a player: its place among the panels that
    /// hold a robot (a human panel; a computer's belongs to the host);
    /// `None` for a spectator or a stranger.
    pub fn robot_index(&self, player: PlayerId) -> Option<usize> {
        self.contestants()
            .position(|s| s.player == player && s.is_human())
    }

    pub fn leave(&mut self, player: PlayerId) {
        self.slots.retain(|s| s.player != player);
        self.members.retain(|(id, _)| *id != player);
    }

    /// Every human with a robot is ready, and there are at least two robots.
    pub fn all_ready(&self) -> bool {
        let robots = self.slots.iter().filter(|s| s.has_robot()).count();
        robots >= 2
            && self
                .slots
                .iter()
                .filter(|s| s.kind == kind::HUMAN || s.kind == kind::HUMAN_WITH_ROBOT)
                .all(|s| s.ready && s.has_robot())
    }

    /// Every seated human (a panel with a robot) is ready: the original's test before it begins
    /// without asking (`FUN_00495450` over `FUN_0047ca10`, docs/38 section 16).
    pub fn all_seated_ready(&self) -> bool {
        self.slots
            .iter()
            .filter(|s| s.kind == kind::HUMAN_WITH_ROBOT)
            .all(|s| s.ready)
    }

    /// The enumeration reply's two words.
    pub fn host_data(&self) -> RaHostData {
        let count = |a: u32, b: u32| {
            self.slots
                .iter()
                .filter(|s| s.kind == a || s.kind == b)
                .count() as u32
        };
        RaHostData {
            slots_23: count(kind::COMPUTER, kind::COMPUTER_WITH_ROBOT),
            slots_01: count(kind::HUMAN, kind::HUMAN_WITH_ROBOT),
        }
    }

    /// The `giGroupInfoMessage`s that describe the panels (spectators have
    /// none: the session's player list names them).
    pub fn group_infos(&self) -> Vec<Message> {
        self.slots
            .iter()
            .filter(|s| !s.is_spectator())
            .map(|s| Message::GroupInfo(s.to_group_info()))
            .collect()
    }

    /// The host's settings as messages.
    pub fn settings(&self) -> Vec<Message> {
        vec![
            Message::ChangeGameType(self.game_type.clone()),
            Message::ChangeWeightClass(self.weight_class.clone()),
            Message::ArenaPreview(self.arena.clone()),
        ]
    }

    /// Apply a lobby message from `from`. A joiner may only describe its own
    /// slot; the host's word is final.
    pub fn apply(
        &mut self,
        from: PlayerId,
        host: PlayerId,
        message: &Message,
    ) -> Option<LobbyChange> {
        match message {
            Message::GroupInfo(g) => {
                if g.slot as usize >= PLAYER_SLOTS || g.state == kind::SPECTATOR {
                    return None;
                }
                if from != host && g.state == kind::LEFT {
                    // A joiner empties its own panel.
                    let own = self.panel(g.slot).is_some_and(|p| p.player == from);
                    if !own || g.owner != from {
                        return None;
                    }
                    self.remove_panel(g.slot);
                    return Some(LobbyChange::Slots);
                }
                if from != host {
                    // A joiner speaks for a panel of its own, as a human,
                    // and does not set its ready flag that way.
                    let human = g.state == kind::HUMAN || g.state == kind::HUMAN_WITH_ROBOT;
                    if g.owner != from || !human {
                        return None;
                    }
                    // The original's joiner takes a panel by sending its
                    // `GroupInfo` (to everyone, live): its own panel, a free
                    // one, or one still open (state 0 draws as OPEN on the
                    // original), and sets its ready flag the same way.
                    let displaced = match self.panel(g.slot) {
                        Some(old) if old.player == from => None,
                        Some(old) if old.kind == kind::HUMAN => Some(old.clone()),
                        None => None,
                        Some(_) => return None,
                    };
                    let had = self
                        .slots
                        .iter()
                        .position(|s| s.player == from && s.is_human() && s.index != g.slot);
                    if let Some(at) = had {
                        if self.slots[at].kind == kind::HUMAN_WITH_ROBOT {
                            return None;
                        }
                        self.slots.remove(at);
                    }
                    let slot = self.with_kept_design(Slot::from_group_info(g));
                    self.put_panel(slot);
                    if let Some(mut moved) = displaced {
                        // Whoever held the open panel waits for another.
                        match self.free_panel() {
                            Some(i) => {
                                moved.index = i;
                                self.put_panel(moved);
                            }
                            None => {
                                moved.index = NO_PANEL;
                                moved.kind = kind::SPECTATOR;
                                self.slots.push(moved);
                            }
                        }
                    }
                    return Some(LobbyChange::Slots);
                }
                if g.state == kind::LEFT {
                    self.remove_panel(g.slot);
                    return Some(LobbyChange::Slots);
                }
                let slot = self.with_kept_design(Slot::from_group_info(g));
                self.put_panel(slot);
                Some(LobbyChange::Slots)
            }
            Message::PlayerReady { player, ready } => {
                if from != host && *player != from {
                    return None;
                }
                let slot = self.slot_mut(*player)?;
                // Only a human panel with a robot has a ready flag.
                if !slot.is_human() || !slot.has_robot() {
                    return None;
                }
                slot.ready = *ready;
                Some(LobbyChange::Ready {
                    player: *player,
                    ready: *ready,
                })
            }
            Message::ChangeGameType(s) if from == host => {
                self.game_type = s.clone();
                Some(LobbyChange::Settings)
            }
            Message::ChangeWeightClass(s) if from == host => {
                self.weight_class = s.clone();
                Some(LobbyChange::Settings)
            }
            Message::ArenaPreview(s) if from == host => {
                self.arena = s.clone();
                Some(LobbyChange::Settings)
            }
            Message::Text(text) => {
                self.chat.push((from, text.clone()));
                Some(LobbyChange::Chat {
                    from,
                    text: text.clone(),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    /// The eject the original's host sent for the port joiner's panel (live,
    /// `mp-eject-host`: panel 1 of owner `0x10001`, flags `0x28`).
    #[test]
    fn an_eject_is_the_group_info_the_original_sent() {
        let old = Slot {
            index: 1,
            player: 0x10001,
            kind: kind::HUMAN_WITH_ROBOT,
            name: "PortJoiner".into(),
            team: "The Rookies".into(),
            team_logo: "Teams\\orange_skull.bmp".into(),
            robot: "Sting Ray".into(),
            ready: true,
            design: Some(vec![1, 2, 3]),
        };
        let bytes = Message::GroupInfo(eject_info(&old, "Default Player")).encode();
        let mut want = vec![0x71, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 0, 4, 0, 0, 0];
        for text in ["The Rookies", "", "Sting Ray", "Default Player"] {
            want.extend((text.len() as u32).to_le_bytes());
            want.extend(text.bytes());
        }
        want.extend([0, 1, 0, 0, 0, 1]);
        assert_eq!(bytes, want);
    }

    #[test]
    fn the_originals_hosts_admit_two_peers_in_a_deathmatch_and_four_otherwise() {
        assert_eq!(super::session_max("DEATHMATCH"), 2);
        assert_eq!(super::session_max("TEAM MATCH"), 4);
    }

    use super::*;

    fn panel(index: u32, player: PlayerId, kind: u32, name: &str) -> Slot {
        Slot {
            index,
            player,
            kind,
            name: name.into(),
            robot: "Razor".into(),
            ..Slot::default()
        }
    }

    fn claim(slot: u32, owner: PlayerId, ready: bool) -> Message {
        Message::GroupInfo(GroupInfo {
            slot,
            owner,
            state: kind::HUMAN_WITH_ROBOT,
            strings: [
                "The Rookies".into(),
                "Teams\\orange_skull.bmp".into(),
                "C.R.U.S.H.".into(),
                "Default Player".into(),
            ],
            flag: ready,
            value: 1,
            robot: None,
        })
    }

    #[test]
    fn group_infos_round_trip_a_lobby() {
        let mut host = Lobby::default();
        host.join(1, "Host");
        host.join(2, "Guest");
        let mut guest_panel = panel(1, 2, kind::HUMAN_WITH_ROBOT, "Guest");
        guest_panel.design = Some(vec![1, 2, 3]);
        host.set_panel(guest_panel);
        host.set_panel(panel(0, 1, kind::COMPUTER_WITH_ROBOT, "Host"));
        let mut guest = Lobby::default();
        guest.add_watcher(1, "Host");
        guest.add_watcher(2, "Guest");
        for m in host.group_infos() {
            let bytes = m.encode();
            let m = Message::decode(&bytes).unwrap();
            assert_eq!(guest.apply(1, 1, &m), Some(LobbyChange::Slots));
        }
        assert_eq!(guest.slots, host.slots);
        assert_eq!(
            host.host_data(),
            RaHostData {
                slots_23: 1,
                slots_01: 1
            }
        );
    }

    /// Nobody has a panel on arrival (the original's host lists itself and
    /// every peer under `Spectators:`); a claim takes one.
    #[test]
    fn peers_arrive_as_spectators_and_a_claim_takes_a_panel() {
        let mut lobby = Lobby::default();
        for (i, n) in ["A", "B", "C", "D", "E", "F"].iter().enumerate() {
            lobby.join(i as u32 + 1, n);
        }
        assert_eq!(lobby.players().count(), 0);
        assert_eq!(lobby.spectators().count(), 6);
        assert_eq!(lobby.members.len(), 6);
        // A spectator neither readies nor owns a robot.
        let ready = Message::PlayerReady {
            player: 5,
            ready: true,
        };
        assert_eq!(lobby.apply(5, 1, &ready), None);
        assert_eq!(lobby.robot_index(5), None);
        // E claims panel 2: it leaves the spectators.
        assert_eq!(
            lobby.apply(5, 1, &claim(2, 5, false)),
            Some(LobbyChange::Slots)
        );
        assert!(!lobby.slot(5).unwrap().is_spectator());
        assert_eq!(lobby.spectators().count(), 5);
        assert_eq!(lobby.robot_index(5), Some(0));
        // A panel with a robot is not taken by another.
        assert_eq!(lobby.apply(6, 1, &claim(2, 6, false)), None);
        assert!(lobby.slot(6).unwrap().is_spectator());
        // Ready needs the robot.
        assert!(lobby.apply(5, 1, &ready).is_some());
        assert_eq!(lobby.host_data().slots_01, 1);
        // The claimant empties its own panel (a joiner's eject).
        let left = Message::GroupInfo(GroupInfo {
            slot: 2,
            owner: 5,
            state: kind::LEFT,
            ..GroupInfo::default()
        });
        assert_eq!(lobby.apply(6, 1, &left), None, "not F's panel");
        assert_eq!(lobby.apply(5, 1, &left), Some(LobbyChange::Slots));
        assert!(lobby.slot(5).unwrap().is_spectator());
        assert_eq!(lobby.players().count(), 0);
        // The host's word removes any panel, a computer's too.
        lobby.set_panel(panel(3, 1, kind::COMPUTER_WITH_ROBOT, "A"));
        let left = Message::GroupInfo(GroupInfo {
            slot: 3,
            owner: 1,
            state: kind::LEFT,
            ..GroupInfo::default()
        });
        assert_eq!(lobby.apply(1, 1, &left), Some(LobbyChange::Slots));
        assert_eq!(lobby.players().count(), 0);
        assert_eq!(lobby.spectators().count(), 6);
    }

    /// The original's joiner takes the host's open panel 0 with its robot
    /// (live, docs/38 section 14): it gets the panel and its ready flag counts.
    #[test]
    fn an_original_joiner_claims_an_open_panel_with_its_group_info() {
        let mut lobby = Lobby::default();
        lobby.join(0x10001, "Host");
        lobby.join(0x10002, "Default Player");
        assert_eq!(
            lobby.apply(0x10002, 0x10001, &claim(0, 0x10002, false)),
            Some(LobbyChange::Slots)
        );
        let p0 = lobby.panel(0).unwrap();
        assert_eq!((p0.player, p0.robot.as_str()), (0x10002, "C.R.U.S.H."));
        // The host holds no panel.
        assert!(lobby.slot(0x10001).unwrap().is_spectator());
        assert_eq!(lobby.players().count(), 1);
        // Ready is the panel's flag.
        lobby.apply(0x10002, 0x10001, &claim(0, 0x10002, true));
        assert!(lobby.panel(0).unwrap().ready);
    }

    #[test]
    fn a_joiner_only_speaks_for_its_own_slot() {
        let mut lobby = Lobby::default();
        lobby.join(1, "Host");
        lobby.join(2, "Guest");
        lobby.set_panel(panel(0, 1, kind::HUMAN_WITH_ROBOT, "Host"));
        lobby.set_panel(panel(1, 2, kind::HUMAN_WITH_ROBOT, "Guest"));
        let forged = Message::PlayerReady {
            player: 1,
            ready: true,
        };
        assert_eq!(lobby.apply(2, 1, &forged), None);
        let own = Message::PlayerReady {
            player: 2,
            ready: true,
        };
        assert!(lobby.apply(2, 1, &own).is_some());
        assert!(lobby.slot(2).unwrap().ready);
        assert_eq!(
            lobby.apply(2, 1, &Message::ChangeGameType("x".into())),
            None
        );
    }

    #[test]
    fn ready_needs_two_robots_and_every_human() {
        let mut lobby = Lobby::default();
        lobby.join(1, "Host");
        lobby.join(2, "Guest");
        lobby.set_panel(panel(0, 1, kind::HUMAN_WITH_ROBOT, "Host"));
        assert!(!lobby.all_ready());
        lobby.set_panel(panel(1, 2, kind::HUMAN_WITH_ROBOT, "Guest"));
        assert!(!lobby.all_ready());
        lobby.slots[0].ready = true;
        assert!(!lobby.all_ready());
        lobby.slots[1].ready = true;
        assert!(lobby.all_ready());
        // A human who has not picked a robot holds the match back.
        lobby.set_panel(panel(2, 3, kind::HUMAN, "Third"));
        assert!(!lobby.all_ready());
    }
}
