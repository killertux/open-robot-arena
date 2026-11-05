//! Input: what a click lands on, and what a click changes.
//!
//! The shell has had no input until now — screens were placed and drawn, and
//! nothing was ever *pressed*. This is the first half of the workshop's
//! interaction, and it is deliberately small: a click is hit-tested against the
//! placed tree, and the two widgets the shell's own screens use for selection —
//! a `CheckBox` and the `OptionGroup` that holds it — have the state machine the
//! shipped files imply.
//!
//! ## A click is hit-tested against the placed tree, front to back
//!
//! [`hit`] walks the tree the way it is *drawn*, in reverse: the last thing
//! drawn is the first thing a click lands on. Within a subtree the children come
//! before their parent, so a click on a button's caption finds the caption and
//! the caller can walk up to the button — which is what a GUI does, and what
//! [`hit`]'s path return lets it do. A widget outside its clip cannot be hit: a
//! scrolled list's cells stop at the trough.
//!
//! The `PassMouse` field is **not** consulted, and that is the data's doing: it
//! is `true` on all 1,298 widgets in the corpus that carry it, so it
//! distinguishes nothing. It is a base-class field the writer always emits, not
//! a flag the screens vary.
//!
//! ## The state machine is measured off the corpus
//!
//! A `CheckBox` carries up to four `Image` children — the states — and exactly
//! one of them is `Visible`. Across every `OptionGroup` member in all 116
//! screens, with no exceptions:
//!
//! | member | state images | visible |
//! |---|---|---|
//! | the group's `Selection` | 4 | 2 |
//! | the group's `Selection` | 3 | 1 |
//! | any other | 4 or 3 | 0 |
//!
//! So the *unselected* look is always the first image, and the *selected* look is
//! the second-to-last — the last being the disabled state the binary names
//! (`Normal`, `Rollover`, `Clicked`, `Disabled` sit together in its schema). That
//! is the whole rule: [`set_state`] shows `0` or `len - 2`, and [`select_option`]
//! applies it to a group by flipping `Checked` and the images' `Visible`.

use ra2fmt::gib::{AttachmentKind, Widget, WidgetKind};

use crate::{draw_key, draw_order, is_control, Frame, Grid, Placement};

impl Frame {
    /// Whether a point is inside the rectangle. The right and bottom edges are
    /// outside it, so two rectangles that abut do not both claim the boundary.
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
}

/// The widget a click at `x, y` lands on, as a path from the roots.
///
/// `[]` is a root itself; `[2, 0]` is the first child of the third root. Only
/// widgets that are `Visible` and inside their clip can be hit.
pub fn hit(roots: &[Placement<'_>], x: f32, y: f32) -> Option<Vec<usize>> {
    hit_with(roots, x, y, can_take_a_click)
}

/// The **control** a click operates, passing through chrome.
///
/// [`hit`] answers "what is under the point" and can stop on a panel, a proxy's
/// empty rectangle or a label. A shell button must be found even when the file
/// draws a full-screen proxy over its rectangle — `select_team.gib`'s bottom
/// navigation proxy is 1280×50 and covers the `continue button` at 779,443,
/// while the bar's own art ends at x=768 — or when a decorative plate is over
/// it. This variant stops only on [`is_control`] classes; the path still carries
/// the containers above, so a caption's button is found from the path.
pub fn hit_control(roots: &[Placement<'_>], x: f32, y: f32) -> Option<Vec<usize>> {
    hit_with(roots, x, y, is_control)
}

fn hit_with(
    roots: &[Placement<'_>],
    x: f32,
    y: f32,
    takes: fn(&WidgetKind) -> bool,
) -> Option<Vec<usize>> {
    for index in front_to_back(roots.iter().map(|p| p.widget)) {
        let mut path = vec![index];
        if let Some(found) = hit_at(&roots[index], x, y, &mut path, takes) {
            return Some(found);
        }
    }
    None
}

/// Indices ordered front to back: the draw order reversed, ties keeping file
/// order, so what is drawn last is tested first.
fn front_to_back<'a>(widgets: impl Iterator<Item = &'a Widget>) -> Vec<usize> {
    let mut order: Vec<(i32, usize)> = widgets.enumerate().map(|(i, w)| (draw_key(w), i)).collect();
    // Higher draw key is drawn later and so is in front; a negative order on a
    // pressed control draws last (see `ra2ui::draw_key`); where two share it,
    // the **earlier** record is in front — see `draw_order_parented`.
    order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    order.into_iter().map(|(_, i)| i).collect()
}

fn hit_at(
    p: &Placement<'_>,
    x: f32,
    y: f32,
    path: &mut Vec<usize>,
    takes: fn(&WidgetKind) -> bool,
) -> Option<Vec<usize>> {
    for index in front_to_back(p.children.iter().map(|c| c.widget)) {
        path.push(index);
        if let Some(found) = hit_at(&p.children[index], x, y, path, takes) {
            return Some(found);
        }
        path.pop();
    }
    let inside = p.frame.contains_point(x, y) && p.clip.is_none_or(|c| c.contains_point(x, y));
    // A hit does not stop at the base `Widget`. It is the *abstract* class — the
    // one kind the writer leaves headerless, whose only fields are its note and
    // `PassMouse` — so a record of it is a container, with no class behaviour to
    // run. The data draws them over controls on purpose: `ch_stepbuttons.gib`'s
    // `step 1 button`, `step 2 button` and `snap checkbox` each have a nameless
    // `Widget` a pixel larger drawn over them, holding the `1` and the `2`, and
    // `chassis.gib` draws its two notes over its two buttons. Refusing to stop at
    // one lets the search fall through to what is behind it, which is what makes
    // those controls clickable at all.
    (p.widget.visible() && inside && takes(&p.widget.kind)).then(|| path.clone())
}

/// Whether a class can be what a click *lands on*.
///
/// A layout container cannot: it has no behaviour of its own, so a hit passes
/// through it to whatever is behind. The **measured** case is the base `Widget`,
/// the class the writer leaves headerless — `botlab/ch_stepbuttons.gib` draws a
/// nameless one over each of its step buttons, `chassis.gib` draws its notes over
/// its two — and a hit that stopped there would make those controls unclickable.
fn can_take_a_click(kind: &WidgetKind) -> bool {
    !matches!(kind, WidgetKind::Widget)
}

/// The widget a [`hit`] path names, whose first element indexes the `roots`
/// slice rather than a widget's children.
pub fn at_roots<'a>(roots: &'a [Placement<'a>], path: &[usize]) -> Option<&'a Widget> {
    let first = *path.first()?;
    at(roots.get(first)?.widget, &path[1..])
}

/// The widget a path names, relative to one root's own children.
pub fn at<'a>(root: &'a Widget, path: &[usize]) -> Option<&'a Widget> {
    let mut widget = root;
    for step in path {
        widget = widget.children.get(*step)?;
    }
    Some(widget)
}

/// The same, mutably.
pub fn at_mut<'a>(root: &'a mut Widget, path: &[usize]) -> Option<&'a mut Widget> {
    let mut widget = root;
    for step in path {
        widget = widget.children.get_mut(*step)?;
    }
    Some(widget)
}

