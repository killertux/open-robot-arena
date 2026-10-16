//! Parser for `.gib` — the game's own GUI *layout* language.
//!
//! Every screen in the shell is one of these files: the main menu, the team HQ,
//! the whole bot lab, the in-match HUD, the dialogs. They are plain text with
//! CRLF line endings and describe a tree of widgets:
//!
//! ```text
//! T 1.09
//! [SplitFrame]
//! Horizontal true
//! ...
//! NumChildren 2
//! [Group]
//! Name left
//! ...
//! NumChildren 1
//! [Background]
//! Tiling false
//! Image true 0
//! Widget true
//! [Border]
//! Width 1
//! ...
//! [Placeholder]
//! Name 3d placeholder
//! ...
//! ```
//!
//! ## The grammar
//!
//! A document is `T <version>`, then a sequence of blocks. A block is a `[Kind]`
//! line followed by field lines, and a field line is a label, a space, and a
//! value that runs to the end of the line — values may contain spaces
//! (`Name New [Image]`) or be empty (`Notes `), so a label is what tells a field
//! line from anything else. A line whose first token is *not* a label is an
//! **unlabelled value**: the writer emits exactly one of those after an `Image`'s
//! `_Position` (its texture UV rect, four numbers) and one after a `Glyph`'s
//! `Compositor` (one number), and nowhere else — 1539 and 40 of them, one per
//! widget of each kind, in every file that has them.
//!
//! What makes a block a *widget* rather than an *attachment* is that it carries
//! **`NumChildren`**; an attachment (`Background`, `Border`, `Caption`,
//! `DisplayBox`, `Scroller`) never does. That is the whole classification rule,
//! and it is derived from the format rather than from a list: a widget's
//! `NumChildren` says how many *widget* blocks follow it, and the attachment
//! blocks that appear in that run hang off the widget they follow rather than
//! being counted. The writer is not consistent about field order — `NumChildren`
//! can be the last field or come before `Name`/`ID`/`Position` — so nothing here
//! may depend on order. What *is* consistent, and what this parser relies on, is
//! that **all of a widget's own fields precede its children**: no field line ever
//! appears between two child blocks.
//!
//! ## Geometry
//!
//! `Position` and `Size` are four numbers: an amount and a flag for each axis
//! from the top-left, `Position x x_is_fraction y y_is_fraction` and
//! `Size w w_is_fraction h h_is_fraction`. The flag slot is 0 or 1 everywhere in
//! the shipped files, and it means exactly what it looks like: with the flag set
//! the amount is a fraction of the parent, without it the amount is pixels. So
//! `Position 237 0 191 0` is a pixel offset and `ReportedPos 0.5 1 0.07 1` is an
//! anchored one. There is one shipped exception — a flag slot holding
//! `1242948` — which is a typo in the data, not a second rule.

use crate::Error;

/// A parsed `.gib` document.
#[derive(Debug, Clone, PartialEq)]
pub struct Gib {
    pub version: String,
    /// The top-level blocks. Most files are a single `Window`, but a good many
    /// are *fragments* that a screen loads into a slot at run time, and those
    /// start with a lone attachment.
    pub blocks: Vec<Block>,
}

impl Gib {
    /// The top-level widgets, in file order.
    pub fn widgets(&self) -> impl Iterator<Item = &Widget> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Widget(w) => Some(w),
            Block::Attachment(_) => None,
        })
    }

    /// Every widget in the document, depth first.
    pub fn walk(&self) -> WidgetWalk<'_> {
        let mut stack = Vec::new();
        for block in self.blocks.iter().rev() {
            if let Block::Widget(w) = block {
                stack.push(w);
            }
        }
        WidgetWalk { stack }
    }
}

/// Depth-first walk over a document's widgets.
pub struct WidgetWalk<'a> {
    stack: Vec<&'a Widget>,
}

impl<'a> Iterator for WidgetWalk<'a> {
    type Item = &'a Widget;
    fn next(&mut self) -> Option<&'a Widget> {
        let w = self.stack.pop()?;
        self.stack.extend(w.children.iter().rev());
        Some(w)
    }
}

/// A top-level block: either a widget or one of the attachable blocks.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Widget(Widget),
    Attachment(Attachment),
}

