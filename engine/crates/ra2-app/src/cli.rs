//! The command line: one binary, two ways of showing the same shell.

use std::path::PathBuf;

/// What the binary was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub game: Option<PathBuf>,
    pub offscreen: Option<PathBuf>,
    pub size: [u32; 2],
    pub clicks: Vec<[f32; 2]>,
    pub help: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            game: None,
            offscreen: None,
            size: [1280, 1005],
            clicks: Vec::new(),
            help: false,
        }
    }
}

pub fn usage() -> &'static str {
    "usage: ra2-app [--game DIR] [--offscreen PNG] [--size WxH] [--clicks \"x,y;x,y;...\"]\n\
     \n\
     With no --offscreen the shell opens in a window. With it, the same loop runs\n\
     headlessly: boot, apply the click script in the shell's 1280x1005 coordinates,\n\
     write the final frame to PNG, print the trace and exit. Escape quits the window.\n\
     \n\
       --game DIR       the install (default: RA2_GAME, else local/game)\n\
       --offscreen PNG  render the final state and exit (the CI path)\n\
       --size WxH       window/target size (default 1280x1005; the shell stays 1280x1005)\n\
       --clicks SCRIPT  semicolon-separated x,y points, applied in order\n\
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
            "--size" => parsed.size = parse_size(&next(&mut args, "--size")?)?,
            "--clicks" => parsed.clicks = parse_clicks(&next(&mut args, "--clicks")?)?,
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
}
