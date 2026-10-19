# 03 — Native Engine Design

Target: a from-scratch, cross-platform engine (Linux / macOS / modern Windows)
that runs *Robot Arena 2* using the player's own game data.

## Why this is feasible

1. **Data formats are documented** — GMI/GMF meshes (with embedded Havok physics),
   `.bot` robots, components, arenas. Our `tools/gmi.py` already parses 181/181
   meshes.
2. **A lot of game logic is Python** — AI strategies, arena hazards, match flow,
   UI helpers. It is readable and reimplementable rather than opaque machine code.
3. **The engine's object model is exposed** — Boost.Python bindings reveal the C++
   classes the scripts use (`Arena`, `AI`, `SoundWizard`, `ArenaManager`).

## Proposed stack

| Concern | Choice | Rationale |
|---|---|---|
| Language | **Rust** | `wgpu` spans Vulkan/Metal/D3D12; strong asset/graphics crates; safety for a long-lived project. |
| Graphics | **wgpu** (Vulkan/Metal/D3D12) | The original is fixed-function D3D8; we emulate its look with modern shaders. |
| Window/input | `winit` | Cross-platform. |
| Audio | `rodio` / `kira` | Plays the original `.wav`/`.mp3`. |
| Physics | **`havok17`** (default; Havok 1.7 dynamics written from the recovered spec, Jolt as its geometry engine) or **Jolt** (`RA2_PHYSICS=jolt`, via the safe `oxijolt` bindings) | See `docs/29-havok17-backend.md`. Hidden behind `ra2sim`'s `PhysicsBackend` trait so it can be swapped (e.g. Rapier) without touching the sandbox. The original is **Havok Hardcore 1.7.1** — see `docs/05-validation-report.md` for the mapping and the differences that matter (friction combination, torque-driven motors). |
| Math | `glam` | |
| Format parsing | our own crates | GMI, BOT, CFL3, GIB, TGA/BMP. |

## Repository layout (target)

```
engine/
  crates/
    ra2fmt/      # pure data: GMI(.gmf), GMA(ascii), BOT, CFL3(.cfz), GIB, textures
    ra2sim/      # physics (Jolt behind a PhysicsBackend trait), scene assembly, sim
    ra2script/   # the plus/Arenas/AI object model on an embedded CPython 3
    ra2ui/       # the shell's layout model: .gib widget trees -> rectangles
    ra2-render/  # wgpu renderer: meshes, and the shell's 2D screens (`ui`)
    ra2-shell/   # the shell's entry point: boot the menu, navigate it headlessly
    ra2app/      # winit app: menus, workshop, matches                            (planned)
tools/           # python tooling (format probing, conversion, diffs)
local/           # (git-ignored) game files + converted cache
```

## Milestones (each independently demoable)

- **M1 — Asset viewer.** ✅ Load a stock `.bot` + its components' `.gmf` +
  textures and render the assembled robot; load an arena `.gmf` and render it;
  assemble arena scenes from their `.py` scripts.
- **M2 — Physics sandbox.** ✅ Drop a bot into an arena with Jolt; drive it;
  collide with walls and movable props. *(Implemented as `ra2sim` + `ra2-sim`,
  verified headlessly by rendering a frame strip.)*
- **M3 — Two robots fight.** ✅ Weapons/motors via the component/controller model;
  damage + buckling *(implemented as `ra2sim::rig`/`robot`/`combat`/`duel` plus the
  `ra2-duel` runner)*.
- **M4 — Script layer.** ✅ The engine side of the `plus`/`Arenas`/`AI` object
  model, on an embedded CPython 3, running the game's own Python 2 sources
  unmodified. The arena surface, the shipped AI and the hazards have all landed —
  with the AI in place the headline duel closes and trades blows, and standing on
  a registered SmartZone fires the arena's own spikes *(implemented as
  `ra2script` + `ra2sim::hazards`)*. What remains is presentation, not behaviour:
  the camera, audio and particle calls are accepted and recorded rather than
  played (F13 in [`06-deferred-fidelity.md`](06-deferred-fidelity.md)).