/// A widget: a `[Kind]` block with a `NumChildren` count.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub kind: WidgetKind,
    /// True when the file wrote **no `[Kind]` header** for this record. The base
    /// class `Widget` is the only kind the writer leaves nameless — its header
    /// comes from a virtual call that returns nothing for it — and such a child
    /// does **not** count against its parent's `NumChildren`. Measured over the
    /// whole corpus: `sum(NumChildren)` plus the number of headerless records is
    /// `records - 1` in all 116 files.
    pub implicit: bool,
    pub fields: Vec<Field>,
    /// `Background`/`Border`/`Caption`/... blocks that follow this widget.
    pub attachments: Vec<Attachment>,
    pub children: Vec<Widget>,
}

impl Widget {
    /// The first field with this label.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name.as_deref() == Some(name))
    }

    /// Every field with this label, in file order (`Point`, `StateImage`, ...).
    pub fn fields_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Field> {
        self.fields
            .iter()
            .filter(move |f| f.name.as_deref() == Some(name))
    }

    /// The unlabelled value the writer emitted, if any.
    pub fn unnamed(&self) -> Option<&Field> {
        self.fields.iter().find(|f| f.name.is_none())
    }

    fn text_field(&self, name: &str) -> Option<&str> {
        self.field(name).map(|f| f.text())
    }

    /// The author's name for this widget (`Name`), which is empty on many.
    pub fn name(&self) -> &str {
        self.text_field("Name").unwrap_or("")
    }

    /// The author's free-text note (`Notes`) — often the only documentation a
    /// button has: *"click to return to HQ"*.
    pub fn notes(&self) -> &str {
        self.text_field("Notes").unwrap_or("")
    }

    /// The widget's id, which the game's own code matches on. `-1` when it has
    /// none.
    pub fn id(&self) -> i32 {
        self.field("ID")
            .and_then(|f| f.number())
            .map(|n| n as i32)
            .unwrap_or(-1)
    }

    pub fn visible(&self) -> bool {
        self.field("Visible").and_then(|f| f.flag()).unwrap_or(true)
    }

    pub fn child_order(&self) -> i32 {
        self.field("ChildOrder")
            .and_then(|f| f.number())
            .map(|n| n as i32)
            .unwrap_or(0)
    }

    pub fn clip_mode(&self) -> i32 {
        self.field("ClipMode")
            .and_then(|f| f.number())
            .map(|n| n as i32)
            .unwrap_or(0)
    }

    pub fn alignment(&self) -> i32 {
        self.field("Alignment")
            .and_then(|f| f.number())
            .map(|n| n as i32)
            .unwrap_or(0)
    }

    /// `Position` — the offset within the parent.
    pub fn position(&self) -> Option<Axes> {
        self.field("Position").and_then(Axes::parse)
    }

    /// `Size` — the widget's own extent, width then height.
    pub fn size(&self) -> Option<Axes> {
        self.field("Size").and_then(Axes::parse)
    }

    /// `ReportedPos` — what the editor last showed for a `Window`/`Group`.
    pub fn reported_pos(&self) -> Option<Axes> {
        self.field("ReportedPos").and_then(Axes::parse)
    }

    pub fn attachment(&self, kind: &str) -> Option<&Attachment> {
        self.attachments.iter().find(|a| a.kind.name() == kind)
    }

    /// The `OnClick` handler name, when the widget is a button.
    pub fn on_click(&self) -> Option<&str> {
        self.field("OnClick").map(|f| f.text())
    }

    /// The `_Filename` of an `Image`.
    pub fn filename(&self) -> Option<&str> {
        self.field("_Filename").map(|f| f.text())
    }

    /// An `Image`'s source rect in its texture, from the unlabelled field.
    pub fn uv(&self) -> Option<[f32; 4]> {
        let f = self.unnamed()?;
        let v = f.numbers();
        (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
    }

    /// The `_Text`/`Text` a label draws, for `Text`, `TextBox`, `ListBox`,
    /// `DropDown` and `Glyph`.
    pub fn text(&self) -> Option<&str> {
        self.text_field("_Text").or_else(|| self.text_field("Text"))
    }

    /// The `_Font` a label draws with.
    pub fn font(&self) -> Option<&str> {
        self.text_field("_Font")
    }

    /// `_ForeColor`, or `ForeColor` on the list widgets.
    pub fn fore_color(&self) -> Option<[f32; 4]> {
        self.field("_ForeColor")
            .or_else(|| self.field("ForeColor"))
            .and_then(|f| f.four())
    }
}