/// Set a `CheckBox`'s state: `Checked` for the record, and the state image that
/// matches (see the module notes: `0` unselected, `len - 2` selected).
///
/// Returns whether anything changed.
pub fn set_state(widget: &mut Widget, selected: bool) -> bool {
    if widget.kind != WidgetKind::CheckBox {
        return false;
    }
    let count = widget.children.len();
    if count == 0 {
        return false;
    }
    let want = if selected { count - 2 } else { 0 };
    let mut changed = false;
    for (index, child) in widget.children.iter_mut().enumerate() {
        let show = index == want;
        if child.visible() != show {
            set_field(child, "Visible", if show { "true" } else { "false" });
            changed = true;
        }
    }
    let checked = selected.to_string();
    if field_of(widget, "Checked").is_none_or(|c| c != checked) {
        set_field(widget, "Checked", &checked);
        changed = true;
    }
    changed
}

/// Enable or disable a `Button` (`FUN_005ec4e0(enabled)`): a disabled button shows its last state image
/// (`StateImage 3`, the dimmed one: the shipped `*_button_n.bmp` tinted `0.5 0.5 0.5`), an enabled one its
/// first. Returns whether anything changed. A button with one state image only is left alone.
pub fn set_button_enabled(widget: &mut Widget, enabled: bool) -> bool {
    if widget.kind != WidgetKind::Button || widget.children.len() < 2 {
        return false;
    }
    let want = if enabled {
        0
    } else {
        widget.children.len() - 1
    };
    let mut changed = false;
    for (index, child) in widget.children.iter_mut().enumerate() {
        let show = index == want;
        if child.visible() != show {
            set_field(child, "Visible", if show { "true" } else { "false" });
            changed = true;
        }
    }
    changed
}

/// Enable or disable a `CheckBox` (`FUN_005ec4e0` on one of the lab's tabs): its fourth state image
/// (`StateImage 3`, the shipped `_n` art tinted alpha 0.3) is the disabled look. Enabling puts back
/// the selected or the normal image. Returns whether anything changed.
pub fn set_checkbox_enabled(widget: &mut Widget, enabled: bool) -> bool {
    if widget.kind != WidgetKind::CheckBox || widget.children.len() < 4 {
        return false;
    }
    let count = widget.children.len();
    let selected = field_of(widget, "Checked").is_some_and(|c| c == "true");
    let want = if !enabled {
        count - 1
    } else if selected {
        count - 2
    } else {
        0
    };
    let mut changed = false;
    for (index, child) in widget.children.iter_mut().enumerate() {
        let show = index == want;
        if child.visible() != show {
            set_field(child, "Visible", if show { "true" } else { "false" });
            changed = true;
        }
    }
    changed
}

