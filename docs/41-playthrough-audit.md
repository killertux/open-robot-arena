# 41 - Playthrough audit and the five-process flake (`fx/e2e`, 2026-11-02)

## The flaky test

`netplay::a_scripted_match_across_five_processes_counts_every_hit_once` failed
now and then under load ("a guest missed the late `gg` chat"). The chat was not
lost on the wire. Reproduced by running 8 copies at once (40 processes on 8
cores; 2 in 24 failed), with per-process diagnostics, it was **four separate
causes**, three of them protocol bugs that also bite a slow machine in play:

1. **Test timing (the reported symptom).** A joiner's script runs by its *own*
   fight steps and the host ended the match at *its* step 400. Under load a
   joiner can start fighting after the host has already finished, so the `gg`
   at the joiner's step 45 was never sent. The host now ends the match once the
   scripted chat has reached it (fallback: step 6000), so the assertions see a
   lost line rather than a late joiner.
2. **Silence timeout (`ra2net::link`).** A joiner builds its match on the game
   loop, which also pumps its link, so it is silent for seconds; the 10 s
   silence rule dropped it, the host (hearing no one) began alone and the
   dropped joiner never got `StartCountDown`. The silence timeout is 30 s
   (DirectPlay acks from its own threads; ours cannot).
3. **Retransmission timeout (`ra2net::link`).** The round-trip sample grows
   three-fold each time a busy peer answers late (it reached seconds), and the
   resend timeout is three times that: 16 frames the socket dropped waited
   longer for their resend than the whole match lasted (found in the links'
   state: `in_flight` frames with `retries=0` after 100 s, the peer holding
   everything after them out of order). The timeout is now capped (0.5 s, back
   off to 3.2 s).
4. **Connect patience.** A joiner gave up after 12 tries (2.4 s) of `CONNECT`,
   under the session's own 10 s patience; the link now keeps trying for 10 s
   (the host's `Accepting` side still closes at once).

Tests: `a_slow_round_trip_does_not_stretch_the_next_resend`; the two silence
tests and the idle-keep-alive test now use the 30 s timeout, and the talking
peer that never acknowledges now really exercises the retry limit (its peer no
longer times out first). Result: before, 2 of 24 and, with 12 copies at once,
all runs failed; after, **80 of 80** at 7 copies at once (35 processes on 8
cores) and a final **30 of 30** on the committed code.

## Playthrough audit

Run on scratch installs only (`--game` a copy with `Teams/`, `AI/`,
`Robot Designs/`, `ra2.cfg` copied, `ORA_SAVE_DIR` a scratch directory).

* **Exhibition, every arena and type.** `ra2-app --play` over the 12 shipped
  arenas and the five game types: all 28 valid combinations (8 deathmatch, 8
  battle royal, 8 team match, 2 tabletop, 2 king of the hill; the arena list is
  filtered by the type, as in the original) reach the results screen. **Bug
  found:** on *Bridge of Doom* the AI's tick raised `TypeError: can only
  concatenate list (not "tuple") to list`, so the robots' brains stopped when
  they went to cross the bridge. `Arena.GetPath` returned a list and
  `Bridge.py` (`path1 + (self.ToGrid(end),)`) and `hilltop.py`
  (`((0, 13),) + path`, leaving a hazard) add tuples to it. The route is now a
  tuple of tuples (docs/33's table had recorded the difference as "ours returns
  a list"). **Fixed**; test `the_route_is_a_tuple_the_arenas_scripts_can_add_to`.
  No tracebacks, `failed` or `panicked` lines in any of the 70 runs afterwards.
* **Match length.** The dropdown offers No limit and 1 to 5 minutes (tests in
  `exhibition.rs`); the headless `--play` overrides the length with
  `--seconds`, so a judged end is covered by `an_event_match_lasts_the_sessions_match_time`
  and docs/36 section 9.6, not by this sweep.
* **Career.** `--hq --simulate-season` plays the nine-event season to its end
  (601 to 641 points, rank 14, saved and reloaded). The full flow (new team,
  Bot Lab, scrap/import/export, join, rounds, award, standings, save, restart,
  load, Finish Later, Forfeit) is `playthrough::a_whole_career_new_team_to_forfeit_survives_a_restart`
  and its neighbours; `saves.rs` checks a read-only install stays untouched.
* **Main menu.** Options (video and audio tabs), Credits, Exhibition, Team HQ,
  Multiplayer open the screens the `.gib` names. The original has no Help
  button on this screen and Options has no key-binding tab: keys are bound per
  robot in the Bot Lab's wiring (saved in the `.bot`), covered by the wiring
  tests.
* **LAN.** Host and join on loopback, lobby chat, a five-process match, a
  rematch in one lobby, a leaving joiner and a host ending early are the
  `netplay.rs` tests (this section's flake lives there).

## Open

* The Bot Lab, a windowed session with real input and the credits screen's
  scroll/return were not driven by hand here; they rest on the existing tests.
* Under extreme overload (60 processes on 8 cores) a joiner can still be
  dropped (30 s of silence while its match builds); a link serviced from its own
  thread, as DirectPlay's is, would remove the cause.
