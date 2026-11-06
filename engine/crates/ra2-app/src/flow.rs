//! The hand-offs between the shell and the live game modes.
//!
//! The windowed client boots the shell (`ra2_shell::Shell`) and a player walks
//! the shipped screens. Three of those screens are only the entry to a live
//! mode that this crate implements beside the shell:
//!
//! * the Team HQ's event bracket (`teamhq/event_bracket.gib`): its `play
//!   button` starts the event's match, and the round's results, repairs and
//!   award follow ([`crate::hq_mode::Session`]);
//! * the bot lab (`botlab/botlabwin.gib`), reached from the HQ's `workshop
//!   button`: the editable workshop ([`crate::build_mode::Builder`]);
//! * the exhibition setup's `start game button` ([`crate::play_mode`]).
//!
//! The functions here are the hand-offs, kept out of the window loop so the
//! route can be driven and tested headlessly. The career on disk (`Teams/`) is
//! the state the shell and the event flow share, as the original keeps one team
//! record for both.

use std::path::{Path, PathBuf};

use ra2_shell::Shell;

use crate::build_mode::Builder;
use crate::hq_mode;

/// The bracket screen the shell shows for an entered event.
pub const BRACKET_SCREEN: &str = "teamhq/event_bracket.gib";

/// Which robot cell of the Team HQ's inventory a point is on. The inventory
/// grid lays its cells out evenly (`ra2ui::hq`), down each column first, in
/// roster order; the cell index is the roster index.
pub fn inventory_cell_at(shell: &Shell, point: [f32; 2]) -> Option<usize> {
    if !shell
        .base_screen_id()
        .eq_ignore_ascii_case(ra2ui::hq::SCREEN)
        || shell.window_depth() > 0
    {
        return None;
    }
    let placed = ra2ui::resolve(shell.document(), shell.size());
    let grid = placed
        .iter()
        .flat_map(|p| p.walk())
        .find(|p| p.widget.name() == ra2ui::hq::ROBOT_GRID)?;
    if !grid.frame.contains_point(point[0], point[1]) {
        return None;
    }
    grid.children
        .iter()
        .filter(|cell| {
            let name = cell.widget.name();
            name != "Sel Bar" && name != "High Bar"
        })
        .position(|cell| cell.frame.contains_point(point[0], point[1]))
}

/// The `.bot` of roster entry `index` of the career's team.
pub fn inventory_robot_path(game: &Path, index: usize) -> Option<PathBuf> {
    let career = hq_mode::load_career(game).ok()?;
    // The inventory's cell `index` is robot slot `index` (empty cells have no robot).
    let slot = *career
        .player()
        .robots
        .iter()
        .find(|slot| **slot == index as i32)?;
    let path = hq_mode::player_robot_path(game, career.player_slot, slot);
    path.is_file().then_some(path)
}

/// The workshop for the HQ's `workshop button`: the inventory robot the player
/// selected, edited in place (`Back to HQ` writes it back to its team slot),
/// or a new robot from the stock chassis when none is selected.
pub fn open_workshop(game: &Path, robot: Option<usize>) -> Result<Builder, String> {
    let path = robot.and_then(|index| inventory_robot_path(game, index));
    // An empty cell opens the original's blank robot (no chassis shape yet); a robot opens as it is.
    let mut builder = match path {
        Some(_) => Builder::open(game, path.as_deref())?,
        None => Builder::open_blank(game, crate::SHELL_SIZE)?,
    };
    builder.apply_debug_panel_default()?;
    match path {
        Some(path) => builder.set_team_target(path),
        None => {
            if let Ok(career) = hq_mode::load_career(game) {
                builder.set_team_new(career.player_slot, robot);
            }
        }
    }
    Ok(builder)
}

/// The workshop button's guard (`FUN_004fee90`): the robot entered in the
/// team's current event (`Robot In Event`) cannot be edited; the HQ shows
/// `Robot Arena 2` / this message instead of the lab.
pub const ROBOT_IN_EVENT: &str = "This robot is currently entered in an event and is not editable.";