- **M5 — Game shell.** ✅ Done, with two named residues (below). A `.gib` reader
  (`ra2fmt::gib`) parses all 116 shipped screens — the main menu, team HQ, the
  bot lab, the HUD, every dialog — into widget trees, and `ra2ui` resolves those
  trees into rectangles: `Position`/`Size` with their fraction-or-pixels flags,
  `Alignment` as the 3×3 anchor, split frames into their two panes. All 4,348
  widgets place with finite frames, 57 of the 116 files entirely on screen, and
  the main menu's logo, backdrop and button strip land where its own art says
  they should — which is checked by *looking*: `tools/ui_compose.py` rasterises a
  screen from the engine's own rectangles and the game's own art, and the main
  menu comes out as the main menu. The `fg_*.giff` faces are parsed and *drawn*
  (`ra2fmt::giff`, `ra2 font`, and `tools/ui_compose.py` renders a screen's
  labels with the game's own faces and colours — the main menu's `MULTIPLAYER
  TEAM HQ` reads as itself), a resolver given those faces sizes a zero-`Size`
  label from its own text, and the containers place their own children:
  `ClipMode` 0/1/2 into a clip rectangle, a `Grid`/`ListBox`'s cells from
  `Rows`/`Columns`/`RowHeight`/`ColWidths`, and a `ScrollBar`'s three parts at
  its two ends and between. The screens are *drawn*: `ra2-render`'s 2D path
  (`ui`) turns a placed screen into textured quads — art, fills and glyph runs,
  each carrying the clip `ClipMode` worked out — and draws them headlessly, so
  `uishot <screen.gib> <out.png>` renders any of the 116 shipped screens. The
  cells' *content* is filled from the game's own data for the shipped lists
  (`ra2ui::cells`, `components`, `events`, `standings`, `results`); the lists
  whose source is runtime state are named in the recon notes, and the two S2
  screens take that state as an explicit input. The visual
  pass is done: tiles repeat at their texture's own size, the selection bars draw
  `Point Selection.tga` (the game's own art, named by no screen), captions are
  found by their `Caption` attachment and centred in their buttons, and the
  bottom bar's chrome — written after the bar rather than in it — lands in the
  bar. Every `+Background` draws what its own body says: a colour fill or a
  texture, behind its widget. `ra2-shell` boots the shipped `Scripts/Main.py`,
  opens the data's main menu and walks the menu → bot lab and menu → arena-picker
  routes by hit-tested clicks (S6). **Named residues:** some title
  boxes are narrower than the shifted advance table's extent, so they clip as
  the data says; S4's workshop list and paint tools are the game's
  C++ and are named/deferred rather than approximated. The font's rect/page
  packing is settled (S8): cells are the page's own glyph runs, each row is
  rotated by its right-slot records and the advance table is read two records
  on.
- **M6 — Packaging.** ✅ Done for Linux x86_64: the portable tarball, launcher,
  installer, `.desktop`, icon and unpack-and-run smoke are built and measured
  ([`08-packaging.md`](08-packaging.md)). macOS and modern Windows are a
  documented, honest, **unverified** path ([`09-platforms.md`](09-platforms.md));
  no build or artifact exists on either.

## M5 design notes

- **The screens are data, so the screens are what run.** As with the arenas and
  the AI, the shell's screens are not re-authored: every menu, the whole bot lab,
  the team HQ and the in-match HUD are the game's own `.gib` files, parsed and
  placed. What is ours is the reader (`ra2fmt::gib`) and the layout model
  (`ra2ui`), and both are pinned by the corpus rather than by example — the
  reader by parsing all 116 files into a closed vocabulary, the layout by
  resolving all 4,348 widgets and checking that 57 files place entirely on
  screen and that the main menu's own art lands where it says it should.
- **Rules are measured, and the measurement is kept.** `Alignment` is a 3×3
  anchor because every one of the 841 values in the corpus is one of those nine
  combinations, and that check is a test rather than a comment. Where a rule is
  only *inferred* — the split frame's slot order, the anchor's y-down frame —
  the evidence that supports it is written down next to it.
- **The check is a picture, as everywhere else in this project.** The geometry
  rules were read out of the corpus and confirmed three ways by numbers, but the
  thing that settles a layout is the eye: `tools/ui_compose.py` takes
  `uidump --rects` — the resolver's own output — and the game's own `.tga`/`.bmp`
  files, and composites a screen. The main menu's cement background, centred saw
  plate with its blade teeth, `ROBOT 2 ARENA` logo at the top centre and the two
  gears along the bottom are all recognisable at a glance, and the one thing that
  was *not* obvious from the data — that `ChildOrder` is the draw order
  ascending, not the file's order — was found by looking at a wrong picture
  first.
- **The shell's own Python runs, and what it builds is a widget tree.** `plus`
  is the engine's face to the scripts and `giScript` is the engine's side of the
  shell's own `Scripts/Main.py`: the shipped boot file runs as written, hooks the
  interpreter's output into `giScript`, imports every arena and brain, and hands
  the engine `dispatchEvent`. `Gooey.Plain` — the *shipped* wrapper, not a
  paraphrase — then builds its debug windows through `plus.addWindow`/`addText`,
  and what those build is the same `ra2fmt::gib::Widget` a `.gib` screen is made
  of: a `Window` at its `Position`/`Size` with a `Text` per line carrying
  `_Font`/`_Text`/`_ForeColor`. So `ra2ui` places a script's window and
  `ra2-render` draws it by exactly the code path a screen takes —
  `cargo run -p ra2-render --example gooeyshot` is the whole loop in one command.
  That is the workshop's machinery; the workshop is layout and interaction on
  top of it, and nothing about the seam is left to build.
- **A tab can have steps, and the workshop walks them.** The chassis tab's intro
  offers two, and `workshop::STEPS` is the table: `structure design button` and
  `armor button`, each putting a screen into the tab's own `content slot` and
  leaving the tab selected. Where the button's note and the screen's disagree the
  screen wins, and the recon notes carry both — `armor button`'s note names a file
  the game does not ship and a slot the binary does not contain. The demo walks
  both, and it has to *find* the click point rather than take the button's centre:
  `chassis.gib`'s `structure note` label is drawn over its buttons, so a click on
  one's middle lands on the label, which is what a label in front should do.
- **A shape that looks like evidence may not be, and a *note* can be the witness.**
  The shell has 328 pairs of overlapping siblings sharing a `ChildOrder`, and their
  dominant shape — a control followed by the label that names it — reads as proof
  that the later one is in front. It is not proof: those buttons draw no art, so
  the label is in front of nothing either way. What settled the direction was a
  screen whose own *note* requires a control to work: the palette's category
  buttons filter its grid, so they cannot be behind it. When the corpus's shape is
  ambiguous, a screen that says what it does is the better witness.
- **A value can mean *nothing*, and that has to be read off the corpus.** The
  headerless record is the base class, `Position` with both halves fractional is a
  percentage, and a `_Size` of `1280 0 1005 0` is the editor saying *no size* —
  three shapes in the format that are not values but the *absence* of one. Each
  was found by measuring what the corpus does with it (0 `[Widget]` headers in 116
  files, 18 canvas-sized `_Size` fields in 14) rather than by assuming a sentinel.
- **A screen's own note is a specification, and the data behind it is shipped.**
  The component palette's grid says *"fill with components in the current
  category"*, and everything that sentence needs is in the game: the categories
  are the option group's buttons, the category is the definition's `type` — the
  same field the sim's component factory switches on, so the shell and the sim
  cannot disagree about what a component is — and the definitions and their art
  ship in `Components.cfz` and `Components/`. The armour grid and this one are the
  same shape: a screen that states its own content, and game data that is exactly
  that content. The lists whose content is *runtime* rather than shipped — the
  team HQ's standings and the match results — are the same rule with a live
  source: `ra2ui::standings` and `ra2ui::results` take that state as an explicit
  input, because the game never writes it to a file the port can read.
- **The shell is checked against its own index, not only against itself.** The
  game ships the UI editor's project document, and it is a second witness to
  every screen: the name and class of each file's root, and the 489 widgets the
  designer tagged. Reading it (`ra2ui::project`) is cheap and it is a *check* —
  113 files, every root, every tagged widget, in the class stated, with the
  screen's `Proxy`s inlined first because that is what the document describes and
  what the shell draws. Where a file and the document disagree the file wins, and
  the test names the difference: the document is a snapshot, so two files it names
  are gone and three screens are newer than it. That is the same shape as every
  other rule here — a measurement against the corpus that is kept as a test —
  with the corpus being the game's own index of itself.
- **A list is a template repeated, and what fills it is game data.** `ra2ui::cells`
  makes a cell per row at the geometry the list itself states and binds a row's
  values into the children the template's own `Notes` name — and where the data
  and the code disagree the data wins, because there is nothing to re-author: the
  armour grid's four cells are the four `Armor_Definitions.txt` rows, with their
  own textures, descriptions and weights, and `cargo run -p ra2-render --example
  workshop` renders them. The same machinery now covers the lists whose rows are
  **runtime**: `ra2ui::standings` fills `hq_league.gib` and `season_results.gib`
  from the game's team tables (`Teams/Teams.txt` + `AI/Teams.txt`, loaded by
  `FUN_00502150`) with the live league's points as an explicit input — the
  points are the window's own table and never ship — and `ra2ui::results` loads
  `result_cell.gib` into `common/matchresults.gib`'s four `result slot` groups,
  one per bot in the match. Where a note and a file disagree the file wins:
  `hq_robots.gib` says `robot_cell.gib`, and the shipped cell is `bot_cell.gib`.
