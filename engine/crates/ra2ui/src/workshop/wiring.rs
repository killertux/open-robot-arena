//! The control board: the wiring half of the workshop.
//!
//! A `.bot` ends with two sections, and they are the ground truth here:
//!
//! ```text
//! <controllers>
//! Name: Forward
//! 1 0 1              ← cell x, cell y, control kind
//! 2                  ← number of key rows
//! 0 0 17 0           ← DirectInput scan code 17 = W
//! 0 0 31 0           ← 31 = S
//! <bindings>
//! 4                  ← component index
//! Spin Clockwise     ← the action
//! 1 0 0              ← controller cell x, cell y, binding slot
//! ```
//!
//! The third number of a controller is its kind: **1 analog, 2 switch, 4
//! button**. The pairing of a kind with what it can drive is not invented: the
//! game's own RTTI names the template classes, and they are the complete matrix
//! (`?AV?$Analog@VSpinMotor@@@Bot@@`, `?AV?$Button@VBurstMotor@@@Bot@@`, …):
//!
//! - analog + spin motor — two inputs, clockwise and counter-clockwise;
//! - switch + spin motor — on/off clockwise or counter-clockwise;
//! - button + spin motor — on while held, clockwise or counter-clockwise;
//! - button + burst motor / burst piston / cannon — `Fire`;
//! - analog or button + servo motor — the two spin directions;
//! - analog or button + servo piston — `Extend` / `Retract`;
//! - analog + steering — `Forward`/`Backward` (slot 0), `Left`/`Right` (slot 1).
//!
//! A **weapon is never bound directly**. It hangs off a motor, and every one of
//! the 94 stock weapons is driven by wiring its motor; a `Fire` row names a
//! `BurstMotor` (25) or a `BurstPiston` (6). Likewise a wheel is driven through
//! its spin motor, not its own row.

use ra2fmt::bot::{Binding, Bot, Component, Controller};

/// What a controller slot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlKind {
    /// Two inputs: positive and negative (type code `1`).
    Analog,
    /// Toggles power on and off (type code `2`).
    Switch,
    /// On while held (type code `4`).
    Button,
}

impl ControlKind {
    pub fn code(self) -> i32 {
        match self {
            ControlKind::Analog => 1,
            ControlKind::Switch => 2,
            ControlKind::Button => 4,
        }
    }

    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(ControlKind::Analog),
            2 => Some(ControlKind::Switch),
            4 => Some(ControlKind::Button),
            _ => None,
        }
    }

    /// How many binding slots the kind offers.
    pub fn slots(self) -> usize {
        match self {
            ControlKind::Analog => 2,
            ControlKind::Switch | ControlKind::Button => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ControlKind::Analog => "Analog",
            ControlKind::Switch => "Switch",
            ControlKind::Button => "Button",
        }
    }
}

/// The action names that ship in a `.bot` binding, exactly as the file spells
/// them. These are what [`Action::parse`] accepts and what an error quotes.
pub const SHIPPED_ACTION_NAMES: &[&str] = &[
    "Backward",
    "Fire",
    "Forward",
    "Left",
    "On/Off Clockwise",
    "On/Off Counter-clockwise",
    "Right",
    "Spin Clockwise",
    "Spin Counter-clockwise",
];

/// The actions the binary's control templates define, including the handful no
/// stock bot uses.
pub const ACTION_NAMES: &[&str] = &[
    "Attract On/Off",
    "Backward",
    "Extend",
    "Fire",
    "Forward",
    "Left",
    "On Clockwise",
    "On Counter-clockwise",
    "On/Off Clockwise",
    "On/Off Counter-clockwise",
    "Repel On/Off",
    "Retract",
    "Right",
    "Spin Clockwise",
    "Spin Counter-clockwise",
];

/// One wiring action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    SpinClockwise,
    SpinCounterClockwise,
    OnOffClockwise,
    OnOffCounterClockwise,
    OnClockwise,
    OnCounterClockwise,
    Fire,
    Forward,
    Backward,
    Left,
    Right,
    Extend,
    Retract,
    RepelOnOff,
    AttractOnOff,
}

