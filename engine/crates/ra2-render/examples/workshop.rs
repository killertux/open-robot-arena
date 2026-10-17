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
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("workshop"));
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
            let (category, filled) = fill_components_grid(&mut lab, &game)?.unwrap_or(("", 0));
            println!(
                "  filled the components grid with {filled} {category} cells from Components.cfz"
            );
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
    // The palette's six categories, each a click and a refill.
    render_categories(&mut lab, &game, &out, screen)?;
    // And screens outside the workshop, to the same two rules.
    render_events(&game.join("UI"), &game, &out)?;
    render_arenas(&game.join("UI"), &game, &out)?;
    render_standings(&game.join("UI"), &game, &out)?;
    render_match_results(&game.join("UI"), &game, &out)?;
    Ok(())
}

/// Compose `hq_league.gib` and `season_results.gib`, fill each grid with the
/// game's own team roster, and render them.
///
/// The identity is shipped (`Teams/Teams.txt` + `AI/Teams.txt`); the points and
/// rank are the live league's and never ship, so the picture uses the explicit
/// empty table — a fresh season: no points, the roster in the manager's order.
fn render_standings(
    ui: &std::path::Path,
    game: &std::path::Path,
    out: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let rows = ra2ui::standings::empty(game)?;
    let mut filled = 0;
    for (screen, grid, template) in [
        (
            ra2ui::standings::LEAGUE_SCREEN,
            ra2ui::standings::LEAGUE_GRID,
            ra2ui::standings::LEAGUE_TEMPLATE,
        ),
        (
            ra2ui::standings::RESULTS_SCREEN,
            ra2ui::standings::RESULTS_GRID,
            ra2ui::standings::RESULTS_TEMPLATE,
        ),
    ] {
        let mut gib = ra2ui::workshop::load(ui, screen)?;
        ra2ui::inline_proxies_in(&mut gib, ui);
        let cell = ra2ui::cells::template(ui, template)?;
        for block in gib.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            let Some(list) = ra2ui::workshop::find_mut(root, grid) else {
                continue;
            };
            filled = if screen == ra2ui::standings::LEAGUE_SCREEN {
                ra2ui::standings::fill_league(list, &cell, &rows)
            } else {
                ra2ui::standings::fill_results(list, &cell, &rows)
            };
            break;
        }

        // Read the first row back by the names the cell template's notes give,
        // before drawing it: the measurement is the text, not only the cells.
        if let Some(Block::Widget(root)) = gib.blocks.iter().find(|b| matches!(b, Block::Widget(_)))
        {
            if let Some(list) = ra2ui::workshop::find(root, grid) {
                if let Some(row) = list.children.iter().find(|c| c.name().contains("cell")) {
                    println!(
                        "  {screen}: row 1 = rank {}, {}, {} points, {} ({})",
                        cell_value(row, ra2ui::standings::RANK),
                        cell_value(row, ra2ui::standings::NAME),
                        cell_value(row, ra2ui::standings::POINTS),
                        cell_value(row, ra2ui::standings::MOTTO),
                        cell_art(row, ra2ui::standings::LOGO)
                    );
                }
            }
        }

        let mut assets = UiAssets::open(game)?;
        let placed = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()));
        let placed = placed.into_iter().collect::<Vec<_>>();
        let scene = assets.scene(&placed, [1280, 1005]);
        let renderer = Renderer::new()?;
        let ui_renderer = UiRenderer::new(renderer.device());
        let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
        let name = if screen == ra2ui::standings::LEAGUE_SCREEN {
            "hq_league.png"
        } else {
            "season_results.png"
        };
        let path = out.join(name);
        image::RgbaImage::from_raw(1280, 1005, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!(
            "  filled {screen} with {filled} rows and rendered {}",
            path.display()
        );
    }
    Ok(filled)
}