- **A runtime list states its own shape, and the shape is the test.** The
  standings' 16 rows are not made up: both screens say `Rows 16`, the two team
  tables hold exactly sixteen named teams, and the fill labels the i-th row
  `i + 1` because that is what `FUN_00500230` does. The match results' four
  slots are 478×80 at the file's own positions, and the cell's constructor gives
  the default visibility the template file does not (`immobilized text`,
  `winner text` and `Spectator [TextBox]` start hidden). Tests pin the geometry
  and the bound text (`the_league_grid_is_filled_from_the_standings`,
  `the_match_results_slots_take_the_matchs_own_results`), and the picture is
  `hq_league.png` / `season_results.png` / `matchresults.png` from the workshop
  example.
- **The shell has input, and the workshop's tabs are live.** `ra2ui::input`
  hit-tests a point against the placed tree the way it is drawn — in reverse, so
  the last thing drawn is the first thing hit, and children before their parent
  so a click on a tab's icon still finds the tab. What a click *changes* is the
  data's own: the state images' `Visible` and the `Checked` field, by the rule
  the corpus states with no exceptions (see the recon notes). `workshop::BotLab`
  is that state kept live — clicking a tab flips the toolbar's images and swaps
  the screen in `content slot`, and it deliberately does **not** rebuild the
  screen from disk, which would put the file's baked selection back. The demo is
  a sequence of real clicks, each hit-tested against where the layout put the
  button (`cargo run -p ra2-render --example workshop`).
