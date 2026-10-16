//! The UI project document: the shell's own index of itself.
//!
//! `local/game/UI/Project Header.html` ships with the game. It is the UI editor's
//! project summary — a tree of the project's directories and the `.gib` files in
//! them, each with **the name and class of its root widget** and a list of the
//! widgets the designer tagged, with the note attached to each. It is the same
//! kind of document as a screen's own `Notes`, one level up: not the game's
//! *definition* of a screen — the `.gib` is that — but the designer's index of all
//! of them, and a second, independent statement of what each file contains.
//!
//! That makes it a check. For the 113 files it names *and* that ship, the actual
//! file has a root with that name and that class, and every widget it tags is
//! there in the class it states — asserted by the corpus test below. It is a
//! *snapshot*, so where the build has moved past it the test names the difference
//! rather than averaging it away: two files it names are not shipped
//! (`common/skipintro.gib` and the editor's own `testfile.gib`), and three
//! screens ship that it does not index (`botlab/comp_properties.gib`,
//! `botlab/info_display.gib`, `help/help_blank.gib`). The last of those three is
//! the reason the binary's own table has to exist: `info_display.gib` is the info
//! bar the C++ loads into `lowersplit`, and it appears in no document — the
//! document predates it.
//!
//! It is also where several of the shell's *links* are written down, in prose the
//! notes share: `options.gib`'s *"click to load video_options.gib into 'options
//! content group'"* and `chassis.gib`'s *"click to load ch_armor.gib into 'right
//! content slot'"*. [`Load`] reads that shape out of a note, so the links the
//! document states can be taken as links rather than as text — and the second of
//! them is a link to a file that is *not shipped*, which the shipped screen's own
//! note says too. The document is a second witness that the stale note is the
//! data's, not a misreading of it.
//!
//! ## Two things the document says that the files do not
//!
//! - **A file's "root" is its first record.** Nine entries are rooted at an
//!   unnamed `[Background]`, and in all nine that is the root's *attachment*,
//!   which the writer put before the root's own header (`hq_league.gib` opens with
//!   `[Background]` and then `[SplitFrame]`). Our reader dresses the following
//!   widget with it, so those roots are the widgets the attachment belongs to.
//! - **Attachment names.** The document names nine `DisplayBox`es (`chat
//!   displaybox`, `component description`, …) and **not one attachment in the
//!   corpus carries a `Name` field** — 0 of 758. The editor's project knows names
//!   the `.gib` writer does not serialize, so those nine can be checked by class
//!   and not by name.
//!
//! ## The format
//!
//! HTML around a `<PRE>` block, whose structure is the indentation: a directory
//! is a heading line at one tab, a file at two (or one, at the top level), and
//! everything about a file — its tagged widgets and their notes — is at three. A
//! file line ends with `(root = "<name>" [<Class>])`; a widget line is
//! `"<name>"<tab>[<Class>]` followed by its note on the next line, when it has
//! one.

use std::path::{Path, PathBuf};

/// One `.gib` the document describes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    /// The directory it is in, as the document gives it (`botlab`, `common`), or
    /// empty for one at the top level.
    pub dir: String,
    /// The file's name, without a directory.
    pub file: String,
    /// The name of its root widget (`""` when the root is unnamed).
    pub root: String,
    /// The class of its root widget, as the document names it.
    pub root_kind: String,
    /// The widgets it tags, in document order.
    pub children: Vec<Child>,
}

impl FileEntry {
    /// The path this entry names, relative to the `UI` directory.
    pub fn path(&self) -> String {
        if self.dir.is_empty() {
            self.file.clone()
        } else {
            format!("{}/{}", self.dir, self.file)
        }
    }
}

/// A widget the document tags, with the note the designer attached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    pub name: String,
    pub kind: String,
    pub note: String,
}

/// The document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Project {
    pub files: Vec<FileEntry>,
}

impl Project {
    /// The entry for a file, by the path [`FileEntry::path`] gives.
    pub fn get(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path() == path)
    }

    /// Every link the notes state, as [`Load::parse`] reads them.
    pub fn loads(&self) -> Vec<Load> {
        let mut out = Vec::new();
        for file in &self.files {
            for child in &file.children {
                if let Some(load) = Load::parse(&child.note) {
                    out.push(Load {
                        from: file.path(),
                        widget: child.name.clone(),
                        ..load
                    });
                }
            }
        }
        out
    }
}

/// A link a note states: *"click to load `screen` into '`slot`'"*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Load {
    /// The file the widget is in.
    pub from: String,
    /// The widget that carries the note.
    pub widget: String,
    /// The `.gib` it loads.
    pub screen: String,
    /// The name of the widget the screen goes into.
    pub slot: String,
}

