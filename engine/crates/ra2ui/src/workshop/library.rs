//! The workshop's read-only view of the game's data: component definitions and
//! meshes, armour rows, and the texture/edge/decal libraries.
//!
//! Everything here is the shipped data, addressed the way the game addresses
//! it. Component definitions live in `Components.cfz` (a CFL3 archive) and their
//! meshes under `Components/<dir>/<model>`; `Armor_Definitions.txt` lives in
//! `RA2.cfz`; the paint lists are the files in `Texture Library/`, its `decals/`
//! and `edges/` subdirectories, and the player's own `Custom Textures/` folder.
//!
//! The lists are **file names**, because that is what the screens' own notes say
//! they are: `botlab/decals.gib`'s list is *"fill with all the decal image
//! filenames found in the texture library"*, and `surfacelayer.gib`'s and
//! `texturefill.gib`'s are *"…image filenames…"*. `import_texture.gib`'s note is
//! *"populate with available custom textures"*. No index file is read:
//! `Custom Textures/CustomTextures.txt` is a 40-byte note to the player, not a
//! list.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use glam::Vec3;
use ra2fmt::armor::ArmourDef;
use ra2fmt::bot::Bot;
use ra2fmt::cfl::Cfl;
use ra2fmt::component::ComponentDef;
use ra2fmt::gmi::{Attachment, GmiScene};
use ra2fmt::mount::tm_mat4;

/// Why the game's data could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryError {
    /// The path is not a game directory (no `Components` folder).
    NoGameRoot(PathBuf),
    /// A file could not be read.
    Io(String),
    /// A `.gmf` or archive did not parse.
    Parse(String),
    /// A component definition or mesh the caller named is not shipped.
    MissingComponent(String),
    /// An image could not be decoded.
    Image(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::NoGameRoot(p) => write!(f, "{} is not a game directory", p.display()),
            LibraryError::Io(e) => write!(f, "io: {e}"),
            LibraryError::Parse(e) => write!(f, "parse: {e}"),
            LibraryError::MissingComponent(file) => write!(f, "component {file:?} is not shipped"),
            LibraryError::Image(e) => write!(f, "image: {e}"),
        }
    }
}

impl std::error::Error for LibraryError {}

/// One component, as the game loads it: its definition, its mesh's attachment
/// points, and the two numbers the stats panel needs.
///
/// The mass rule is the simulator's (`ra2sim::rig`): a definition's `mass =`
/// field wins; otherwise the `.gmf`'s rigid-body masses are summed **as the
/// sim sums them** (the top-level bodies of each collection); and when that sum
/// is zero (every stock wheel) the body is priced by its bounding-box volume
/// (`ra2fmt::component::zero_mass_fill`). `has_hull` mirrors the sim's rule that
/// a component with no geometry contributes no mass to the robot.
#[derive(Debug, Clone, Default)]
pub struct ComponentInfo {
    /// `.bot` class string (`SpinMotor`, `Wheel`, ...).
    pub class: String,
    /// `.bot` file path (`Components\\ztek1.txt`).
    pub file: String,
    /// Display name from the definition.
    pub name: String,
    /// Definition `base` class (`Weapon`, `SpinMotor`, ...).
    pub base: String,
    /// The definition, when it loaded.
    pub def: Option<ComponentDef>,
    /// The component's own mount points, keyed by the id a `.bot` names.
    pub mounts: BTreeMap<i32, Attachment>,
    /// Mass in kg, by the simulator's rule.
    pub mass: f32,
    /// Whether the mesh has geometry (and so counts toward the robot's mass).
    pub has_hull: bool,
    /// Whether the definition *and* mesh loaded.
    pub loaded: bool,
}

impl ComponentInfo {
    /// A component that did not load: the simulator treats it as massless with
    /// no mounts, and falls back to the role's hitpoints.
    pub fn unloaded(class: &str, file: &str) -> Self {
        ComponentInfo {
            class: class.to_string(),
            file: file.to_string(),
            ..Default::default()
        }
    }

    /// Whether this component's mount points include `mount`.
    pub fn has_mount(&self, mount: i32) -> bool {
        self.mounts.contains_key(&mount)
    }
}

/// A texture file the paint screens can list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureEntry {
    /// The file name the list row shows (`Gear.tga`).
    pub name: String,
    /// Absolute path, for loading.
    pub path: PathBuf,
}

