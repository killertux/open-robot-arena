//! The control board: how a robot's wiring turns key presses and AI inputs into
//! commands for its motors, pistons and burst devices.
//!
//! # What the original does (recovered from `Robot Arena 2.exe`)
//!
//! * A `.bot` lists *controls* (`Name: Forward`, a grid cell, a type: 1 analog,
//!   2 switch, 4 button, and one DirectInput scan-code row per binding slot) and
//!   *wires* (`component`, `action`, `x y slot`): the wire names the control by
//!   its grid cell and the slot of that control that drives it. An analog
//!   `Forward` has two slots, `W` (slot 0) and `S` (slot 1).
//! * Each component class registers its own actions with a handler object
//!   (`FUN_004ecdf0` for `ServoMotor`, `FUN_004f5120` for `SpinMotor`): the
//!   analog `Spin Clockwise`/`Spin Counter-clockwise` carry a scale of `+1`/`-1`,
//!   the button `On ...` and switch `On/Off ...` ones `+100`/`-100`.
//! * The handler (`0x4ed2f0`) keeps the list of inputs that are active, and every
//!   change re-sums them: `command = clamp(sum, -100, 100)` (`0x4ed370`, `0x4ed860`
//!   is the sum, `0x4599b0` the clamp). The motor stores the result as its speed
//!   percentage (`motor+0x7c`, `0x48ead0`); the motor step (`0x481450`) then drives
//!   at `percent * 0.01 * speed`, and at `0` releases the motor (or locks it, if
//!   its auto-lock flag is set).
//! * The AI's `Input(name, slot, value)` (`plus.AI` method at `0x4012f0` ->
//!   `0x4dd5f0` -> `0x508ce0`) walks every control of the bot, matches it by
//!   *name* and slot, stores `value` and delivers it as a press (non-zero) or
//!   release (zero). It is a per-call event, not just a level: a burst device
//!   fires on every non-zero call.
//!
//! # What is ours
//!
//! The keyboard path (DirectInput scan code -> slot press) is as above. How a
//! switch latches on a key press (toggle) is inferred from its name; how fast a
//! burst motor re-cocks is not in the binary's wiring code and is a guess
//! (see [`COCK_FRACTION`]).

use std::collections::BTreeMap;

use ra2fmt::bot::Bot;

/// The percentage a button or switch contributes, and the clamp of the sum.
pub const FULL: f32 = 100.0;

/// How fast a burst device cocks again after firing, as a fraction of its rated
/// speed. The tutorial says "slowly, but powerfully"; the number is a guess.
pub const COCK_FRACTION: f32 = 0.15;

/// What a control is (`position[2]` of the `.bot` row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Type 1: a stick or key pair; the value is the signed input.
    Analog,
    /// Type 2: latches on and off.
    Switch,
    /// Type 4: active while held.
    Button,
}

impl Kind {
    fn from_code(code: i32) -> Option<Kind> {
        match code {
            1 => Some(Kind::Analog),
            2 => Some(Kind::Switch),
            4 => Some(Kind::Button),
            _ => None,
        }
    }
}

/// What a wire does to its component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Adds `sign` times the control's value (analog) or `sign * 100` (held
    /// button, latched-on switch) to the component's speed command:
    /// `Spin Clockwise`, `On Clockwise`, `On/Off Counter-clockwise`, `Extend`,
    /// `Retract`.
    Drive(i8),
    /// `Fire`: triggers a burst motor or burst piston.
    Fire,
    /// Adds `sign` times the control's value to a `Steering` unit's steer
    /// command (`Left` `+1`, `Right` `-1`).
    Steer(i8),
}

impl Action {
    /// The action behind a `.bot` wire's name, if this engine knows it.
    pub fn parse(name: &str) -> Option<Action> {
        let n = name.trim();
        Some(match n {
            "Spin Clockwise" | "On Clockwise" | "On/Off Clockwise" | "Extend" => Action::Drive(1),
            "Spin Counter-clockwise"
            | "On Counter-clockwise"
            | "On/Off Counter-clockwise"
            | "Retract" => Action::Drive(-1),
            "Fire" => Action::Fire,
            _ => return None,
        })
    }