/// Whether a `CheckBox` shows its disabled image (see [`set_checkbox_enabled`]).
pub fn checkbox_dimmed(widget: &Widget) -> bool {
    widget.kind == WidgetKind::CheckBox
        && widget.children.len() >= 4
        && widget.children.last().is_some_and(|c| c.visible())
}

/// Whether a button shows its dimmed state (see [`set_button_enabled`]): the first state image is
/// hidden and the last is shown. Anything else, including a widget that is not a button, is enabled.
pub fn button_dimmed(widget: &Widget) -> bool {
    widget.kind == WidgetKind::Button
        && widget.children.len() >= 2
        && widget.children.first().is_some_and(|c| !c.visible())
        && widget.children.last().is_some_and(|c| c.visible())
}

/// Select `child` in the option group called `group`: the group's `Selection`
/// is set, and every `CheckBox` under it is put into its selected or unselected
/// state.
///
/// Returns the names that changed.
pub fn select_option(root: &mut Widget, group: &str, child: &str) -> Vec<String> {
    let mut changed = Vec::new();
    if !select_in(root, group, child, &mut changed) {
        return Vec::new();
    }
    changed
}

fn select_in(widget: &mut Widget, group: &str, child: &str, changed: &mut Vec<String>) -> bool {
    if widget.name() == group && widget.kind == WidgetKind::OptionGroup {
        set_field(widget, "Selection", child);
        // A `CheckBox`'s own children are its state images, so collect the
        // indices first and then touch each one.
        let targets: Vec<usize> = (0..widget.children.len())
            .filter(|i| widget.children[*i].kind == WidgetKind::CheckBox)
            .collect();
        for index in targets {
            let selected = widget.children[index].name() == child;
            let name = widget.children[index].name().to_string();
            if set_state(&mut widget.children[index], selected) {
                changed.push(name);
            }
        }
        return true;
    }
    widget
        .children
        .iter_mut()
        .any(|c| select_in(c, group, child, changed))
}

/// The index of the selected row of a `ListBox`/`Grid`, if one was selected.
///
/// The selection is the list's own `Sel Bar` (`Grid::selection` names its
/// child `ID`), and the row is recovered from the bar's y divided by the
/// list's `RowHeight`. This port also keeps `_SelectedRow` on the list so a
/// selection at row 0 is distinguishable from the file's unselected bar; the
/// field is our in-memory marker and is never written to disk.
pub fn selected_row(list: &Widget) -> Option<usize> {
    if let Some(field) = list.field("_SelectedRow") {
        return field.number().map(|n| n.max(0.0) as usize);
    }
    let grid = Grid::from_widget(list)?;
    let bar = selection_bar(list, &grid)?;
    if !bar.visible() {
        return None;
    }
    let y = bar.position()?.second.amount;
    (grid.row_height > 0.0).then(|| (y / grid.row_height).max(0.0) as usize)
}

/// Select `row` in a `ListBox`/`Grid`: move its `Sel Bar` to the row, mark it
/// visible, and remember the index. Returns `false` when the list is not a
/// `Grid`/`ListBox` or the row has no text child.
pub fn select_row(list: &mut Widget, row: usize) -> bool {
    let Some(grid) = Grid::from_widget(list) else {
        return false;
    };
    if grid.row_height <= 0.0 || row >= row_count(list) {
        return false;
    }
    let y = grid.row_height * row as f32;
    let Some(bar) = selection_bar_mut(list, &grid) else {
        return false;
    };
    set_field(bar, "Position", &format!("0 0 {y} 0"));
    set_field(bar, "Visible", "true");
    set_field(list, "_SelectedRow", &row.to_string());
    true
}

/// The text rows a list has drawn. The file's own `Sel Bar`/`High Bar` are not
/// text, so they are not rows.
pub fn row_count(list: &Widget) -> usize {
    list.children
        .iter()
        .filter(|c| c.kind == WidgetKind::Text && c.field("_Font").is_some())
        .count()
}

fn selection_bar<'a>(list: &'a Widget, grid: &Grid) -> Option<&'a Widget> {
    grid.selection
        .and_then(|id| {
            list.children
                .iter()
                .find(|c| c.field("ID").and_then(|f| f.number()) == Some(id as f32))
        })
        .or_else(|| list.children.iter().find(|c| c.name() == "Sel Bar"))
}

fn selection_bar_mut<'a>(list: &'a mut Widget, grid: &Grid) -> Option<&'a mut Widget> {
    let index = match grid.selection {
        Some(id) => list
            .children
            .iter()
            .position(|c| c.field("ID").and_then(|f| f.number()) == Some(id as f32)),
        None => None,
    }
    .or_else(|| list.children.iter().position(|c| c.name() == "Sel Bar"))?;
    list.children.get_mut(index)
}

/// A widget's field as text.
pub fn field_of<'a>(widget: &'a Widget, name: &str) -> Option<&'a str> {
    widget.field(name).map(|f| f.text())
}

/// Replace a field's value, adding the field if the record does not carry it.
pub fn set_field(widget: &mut Widget, name: &str, value: &str) {
    if let Some(field) = widget.field_mut(name) {
        field.raw = value.to_string();
        return;
    }
    widget.fields.push(ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw: value.to_string(),
    });
}