/// One edge style from `Texture Library/edges/*.txt`.
///
/// ```text
/// Filename: rivets.tga
/// Preview: rivets_preview.bmp
/// Length ratio (world pixels per file pixel): .1
/// Width (world pixels): 5
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeStyle {
    /// The style's name: the `Filename` value (`rivets.tga`).
    pub name: String,
    /// The image that carries the edge.
    pub file: String,
    /// Its preview bitmap.
    pub preview: Option<String>,
    /// World pixels per file pixel.
    pub length_ratio: f32,
    /// World-pixel width of the strip.
    pub width: f32,
    pub path: PathBuf,
}

/// A directory of image files, listed in file-name order.
#[derive(Debug, Clone, Default)]
pub struct TextureLibrary {
    pub root: PathBuf,
    pub entries: Vec<TextureEntry>,
}

impl TextureLibrary {
    /// Scan `root` for image files. Missing directories yield an empty list.
    pub fn scan(root: &Path) -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(root) {
            for entry in dir.flatten() {
                let path = entry.path();
                if !path.is_file() || !is_image(&path) {
                    continue;
                }
                if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
                    entries.push(TextureEntry { name, path });
                }
            }
        }
        entries.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        TextureLibrary {
            root: root.to_path_buf(),
            entries,
        }
    }

    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }

    /// Case-insensitive lookup by file name.
    pub fn find(&self, name: &str) -> Option<&TextureEntry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("bmp" | "tga" | "jpg" | "jpeg" | "png")
    )
}

/// The game directory, opened once and read on demand.
///
/// Component info is cached because a 57-bot stats pass asks for the same
/// definition and mesh dozens of times.
pub struct GameLibrary {
    root: PathBuf,
    components: Option<Cfl>,
    armour: Vec<ArmourDef>,
    textures: TextureLibrary,
    decals: TextureLibrary,
    edges: Vec<EdgeStyle>,
    custom: TextureLibrary,
    cache: RefCell<HashMap<String, ComponentInfo>>,
}

impl std::fmt::Debug for GameLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameLibrary")
            .field("root", &self.root)
            .field("armour", &self.armour.len())
            .field("textures", &self.textures.entries.len())
            .finish()
    }
}

