//! Render every tab of the robot workshop — by *clicking* them.
//!
//! ```text
//! cargo run -q -p ra2-render --example workshop -- [out_dir]
//! ```
//!
//! The workshop is `botlabwin.gib` plus the fragment a tab loads into its
//! `content slot` (`ra2ui::workshop`), and this does not compose the tabs
//! directly: it opens the workshop on the tab the file itself has selected and
//! then, for each of the six, clicks the tab's own button at the centre of where
//! the layout put it. Every picture after the first is the result of a click
//! that was hit-tested against the placed tree.

use std::path::PathBuf;

use ra2_render::ui::{UiAssets, UiRenderer};
use ra2_render::Renderer;
use ra2fmt::gib::Block;
use ra2ui::workshop::{BotLab, STEPS, TABS};

fn game_root() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("RA2_GAME") {
        return Some(PathBuf::from(root));
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("local/game");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or("/tmp/workshop".into()));
    std::fs::create_dir_all(&out)?;
    let game = game_root().ok_or("no local/game (set RA2_GAME)")?;
    let (width, height) = (1280u32, 1005u32);
    let screen = [width as f32, height as f32];

    let mut lab = BotLab::open(&game.join("UI"))?;
    println!(
        "the file opens on {} with {} in {}",
        lab.tab,
        lab.content_root().map(|w| w.name()).unwrap_or(""),
        ra2ui::workshop::CONTENT_SLOT
    );

    for tab in &TABS {
        // Where the layout put this tab's button, and a click at its centre.
        let (x, y) = {
            let placed = lab.placements(screen, None);
            let button = placed
                .iter()
                .flat_map(|p| p.walk())
                .find(|p| p.widget.name() == tab.button)
                .unwrap_or_else(|| panic!("no button called {}", tab.button));
            (
                button.frame.x + button.frame.w / 2.0,
                button.frame.y + button.frame.h / 2.0,
            )
        };
        let hit = lab.click(x, y, screen, None)?;
        assert_eq!(
            hit.map(|t| t.button),
            Some(tab.button),
            "a click at {x},{y} should select {}",
            tab.button
        );

        // The palette's grid says what fills it — *"fill with components in the
        // current category"* — and the categories are the game's own component
        // definitions, so fill it before drawing.
        if tab.button == "components button" {
            let filled = fill_components_grid(&mut lab, &game)?;
            println!("  filled the components grid with {filled} cells from Components.cfz");
        }

        let mut assets = UiAssets::open(&game)?;
        let placed = lab.placements(screen, Some(assets.faces()));
        let placed = placed.into_iter().collect::<Vec<_>>();
        let scene = assets.scene(&placed, [width, height]);
        let renderer = Renderer::new()?;
        let ui = UiRenderer::new(renderer.device());
        let pixels = ui.render(renderer.device(), renderer.queue(), &scene)?;
        let name = tab.button.trim_end_matches(" button").replace(' ', "_");
        let path = out.join(format!("{name}.png"));
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!(
            "  clicked {:<20} at {:>7.1},{:<7.1} -> {:<44} {} in {}",
            tab.button,
            x,
            y,
            tab.fragment,
            lab.content_root().map(|w| w.name()).unwrap_or(""),
            path.display()
        );
    }
    // The chassis tab's own steps: a click on `structure design button` or
    // `armor button` replaces the tab's screen, and one of them is not where its
    // own label is (the `structure note` label is drawn over the buttons), so the
    // point is found by hit-testing rather than by taking the button's centre.
    lab.select("chassis button")?;
    for step in &STEPS {
        let (x, y) = reachable(&lab, step.button, screen);
        assert!(lab.click(x, y, screen, None)?.is_none());
        assert_eq!(lab.step, Some(step.button));
        let mut assets = UiAssets::open(&game)?;
        let placed = lab.placements(screen, Some(assets.faces()));
        let placed = placed.into_iter().collect::<Vec<_>>();
        let scene = assets.scene(&placed, [width, height]);
        let renderer = Renderer::new()?;
        let ui = UiRenderer::new(renderer.device());
        let pixels = ui.render(renderer.device(), renderer.queue(), &scene)?;
        let name = step.button.trim_end_matches(" button").replace(' ', "_");
        let path = out.join(format!("{name}.png"));
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!(
            "  stepped  {:<24} -> {:<36} {}",
            step.button,
            lab.content_root().map(|w| w.name()).unwrap_or(""),
            path.display()
        );
        // The armour grid's own `Notes` say to fill it with four armour cells,
        // and the four armour types are in the game's `Armor_Definitions.txt`.
        if step.button == "armor button" {
            let filled = fill_armour_grid(&mut lab, &game.join("UI"), &out)?;
            println!("  filled the armour grid with {filled} cells from Armor_Definitions.txt");
        }
        lab.select("chassis button")?;
    }
    Ok(())
}