    /// The action behind a wire on a component of class `class`: the
    /// `Steering` unit registers its own four (`FUN_004f5fe0`..`0x4f6167`):
    /// `Forward` `-1` and `Backward` `+1` on its drive handler (`0x4f6600` ->
    /// `0x4f6720`), `Left` `+1` and `Right` `-1` on its steer handler
    /// (`0x4f6680` -> `0x4f6790`). (The `Hovercraft` class registers the same
    /// names with other signs; no stock robot has one and it is not modelled.)
    pub fn parse_for(class: &str, name: &str) -> Option<Action> {
        if class.trim().eq_ignore_ascii_case("Steering") {
            match name.trim() {
                "Forward" => return Some(Action::Drive(-1)),
                "Backward" => return Some(Action::Drive(1)),
                "Left" => return Some(Action::Steer(1)),
                "Right" => return Some(Action::Steer(-1)),
                _ => {}
            }
        }
        Action::parse(name)
    }
}

/// One control on the board.
#[derive(Clone, Debug)]
pub struct Control {
    pub name: String,
    pub kind: Kind,
    /// The scan code of each slot, `0` for none.
    pub keys: Vec<i32>,
}

/// One wire: control `controller`'s `slot` acts on `component`.
#[derive(Clone, Copy, Debug)]
pub struct Wire {
    pub component: usize,
    pub action: Action,
    pub controller: usize,
    pub slot: usize,
}

/// A bot's wiring.
#[derive(Clone, Debug, Default)]
pub struct Wiring {
    pub controls: Vec<Control>,
    pub wires: Vec<Wire>,
}

impl Wiring {
    /// Read the control board out of a parsed `.bot`.
    pub fn from_bot(bot: &Bot) -> Wiring {
        let mut controls = Vec::new();
        let mut cells = Vec::new();
        for c in &bot.controllers {
            let Some(kind) = Kind::from_code(c.position[2]) else {
                continue;
            };
            controls.push(Control {
                name: c.name.trim().to_string(),
                kind,
                // 1.07 files hold the code in the second number, later ones in
                // the third.
                keys: c
                    .bindings
                    .iter()
                    .map(|b| if b[2] != 0 { b[2] } else { b[1] })
                    .collect(),
            });
            cells.push([c.position[0], c.position[1]]);
        }
        let mut wires = Vec::new();
        for b in &bot.bindings {
            let class = usize::try_from(b.component)
                .ok()
                .and_then(|i| bot.components.get(i))
                .map(|c| c.class.as_str())
                .unwrap_or("");
            let Some(action) = Action::parse_for(class, &b.action) else {
                continue;
            };
            let Some(controller) = cells
                .iter()
                .position(|c| *c == [b.control[0], b.control[1]])
            else {
                continue;
            };
            if b.component < 0 || b.component as usize >= bot.components.len() {
                continue;
            }
            wires.push(Wire {
                component: b.component as usize,
                action,
                controller,
                slot: b.control[2].max(0) as usize,
            });
        }
        Wiring { controls, wires }
    }

    /// Whether any wire drives or fires `component`.
    pub fn is_wired(&self, component: usize) -> bool {
        self.wires.iter().any(|w| w.component == component)
    }

    fn controls_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.controls
            .iter()
            .enumerate()
            .filter(move |(_, c)| c.name == name)
            .map(|(i, _)| i)
    }
}

/// A set of held DirectInput scan codes (`0..256`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeySet([u64; 4]);

impl KeySet {
    pub fn set(&mut self, code: i32, down: bool) {
        if !(0..256).contains(&code) {
            return;
        }
        let (w, b) = (code as usize / 64, code as usize % 64);
        if down {
            self.0[w] |= 1 << b;
        } else {
            self.0[w] &= !(1 << b);
        }
    }

