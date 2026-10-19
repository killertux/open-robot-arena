//! The robot stats panel: weight, hitpoints and cost.
//!
//! The weight and hitpoint rules are the simulator's, not a second formula:
//!
//! - **chassis mass** = surface area × the armour row's weight per square unit,
//!   floored at 5 kg — `ra2fmt::bot::Chassis::mass`, the same function
//!   `ra2sim::rig` calls;
//! - **component mass** = the definition's `mass =` field, else the sum of the
//!   `.gmf`'s top-level rigid-body masses, else the F12 zero-mass fill (all
//!   shared through `ra2fmt::component`);
//! - **chassis hitpoints** = the armour hardness the bot file records;
//! - **component hitpoints** = `hitpoints =` in the definition, else the
//!   factory's category table (`FUN_0045cf40`: power/extenders/mechanics/
//!   mobility/weapons/extras → 400/100/400/400/1000/1000), else the role's
//!   fallback for a component that did not load.
//!
//! A component with no geometry contributes **no mass** — the simulator's rule.
//! That matters for an unloaded SmartZone or a missing definition.
//!
//! **Cost is not recovered.** No shipped component definition, event, team or
//! bot file carries a price, and the match layer scores points, not money. The
//! model therefore reports `cost: None` rather than inventing a number; see
//! `docs/15-workshop-model.md`.

use ra2fmt::bot::{Bot, Chassis};
use ra2fmt::component::ComponentDef;

use super::library::ComponentInfo;

/// The three shipped weight classes. The bot file records the class the editor
/// wrote; the thresholds are the game's (light < 250 kg, medium 250–400,
/// heavy > 400).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightClass {
    Light,
    Medium,
    Heavy,
}

impl WeightClass {
    pub fn of_mass(mass: f32) -> WeightClass {
        if mass < 250.0 {
            WeightClass::Light
        } else if mass <= 400.0 {
            WeightClass::Medium
        } else {
            WeightClass::Heavy
        }
    }

    pub fn from_file(class: i32) -> WeightClass {
        match class {
            1 => WeightClass::Medium,
            2 => WeightClass::Heavy,
            _ => WeightClass::Light,
        }
    }

    /// The inventory's class names, the exe's strings at `0x6fd7b0`.
    pub fn full_label(self) -> &'static str {
        match self {
            WeightClass::Light => "Lightweight",
            WeightClass::Medium => "Middleweight",
            WeightClass::Heavy => "Heavyweight",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WeightClass::Light => "Light",
            WeightClass::Medium => "Medium",
            WeightClass::Heavy => "Heavy",
        }
    }
}

/// The smallest role fallbacks, which the simulator uses when a component did
/// not load (it has no definition to read a category from).
///
/// These are `ra2sim::rig::Role::default_hitpoints`'s numbers, keyed by the
/// `.bot` class the file names.
pub fn role_hitpoints(class: &str) -> f32 {
    match class {
        "Chassis" => 1000.0,
        "Weapon" => 600.0,
        "SpinMotor" | "ServoMotor" | "BurstMotor" => 350.0,
        "BurstPiston" | "ServoPiston" => 350.0,
        "Wheel" => 250.0,
        "ControlBoard" => 200.0,
        "Battery" | "AirTank" => 250.0,
        _ => 300.0,
    }
}

/// One part's contribution to the panel.
#[derive(Clone, Debug, PartialEq)]
pub struct PartStats {
    pub index: usize,
    pub class: String,
    pub name: String,
    /// Mass in kg.
    pub mass: f32,
    /// Structural hitpoints.
    pub hitpoints: f32,
}

/// The whole robot's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct BotStats {
    /// Total mass in kg, exactly as the simulator reports it.
    pub mass: f32,
    /// Sum of every component's structural hitpoints (the chassis' is the
    /// armour hardness).
    pub hitpoints: f32,
    /// The chassis' own hitpoints (the armour hardness).
    pub chassis_hitpoints: f32,
    /// The class the `.bot` records, and the class the mass implies.
    pub declared_class: WeightClass,
    pub computed_class: WeightClass,
    /// No shipped table carries a price; this stays `None` rather than guessing.
    pub cost: Option<f32>,
    pub parts: Vec<PartStats>,
}

/// A component's hitpoints by the simulator's rule.
pub fn component_hitpoints(class: &str, def: Option<&ComponentDef>) -> f32 {
    def.and_then(|d| d.number("hitpoints"))
        .or_else(|| ra2fmt::component::category_hitpoints(def))
        .unwrap_or_else(|| role_hitpoints(class))
}