/// One `[Kind]` block that attaches to the widget before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub fields: Vec<Field>,
}

impl Attachment {
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name.as_deref() == Some(name))
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        self.field(name).and_then(|f| f.flag())
    }

    pub fn number(&self, name: &str) -> Option<f32> {
        self.field(name).and_then(|f| f.number())
    }

    pub fn four(&self, name: &str) -> Option<[f32; 4]> {
        self.field(name).and_then(|f| f.four())
    }
}

/// A field line: a label and the rest of the line, or an unlabelled value.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// `None` for the unlabelled value an `Image` and a `Glyph` each carry.
    pub name: Option<String>,
    /// The value as written, to end of line.
    pub raw: String,
}

impl Field {
    /// The value as text. Whitespace-only values come back empty.
    pub fn text(&self) -> &str {
        &self.raw
    }

    /// Every whitespace-separated token that reads as a number, in order.
    /// Tokens that do not read are skipped, so `Margins _Size 1` yields `[1]`.
    pub fn numbers(&self) -> Vec<f32> {
        self.raw
            .split_whitespace()
            .filter_map(read_number)
            .collect()
    }

    /// The first number, when the value is a single number.
    pub fn number(&self) -> Option<f32> {
        read_number(self.raw.split_whitespace().next()?)
    }

    /// A boolean written the way the writer writes them.
    pub fn flag(&self) -> Option<bool> {
        match self.raw.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    /// Exactly four numbers, or nothing.
    pub fn four(&self) -> Option<[f32; 4]> {
        let v = self.numbers();
        (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
    }
}

/// The two axes a `Position` or `Size` states, in the order the field writes
/// them: for a `Position` that is x then y, for a `Size` it is width then
/// height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axes {
    pub first: Length,
    pub second: Length,
}

impl Axes {
    /// Read `a a_is_fraction b b_is_fraction`, or nothing if the field does not
    /// hold exactly four numbers.
    pub fn parse(field: &Field) -> Option<Axes> {
        let v = field.numbers();
        if v.len() != 4 {
            return None;
        }
        Some(Axes {
            first: Length::new(v[0], v[1]),
            second: Length::new(v[2], v[3]),
        })
    }

    /// Both axes in the parent's units, given the parent's extent along them.
    pub fn resolve(&self, parent_first: f32, parent_second: f32) -> [f32; 2] {
        [
            self.first.resolve(parent_first),
            self.second.resolve(parent_second),
        ]
    }
}

/// One axis of a `Position`/`Size`: an amount, and whether it is a fraction of
/// the parent rather than a pixel count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Length {
    pub amount: f32,
    pub fraction: bool,
}

impl Length {
    fn new(amount: f32, flag: f32) -> Length {
        Length {
            amount,
            fraction: flag != 0.0,
        }
    }

    /// Read the single-axis form `amount is_fraction`, or nothing if the field
    /// does not hold exactly two numbers.
    ///
    /// A `SplitFrame`'s `MasterSize` is written this way — `0.14 1`, `90 0` — in
    /// all twenty-six of the corpus's split frames, and it is one axis, not two:
    /// the other two numbers an `Axes` needs are not there.
    pub fn parse(field: &Field) -> Option<Length> {
        let v = field.numbers();
        if v.len() != 2 {
            return None;
        }
        Some(Length::new(v[0], v[1]))
    }

    /// The amount in the parent's units (pixels, given the parent's extent).
    pub fn resolve(&self, parent: f32) -> f32 {
        if self.fraction {
            self.amount * parent
        } else {
            self.amount
        }
    }
}

/// The kind of a widget block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidgetKind {
    /// The base class, and the one kind whose `[Kind]` header the writer leaves
    /// out — a bare container that carries no class fields of its own beyond
    /// `Notes` and `PassMouse`. A block that starts with a field line rather
    /// than a header is one of these.
    Widget,
    Window,
    Group,
    SplitFrame,
    Image,
    Text,
    TextBox,
    Button,
    CheckBox,
    Rectangle,
    Lines,
    Slider,
    ScrollBar,
    ListBox,
    Grid,
    OptionGroup,
    DropDown,
    Placeholder,
    Proxy,
    Glyph,
    /// A kind this build does not know. Kept rather than rejected so a mod's
    /// screen still loads.
    Other(String),
}

