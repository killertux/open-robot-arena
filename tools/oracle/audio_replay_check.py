#!/usr/bin/env python3
"""Level check of the mixer model against a recording of the same run (docs/35 section 13).

    audio_replay_check.py RECORDING.wav REPLAY.wav BELL_S [T0 T1]

REPLAY.wav is `ra2-audio`'s `replay` example (the original's own call log mixed with our
flat, centred, hundredths-of-a-dB gain model).  Prints, for the window [BELL+T0, BELL+T1]
(default 5.5 to 55 s), the two RMS levels and the per-second difference (mean, std) so that
what is left over is the events, not the mixing rules.
"""
import array
import math
import sys
import wave


def secs(path):
    w = wave.open(path)
    rate = w.getframerate()
    s = array.array("h")
    s.frombytes(w.readframes(w.getnframes()))
    out = []
    for i in range(0, len(s) - rate * 2, rate * 2):
        seg = s[i : i + rate * 2]
        out.append(10 * math.log10(sum(x * x for x in seg) / len(seg) / 32768.0**2 + 1e-18))
    return out


def main():
    a, b, bell = secs(sys.argv[1]), secs(sys.argv[2]), float(sys.argv[3])
    t0 = int(bell + (float(sys.argv[4]) if len(sys.argv) > 4 else 5.5))
    t1 = int(bell + (float(sys.argv[5]) if len(sys.argv) > 5 else 55))
    t1 = min(t1, len(a), len(b))
    d = [b[i] - a[i] for i in range(t0, t1)]
    mean = sum(d) / len(d)
    std = math.sqrt(sum((x - mean) ** 2 for x in d) / len(d))

    def power(v):
        return 10 * math.log10(sum(10 ** (x / 10) for x in v) / len(v))

    print(f"window {t0}..{t1} s: recording {power(a[t0:t1]):.1f} dB, replay {power(b[t0:t1]):.1f} dB, "
          f"per-second difference {mean:+.2f} dB (std {std:.2f})")


if __name__ == "__main__":
    main()