- **A negative `ChildOrder` means *no order*, not "behind", and what it resolves
  to depends on the record.** The corpus has 613 children with a negative order
  and 73 of them overlap a non-negative sibling. The pressed controls among those
  — a `Button`, `CheckBox`, `TextBox`, `Slider`, `DropDown` — must draw last or
  the main menu would have no `EXHIBITION`/`TEAM HQ`/`MULTIPLAYER` over its saw
  plate, the bottom bar's buttons would be under their panel, and the credits
  window's close button would be under its backdrop. Chrome with a negative order
  stays first: `select_team.gib`'s full-screen `cement img` is -1 and belongs
  behind its dialog. A `Grid`/`ListBox` is an interactive content area, not a
  pressed control, and stays with the chrome: the palette's grid is -1 and the
  category bar above it is -1, the bar is written first, and the screen's note
  says the bar filters the grid. `ra2ui::draw_key` and
  `input::hit_control` share that ordering — the latter skips chrome so a
  full-screen `Proxy` (`select_team.gib`'s nav bar is 1280×50 over the 779,443
  `continue button`) does not swallow a click. Test:
  `a_negative_child_order_draws_last_only_on_a_control`; the earlier reading
  ("-1 is behind") is what hid the main menu's buttons, and the old evidence for
  it was a mis-parented modal backdrop from before the records were cut at their
  own `NumChildren`.
- **The workshop is data too, and it composes.** `ra2ui::workshop` turns
  `botlabwin.gib` plus a tab's fragment into the screen the game shows: the
  fragment goes into the group its own `Notes` names (`content slot`), the info
  bar into `lowersplit`, and the nav bar arrives through the window's `Proxy`.
  The table of which button shows which screen is the toolbar's own `Notes` —
  five of six name their file outright — and the test re-reads those notes from
  the shipped screen so the table cannot drift from the data it came from.
  `cargo run -p ra2-render --example workshop` renders all six tabs. What the
  workshop still lacks is the *interaction*: a tab's own steps (the chassis
  tab's `Chassis 1`/`Chassis 2`/`Armor`), the component list's contents and the
  paint tools are the game's C++, and they are the next slice.
- **A screen is composed of fragments, and the data says which.** A `Proxy`
  names another `.gib` — nine shipped screens name the bottom navigation bar —
  and the fragment is placed inside the proxy's own rectangle, so
  `ra2ui::inline_proxies_in` turns a screen plus its fragments into one widget
  tree before anything is placed. That is the workshop's own mechanism, and it
  is the reason `botlabwin.gib` shows the shell's nav bar without a line of C++.
- **A record's boundary is its own `NumChildren`, and the `[Kind]` header is
  optional.** The parser used to read a block's fields until the next header,
  which is right for the five attachments and wrong for everything else: the
  base `Widget` class is written *without* a header (the writer's `FUN_005e7c20`
  emits it from a virtual call that returns nothing for the base), so a
  headerless record's fields were read as trailing fields of the one before it.
  758 of them — a fifth of every screen. A record now ends at its `NumChildren`
  (checked: no field of a record ever follows it, in all 116 files), a headerless
  record is a `Widget`, and — measured with no exceptions — `sum(NumChildren)`
  plus the headerless records is `records - 1`, so the count names only the
  children that carry a kind. The corpus is 4,348 widgets, not 3,590, and the
  main menu is one tree instead of thirteen roots. What is left over is named:
  an uncounted record whose group is already full has nowhere to go, and 67
  files leave a few at the top level.
- **A shared rule is not a checked rule, and a picture has to be legible.** The
  two renderers agreed on every glyph and were both upside-down: the face's page
  is stored bottom-up and neither swapped the `v`s of a glyph's quad, so the
  agreement the second renderer exists to provide said nothing about it. What
  found it was a *measurement of a shape* — draw a `T` and an `L` and the ink's
  own distribution says which way up they are — and what confirmed it was a
  picture legible enough to *read words in*: the workshop's toolbar now reads
  `Overview Chassis Components Wiring Paint Shop Test Robot`. Two readings of a
  low-zoom render as "mirrored" were wrong along the way, which is why the
  method asks for a word rather than an impression. The same lesson closed the
  horizontal question: the drawing of `LOCAL FJORD 12345` reads, and the
  *column* profile that had suggested otherwise was measuring a window that can
  catch a neighbouring glyph's ink.
- **A rule that is only *half* right shows up as a wrong picture, and the
  workshop is what found the last one.** `MasterSize` is one axis — all
  twenty-six values in the corpus say so — and the resolver had read it as a
  two-axis pair, found no four-number value, and fallen back to *half the
  frame* for every split whose master is not the first slot. Nothing caught it
  because the fallback is exact when `MasterSize` is `0.5 1`, which most of them
  are: `botlabwin.gib`'s `0.14 1` and `repairs.gib`'s `48 0` are what broke the
  tie, and reading it as one axis moved 23 widgets back onto the screen and two
  whole files with them.