    pub fn contains(&self, code: i32) -> bool {
        (0..256).contains(&code) && self.0[code as usize / 64] >> (code as usize % 64) & 1 == 1
    }

    pub fn is_empty(&self) -> bool {
        self.0 == [0; 4]
    }
}

/// DirectInput scan codes of the keys the stock wiring uses, for callers that
/// name keys rather than number them.
pub mod dik {
    pub const W: i32 = 17;
    pub const A: i32 = 30;
    pub const S: i32 = 31;
    pub const D: i32 = 32;
    pub const SPACE: i32 = 57;
}

/// The live state of a robot's control board.
#[derive(Clone, Debug, Default)]
pub struct ControlState {
    /// The value each `(control, slot)` holds, `-100..=100`.
    held: BTreeMap<(usize, usize), f32>,
    /// Switches that are on.
    latched: BTreeMap<(usize, usize), bool>,
    /// Slots pressed since the last [`ControlState::end_step`].
    pressed: Vec<(usize, usize)>,
    keys: KeySet,
    synth: bool,
}

impl ControlState {
    /// `Input(name, slot, value)`: set the slot of every control called `name`.
    ///
    /// A non-zero value is a press (it fires burst devices every time, as the
    /// original delivers each call as an event); zero is a release. A switch is
    /// on while its last value is non-zero.
    pub fn input(&mut self, wiring: &Wiring, name: &str, slot: i32, value: f32) {
        let slot = slot.max(0) as usize;
        for c in wiring.controls_named(name) {
            self.held.insert((c, slot), value.clamp(-FULL, FULL));
            if wiring.controls[c].kind == Kind::Switch {
                self.latched.insert((c, slot), value != 0.0);
            }
            if value != 0.0 {
                self.pressed.push((c, slot));
            }
        }
    }

    /// The keys held now; presses and releases since the last call act on the
    /// slots bound to them (a switch toggles on a press).
    pub fn set_keys(&mut self, wiring: &Wiring, keys: KeySet) {
        if keys == self.keys {
            return;
        }
        for (c, control) in wiring.controls.iter().enumerate() {
            for (slot, &code) in control.keys.iter().enumerate() {
                if code == 0 {
                    continue;
                }
                let (was, now) = (self.keys.contains(code), keys.contains(code));
                if was == now {
                    continue;
                }
                self.held.insert((c, slot), if now { FULL } else { 0.0 });
                if now {
                    self.pressed.push((c, slot));
                    if control.kind == Kind::Switch {
                        let on = self.latched.entry((c, slot)).or_insert(false);
                        *on = !*on;
                    }
                }
            }
        }
        self.keys = keys;
    }

    /// A stand-in driver's `throttle`/`steer` (`-1..=1`), as the `Forward` and
    /// `LeftRight` keys a human would hold: forward is slot 0 (`W`), reverse
    /// slot 1 (`S`), left slot 0 (`A`), right slot 1 (`D`).
    pub fn set_drive(&mut self, wiring: &Wiring, throttle: f32, steer: f32) {
        let active = throttle != 0.0 || steer != 0.0;
        if !active && !self.synth {
            return;
        }
        self.synth = active;
        let (t, s) = (throttle.clamp(-1.0, 1.0), steer.clamp(-1.0, 1.0));
        for (name, a, b) in [
            ("Forward", t.max(0.0), (-t).max(0.0)),
            ("LeftRight", (-s).max(0.0), s.max(0.0)),
        ] {
            for c in wiring.controls_named(name) {
                self.held.insert((c, 0), a * FULL);
                self.held.insert((c, 1), b * FULL);
            }
        }
    }

    /// The speed command of `component` in `-100..=100`: the clamped sum of
    /// what its wires contribute; `None` when nothing drives it.
    pub fn command(&self, wiring: &Wiring, component: usize) -> Option<f32> {
        self.sum(wiring, component, |a| match a {
            Action::Drive(sign) => Some(sign),
            _ => None,
        })
    }