impl Action {
    /// The exact string the `.bot` stores.
    pub fn name(self) -> &'static str {
        match self {
            Action::SpinClockwise => "Spin Clockwise",
            Action::SpinCounterClockwise => "Spin Counter-clockwise",
            Action::OnOffClockwise => "On/Off Clockwise",
            Action::OnOffCounterClockwise => "On/Off Counter-clockwise",
            Action::OnClockwise => "On Clockwise",
            Action::OnCounterClockwise => "On Counter-clockwise",
            Action::Fire => "Fire",
            Action::Forward => "Forward",
            Action::Backward => "Backward",
            Action::Left => "Left",
            Action::Right => "Right",
            Action::Extend => "Extend",
            Action::Retract => "Retract",
            Action::RepelOnOff => "Repel On/Off",
            Action::AttractOnOff => "Attract On/Off",
        }
    }

    /// Parse the file's spelling; the error carries the list of names the game
    /// defines.
    pub fn parse(name: &str) -> Result<Action, WiringError> {
        let name = name.trim();
        ACTION_NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map(|i| {
                [
                    Action::AttractOnOff,
                    Action::Backward,
                    Action::Extend,
                    Action::Fire,
                    Action::Forward,
                    Action::Left,
                    Action::OnClockwise,
                    Action::OnCounterClockwise,
                    Action::OnOffClockwise,
                    Action::OnOffCounterClockwise,
                    Action::RepelOnOff,
                    Action::Retract,
                    Action::Right,
                    Action::SpinClockwise,
                    Action::SpinCounterClockwise,
                ][i]
            })
            .ok_or_else(|| WiringError::UnknownAction {
                name: name.to_string(),
                known: ACTION_NAMES,
            })
    }
}

/// The control names that ship across the 57 stock bots. Player names are free
/// text; these are the ones the corpus uses (the AI talks to `Forward` and
/// `LeftRight` by name — `AI/__init__.py`'s `Input('Forward', 0, …)`).
pub const SHIPPED_CONTROL_NAMES: &[&str] = &[
    "Fire",
    "Flip",
    "Forward",
    "ForwardBack",
    "JabLeft",
    "JabRight",
    "LeftPaw",
    "LeftRight",
    "Lefthook",
    "New Analog",
    "New Button",
    "RightPaw",
    "Righthook",
    "Smash",
    "Spin",
    "TurboForward",
    "UpDown",
    "spin",
    "twirl",
];

/// The names the shipped AI scripts drive the board with.
pub const AI_CONTROL_NAMES: &[&str] = &["Forward", "LeftRight"];

/// One control on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub name: String,
    /// Grid cell: column, row.
    pub cell: [i32; 2],
    pub kind: ControlKind,
    /// DirectInput scan codes, one row per binding slot. 1.07 writes three
    /// numbers, newer files four; the reader pads 1.07 with a zero.
    pub keys: Vec<[i32; 4]>,
}

impl Control {
    pub fn new(name: impl Into<String>, cell: [i32; 2], kind: ControlKind) -> Self {
        Control {
            name: name.into(),
            cell,
            kind,
            keys: Vec::new(),
        }
    }

    pub fn with_keys(mut self, keys: Vec<[i32; 4]>) -> Self {
        self.keys = keys;
        self
    }

    /// The DirectInput code of a binding row: 1.07/1.08 files write three
    /// numbers (`0 57 0`, the code second), newer ones four (code third) — the
    /// same rule the simulator's wiring reader applies.
    pub fn key_code(row: &[i32; 4]) -> i32 {
        if row[2] != 0 {
            row[2]
        } else {
            row[1]
        }
    }

    /// `"W - S"`-style input label, as `botlab/control_cell.gib` shows it: the
    /// keys joined with `" - "` (the string at `0x6f1cc0` that the exe's
    /// control-cell filler `FUN_004451b0` puts between them).
    pub fn key_label(&self) -> String {
        self.keys
            .iter()
            .map(|row| dik_name(Self::key_code(row)))
            .collect::<Vec<_>>()
            .join(" - ")
    }

