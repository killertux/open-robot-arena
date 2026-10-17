# 20 — Client audio and the options screen (M7.5/M7.6 wiring)

This is the running-client half of `docs/13-audio-and-writers.md`. The libraries
are unchanged in what they claim; this page records where the app calls them,
which controls edit which key, and what the evidence is.

## 1. Where it lives

| File | What it owns |
|---|---|
| `ra2-app/src/audio.rs` | `AudioSystem`: the music-per-screen policy, the UI click, the match drain, the `--audio-report` lines |
| `ra2-app/src/options.rs` | The options screen's staged `ra2.cfg`, control→key map, mirroring the values back into the shipped records, `save`/`cancel` |
| `ra2-app/src/window.rs` | One `AudioSystem` per window; the screen switch in `render`, the match drain in `about_to_wait`, the click and options handling |
| `ra2-app/src/headless.rs` | The same policy over `Audio::without_device`, one step at a time |
| `ra2-app/src/match_mode.rs` | `Session::audio_listener` (camera eye + aim) and the offscreen match's drain |
| `ra2script/src/lib.rs` | `SoundQueue::apply_requests` (a batch already taken by `take_sound_requests`) and `arena_intro` (`DoIntro`) |

## 2. Music per screen

`music_for_screen(Shell::screen_id())` is consulted on every shell transition.
The resolved file is compared to the current one, so a redraw never restarts a
track. Two screens in one group (the team picker and the HQ) share a track, and
an unmapped screen (`exhibition/`, `help/`, `common/options.gib`, …) stops the
music, as `music_for_screen` documents. The three tracks are the only starts:

| Screen | Track |
|---|---|
| `mainmenu/…` | `Music/Main UI/Enter the Battlefield.mp3` |
| `teamhq/…` | `Music/Team HQ/Robot Tribal Dance.mp3` |
| `botlab/…` | `Music/Workshop/Deep inside the Bot Lab.mp3` |

The `--hq` session reports `teamhq/teamhq.gib` while an HQ screen is up and no
track while its event match is on screen. `--build` reports
`botlab/botlabwin.gib`. A plain match has no UI track; its music is the arena's
own script sound.

## 3. Clicks

`Step.hit != "nothing"` is the activation rule: the shell's hit test only stops
on a control, so a miss (the bars, empty chrome) is silent, while a button, tab,
checkbox or option-group member is one click. The sound is
`Sounds/button_click.wav` at per-call gain `1.0`; `Audio::play_2d` multiplies it
by the SFX category (`SoundVolume`, `SoundMuted`), so muting works.

The shipped executable contains the string `Sounds\button_click.wav`. No call
site was traced, and the scripts/UI notes do not name it. **The mapping "every
activating shell click plays it" is this client's, not established game
behaviour.**

## 4. Match sounds

Every app tick (50 ms) and every offscreen match step:

1. `ra2script::take_sound_requests()` removes the pending batch but keeps the
   handle table.
2. `SoundQueue::apply_requests(batch, game, listener, audio)` resolves each path
   against the install and starts/stops the handle.
3. The listener is `duel.camera_pose(camera)`: ear at the eye, forward
   `normalize(aim - eye)`, up world `+Y`. A positional emitter is heard from
   where the frame is drawn; the report prints the listener separately because
   `PlayEntry` records the emitter, not the ears.

The arena's `DoIntro(true)` is called at match setup when `ArenaIntros=1` (the
shipped default). `Arenas.SuperArena.Tick` advances the intro generator, so
`intro_music`, the announcers and the crowd creation become ordinary requests.
`plus.fadeInToLoop`/`fadeOutLoop`/`setVolume` are still recorded, not performed,
so the crowd loop is created but its fade does not reach the mixer.

## 5. Options screen

The main menu's `Options button` note (`click to bring up options dialog`) is a
purpose-named link to `common/options.gib`; the OK/Cancel notes are links back
to the boot screen. The shell therefore renders the shipped window and its two
fragments, and `ra2-app` handles the settings behind them.

| Control | Key | Mapping |
|---|---|---|
| `effects volume slider` | `SoundVolume` | left `-10000`, right `0` (**our scale**; the screen states none) |
| `music volume slider` | `MusicVolume` | same |
| `effects checkbox` | `SoundMuted` | checked = on |
| `music checkbox` | `MusicMuted` | checked = on |
| `intro off/on` | `ArenaIntros` | |
| `fullscreen checkbox` | `FullScreen` | initial window only; a running window does not switch mode |
| `Camera dropdown` | `DefaultCamera` | cycles `watch`→`static`→`orbit` |
| `effects off/on` | `GraphicEffects` | |
| `damage off/on` | `GraphicMessages` | |
| `2/4 player` | `EventNumBots` | |
| `detail low/med/high` | `GraphicDetail` | `0/1/2` is **our numbering** |
| `resolution dropdown` | — | **read-only**; the screen says "fill with screen resolution options" but the data/file has no list and the executable's list was not recovered. The current `FSDisplayWidth`×`FSDisplayHeight` is shown. |

`BitsPerPixel` and `UpdateInterval` are not on the shipped screen; no control
was invented. `FSDisplayWidth`/`FSDisplayHeight` feed the initial window size;
the resolution dropdown only displays them and does not edit them.

Writing uses `Cfg::write`, which changes only the edited value lines. A control
change is staged, written immediately and applied to the live mixer (volume
preview); `ok button` writes again as a check. `cancel button` writes back the
exact bytes read when the window opened. Opening the window reloads the file;
boot never writes.

## 6. Evidence and limits

Commands and printed evidence are in the task/commit record; the tests are:

- `ra2-app::audio`: track switch/no-restart/stop, one click per call, SFX gain
  and mute, the no-device report.
- `ra2-app::headless`: the named route main menu → team HQ → robots → bot lab
  prints three music starts and four clicks, one per activation.
- `ra2-app::match_mode`: a match prints the arena intro music and an announcer
  as `Sound3d` entries with the listener off the origin.
- `ra2-app::options`: control→key mapping, slider scale, sync into the shipped
  fragments, cancel restores bytes, and the shipped `ra2.cfg` keeps every other
  line/CRLF/final newline when a volume and the camera change.
- `ra2-shell`: the three options links are notes the shipped screens carry.

**Unproven:** audible playback through a physical output device (this
environment has no output that was exercised); the click-sound provenance
above; the slider scale and `GraphicDetail` numbering; the intro crowd fade.