impl WidgetKind {
    fn parse(name: &str) -> WidgetKind {
        use WidgetKind::*;
        match name {
            "Widget" => Widget,
            "Window" => Window,
            "Group" => Group,
            "SplitFrame" => SplitFrame,
            "Image" => Image,
            "Text" => Text,
            "TextBox" => TextBox,
            "Button" => Button,
            "CheckBox" => CheckBox,
            "Rectangle" => Rectangle,
            "Lines" => Lines,
            "Slider" => Slider,
            "ScrollBar" => ScrollBar,
            "ListBox" => ListBox,
            "Grid" => Grid,
            "OptionGroup" => OptionGroup,
            "DropDown" => DropDown,
            "Placeholder" => Placeholder,
            "Proxy" => Proxy,
            "Glyph" => Glyph,
            other => Other(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            WidgetKind::Other(s) => s,
            k => k.static_name(),
        }
    }

    fn static_name(&self) -> &'static str {
        use WidgetKind::*;
        match self {
            Widget => "Widget",
            Window => "Window",
            Group => "Group",
            SplitFrame => "SplitFrame",
            Image => "Image",
            Text => "Text",
            TextBox => "TextBox",
            Button => "Button",
            CheckBox => "CheckBox",
            Rectangle => "Rectangle",
            Lines => "Lines",
            Slider => "Slider",
            ScrollBar => "ScrollBar",
            ListBox => "ListBox",
            Grid => "Grid",
            OptionGroup => "OptionGroup",
            DropDown => "DropDown",
            Placeholder => "Placeholder",
            Proxy => "Proxy",
            Glyph => "Glyph",
            Other(_) => unreachable!(),
        }
    }
}

/// The kind of an attached block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    Background,
    Border,
    Caption,
    DisplayBox,
    Scroller,
    Other(String),
}

impl AttachmentKind {
    fn parse(name: &str) -> AttachmentKind {
        use AttachmentKind::*;
        match name {
            "Background" => Background,
            "Border" => Border,
            "Caption" => Caption,
            "DisplayBox" => DisplayBox,
            "Scroller" => Scroller,
            other => Other(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            AttachmentKind::Other(s) => s,
            AttachmentKind::Background => "Background",
            AttachmentKind::Border => "Border",
            AttachmentKind::Caption => "Caption",
            AttachmentKind::DisplayBox => "DisplayBox",
            AttachmentKind::Scroller => "Scroller",
        }
    }
}

/// A field label: an identifier. Numeric tokens are values, not labels.
fn is_label(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One number as the game writes them. That includes the spellings the
/// Microsoft runtime uses for the non-finite ones — `1.#INF`, `-1.#INF`,
/// `1.#IND` — which are not a theory but *are* in the data: three bot-lab
/// screens carry `0 0 1.#INF 1.#INF` as an image's texture rect.
fn read_number(token: &str) -> Option<f32> {
    if let Ok(v) = token.parse::<f32>() {
        return Some(v);
    }
    let (sign, body) = match token.strip_prefix('-') {
        Some(rest) => (-1.0f32, rest),
        None => (1.0, token),
    };
    let body = body.strip_prefix("1.")?.strip_prefix('#')?;
    match body {
        "INF" | "INFINITY" => Some(sign * f32::INFINITY),
        "IND" | "QNAN" | "SNAN" | "NAN" => Some(f32::NAN),
        _ => None,
    }
}

/// Parse a `.gib` layout.
pub fn parse(text: &str) -> Result<Gib, Error> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut p = Parser {
        lines,
        at: 0,
        pending: Vec::new(),
    };

    p.skip_blank();
    let header = p.lines.get(p.at).copied().unwrap_or("");
    let Some(version) = header.strip_prefix("T ") else {
        return Err(Error::BadMagic(format!(
            "a .gib starts with 'T <version>', found {header:?}"
        )));
    };
    let version = version.trim().to_string();
    p.at += 1;

    let mut blocks = Vec::new();
    while {
        p.skip_blank();
        p.at < p.lines.len()
    } {
        blocks.push(p.block()?);
    }
    Ok(Gib { version, blocks })
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    at: usize,
    /// Attachments read but not yet hung on a widget. The shipped writer puts a
    /// widget's attachments *before* it — every `[Background]` in the corpus is
    /// followed by a widget, and the head of `armor.gib` shows a `Background`
    /// and a `Border` between a group and the group they dress — so they are
    /// carried forward and taken by the next widget block.
    pending: Vec<Attachment>,
}

impl<'a> Parser<'a> {
    fn skip_blank(&mut self) {
        while self.lines.get(self.at).is_some_and(|l| l.trim().is_empty()) {
            self.at += 1;
        }
    }

