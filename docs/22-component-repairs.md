# 22 — Component repairs: `Team+0x12c`, the bench and the row model

This is the component-level recon behind `ra2sim::career::Repair` and
`ra2ui::repairs`. Every address is in `Robot Arena 2.exe` (SHA-256
`bc0f6833ece8c47e8625e5b12940d2509540cb393566577504b50b2318351a4f`, PE32,
x86). The screen is `UI/teamhq/repairs.gib`.

## 1. The damage block: two maps at `Team+0x12c`

`Team+0x12c` is not a scalar flag and not a list of robots. It is a pair of
`std::map<int, …>` instances laid out at `+0x12c` and `+0x13c`:

| field | type | value | evidence |
|---|---|---|---|
| `Team+0x12c` | `map<int, {current hp, max hp}>` | one node per damaged component: key at node `+0`, current hitpoints at `+4`, pristine maximum at `+8` | save writer `0x4fb377`; load `0x4faee9`; bench reader `0x4d64c2` and `0x4d671b` |
| `Team+0x13c` | `map<int, {fractured flag}>` | key at `+0`, flag at `+4`; `1` draws the `damage.tga` icon | bench reader `0x4d64c2`/`0x4d6521`; writer `0x4df8da` |

The key is an integer component id. The bench resolves it through the robot's
live component table (`FUN_004dab00(0, &id)` at `0x4d667d`/`0x4d670c` calls
`FUN_00445680` and `FUN_00440f40`), and the name it prints is
`FUN_004dd780(id)`. The exact derivation of the id (the `.bot` component
ordinal versus a runtime handle) is **not proven**; the port uses the design's
0-based component index and names it as such.

### Save and load

`Team::save` (`FUN_004fb010`) writes the block only when `Robot In Event` is
not `-1`:

```c
0x4fb36b  MOV ECX, EDI            ; Team*
0x4fb36d  CALL 0x004d63b0         ; return Team+0xc0 (Robot In Event)
0x4fb372  CMP EAX, -1
0x4fb375  JZ  0x004fb387
0x4fb377  LEA EDX, [EDI+0x12c]    ; the damage block
0x4fb37d  PUSH EDX
0x4fb37e  PUSH ESI
0x4fb37f  CALL 0x004dfaa0
```

`FUN_004dfaa0` is a thin wrapper over `FUN_004df7f0`, which writes **two**
lists. The first pass iterates the map at the block base (`ESI = Team+0x12c`):

```c
0x4df84e  CALL 0x0047d8c0        ; node
0x4df867  MOV EAX, [EAX+8]       ; max hp
0x4df879  MOV ECX, [EAX+4]       ; current hp
0x4df87c  MOV EDX, [EBX]         ; component id
...                              ; write id " ", current " ", max, newline
```

The second pass starts at `ESI+0x10` (`0x4df8da`, i.e. `Team+0x13c`) and writes
`<id> <flag>` per node (`0x4df936` reads key, `0x4df946` reads `+4`). The
separator string at `0x6f1e48` is a single space. The reader
`FUN_004dfac0` → `FUN_004df9a0` reads a count and three values per node in the
first list, then a count and two values per node in the second
(`0x4df9a0`–`0x4dfa9f`), so the two-list shape is established in both
directions.

`FUN_004faa60` (called from `FUN_004faaa0`, the event clear) destroys and
reinitialises both maps. The Team constructor `FUN_004f9e90` and destructor
`FUN_004fa080` construct/destroy them (`0x4f9fe8`, `0x4fa0b6`).

### How the bench reads a row

`RepairsWindow::open` is `FUN_004d62a0`. It stores the current team at
`window+0x1c4` (`0x4d62d2`), reads `Robot In Event`, and builds the entered
robot with `FUN_004fa890(team, event, 1)`. That function calls
`FUN_004d6870` (`0x4d6870`, literally `LEA EAX,[ECX+0x12c]`) and copies the
saved block into the robot with `FUN_004faa30`
(`FUN_004fb690`/`FUN_004fb6f0`, the two map copy-assignments).

`FUN_004d6480` draws the list:

* `0x4d64bc` loads `[window+0x1c4]` and `0x4d64c2` calls `FUN_004d6870`, so
  the two lists are the team's.
* The first loop walks `Team+0x13c` (`LEA EBX,[EDI+0x10]`, `0x4d64da`). For
  each node it takes the key (`0x4d6512`), checks `[node+4] == 1`
  (`0x4d6521`), and when true loads `UI\teamhq\damage.tga` (string at
  `0x6fd5b8`) and draws it at the component's row
  (`0x4d6552`–`0x4d65fd`).
