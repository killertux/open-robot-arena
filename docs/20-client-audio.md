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
`plus.setVolume`/`fadeInToLoop`/`fadeOutLoop` are performed (see §8).

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
environment has no output that was exercised); the slider scale and
`GraphicDetail` numbering.

## 7. What the original plays (recovered, M-audio pass)

Everything below is from `Robot Arena 2.exe` (Ghidra session, addresses are
file VAs) and the recordings in `local/oracle/original/audio/`.

**Engine.** Music is a DirectMusic segment (`giSoundSegment::Create`, "Unable to
Load file"/"Set Midi Params" strings; the Wine log shows `dmime`/`dmsynth`, two
audio paths created by `performance_CreateStandardAudioPath`: type 8 dynamic
stereo for the music, type 6 dynamic 3D for effects). Effects are `SoundWizard`
(`SWHandle`) over `DSOUND.dll`. The Python binding is registered at `0x4b7750`:
`createSound`, `playSound`, `playOnce`, `loopSound`, `stopSound`, `removeSound`,
`stopAllSounds`, `setMinDistance`, `setMaxDistance`, `setVolume`, `fadeIn`,
`fadeInToLoop`, `fadeOut`, `fadeOutLoop`. The scripts use `setVolume` (13),
`fadeInToLoop` (12) and `fadeOutLoop` (24); `setMin/MaxDistance`, `fadeIn`,
`fadeOut` and `stopAllSounds` are never called by the shipped scripts.

**Fixed-name sounds (string -> user).**

| File | Referenced at | Use |
|---|---|---|
| `button_click.wav` | `0x5ec275` | button class |
| `match_countdown.wav`, `chat.wav`, `critical_hit.wav`, `immobile_countdown.wav`, `frc_generic1/2/4/5.wav` | `0x4158bc`, `0x415849`, `0x4157d6`, `0x415763`, `0x4156f0..` | nine `SoundWizard` objects the match object builds in `FUN_00415500` |
| `match_countdown_high.wav` | `0x6f36fc` (data) | the countdown bell (our mapping) |
| `cmp_removal.wav` | `0x42f2be` | a component removed in the workshop |
| `snapshot.wav` | `0x43b8f2`, `0x5d7a67` | workshop snapshot |
| `point_plotted.wav` | `0x478fdf`, `0x47905d` | a graph point |
| `eject.wav` | `0x47d3f2`, `0x4d0791` | eject (network lobby) |
| `bracket_hit.wav`, `cannon_ball.wav` | `0x503ed0`, `0x6ff568` | component tables (also in `damagesounds`) |
| `player_joined.wav`, `spectator_joined.wav` | strings `0x6f4e9c`, `0x6f4760` (the latter used at `0x492f11`) | network lobby only |
| `crowd/*.wav` (24 files incl. `*_to_*` transitions) | `0x6fd1d4..0x6fd484` | a crowd-mood table (bored ... ecstatic); the shipped arena scripts only use `LoudCheer_Loop.wav` |

**Component sound keys** (`Components.cfz` definitions, read by the definition
parser): `damagesounds` (comma list, 31 definitions), `attachsound` (workshop
attach: `cmp_generic` 89, `cmp_wheels` 13, `cmp_sawblade` 6, `cmp_airtank` 6,
`cmp_battery` 2, ...), `firesound` (weapons), `motorsound` (loop) and
`runsequence` (`motor_spinup`, `motorloop`, `motor_spindown`). Chassis impact
sounds come from the armour (`chassis_plast/alum/steel/tit`).
`ra2sim::rig::ComponentSounds` carries them.

**Impact rule (`FUN_0045f3a0`, the victim's vtable `+0x6c`, from `FUN_00417510`
for every priced contact).** (1) Sparks only when `10 < damage`. (2) Sound: a
random element of the struck component's `damagesounds` (`FUN_0045cad0`: random
index into the vector of 16-byte strings at `component+4..+8`) is played at
`FUN_004194f0(CRect(4.0, 100.0, -1500.0, 0.0))` hundredths of a decibel, a
clamped linear map of the damage from 4 (silent, `-1500` is the floor and the
test is `-1500 < v`) to 100 (unity). The rectangle is built in the match-object
constructor (`0x412570`) from the floats at `0x6a5794/98/8c/90`. This is
`ra2_audio::rules::impact_volume_hdb`. A second rectangle at `+0x14`
(`0.5, 100, ?, 0`) is the variant selected by the second argument and is not
used by `FUN_0045f3a0`.

