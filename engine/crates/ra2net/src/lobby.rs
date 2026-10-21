//! The pre-match lobby (`prematch.gib`, `MultiplayerPrematchWnd`) as both ends
//! see it: one slot per player, kept in step with `giGroupInfoMessage`s, plus
//! the ready flags, the host's game settings and the chat.
//!
//! The original's slot kinds (`slot+0x54`, counted by `FUN_0047ba70` for the
//! enumeration reply): 0 and 1 are humans, 2 and 3 computer players; the odd
//! kinds have a robot loaded (`FUN_0047d120` draws the robot preview for 1 and
//! 3) and 4 is a slot whose player left. The mapping of kinds to "human" and
//! "computer" is inferred from the two counts the host reports (docs/38 §5).

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
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Slot {
    pub player: PlayerId,
    pub kind: u32,
    pub name: String,
    pub team: String,
    pub robot: String,
    pub robot_file: String,
    pub ready: bool,
    /// The robot design stream, when one is chosen.
    pub design: Option<Vec<u8>>,
}

impl Slot {
    pub fn has_robot(&self) -> bool {
        self.kind == kind::HUMAN_WITH_ROBOT || self.kind == kind::COMPUTER_WITH_ROBOT
    }

    pub fn to_group_info(&self, index: usize) -> GroupInfo {
        GroupInfo {
            player: self.player,
            group: index as u32,
            state: self.kind,
            strings: [
                self.name.clone(),
                self.team.clone(),
                self.robot.clone(),
                self.robot_file.clone(),
            ],
            flag: self.ready,
            value: 0,
            robot: self.design.clone(),
        }
    }

    fn from_group_info(g: &GroupInfo) -> Slot {
        let [name, team, robot, robot_file] = g.strings.clone();
        Slot {
            player: g.player,
            kind: g.state,
            name,
            team,
            robot,
            robot_file,
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
    pub arena: String,
    pub chat: Vec<(PlayerId, String)>,
}

impl Lobby {
    pub fn slot(&self, player: PlayerId) -> Option<&Slot> {
        self.slots.iter().find(|s| s.player == player)
    }

    pub fn slot_mut(&mut self, player: PlayerId) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| s.player == player)
    }

    /// A human joined (host side, on the session's player-joined).
    pub fn join(&mut self, player: PlayerId, name: &str) {
        if self.slot(player).is_none() {
            self.slots.push(Slot {
                player,
                kind: kind::HUMAN,
                name: name.to_string(),
                ..Slot::default()
            });
        }
    }

    pub fn leave(&mut self, player: PlayerId) {
        self.slots.retain(|s| s.player != player);
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

    /// The `giGroupInfoMessage`s that describe the whole lobby.
    pub fn group_infos(&self) -> Vec<Message> {
        self.slots
            .iter()
            .enumerate()
            .map(|(i, s)| Message::GroupInfo(s.to_group_info(i)))
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
                if from != host && g.player != from {
                    return None;
                }
                if g.state == kind::LEFT {
                    self.leave(g.player);
                    return Some(LobbyChange::Slots);
                }
                let mut slot = Slot::from_group_info(g);
                match self.slot_mut(g.player) {
                    Some(old) => {
                        if from != host {
                            // A joiner does not set its own ready flag by slot.
                            slot.ready = old.ready;
                        }
                        *old = slot;
                    }
                    None => {
                        let at = (g.group as usize).min(self.slots.len());
                        self.slots.insert(at, slot);
                    }
                }
                Some(LobbyChange::Slots)
            }
            Message::PlayerReady { player, ready } => {
                if from != host && *player != from {
                    return None;
                }
                let slot = self.slot_mut(*player)?;
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
    use super::*;

    #[test]
    fn group_infos_round_trip_a_lobby() {
        let mut host = Lobby::default();
        host.join(1, "Host");
        host.join(2, "Guest");
        host.slots[1].kind = kind::HUMAN_WITH_ROBOT;
        host.slots[1].robot = "Razor".into();
        host.slots[1].design = Some(vec![1, 2, 3]);
        let mut guest = Lobby::default();
        for m in host.group_infos() {
            let bytes = m.encode();
            let m = Message::decode(&bytes).unwrap();
            assert_eq!(guest.apply(1, 1, &m), Some(LobbyChange::Slots));
        }
        assert_eq!(guest.slots, host.slots);
        assert_eq!(
            host.host_data(),
            RaHostData {
                slots_23: 0,
                slots_01: 2
            }
        );
    }

    #[test]
    fn a_joiner_only_speaks_for_its_own_slot() {
        let mut lobby = Lobby::default();
        lobby.join(1, "Host");
        lobby.join(2, "Guest");
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
        for s in &mut lobby.slots {
            s.kind = kind::HUMAN_WITH_ROBOT;
        }
        assert!(!lobby.all_ready());
        lobby.slots[0].ready = true;
        lobby.slots[1].ready = true;
        assert!(lobby.all_ready());
    }
}