    fn from_controller(c: &Controller) -> Result<Self, WiringError> {
        let kind =
            ControlKind::from_code(c.position[2]).ok_or(WiringError::InvalidControlCode {
                code: c.position[2],
            })?;
        let control = Control {
            name: c.name.clone(),
            cell: [c.position[0], c.position[1]],
            kind,
            keys: c.bindings.clone(),
        };
        control.check()?;
        Ok(control)
    }

    fn to_controller(&self) -> Controller {
        Controller {
            name: self.name.clone(),
            position: [self.cell[0], self.cell[1], self.kind.code()],
            bindings: self.keys.clone(),
        }
    }

    fn check(&self) -> Result<(), WiringError> {
        validate_name(&self.name)?;
        if self.keys.len() > self.kind.slots() {
            return Err(WiringError::TooManyKeys {
                kind: self.kind,
                got: self.keys.len(),
                slots: self.kind.slots(),
            });
        }
        Ok(())
    }
}

/// Validate a player-given control name: the `.bot` writes it after `Name: ` on
/// one line, so it cannot be empty or contain a line break, and it must not
/// already carry the label.
pub fn validate_name(name: &str) -> Result<(), WiringError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(WiringError::InvalidName("a control needs a name".into()));
    }
    if trimmed.contains(['\n', '\r']) {
        return Err(WiringError::InvalidName(
            "a control name cannot contain a line break".into(),
        ));
    }
    if let Some(rest) = trimmed.strip_prefix("Name: ") {
        return Err(WiringError::InvalidName(format!(
            "write {rest:?}, not the file's label"
        )));
    }
    Ok(())
}

/// The wiring half of a bot: the controls on the board and their bindings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlBoard {
    pub controls: Vec<Control>,
    pub bindings: Vec<Binding>,
}

impl ControlBoard {
    /// Read a bot's controllers and bindings into the model.
    pub fn from_bot(bot: &Bot) -> Result<Self, WiringError> {
        let controls = bot
            .controllers
            .iter()
            .map(Control::from_controller)
            .collect::<Result<Vec<_>, _>>()?;
        for binding in &bot.bindings {
            Action::parse(&binding.action)?;
        }
        Ok(ControlBoard {
            controls,
            bindings: bot.bindings.clone(),
        })
    }

    /// Write the board back into a bot.
    pub fn apply_to(&self, bot: &mut Bot) {
        bot.controllers = self.controls.iter().map(Control::to_controller).collect();
        bot.bindings = self.bindings.clone();
    }

    /// A starting board with the four controls the stock bots use: `Forward`
    /// and `LeftRight` on W/S and A/D, `Fire` on Space and `Spin` on Q.
    pub fn default_board() -> Self {
        let mut board = ControlBoard::default();
        board
            .add(
                Control::new("Forward", [1, 0], ControlKind::Analog)
                    .with_keys(vec![[0, 0, 17, 0], [0, 0, 31, 0]]),
            )
            .expect("a fresh cell");
        board
            .add(
                Control::new("LeftRight", [2, 0], ControlKind::Analog)
                    .with_keys(vec![[0, 0, 30, 0], [0, 0, 32, 0]]),
            )
            .expect("a fresh cell");
        board
            .add(Control::new("Fire", [0, 0], ControlKind::Button).with_keys(vec![[0, 0, 57, 0]]))
            .expect("a fresh cell");
        board
            .add(Control::new("Spin", [0, 1], ControlKind::Switch).with_keys(vec![[0, 0, 16, 0]]))
            .expect("a fresh cell");
        board
    }

    /// Add a control. Cells are exclusive: the game's grid holds one control
    /// per slot.
    pub fn add(&mut self, control: Control) -> Result<(), WiringError> {
        control.check()?;
        if let Some(other) = self.control_at(control.cell) {
            return Err(WiringError::CellTaken {
                cell: control.cell,
                by: other.name.clone(),
            });
        }
        self.controls.push(control);
        Ok(())
    }

    /// Remove the control in `cell` and every binding that named it.
    pub fn remove(&mut self, cell: [i32; 2]) -> Option<Control> {
        let index = self.controls.iter().position(|c| c.cell == cell)?;
        let control = self.controls.remove(index);
        self.bindings
            .retain(|b| b.control[0] != cell[0] || b.control[1] != cell[1]);
        Some(control)
    }