/// Forget a list's selection: hide its bar and drop the in-memory row/column
/// markers. A re-filled list starts with no row, which is what the screen shows
/// until the player clicks one (the shipped `Sel Bar` position is an author's
/// leftover, not a selection).
pub fn clear_selection(list: &mut Widget) {
    list.fields.retain(|field| {
        !matches!(
            field.name.as_deref(),
            Some("_SelectedRow" | "_SelectedColumn")
        )
    });
    if let Some(grid) = Grid::from_widget(list) {
        if let Some(bar) = selection_bar_mut(list, &grid) {
            set_field(bar, "Visible", "false");
        }
    }
}

/// Every widget of a screen in draw order, back to front — the order a click
/// tests in reverse.
pub fn draw_order_of<'a>(roots: &'a [Placement<'a>]) -> Vec<&'a Placement<'a>> {
    draw_order(roots)
}

/// The rows a list draws: its content children, i.e. everything except the
/// selection and highlight bars.
///
/// A text list's rows are `Text` children ([`row_count`]); a robot grid's cells
/// are the `choosebot_cell.gib` groups, so the count has to be the content,
/// not just the text.
pub fn content_count(list: &Widget) -> usize {
    let grid = Grid::from_widget(list);
    list.children
        .iter()
        .filter(|child| !is_bar(child, grid.as_ref()))
        .count()
}

fn is_bar(child: &Widget, grid: Option<&Grid>) -> bool {
    if child.name() == "Sel Bar"
        || child.name() == "High Bar"
        || child.name().starts_with("row line")
    {
        return true;
    }
    let Some(grid) = grid else { return false };
    let id = child.field("ID").and_then(|f| f.number());
    [grid.selection, grid.highlight]
        .into_iter()
        .flatten()
        .any(|wanted| id == Some(wanted as f32))
}

/// How many rows of `list` the screen shows at once.
///
/// The arena picker's own `Rows 6` is the visible count. A drop-down list spells
/// `Rows 1` (its collapsed state) and the screen sizes the open list, so there
/// the count is the list's own height divided by its row height — five rows for
/// the robot picker's 100-pixel team list.
pub fn visible_rows(list: &Widget) -> usize {
    let Some(grid) = Grid::from_widget(list) else {
        return 0;
    };
    // The viewport the shell measured from the laid-out scroller.
    if let Some(rows) = field_of(list, "_ViewRows").and_then(|v| v.parse::<usize>().ok()) {
        return rows;
    }
    if grid.rows > 1 {
        return grid.rows as usize;
    }
    if grid.row_height <= 0.0 {
        return 0;
    }
    let height = list.size().map(|s| s.second.amount).unwrap_or(0.0);
    ((height / grid.row_height).floor() as usize).max(1)
}

/// Scroll a single-column list to `offset`, clamped to its content.
///
/// The rows are the list's own content children: their `Position` is moved so
/// the first visible row is at the top, and rows outside the viewport are
/// hidden — a click cannot reach a row the mouse cannot see. The selection bar
/// moves with its row. The `+Scroller` attachment is shown when there is
/// something to scroll, and its thumb is placed at the offset's fraction.
///
/// Returns the clamped offset. Lists with more than one column (the robot
/// picker's `6 × 1` grid) are not scrolled: their cells all share row 0.
pub fn scroll_rows(list: &mut Widget, offset: usize) -> usize {
    let Some(grid) = Grid::from_widget(list) else {
        return 0;
    };
    if grid.row_height <= 0.0 || grid.columns > 1 {
        return 0;
    }
    let count = content_count(list);
    let visible = visible_rows(list).max(1);
    let max = count.saturating_sub(visible);
    let offset = offset.min(max);
    let height = grid.row_height;
    let selected = field_of(list, "_SelectedRow").and_then(|value| value.parse::<usize>().ok());
    // A drop-down's list is sized by the screen (100 px of 18-px rows): the row the height only
    // reaches into shows cut off at the clip (live `exhibition-time-open`: "5" half drawn).
    let list_height = list.size().map(|s| s.second.amount).unwrap_or(0.0);
    let partial = usize::from(
        grid.rows <= 1
            && field_of(list, "_ViewRows").is_none()
            && list_height > visible as f32 * height + 1.0
            && count > offset + visible,
    );
    let mut ordinal = 0;
    for child in list.children.iter_mut() {
        if is_bar(child, Some(&grid)) {
            continue;
        }
        let shown = ordinal >= offset && ordinal < offset + visible + partial;
        let (dx, dy) = if child.kind == WidgetKind::Text {
            (ROW_TEXT_X, ROW_TEXT_Y)
        } else {
            (0.0, 0.0)
        };
        set_field(
            child,
            "Position",
            &format!(
                "{dx} 0 {} 0",
                (ordinal as f32 - offset as f32) * height + dy
            ),
        );
        set_field(child, "Visible", if shown { "true" } else { "false" });
        ordinal += 1;
    }
    if let Some(selected) = selected {
        if selected < count {
            if let Some(bar) = selection_bar_mut(list, &grid) {
                set_field(
                    bar,
                    "Position",
                    &format!("0 0 {} 0", (selected as f32 - offset as f32) * height),
                );
                set_field(
                    bar,
                    "Visible",
                    if selected >= offset && selected < offset + visible {
                        "true"
                    } else {
                        "false"
                    },
                );
            }
        }
    }
    set_field(list, "_ScrollTop", &offset.to_string());
    set_scroller_visible(list, count > visible, offset, max);
    offset
}

