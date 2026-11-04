//! The league standings: `hq_league.gib`'s grid and `season_results.gib`'s.
//!
//! Both screens state their own content. The league grid says *"fill with
//! league_cell.gib with the teams in order of rank"*; the season-results grid
//! says *"fill with sresults_cell.gib just like league standings"*. So the two
//! are the same list with two cell templates, and the order is the rank order.
//!
//! ## Where a row's values come from
//!
//! The **identity** is shipped. The game's team manager loads two tables —
//! `FUN_00502150` reads `Teams\Teams.txt` first and `AI\Teams.txt` after it — and
//! a team record's first three lines are its name, motto and logo
//! ([`ra2fmt::team`]). The player's file has sixteen slots and one named team in
//! the shipped build; the AI table has fifteen. Together they are the sixteen
//! rows the grids declare (`Rows 16`).
//!
//! The **rank and points are not in those files**. The league window's fill
//! (`FUN_00500230`, reached from the tab's refresh `FUN_004fe0c0`) creates a
//! `LeagueCell` per team (`FUN_004fcf50`, its constructor names `league team
//! name`, `motto`, `points`, `rank`, `league team logo`) and hands it the live
//! league's points for that team; the row's rank is its position in that live
//! table, labelled `i + 1`, which is why the screen's note says *"in order of
//! rank"*. The same shape fills the season-results grid (`FUN_004e8860`,
//! `SeasonResultCell` at `FUN_004e8020`).
//!
//! The shipped records are not a points table: the field the constructor sets to
//! `10000.0` (`Team + 0xb8`) is written as `10000` for the player and `100000`
//! for the AI teams, but it is the team's money; the fifteen numbers per record
//! (`Team + 0x5c`) are all zero in both shipped files. So [`roster`] is the
//! identity and [`table`] takes the points as an explicit input — the live
//! league's table. [`empty`] is the explicit empty source: the shipped roster
//! with no points, which is the shape a fresh season has.

use std::path::{Path, PathBuf};

use ra2fmt::gib::Widget;

pub use ra2fmt::team::Team;

/// The league screen and its grid.
pub const LEAGUE_SCREEN: &str = "teamhq/hq_league.gib";
pub const LEAGUE_GRID: &str = "league grid";
/// The cell template the league grid's own note names.
pub const LEAGUE_TEMPLATE: &str = "teamhq/league_cell.gib";

/// The season-results screen and its grid.
pub const RESULTS_SCREEN: &str = "teamhq/season_results.gib";
pub const RESULTS_GRID: &str = "results grid";
/// The cell template the results grid's own note names.
pub const RESULTS_TEMPLATE: &str = "teamhq/sresults_cell.gib";

/// The children a league cell binds, by the names the template's own `Notes`
/// give them.
pub const NAME: &str = "league team name";
pub const MOTTO: &str = "motto";
pub const POINTS: &str = "points";
pub const RANK: &str = "rank";
pub const LOGO: &str = "league team logo";

/// One row of the standings: a team, its points and the rank it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Standing {
    /// 1 is the top of the table. The screen's own note is *"in order of rank"*,
    /// and the game labels the i-th live row `i + 1`.
    pub rank: i32,
    /// The live league's points for this team. Zero is a real value at the start
    /// of a season, not a placeholder for "unknown".
    pub points: i32,
    pub team: Team,
}

/// The shipped team tables: the player's `Teams/Teams.txt` first, then the
/// fifteen AI teams in `AI/Teams.txt`, which is the order the game's manager
/// loads them (`FUN_00502150`). The manager has sixteen slots; the AI table
/// fills the player's empty slots, so a full table is sixteen teams.
pub fn roster(game: &Path) -> Result<Vec<Team>, String> {
    let player = ra2fmt::team::read(&ra2fmt::saves::resolve(game, "Teams/Teams.txt"))?;
    let ai = ra2fmt::team::read(&ra2fmt::saves::resolve(game, "AI/Teams.txt"))?;
    let mut teams = player;
    teams.extend(ai);
    teams.truncate(SLOTS);
    Ok(teams)
}