- **The top level is a sibling set, and two special cases died when the records
  were cut right.** A file's own records are ordered by `ChildOrder` exactly as
  a widget's children are, so the resolver sorts them and nothing else: a
  backdrop is an `Attachment` (drawn behind its widget), a negative order on
  chrome draws first, and a negative order on a pressed control draws last (see
  the `draw_key` note above). The
  old rule — "the top level keeps its own order" — was read off a main menu that
  was in fact a thirteen-root forest, and the resolver's other special case,
  three bar-chrome images re-parented onto the bar written before them, was
  compensating for records the reader had failed to close. Both are gone, and
  `credits.gib`'s window is visible over its own backdrop for the first time.
- **What the shell still does not draw is named.** `ra2-render`'s `ui` path draws
  a `Glyph` (a wrapped paragraph in its `_Margin` box, with the writer's `\n`
  line breaks) and a `Lines` polyline (its point pairs), so the undrawn kinds are
  down to the `Placeholder` — the rectangle the 3D view fills, which is not the
  shell's to draw at all. The nesting question is now **closed**: each attachment
  owns the headerless base `Widget` record written after the widget it dresses
  (its *body*), so all 116 files nest to one root, and the two backgrounds that
  used to land over their dialogs are behind them. The measurement is in the
  recon notes; the parser's `Attachment::body` is the rule.
- **An attachment is the data's, and its body is what it draws.** A
  `+Background` carries no colour and no filename — only `Tiling`, `Image` and
  `Widget` — so it was tempting to invent a fill. The body says otherwise: a
  headerless base `Widget` with its own frame and exactly one `[Image]` child,
  which is either a **colour fill** (a blank `_Filename` plus `_ForeColor`, 223
  of 295) or a **texture** (72, with the `Tiling true` ones repeating at the
  texture's size). `hq_league.gib` is the worked case: a purple 640×480 fill
  behind the whole screen and a light 607×319 panel behind the grid. The
  renderer draws the body before the widget it dresses and the other four kinds
  after it, and a blank-named `Image` became a fill quad everywhere — which also
  paints the match-results title bar and every button's state background. The
  tests are `every_background_is_a_colour_fill_or_a_texture`,
  `an_attachment_body_follows_the_widget_it_dresses` and
  `the_match_results_background_is_drawn_behind_its_cells`; the pictures are
  `uishot` before/after on `matchresults.gib` and `hq_league.gib`.
- **The renderer is checked against a second renderer, written the other way
  round.** `tools/ui_compose.py` rasterises a screen on the CPU, in Python, from
  the resolver's own rectangles; `ra2-render`'s `ui` path draws the same screen
  with wgpu. They share no code and no algorithm — one blends bytes, the other
  rasterises triangles over a scissor — so when they agree, the *model* is what
  agrees, not an implementation detail. On `mainmenu.gib` the mean per-channel
  difference is 0.42 of 255 and on `hq_league.gib` 0.24, with what is left
  confined to glyph and art edges. It is the noisiest and most useful check in
  the project, and it earned its keep immediately: it found that the composite
  drew `Rectangle` fills not at all, and that *both* of them had a rule wrong.
- **A wrong picture is data too.** The composite turned every glyph's rows over,
  which turned the `M` of `MULTIPLAYER` into something `V`-ish; that single
  wrong pixel rule had already been read as "the font's own tight advances" and
  written into the recon notes as a property of the *font*. It was the
  renderer's, and with it fixed the label reads exactly. The same pass corrected
  the draw order: `ChildOrder` orders *siblings*, and a negative order on a
  pressed control draws last — which is why the menu's buttons and labels sit on
  top of the saw plate rather than under it.
- **The renderer is checked against a second renderer, written the other way
  round.** `tools/ui_compose.py` rasterises a screen on the CPU, in Python, from
  the resolver's own rectangles; `ra2-render`'s `ui` path draws the same screen
  with wgpu. They share no code and no algorithm — one blends bytes, the other
  rasterises triangles over a scissor — so when they agree it is the *model* that
  agrees. On `mainmenu.gib` the mean per-channel difference is 0.42 of 255 and on
  `hq_league.gib` 0.24, with the rest at glyph and art edges. It earned its keep
  at once: it found that the composite drew `Rectangle` fills not at all, and
  that *both* of them had a rule wrong.
- **A wrong picture is data too.** The composite turned every glyph's rows over,
  which turned the `M` of `MULTIPLAYER` into something `V`-ish; that one wrong
  rule had already been read as "the font's own tight advances" and written into
  the recon notes as a property of the *font*. It was the renderer's, and with it
  fixed the label reads exactly. The same pass corrected the draw order:
  `ChildOrder` orders *siblings*, and a negative order on a pressed control
  draws last.
- **What is not resolved is not guessed.** Zero `Size` (the editor's automatic
  size, which needs font metrics), `ClipMode`, and the containers that lay their
  own children out were each named as unresolved rather than approximated, and
  the corpus test records how many widgets that affects. As each fell, the
  measurement that fell it came with it: the 4,348 widgets' `ClipMode`s are
  2289/1999/60 and 47 of the 60 parent-clipped ones stick out of their
  parents; the 37 selection bars all hang from the row they name at the row
  height, and 31 of the 37 span their columns; the 30 scroll bars' three part
  indices agree with the children's names and kinds. Each of those is a test,
  and where the data does something else — six authored widths, four rows in a
  one-row list — the exception is *listed*, not smoothed into a formula that
  would fit it.

- **The entry point boots the data, and the clicks are the data's.** `ra2-shell`
  finds the install (`RA2_GAME_DIR`, else `local/game` above the working
  directory or the crate; `RA2.cfz`, `UI/` and `Scripts/` are required), boots
  the shipped `Scripts/Main.py` through `ra2script`, and opens the main menu.
  The boot file is named by the game's own executable (`mainmenu\mainmenu.gib`
  beside the `Main Menu Win` class) and its root *is* `Main Menu Win`; `Main.py`
  itself names no screen — it imports the arenas and brains and registers
  `dispatchEvent`, which is the script layer under the screens, not the screen
  graph. A click is hit-tested against the placed tree and the control's own
  `Notes` decides: a `.gib` in the note is loaded (into the group the note names,
  or as the screen), and the handful of notes that name a purpose instead of a
  file — `link to team HQ`, `click to go to the bot workshop` — are the small
  `nav::LINKS` table whose test re-reads every note and target from the shipped
  files. The routes are input files (`click x y` or `click "Widget Name"`,
  `wait n`, `shot name.png`), so the run is headless and repeatable:
  `cargo run -q -p ra2-shell -- --route botlab --out /tmp/ra2-shell`. Main menu →
  team picker → team HQ → robot inventory → workshop → `chassis button` →
  `structure design button` is the first route; main menu → exhibition setup →
  the arena picker is the second. The picker's rows are the game's arena scripts
  (`ra2ui::arenas`), because the loaded list's own note says *"populate with arena
  names"*. What the route does **not** cross is the match itself: starting a
  match, the team and robot pickers' contents, and the bot lab's list/paint tools
  are the game's C++, and the screens stop there.