/// Select a cell of a `ListBox`/`Grid`: the row and column the click computed.
///
/// `RowSelect` lists select the whole row, so the column is 0. The selection bar
/// is moved to the cell's own position (its x is the sum of the columns before
/// it) and the index is remembered in `_SelectedRow`/`_SelectedColumn`.
pub fn select_cell(list: &mut Widget, row: usize, column: usize) -> bool {
    let Some(grid) = Grid::from_widget(list) else {
        return false;
    };
    let count = content_count(list);
    let columns = grid.columns.max(1) as usize;
    if row.saturating_mul(columns).saturating_add(column) >= count {
        return false;
    }
    let column = if grid.row_select { 0 } else { column };
    let y = grid.row_height * row as f32;
    let x = grid.column_x(column as i32);
    let even = field_of(list, "_EvenCells") == Some("true");
    let (rows_n, cols_n) = (grid.rows.max(1) as f32, columns as f32);
    let Some(bar) = selection_bar_mut(list, &grid) else {
        return false;
    };
    if even {
        // The list divides its rectangle evenly (the Team HQ inventory); the frame is the cell.
        let (w, h) = (1.0 / cols_n, 1.0 / rows_n);
        // The bar's `Lines` frame is drawn in the file's 213 x 70 pixels; the original's frame is the whole
        // cell (live `teamhq-scrap-selected`: 618..957 x 373..493 at 1024x768), so its points become
        // fractions of the bar.
        let native = bar
            .field("Size")
            .map(|f| f.numbers())
            .filter(|n| n.len() >= 4 && n[1] == 0.0 && n[3] == 0.0 && n[0] > 0.0 && n[2] > 0.0)
            .map(|n| (n[0], n[2]));
        if let Some((nw, nh)) = native {
            relative_points(bar, nw, nh);
        }
        set_field(
            bar,
            "Position",
            &format!("{} 1 {} 1", column as f32 * w, row as f32 * h),
        );
        set_field(bar, "Size", &format!("{w} 1 {h} 1"));
    } else {
        set_field(bar, "Position", &format!("{x} 0 {y} 0"));
    }
    set_field(bar, "Visible", "true");
    set_field(list, "_SelectedRow", &row.to_string());
    set_field(list, "_SelectedColumn", &column.to_string());
    let top = field_of(list, "_ScrollTop")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if top > 0 {
        // The bar was just written in unshifted row coordinates; put it back
        // into the scrolled list so it stays on the row the user sees.
        let visible = visible_rows(list).max(1);
        let y = (row.saturating_sub(top)) as f32 * grid.row_height;
        if let Some(bar) = selection_bar_mut(list, &grid) {
            set_field(bar, "Position", &format!("{x} 0 {y} 0"));
            set_field(
                bar,
                "Visible",
                if row >= top && row < top + visible {
                    "true"
                } else {
                    "false"
                },
            );
        }
    }
    true
}

/// Turn the absolute `Point`s of every `Lines` under `widget` into fractions of a `nw` x `nh` box.
fn relative_points(widget: &mut Widget, nw: f32, nh: f32) {
    if widget.kind == WidgetKind::Lines {
        for field in widget
            .fields
            .iter_mut()
            .filter(|f| f.name.as_deref() == Some("Point"))
        {
            let n = field.numbers();
            if n.len() >= 4 && n[1] == 0.0 && n[3] == 0.0 {
                field.raw = format!("{} 1 {} 1", n[0] / nw, n[2] / nh);
            }
        }
    }
    for child in widget.children.iter_mut() {
        relative_points(child, nw, nh);
    }
}

/// The selected cell of a list: `(row, column)`, or `(row, 0)` for a
/// row-selecting list.
pub fn selected_cell(list: &Widget) -> Option<(usize, usize)> {
    if let Some(row) = field_of(list, "_SelectedRow").and_then(|v| v.parse::<usize>().ok()) {
        let column = field_of(list, "_SelectedColumn")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        return Some((row, column));
    }
    selected_row(list).map(|row| (row, 0))
}

/// The `ScrollBar` the list's `+Scroller` attachment carries, when it has one.
pub fn scroller(list: &Widget) -> Option<&Widget> {
    let body = list
        .attachments
        .iter()
        .find(|a| a.kind == AttachmentKind::Scroller)?
        .body
        .as_deref()?;
    scrollbar_in(body)
}