/// Fill `components.gib`'s palette grid with the components of one category, from
/// the game's own `Components.cfz`.
fn fill_components_grid(
    lab: &mut BotLab,
    game: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let all = ra2ui::components::all(game)?;
    let (category, _) = ra2ui::components::CATEGORIES[0];
    let shown = ra2ui::components::in_category(&all, category);
    for block in lab.document.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(grid) = ra2ui::workshop::find_mut(root, "components grid") else {
            continue;
        };
        return Ok(ra2ui::components::fill_grid(grid, game, &shown));
    }
    Ok(0)
}

/// Fill `armor.gib`'s grid with one cell per armour type and render it.
fn fill_armour_grid(
    lab: &mut BotLab,
    ui: &std::path::Path,
    out: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let game = ui.parent().expect("the game root");
    let cfl = ra2fmt::cfl::Cfl::open_path(&game.join("RA2.cfz"))?;
    let text = cfl.get("Armor_Definitions.txt").ok_or("the armour table")?;
    let armour = ra2fmt::armor::parse(std::str::from_utf8(&text)?);
    let template = ra2ui::cells::template(ui, "botlab/armor_cell.gib")?;

    for block in lab.document.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(grid) = ra2ui::workshop::find_mut(root, "armor grid") else {
            continue;
        };
        let rows = ra2ui::cells::fill(grid, &template, armour.len());
        let bars = grid.children.len() - armour.len();
        for (cell, def) in grid.children[bars..].iter_mut().zip(&armour) {
            ra2ui::cells::bind_art(cell, "armor img", &def.texture);
            ra2ui::cells::bind_prose(cell, &def.description);
            ra2ui::cells::bind_value(cell, "weight value", &format!("{}", def.weight as i32));
        }
        let path = out.join("armor_filled.png");
        let mut assets = UiAssets::open(game)?;
        let placed = lab
            .placements([1280.0, 1005.0], Some(assets.faces()))
            .into_iter()
            .collect::<Vec<_>>();
        let scene = assets.scene(&placed, [1280, 1005]);
        let renderer = Renderer::new()?;
        let ui_renderer = UiRenderer::new(renderer.device());
        let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
        image::RgbaImage::from_raw(1280, 1005, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!("  wrote {}", path.display());
        return Ok(rows);
    }
    Ok(0)
}

/// A point inside `name` that a click reaches it at — found by hit-testing,
/// because a label can be drawn over a button.
fn reachable(lab: &BotLab, name: &str, screen: [f32; 2]) -> (f32, f32) {
    let placed = lab.placements(screen, None);
    let frame = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == name)
        .unwrap_or_else(|| panic!("no widget called {name}"))
        .frame;
    for iy in 1..7 {
        for ix in 1..9 {
            let x = frame.x + frame.w * ix as f32 / 9.0;
            let y = frame.y + frame.h * iy as f32 / 7.0;
            let Some(path) = ra2ui::input::hit(&placed, x, y) else {
                continue;
            };
            let mut placement = &placed[path[0]];
            let mut found = placement.widget.name() == name;
            for index in &path[1..] {
                placement = &placement.children[*index];
                found |= placement.widget.name() == name;
            }
            if found {
                return (x, y);
            }
        }
    }
    panic!("no click reaches {name}");
}
