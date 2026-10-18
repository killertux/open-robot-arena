//! The command line: one binary, two ways of showing the same shell.

use std::path::PathBuf;

/// What the binary was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub game: Option<PathBuf>,
    pub offscreen: Option<PathBuf>,
    pub size: [u32; 2],
    /// Whether `--size` was given, so the startup `ra2.cfg` size may be used
    /// when it was not.
    pub size_set: bool,
    pub clicks: Vec<[f32; 2]>,
    /// `--click-names A;B;...`: named controls, for a reproducible shell route.
    pub click_names: Vec<String>,
    pub help: bool,
    /// `--match`: play a `ra2sim::match::Match` instead of the shell.
    pub match_mode: bool,
    /// `--play`: the exhibition flow, with robot 0 driven by `--drive`.
    pub play: bool,
    /// `--play --drive "0-2:1,0;2-4:1,1"`.
    pub drive: Option<String>,
    /// `--play --team N --bot N`: the player's team record and `.bot` slot.
    pub team: Option<usize>,
    pub bot: Option<i32>,
    /// `--play --opponent-team N --opponent-bot N`.
    pub opponent_team: Option<usize>,
    pub opponent_bot: Option<i32>,
    /// `--play --gametype deathmatch|battle-royal|team-match|tabletop|king-of-hill`.
    pub gametype: Option<String>,
    /// `--play --arena-index N`: the arena row to click, after scrolling.
    pub arena_index: Option<usize>,
    /// `--play --time SECONDS`: the setup screen's match length.
    pub time: Option<f32>,
    /// `--hq`: boot the Team HQ and its career.
    pub hq: bool,
    /// `--hq --simulate-event N`: play schedule row N headlessly.
    pub simulate_event: Option<usize>,
    /// `--hq --simulate-season`: play all nine events with deterministic
    /// opponents and write the season evidence frames.
    pub simulate_season: bool,
    /// `--hq --report-damage N`: write N damage into the player's first
    /// component and tear off a second one, so the report hands the career a
    /// component-level repair list. The shipped match in this environment never
    /// closes on its opponent, so its report has zero damage; this is the
    /// evidence/test knob for the repairs screen, not a game rule.
    pub report_damage: Option<f32>,
    /// `--match --out DIR`: the offscreen evidence run.
    pub out: Option<PathBuf>,
    /// Fight seconds before the evidence script's decision (and the HUD clock's
    /// configured length).
    pub seconds: Option<f32>,
    /// `--quit-at N`: open the pause dialog at N fight seconds, then quit.
    pub quit_at: Option<f32>,
    /// `--bots a.bot,b.bot`, overriding the two shipped AI bots.
    pub bots: Vec<PathBuf>,
    /// `--arena PATH`, overriding `Arenas/box.py`.
    pub arena: Option<PathBuf>,
    /// `--build`: the bot lab as a live workshop.
    pub build_mode: bool,
    /// `--start PATH`: a `.bot` to open in `--build` (default: a stock chassis).
    pub start: Option<PathBuf>,
    /// `--save-as NAME`: the design name the headless `--build` run types.
    pub save_as: Option<String>,
    /// `--audio-report`: print every play call (path, kind, gain, position).
    pub audio_report: bool,
    /// `--match --out DIR --camera-shots intro@2.5,countdown,f2@13,f3,f3b,f4`:
    /// one PNG per camera moment (`cam-<name>.png`), see
    /// [`crate::match_mode::run_camera_shots`].
    pub camera_shots: Vec<String>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            game: None,
            offscreen: None,
            size: [1280, 1005],
            size_set: false,
            clicks: Vec::new(),
            click_names: Vec::new(),
            help: false,
            match_mode: false,
            play: false,
            drive: None,
            team: None,
            bot: None,
            opponent_team: None,
            opponent_bot: None,
            gametype: None,
            arena_index: None,
            time: None,
            hq: false,
            simulate_event: None,
            simulate_season: false,
            report_damage: None,
            out: None,
            seconds: None,
            quit_at: None,
            bots: Vec::new(),
            arena: None,
            build_mode: false,
            start: None,
            save_as: None,
            audio_report: false,
            camera_shots: Vec::new(),
        }
    }
}