fn scrollbar_in(widget: &Widget) -> Option<&Widget> {
    if widget.kind == WidgetKind::ScrollBar {
        return Some(widget);
    }
    widget.children.iter().find_map(scrollbar_in)
}

fn scrollbar_in_mut(widget: &mut Widget) -> Option<&mut Widget> {
    if widget.kind == WidgetKind::ScrollBar {
        return Some(widget);
    }
    widget.children.iter_mut().find_map(scrollbar_in_mut)
}

/// Show or hide the list's scrollbar, and place its thumb at `offset / max`.
/// Show or hide a list's `Scroller` bar (the original hides it while every row fits).
pub fn show_scroller(list: &mut Widget, visible: bool) {
    set_scroller_visible(list, visible, 0, 0);
}

fn set_scroller_visible(list: &mut Widget, visible: bool, offset: usize, max: usize) {
    let track = list.size().map(|s| s.second.amount).unwrap_or(0.0);
    let Some(body) = list
        .attachments
        .iter_mut()
        .find(|a| a.kind == AttachmentKind::Scroller)
        .and_then(|a| a.body.as_deref_mut())
    else {
        return;
    };
    // A dropdown's list keeps its bar in a hidden wrapper (`Visible false`,
    // `ClipMode 2`); the exe shows the wrapper with the bar.
    if !visible || body.kind != WidgetKind::ScrollBar {
        set_field(body, "Visible", if visible { "true" } else { "false" });
    }
    let Some(bar) = scrollbar_in_mut(body) else {
        return;
    };
    set_field(bar, "Visible", if visible { "true" } else { "false" });
    let model = crate::ScrollBar::from_widget(bar);
    if visible {
        reveal_trough(bar);
    }
    // The three parts take their visibility from the bar; the file leaves the
    // buttons and the track hidden because the control was never enabled.
    for index in [model.less, model.more, model.slider] {
        if let Some(part) = bar.children.get_mut(index) {
            set_field(part, "Visible", if visible { "true" } else { "false" });
            if index == model.slider {
                thumb(part, offset, max, track);
            }
        }
    }
}

/// Place a horizontal bar's thumb `x` pixels from the left end of its slider track (the thumb is the slider's
/// own `Image`). `true` when the list has such a bar.
pub fn set_horizontal_thumb(list: &mut Widget, x: f32) -> bool {
    let Some(body) = list
        .attachments
        .iter_mut()
        .find(|a| a.kind == AttachmentKind::Scroller)
        .and_then(|a| a.body.as_deref_mut())
    else {
        return false;
    };
    let Some(bar) = scrollbar_in_mut(body) else {
        return false;
    };
    let model = crate::ScrollBar::from_widget(bar);
    let Some(image) = bar.children.get_mut(model.slider).and_then(|slider| {
        slider
            .children
            .iter_mut()
            .find(|c| c.kind == WidgetKind::Image)
    }) else {
        return false;
    };
    set_field(image, "Position", &format!("{} 0 0 0", x.round()));
    true
}

/// The scroll trough (`UI_ScrollBack.bmp`) is a hidden widget of a drop-down's bar in the file; the
/// bar shows it with its parts.
fn reveal_trough(widget: &mut Widget) -> bool {
    let mut found = widget.kind == WidgetKind::Image
        && widget
            .field("_Filename")
            .is_some_and(|f| f.text().to_ascii_lowercase().contains("scrollback"));
    for child in widget.children.iter_mut() {
        found |= reveal_trough(child);
    }
    // The trough hangs on the slider's `Background` attachment.
    for attachment in widget.attachments.iter_mut() {
        if let Some(body) = attachment.body.as_deref_mut() {
            found |= reveal_trough(body);
        }
    }
    if found && widget.kind != WidgetKind::Image {
        set_field(widget, "Visible", "true");
    }
    found
}

fn thumb(slider: &mut Widget, offset: usize, max: usize, track: f32) {
    let Some(image) = slider
        .children
        .iter_mut()
        .find(|c| c.kind == WidgetKind::Image)
    else {
        return;
    };
    let thumb_height = image.size().map(|s| s.second.amount).unwrap_or(0.0);
    let travel = (track - 2.0 * 16.0 - thumb_height).max(0.0);
    let fraction = if max == 0 {
        0.0
    } else {
        offset as f32 / max as f32
    };
    set_field(
        image,
        "Position",
        &format!("0 0 {} 0", (fraction * travel).round()),
    );
}

/// Where a row's text sits inside its row box, measured against the original's
/// arena list (best shift of our render onto the capture: 2 px right, 5 px up).
pub(crate) const ROW_TEXT_X: f32 = 2.0;
pub(crate) const ROW_TEXT_Y: f32 = -5.0;