    /// A `Steering` unit's steer command in `-100..=100` (its `Left`/`Right`
    /// wires, summed and clamped like a drive command); `None` when nothing
    /// steers it.
    pub fn steer_command(&self, wiring: &Wiring, component: usize) -> Option<f32> {
        self.sum(wiring, component, |a| match a {
            Action::Steer(sign) => Some(sign),
            _ => None,
        })
    }

    fn sum(
        &self,
        wiring: &Wiring,
        component: usize,
        sign_of: impl Fn(Action) -> Option<i8>,
    ) -> Option<f32> {
        let mut sum = 0.0;
        let mut any = false;
        for w in wiring.wires.iter().filter(|w| w.component == component) {
            let Some(sign) = sign_of(w.action) else {
                continue;
            };
            any = true;
            let key = (w.controller, w.slot);
            let value = self.held.get(&key).copied().unwrap_or(0.0);
            sum += f32::from(sign)
                * match wiring.controls[w.controller].kind {
                    Kind::Analog => value,
                    Kind::Button => {
                        if value != 0.0 {
                            FULL
                        } else {
                            0.0
                        }
                    }
                    Kind::Switch => {
                        if self.latched.get(&key).copied().unwrap_or(false) {
                            FULL
                        } else {
                            0.0
                        }
                    }
                };
        }
        any.then(|| sum.clamp(-FULL, FULL))
    }

    /// Whether a `Fire` wire of `component` was pressed since the last
    /// [`ControlState::end_step`]. Every wire sharing the slot sees the press.
    pub fn fired(&self, wiring: &Wiring, component: usize) -> bool {
        wiring.wires.iter().any(|w| {
            w.component == component
                && w.action == Action::Fire
                && self.pressed.contains(&(w.controller, w.slot))
        })
    }

    /// Forget this step's presses (called once the step's joints are driven).
    pub fn end_step(&mut self) {
        self.pressed.clear();
    }