**Spatialisation.** The recording of a Scout-vs-ALARM Combat Arena fight is
exactly dual-mono (left == right for every sample), while the main-menu
recording has the same stereo width as the decoded MP3 (L-R energy 4.87% in both
the original and ours). So the music is stereo and the match's effects are
centred. We therefore play script sounds created at `(0,0,0)` (all music,
announcers and crowd) and the match's impacts/motors centred; a sound with a
real location (hazard loops) keeps the positional model with DirectSound3D's
`1/d` rolloff.

## 8. What the client does now

* **Fades and volume.** `plus.setVolume(h, v, ?)` stores the handle's level
  (hundredths of a dB; applies to the next play too, since the scripts set it
  before `loopSound`), `fadeInToLoop(h, v, ms)` starts the handle looping from
  silence and ramps linearly to `v`, `fadeOutLoop(h, ms)` ramps to silence and
  stops. `Audio::advance(dt)` runs them on the script host's match time. The
  second `setVolume` integer is accepted and unused (every shipped call passes
  0).
* **Match sounds** (`ra2-app/src/match_sounds.rs`): impacts (rule above),
  countdown beeps and bell, chassis criticals, chat, and a loop per turning
  motor (`motorsound`, or the middle file of `runsequence`), faded in/out over
  0.25 s, released after 0.5 s of stillness. Unrecovered: the original's motor
  pitch/volume scaling with speed, `firesound` timing, arena material sounds
  (`SetSubMaterialSound`/`AddXtraSound`), the crowd-mood table, the immobility
  countdown tick, `frc_generic*`.
* **UI** (`audio::UiSound`): click (activating controls), workshop attach
  (the definition's `attachsound`) and removal. Snapshot, point-plotted, eject
  and the lobby sounds have no UI in this client yet; `AudioSystem::ui` takes
  them when it does.
* **Hooks for the shell.** `AudioSystem::on_screen(screen_id)` /
  `on_screen_opt` switch music (unchanged); `AudioSystem::ui(UiSound)` plays a
  fixed UI sound.
* **Offline render.** `RA2_AUDIO_WAV=out.wav` makes the offscreen runners
  (`--offscreen`, `--match --out`) capture instead of discarding and write a
  48 kHz stereo WAV; `RA2_AUDIO_SECONDS`, `RA2_AUDIO_START`,
  `RA2_AUDIO_MUSIC_OFFSET` pick the window. `RA2_ARENA_INTROS=0` skips the
  arena intro for a fight-only render. `cargo run -p ra2-audio --example
  wavtool` has `stats`, `xcorr`, `wcorr`, `spectra`.

## 9. Comparison with the recordings

`mainmenu-music__15s__000.wav` (13.3 s) is **Main UI / Enter the Battlefield**
starting about 38.3 s into the track (the capture began after the game had
booted); the rest of the screens' tracks give envelope correlations 0.42-0.45
against 0.49 for it and waveform correlations 0.05-0.08 against 0.25. Our
render of the same window (offset 38.3 s, `SoundVolume=MusicVolume=-600`):

| | original | ours |
|---|---:|---:|
| overall RMS | -21.5 dB | -21.4 dB |
| per-second RMS (13 s) | -18.7 -20.0 -21.8 -23.5 -21.3 -21.8 -20.6 -20.1 -21.6 -21.5 -23.3 -25.1 -23.4 | -18.8 -19.6 -21.8 -23.3 -22.1 -21.0 -21.2 -19.3 -21.6 -21.4 -22.9 -24.0 -25.1 |
| spectral-shape correlation (9 octave bands) | | 0.976 |
| L-R energy ratio | 4.87% | 4.87% |

The -6 dB (`-600`) gain model is confirmed (the decoded track is 5.8 dB hotter).

The Combat Arena fight (`match-combat-arena-scout-alarm__20s__000.wav`, 18.1 s)
is dual-mono, RMS -22.2 dB overall (-20 dB at the start, -24 dB by 17 s), with
eight isolated full-scale transients (0.53, 2.49, 4.68, 5.90, 7.28, 9.64, 12.84,
14.09 s) over a continuous bed. Ours (introless, from 3.0 s after the match
clock, i.e. the last second of the countdown then the fight): overall -18.9 dB
(beep first second -17 dB, then a steady -19.1..-20 dB motor bed), spectral
shape correlation 0.87 (oracle has more 500 Hz-4 kHz energy: the crowd and
impacts we do not play). Our Scout-vs-ALARM fight produced **no robot-robot
contact** in 21 s (the AI behaviours diverge from the original), so there are
no impact onsets to line up; the impact path is covered by a unit test.

**Known presentation gap that affects audio.** `plus.introComplete()` is a no-op
and the match counts down immediately, so with `ArenaIntros=1` the arena's intro
music and announcers play over the countdown and the first 25 s of the fight,
where the original plays them before the countdown (its recording of the fight
has none). The ordering belongs to the match/camera work.