/// Fill a single-column list with text rows, in the face the arena picker uses.
///
/// The row height and width are the list's own `Sel Bar` (`159 0 18 0` for the
/// arena picker); a re-fill replaces the rows it made and leaves the file's two
/// bars alone. Returns how many rows were written.
pub fn text_rows(list: &mut Widget, texts: &[String]) -> usize {
    let Some(bar) = list.children.iter().find(|c| c.name() == "Sel Bar") else {
        return 0;
    };
    let height = bar.size().map(|s| s.second.amount).unwrap_or(18.0);
    let width = bar.size().map(|s| s.first.amount).unwrap_or(159.0);
    // The list's own face when it states one (the picker's dropdown says
    // `fg_mcn16`); the arena picker states none, so it uses the shell's most
    // common row face (`ra2ui::arenas::ROW_FACE`).
    let face = field_of(list, "_Font")
        .unwrap_or(crate::arenas::ROW_FACE)
        .to_string();
    list.children
        .retain(|c| c.kind != WidgetKind::Text || c.field("_Font").is_none());
    let field = |name: &str, raw: &str| ra2fmt::gib::Field {
        name: Some(name.to_string()),
        raw: raw.to_string(),
    };
    let mut made = 0;
    for (i, text) in texts.iter().enumerate() {
        let y = height * i as f32;
        list.children.push(Widget {
            kind: WidgetKind::Text,
            implicit: false,
            fields: vec![
                field("Name", ""),
                field("ID", "-1"),
                field("Position", &format!("{ROW_TEXT_X} 0 {} 0", y + ROW_TEXT_Y)),
                field("Size", &format!("{width} 0 {height} 0")),
                field("Visible", "true"),
                // In front of the file's own bars (`ChildOrder` 3 and 4), as the
                // capture draws the selected row's text white over its bar.
                field("ChildOrder", "10"),
                field("ClipMode", "0"),
                field("NumChildren", "0"),
                field("_Font", &face),
                field("_Text", text),
                field("_ForeColor", "1 1 1 1"),
                field("_Name", ""),
                field("_Position", "0 0 0 0"),
            ],
            attachments: Vec::new(),
            children: Vec::new(),
        });
        made += 1;
    }
    made
}