impl Load {
    /// Read a note, or nothing when it does not state a load.
    ///
    /// The shape is the one the document's notes use: *"click to load
    /// ch_armor.gib into 'right content slot'"* — an optional prefix, `load`, a
    /// file name ending in `.gib`, `into`, and a slot name in single quotes. The
    /// `.gib` is what tells a link from the notes that state *what a cell binds*,
    /// which use the same words without a file: `bot_cell.gib`'s *"load bot image
    /// into 'bot snapshot img' child of this group"* is not a link.
    pub fn parse(note: &str) -> Option<Load> {
        let rest = note.split("load ").nth(1)?;
        let screen = rest.split_whitespace().next()?;
        if !screen.ends_with(".gib") {
            return None;
        }
        let slot = rest.split("into '").nth(1)?.split('\'').next()?;
        Some(Load {
            from: String::new(),
            widget: String::new(),
            screen: screen.to_string(),
            slot: slot.to_string(),
        })
    }
}

/// Parse the document.
pub fn parse(text: &str) -> Project {
    let mut project = Project::default();
    let mut dir = String::new();
    let mut file: Option<FileEntry> = None;
    for line in text.lines() {
        let depth = line.chars().take_while(|c| *c == '\t').count();
        let body = &line[depth..];
        // A directory heading, at one tab.
        if depth == 1 {
            if let Some(rest) = heading(body, "#000080") {
                flush(&mut project, &mut file);
                dir = rest.split("</FONT>").next().unwrap_or("").to_string();
                continue;
            }
        }
        // A file, with the root it states.
        if let Some(caption) = heading(body, "#008000") {
            if let Some(mut entry) = file_line(caption) {
                flush(&mut project, &mut file);
                if depth == 2 {
                    entry.dir = dir.clone();
                }
                file = Some(entry);
            }
            continue;
        }
        if depth != 3 {
            continue;
        }
        // A tagged widget, or the note that follows one.
        if let Some(child) = child_line(body) {
            if let Some(file) = file.as_mut() {
                file.children.push(child);
            }
        } else if !body.is_empty() {
            if let Some(child) = file.as_mut().and_then(|f| f.children.last_mut()) {
                if child.note.is_empty() {
                    child.note = body.to_string();
                }
            }
        }
    }
    flush(&mut project, &mut file);
    project
}

fn flush(project: &mut Project, file: &mut Option<FileEntry>) {
    if let Some(entry) = file.take() {
        project.files.push(entry);
    }
}

/// What follows a `<FONT COLOR=#……>` span's opening tag, when the line is one —
/// the rest of the line, so a caller can take the label it wants and a file line
/// can still see the root it states after its closing tag.
fn heading<'a>(line: &'a str, colour: &str) -> Option<&'a str> {
    let open = format!("<FONT COLOR={colour} SIZE=4>");
    line.strip_prefix(&open)
}

/// `botlabwin.gib</FONT> (root = "botlab win" [Window])` as an entry.
fn file_line(caption: &str) -> Option<FileEntry> {
    let (file, rest) = caption.split_once("</FONT>")?;
    let root = rest.split("(root = \"").nth(1)?;
    let (name, rest) = root.split_once("\" [")?;
    let kind = rest.split(']').next()?;
    Some(FileEntry {
        dir: String::new(),
        file: file.trim().to_string(),
        root: name.to_string(),
        root_kind: kind.to_string(),
        children: Vec::new(),
    })
}

/// `"snap checkbox"<tab>[CheckBox]` as a tagged widget.
fn child_line(line: &str) -> Option<Child> {
    let rest = line.strip_prefix('"')?;
    let (name, rest) = rest.split_once("\"\t[")?;
    let kind = rest.split(']').next()?;
    Some(Child {
        name: name.to_string(),
        kind: kind.to_string(),
        note: String::new(),
    })
}