    pub fn control_at(&self, cell: [i32; 2]) -> Option<&Control> {
        self.controls.iter().find(|c| c.cell == cell)
    }

    /// Every control with this name. Two stock bots carry two controls called
    /// `New Analog`, so a name is not necessarily unique.
    pub fn controls_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Control> + 'a {
        self.controls
            .iter()
            .filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    /// The names on this board, in board order — what an unknown-name error
    /// quotes.
    pub fn names(&self) -> Vec<String> {
        self.controls.iter().map(|c| c.name.clone()).collect()
    }

    /// Wire `component` to `action` on the control in `cell`, slot `slot`.
    pub fn bind(
        &mut self,
        component: usize,
        action: Action,
        cell: [i32; 2],
        slot: usize,
    ) -> Result<(), WiringError> {
        let control = self
            .control_at(cell)
            .ok_or_else(|| WiringError::NoControlAt {
                cell,
                available: self.names(),
            })?;
        if slot >= control.kind.slots() {
            return Err(WiringError::SlotOutOfRange {
                kind: control.kind,
                slot,
                slots: control.kind.slots(),
            });
        }
        self.bindings.push(Binding {
            component: component as i32,
            action: action.name().to_string(),
            control: [cell[0], cell[1], slot as i32],
        });
        Ok(())
    }

    /// Wire `component` to `action`, checking it against the game's matrix
    /// before the row is written.
    ///
    /// [`ControlBoard::bind`] is the raw writer used by the corpus round-trip
    /// (a loaded row must be re-writable even if it is unusual). An editor's
    /// click goes through this method, so a button cannot be given an analog
    /// spin action and the error names the actions that kind *can* give.
    pub fn bind_checked(
        &mut self,
        components: &[Component],
        component: usize,
        action: Action,
        cell: [i32; 2],
        slot: usize,
    ) -> Result<(), WiringError> {
        let control = self
            .control_at(cell)
            .ok_or_else(|| WiringError::NoControlAt {
                cell,
                available: self.names(),
            })?;
        if slot >= control.kind.slots() {
            return Err(WiringError::SlotOutOfRange {
                kind: control.kind,
                slot,
                slots: control.kind.slots(),
            });
        }
        let Some(target) = components.get(component) else {
            return Err(WiringError::ComponentOutOfRange(component));
        };
        let allowed = Self::actions_for(control.kind, &target.class, slot);
        if !allowed.contains(&action) {
            return Err(WiringError::ActionNotAllowed {
                kind: control.kind,
                class: target.class.clone(),
                slot,
                action,
                allowed: allowed.iter().map(|a| a.name()).collect(),
            });
        }
        self.bind(component, action, cell, slot)
    }

    /// Wire by control name. The error for an unknown name quotes the board's
    /// own list, which is the list the player can see.
    pub fn bind_named(
        &mut self,
        name: &str,
        component: usize,
        action: Action,
        slot: usize,
    ) -> Result<(), WiringError> {
        let Some(control) = self.controls_named(name).next() else {
            return Err(WiringError::UnknownControl {
                name: name.to_string(),
                available: self.names(),
            });
        };
        let cell = control.cell;
        self.bind(component, action, cell, slot)
    }

    pub fn unbind(&mut self, component: usize, cell: [i32; 2], slot: usize) {
        self.bindings.retain(|b| {
            !(b.component == component as i32
                && b.control[0] == cell[0]
                && b.control[1] == cell[1]
                && b.control[2] == slot as i32)
        });
    }

