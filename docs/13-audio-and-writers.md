# 13 — Audio, settings, and the writers (M7.5, M7.6, M9.1)

Three workstreams landed together. Each is a library with tests; the
integration into the running client is named at the end and is not done yet.

## 1. `ra2.cfg` (W3)

`ra2fmt::cfg` reads and writes the game's own settings file, not a new one.

- The shipped `ra2.cfg` is 649 bytes, 32 lines, CRLF including the final line.
- `parse → to_text` is **byte-identical** to the shipped file. Physical line
  order, section order, key spelling and order, duplicate keys, unknown keys
  and unrecognised lines are all preserved; only the value after `=` is
  rewritten.
- Typed accessors exist for the keys the client reads (`FullScreen`,
  `FSDisplayWidth`, `FSDisplayHeight`, `BitsPerPixel`, `DefaultCamera`,
  `SoundVolume`, `MusicVolume`, `SoundMuted`, `MusicMuted`, `EventNumBots`,
  `ArenaIntros`, `UpdateInterval`, `GraphicDetail`, `GraphicEffects`,
  `GraphicMessages`), with the shipped values as documented defaults.
- Tests: byte round-trip, a three-key modification that changes only those
  lines, missing-key defaults, duplicate keys, and LF/no-final-newline
  preservation. The game-file tests skip with a message when `local/game` is
  absent (CI).

Not done: the **options screen** does not yet drive it, and the client does not
yet read display size/camera/volume from it at startup.

## 2. `ra2-audio` (W2)

A new crate, `rodio 0.23` with the `playback`, `mp3` and `wav` features.

Decoded formats, measured by the `decode` example:

| Track | Format | Rate | Channels | Duration |
|---|---|---|---:|---:|
| `Music/Main UI/Enter the Battlefield.mp3` | MP3 | 44.1 kHz | 2 | 266.815 s |
| `Music/Team HQ/Robot Tribal Dance.mp3` | MP3 | 44.1 kHz | 2 | 334.994 s |
| `Music/Workshop/Deep inside the Bot Lab.mp3` | MP3 | 44.1 kHz | 2 | 324.075 s |

The MP3s are raw MPEG frames with no Xing/VBRI header, so the duration is
counted by decoding, not read from a header. Sample WAVs decode as 8/16-bit
mono 22.05 kHz or stereo 44.1 kHz, all non-zero length.

**Volume.** `gain = 10^(value / 100 / 20)` — DirectSound hundredths of a
decibel. `-600` → `0.501187` (−6 dB), `-10000` → silence, `≥ 0` → unity. The
mute flags override to zero. The exe's `SoundVolume=%d` / `MusicVolume=%d`
strings are consistent with this reading; nothing found contradicts it.