/// Compose `common/matchresults.gib` and fill its four slots with a match.
///
/// The real source is the live match and does not ship; this is the shape the
/// screen needs, filled here with two explicit results so the row's text can be
/// read and the labels checked by eye. The art is the game's own.
fn render_match_results(
    ui: &std::path::Path,
    game: &std::path::Path,
    out: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut gib = ra2ui::workshop::load(ui, "common/matchresults.gib")?;
    ra2ui::inline_proxies_in(&mut gib, ui);
    let template = ra2ui::cells::template(ui, ra2ui::results::TEMPLATE)?;
    let results = [
        ra2ui::results::MatchResult {
            team_logo: "Teams\\orange_skull.bmp".into(),
            bot_preview: "exhibition\\sample_bot.bmp".into(),
            points: 248,
            bot_name: "The Wrangler".into(),
            team_name: "TEAM STINKBOMBS".into(),
            immobilized: true,
            winner: false,
        },
        ra2ui::results::MatchResult {
            team_logo: "AI\\red_zone.bmp".into(),
            bot_preview: "exhibition\\sample_bot.bmp".into(),
            points: 125,
            bot_name: "Red Menace".into(),
            team_name: "Red Zone".into(),
            immobilized: false,
            winner: true,
        },
    ];
    let mut filled = 0;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        filled = ra2ui::results::fill(root, &template, &results);
        break;
    }
    println!("  matchresults: read back the two rows from the filled cells");
    if let Some(Block::Widget(root)) = gib.blocks.iter().find(|b| matches!(b, Block::Widget(_))) {
        for slot in ra2ui::results::SLOTS.iter().take(results.len()) {
            let Some(slot) = ra2ui::workshop::find(root, slot) else {
                continue;
            };
            let Some(cell) = slot
                .children
                .iter()
                .find(|c| c.name() == ra2ui::results::CELL)
            else {
                continue;
            };
            println!(
                "    {}: {}, {}, {} points",
                slot.name(),
                cell_value(cell, ra2ui::results::BOT_NAME),
                cell_value(cell, ra2ui::results::TEAM_NAME),
                cell_value(cell, ra2ui::results::POINTS)
            );
        }
    }

    let mut assets = UiAssets::open(game)?;
    let placed = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()));
    let placed = placed.into_iter().collect::<Vec<_>>();
    let scene = assets.scene(&placed, [1280, 1005]);
    let renderer = Renderer::new()?;
    let ui_renderer = UiRenderer::new(renderer.device());
    let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
    let path = out.join("matchresults.png");
    image::RgbaImage::from_raw(1280, 1005, pixels)
        .ok_or("readback")?
        .save(&path)?;
    println!(
        "  filled {} match-result slots and rendered {}",
        filled,
        path.display()
    );
    Ok(filled)
}

/// A `TextBox` row's value, from its own `Text` child.
fn cell_value(row: &ra2fmt::gib::Widget, name: &str) -> String {
    ra2ui::workshop::find(row, name)
        .and_then(|w| {
            w.children
                .iter()
                .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
        })
        .and_then(|c| c.field("_Text"))
        .map(|f| f.text().to_string())
        .unwrap_or_default()
}

/// A row's art file, from the child the template names.
fn cell_art(row: &ra2fmt::gib::Widget, name: &str) -> String {
    ra2ui::workshop::find(row, name)
        .and_then(|w| w.filename())
        .unwrap_or("")
        .to_string()
}

/// Compose `hq_events.gib`, fill its own grid with the game's own events, and
/// render it.
///
/// A screen outside the workshop, to show that the same two rules hold there: the
/// grid's `Notes` name the cell template (*"fill with this season's events
/// (event_cell.gib)"*), and the events are `Events.txt`.
fn render_events(
    ui: &std::path::Path,
    game: &std::path::Path,
    out: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut gib = ra2ui::workshop::load(ui, "teamhq/hq_events.gib")?;
    ra2ui::inline_proxies_in(&mut gib, ui);
    let template = ra2ui::workshop::load(ui, ra2ui::events::TEMPLATE)?;
    let events = ra2ui::events::all(game)?;
    let mut filled = 0;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(grid) = ra2ui::workshop::find_mut(root, ra2ui::events::GRID) else {
            continue;
        };
        filled = ra2ui::events::fill_grid(grid, game, &template, &events);
        break;
    }

    let mut assets = UiAssets::open(game)?;
    let placed = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()));
    let placed = placed.into_iter().collect::<Vec<_>>();
    let scene = assets.scene(&placed, [1280, 1005]);
    let renderer = Renderer::new()?;
    let ui_renderer = UiRenderer::new(renderer.device());
    let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
    let path = out.join("hq_events.png");
    image::RgbaImage::from_raw(1280, 1005, pixels)
        .ok_or("readback")?
        .save(&path)?;
    println!(
        "  filled the event grid with {filled} events and rendered {}",
        path.display()
    );
    Ok(filled)
}