    /// Every action a control of `kind` may give `class` in `slot`.
    ///
    /// The matrix is the binary's RTTI template list (see the module note). A
    /// wheel, weapon or other passive part has no row: it is driven through the
    /// motor it hangs off.
    pub fn actions_for(kind: ControlKind, class: &str, _slot: usize) -> &'static [Action] {
        use Action::*;
        match (kind, class) {
            // `Analog<SpinMotor>`: two inputs, opposite directions.
            (ControlKind::Analog, "SpinMotor") => &[SpinClockwise, SpinCounterClockwise][..],
            // `Switch<SpinMotor>`: a toggle, two directions.
            (ControlKind::Switch, "SpinMotor") => &[OnOffClockwise, OnOffCounterClockwise][..],
            // `Button<SpinMotor>`: on while held, two directions.
            (ControlKind::Button, "SpinMotor") => &[OnClockwise, OnCounterClockwise][..],
            // Weapons are fired through the motor that carries them.
            (ControlKind::Button, "BurstMotor" | "BurstPiston" | "Cannon") => &[Fire][..],
            // `Analog<ServoMotor>` / `Button<ServoMotor>`.
            (ControlKind::Analog | ControlKind::Button, "ServoMotor") => {
                &[SpinClockwise, SpinCounterClockwise][..]
            }
            // `Analog<ServoPiston>` / `Button<ServoPiston>`.
            (ControlKind::Analog | ControlKind::Button, "ServoPiston") => &[Extend, Retract][..],
            // `Analog<Steering>`: the component takes one axis per control, and
            // the player chooses which (a Forward control is wired
            // Forward/Backward, a LeftRight control Left/Right). Which action
            // sits on slot 0 and which on slot 1 is the player's own wiring —
            // the corpus has both orientations — so both slots allow all four.
            (ControlKind::Analog, "Steering") => &[Forward, Backward, Left, Right][..],
            // `Analog<Magnet>` / `Button<Magnet>` / `Switch<Magnet>`.
            (ControlKind::Analog | ControlKind::Button | ControlKind::Switch, "Magnet") => {
                &[RepelOnOff, AttractOnOff][..]
            }
            _ => &[][..],
        }
    }

    /// Check every row of the board. `components` is the bot's component list
    /// (index 0 the chassis); a binding must name an existing component and an
    /// action its control kind and that component's class allow.
    pub fn validate(&self, components: &[Component]) -> Vec<WiringError> {
        let mut errors = Vec::new();
        for (i, control) in self.controls.iter().enumerate() {
            if let Err(e) = control.check() {
                errors.push(e);
            }
            if self.controls[..i]
                .iter()
                .any(|other| other.cell == control.cell)
            {
                errors.push(WiringError::CellTaken {
                    cell: control.cell,
                    by: self.controls[..i]
                        .iter()
                        .find(|o| o.cell == control.cell)
                        .map(|o| o.name.clone())
                        .unwrap_or_default(),
                });
            }
        }
        for binding in &self.bindings {
            let cell = [binding.control[0], binding.control[1]];
            let slot = binding.control[2].max(0) as usize;
            let Some(control) = self.control_at(cell) else {
                errors.push(WiringError::NoControlAt {
                    cell,
                    available: self.names(),
                });
                continue;
            };
            let Ok(action) = Action::parse(&binding.action) else {
                errors.push(WiringError::UnknownAction {
                    name: binding.action.clone(),
                    known: ACTION_NAMES,
                });
                continue;
            };
            if slot >= control.kind.slots() {
                errors.push(WiringError::SlotOutOfRange {
                    kind: control.kind,
                    slot,
                    slots: control.kind.slots(),
                });
                continue;
            }
            let Some(component) = usize::try_from(binding.component)
                .ok()
                .and_then(|i| components.get(i))
            else {
                errors.push(WiringError::ComponentOutOfRange(
                    binding.component.max(0) as usize
                ));
                continue;
            };
            let allowed = Self::actions_for(control.kind, &component.class, slot);
            if !allowed.contains(&action) {
                errors.push(WiringError::ActionNotAllowed {
                    kind: control.kind,
                    class: component.class.clone(),
                    slot,
                    action,
                    allowed: allowed.iter().map(|a| a.name()).collect(),
                });
            }
        }
        errors
    }
}

