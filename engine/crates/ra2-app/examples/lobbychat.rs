//! Diagnostic: the pre-match lobby at 1024x768 with chat lines in `chat messages`
//! (compare with the original's `mp-lobby-host-chat-two`).
//!
//! ```text
//! lobbychat <game> <out.png> [line ...]
//! ```
use ra2_shell::{Shell, PREMATCH};

fn main() -> Result<(), String> {
    ra2fmt::saves::isolate_for_tests();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let game = args
        .first()
        .ok_or("usage: lobbychat <game> <out.png> [line ...]")?;
    let out = args
        .get(1)
        .ok_or("usage: lobbychat <game> <out.png> [line ...]")?;
    let lines: Vec<String> = if args.len() > 2 {
        args[2..].to_vec()
    } else {
        vec!["[Default Player] hi from host".into()]
    };
    let mut shell =
        Shell::boot(std::path::Path::new(game), [1024.0, 768.0]).map_err(|e| format!("{e:?}"))?;
    shell.goto(PREMATCH).map_err(|e| format!("{e:?}"))?;
    let doc = shell.document_mut().ok_or("no document")?;
    ra2_app::net_play::set_display_box(doc, "chat messages", &lines.join("\n"));
    if std::env::var("DBG").is_ok() {
        for p in shell.placements().iter().flat_map(|p| p.walk()) {
            if p.widget.attachment("DisplayBox").is_some() {
                eprintln!(
                    "DBG {:?} {:?} {:?}",
                    p.widget.kind,
                    p.widget.name(),
                    p.frame
                );
            }
        }
    }
    shell.shot(out)?;
    Ok(())
}