/// Whether inventory robot `index` is the one entered in the career's
/// current event.
pub fn robot_in_event(game: &Path, index: usize) -> bool {
    let Ok(career) = hq_mode::load_career(game) else {
        return false;
    };
    career.entered_event().is_some()
        && career.player().event_robot == index as i32
        && career.player().robots.contains(&(index as i32))
}

/// What a click on the Team HQ's inventory does besides the shell's own step: the clicked
/// cell becomes the selection (the cell number is the robot slot), a selection whose robot is
/// gone (scrapped) is dropped, and the `scrap button` asks its question. `cell` is
/// [`inventory_cell_at`] taken before the click.
pub fn inventory_click(
    shell: &mut Shell,
    pick: &mut Option<usize>,
    cell: Option<usize>,
    step: &ra2_shell::Step,
) -> Result<(), String> {
    // The grid's own selection is the pick: a robot's cell or a `New Robot` cell (Workshop and Import
    // work on it), none after a refill (a scrap, an import).
    match shell.inventory_selection() {
        Some(selection) => *pick = selection,
        None if cell.is_some() => *pick = cell,
        None => {}
    }
    if step.hit_widget() == Some("scrap button")
        && shell
            .base_screen_id()
            .eq_ignore_ascii_case(ra2ui::hq::SCREEN)
        && shell.window_depth() == 0
    {
        scrap_prompt(shell, *pick)?;
    }
    if shell
        .base_screen_id()
        .eq_ignore_ascii_case(ra2ui::hq::SCREEN)
        && shell.window_depth() == 0
    {
        // `FUN_00500690` / `FUN_00500860`: the selected cell's export or import window. A dimmed
        // button never reaches here with a selection it cannot use (`ask_export` wants a robot,
        // `ask_import` an empty cell).
        match (step.hit_widget(), *pick) {
            (Some("export button"), Some(index)) => shell.ask_export(index)?,
            (Some("import button"), Some(index)) => shell.ask_import(index)?,
            _ => {}
        }
    }
    Ok(())
}

/// The inventory's `scrap button` (`FUN_004ff0c0`) with robot `pick` selected: the
/// robot entered in the current event only gets the OK box; any other asks
/// `Are you sure you want to scrap <name>?` (Yes/No). Nothing selected: nothing.
pub fn scrap_prompt(shell: &mut Shell, pick: Option<usize>) -> Result<(), String> {
    let Some(index) = pick else { return Ok(()) };
    let Some(name) = shell.hq_robot_name(index).map(str::to_string) else {
        return Ok(());
    };
    if robot_in_event(shell.game(), index) || shell.entered_event_slot() == Some(index) {
        return shell.show_message("Robot Arena 2", ra2sim::career::SCRAP_IN_EVENT);
    }
    shell.ask_scrap(index, &name)
}

/// The workshop is being left (`Back to HQ`, `Main Menu`): write a team robot
/// back to its slot and refresh the shell's HQ data, so the inventory, the join
/// window and the exhibition pickers show the edited robot.
pub fn close_workshop(builder: &mut Builder, shell: &mut Shell) -> Result<Option<PathBuf>, String> {
    let saved = builder.save_to_team()?;
    if let Some(view) = hq_mode::shell_view(shell.game()) {
        shell.set_hq_view(view);
    }
    Ok(saved)
}

/// The control that took the workshop's own shell out of the lab, so the
/// application's shell (still on the lab screen) can follow it: the lab's
/// `back button` (`Back to HQ`, `0x43b6d0`: the Team HQ window) or the bottom
/// bar's `MainMenu button`.
pub fn lab_exit_control(workshop_shell: &Shell) -> Option<String> {
    let screen = workshop_shell.base_screen_id();
    if screen.eq_ignore_ascii_case(ra2ui::hq::SCREEN) {
        Some("back button".to_string())
    } else if screen.eq_ignore_ascii_case(ra2_shell::nav::BOOT_SCREEN) {
        Some("MainMenu button".to_string())
    } else {
        None
    }
}

/// Whether a click on `screen` hit a control the executable binds to leaving
/// the game (the main menu's `Quit button`, handler `0x486c30`). The shell
/// itself has nothing to show for it; the application exits.
pub fn quits(screen: &str, step: &ra2_shell::Step) -> bool {
    step.hit_widget()
        .and_then(|name| ra2_shell::bindings::lookup(screen, name))
        .is_some_and(|binding| binding.action == ra2_shell::bindings::Action::Quit)
}