pub fn usage() -> &'static str {
    "usage: ra2-app [--game DIR] [--offscreen PNG] [--size WxH] [--clicks \"x,y;x,y;...\"]\n\
     \x20                [--click-names \"Name;Name;...\"] [--audio-report]\n\
     \x20      ra2-app --match [--seconds N] [--out DIR] [--quit-at N]\n\
     \x20                [--bots A.bot,B.bot] [--arena PATH] [--audio-report]\n\
     \x20                [--camera-shots intro@2.5,countdown,f2@13,f3@17,f3b@20,f4@24]\n\
     \x20      ra2-app --play --drive \"0-2:1,0;2-4:1,1\" [--seconds N] [--out DIR]\n\
     \x20                [--team N --bot N] [--opponent-team N --opponent-bot N]\n\
     \x20                [--gametype deathmatch|battle-royal|team-match|tabletop|king-of-hill]\n\
     \x20                [--arena PATH | --arena-index N] [--time SECONDS] [--size WxH]\n\
     \x20      ra2-app --build [--start BOT] [--save-as NAME] [--seconds N] [--out DIR]\n\
     \x20                [--size WxH]\n\
     \x20      ra2-app --hq [--simulate-event N | --simulate-season] [--seconds N]\n\
     \x20                [--report-damage N] [--out DIR]\n\
     \n\
     With no --offscreen the shell opens in a window. With it, the same loop runs\n\
     headlessly: boot, apply the click script in the shell's 1280x1005 coordinates,\n\
     write the final frame to PNG, print the trace and exit. Escape quits the window.\n\
     \n\
     --match plays a DEATHMATCH on the box arena (two shipped AI bots by default):\n\
     in a window, or headlessly with --out, which writes countdown/fight/results\n\
     PNGs at the named moments and prints every state transition. In the window,\n\
     Escape pauses, C cycles the camera; the pause dialog resumes or quits to menu.\n\
     \n\
     --play starts the exhibition match from the shipped setup screen and drives\n\
     robot 0 with --drive (fight-time seconds; throttle,steer). With --out it\n\
     clicks Exhibition, the two robot pickers, Arena (scrolling to the row),\n\
     hazards, Game Settings and Start Game, then writes play-setup.png,\n\
     play-drive.png and play-results.png. --team/--bot and\n\
     --opponent-team/--opponent-bot choose the records and .bot slots;\n\
     --gametype filters the arena list; --arena-index clicks the row after\n\
     scrolling it into view; --time is the setup screen's match length.\n\
     \n\
     --build opens the bot lab through the shipped route and edits a live model:\n\
     a palette click attaches a component at the model's mount, the armour grid\n\
     sets the plate, a texture plus a face row paints, Enter saves, Ctrl+Z undoes.\n\
     With --out it runs a scripted build headlessly and writes build-chassis.png,\n\
     build-parts.png, build-wiring.png, wiring-add.png, wiring-bound.png,\n\
     build-paint.png and build-test.png.\n\
     \n\
       --game DIR       the install (default: RA2_GAME, else local/game)\n\
       --offscreen PNG  render the final state and exit (the CI path)\n\
       --size WxH       window/target size (default 1280x1005; the shell stays 1280x1005)\n\
       --clicks SCRIPT  semicolon-separated x,y points, applied in order\n\
       --click-names S  semicolon-separated widget names, applied in order\n\
       --audio-report   print every audio play call (path, kind, gain, position)\n\
       --match          play a match instead of the shell\n\
       --play           start the exhibition match from the shipped setup screen\n\
       --drive SCRIPT   --play's robot 0 input: \"start-end:throttle,steer;...\"\n\
       --team N         --play: the player's Teams/Teams.txt record (default: career team)\n\
       --bot N          --play: the player's .bot slot in that team's Robots line\n\
       --opponent-team N  --play: the AI/Teams.txt record (default: team 1)\n\
       --opponent-bot N   --play: the opponent's .bot slot (default: 0)\n\
       --gametype T     --play: deathmatch|battle-royal|team-match|tabletop|king-of-hill\n\
       --arena-index N  --play: click arena row N, scrolling it into view\n\
       --time SECONDS   --play: the setup screen's match length\n\
       --build          open the workshop (bot lab) as an editor\n\
       --start BOT      the .bot to open in --build (default: a stock chassis)\n\
       --save-as NAME   the design name the headless --build run types and saves\n\
       --seconds N      fight length for --match/--play; test length for --build\n\
       --simulate-event N  --hq: play schedule row N's whole event headlessly\n\
       --simulate-season   --hq: play the nine-event season and write the evidence\n\
       --report-damage N  --hq evidence knob: N damage to the player's components\n\
       --out DIR        write the match/play/build PNG frames and exit\n\
       --quit-at N      pause at N fight seconds, quit to the menu, write pause/menu PNGs\n\
       --bots A,B       two .bot paths for --match\n\
       --arena PATH     arena script for --match/--play (default Arenas/box.py)\n\
       --camera-shots L --match --out: one cam-<name>.png per entry of L; intro@T is T\n\
                        seconds into the arena intro, countdown@N the countdown showing N,\n\
                        f2|f3|f3b|f4@S presses that camera key at S fight seconds\n\
       -h, --help       this text"
}