/// The chassis' hitpoints: the armour hardness the bot file records, else the
/// chassis role's fallback.
pub fn chassis_hitpoints(chassis: Option<&Chassis>) -> f32 {
    chassis
        .and_then(|c| c.armour)
        .map(|a| a.hardness)
        .unwrap_or_else(|| role_hitpoints("Chassis"))
}

impl BotStats {
    /// Compute from a bot and its resolved component info (index-aligned).
    ///
    /// `mass` is summed only over components with geometry, and floored at 1 kg,
    /// exactly as `ra2sim::rig::RobotRig::mass` is.
    pub fn new(bot: &Bot, info: &[ComponentInfo]) -> BotStats {
        let mut mass = 0.0f32;
        let mut hitpoints = 0.0f32;
        let mut parts = Vec::with_capacity(bot.components.len());
        let mut chassis_hp = role_hitpoints("Chassis");

        for (i, component) in bot.components.iter().enumerate() {
            let part = info.get(i);
            let (part_mass, part_hp, has_hull) = if i == 0 {
                let chassis = component.chassis.as_ref();
                let m = chassis.map(|ch| ch.mass()).unwrap_or(0.0).max(5.0);
                let hp = chassis_hitpoints(chassis);
                chassis_hp = hp;
                let hull = chassis
                    .and_then(|c| c.mesh.as_ref())
                    .is_some_and(|m| !m.points.is_empty());
                (m, hp, hull)
            } else {
                match part {
                    Some(info) => (
                        info.mass,
                        component_hitpoints(&component.class, info.def.as_ref()),
                        info.has_hull,
                    ),
                    None => (0.0, component_hitpoints(&component.class, None), false),
                }
            };
            if has_hull {
                mass += part_mass;
            }
            hitpoints += part_hp;
            parts.push(PartStats {
                index: i,
                class: component.class.clone(),
                name: part.map(|p| p.name.clone()).unwrap_or_else(|| {
                    if i == 0 {
                        "Chassis".into()
                    } else {
                        component.class.clone()
                    }
                }),
                mass: part_mass,
                hitpoints: part_hp,
            });
        }

        let mass = mass.max(1.0);
        BotStats {
            mass,
            hitpoints,
            chassis_hitpoints: chassis_hp,
            declared_class: WeightClass::from_file(bot.class),
            computed_class: WeightClass::of_mass(mass),
            cost: None,
            parts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::bot::{Armour, BotMesh};

    fn mesh(side: f32) -> BotMesh {
        // Two triangles making a `side × side` square, area = side².
        BotMesh {
            points: vec![
                [0.0, 0.0, 0.0],
                [side, 0.0, 0.0],
                [side, 0.0, side],
                [0.0, 0.0, side],
            ],
            normals: vec![[0.0, 1.0, 0.0]; 4],
            uvs: vec![[0.0, 0.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    #[test]
    fn the_chassis_mass_is_area_times_armour_weight() {
        let ch = Chassis {
            mesh: Some(mesh(2.0)),
            armour: Some(Armour {
                type_id: 2,
                weight: 20.0,
                hardness: 175.0,
            }),
            ..Default::default()
        };
        // 4 m² × 20 kg/m² = 80 kg.
        assert!((ch.mass() - 80.0).abs() < 1e-4);
    }

    #[test]
    fn weight_classes_use_the_games_thresholds() {
        assert_eq!(WeightClass::of_mass(249.9), WeightClass::Light);
        assert_eq!(WeightClass::of_mass(250.0), WeightClass::Medium);
        assert_eq!(WeightClass::of_mass(400.0), WeightClass::Medium);
        assert_eq!(WeightClass::of_mass(400.1), WeightClass::Heavy);
    }

    #[test]
    fn hitpoints_follow_the_definitions_category_table() {
        let def = ra2fmt::component::parse("type = weapons\nhitpoints = 500\n");
        assert_eq!(component_hitpoints("Weapon", Some(&def)), 500.0);
        let def = ra2fmt::component::parse("type = extenders\n");
        assert_eq!(component_hitpoints("Component", Some(&def)), 100.0);
        // An unloaded component falls back to its role.
        assert_eq!(component_hitpoints("SpinMotor", None), 350.0);
        assert_eq!(component_hitpoints("Wheel", None), 250.0);
    }

    #[test]
    fn cost_is_named_as_unrecovered_not_invented() {
        let stats = BotStats::new(&Bot::default(), &[]);
        assert_eq!(stats.cost, None);
    }
}