/// A wiring problem, with the list the caller needs to fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WiringError {
    /// Two controls in one cell.
    CellTaken { cell: [i32; 2], by: String },
    /// A binding names a cell with no control.
    NoControlAt {
        cell: [i32; 2],
        available: Vec<String>,
    },
    /// A binding names a control that is not on the board.
    UnknownControl {
        name: String,
        available: Vec<String>,
    },
    /// A name that cannot be written on one `Name:` line.
    InvalidName(String),
    /// A controller kind code that is not 1, 2 or 4.
    InvalidControlCode { code: i32 },
    /// More key rows than the kind has slots.
    TooManyKeys {
        kind: ControlKind,
        got: usize,
        slots: usize,
    },
    /// A binding slot past the kind's inputs.
    SlotOutOfRange {
        kind: ControlKind,
        slot: usize,
        slots: usize,
    },
    /// A binding to a component that does not exist.
    ComponentOutOfRange(usize),
    /// The action does not fit this kind and component class.
    ActionNotAllowed {
        kind: ControlKind,
        class: String,
        slot: usize,
        action: Action,
        allowed: Vec<&'static str>,
    },
    /// An action string the game does not define.
    UnknownAction {
        name: String,
        known: &'static [&'static str],
    },
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WiringError::CellTaken { cell, by } => {
                write!(f, "cell {:?} is already taken by {by:?}", cell)
            }
            WiringError::NoControlAt { cell, available } => write!(
                f,
                "no control at cell {:?}; the board has {}",
                cell,
                quoted(available)
            ),
            WiringError::UnknownControl { name, available } => write!(
                f,
                "control {name:?} is not on this board; shipped controls here: {}",
                quoted(available)
            ),
            WiringError::InvalidName(e) => write!(f, "invalid control name: {e}"),
            WiringError::InvalidControlCode { code } => {
                write!(
                    f,
                    "control kind {code} is not 1 (analog), 2 (switch) or 4 (button)"
                )
            }
            WiringError::TooManyKeys { kind, got, slots } => write!(
                f,
                "a {} has {slots} input(s), got {got} key rows",
                kind.label()
            ),
            WiringError::SlotOutOfRange { kind, slot, slots } => write!(
                f,
                "slot {slot} is past a {}'s {slots} input(s)",
                kind.label()
            ),
            WiringError::ComponentOutOfRange(index) => {
                write!(f, "binding names component {index}, which does not exist")
            }
            WiringError::ActionNotAllowed {
                kind,
                class,
                slot,
                action,
                allowed,
            } => write!(
                f,
                "a {} on slot {slot} cannot give {class} {:?}; it can give: {}",
                kind.label(),
                action.name(),
                quoted(allowed)
            ),
            WiringError::UnknownAction { name, known } => write!(
                f,
                "unknown action {name:?}; the game's actions are: {}",
                quoted(known)
            ),
        }
    }
}

impl std::error::Error for WiringError {}