    /// The kind of the block at the cursor, if the cursor is on a header.
    fn header(&self) -> Option<&'a str> {
        let line = self.lines.get(self.at)?;
        let inner = line.strip_prefix('[')?;
        let kind = inner.strip_suffix(']')?;
        Some(kind)
    }

    /// One record: an optional `[Kind]` header and then its fields, ending at
    /// the record's own `NumChildren` — which is the last field of every record
    /// in the corpus, checked over all 116 files and 3,590 widgets.
    ///
    /// The header is **optional** and that is not a quirk of the reader: the
    /// base class `Widget` is the one kind whose header the writer leaves out,
    /// which the binary's own schema says — `[Widget]` is the only entry in the
    /// field table whose class fields are just `Notes ` and `PassMouse `, and a
    /// headerless record carries exactly those. Twenty records in `repairs.gib`
    /// are like this, and reading each as part of the one before it is what left
    /// blocks a level up. The writer emits the header from a virtual call, so
    /// the base class's is empty.
    ///
    /// An *attachment*'s record has no `NumChildren` at all, so its fields run
    /// to the next header instead; that is the only place a record's end is not
    /// its own `NumChildren`.
    fn record(&mut self) -> Result<Option<(String, Vec<Field>, bool)>, Error> {
        self.skip_blank();
        if self.at >= self.lines.len() {
            return Ok(None);
        }
        let (kind, named) = match self.header() {
            Some(kind) => {
                let kind = kind.to_string();
                self.at += 1;
                (kind, true)
            }
            // No header: the base class, whose name the writer omits.
            None => ("Widget".to_string(), false),
        };
        let mut out = Vec::new();
        while let Some(line) = self.lines.get(self.at) {
            if line.trim().is_empty() {
                self.at += 1;
                continue;
            }
            if line.starts_with('[') {
                break;
            }
            let (name, raw) = match line.split_once(' ') {
                Some((name, value)) if is_label(name) => (Some(name.to_string()), value),
                _ => (None, line.trim()),
            };
            let count = name.as_deref() == Some("NumChildren");
            out.push(Field {
                name,
                raw: raw.trim_end().to_string(),
            });
            self.at += 1;
            if count {
                break;
            }
        }
        Ok(Some((kind, out, named)))
    }