* The second loop walks `Team+0x12c` (`0x4d6636`). For each node the key is
  looked up in the live component table (`FUN_004dab00`, `0x4d667d`). If the
  component is gone, only its name is drawn (`0x4d6689`–`0x4d66fa`). If it is
  still on the robot, the health bar is

  ```c
  f = (float)[node+8] - (float)[node+4];   // max - current
  f = f / (float)[node+8];                 // / max
  if (f > 1.0) f = 1.0;
  f = f * 20.0;                            // the bar's 20-unit width
  FUN_0058ada0(f, 1);
  ```

  (`0x4d671b`–`0x4d6784`). The text is the component name from
  `FUN_004dd780(id)`.

So one row is one damaged component; the row shows the component's name/class, the
`(max-current)/max` bar and, for a fractured component, `damage.tga`.

### Completion is per component

`FUN_004d6e10` is the window's tick:

```c
i = FUN_00424c40();                  // [list+0xfc], selected row
FUN_005ff7f0(i, 0); FUN_00440f40(..); // the selected row's data
progress = FUN_004d7280(&row);        // window+0x1a0
if (FUN_0058b470(progress)) {         // compared with window+0x160
    FUN_004d6870();                   // Team+0x12c
    *FUN_004d75c0(&row) = 0;          // Team+0x13c: clear the fractured flag
    *FUN_004d7570(&row) = ...         // Team+0x12c: current = max
    FUN_0058b3d0(progress);           // subtract the served time
    FUN_004d7010(i);                  // remove row i
}
```

`FUN_004d7010` calls `FUN_005ff530(i)`, decrements a count at `[this+0x168]`
(`0x4d701e`–`0x4d7027`), and adjusts the selection. `FUN_005ff530` erases the
`i`-th 0x18-byte element from the array at `[this+4]` and decrements
`[this+0xcc]`; it is a model/list erase, **not** the component restore (the
restore is the two map writes just above). It is the selected component that
is restored, not the robot: repairs are **per component**.

## 2. The clock: 30 minutes

`FUN_004d62a0` sets the clock at `window+0x160`:

```c
0x4d635f  PUSH 1
0x4d6361  PUSH 0x1e
0x4d6363  LEA ECX, [ESI+0x160]
0x4d6369  CALL 0x0058b150
```

`FUN_0058b150(0x1e, 1)` dispatches to `FUN_0058b2f0(30)`, the **minutes**
setter: it stores 30 in the `giTime` middle field (`+8`) and 0 in `+4`/`+0xc`,
i.e. `00:30:00`. The `giTime` renderer (`FUN_0058aed0`) pads and separates three
base-60 fields (`+4`, `+8`, `+0xc`), and `FUN_0058b3d0` borrows base-60 from
the seconds field to the minutes and hours fields. The screen's own instruction
text says *"You have 30 minutes to make repairs to your robot between
rounds."* The unit is therefore **minutes**, and the model stores it as 1800
seconds (`CareerPolicy::repair_seconds`). The progress bar is the same
`giTime` type; the tick subtracts the served time and completes one row when
it is full.

## 3. What remains unproven

1. **The writer of the block during a match.** `Team+0x12c`/`+0x13c` are
   referenced by the constructor/destructor (`FUN_004f9e90`/`FUN_004fa080`),
   the clear (`FUN_004faa60`), save/load (`FUN_004fb010`/`FUN_004fabf0`) and
   the bench (`FUN_004d62a0`/`FUN_004d6480`/`FUN_004d6e10`). The insert that
   records a hit was not located; the save path proves the stored shape, not
   which match routine fills it.
2. **The key's exact identity.** The screen resolves it through the robot's
   component table; whether it is the `.bot` ordinal or another runtime id is
   not proven. The port uses the sim's component index and says so in the
   sidecar and the screen.
3. **The per-row duration.** The window has one clock and the tick clears one
   selected row when it fills; whether the original gives every component the
   same 30 minutes or scales it by damage is not established. The model gives
   each row the policy's time and completes rows independently.
4. **Non-fracture damage and the icon.** The `+0x13c` flag is the icon's sole
   condition in `FUN_004d6480`. The port sets it when a single blow exceeds
   the component's `fracture` threshold (`FUN_0045f2b0`'s second break rule),
   and also marks a detached component fractured for the icon; the original's
   exact flag writer is part of (1).
