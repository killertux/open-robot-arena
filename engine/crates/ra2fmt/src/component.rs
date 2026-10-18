//! Minimal parser for RA2 component definition `.txt` files.
//!
//! These live inside `Components.cfz` (a CFL3 archive) and map a component class
//! to its mesh: `dir` + `model` resolve to `Components/<dir>/<model>`.

#[derive(Debug, Clone, Default)]
pub struct ComponentDef {
    pub name: String,
    pub dir: String,
    pub class: String,
    pub model: String,
    pub preview: String,
    pub standard: bool,
    /// All raw `key = value` pairs, lower-cased keys.
    pub fields: Vec<(String, String)>,
}

impl ComponentDef {
    /// Path of the mesh relative to the `Components` directory.
    pub fn mesh_rel_path(&self) -> String {
        if self.dir.is_empty() {
            self.model.clone()
        } else {
            format!("{}/{}", self.dir, self.model)
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The first number of a `key = <n> ...` field.
    pub fn number(&self, key: &str) -> Option<f32> {
        self.get(key)?.split_whitespace().next()?.parse().ok()
    }

    /// The material the component is made of, and its damage scale (F7).
    ///
    /// The definition's `material = metal(.4)` line carries a class name and,
    /// in parentheses, an override for the material object's `+4` float — the
    /// multiplier `FUN_00489470` applies to a hit from this component. The
    /// engine's `MaterialObject` subclasses give each class a default:
    /// `Material`/`Metal` **0.1** (`FUN_00402c70` / `FUN_004891e0` store
    /// `0x3dcccccd`), `Rubber` **0.0** (`FUN_004899e0`), `Arenium` **0.5**
    /// (`FUN_00402c50` stores `0x3f000000`). A component inherits `Metal`
    /// unless its definition says otherwise (`FUN_0045cc10` calls
    /// `FUN_00489110`, whose constructor allocates a Metal material and stores
    /// it at `+0x64`), and the factory (`FUN_0045cf40`, `0x45d017`) overwrites
    /// that material's `+4` from the definition when the override is `>= 0`.
    ///
    /// Only five shipped definitions declare the key: the four casters are
    /// `rubber(0)` and the ram plate is `metal(.4)`.
    pub fn material(&self) -> MaterialDef {
        let Some(value) = self.get("material") else {
            return MaterialDef::default();
        };
        let (name, scale) = match value.split_once('(') {
            Some((name, rest)) => {
                let scale = rest
                    .split(')')
                    .next()
                    .and_then(|n| n.trim().parse::<f32>().ok());
                (name.trim().to_ascii_lowercase(), scale)
            }
            None => (value.trim().to_ascii_lowercase(), None),
        };
        // `FUN_0045cf40` applies the override only when it is `>= 0`; a
        // negative value leaves the class constructor's default in place.
        let scale = scale
            .filter(|s| *s >= 0.0)
            .or_else(|| material_class_scale(&name))
            .unwrap_or(DEFAULT_MATERIAL_SCALE);
        MaterialDef { name, scale }
    }

    /// The `type` category (the component factory's switch), lower-cased.
    pub fn category(&self) -> Option<String> {
        self.get("type").map(|s| {
            s.split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        })
    }
}

/// Parse a `key = value` component definition (text).
pub fn parse(text: &str) -> ComponentDef {
    let mut d = ComponentDef::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
        match k.as_str() {
            "name" => d.name = v.clone(),
            "dir" => d.dir = v.clone(),
            "base" => d.class = v.clone(),
            "model" => d.model = v.clone(),
            "preview" => d.preview = v.clone(),
            "standard" => d.standard = v.trim() == "1",
            _ => {}
        }
        d.fields.push((k, v));
    }
    d
}

/// A component's material class and the damage multiplier its `+4` float holds.
///
/// The engine has four `MaterialObject` classes. Their constructors are in the
/// binary at `0x402c70` (`Material`, scale 0.1), `0x402c50` (`Arenium`, 0.5),
/// `0x4891e0` (`Metal`, 0.1) and `0x4899e0` (`Rubber`, 0.0); each stores the
/// float at `this[1]` (`+4`). A component definition may override it with
/// `material = <class>(<float>)`.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDef {
    pub name: String,
    pub scale: f32,
}

/// The engine's default component material: `Metal`, whose `+4` is 0.1.
pub const DEFAULT_MATERIAL_SCALE: f32 = 0.1;

/// The default scale of a material class, from its constructor.
pub fn material_class_scale(name: &str) -> Option<f32> {
    match name {
        "metal" => Some(0.1),
        "rubber" => Some(0.0),
        "arenium" => Some(0.5),
        _ => None,
    }
}

impl Default for MaterialDef {
    fn default() -> Self {
        MaterialDef {
            name: "metal".into(),
            scale: DEFAULT_MATERIAL_SCALE,
        }
    }
}

/// The maximum hitpoints the component factory gives a category when the
/// definition does not declare them.
///
/// Recovered from the binary: `FUN_0045cf40` switches on the definition's `type`
/// (the field the `.txt` parser fills from the `type = ` key, read back by
/// `FUN_0049aa30`) and stores 400 / 100 / 400 / 400 / 1000 / 1000 into the
/// component's maximum, after which a `hitpoints = ` line overrides it.
pub fn category_hitpoints(def: Option<&ComponentDef>) -> Option<f32> {
    match def.and_then(ComponentDef::category).as_deref() {
        Some("power") => Some(400.0),
        Some("extenders") => Some(100.0),
        Some("mechanics") => Some(400.0),
        Some("mobility") => Some(400.0),
        Some("weapons") => Some(1000.0),
        Some("extras") => Some(1000.0),
        _ => None,
    }
}