/// Parse `args` (without the program name).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game" => parsed.game = Some(PathBuf::from(next(&mut args, "--game")?)),
            "--offscreen" => {
                parsed.offscreen = Some(PathBuf::from(next(&mut args, "--offscreen")?))
            }
            "--size" => {
                parsed.size = parse_size(&next(&mut args, "--size")?)?;
                parsed.size_set = true;
            }
            "--clicks" => parsed.clicks = parse_clicks(&next(&mut args, "--clicks")?)?,
            "--click-names" => {
                parsed.click_names = next(&mut args, "--click-names")?
                    .split(';')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "--audio-report" => parsed.audio_report = true,
            "--match" => parsed.match_mode = true,
            "--play" => parsed.play = true,
            "--drive" => parsed.drive = Some(next(&mut args, "--drive")?),
            "--team" => parsed.team = Some(parse_index(&next(&mut args, "--team")?, "--team")?),
            "--bot" => parsed.bot = Some(parse_int(&next(&mut args, "--bot")?, "--bot")?),
            "--opponent-team" => {
                parsed.opponent_team = Some(parse_index(
                    &next(&mut args, "--opponent-team")?,
                    "--opponent-team",
                )?)
            }
            "--opponent-bot" => {
                parsed.opponent_bot = Some(parse_int(
                    &next(&mut args, "--opponent-bot")?,
                    "--opponent-bot",
                )?)
            }
            "--gametype" => parsed.gametype = Some(next(&mut args, "--gametype")?),
            "--arena-index" => {
                parsed.arena_index = Some(parse_index(
                    &next(&mut args, "--arena-index")?,
                    "--arena-index",
                )?)
            }
            "--time" => {
                let value = next(&mut args, "--time")?;
                let seconds: f32 = value
                    .parse()
                    .map_err(|_| format!("--time is not a number: {value:?}"))?;
                if !(seconds.is_finite() && seconds > 0.0) {
                    return Err(format!("--time must be positive, got {seconds}"));
                }
                parsed.time = Some(seconds);
            }
            "--hq" => parsed.hq = true,
            "--simulate-event" => {
                let value = next(&mut args, "--simulate-event")?;
                parsed.simulate_event = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--simulate-event is not a number: {value:?}"))?,
                );
            }
            "--simulate-season" => parsed.simulate_season = true,
            "--report-damage" => {
                let value = next(&mut args, "--report-damage")?;
                let damage: f32 = value
                    .parse()
                    .map_err(|_| format!("--report-damage is not a number: {value:?}"))?;
                if !(damage.is_finite() && damage >= 0.0) {
                    return Err(format!(
                        "--report-damage must be non-negative, got {damage}"
                    ));
                }
                parsed.report_damage = Some(damage);
            }
            "--out" => parsed.out = Some(PathBuf::from(next(&mut args, "--out")?)),
            "--seconds" => {
                let value = next(&mut args, "--seconds")?;
                let seconds: f32 = value
                    .parse()
                    .map_err(|_| format!("--seconds is not a number: {value:?}"))?;
                parsed.seconds = Some(seconds);
            }
            "--quit-at" => {
                let value = next(&mut args, "--quit-at")?;
                let seconds: f32 = value
                    .parse()
                    .map_err(|_| format!("--quit-at is not a number: {value:?}"))?;
                parsed.quit_at = Some(seconds);
            }
            "--bots" => {
                let value = next(&mut args, "--bots")?;
                parsed.bots = value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .collect();
            }
            "--camera-shots" => {
                parsed.camera_shots = next(&mut args, "--camera-shots")?
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            "--arena" => parsed.arena = Some(PathBuf::from(next(&mut args, "--arena")?)),
            "--build" => parsed.build_mode = true,
            "--start" => parsed.start = Some(PathBuf::from(next(&mut args, "--start")?)),
            "--save-as" => parsed.save_as = Some(next(&mut args, "--save-as")?),
            "-h" | "--help" => parsed.help = true,
            other => return Err(format!("unknown argument {other:?}\n{}", usage())),
        }
    }
    Ok(parsed)
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value\n{}", usage()))
}