    /// The value a slot of the control called `name` holds (`GetInputStatus`).
    pub fn status(&self, wiring: &Wiring, name: &str, slot: i32) -> f32 {
        wiring
            .controls_named(name)
            .next()
            .and_then(|c| self.held.get(&(c, slot.max(0) as usize)))
            .copied()
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Wiring {
        let ctl = |name: &str, kind, keys: &[i32]| Control {
            name: name.into(),
            kind,
            keys: keys.to_vec(),
        };
        Wiring {
            controls: vec![
                ctl("Forward", Kind::Analog, &[dik::W, dik::S]),
                ctl("Spin", Kind::Switch, &[dik::SPACE]),
                ctl("Fire", Kind::Button, &[dik::SPACE + 1]),
            ],
            wires: vec![
                Wire {
                    component: 4,
                    action: Action::Drive(1),
                    controller: 0,
                    slot: 0,
                },
                Wire {
                    component: 4,
                    action: Action::Drive(-1),
                    controller: 0,
                    slot: 1,
                },
                Wire {
                    component: 7,
                    action: Action::Drive(1),
                    controller: 1,
                    slot: 0,
                },
                Wire {
                    component: 9,
                    action: Action::Fire,
                    controller: 2,
                    slot: 0,
                },
            ],
        }
    }

    #[test]
    fn a_steering_unit_registers_its_own_four_actions() {
        // `0x4f5fe0..0x4f6167`: Forward -1, Backward +1, Left +1, Right -1.
        assert_eq!(
            Action::parse_for("Steering", "Forward"),
            Some(Action::Drive(-1))
        );
        assert_eq!(
            Action::parse_for("Steering", "Backward"),
            Some(Action::Drive(1))
        );
        assert_eq!(
            Action::parse_for("Steering", "Left"),
            Some(Action::Steer(1))
        );
        assert_eq!(
            Action::parse_for("Steering", "Right"),
            Some(Action::Steer(-1))
        );
        assert_eq!(Action::parse_for("SpinMotor", "Forward"), None);
        let w = Wiring {
            controls: vec![Control {
                name: "LeftRight".into(),
                kind: Kind::Analog,
                keys: vec![dik::A, dik::D],
            }],
            wires: vec![
                Wire {
                    component: 7,
                    action: Action::Steer(1),
                    controller: 0,
                    slot: 0,
                },
                Wire {
                    component: 7,
                    action: Action::Steer(-1),
                    controller: 0,
                    slot: 1,
                },
            ],
        };
        let mut s = ControlState::default();
        s.input(&w, "LeftRight", 1, 100.0);
        assert_eq!(s.steer_command(&w, 7), Some(-100.0));
        assert_eq!(s.command(&w, 7), None, "steering wires are not a drive");
    }

    #[test]
    fn the_keys_drive_the_wires_and_the_sum_is_clamped() {
        let w = board();
        let mut s = ControlState::default();
        assert_eq!(s.command(&w, 4), Some(0.0));
        assert_eq!(s.command(&w, 5), None);
        let mut keys = KeySet::default();
        keys.set(dik::W, true);
        s.set_keys(&w, keys);
        assert_eq!(s.command(&w, 4), Some(100.0));
        keys.set(dik::W, false);
        keys.set(dik::S, true);
        s.set_keys(&w, keys);
        assert_eq!(s.command(&w, 4), Some(-100.0));
        // The AI's signed analog value passes straight through, clamped.
        s.input(&w, "Forward", 0, 40.0);
        s.input(&w, "Forward", 1, 0.0);
        assert_eq!(s.command(&w, 4), Some(40.0));
        s.input(&w, "Forward", 0, 100.0);
        s.input(&w, "Forward", 1, -100.0);
        assert_eq!(s.command(&w, 4), Some(100.0), "100 + 100 clamps");
    }

    #[test]
    fn a_switch_latches_and_a_press_fires_once() {
        let w = board();
        let mut s = ControlState::default();
        let mut keys = KeySet::default();
        keys.set(dik::SPACE, true);
        s.set_keys(&w, keys);
        keys.set(dik::SPACE, false);
        s.set_keys(&w, keys);
        assert_eq!(s.command(&w, 7), Some(100.0), "on after a tap");
        keys.set(dik::SPACE, true);
        s.set_keys(&w, keys);
        assert_eq!(s.command(&w, 7), Some(0.0), "off after the second tap");
        // The AI sets the switch directly.
        s.input(&w, "Spin", 0, 1.0);
        assert_eq!(s.command(&w, 7), Some(100.0));
        s.input(&w, "Spin", 0, 0.0);
        assert_eq!(s.command(&w, 7), Some(0.0));
        // Every non-zero call is a trigger; a level alone is not.
        assert!(!s.fired(&w, 9));
        s.input(&w, "Fire", 0, 1.0);
        assert!(s.fired(&w, 9));
        s.end_step();
        assert!(!s.fired(&w, 9), "a press lasts one step");
    }

    #[test]
    fn a_stand_in_driver_holds_the_forward_keys() {
        let w = board();
        let mut s = ControlState::default();
        s.set_drive(&w, 0.5, 0.0);
        assert_eq!(s.command(&w, 4), Some(50.0));
        s.set_drive(&w, -1.0, 0.0);
        assert_eq!(s.command(&w, 4), Some(-100.0));
        s.set_drive(&w, 0.0, 0.0);
        assert_eq!(s.command(&w, 4), Some(0.0));
    }

    #[test]
    fn stock_actions_parse() {
        assert_eq!(Action::parse("Spin Clockwise"), Some(Action::Drive(1)));
        assert_eq!(
            Action::parse("On/Off Counter-clockwise"),
            Some(Action::Drive(-1))
        );
        assert_eq!(Action::parse("Fire"), Some(Action::Fire));
        assert_eq!(Action::parse("Forward"), None);
    }
}