/// Compose `exhibition/arena.gib`, fill its listbox with the game's own arena
/// names, and render it.
fn render_arenas(
    ui: &std::path::Path,
    game: &std::path::Path,
    out: &std::path::Path,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut gib = ra2ui::workshop::load(ui, "exhibition/arena.gib")?;
    ra2ui::inline_proxies_in(&mut gib, ui);
    let arenas = ra2ui::arenas::all(game)?;
    let mut filled = 0;
    for block in gib.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        let Some(list) = ra2ui::workshop::find_mut(root, ra2ui::arenas::LIST) else {
            continue;
        };
        filled = ra2ui::arenas::fill_list(list, &arenas);
        break;
    }

    let mut assets = UiAssets::open(game)?;
    let placed = ra2ui::resolve_with(&gib, [1280.0, 1005.0], Some(assets.faces()));
    let placed = placed.into_iter().collect::<Vec<_>>();
    let scene = assets.scene(&placed, [1280, 1005]);
    let renderer = Renderer::new()?;
    let ui_renderer = UiRenderer::new(renderer.device());
    let pixels = ui_renderer.render(renderer.device(), renderer.queue(), &scene)?;
    let path = out.join("arena_list.png");
    image::RgbaImage::from_raw(1280, 1005, pixels)
        .ok_or("readback")?
        .save(&path)?;
    println!(
        "  filled the arena list with {filled} names and rendered {}",
        path.display()
    );
    Ok(filled)
}

/// Fill `components.gib`'s palette grid with the components of one category, from
/// the game's own `Components.cfz`.
fn fill_components_grid(
    lab: &mut BotLab,
    game: &std::path::Path,
) -> Result<Option<(&'static str, usize)>, Box<dyn std::error::Error>> {
    let all = ra2ui::components::all(game)?;
    // `fill_selected` reads the category the screen itself has selected, so this
    // is the whole of the grid's instruction in one call.
    for block in lab.document.blocks.iter_mut() {
        let Block::Widget(root) = block else { continue };
        return Ok(ra2ui::components::fill_selected(root, game, &all));
    }
    Ok(None)
}

/// Walk the palette's six category buttons: a click selects one in its group, the
/// grid fills for it, and the screen renders.
fn render_categories(
    lab: &mut BotLab,
    game: &std::path::Path,
    out: &std::path::Path,
    screen: [f32; 2],
) -> Result<(), Box<dyn std::error::Error>> {
    lab.select("components button")?;
    for (category, button) in ra2ui::components::CATEGORIES {
        // Selected the way a click on it would be — the same `select_option` the
        // click path calls — and not by clicking, because of what filling the
        // grid does to the buttons: see the note on `ra2ui::components`'s grid
        // and `docs/05`. Once the palette holds cells, the grid's own `Sel Bar`
        // covers the option group's row (`Sel Bar` is visible and 78×72 at the
        // grid's origin; the grid is later in the file than `comp upper grp` with
        // the same `ChildOrder -1`), so a click reaches the bar instead.
        for block in lab.document.blocks.iter_mut() {
            let Block::Widget(root) = block else { continue };
            ra2ui::input::select_option(root, ra2ui::components::GROUP, button);
            break;
        }
        let (shown, filled) = fill_components_grid(lab, game)?.unwrap_or(("", 0));
        assert_eq!(shown, category);

        let mut assets = UiAssets::open(game)?;
        let placed = lab.placements(screen, Some(assets.faces()));
        let placed = placed.into_iter().collect::<Vec<_>>();
        let scene = assets.scene(&placed, [1280, 1005]);
        let renderer = Renderer::new()?;
        let ui = UiRenderer::new(renderer.device());
        let pixels = ui.render(renderer.device(), renderer.queue(), &scene)?;
        let path = out.join(format!("components_{category}.png"));
        image::RgbaImage::from_raw(1280, 1005, pixels)
            .ok_or("readback")?
            .save(&path)?;
        println!(
            "  filled the palette with {filled:2} {category} and rendered {}",
            path.display()
        );
    }
    Ok(())
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
