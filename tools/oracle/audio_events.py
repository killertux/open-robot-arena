#!/usr/bin/env python3
"""Normalise a capture_audio.py log into one event per line.

    audio_events.py local/oracle/original/audio-NAME__live__NNN.jsonl > NAME.events.tsv

Columns: `t_s  kind  file  volume_hdb  handle  note`.  `t_s` is seconds from the
start of the WAV (the parec start); `kind` is one of

  create       SoundWizard::Create (0x4f0000): file, initial volume (hdb)
  play         Play (0x4f03b0); `note` is `once` or `once=0`; volume is arg 3
  loop         Loop (0x4f0680)
  stop         Stop (0x4f05c0)
  setvolume    SetVolume (0x4f0bb0): volume, `note` = fade ms
  fadeout      FadeOut / FadeOutLoop (0x4f0e30 / 0x4f0ec0): `note` = ms
  fadein       FadeInToLoop (0x4f0f50): volume, `note` = ms
  music_dir    SetMusicFolder (0x4f1350)
  music_play   music track start (0x4f12d0): file, volume
  ui_click     the UI one-shot (0x5e6070 -> 0x6287e0): file
  music_vol    global music volume (0x4f1600)

Buffer-level rows (B, D) and positions are dropped; they are in the raw log.
"""
import json
import os
import sys


def base(name):
    if not name:
        return ""
    return name.replace("\\", "/").split("/")[-1]


def main():
    path = sys.argv[1]
    rows = [json.loads(l) for l in open(path) if l.strip()]
    meta = rows[0]
    t0 = meta["parec_epoch_ms"]
    names = {}
    out = []

    def t(ms):
        return (ms - t0) / 1000.0

    for r in rows[1:]:
        tag = r[0]
        if tag == "C":
            h = r[6][0] if r[6] else -1
            vol = int(r[4][2], 16)
            vol = vol - (1 << 32) if vol >= 1 << 31 else vol
            names[h] = base(r[5])
            out.append((t(r[2]), "create", base(r[5]), vol, h, ""))
        elif tag == "P":
            h = r[3]
            out.append((t(r[2]), "play", names.get(h, ""), r[5], h, "once" if r[4] else ""))
        elif tag == "L":
            out.append((t(r[2]), "loop", names.get(r[3], ""), "", r[3], ""))
        elif tag == "S":
            out.append((t(r[2]), "stop", names.get(r[3], ""), "", r[3], ""))
        elif tag == "V":
            out.append((t(r[2]), "setvolume", names.get(r[3], ""), r[4], r[3], r[5]))
        elif tag in "OQ":
            out.append((t(r[2]), "fadeout", names.get(r[3], ""), "", r[3], r[4]))
        elif tag == "I":
            out.append((t(r[2]), "fadein", names.get(r[3], ""), r[4], r[3], r[5]))
        elif tag == "K":
            out.append((t(r[2]), "music_dir", base(r[3]), "", "", ""))
        elif tag == "T":
            out.append((t(r[2]), "music_play", base(r[3]), r[4], "", ""))
        elif tag == "U":
            out.append((t(r[2]), "ui_click", base(r[3]), "", "", ""))
        elif tag == "G":
            out.append((t(r[2]), "music_vol", "", r[3], "", ""))
    out.sort(key=lambda x: x[0])
    print("t_s\tkind\tfile\tvolume_hdb\thandle\tnote")
    for o in out:
        print("%.3f\t%s\t%s\t%s\t%s\t%s" % o)


if __name__ == "__main__":
    main()