/// The manager's sixteen team slots, which is also the grids' `Rows 16`.
pub const SLOTS: usize = 16;

/// Build the standings from a roster and the live league's points.
///
/// Points decide the order, highest first; equal points keep the roster's own
/// order (the sort is stable), so a fresh season with no points shows the roster
/// in the order the manager loaded it. The rank is the row's position, which is
/// the rule the screen's note and the game's own label both state.
pub fn table(teams: &[Team], points: &[i32]) -> Vec<Standing> {
    let mut order: Vec<usize> = (0..teams.len()).collect();
    order.sort_by(|&a, &b| {
        let a_points = points.get(a).copied().unwrap_or(0);
        let b_points = points.get(b).copied().unwrap_or(0);
        b_points.cmp(&a_points).then(a.cmp(&b))
    });
    order
        .into_iter()
        .enumerate()
        .map(|(index, team)| Standing {
            rank: index as i32 + 1,
            points: points.get(team).copied().unwrap_or(0),
            team: teams[team].clone(),
        })
        .collect()
}

/// The shipped roster with the explicit empty points table: no points and the
/// manager's own order. This is what a season looks like before it is played,
/// and what a caller has until the live table exists.
pub fn empty(game: &Path) -> Result<Vec<Standing>, String> {
    let teams = roster(game)?;
    Ok(table(&teams, &vec![0; teams.len()]))
}

/// Fill the league grid with one cell per standing, at the grid's own geometry.
pub fn fill_league(grid: &mut Widget, template: &ra2fmt::gib::Gib, rows: &[Standing]) -> usize {
    fill(grid, template, rows, true)
}

/// Fill the season-results grid. Its cell has no motto child, so only the fields
/// the template has are bound.
pub fn fill_results(grid: &mut Widget, template: &ra2fmt::gib::Gib, rows: &[Standing]) -> usize {
    fill(grid, template, rows, false)
}

fn fill(
    grid: &mut Widget,
    template: &ra2fmt::gib::Gib,
    rows: &[Standing],
    with_motto: bool,
) -> usize {
    let made = crate::cells::fill(grid, template, rows.len());
    let bars = grid.children.len() - made;
    for (cell, row) in grid.children[bars..].iter_mut().zip(rows) {
        crate::cells::bind_value(cell, RANK, &row.rank.to_string());
        crate::cells::bind_value(cell, NAME, &row.team.name);
        crate::cells::bind_value(cell, POINTS, &row.points.to_string());
        crate::cells::bind_art(cell, LOGO, &row.team.logo);
        if with_motto {
            crate::cells::bind_value(cell, MOTTO, &row.team.motto);
        }
    }
    made
}