/// The event match's length when the session never touched the exhibition
/// time: the match settings constructor's 3 minutes (`0x41276d`,
/// [`ra2sim::r#match::DEFAULT_MATCH_SECONDS`]).
pub const EVENT_MATCH_SECONDS: f32 = ra2sim::r#match::DEFAULT_MATCH_SECONDS;

/// The event match's length. The bracket's match setup (`FUN_0046c720`)
/// writes the arena, game type, robots per match and advancing robots into
/// the global match settings but never the time; the only writer of the
/// time is the exhibition setup's dropdown (`FUN_00474330` ->
/// `FUN_00414230`). So an event match is as long as the last exhibition
/// time the player chose this session, and 3 minutes when they chose none.
/// "No limit" there is no limit here either.
pub fn event_match_seconds(shell: &Shell) -> f32 {
    match shell.time_choice() {
        Some(Some(seconds)) => seconds,
        Some(None) => f32::INFINITY,
        None => EVENT_MATCH_SECONDS,
    }
}

/// The bracket's `play button`: the event the shell entered (`Events.txt` id
/// and the join window's robot column), its next round's match, with the
/// player driving the entered robot. A saved career already in that event
/// resumes it; the round's award is what writes the career.
pub fn start_event_match(
    game: &Path,
    entered: Option<(usize, usize)>,
    seconds: f32,
) -> Result<hq_mode::Session, String> {
    let mut hq = hq_mode::Session::open(game)?;
    hq.set_player_drives(true);
    hq.set_event_seconds(seconds);
    match entered {
        Some((event, robot)) => hq.enter_or_resume(event, Some(robot))?,
        None => hq.resume_event()?,
    }
    hq.start_match()?;
    Ok(hq)
}

/// The HQ's event button on the event in progress when its repairs bench was left open
/// (`FUN_00500c40`): the bench with the clock that is left, over the saved career. `None` when the
/// saved career has no bench open (the bracket is what the button shows then).
pub fn reopen_bench(game: &Path, event: usize) -> Option<Result<hq_mode::Session, String>> {
    let mut hq = hq_mode::Session::open(game).ok()?;
    let in_it = matches!(
        hq.career.phase,
        ra2sim::career::SeasonPhase::InEvent { event: current } if current == event
    );
    if !(in_it && hq.career.bench_pending) {
        return None;
    }
    hq.set_player_drives(true);
    Some(hq.reopen_bench(event).map(|()| hq))
}

/// The bracket's `forfeit button`: the event closes as a loss with the rounds
/// won so far, and the career is saved.
pub fn forfeit_event(game: &Path, entered: Option<(usize, usize)>) -> Result<(), String> {
    let mut hq = hq_mode::Session::open(game)?;
    match entered {
        Some((event, robot)) => hq.enter_or_resume(event, Some(robot))?,
        None => hq.resume_event()?,
    }
    hq.forfeit_event()
}

/// The event flow is back on the HQ (the player left the bracket, or the
/// event ended): the shell shows the Team HQ again with the saved career.
pub fn return_to_shell_hq(shell: &mut Shell) -> Result<(), String> {
    if let Some(view) = hq_mode::shell_view(shell.game()) {
        shell.set_hq_view(view);
    }
    if shell.base_screen_id().eq_ignore_ascii_case(BRACKET_SCREEN) {
        // The bracket's own `later button` is the shipped way back to the HQ.
        shell.click_named("later button")?;
    }
    Ok(())
}

/// Esc on a shell screen or window. The original's menus ignore it: the base
/// window's key handler (`0x5ee6b0`) only forwards a key to the focused
/// control, and the only key handlers that test for Esc (`0x1b`) are the
/// match window's (`0x475ee0`), the in-game popup's (`0x476a1b`) and the
/// controls reminder's (`0x482a10`). Nothing changes and the client keeps
/// running; the answer is whether the application should exit (never).
pub fn shell_escape(_shell: &mut Shell) -> bool {
    false
}