/// Where the document is, given the `UI` directory it describes.
pub fn find(ui_root: &Path) -> Option<PathBuf> {
    let path = ui_root.join("Project Header.html");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2fmt::gib::{AttachmentKind, Block, Gib, Widget, WidgetKind};

    fn ui_root() -> Option<PathBuf> {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("local/game/UI");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    fn document() -> Option<(PathBuf, Project)> {
        let ui = ui_root()?;
        let text = std::fs::read_to_string(find(&ui)?).ok()?;
        Some((ui, parse(&text)))
    }

    fn root_of(gib: &Gib) -> Option<&Widget> {
        gib.blocks.iter().find_map(|b| match b {
            Block::Widget(w) => Some(w),
            _ => None,
        })
    }

    /// The document's own contents, before it is compared with anything: it
    /// indexes the whole shell, not a handful of screens.
    #[test]
    fn the_project_document_indexes_the_whole_shell() {
        let Some((_, project)) = document() else {
            return;
        };
        assert_eq!(project.files.len(), 115, "files described");
        let tagged: usize = project.files.iter().map(|f| f.children.len()).sum();
        assert_eq!(tagged, 489, "widgets tagged");
        // The project's seven directories, the shell's own.
        let mut dirs: Vec<&str> = project.files.iter().map(|f| f.dir.as_str()).collect();
        dirs.sort_unstable();
        dirs.dedup();
        assert_eq!(
            dirs,
            [
                "",
                "botlab",
                "common",
                "exhibition",
                "help",
                "mainmenu",
                "multiplayer",
                "teamhq"
            ]
        );
        // A root is one of the four classes the design used for them. `Background`
        // is an attachment, and in those entries it is the root's first record.
        let mut roots: Vec<&str> = project.files.iter().map(|f| f.root_kind.as_str()).collect();
        roots.sort_unstable();
        roots.dedup();
        assert_eq!(roots, ["Background", "Group", "SplitFrame", "Window"]);
    }

    /// Every file the document names *and* that ships agrees with it: the root is
    /// the name and class it states, and every widget it tags is in the file in
    /// the class it states.
    ///
    /// Two entries are not shipped and three shipped screens are not indexed, and
    /// both directions are named below rather than tolerated.
    #[test]
    fn the_project_document_agrees_with_the_shipped_screens() {
        let Some((ui, project)) = document() else {
            return;
        };
        let (mut checked, mut attached) = (0, 0);
        let mut missing = Vec::new();
        for entry in &project.files {
            let path = ui.join(entry.path());
            if !path.is_file() {
                missing.push(entry.path());
                continue;
            }
            let mut gib = crate::workshop::load(&ui, &entry.path())
                .unwrap_or_else(|e| panic!("{}: {e}", entry.path()));
            // The document tags what the screen *shows*: a screen whose nav bar
            // is a `Proxy` to `common/bottomnav.gib` is tagged with that bar's
            // own buttons, so the check is against the inlined screen — the same
            // composition the shell draws.
            crate::inline_proxies_in(&mut gib, &ui);
            let root = root_of(&gib).unwrap_or_else(|| panic!("{}: no root", entry.path()));
            let attachment = AttachmentKind::from_name(&entry.root_kind);
            if attachment == AttachmentKind::Other(entry.root_kind.clone()) {
                // A real root: the document's root is the file's root.
                assert_eq!(root.name(), entry.root, "{}: root name", entry.path());
                assert_eq!(
                    root.kind.name(),
                    entry.root_kind,
                    "{}: root class",
                    entry.path()
                );
            } else {
                // The document's "root" is the file's *first record*, which in
                // these files is the root's own attachment — nine entries, one of
                // them the editor's scratch file — so the root we find is the
                // widget that attachment belongs to.
                assert!(entry.root.is_empty(), "{}: unnamed", entry.path());
                assert!(
                    root.attachments.iter().any(|a| a.kind == attachment),
                    "{}: the root carries its {}",
                    entry.path(),
                    entry.root_kind
                );
                attached += 1;
            }
            let all: Vec<&Widget> = gib.walk().collect();
            for child in &entry.children {
                let want = WidgetKind::from_name(&child.kind);
                if want == WidgetKind::Other(child.kind.clone()) {
                    // An attachment, which the writer leaves anonymous (`chat
                    // displaybox`, `component description`, …): there is no name
                    // in the file to check, so the class is the check.
                    let found = all.iter().any(|w| {
                        w.attachments
                            .iter()
                            .any(|a| a.kind.name() == child.kind.as_str())
                    });
                    assert!(
                        found,
                        "{}: no {} attachment for {:?}",
                        entry.path(),
                        child.kind,
                        child.name
                    );
                } else {
                    let found = all.iter().find(|w| w.name() == child.name);
                    let found = found
                        .unwrap_or_else(|| panic!("{}: no widget {:?}", entry.path(), child.name));
                    assert_eq!(
                        found.kind.name(),
                        child.kind,
                        "{}: class of {:?}",
                        entry.path(),
                        child.name
                    );
                }
            }
            checked += 1;
        }
        assert_eq!(checked, 113, "files that ship and are indexed");
        assert_eq!(attached, 8, "entries whose first record is an attachment");
        assert_eq!(
            missing,
            ["testfile.gib", "common/skipintro.gib"],
            "named but not shipped, in the document's order"
        );
    }

    /// The other direction: the screens that ship and that the document does not
    /// index. `info_display.gib` is the one that matters — the binary's table
    /// knows it and the document does not.
    #[test]
    fn three_shipped_screens_are_not_indexed() {
        let Some((ui, project)) = document() else {
            return;
        };
        let mut unindexed = Vec::new();
        for entry in std::fs::read_dir(&ui).unwrap() {
            let dir = entry.unwrap().path();
            if !dir.is_dir() {
                continue;
            }
            for file in std::fs::read_dir(&dir).unwrap() {
                let path = file.unwrap().path();
                if path.extension().and_then(|e| e.to_str()) != Some("gib") {
                    continue;
                }
                let relative = format!(
                    "{}/{}",
                    dir.file_name().unwrap().to_string_lossy(),
                    path.file_name().unwrap().to_string_lossy()
                );
                if project.get(&relative).is_none() {
                    unindexed.push(relative);
                }
            }
        }
        unindexed.sort();
        assert_eq!(
            unindexed,
            [
                "botlab/comp_properties.gib",
                "botlab/info_display.gib",
                "help/help_blank.gib"
            ]
        );
        // And one of the three is in the binary's table: the info bar the C++
        // loads into `lowersplit`.
        assert!(unindexed.iter().any(|p| p == "botlab/info_display.gib"));
    }

    /// The document's notes state the shell's links, and they read as links.
    #[test]
    fn the_document_states_the_shells_links() {
        let Some((_, project)) = document() else {
            return;
        };
        let loads = project.loads();
        let as_pairs: Vec<(&str, &str, &str, &str)> = loads
            .iter()
            .map(|l| {
                (
                    l.from.as_str(),
                    l.widget.as_str(),
                    l.screen.as_str(),
                    l.slot.as_str(),
                )
            })
            .collect();
        assert_eq!(
            as_pairs,
            [
                (
                    "common/options.gib",
                    "video tab",
                    "video_options.gib",
                    "options content group"
                ),
                (
                    "common/options.gib",
                    "audio tab",
                    "audio_options.gib",
                    "options content group"
                ),
                (
                    "botlab/chassis.gib",
                    "armor button",
                    "ch_armor.gib",
                    "right content slot"
                ),
            ],
            "the links the document writes down, in the document's order"
        );
        // One of the three names a file that is not shipped — and the shipped
        // screen's own note names it too, so the staleness is the data's.
        assert!(!ui_root().unwrap().join("botlab/ch_armor.gib").exists());
        let chassis =
            std::fs::read_to_string(ui_root().unwrap().join("botlab/chassis.gib")).unwrap();
        assert!(chassis.contains("Notes click to load ch_armor.gib into 'right content slot'"));
    }

    /// A note that is not a link is not read as one — including the ones that use
    /// the same words to say what a cell binds.
    #[test]
    fn a_note_that_states_no_load_is_not_a_link() {
        assert_eq!(Load::parse("click to set snap on or off"), None);
        assert_eq!(
            Load::parse("click to show 3dview.gib and overview.gib"),
            None
        );
        assert_eq!(Load::parse("load stuff into this slot"), None);
        assert_eq!(
            Load::parse("load bot image into 'bot snapshot img' child of this group"),
            None
        );
        assert_eq!(
            Load::parse("player bot image loaded into 'bot img' (child of this group)"),
            None
        );
        assert_eq!(
            Load::parse("click to load ch_armor.gib into 'right content slot'"),
            Some(Load {
                from: String::new(),
                widget: String::new(),
                screen: "ch_armor.gib".into(),
                slot: "right content slot".into(),
            })
        );
    }

    /// Every class the document names is one the reader knows: the class
    /// vocabulary of the shell is its widget kinds plus its attachment kinds.
    #[test]
    fn every_class_the_document_names_is_one_the_reader_knows() {
        let Some((_, project)) = document() else {
            return;
        };
        let unknown = |name: &str| {
            WidgetKind::from_name(name) == WidgetKind::Other(name.to_string())
                && AttachmentKind::from_name(name) == AttachmentKind::Other(name.to_string())
        };
        for entry in &project.files {
            assert!(
                !unknown(&entry.root_kind),
                "{}: root class {:?}",
                entry.path(),
                entry.root_kind
            );
            for child in &entry.children {
                assert!(
                    !unknown(&child.kind),
                    "{}: class of {:?}",
                    entry.path(),
                    child.name
                );
            }
        }
    }
}
