# 35 — Audio events: what the original plays, from its own sound-call log (M-audio3)

The earlier waves (docs 13, 20) worked from four recordings that were later lost.
This wave re-recorded the original and, for the first time, **logged every sound
call the game makes**, with its time, file and volume, so the behaviour rests on
a call log and not on listening. Everything is from `Robot Arena 2.exe` (file VAs)
and `local/oracle/original/audio-*` (git-ignored; manifest rows in
`local/oracle/manifest.tsv`). Nothing here is transliterated code: it is the
recovered behaviour, its addresses, and the numbers.

## 1. Method, and what Wine does not let us hear

* **Capture** (`tools/oracle/capture_audio.py`): flatpak Wine 9.0 on Xephyr `:5`,
  `parec --monitor-stream` of the game's own PulseAudio/PipeWire sink input
  (48 kHz stereo, the game's stream alone) next to `tools/oracle/frida/ra2_sounds.js`
  hooking the sound API. The first line of each `.jsonl` carries the host epoch of
  the WAV start, so a call at epoch `E` sits at `(E - start)` s in the WAV
  (`audio_events.py` normalises the logs to `t_s kind file volume handle note`).
* **Captured** (`audio-*`): `menu-ui` (main menu music, hover, Options/Cancel/
  Exhibition clicks), `hq-music`, `workshop-music`, `match-box-scout-alarm` (two
  full Combat Arena matches, Scout v ALARM, both shipped AIs, hazards on, intro on:
  Start Game to the results screen and Continue; the second with a backtrace on
  every `Create`), `fight-scout-alarm` (three fights with the intro skipped, 120 to
  170 s each), `music-probe`, `ui-click-survey` (43 clicks over the exhibition,
  options, main menu, Team HQ and workshop screens with a per-click verdict).
* **Limits.** The UI click is a *DirectMusic* one-shot (`0x5e6070`); under Wine it is
  **inaudible** (the sink input is silent, peak 1 LSB, while the call log shows the
  call), so for the clicks the log is the evidence. The `SoundWizard` effects
  (DirectSound) and the MP3 music are audible and recorded. A call at the bell that
  starts the Main UI track (section 3) leaves no stereo content in the recording
  (match audio is exactly dual-mono from the countdown to the end); whether a
  Windows machine plays it is not established. Repeated `frida` attaches wedge
  `frida-server` after a few minutes (restart it; the survey uses one attach), and
  `pkill -f <pattern>` kills the shell whose command line contains the pattern.

## 2. The sound API (addresses)

`SoundWizard` is one global (`0x7146d0` -> object `0x7161b8`); every effect handle is
an `SWHandle {id, valid}` passed *by pointer*; volumes are DirectSound hundredths of
a decibel (0 = unity, -10000 = silent).

| Call | Address | Notes |
|---|---|---|
| `Create(ret*, name, volume, is3D, ?, &pos, max, min)` | `0x4f0000` | ret 0x20; `name` is a std::string object (char* at +4); max/min distance 100.0/10.0 (floats at `0x6ab1cc/d0`) |
| `Play(h, once, vol)` | `0x4f03b0` | `playSound` = (0,0), `playOnce` = (1,0) from the Python binding `0x4b7750` table |
| `Loop(h)` | `0x4f0680` | |
| `Stop(h)` | `0x4f05c0` | |
| `SetVolume(h, vol, ms)` | `0x4f0bb0` | `ms > 0` makes a fade object stepping the volume about every 0.34 s |
| `FadeOut(h, ms)`, `FadeOutLoop(h, ms)` | `0x4f0e30`, `0x4f0ec0` | |
| `FadeInToLoop(h, vol, ms)` | `0x4f0f50` | sets -10000, loops, fades to `vol` |
| position / min / max distance | `0x4f10a0 / 0x4f10f0 / 0x4f1130` | DirectSound3D wrappers `0x629f90 / 0x629f10 / 0x629f50` |
| `CreateAndPlayOnce` | `0x4f0150`..`0x4f0588` | what the C++ one-shots use (impacts, arena material) |
| `SetMusicFolder(dir, 1, 3.0)` | `0x4f1350` | lists `dir\*.mp3` into the playlist at `+0x70` |
| music play / next / tick / stop | `0x4f12d0` / `0x4f1550` / `0x4f1620` / `0x4f15f0` -> `0x4f1330` | `0x4f12d0(track, volume, 0)` -> `0x629760`; the global music volume is `0x718394` (set by `0x4f1600`) |
| UI one-shot `PlayOnce(name)` | `0x5e6070` | gated by the byte at `0x7164cc` (UI sounds on); load `0x6287e0`, download `0x629da0`, play `0x629370`, unload `0x629030` |

3D sources are created at (0,0,0) or at their own position, but the listener is never
moved: every match effect is effectively centred (the recordings are dual-mono).

## 3. Music

The music folders hold one track each (`Main UI`, `Team HQ`, `Workshop`), so
`SetMusicFolder` + play is "this area's track, looping" (`0x4f1620` re-plays it at the
end).

* **Exhibition and options screens do not touch the music.** The Main UI track keeps
  playing through the exhibition setup, its tabs and the Options window (no music
  call in `music-probe` or the survey). Our client used to stop it there; it does not
  now (`AudioSystem::on_screen`).
* **Entering an area**: the main-menu window (`0x4e48b0`) calls `SetMusicFolder("Music\\Main UI")`
  then play, the Team HQ `Music\Team HQ`, the workshop `Music\Workshop`. Setting the folder
  that is already playing does **not** restart it (Main Menu after the exhibition, and
  the workshop's three `SetMusicFolder` calls per tab change: no audible change).
* **Leaving an area for another** (`ui-click-survey`, `hq-music`): the old track is
  **cut at once at the click** (80.25 s, 95.2 s, 116.8 s: -75 dB silence from the
  next sample, no fade), the folder is set about **1.1 s** later (the screen builds), and the new
  track is audible **1.2 to 2.0 s after that** (`hq-music`: 2.9 s from the folder call).
  `MUSIC_GAP_SECONDS = 2.5` is the client's single number for the silent gap.
* **Start Game**: `0x4f15f0` -> `0x4f1330` stops the music at the click. **The bell**:
  `0x4f1550` -> `0x4f12d0` is called again at the bell (Main UI track, -600); the
  recording shows nothing stereo, so the client does not play it (open).
* **Results -> Continue**: the arena sounds stop (every crowd loop, the chant, the intro
  music handle `S`) and `SetMusicFolder("Music\\Main UI")` + play run at once.
* No music fade was seen: the old track's cut is instant, and the `3.0` third argument
  of `SetMusicFolder(dir, 1, 3.0)` (`0x6ab1d8`) produced no audible fade or delay we could
  tie to it (the new track's start varied from 1.2 to 2.9 s after the call). Effects
  fade only through `SetVolume(ms)`, `FadeOut*` and `FadeInToLoop`, which the arena
  scripts call (`docs/20` section 8).

## 4. UI sounds

* **`Sounds\button_click.wav`** is played from the push-button class's mouse-up
  handler (`0x5ec1e0`): pressed, still inside, not a toggle-type button (`+0xe5`), with
  the sound flag (`+0xf8`) set. Verdicts from 43 live clicks (`.clicks.tsv`): the main menu's
  EXHIBITION / TEAM HQ, Options, Main Menu, OK, Cancel, Continue, Workshop, Back to HQ,
  the slot's computer icon and the match-time drop-down button click; the exhibition
  tabs, the arena list, the hazards check box, the mode pictures, the weight
  buttons, the options tabs, check boxes, radios and sliders, the HQ tabs and the
  robot cards do not. There is **no hover sound**: moving over buttons makes no call.
  Client rule: `crate::plays_click` = the widget class is `Button` (`Step::hit` begins
  `Button`); the eject button keeps its own.
* **Other UI one-shots** are `SoundWizard` ones: `player_joined.wav` when a robot is
  accepted into an exhibition slot (the Select Computer Robot dialog's OK; the lobby
  also uses it), `snapshot.wav` at the workshop's snapshot button (the name decoded
  badly in the log: `0x43b8f2`). `AudioSystem::shell_step` plays both.
* The workshop builds five `SoundWizard` loop handles for the robot preview
  (`motorloop.wav` x4 at -700, `burst_trigger.wav` at -500) and never plays them.

## 5. A match, event by event (`match-box-scout-alarm`, times from the first intro line)

| t | Event | Source |
|---|---|---|
| -0.25 | `smell_glue.wav` (intro music) created, looped at 0 | `box.py` `Introduction` |
| 0 | the 22 crowd sounds, `frc_generic1/2/4/5`, `immobile_countdown`, `critical_hit`, `chat`, `match_countdown` created (the match object, `0x415500`; crowd `0x4d46e0`) | C++ |
| 0 | arena line played; `LoudCheer_Loop` fades in to -100 over 0.8 s | script |
| +3 | generic line; +8 hazard line (hazards on); +12 bots line | script |
| +20 | intro music `FadeOutLoop` 2000 ms | script |
| +22 | first `match_countdown.wav`, then one per second at +23 and +24 | countdown window |
| +25 | bell `match_countdown_high.wav` (created and played) | countdown window |
| +25.5 | **one of the six `Begin_*` lines**, 0.50 to 0.68 s after the bell | countdown window (`0x474ef7`) |
| fight | crowd, impacts, weapon triggers, motors (sections 6 to 8) | C++ |
| end | third critical: `critical_hit`, `loud_burstcheer`, `End_*` line, the outro music (`hopp_club`) looped | C++ + script |

The first countdown beep is *played twice* at the same instant (two plays of one
handle: a DirectSound buffer already playing ignores the second), so one audible
beep; the client plays one. The Begin line's gap was 0.51, 0.50, 0.51, 0.50, 0.68 s in five
captures (`BEGIN_DELAY = 0.5`; the test accepts 0.45 to 0.75).

## 6. Impacts and arena materials

**Robot component hit** (`FUN_0045f3a0`, from `0x45f466`; docs/20 section 7):
`CreateAndPlayOnce(random damagesound, volume)` per priced contact, **per victim
component** (both robots of a contact play one each: `titanium_hit` + `hammer_hit`,
each at its own volume). Live volumes: `hammer_hit` -1496..-1304 (damage 4 to 12),
`titanium_hit*` -1481..0; `bracket_hit`, `steel_collision*`, `rubber_collision*` are the
other `damagesounds` seen. The volume rule `ra2_audio::rules::impact_volume_hdb`
(damage 4 -> -1500 floor, 100 -> 0, linear in between) holds for every one of the 88
impacts of the full match: created volume == played volume, all in -1500..0. The
distribution is not ours yet: live, 15 % of impacts are louder than -900; ours
2 %, because our contact damage is smaller (the combat/physics side, not audio).

**Arena sub-material sounds** (`SetSubMaterialSound(name, "metal", factor, file)`,
`0x4bbc17` -> `0x404cc0`; played by the handler at `0x405090`, call site `0x4051b0`
-> `CreateAndPlayOnce`): a robot touching a registered sub-material plays `file` as
a **new one-shot per contact** at

```
v      = factor * FUN_0045a5a0(contact)        (the same magnitude the damage uses)
play   only if v > 0.5                          (0x6a579c)
volume = CRect2(v): v in 0.5..100 -> -300..0    (0x6a57a0 floor, 0x6a5798 top; FUN_00419240 picks
                                                  the match object's second rectangle, +0x14)
```

Live: `hzd_spike_hit.wav` x65 (b) and `sawblade_hits_loop.wav` x37 (c), created
per contact at -137..-298, about 1 Hz per body like the collision-event gate. The
first rectangle (`+0x04`, 4..100 -> -1500..0) is the impact one. `ra2_audio::rules::material_volume_hdb`
is the rule; `ra2sim::combat::material_contacts` finds the contacts (a robot part against
a named prop body); `MatchSounds` looks the prop name up in the arena's registrations
(`ra2script` `ArenaData::sub_materials`). Not recovered: `AddXtraSound` (no shipped arena
registers one with audio), and how the arena's static mesh (not a prop) is
matched to a sub-material name: a hit on the merged static collider makes no material
sound in our sim.

## 7. Weapons and motors

* **Weapon trigger sounds play on the trigger, not on the stroke.** A piston
  (`burstpiston1`, `firesound = air_trigger.wav`, class at `0x448750`, created at -0, level 0)
  plays on **every** trigger press (about every 0.5 s while the shipped AI holds `Flip`,
  81 to 150 per fight); the electric burst motor (`snapper2`, `burst_trigger.wav`,
  `0x446810`, created at **-500**) plays only when it is armed and not already fired
  (twice in 100 s). Neither plays when the component is destroyed (`0x401860`).
  `ra2sim::Robot::take_fire_events` reports the presses (a tiny addition to
  `power_demand`, where the burst timers already read `Fire`).
* **Motor loops**: one `SoundWizard` handle per motor component (`motorloop2.wav`, created
  at **-700**, 3D at the origin), `Loop` while the motor's output command is non-zero and
  `Stop` when it returns to zero; live the AI's one-second think makes every handle
  stop and restart about 0.13 s later (572 loop calls over a 70 s fight for six
  handles). No volume change during play. At the match end all six are stopped at once.
  Ours now follows the command (not the measured hinge rate), at -700, with the pitch
  of docs/20 section 10.
* Weapon `firesound`s of other classes (`cannon`, `flamethrower`, `magnet`, `hovercraft`) were
  not in a captured robot; they play at the default 0 until measured.

## 8. The crowd

The crowd object (docs/20 section 10) is built with the match and **ticks through the
arena intro**: excitement 55 falls a point a second, `normal_to_somewhat` comes 11.1 s
after the crowd sounds are created and `somewhat_to_bored` 10 s after that, so the
crowd is bored at the bell and the fight start re-enters with `somewhat_to_bored`
(not `mildly_to_normal`, as the countdown-only tick gave). `frc_generic*` + a
`loud_burstcheer` play when a **component comes off** (`FUN_00417d70` -> `0x45f4e0`: 4 to
13 s into three of five fights), and **not** when the robot is destroyed (that is
`critical_hit` x3, one `loud_burstcheer` and the arena's `End_*` line). Mood rises
play `med_burstcheer` (seen at 2 s steps once the hits start).

## 9. In the client

| Behaviour | Where | Test |
|---|---|---|
| `Begin_*` at bell + 0.5 s | `match_sounds::BEGIN_LINES/BEGIN_DELAY` | `match_mode::the_bell_is_followed_half_a_second_later_by_one_begin_line`, `audio_events::the_begin_line_follows_the_bell...` |
| crowd ticks in the intro | `MatchSounds::observe` | `the_crowd_ticks_through_the_intro`, `audio_events::the_crowd_is_bored_at_the_bell` |
| trigger sounds, levels | `Robot::take_fire_events`, `SoundCue::Fire` | `duel::a_trigger_press_makes_one_fire_event...`, `weapon_triggers_sound_once_per_press`, `audio_events::motor_and_weapon_levels...` |
| motor loops from the command, -700 | `MatchSounds::observe`, `MOTOR_LOOP_HDB` | `audio_events::motor_and_weapon_levels...` |
| component loss: `frc` + burst cheer; destruction: burst cheer only | `MatchSounds::observe` | live comparison (section 10) |
| arena material sounds | `rules::material_volume_hdb`, `combat::material_contacts`, `SoundCue::Material` | `rules::arena_material_sounds...`, `duel::a_robot_touching_a_named_prop...`, `audio_events::arena_material_sounds_use_the_second_rectangle` |
| click only for `Button`; `player_joined`, `snapshot` | `plays_click`, `AudioSystem::shell_step` | `audio::only_push_buttons_click`, `headless::the_named_route...` |
| music: unmapped screens keep the track; cut + gap | `AudioSystem::on_screen`, `with_music_gap`, `tick_music` | `audio::another_area_cuts_the_track...`, `audio::a_screen_change_switches_music...` |
| intro schedule follows `box.py` | scripts | `audio_events::the_intro_and_countdown_follow_the_box_script` |

The `audio_events` tests read the git-ignored captures and skip without them.

## 10. Validation: ours against the recordings, same scenario

Scenario: Combat Arena, Scout v ALARM, hazards on, intro on, 100 s
(`RA2_MATCH_HAZARDS=1 RA2_AUDIO_TRACE=1 ra2-app --match --seconds 100 --bots
AI/Team0/Bot0.bot,AI/Team0/Bot1.bot`), against `audio-match-box-scout-alarm__live__000`,
both aligned on the bell (`tools/oracle/audio_compare.py`). Times are seconds from the
bell; the picks (which announcer, which impact sample) are random in both.

| | original | before | after |
|---|---:|---:|---:|
| countdown beeps | -3.0 -2.0 -1.0 | same | same |
| `Begin_*` line | +0.51 | none | +0.48 |
| intro lines (arena / generic / hazard / bots) | -25.0 / -22.0 / -17.0 / -13.0 | -21.0, -18.0 / none / none / -13.0 (hazards off) | -25.0 / -22.0 / -17.0 / -13.0 |
| crowd `normal_to_somewhat` | -14.5 | +18.3 (countdown-only tick) | -14.1 |
| crowd at the bell | `somewhat_to_bored` | `normal_loop` + `mildly_to_normal` | `somewhat_to_bored` |
| burst-motor trigger sounds, first 60 s | 2 | 136 (swing detector) | 3 |
| piston trigger sounds, first 60 s | 81 | 0 | 98 |
| motor loop starts, first 60 s | 486 | 0 after the bell | 380 |
| impacts, first 60 s | 80 | 76 | 74 |
| arena material sounds | 0 (none touched) | 0 | 5 (hazards on) |
| fight-window level (bell + 5.5..33.5 s) | -19.2 dB | -22.6 dB | -27.1 dB |
| spectral shape, same window (9 octave bands) | | 0.923 | 0.862 |

Reading the last two rows: the event timing and the class counts now match; the
level and spectrum do not, and got worse for a good reason. Before, the motors played at 0 dB (7 dB too
loud) and the burst trigger fired 136 times, which happened to fill the spectrum. Now
the remaining 8 dB gap is the crowd and the hits: live, the first big hits (>50
damage) excite the crowd to `normal` and `loud` loops (-21 / -15 dB at unity), and 15 %
of impacts are louder than -900, which supply the 4 and 8 kHz bands (-12 and -17 dB
low here). Our contact damage is smaller, so our crowd stays bored (-24 dB) and our
hits stay near the floor. That is the damage scale of the combat side, flagged in
`docs/20` section 10 and not touched here.

## 11. Open

* The Main UI request at the bell (section 3) and whether a Windows machine plays it.
* `AddXtraSound`; arena material sounds for a hit on the static collider.
* Weapon firesound levels of the cannon, flamethrower, magnet and hovercraft classes.
* Impact loudness distribution (damage scale), the crowd it excites, and the restart
  cadence of the motor loops (ours stop/start about twice as often as the original's one per
  second per handle).
* The music's silent gap is one constant (2.5 s) for a 1.1 s + 1.2..2.0 s range.