/// The list's own cell lines (`BorderColor 0 0 0 1`, `BorderWidth 1`) for a list whose cells are
/// not made as widgets (the import window's file list): one black pixel at the top of every row
/// (also the empty ones, 8 rows in the capture) and one down the left edge, none at the right or
/// under the last row (`teamhq-import-dialog`: lines at y = 0, 18, ... 126 of the list, x = 0).
/// Drawn under the row text.
pub fn row_lines(list: &mut Widget, rows: usize) {
    list.children.retain(|c| !c.name().starts_with("row line"));
    let Some(height) = Grid::from_widget(list).map(|g| g.row_height) else {
        return;
    };
    let line = |name: String, pos: String, size: &str| {
        let field = |name: &str, raw: &str| ra2fmt::gib::Field {
            name: Some(name.to_string()),
            raw: raw.to_string(),
        };
        Widget {
            kind: WidgetKind::Rectangle,
            implicit: false,
            fields: vec![
                field("Filled", "true"),
                field("FillColor", "0 0 0 1"),
                field("BorderWidth", "0"),
                field("BorderColor", "0 0 0 0"),
                field("Name", &name),
                field("ID", "-1"),
                field("Position", &pos),
                field("Size", size),
                field("Visible", "true"),
                field("ChildOrder", "6"),
                field("ClipMode", "0"),
                field("NumChildren", "0"),
            ],
            attachments: Vec::new(),
            children: Vec::new(),
        }
    };
    for i in 0..rows {
        list.children.push(line(
            format!("row line {i}"),
            format!("0 0 {} 0", height * i as f32),
            "1 1 1 0",
        ));
    }
    list.children
        .push(line("row line left".into(), "0 0 0 0".into(), "1 0 1 1"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{resolve, DESIGN_SCREEN};
    use ra2fmt::gib::Gib;

    fn ui_root() -> Option<std::path::PathBuf> {
        ra2fmt::saves::isolate_for_tests();
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

    fn screen(relative: &str) -> Option<Gib> {
        let ui = ui_root()?;
        std::fs::read_to_string(ui.join(relative))
            .ok()
            .and_then(|t| ra2fmt::gib::parse(&t).ok())
    }

    /// The state rule, re-derived from the file rather than trusted: the
    /// group's `Selection` shows the second-to-last state image and every other
    /// member shows the first.
    #[test]
    fn the_shipped_selection_is_the_second_to_last_state_image() {
        let Some(gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let mut groups = 0;
        for w in gib.walk() {
            if w.kind != WidgetKind::OptionGroup {
                continue;
            }
            groups += 1;
            let selected = field_of(w, "Selection").unwrap_or("").to_string();
            for child in w.children.iter().filter(|c| c.kind == WidgetKind::CheckBox) {
                let images: Vec<&Widget> = child
                    .children
                    .iter()
                    .filter(|c| c.kind == WidgetKind::Image)
                    .collect();
                let visible: Vec<usize> = images
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.visible())
                    .map(|(n, _)| n)
                    .collect();
                let want = if child.name() == selected {
                    images.len() - 2
                } else {
                    0
                };
                assert_eq!(
                    visible,
                    vec![want],
                    "{} should show image {want}",
                    child.name()
                );
            }
        }
        assert_eq!(groups, 1, "the toolbar's own option group");
    }

    /// A click lands on the widget drawn last, and on the innermost one under
    /// the point — the caption inside a button rather than the button itself.
    #[test]
    fn a_click_hits_the_innermost_widget_drawn_under_it() {
        let Some(gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let roots = resolve(&gib, DESIGN_SCREEN);
        // `chassis button` is the second tab, placed at 431,37 100x24.
        let path = hit(&roots, 437.0, 120.0).expect("a click on the chassis tab");
        let widget = at_roots(&roots, &path).unwrap();
        assert!(
            widget.kind == WidgetKind::Image || widget.kind == WidgetKind::CheckBox,
            "hit {} {}",
            widget.kind.name(),
            widget.name()
        );
        assert!(
            path.len() >= 2,
            "the innermost widget under the point, not the root: {path:?}"
        );
        // Away from the toolbar the hit is the screen behind it, never a tab.
        let elsewhere = hit(&roots, 20.0, 900.0).expect("the screen itself");
        assert!(!at_roots(&roots, &elsewhere)
            .unwrap()
            .name()
            .ends_with("button"));
        // Right of the screen: nothing at all.
        assert!(hit(&roots, 2000.0, 900.0).is_none());
    }

    /// Clicking a tab makes it the group's selection and flips the two images —
    /// the change is in the *document*, which is where the renderer reads it.
    #[test]
    fn selecting_a_tab_flips_the_state_images() {
        let Some(mut gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            ra2fmt::gib::Block::Widget(w) => Some(w),
            _ => None,
        });
        let root = root.unwrap();

        let changed = select_option(root, "blab option group", "wiring button");
        assert_eq!(changed.len(), 2, "overview and wiring both change");
        // The group says so, and the images do.
        let group = crate::workshop::find(root, "blab option group").unwrap();
        assert_eq!(field_of(group, "Selection"), Some("wiring button"));
        let state = |name: &str| {
            let button = crate::workshop::find(root, name).unwrap();
            let images: Vec<&Widget> = button
                .children
                .iter()
                .filter(|c| c.kind == WidgetKind::Image)
                .collect();
            let visible = images.iter().position(|i| i.visible()).unwrap();
            (field_of(button, "Checked").unwrap().to_string(), visible)
        };
        assert_eq!(state("wiring button"), ("true".to_string(), 2));
        assert_eq!(state("overview button"), ("false".to_string(), 0));
        // Untouched members keep the state they had.
        assert_eq!(state("chassis button"), ("false".to_string(), 0));
    }

    /// `set_state` leaves a widget that is not a `CheckBox` alone.
    #[test]
    fn only_check_boxes_have_states() {
        let Some(mut gib) = screen("botlab/botlabwin.gib") else {
            return;
        };
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            ra2fmt::gib::Block::Widget(w) => Some(w),
            _ => None,
        });
        assert!(!set_state(root.unwrap(), true));
    }

    /// The arena list's own `Rows 6` is the viewport: a five-row offset hides
    /// rows 0–4, puts row 5 at the top and moves the selection bar with its
    /// row. A cell selection on the robot grid keeps the column.
    #[test]
    fn the_arena_list_scrolls_its_own_rows() {
        let Some(mut gib) = screen("exhibition/arena.gib") else {
            return;
        };
        let root = gib.blocks.iter_mut().find_map(|b| match b {
            ra2fmt::gib::Block::Widget(w) => Some(w),
            _ => None,
        });
        let list = crate::workshop::find_mut(root.unwrap(), crate::arenas::LIST).unwrap();
        let names: Vec<String> = (0..12).map(|i| format!("Arena {i}")).collect();
        assert_eq!(text_rows(list, &names), 12);
        assert_eq!(content_count(list), 12);
        assert_eq!(visible_rows(list), 6, "the list's own Rows");
        assert_eq!(scroll_rows(list, 0), 0);
        assert_eq!(scroll_rows(list, 5), 5);
        assert_eq!(field_of(list, "_ScrollTop"), Some("5"));
        let rows: Vec<&Widget> = list
            .children
            .iter()
            .filter(|c| c.kind == WidgetKind::Text)
            .collect();
        assert!(rows[..5].iter().all(|row| !row.visible()));
        assert!(rows[5..11].iter().all(|row| row.visible()));
        assert!(!rows[11].visible(), "row 11 is past the six-row viewport");
        assert_eq!(rows[5].position().unwrap().second.amount, ROW_TEXT_Y);
        assert!(select_cell(list, 10, 0));
        assert_eq!(selected_row(list), Some(10));
        assert_eq!(selected_cell(list), Some((10, 0)));
        let bar = list
            .children
            .iter()
            .find(|c| c.name() == "Sel Bar")
            .unwrap();
        assert!(bar.visible(), "row 10 is in the scrolled viewport");
        assert_eq!(bar.position().unwrap().second.amount, 5.0 * 18.0);
    }
}