## M4 design notes

- **The scripts are the specification, so the scripts are what runs.** Every
  arena and every AI in the game is a subclass of an engine class, and the
  behaviour we want is in *their* bodies, not in ours. `ra2script` therefore boots
  a real CPython, puts the game's own `Arenas/`, `AI/` and `Scripts/` on
  `sys.path`, and runs them as written. Nothing in the repository paraphrases a
  strategy or a hazard.
- **They are Python 2.2.** Rather than rewrite them, a translating import hook
  (`legacy.py`) rewrites the constructs Python 3 will not parse — `print`
  statements, `raise E, msg`, `except E, e`, `<>`, `d.iteritems()`, `it.next()`,
  `file()`, `xrange()` — plus Python 2's *implicit relative imports* (`import
  Hazards` inside package `Arenas` means `Arenas.Hazards`), and reinstates the 2.2
  library functions the scripts import (`string.split`, `types.TupleType`, …).
  Only the game's own directories are translated; the shipped `Scripts/Lib` is the
  Python 2.2 standard library and is deliberately *not* used — the host CPython 3
  one is, behind the same shim. One semantic is not translated: `/` on two ints
  truncates in 2.x; the shipped scripts were checked and none relies on it.
- **What is ours is only the engine side.** `plus.Arena` answers the questions the
  scripts ask the world — `RayTest`, `GetStartPointLocation`, the navigation map
  (`SetMapSize`/`SetMapHeight`/`SetMapWall`/`GetPath`/`FromGrid`), the hazard
  joints (`AddPrismatic`, `GetHinge`) and the SmartZone ids (`RegisterZone`) —
  plus the geometry `AddShadowTriangle`/`SetShadowSource` feed. Camera, audio,
  particle and Gooey calls are accepted and *recorded*, so a run reports exactly
  what it dropped instead of ignoring it.
- **The navigation map is the game's own.** Arenas ship a baked `arena.map`
  beside their model, and `Arenas.SuperArena.LoadMap` reads it; `CreateMap` is the
  fallback for arenas that ship none, and it bakes the grid with `RayTest` against
  the model. The AI paths over that grid (`GetPath` is A* on it), and
  `AddCollisionLine` walls it around each hazard — which is how the scripts keep
  the AI off the spikes.
- **The engine calls back in.** `Arena.Tick` each frame, `Arena.ZoneEvent` when a
  robot enters one of the registered SmartZones, `AddShadowReceivers` once after
  construction — the same sequence the original drives.
- **The AI is the game's own, and it is what makes a fight a fight.** The
  stand-in driver in `ra2sim::duel` only ever charges, so the headline duel
  orbited and pinned itself against a wall. With `AI.SuperAI` + `AI.Tactics` +
  the per-bot brain `Bindings.py` picks by the robot's own `Name:` line, the
  robots close to contact and trade blows: the gap goes negative and both take
  damage. A bot the table does not cover keeps the stand-in.
- **The brains run on the game's cadence, not the physics'.** `AI.SuperAI.Tick`
  refreshes its sensors when its own counter reaches 8 and its comment says
  "once per second", and `AimToHeading` multiplies a turning rate by
  `self.tickInterval` — so a brain tick is an eighth of a second and the physics
  steps many times between them, with the controls held. (`Hazards.Flame` and
  `Hazards.Electricity` count the same way and put the arena's own `Tick` at a
  quarter second.) Getting this wrong changes the AI's behaviour, because its
  `countdownToEvaluation` counts ticks.
- **Two conventions had to be read off the scripts, not guessed.** The scripts
  build a direction from an angle with `vector3(a) = (sin a, 0, cos a)`, so
  `GetHeading` is `atan2(x, z)` of the facing and `GetHeadingTo` is the *error*
  (`AimToHeading` receives it directly). And `Turn(t)` is positive for a
  *positive* heading error while `Robot::drive` applies `yaw = -steer * rate`, so
  the drive negates — the stand-in does the same, and getting it wrong sends the
  robots away from each other.
- **The engine answers, the scripts act.** The simulator publishes a
  [`ra2script::BotSnapshot`] per robot each step (position, facing, speed, yaw
  rate, mass, per-component class and health, component spheres for `rayTest`)
  and drains a queue of [`ra2script::Request`]s afterwards (`plus.damage`,
  `plus.force`, `plus.zap`). Keeping it data rather than a callback trait means
  neither side holds the other's lock while Python runs.
- **Verification.** `ra2script`'s tests assert that all fourteen stock arenas
  register, that `Arenas/box.py`'s configuration arrives intact (3 cameras, 10
  shadow triangles, the 30×30 map with its 207 walls and 24 hazard cells, and the
  saw plus two spike zones with their ids), that `RayTest` finds the floor under a
  start point and `GetPath` routes across the arena, and that the translation
  rewrites the constructs the survey found and nothing else. `scriptdump`
  (`cargo run -p ra2script --example scriptdump -- local/game`) prints what every
  arena configures.


## M2 design notes
- **Swappable physics.** `ra2sim::backend` defines neutral `BodyDesc`/`ShapeDesc`
  /`BodyState` and a `PhysicsBackend` trait (create/remove body, impulses,
  velocities, step, ray cast). `ra2sim::jolt` is the only module that knows about
  Jolt.
- **Bodies from game data.** The arena becomes a static triangle-mesh collider
  built from its rendered geometry; the robot becomes a convex hull of its mesh
  vertices; the arena script's `AddXtra` obstacles become *movable* props from
  their embedded `RBCollection` (crates 40 kg, barrels 1500 kg) using the game's
  own mass/friction/elasticity.
- **Two conventions that bit us** (recorded so they are not re-learned):
  1. Jolt's mesh collider is **one-sided**: the counter-clockwise side is solid.
     RA2 geometry is clockwise as seen from outside, so its winding must be
     reversed for Jolt.
  2. The collider is a single sliding hull, so ordinary friction pins it; the
     controller models tyre grip explicitly and the hull keeps low ground
     friction.
- **Controller.** Arcade `throttle`/`steer` → target forward speed + yaw rate,
  with lateral grip. Per-wheel jointed bodies + motor torque is M3 work.
- **Verification.** `ra2-sim` runs the timeline headlessly and writes
  `frame_NNN.png` plus a `strip.png` montage, so driving and collisions can be
  checked without a display.

## M3 design notes

- **A robot is a component tree.** `ra2sim::rig` rebuilds the `.bot`'s parts as
  nodes with a role (chassis / wheel / motor / weapon / piston / structure), a
  mount transform, geometry, and stats read from the component definitions:
  mass from the mesh's `RBCollection`, `hitpoints`/`damageable`/`concussion`/
  `piercing`/`power` from the `.txt`. `ra2sim::robot` binds that tree to one
  physics body and tracks per-part health.
- **Motors turn their subtree.** A motor rotates its attached parts about the
  component-local +Z axis (RA2's axle direction, confirmed by the saw blade and
  wheel meshes: discs lie in XY). `RobotRig::worlds` re-walks the tree with the
  current rotor angles, so a spinner's disc, arms and weapons all sweep together
  and wheels turn.
- **Pistons slide their subtree.** A `Role::Piston` component splits its child
  group onto a prismatic joint along the axis its definition declares
  (`prismatic = <housing>, <rod>, ax, ay, az, neutral, lower, upper`), with that
  travel as the joint's limits. The mesh ships the housing and the rod as two
  rigid bodies (`burstpiston1.gmf`: `body` 14 kg, `shaft` 8 kg) and the children
  mount on `Host = shaft`, so the rod is the moving half. A parked piston holds
  its rod at the definition's force rather than being welded (F8).
- **Damage is contact-driven, per body.** Jolt's `WereBodiesInContact` tells us
  *whether* two bodies are engaged, but not which parts touched; guessing from
  bounding volumes was unreliable (a robot's parts are spread across its body, so
  its own hull surface often touches where no single part's box does). A robot is
  several hinged bodies, so `ra2sim::combat` asks which bodies of each side met
  and attributes through those: the attacker contributes its best weapon
  (`concussion + piercing`) among the bodies that are touching, and the damage
  lands on the opponent's nearest *destructible* part on the other side of the
  contact. Indestructible fittings (`damageable = false`) are skipped so they
  cannot soak a fight forever.
- **A blow is an event, a grind is a rate.** The relative speed the robots
  *arrive* with (measured before the step, since the collision absorbs it) is
  dealt once when contact begins — that is the game's `amount` per hit — while a
  spinning weapon's rotor tip speed is applied every step, because it works the
  same joint repeatedly. Motionless contact therefore does nothing, two rammers
  trade blows, and a stationary spinner still chews. `CombatConfig.damage_scale`
  converts impact speed to points and is calibrated to the game's own numbers:
  its hazards apply a flat 25, and its AI scripts treat `amount > 10` as a good
  hit.
- **The chassis is dressed from its face groups.** Each side of a chassis gets
  its skin from its own face group in the `.bot` — a polygon on the 256×256 skin
  and the skin coordinate of each of the side's vertices, listed in ascending
  index order. The model's own per-point UV table cannot express a seam (a point
  shared by two sides carries one coordinate), which is what made a decal come
  out rotated or mirrored. See F10.
- **The chassis buckles.** A chassis carries a crush model in the `.bot`
  (buckle points and the top plate's corners). As it takes damage the points the
  file marks as movable shift **down and in** — the floor plate stays, the sides
  cave — and both the drawn chassis and its collider follow (the chassis body's
  shape is re-baked). Before this a chassis only ever died whole. See F9.
- **Parts detach.** When a part's hitpoints reach zero the whole subtree detaches:
  each part becomes an independent rigid body at its current world transform,
  gets an outward impulse, and the robot's collider is rebaked from the parts it
  still has. Destroying the chassis ends the fight (the rest scatters).
- **AI.** Two behaviours share one driver: `cycle` (the default) grinds for ~1.6 s
  then reverses for ~0.8 s so the fight keeps moving; `press` (`RA2_AI=press`)
  never disengages. The trigger is the real physics contact, not an estimate.
- **Verification.** `ra2-duel` writes a frame strip plus a log of breakage
  (`t=… Name: part knocked off`, `… chassis destroyed`). `RA2_DUEL_DRIVE=ram`
  drives every robot straight ahead instead of running the AI, which turns a bout
  into a deterministic head-on collision — the only reliable way to observe the
  damage model, since a driver's own pursuit frequently never closes to contact
  (F7).

### The assembled robot (rigid bodies per component group)

`ra2sim::rig::assemble` groups a robot's components into **one rigid body per
rigidly-mounted group**, with a hinge wherever a motor sits between parent and
child — the structure the `.gmf` motor constraints describe. `ra2sim::robot`
builds each body as a compound shape of its members' geometry, drives the wheel
hinges at the speed and torque their own component definitions declare, spins the
weapon hinges at their rated speed, and reshapes a body in place
(`PhysicsBackend::set_body_shape`) when a part is shot off. It is **the only robot
path** (`ra2-sim` and `ra2-duel` both drive it); the arcade single-hull controller
it once replaced is gone. What the honest drive still cannot do — a quarter of the
stock bots will not get traction — is geometry and collision rather than drive
parameters: F11/F5 in `docs/06-deferred-fidelity.md`.

### Physics gotchas worth remembering (all hit during M3)

1. **Sleeping bodies ignore velocity commands.** A body that settles is parked
   by Jolt and later `set_linear_velocity` calls do nothing, so a robot would
   freeze mid-fight. The backend now activates before setting velocity, and
   game-driven bodies opt out of sleeping entirely.
2. **Jolt mesh colliders are one-sided.** Arena proxies in the wild are not wound
   consistently, so the collider is emitted double-sided (both windings). A
   single-sided proxy had robots being ejected from the box arena at 40 m/s.
3. **Arenas ship both display and proxy geometry.** The proxies are the intended
   physics, but some arenas ship none (the practice arena) and some ship a proxy
   that misses the floor. `load_arena` prefers the proxy, verifies with a ray
   cast that something is under the arena centre, and otherwise falls back to the
   visible geometry; `RA2_COLLISION=proxy|visible` forces either.

## Key design decisions to settle

1. **Scripting strategy** — embed CPython (run original 2.2 scripts) vs. native
   reimplementation of the API. (See question to user.)
2. **Physics fidelity** — matching Havok "feel" including the famous explosions.
3. **Asset conversion** — convert-on-load vs. pre-baked GLB/PNG cache.
4. **Coordinate system** — GMI is left-handed (+X left, +Y away, +Z up); decide
   the engine convention and convert at load.

## Fidelity oracle

Keep the original running under Wine (already set up) to A/B compare rendering,
movement, and physics behaviour as we implement.