fn quoted<S: AsRef<str>>(items: &[S]) -> String {
    items
        .iter()
        .map(|s| format!("{:?}", s.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// DirectInput scan-code names for the keys the corpus uses (and the usual
/// neighbours). The `.bot` stores scan codes, not characters: 17 is `W`, 31 is
/// `S`, 30 is `A`, 32 is `D`, 57 is Space.
pub fn dik_name(code: i32) -> String {
    let name = match code {
        1 => "Esc",
        14 => "Backspace",
        15 => "Tab",
        16 => "Q",
        17 => "W",
        18 => "E",
        19 => "R",
        20 => "T",
        21 => "Y",
        22 => "U",
        23 => "I",
        24 => "O",
        25 => "P",
        28 => "Enter",
        29 => "LCtrl",
        30 => "A",
        31 => "S",
        32 => "D",
        33 => "F",
        34 => "G",
        35 => "H",
        36 => "J",
        37 => "K",
        38 => "L",
        42 => "LShift",
        44 => "Z",
        45 => "X",
        46 => "C",
        47 => "V",
        48 => "B",
        49 => "N",
        50 => "M",
        54 => "RShift",
        57 => "Space",
        59 => "F1",
        60 => "F2",
        61 => "F3",
        62 => "F4",
        63 => "F5",
        64 => "F6",
        65 => "F7",
        66 => "F8",
        67 => "F9",
        68 => "F10",
        71 => "Numpad7",
        72 => "Numpad8",
        73 => "Numpad9",
        75 => "Numpad4",
        76 => "Numpad5",
        77 => "Numpad6",
        79 => "Numpad1",
        80 => "Numpad2",
        81 => "Numpad3",
        82 => "Numpad0",
        87 => "F11",
        88 => "F12",
        other => return format!("Key {other}"),
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn game_root() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("RA2_GAME") {
            let dir = PathBuf::from(dir);
            return dir.join("Components").is_dir().then_some(dir);
        }
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        while dir.pop() {
            let candidate = dir.join("local/game");
            if candidate.join("Components").is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    fn bots(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                bots(&path, out);
            } else if path.extension().is_some_and(|e| e == "bot") {
                out.push(path);
            }
        }
    }

    #[test]
    fn the_default_board_is_four_controls_and_its_keys_read_as_wsad() {
        let board = ControlBoard::default_board();
        assert_eq!(board.names(), ["Forward", "LeftRight", "Fire", "Spin"]);
        let forward = board.control_at([1, 0]).unwrap();
        assert_eq!(forward.kind, ControlKind::Analog);
        assert_eq!(forward.key_label(), "W - S");
        assert_eq!(board.control_at([2, 0]).unwrap().key_label(), "A - D");
        assert_eq!(board.control_at([0, 0]).unwrap().key_label(), "Space");
    }

    #[test]
    fn a_binding_written_saved_and_reread_is_identical() {
        let mut bot = Bot {
            name: "Wired".into(),
            components: vec![
                Component {
                    parent: -1,
                    class: "Chassis".into(),
                    ..Default::default()
                },
                Component {
                    parent: 0,
                    class: "SpinMotor".into(),
                    ..Default::default()
                },
                Component {
                    parent: 0,
                    class: "BurstMotor".into(),
                    ..Default::default()
                },
                Component {
                    parent: 0,
                    class: "Steering".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let mut board = ControlBoard::default_board();
        board
            .bind_named("Forward", 1, Action::SpinClockwise, 0)
            .unwrap();
        board.bind_named("LeftRight", 3, Action::Left, 0).unwrap();
        board.bind_named("LeftRight", 3, Action::Right, 1).unwrap();
        board.bind_named("Fire", 2, Action::Fire, 0).unwrap();
        board
            .bind_named("Spin", 1, Action::OnOffClockwise, 0)
            .unwrap();
        assert!(
            board.validate(&bot.components).is_empty(),
            "{:?}",
            board.validate(&bot.components)
        );
        board.apply_to(&mut bot);

        let bytes = bot.write();
        let back = ra2fmt::bot::parse(&bytes).unwrap();
        assert_eq!(back.controllers, bot.controllers);
        assert_eq!(back.bindings, bot.bindings);
        let reread = ControlBoard::from_bot(&back).unwrap();
        assert_eq!(reread, board);
    }

    #[test]
    fn an_unknown_control_name_is_rejected_with_the_boards_list() {
        let mut board = ControlBoard::default_board();
        let err = board
            .bind_named("Jump", 1, Action::SpinClockwise, 0)
            .unwrap_err();
        assert_eq!(
            err,
            WiringError::UnknownControl {
                name: "Jump".into(),
                available: vec![
                    "Forward".into(),
                    "LeftRight".into(),
                    "Fire".into(),
                    "Spin".into()
                ],
            }
        );
        let text = err.to_string();
        for name in ["Forward", "LeftRight", "Fire", "Spin"] {
            assert!(text.contains(name), "{text}");
        }
    }

    #[test]
    fn a_button_cannot_spin_a_motor_and_says_what_it_can_do() {
        let board = ControlBoard::default_board();
        // `Spin` is a switch: its only spin-motor actions are on/off.
        let control = board.control_at([0, 1]).unwrap();
        assert_eq!(
            ControlBoard::actions_for(control.kind, "SpinMotor", 0),
            &[Action::OnOffClockwise, Action::OnOffCounterClockwise]
        );
        assert!(ControlBoard::actions_for(ControlKind::Button, "Weapon", 0).is_empty());
        assert_eq!(
            ControlBoard::actions_for(ControlKind::Button, "BurstMotor", 0),
            &[Action::Fire]
        );
        assert_eq!(
            ControlBoard::actions_for(ControlKind::Analog, "Steering", 0),
            &[
                Action::Forward,
                Action::Backward,
                Action::Left,
                Action::Right
            ]
        );
        assert_eq!(
            ControlBoard::actions_for(ControlKind::Analog, "Steering", 1),
            &[
                Action::Forward,
                Action::Backward,
                Action::Left,
                Action::Right
            ]
        );
    }

    #[test]
    fn bind_checked_refuses_an_action_the_kind_and_class_do_not_allow() {
        let components = vec![
            Component {
                parent: -1,
                class: "Chassis".into(),
                ..Default::default()
            },
            Component {
                parent: 0,
                class: "SpinMotor".into(),
                ..Default::default()
            },
        ];
        let mut board = ControlBoard::default_board();
        let err = board
            .bind_checked(&components, 1, Action::OnOffClockwise, [1, 0], 0)
            .unwrap_err();
        match err {
            WiringError::ActionNotAllowed {
                kind,
                class,
                action,
                allowed,
                ..
            } => {
                assert_eq!(kind, ControlKind::Analog);
                assert_eq!(class, "SpinMotor");
                assert_eq!(action, Action::OnOffClockwise);
                assert_eq!(allowed, vec!["Spin Clockwise", "Spin Counter-clockwise"]);
            }
            other => panic!("{other:?}"),
        }
        // The analog spin action goes in, and the raw writer is not affected.
        board
            .bind_checked(&components, 1, Action::SpinClockwise, [1, 0], 0)
            .unwrap();
        assert_eq!(board.bindings.len(), 1);
        assert!(board.validate(&components).is_empty());
    }

    #[test]
    fn a_cell_holds_one_control() {
        let mut board = ControlBoard::default_board();
        let err = board
            .add(Control::new("Extra", [1, 0], ControlKind::Button))
            .unwrap_err();
        assert!(matches!(err, WiringError::CellTaken { cell, .. } if cell == [1, 0]));
        // Removing the control also removes the bindings that named it.
        board
            .bind_named("Forward", 1, Action::SpinClockwise, 0)
            .unwrap();
        assert_eq!(board.bindings.len(), 1);
        assert!(board.remove([1, 0]).is_some());
        assert!(board.bindings.is_empty());
    }

    #[test]
    fn an_action_name_parses_to_its_file_spelling() {
        for name in SHIPPED_ACTION_NAMES {
            let action = Action::parse(name).unwrap();
            assert_eq!(action.name(), *name);
        }
        let err = Action::parse("Spin Sideways").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("the game's actions"), "{text}");
        assert!(text.contains("Spin Clockwise"), "{text}");
    }

    /// Every shipped bot's controllers and bindings convert through the model
    /// and write back to the same file rows.
    #[test]
    fn every_shipped_board_round_trips_through_the_model() {
        let Some(root) = game_root() else { return };
        let mut files = Vec::new();
        for sub in ["Robot Designs", "AI", "Teams"] {
            let dir = root.join(sub);
            if dir.is_dir() {
                bots(&dir, &mut files);
            }
        }
        files.sort();
        assert!(files.len() >= 57);
        let mut rows = 0usize;
        let mut names = std::collections::BTreeSet::new();
        let mut actions = std::collections::BTreeSet::new();
        for path in &files {
            let data = std::fs::read(path).unwrap();
            let bot = ra2fmt::bot::parse(&data).unwrap();
            let board = ControlBoard::from_bot(&bot).unwrap_or_else(|e| {
                panic!("{}: {e}", path.display());
            });
            let errors = board.validate(&bot.components);
            assert!(errors.is_empty(), "{}: {errors:?}", path.display());
            for control in &board.controls {
                names.insert(control.name.clone());
            }
            for binding in &board.bindings {
                actions.insert(binding.action.clone());
            }
            let mut back = bot.clone();
            board.apply_to(&mut back);
            assert_eq!(back.controllers, bot.controllers);
            assert_eq!(back.bindings, bot.bindings);
            rows += board.bindings.len();
        }
        assert!(rows > 700, "expected the shipped wiring, counted {rows}");
        // The name and action constants are the corpus', not a hand list: the
        // 19 names and 9 action strings below are exactly what the files use.
        assert_eq!(
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            SHIPPED_CONTROL_NAMES
        );
        assert_eq!(
            actions.iter().map(String::as_str).collect::<Vec<_>>(),
            SHIPPED_ACTION_NAMES
        );
    }
}