**Music by screen** (from the exe's three `Music\...` directory names):

| Screen group | Track |
|---|---|
| `mainmenu/…` | `Music/Main UI/Enter the Battlefield.mp3` |
| `teamhq/…` | `Music/Team HQ/Robot Tribal Dance.mp3` |
| `botlab/…` | `Music/Workshop/Deep inside the Bot Lab.mp3` |

`exhibition/`, `multiplayer/`, `help/` and `armor/` return `None` rather than
guessing a track.

**No device is not an error.** `Audio::new()` falls back to `device: None`; play
calls are successful no-ops recorded in a `PlayLog` (path, kind, effective
gain, position). That is what the tests assert. 3D is `rodio`'s `Spatial` with
ears at ±7.5 cm, each ear `pan × min(1, 1/d²)`; no Doppler, occlusion or
reverb.

**Untested:** audible playback through a physical device. This environment has
no output that was exercised.

## 3. `.bot` writer (W1)

`ra2fmt::bot::Bot::write` targets version 1.12 and is measured against the
shipped corpus: **57 files** (6 `Robot Designs/`, 45 `AI/Team*/Bot*.bot`, 6
`Teams/Team0/Bot*.bot`; versions 9×1.07, 2×1.08, 4×1.11, 42×1.12).

- Semantic round-trip (`parse(write(parse(f))) == parse(f)`): **57/57**.
- Byte-identical (`write(parse(f)) == f`): **57/57**.

Byte equality is reached by modelling the format, not by storing the source
text: the game's `%g` number form (`-2.38419e-007`), per-file LF/CRLF, the
trailing space after controller-binding and top-corner rows, the flat
file-ordered face entries, the TGA blobs, and version-gated sections.

The parser previously dropped data; it now carries it: the 1.12 header texture
flag, the snapshot TGA, line endings, buckle model/heading, controllers and
bindings, `Component::smart_zone`, `Chassis.face_count` and `armour_material`,
the flat face list (60 rows naming past-the-end groups are preserved instead of
dropped), and mesh normals.

Known limits, named: a true unknown chassis sub-section is rejected by the
reader (no shipped file sets the flag); multi-model 3D blocks are merged into
one by the reader (all 57 have one); inconsistent 3D count headers are
recomputed; negative mesh indices are stored unsigned (none in the corpus);
version-gated data is omitted by design for 1.07/1.08.

## 4. Team records (W1)

`ra2fmt::team` now carries the whole record, not just the identity lines: the
robots, event robot, the 14 state lines (typed), the 15-int season line and
money, the closing lines and the line ending.

- `Teams/Teams.txt` (16 records) and `AI/Teams.txt` (15 records) both
  round-trip **byte-for-byte**.
- Setting the 15 season ints and money, writing and re-reading works.
- 12 of the 14 state lines still have no recovered meaning; they are preserved
  as typed values but unnamed. There is no high-level "add team" API yet.

## 5. What is not wired yet

- `ra2-app` does not play music or sounds. The script layer's sounds are now
  real, though: `plus.createSound` keeps a `SoundId` handle table and
  `playSound`/`loopSound`/`stopSound`/`removeSound` append ordered
  `SoundRequest`s to `ra2script::SoundQueue`. `SoundQueue::apply(&mut Audio)`
  resolves each script path against the game directory (case-insensitively,
  backslashes or slashes) and drives `ra2-audio`; no device is required and the
  tests exercise it with `Audio::without_device()`. The app still has to call
  it once per frame.
- The options screen does not edit `ra2.cfg`.
- The workshop does not yet call the `.bot` writer; that is M9's UI work.
- Nothing in this batch is exercised through a real audio device.
- `plus.fadeInToLoop`, `plus.fadeOutLoop` and `plus.setVolume` are still
  recorded rather than performed, so the intro's crowd fades do not reach the
  mixer.

## 6. plus sound semantics (measured from the scripts)

The five calls across `Arenas/`, `AI/` and `Scripts/` are 91 `createSound`,
56 `playSound`, 18 `loopSound`, 16 `removeSound` and 6 `stopSound`.

- `createSound(path, loop, position)` stores the **raw** path and position and
  returns a handle starting at 1. `loop` is read truthily, because the scripts
  pass both `True` and `1`. `loop=False` still creates: the arenas create
  intro music with `False` and loop it later.
- `playSound(handle)` queues a play with the sound's current loop flag; an
  unknown or removed handle is a `ValueError` naming the handle.
- `loopSound(handle)` sets the handle's loop flag and queues `Loop`; the
  scripts use it to start the intro music and the trapdoor/compressor loops.
- `stopSound(handle)` queues a stop and keeps the handle; unknown is a warning.
- `removeSound(handle)` frees the handle and queues a stop. The arenas call it
  from `__del__` and again from `DoIntro`, so a second removal is tolerated
  with a named warning in `State::log`, not an exception.
- `playOnce(handle)` queues a non-looping play; the end-of-match announcer is
  its only shipped caller.
- Paths are resolved at drain time, not at `createSound`; a missing file is
  reported in `apply`'s problem list and skipped.