/// The `fracture` default the same factory sets per category.
pub fn category_fracture(def: Option<&ComponentDef>) -> Option<f32> {
    match def.and_then(ComponentDef::category).as_deref() {
        Some("extenders") => Some(15.0),
        Some("mechanics") | Some("mobility") | Some("weapons") => Some(20.0),
        Some("extras") => Some(60.0),
        _ => None,
    }
}

/// `damageable = false` (the factory's own field).
pub fn flag_false(def: &ComponentDef, key: &str) -> bool {
    def.get(key)
        .map(|v| v.trim().eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// The density our zero-mass fill uses. See [`zero_mass_fill`].
///
/// A density of **139** keeps the mini wheel (the wheel every stock AI bot runs,
/// 0.652 × 0.652 × 0.135 m) at exactly 8 kg, which is the mass the rest of the
/// simulation is calibrated against. The one documented stock wheel weight (the
/// mudtire at 22.5 kg, from the figure that closes Mud Runner's total) wants
/// 74 kg/m³ instead, which would make that same mini wheel 4.2 kg, while the
/// 139 that keeps the mini wheel at 8 puts the mudtire at 42 kg — badly over the
/// documented total. No single density does both, so either the engine's fill
/// is not volume-proportional or one anchor is wrong; that conflict is F12 in
/// `docs/06-deferred-fidelity.md`. The rule's shape — a zero-mass body is priced
/// by the room it takes up — is what matters, and the constant keeps the stock
/// bots where they are.
pub const ZERO_MASS_DENSITY: f32 = 139.0;

/// The mass a `.gmf` body with `mass = 0` is given, from the room it takes up.
///
/// Every stock wheel's rigid body stores `mass = 0`. The engine's own fill is not
/// reachable (its `.gmf` reader takes the zero at face value), so this is **our**
/// rule: the bounding-box volume times [`ZERO_MASS_DENSITY`], floored at 1 kg.
/// A part with no geometry at all is given 7 kg by its caller before this is
/// reached (see `ra2sim::rig::zero_mass_fill`).
pub fn zero_mass_fill(min: [f32; 3], max: [f32; 3]) -> f32 {
    let e = [
        (max[0] - min[0]).max(0.0),
        (max[1] - min[1]).max(0.0),
        (max[2] - min[2]).max(0.0),
    ];
    let volume = e[0] * e[1] * e[2];
    let density = std::env::var("RA2_ZERO_MASS_DENSITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(ZERO_MASS_DENSITY);
    (volume * density).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_class_defaults_are_the_constructors_floats() {
        // 0x3dcccccd, 0x3f000000 and 0x00000000.
        assert_eq!(material_class_scale("metal"), Some(0.1));
        assert_eq!(material_class_scale("arenium"), Some(0.5));
        assert_eq!(material_class_scale("rubber"), Some(0.0));
        assert_eq!(material_class_scale("unobtainium"), None);
    }

    #[test]
    fn the_material_override_wins_and_absent_means_metal() {
        let plain = parse("name = Saw Blade\ntype = weapons\n");
        assert_eq!(
            plain.material(),
            MaterialDef {
                name: "metal".into(),
                scale: 0.1
            }
        );

        let plate = parse("name = Ram Plate\nmaterial = metal(.4)\n");
        assert_eq!(plate.material().name, "metal");
        assert_eq!(plate.material().scale, 0.4);

        let caster = parse("name = Balance Caster\nmaterial = rubber(0)\n");
        assert_eq!(caster.material().name, "rubber");
        assert_eq!(caster.material().scale, 0.0);

        let name_only = parse("material = arenium\n");
        assert_eq!(name_only.material().scale, 0.5);

        // The factory only applies the override when it is non-negative; a
        // negative value leaves Metal's 0.1.
        let negative = parse("material = metal(-1)\n");
        assert_eq!(negative.material().scale, 0.1);
    }

    /// The shipped definitions, when the game is present: the only three
    /// distinct values in the stock set, one of them the default.
    #[test]
    fn the_shipped_definitions_carry_three_scales() {
        let Some(root) = std::env::var_os("RA2_GAME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                loop {
                    let c = p.join("local/game");
                    if c.is_dir() {
                        return Some(c);
                    }
                    if !p.pop() {
                        return None;
                    }
                }
            })
        else {
            eprintln!("skipping the shipped components: no local/game");
            return;
        };
        let cfl = crate::cfl::Cfl::open_path(&root.join("Components.cfz")).expect("Components.cfz");
        let find = |suffix: &str| {
            cfl.entries()
                .iter()
                .find(|e| e.name.to_ascii_lowercase().ends_with(suffix))
                .and_then(|e| cfl.get_text(&e.name))
                .map(|text| parse(&text).material())
                .unwrap_or_else(|| panic!("{suffix} in Components.cfz"))
        };
        let ram = find("ramplate1.txt");
        let caster = find("caster.txt");
        let blade = find("sawblade5.txt");
        println!(
            "shipped materials: ram plate {} {:.1}, caster {} {:.1}, saw blade {} {:.1}",
            ram.name, ram.scale, caster.name, caster.scale, blade.name, blade.scale
        );
        assert_eq!((ram.name.as_str(), ram.scale), ("metal", 0.4));
        assert_eq!((caster.name.as_str(), caster.scale), ("rubber", 0.0));
        assert_eq!((blade.name.as_str(), blade.scale), ("metal", 0.1));
    }
}