    fn block(&mut self) -> Result<Block, Error> {
        let at = self.at;
        let Some((kind, fields, named)) = self.record()? else {
            return Err(Error::Structure(format!(
                "line {}: expected a record or a [Kind] block, found end of file",
                at + 1,
            )));
        };
        let Some(count) = fields
            .iter()
            .find(|f| f.name.as_deref() == Some("NumChildren"))
            .and_then(|f| f.number())
        else {
            // An attachment: it dresses the widget that comes *next*, so it is
            // held and that widget takes it. On its own at the end of a file it
            // is a *fragment* — a slot's worth of chrome with no widget of its
            // own — and comes back as itself.
            let attachment = Attachment {
                kind: AttachmentKind::parse(&kind),
                fields,
            };
            self.pending.push(attachment.clone());
            self.skip_blank();
            if self.at >= self.lines.len() {
                return Ok(Block::Attachment(attachment));
            }
            return self.block();
        };
        let count = count as usize;
        let mut children = Vec::new();
        let attachments = std::mem::take(&mut self.pending);
        // The children are the next `count` **named** widget blocks. Two things
        // in between do not count: an attachment, which dresses the child that
        // follows (and has already been taken by that child's own `block` call),
        // and a headerless record, which is a child all the same — it is just not
        // one the count names. Both are read and both are kept in place.
        // A count of zero means the record has no children *at all*: the writer
        // gates its whole child loop on `0 < NumChildren` (`FUN_005e7ca0`), so
        // even a headerless child — which the count does not name — is absent.
        if count > 0 {
            let mut counted = 0usize;
            while counted < count {
                self.skip_blank();
                if self.at >= self.lines.len() {
                    return Err(Error::UnexpectedEof {
                        at: self.at,
                        wanted: count - counted,
                    });
                }
                let child = match self.block()? {
                    Block::Widget(w) => w,
                    Block::Attachment(_) => unreachable!("an attachment came back mid-run"),
                };
                // Only a *named* child fills a slot; an unnamed one is a sibling
                // of the child before it, which is what the buttons' caption
                // holders are: `mainmenu.gib`'s `exhibition button` is followed
                // by a headerless `Widget` of the button's own size, and it lands
                // in the group the button is in — the same frame the button's own
                // `Position` was measured against.
                if !child.implicit {
                    counted += 1;
                }
                children.push(child);
            }
        }
        Ok(Block::Widget(Widget {
            kind: WidgetKind::parse(&kind),
            implicit: !named,
            fields,
            attachments,
            children,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The head of `UI/botlab/armor.gib`: a split frame whose left slot carries
    /// a group with a `Background` and a `Border` attached, which itself holds
    /// the 3D view.
    const ARMOR: &str = "T 1.09\r\n\
[SplitFrame]\r\n\
Horizontal true\r\n\
Master 0\r\n\
MasterSize 0.5 1\r\n\
Name armor split\r\n\
ID -1\r\n\
Position 0 0 0 0\r\n\
Size 1 1 1 1\r\n\
Visible true\r\n\
ChildOrder 3\r\n\
ClipMode 1\r\n\
NumChildren 2\r\n\
[Group]\r\n\
ReportedPos 0 0 0 0\r\n\
Alignment 0\r\n\
Name left\r\n\
ID -1\r\n\
Position 0 0 0 0\r\n\
Size 400 0 1 1\r\n\
Visible true\r\n\
ChildOrder 0\r\n\
ClipMode 1\r\n\
NumChildren 1\r\n\
[Background]\r\n\
Tiling false\r\n\
Image true 0\r\n\
Widget true\r\n\
[Border]\r\n\
Width 1\r\n\
Inner 1 1 1 1\r\n\
Outer 0.6 0.6 0.6 1\r\n\
Widget true\r\n\
[Group]\r\n\
ReportedPos 0.5 1 0.07 1\r\n\
Alignment 10\r\n\
Name 3d grp\r\n\
ID -1\r\n\
Position 0.5 1 0.07 1\r\n\
Size 0.92 1 368 0\r\n\
Visible true\r\n\
ChildOrder -1\r\n\
ClipMode 1\r\n\
NumChildren 0\r\n\
[Group]\r\n\
ReportedPos 0 1 1 1\r\n\
Alignment 36\r\n\
Name right\r\n\
ID -1\r\n\
Position 0 1 1 1\r\n\
Size 1 1 1 1\r\n\
Visible true\r\n\
ChildOrder -1\r\n\
ClipMode 1\r\n\
NumChildren 0\r\n";

    #[test]
    fn a_split_frame_holds_two_slots_and_attachments_are_not_children() {
        let gib = parse(ARMOR).unwrap();
        assert_eq!(gib.version, "1.09");
        assert_eq!(gib.blocks.len(), 1);
        let split = gib.widgets().next().unwrap();
        assert_eq!(split.kind, WidgetKind::SplitFrame);
        assert_eq!(split.name(), "armor split");
        assert_eq!(split.children.len(), 2);
        // The Background and Border dress the *next* widget in the run — the
        // group that holds the 3D view — which is what the shipped writer means
        // by putting an attachment before its widget.
        assert!(split.attachments.is_empty());
        let left = &split.children[0];
        assert_eq!(left.name(), "left");
        assert_eq!(left.children.len(), 1);
        assert!(left.attachments.is_empty());
        let view = &left.children[0];
        assert_eq!(view.name(), "3d grp");
        assert_eq!(view.alignment(), 10);
        assert_eq!(view.children.len(), 0);
        assert_eq!(view.attachments.len(), 2);
        assert_eq!(view.attachments[0].kind, AttachmentKind::Background);
        assert_eq!(view.attachments[1].kind, AttachmentKind::Border);
        assert_eq!(
            view.attachment("Border").unwrap().number("Width"),
            Some(1.0)
        );
        assert_eq!(
            view.attachment("Border").unwrap().four("Outer"),
            Some([0.6, 0.6, 0.6, 1.0])
        );
        // The split's second slot is the right pane, not a grandchild of the
        // left one: the two are siblings.
        assert_eq!(split.children[1].name(), "right");

        assert_eq!(gib.walk().count(), 4);
    }

    #[test]
    fn the_two_flag_slots_say_fraction_versus_pixels() {
        let gib = parse(ARMOR).unwrap();
        let split = gib.widgets().next().unwrap();
        // `Position 0 0 0 0` and `Size 1 1 1 1` are relative.
        let size = split.size().unwrap();
        assert!(size.first.fraction && size.second.fraction);
        assert_eq!(size.first.amount, 1.0);
        let left = &split.children[0];
        // `Size 400 0 1 1` is 400 px wide and full height.
        let size = left.size().unwrap();
        assert_eq!(size.first.amount, 400.0);
        assert!(!size.first.fraction);
        assert!(size.second.fraction);
        // `Size 0.92 1 368 0` is 0.92 of the parent wide, 368 px tall.
        let size = left.children[0].size().unwrap();
        assert_eq!(size.resolve(400.0, 400.0), [368.0, 368.0]);
    }

    /// A fragment: a file that is a lone attachment, to be loaded into a slot.
    #[test]
    fn a_file_can_be_a_bare_attachment() {
        let gib = parse("T 1.09\n[Background]\nTiling false\nWidget true\n").unwrap();
        assert!(gib.widgets().next().is_none());
        match &gib.blocks[0] {
            Block::Attachment(a) => assert_eq!(a.kind, AttachmentKind::Background),
            other => panic!("expected an attachment, got {other:?}"),
        }
    }

    #[test]
    fn an_image_carries_its_texture_rect_unlabelled() {
        let text = "T 1.09\n\
[Image]\n\
_Tiling false\n\
_Filename \\mainmenu\\ra2_logo.tga\n\
_Size 512 0 76 0\n\
_Alpha true\n\
_ForeColor 1 1 1 1\n\
_Name \n\
_Position 0 0 0 0\n\
0.15625 0.625 0.65625 0.699219\n\
Name New [Image]\n\
ID -1\n\
Position 0 0 0 0\n\
Size 512 0 76 0\n\
Visible true\n\
ChildOrder 7\n\
ClipMode 0\n\
NumChildren 0\n";
        let gib = parse(text).unwrap();
        let image = gib.widgets().next().unwrap();
        // `Name New [Image]` is one value, brackets and all.
        assert_eq!(image.name(), "New [Image]");
        assert_eq!(image.filename(), Some("\\mainmenu\\ra2_logo.tga"));
        assert_eq!(image.uv(), Some([0.15625, 0.625, 0.65625, 0.699219]));
        assert_eq!(image.id(), -1);
        assert!(image.visible());
        assert_eq!(image.child_order(), 7);
        assert_eq!(image.clip_mode(), 0);
    }

    #[test]
    fn a_non_gib_file_is_rejected() {
        assert!(parse("[Window]\n").is_err());
        assert!(parse("").is_err());
    }

    /// Three shipped screens carry an image whose texture rect is
    /// `0 0 1.#INF 1.#INF` — the Microsoft runtime's spelling of infinity,
    /// written into the layout by the game's own editor. Read as numbers, all
    /// four slots are present.
    #[test]
    fn the_runtime_spelling_of_infinity_reads_as_a_number() {
        assert_eq!(read_number("1.#INF"), Some(f32::INFINITY));
        assert_eq!(read_number("-1.#INF"), Some(-f32::INFINITY));
        assert!(read_number("1.#IND").unwrap().is_nan());
        assert_eq!(read_number("2.5"), Some(2.5));
        assert_eq!(read_number("fred"), None);

        let f = Field {
            name: None,
            raw: "0 0 1.#INF 1.#INF".to_string(),
        };
        assert_eq!(f.numbers().len(), 4);
        assert!(f.numbers()[2].is_infinite());
    }

    fn game_dir() -> Option<std::path::PathBuf> {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

    fn gib_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                gib_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "gib") {
                out.push(path);
            }
        }
    }

    /// Every screen the game ships, parsed. This is the shape of the claim the
    /// parser makes: 116 files, one closed vocabulary of nineteen *named* widget
    /// kinds plus the headerless base `Widget`, five attachments, and no field
    /// this reader could not name.
    #[test]
    fn every_shipped_screen_parses_to_the_same_closed_vocabulary() {
        let Some(ui) = game_dir() else { return };
        let mut files = Vec::new();
        gib_files(&ui, &mut files);
        assert!(files.len() >= 100, "found only {} files", files.len());

        let mut widgets = 0;
        let mut attachment_count = 0;
        let mut kinds = std::collections::BTreeSet::new();
        let mut attachments = std::collections::BTreeSet::new();
        let mut unnamed: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for path in &files {
            let text = std::fs::read_to_string(path).unwrap();
            let gib = parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                gib.version == "1.09" || gib.version == "1.07",
                "{}: version {:?}",
                path.display(),
                gib.version
            );
            for w in gib.walk() {
                widgets += 1;
                kinds.insert(w.kind.name().to_string()); // Every widget carries the writer's whole base set.
                for base in [
                    "Name",
                    "ID",
                    "Position",
                    "Size",
                    "Visible",
                    "ChildOrder",
                    "ClipMode",
                    "NumChildren",
                ] {
                    assert!(
                        w.field(base).is_some(),
                        "{}: a {} with no {base}",
                        path.display(),
                        w.kind.name()
                    );
                }
                *unnamed.entry(w.kind.name().to_string()).or_default() +=
                    w.fields.iter().filter(|f| f.name.is_none()).count();
                if let Some(f) = w.unnamed() {
                    let expected = match w.kind {
                        WidgetKind::Image => 4,
                        WidgetKind::Glyph => 1,
                        _ => 0,
                    };
                    assert_eq!(
                        f.numbers().len(),
                        expected,
                        "{}: {} unlabelled value",
                        path.display(),
                        w.kind.name()
                    );
                }
                for a in &w.attachments {
                    attachment_count += 1;
                    attachments.insert(a.kind.name().to_string());
                    assert!(
                        a.fields.iter().all(|f| f.name.is_some()),
                        "{}: an attached {} with an unlabelled value",
                        path.display(),
                        a.kind.name()
                    );
                    assert!(
                        a.field("NumChildren").is_none(),
                        "{}: an attached {} with NumChildren",
                        path.display(),
                        a.kind.name()
                    );
                }
            }
            // Fragments are a lone attachment at the top level.
            for b in &gib.blocks {
                if let Block::Attachment(a) = b {
                    attachment_count += 1;
                    attachments.insert(a.kind.name().to_string());
                    assert!(a.fields.iter().all(|f| f.name.is_some()));
                }
            }
        }

        let expected_kinds: std::collections::BTreeSet<String> = [
            // The base class, and the only kind the writer leaves *headerless*.
            "Widget",
            "Window",
            "Group",
            "SplitFrame",
            "Image",
            "Text",
            "TextBox",
            "Button",
            "CheckBox",
            "Rectangle",
            "Lines",
            "Slider",
            "ScrollBar",
            "ListBox",
            "Grid",
            "OptionGroup",
            "DropDown",
            "Placeholder",
            "Proxy",
            "Glyph",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(kinds, expected_kinds);
        let expected_attachments: std::collections::BTreeSet<String> =
            ["Background", "Border", "Caption", "DisplayBox", "Scroller"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        assert_eq!(attachments, expected_attachments);

        // The unlabelled value really is one per widget of the two kinds that
        // have one, and nowhere else.
        assert_eq!(unnamed.get("Image").copied().unwrap_or(0), 1539);
        assert_eq!(unnamed.get("Glyph").copied().unwrap_or(0), 40);
        assert_eq!(
            unnamed.values().sum::<usize>(),
            1539 + 40,
            "an unlabelled value appeared on some other kind"
        );
        // The shipped screens, counted.
        assert_eq!(widgets, 4348);
        assert_eq!(attachment_count, 758);
    }
}
