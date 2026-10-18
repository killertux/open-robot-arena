# 27 - Shell layout, window stack and screen graph (recovered)

## 1. Native layout (fx/shell-layout)

The original lays every screen out natively at the display resolution; there is
no 1280x1005 design space. `.gib` records give `Position`/`Size` as
`(value, mode)` pairs per axis: mode 1 = fraction of the parent's extent
(`0.5 1 0.1 1` = 50% across, 10% down), mode 0 = pixels (`160 0 0.9 1` = 160 px
wide, 90% high). Each is resolved against the **parent's frame**; the root record is
`Size 1 1 1 1` (the whole screen). `_Size 1280 1005` in a few files is only the
editor's canvas. `ra2-app` now boots `Shell` at the real target size
(`Shell::boot(game, [w, h])`) and calls `Shell::set_size` when a window
resizes, so the window is 1:1 with the layout (no letterbox) for the shell
screens. Match, HQ and build modes still use `SHELL_SIZE` (their own passes).

`SplitFrame`: `Master N` names the slot that `MasterSize` sizes; slot 0 is always
the first (top/left) pane, slot 1 the second. `exhib_setup.gib`'s main split is
`Horizontal false, Master 1, MasterSize 0.13`: slot 1 is the 13% bar at the
*bottom* (the bottom nav), slot 0 the remaining 87%. The reader had put the
master pane first, which dropped the title and tabs by a whole pane.

Text: a `\n` written in a record's `_Text` is a line break (`credits.gib`).

## 2. Window stack

* `FUN_005e5620` (window manager, `this+0x68` = modal, `+0x6c` = result): hides
  the previous modal, stores the window as the modal and pumps messages until
  the modal pointer is cleared. It is the *modal* show.
* `FUN_005e5690(name)` finds a window by name (`FUN_005e4bd0`) and calls
  `FUN_005e5620`; the Credits handler uses it.
* `FUN_005e5ba0(name, ?, fade, flag)` is the screen *switch*: with a fade flag it
  builds fade-out/fade-in overlays (0.2 s alphas `0x3e4ccccd`), otherwise
  `FUN_005e5810 -> FUN_005e5830` swaps the active window.

So Options (`FUN_00486c50`, `options win`, `common\options.gib`, class
`OptionsWindow`, 0x19c bytes) and Credits (`FUN_00487260`, `credits win`,
`common\credits.gib`, 0x158 bytes) are modal windows over the main menu;
windows are created once and cached by name. The shell now keeps
`Shell.windows` (a stack over the current screen): the top window owns input
(clicks outside its controls are swallowed), the screen under keeps drawing, music
(`audio::on_screen`) ignores window screens, and the window's close/OK/cancel
buttons pop it.

## 3. Main-menu constructor `FUN_00486960`

Binds by widget name (`FUN_004294e0` lookup + `FUN_004873a0` bind):

| widget | handler | does |
|---|---|---|
| `Quit button` | 0x486c30 | quit |
| `Options button` | 0x486c50 | modal `common/options.gib` |
| `credits button` | 0x487260 | modal `common/credits.gib` |
| `exhibition button` | 0x486d70 | "Loading...", `ExhibitionSetupWindow(exhibition\exhib_setup.gib)`, switch (0x5e5ba0) |
| `Team HQ button` | 0x486f10 | "Loading...", `SelectTeamWindow(teamhq\select_team.gib)`, switch |
| `Multiplayer button` | 0x4870b0 | `multiplayer\multiplayer.gib`, plain switch (0x5e5810) |

"Loading..." (`FUN_004e6bd0`, string 0x6f2ad0) is shown only on the *first* visit,
while the heavy window is constructed; later visits find the cached window by name.

## 4. Where the table lives

`engine/crates/ra2-shell/src/bindings.rs` holds the recovered (screen, widget
name) -> action table; `Shell` consults it before a `Notes` string. Screens
whose constructors were not recovered in this pass (select team, team HQ and
its tabs, exhibition setup content, bot lab, results) still navigate by the
`.gib` `Notes` (nav.rs); only the main menu and the options/credits windows are
exe-derived so far. The select-team `continue button`, `new team button`,
`delete team button` names are present in the record and bound by
`SelectTeamWindow` (0x4eb540, init 0x4ec2c0); their handlers are not decoded.