impl GameLibrary {
    /// Open a game directory (one containing `Components/`).
    pub fn open(root: &Path) -> Result<Self, LibraryError> {
        if !root.join("Components").is_dir() {
            return Err(LibraryError::NoGameRoot(root.to_path_buf()));
        }
        let components = Cfl::open_path(&root.join("Components.cfz")).ok();
        let armour = Cfl::open_path(&root.join("RA2.cfz"))
            .ok()
            .and_then(|cf| cf.get_text("Armor_Definitions.txt"))
            .map(|text| ra2fmt::armor::parse(&text))
            .unwrap_or_default();
        Ok(GameLibrary {
            root: root.to_path_buf(),
            components,
            armour,
            textures: TextureLibrary::scan(&root.join("Texture Library")),
            decals: TextureLibrary::scan(&root.join("Texture Library/decals")),
            edges: read_edges(&root.join("Texture Library/edges")),
            custom: TextureLibrary::scan(&root.join("Custom Textures")),
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// Walk up from `start` looking for a game directory.
    pub fn find(start: &Path) -> Option<PathBuf> {
        let mut dir = if start.is_dir() {
            start.to_path_buf()
        } else {
            start.parent()?.to_path_buf()
        };
        for _ in 0..6 {
            if dir.join("Components").is_dir() {
                return Some(dir);
            }
            dir = dir.parent()?.to_path_buf();
        }
        None
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The texture library (`Texture Library/`), the decal library
    /// (`Texture Library/decals/`), the edge styles (`Texture Library/edges/`)
    /// and the player's custom textures (`Custom Textures/`).
    pub fn textures(&self) -> &TextureLibrary {
        &self.textures
    }

    pub fn decals(&self) -> &TextureLibrary {
        &self.decals
    }

    pub fn custom_textures(&self) -> &TextureLibrary {
        &self.custom
    }

    pub fn edges(&self) -> &[EdgeStyle] {
        &self.edges
    }

    /// The four armour rows from `Armor_Definitions.txt`, when the archive is
    /// present.
    pub fn armour(&self) -> &[ArmourDef] {
        &self.armour
    }

    pub fn armour_type(&self, type_id: i32) -> Option<&ArmourDef> {
        self.armour.iter().find(|a| a.type_id == type_id)
    }

    /// Read a component definition's text, from `Components.cfz` first and a
    /// loose file second (the order the renderer uses).
    pub fn component_text(&self, file: &str) -> Option<String> {
        self.components
            .as_ref()
            .and_then(|cf| cf.get_text(file))
            .or_else(|| {
                let path = ra2fmt::find_ci(&self.root, file)?;
                std::fs::read_to_string(path).ok()
            })
    }

    /// Load a component when only its file path is known: the `.bot` class is
    /// the definition's `base` (`SpinMotor`, `Weapon`, `Component`, …).
    pub fn component_info_by_file(&self, file: &str) -> ComponentInfo {
        let Some(text) = self.component_text(file) else {
            return ComponentInfo::unloaded("", file);
        };
        let def = ra2fmt::component::parse(&text);
        self.component_info(&def.class, file)
    }

    /// Load one component by its `.bot` class and file path.
    ///
    /// Mirrors `ra2_render::load_bot_components`: the chassis (index 0), a
    /// `SmartZone` and a component with an empty file are not loaded even when
    /// their definition exists.
    pub fn component_info(&self, class: &str, file: &str) -> ComponentInfo {
        let key = format!("{class}\0{file}");
        if let Some(info) = self.cache.borrow().get(&key) {
            return info.clone();
        }
        let info = self.load_component_info(class, file);
        self.cache.borrow_mut().insert(key, info.clone());
        info
    }

    fn load_component_info(&self, class: &str, file: &str) -> ComponentInfo {
        if class == "SmartZone" || file.is_empty() {
            return ComponentInfo::unloaded(class, file);
        }
        let Some(text) = self.component_text(file) else {
            return ComponentInfo::unloaded(class, file);
        };
        let def = ra2fmt::component::parse(&text);
        let Some(scene) = self.load_component_mesh(&def) else {
            return ComponentInfo::unloaded(class, file);
        };
        let mounts = scene.attachment_table();
        let points = scene_points(&scene);
        let mass = def.number("mass").unwrap_or_else(|| {
            let sum: f32 = scene
                .objects
                .iter()
                .flat_map(|o| &o.rigid_bodies)
                .map(|rb| rb.mass)
                .sum();
            if sum > 0.0 {
                sum
            } else if points.is_empty() {
                7.0
            } else {
                let (mn, mx) = bounds(&points);
                ra2fmt::component::zero_mass_fill(mn, mx)
            }
        });
        ComponentInfo {
            class: class.to_string(),
            file: file.to_string(),
            name: def.name.clone(),
            base: def.class.clone(),
            def: Some(def),
            mounts,
            mass,
            has_hull: !points.is_empty(),
            loaded: true,
        }
    }

    /// Parse a definition's mesh (`.gmf`), resolved case-insensitively under
    /// `Components/`.
    pub fn load_component_mesh(&self, def: &ComponentDef) -> Option<GmiScene> {
        let path = self.component_mesh_path(def)?;
        let bytes = std::fs::read(path).ok()?;
        ra2fmt::gmi::parse(&bytes).ok()
    }

    /// Where a definition's `.gmf` lives, resolved case-insensitively under
    /// `Components/`. The preview renderer needs the directory to resolve the
    /// mesh's own textures.
    pub fn component_mesh_path(&self, def: &ComponentDef) -> Option<PathBuf> {
        ra2fmt::find_ci(&self.root.join("Components"), &def.mesh_rel_path())
    }

    /// Parse a `.bot` file.
    pub fn read_bot(path: &Path) -> Result<Bot, LibraryError> {
        let bytes = std::fs::read(path)
            .map_err(|e| LibraryError::Io(format!("{}: {e}", path.display())))?;
        ra2fmt::bot::parse(&bytes)
            .map_err(|e| LibraryError::Parse(format!("{}: {e}", path.display())))
    }
}

fn bounds(points: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for p in points {
        for i in 0..3 {
            mn[i] = mn[i].min(p[i]);
            mx[i] = mx[i].max(p[i]);
        }
    }
    (mn, mx)
}

/// Every point of every mesh object, transformed into the component's space —
/// the same set the simulator's `hull_points` is built from.
pub fn scene_points(scene: &GmiScene) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    for o in &scene.objects {
        let Some(mesh) = &o.mesh else { continue };
        if mesh.points.is_empty() {
            continue;
        }
        let world = tm_mat4(&o.transform);
        out.extend(
            mesh.points
                .iter()
                .map(|p| world.transform_point3(Vec3::from(*p)).to_array()),
        );
    }
    out
}

fn read_edges(dir: &Path) -> Vec<EdgeStyle> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut style = EdgeStyle {
            name: String::new(),
            file: String::new(),
            preview: None,
            length_ratio: 0.0,
            width: 0.0,
            path: path.clone(),
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim().to_ascii_lowercase().as_str() {
                "filename" => {
                    style.file = value.clone();
                    style.name = value;
                }
                "preview" => style.preview = Some(value),
                key if key.starts_with("length ratio") => {
                    style.length_ratio = value.parse().unwrap_or(0.0)
                }
                key if key.starts_with("width") => style.width = value.parse().unwrap_or(0.0),
                _ => {}
            }
        }
        if !style.name.is_empty() {
            out.push(style);
        }
    }
    out.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn game_root() -> Option<PathBuf> {
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

    #[test]
    fn the_armour_table_is_the_shipped_one() {
        let Some(root) = game_root() else { return };
        let lib = GameLibrary::open(&root).unwrap();
        assert_eq!(lib.armour().len(), 4);
        let plastic = lib.armour_type(0).unwrap();
        assert_eq!((plastic.name.as_str(), plastic.weight), ("Plastic", 8.0));
        assert_eq!(plastic.hardness, 80.0);
        let steel = lib.armour_type(2).unwrap();
        assert_eq!((steel.weight, steel.hardness), (20.0, 175.0));
        let titanium = lib.armour_type(3).unwrap();
        assert_eq!((titanium.weight, titanium.hardness), (16.0, 150.0));
    }

    #[test]
    fn a_component_loads_with_its_mounts_and_mass() {
        let Some(root) = game_root() else { return };
        let lib = GameLibrary::open(&root).unwrap();
        let wheel = lib.component_info("Wheel", "Components\\miniwheel.txt");
        assert!(wheel.loaded, "the mini wheel should load");
        assert_eq!(wheel.base, "Wheel");
        assert!(
            (wheel.mass - 8.0).abs() < 0.5,
            "the mini wheel is the 8 kg anchor, got {}",
            wheel.mass
        );
        assert!(wheel.has_hull);
        let motor = lib.component_info("SpinMotor", "Components\\ztek1.txt");
        assert!((motor.mass - 25.0).abs() < 0.01, "Z-Tek is 25 kg");
        assert!(
            motor
                .mounts
                .values()
                .any(|m| m.types.iter().any(|t| t == "axle_m")),
            "the angle motor should carry an axle_M point"
        );
        // The chassis and SmartZones are not loaded by the renderer, and the
        // workshop mirrors that rule.
        assert!(
            !lib.component_info("SmartZone", "Components\\Styles\\smartzone2.txt")
                .loaded
        );
    }

    #[test]
    fn the_paint_libraries_are_the_games_own_file_lists() {
        let Some(root) = game_root() else { return };
        let lib = GameLibrary::open(&root).unwrap();
        assert!(lib.textures().find("aluminum_horiz.bmp").is_some());
        assert!(lib.decals().find("Gear.tga").is_some());
        let rivets = lib
            .edges()
            .iter()
            .find(|e| e.name == "rivets.tga")
            .expect("the shipped rivets edge style");
        assert_eq!(rivets.length_ratio, 0.1);
        assert_eq!(rivets.width, 5.0);
        assert_eq!(rivets.preview.as_deref(), Some("rivets_preview.bmp"));
        // `CustomTextures.txt` is a note, not an entry.
        assert!(lib.custom_textures().find("CustomTextures.txt").is_none());
    }

    /// A custom texture is any image file in the folder; the list is the
    /// directory, not an index, and the file loads by its list row name.
    #[test]
    fn a_custom_texture_is_listed_and_loads_by_name() {
        use crate::workshop::paint::{Skin, Texture};

        let dir = std::env::temp_dir().join(format!("ra2-custom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let skin = Skin::new(2, 2, [7, 8, 9, 255]);
        std::fs::write(dir.join("MyPaint.tga"), skin.to_tga()).unwrap();
        std::fs::write(dir.join("notes.txt"), b"not an image").unwrap();
        let lib = TextureLibrary::scan(&dir);
        assert_eq!(lib.names(), vec!["MyPaint.tga"]);
        let entry = lib.find("mypaint.tga").expect("case-insensitive lookup");
        let texture = Texture::load(&entry.path).unwrap();
        assert_eq!((texture.width, texture.height), (2, 2));
        assert_eq!(texture.pixel(1, 1), [7, 8, 9, 255]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