fn parse_index(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} is not a row: {value:?}"))
}

fn parse_int(value: &str, flag: &str) -> Result<i32, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} is not a number: {value:?}"))
}

/// `1280x1005`, as the other runners spell it.
pub fn parse_size(value: &str) -> Result<[u32; 2], String> {
    let (w, h) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("--size wants WxH, got {value:?}"))?;
    let w: u32 = w
        .parse()
        .map_err(|_| format!("--size width is not a number: {w:?}"))?;
    let h: u32 = h
        .parse()
        .map_err(|_| format!("--size height is not a number: {h:?}"))?;
    if w == 0 || h == 0 {
        return Err(format!("--size must be non-zero, got {value:?}"));
    }
    Ok([w, h])
}

/// `x,y;x,y` — points in the shell's own 1280×1005 coordinates.
pub fn parse_clicks(value: &str) -> Result<Vec<[f32; 2]>, String> {
    let mut points = Vec::new();
    for part in value.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (x, y) = part
            .split_once(',')
            .ok_or_else(|| format!("--clicks wants x,y pairs, got {part:?}"))?;
        let x: f32 = x
            .trim()
            .parse()
            .map_err(|_| format!("--clicks: {x:?} is not a number"))?;
        let y: f32 = y
            .trim()
            .parse()
            .map_err(|_| format!("--clicks: {y:?} is not a number"))?;
        points.push([x, y]);
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_design_screen_and_no_script() {
        let args = parse(std::iter::empty()).unwrap();
        assert_eq!(args, Args::default());
        assert_eq!(args.size, [1280, 1005]);
        assert!(args.clicks.is_empty());
        assert!(args.offscreen.is_none());
        assert!(!args.hq);
        assert!(args.simulate_event.is_none());
    }

    #[test]
    fn a_click_script_is_semicolon_separated_pairs() {
        assert_eq!(
            parse_clicks("573.5,534.8; 787.2,446.0 ;").unwrap(),
            vec![[573.5, 534.8], [787.2, 446.0]]
        );
        assert!(parse_clicks("").unwrap().is_empty());
        assert!(parse_clicks("12").is_err());
        assert!(parse_clicks("a,b").is_err());
    }

    #[test]
    fn the_size_is_wxh_and_non_zero() {
        assert_eq!(parse_size("1920x1080").unwrap(), [1920, 1080]);
        assert_eq!(parse_size("1280X1005").unwrap(), [1280, 1005]);
        assert!(parse_size("1280").is_err());
        assert!(parse_size("0x1005").is_err());
    }

    #[test]
    fn the_audio_and_named_click_flags_parse() {
        let args = parse(
            [
                "--click-names",
                "Team HQ button; continue button ;",
                "--audio-report",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(args.click_names, ["Team HQ button", "continue button"]);
        assert!(args.audio_report);
        assert!(!args.size_set);
        let args = parse(["--size", "800x600"].into_iter().map(str::to_string)).unwrap();
        assert!(args.size_set);
        assert_eq!(args.size, [800, 600]);
    }

    #[test]
    fn the_flags_parse_into_one_command() {
        let args = parse(
            [
                "--game",
                "game",
                "--offscreen",
                "out.png",
                "--size",
                "1920x1080",
                "--clicks",
                "1,2;3,4",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(args.game, Some(PathBuf::from("game")));
        assert_eq!(args.offscreen, Some(PathBuf::from("out.png")));
        assert_eq!(args.size, [1920, 1080]);
        assert_eq!(args.clicks, vec![[1.0, 2.0], [3.0, 4.0]]);
    }

    #[test]
    fn the_build_flags_parse() {
        let args = parse(
            [
                "--build",
                "--start",
                "AI/Team1/Bot0.bot",
                "--save-as",
                "My Bot",
                "--seconds",
                "3",
                "--out",
                "frames",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert!(args.build_mode);
        assert_eq!(args.start, Some(PathBuf::from("AI/Team1/Bot0.bot")));
        assert_eq!(args.save_as.as_deref(), Some("My Bot"));
        assert_eq!(args.seconds, Some(3.0));
        assert_eq!(args.out, Some(PathBuf::from("frames")));
    }

    #[test]
    fn the_hq_flags_parse() {
        let args = parse(
            [
                "--hq",
                "--simulate-event",
                "2",
                "--seconds",
                "3",
                "--report-damage",
                "40",
                "--out",
                "frames",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert!(args.hq);
        assert_eq!(args.simulate_event, Some(2));
        assert!(!args.simulate_season);
        assert_eq!(args.seconds, Some(3.0));
        assert_eq!(args.report_damage, Some(40.0));
        assert_eq!(args.out, Some(PathBuf::from("frames")));
        assert!(parse(["--simulate-event".into(), "x".into()]).is_err());
        assert!(parse(["--report-damage".into(), "-1".into()]).is_err());
        let season = parse(
            ["--hq", "--simulate-season"]
                .into_iter()
                .map(str::to_string),
        )
        .unwrap();
        assert!(season.simulate_season);
        assert!(season.simulate_event.is_none());
    }

    #[test]
    fn the_play_flags_parse() {
        let args = parse(
            [
                "--play",
                "--drive",
                "0-2:1,0;2-4:1,1",
                "--seconds",
                "6",
                "--out",
                "play",
                "--arena",
                "Arenas/box.py",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert!(args.play);
        assert_eq!(args.drive.as_deref(), Some("0-2:1,0;2-4:1,1"));
        assert_eq!(args.seconds, Some(6.0));
        assert_eq!(args.out, Some(PathBuf::from("play")));
        assert_eq!(args.arena, Some(PathBuf::from("Arenas/box.py")));
        assert!(parse(["--drive".into()].into_iter()).is_err());

        let chosen = parse(
            [
                "--play",
                "--team",
                "5",
                "--bot",
                "1",
                "--opponent-team",
                "3",
                "--opponent-bot",
                "0",
                "--gametype",
                "king-of-hill",
                "--arena-index",
                "10",
                "--time",
                "12",
                "--out",
                "play",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(chosen.team, Some(5));
        assert_eq!(chosen.bot, Some(1));
        assert_eq!(chosen.opponent_team, Some(3));
        assert_eq!(chosen.opponent_bot, Some(0));
        assert_eq!(chosen.gametype.as_deref(), Some("king-of-hill"));
        assert_eq!(chosen.arena_index, Some(10));
        assert_eq!(chosen.time, Some(12.0));
        assert!(parse(["--team", "x"].into_iter().map(str::to_string)).is_err());
        assert!(parse(["--arena-index", "-1"].into_iter().map(str::to_string)).is_err());
        assert!(parse(["--time", "0"].into_iter().map(str::to_string)).is_err());
    }

    #[test]
    fn the_match_flags_parse() {
        let args = parse(
            [
                "--match",
                "--seconds",
                "20",
                "--out",
                "frames",
                "--quit-at",
                "5",
                "--bots",
                "a.bot, b.bot",
                "--arena",
                "Arenas/hilltop.py",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap();
        assert!(args.match_mode);
        assert_eq!(args.seconds, Some(20.0));
        assert_eq!(args.out, Some(PathBuf::from("frames")));
        assert_eq!(args.quit_at, Some(5.0));
        assert_eq!(
            args.bots,
            vec![PathBuf::from("a.bot"), PathBuf::from("b.bot")]
        );
        assert_eq!(args.arena, Some(PathBuf::from("Arenas/hilltop.py")));
        assert!(parse(["--seconds".into(), "x".into()]).is_err());
    }
}
