//! Resolve a `.gib` screen and print where everything lands.
//!
//! ```text
//! cargo run -q -p ra2ui --example uidump -- <screen.gib> [w h] [--all|--rects] [--fonts RA2.cfz]
//! ```
//!
//! `--fonts` sizes the labels the editor left at zero, which is what the game
//! does — measurements from the shipped `fg_*.giff` faces.
//!
//! Without `--all` only the widgets that place children — the windows, groups
//! and split frames — are shown, plus whatever falls outside the screen. The
//! off-screen count is the quickest way to see whether the anchor reading is
//! holding up across the whole corpus.
//!
//! `--rects` instead writes one line per *drawable* widget, in draw order, for a
//! compositor to rasterise (see `tools/ui_compose.py`):
//!
//! ```text
//! <x> <y> <w> <h> <alpha> <r> <g> <b> <u0> <v0> <u1> <v1> <cx> <cy> <cw> <ch> <filename>
//! ```
//!
//! `<cx> <cy> <cw> <ch>` is the rectangle the widget draws inside — the `clip`
//! the resolver worked out from the `ClipMode`s above it — and `0 0 0 0` when
//! nothing clips it.

use ra2ui::{draw_order, resolve_with, Fonts, Frame, Placement, DESIGN_SCREEN};

struct Dump {
    screen: Frame,
    all: bool,
    total: usize,
    outside: usize,
    worst: Vec<(String, Frame)>,
}

impl Dump {
    fn show(&mut self, p: &Placement, depth: usize) {
        self.total += 1;
        let inside = self.screen.contains(&p.frame);
        if !inside {
            self.outside += 1;
            self.worst.push((p.widget.name().to_string(), p.frame));
        }
        if self.all || !p.children.is_empty() || !inside {
            println!(
                "{:indent$}{:<12} {:<22} {:>8.1},{:>8.1} {:>7.1}x{:<7.1}{}{}",
                "",
                p.widget.kind.name(),
                truncate(p.widget.name(), 22),
                p.frame.x,
                p.frame.y,
                p.frame.w,
                p.frame.h,
                if inside { "" } else { "   <- off screen" },
                clip_note(p),
                indent = depth * 2
            );
        }
        for c in &p.children {
            self.show(c, depth + 1);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().cloned() else {
        eprintln!("usage: uidump <screen.gib> [w h] [--all|--rects]");
        std::process::exit(2);
    };
    let number = |i: usize, fallback: f32| -> f32 {
        args.get(i).and_then(|s| s.parse().ok()).unwrap_or(fallback)
    };
    let w = number(1, DESIGN_SCREEN[0]);
    let h = number(2, DESIGN_SCREEN[1]);
    let all = args.iter().any(|a| a == "--all");
    let rects = args.iter().any(|a| a == "--rects");
    let fonts = args
        .iter()
        .position(|a| a == "--fonts")
        .and_then(|i| args.get(i + 1))
        .map(|cfz| Fonts::load(std::path::Path::new(cfz)).expect("load the faces"));

    let text = std::fs::read_to_string(&path).expect("read the layout");
    let gib = ra2fmt::gib::parse(&text).expect("parse the layout");
    let placed = resolve_with(&gib, [w, h], fonts.as_ref());

    if rects {
        // Draw order is tree order; hidden widgets are skipped, which is what
        // leaves a button showing only the first of its four state images.
        let line = |p: &Placement| {
            if !p.widget.visible() {
                return;
            }
            // A label is a text command: the compositor draws it with the face
            // and colour the widget names.
            if let (Some(text), Some(font)) = (p.widget.text(), p.widget.font()) {
                if !text.is_empty() {
                    let c = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let (cx, cy, cw, ch) = clip_fields(p);
                    println!(
                        "{:.2} {:.2} {:.2} {:.2} 1 1.000 1.000 1.000 0 0 1 1 {} {} {} {} text:{}:{:.3} {:.3} {:.3}:{}",
                        p.frame.x, p.frame.y, p.frame.w, p.frame.h, cx, cy, cw, ch, font, c[0], c[1], c[2], text
                    );
                    return;
                }
            }
            let Some(file) = p.widget.filename() else {
                return;
            };
            if file.is_empty() {
                return;
            }
            let uv = p.widget.uv().unwrap_or([0.0, 0.0, 1.0, 1.0]);
            let alpha = p
                .widget
                .field("_Alpha")
                .and_then(|f| f.flag())
                .unwrap_or(true);
            let c = p.widget.fore_color().unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let (cx, cy, cw, ch) = clip_fields(p);
            println!(
                "{:.2} {:.2} {:.2} {:.2} {} {:.3} {:.3} {:.3} {:.6} {:.6} {:.6} {:.6} {} {} {} {} {}",
                p.frame.x,
                p.frame.y,
                p.frame.w,
                p.frame.h,
                u8::from(alpha),
                c[0],
                c[1],
                c[2],
                uv[0],
                uv[1],
                uv[2],
                uv[3],
                cx,
                cy,
                cw,
                ch,
                file.replace('\\', "/")
            );
        };
        for p in draw_order(&placed) {
            line(p);
        }
        return;
    }

    println!(
        "{path} — T {}, {} top-level blocks, screen {w}x{h}",
        gib.version,
        gib.blocks.len()
    );
    let mut d = Dump {
        screen: Frame::new(0.0, 0.0, w, h),
        all,
        total: 0,
        outside: 0,
        worst: Vec::new(),
    };
    for root in &placed {
        d.show(root, 0);
    }
    let (total, outside) = (d.total, d.outside);
    println!(
        "placed {total}, of which {outside} fall outside the screen ({:.1}%)",
        100.0 * outside as f32 / total.max(1) as f32
    );
    d.worst.sort_by(|a, b| {
        let key = |f: &Frame| f.x.abs() + f.y.abs() + f.w.abs() + f.h.abs();
        key(&b.1).partial_cmp(&key(&a.1)).unwrap()
    });
    for (name, f) in d.worst.iter().take(6) {
        println!(
            "   worst: {:<22} {:>9.1},{:>9.1} {:>8.1}x{:<8.1}",
            truncate(name, 22),
            f.x,
            f.y,
            f.w,
            f.h
        );
    }
}

/// The clip rectangle a rects line carries: `0 0 0 0` for "nothing clips me".
fn clip_fields(p: &Placement) -> (f32, f32, f32, f32) {
    match p.clip {
        Some(c) => (c.x, c.y, c.w, c.h),
        None => (0.0, 0.0, 0.0, 0.0),
    }
}

/// A suffix saying how a widget is clipped, when that is not its own frame.
fn clip_note(p: &Placement) -> String {
    let Some(c) = p.clip else {
        return String::new();
    };
    if (c.x, c.y, c.w, c.h) == (p.frame.x, p.frame.y, p.frame.w, p.frame.h) {
        return String::new();
    }
    format!("  clip {:.0},{:.0} {:.0}x{:.0}", c.x, c.y, c.w, c.h)
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}