/// The game's own path form (`Teams\orange_skull.bmp`) as a path under a game
/// directory — for a caller that wants to open the row's logo.
pub fn logo_path(game: &Path, logo: &str) -> PathBuf {
    let mut path = game.to_path_buf();
    for part in logo.split('\\') {
        path.push(part);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::gib::Block;

    fn game_dir() -> Option<PathBuf> {
        ra2fmt::saves::isolate_for_tests();
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

    fn load(game: &Path, screen: &str) -> ra2fmt::gib::Gib {
        crate::workshop::load(&game.join("UI"), screen).unwrap()
    }

    fn root(gib: &mut ra2fmt::gib::Gib) -> &mut Widget {
        gib.blocks
            .iter_mut()
            .find_map(|b| match b {
                Block::Widget(w) => Some(w),
                _ => None,
            })
            .unwrap()
    }

    /// The two tables are the sixteen teams the grids declare: the player's one
    /// named team plus the fifteen AI teams. Empty slots do not become rows.
    #[test]
    fn the_roster_is_the_games_sixteen_named_teams() {
        let Some(game) = game_dir() else { return };
        let teams = roster(&game).unwrap();
        assert_eq!(teams.len(), 16);
        assert_eq!(teams[0].name, "The Rookies");
        assert_eq!(teams[0].motto, "Just getting started...");
        assert_eq!(teams[0].logo, "Teams\\orange_skull.bmp");
        assert_eq!(teams[1].name, "Red Zone");
        assert_eq!(teams[1].motto, "\"Do NOT enter the Red Zone!\"");
        assert_eq!(teams[1].logo, "AI\\red_zone.bmp");
        assert_eq!(teams[15].name, "The Scrappers");
        // The logo is a game path, not a host path.
        assert!(logo_path(&game, &teams[1].logo).is_file());
    }

    /// The order is the live points order; equal points keep the roster order,
    /// and the rank is the row's position.
    #[test]
    fn points_decide_the_rank_and_ties_keep_the_roster_order() {
        let teams = vec![
            Team {
                name: "A".into(),
                motto: String::new(),
                logo: "a.bmp".into(),
                ..Team::default()
            },
            Team {
                name: "B".into(),
                motto: String::new(),
                logo: "b.bmp".into(),
                ..Team::default()
            },
            Team {
                name: "C".into(),
                motto: String::new(),
                logo: "c.bmp".into(),
                ..Team::default()
            },
        ];
        let rows = table(&teams, &[5, 9, 5]);
        assert_eq!(
            rows.iter()
                .map(|r| (r.rank, r.points, r.team.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, 9, "B"), (2, 5, "A"), (3, 5, "C")]
        );
    }

    /// `hq_league.gib`'s own grid states the geometry and the note, and a table
    /// fills it one cell per row with the values bound into the children the
    /// cell template names.
    #[test]
    fn the_league_grid_is_filled_from_the_standings() {
        let Some(game) = game_dir() else { return };
        let mut gib = load(&game, LEAGUE_SCREEN);
        let team = roster(&game).unwrap();
        // The live points table is the one thing that does not ship. This is an
        // explicit source: the player's team leads, the rest are tied.
        let mut points = vec![0; team.len()];
        points[0] = 42;
        points[1] = 7;
        let rows = table(&team, &points);

        let grid = crate::workshop::find_mut(root(&mut gib), LEAGUE_GRID).expect("league grid");
        assert_eq!(
            grid.notes(),
            "fill with league_cell.gib with the teams in order of rank"
        );
        let geometry = crate::Grid::from_widget(grid).unwrap();
        assert_eq!(
            (geometry.rows, geometry.columns, geometry.row_height),
            (16, 1, 70.0)
        );
        assert_eq!(geometry.col_widths[0], 388.0);
        assert_eq!(grid.children.len(), 2, "the grid's own two bars");

        let template = crate::cells::template(&game.join("UI"), LEAGUE_TEMPLATE).unwrap();
        assert_eq!(fill_league(grid, &template, &rows), rows.len());

        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == LEAGUE_GRID)
            .unwrap();
        assert_eq!(grid.children.len(), 2 + rows.len());
        // The scroll bar stands flush inside the right edge of the scroller body
        // (capture: bar at x 924..940, grid from 293), not at the authored x 591.
        let scroller = grid
            .attachments
            .iter()
            .find(|a| a.attachment.kind == ra2fmt::gib::AttachmentKind::Scroller)
            .expect("the grid's scroller");
        let bar = scroller
            .body
            .walk()
            .find(|p| p.widget.kind == ra2fmt::gib::WidgetKind::ScrollBar)
            .expect("the scroll bar");
        assert_eq!(bar.frame.right(), scroller.body.frame.right());
        assert_eq!(grid.frame.w, scroller.body.frame.w);
        for (i, cell) in grid.children.iter().skip(2).enumerate() {
            // `BorderWidth 1` insets the cell inside its slot (live `teamhq-standings`).
            assert_eq!(cell.frame.x, grid.frame.x + 1.0, "row {i}");
            assert_eq!(
                cell.frame.y,
                grid.frame.y + 70.0 * i as f32 + 1.0,
                "row {i}"
            );
            assert_eq!(
                cell.frame.w, grid.frame.w,
                "row {i}: the cell runs to the scroll bar"
            );
            assert_eq!(cell.frame.h, 68.0, "row {i}");
            assert_eq!(cell.widget.name(), "league cell split");
        }

        // The top row is the player's team: rank 1, 42 points, its own motto and
        // logo. Read the children by the names the template's notes give; a
        // `TextBox` keeps its value in its own `Text` child.
        let top = &grid.children[2];
        let text = |name: &str| {
            top.walk()
                .find(|p| p.widget.name() == name)
                .and_then(|p| {
                    p.widget
                        .children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                })
                .and_then(|c| c.field("_Text"))
                .map(|f| f.text())
                .unwrap_or("")
        };
        assert_eq!(text(RANK), "1");
        assert_eq!(text(NAME), "The Rookies");
        assert_eq!(text(MOTTO), "Just getting started...");
        assert_eq!(text(POINTS), "42");
        let logo = top
            .walk()
            .find(|p| p.widget.name() == LOGO)
            .expect("league team logo");
        assert_eq!(logo.widget.filename(), Some("Teams\\orange_skull.bmp"));

        // The next row is the AI team with the second-highest points.
        let second = &grid.children[3];
        let text = |name: &str| {
            second
                .walk()
                .find(|p| p.widget.name() == name)
                .and_then(|p| {
                    p.widget
                        .children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                })
                .and_then(|c| c.field("_Text"))
                .map(|f| f.text())
                .unwrap_or("")
        };
        assert_eq!(text(RANK), "2");
        assert_eq!(text(NAME), "Red Zone");
        assert_eq!(text(POINTS), "7");
    }

    /// `season_results.gib` is the same standings with `sresults_cell.gib`: its
    /// geometry is its own (`Rows 16`, `RowHeight 36`, `ColWidths 201`), and the
    /// cell has no motto child, so none is invented.
    #[test]
    fn the_season_results_grid_is_the_same_standings_without_a_motto() {
        let Some(game) = game_dir() else { return };
        let mut gib = load(&game, RESULTS_SCREEN);
        let teams = roster(&game).unwrap();
        let rows = table(&teams, &[0; 16]);

        let grid = crate::workshop::find_mut(root(&mut gib), RESULTS_GRID).expect("results grid");
        assert_eq!(
            grid.notes(),
            "fill with sresults_cell.gib just like league standings"
        );
        let geometry = crate::Grid::from_widget(grid).unwrap();
        assert_eq!(
            (geometry.rows, geometry.columns, geometry.row_height),
            (16, 1, 36.0)
        );
        assert_eq!(geometry.col_widths[0], 201.0);
        assert_eq!(grid.children.len(), 2, "the grid's own two bars");

        let template = crate::cells::template(&game.join("UI"), RESULTS_TEMPLATE).unwrap();
        let has_motto = template.blocks.iter().any(|b| match b {
            Block::Widget(w) => crate::workshop::find(w, MOTTO).is_some(),
            Block::Attachment(_) => false,
        });
        assert!(!has_motto, "sresults_cell has no motto child");
        assert_eq!(fill_results(grid, &template, &rows[..3]), 3);
        let placed = crate::resolve(&gib, crate::DESIGN_SCREEN);
        let grid = placed
            .iter()
            .flat_map(|p| p.walk())
            .find(|p| p.widget.name() == RESULTS_GRID)
            .unwrap();
        for (i, cell) in grid.children.iter().skip(2).enumerate() {
            assert_eq!(
                cell.frame.y,
                grid.frame.y + 36.0 * i as f32 + 1.0,
                "row {i}"
            );
            assert_eq!(cell.frame.w, grid.frame.w, "row {i}");
        }
        assert_eq!(
            grid.children[2]
                .walk()
                .find(|p| p.widget.name() == NAME)
                .and_then(|p| {
                    p.widget
                        .children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                })
                .and_then(|c| c.field("_Text"))
                .map(|f| f.text()),
            Some("The Rookies")
        );
        assert_eq!(
            grid.children[2]
                .walk()
                .find(|p| p.widget.name() == RANK)
                .and_then(|p| {
                    p.widget
                        .children
                        .iter()
                        .find(|c| c.kind == ra2fmt::gib::WidgetKind::Text)
                })
                .and_then(|c| c.field("_Text"))
                .map(|f| f.text()),
            Some("1")
        );
    }
}
