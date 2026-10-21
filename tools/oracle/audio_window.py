#!/usr/bin/env python3
"""RMS (dB) of a 16-bit stereo WAV over a window, optionally aligned on a bell time.

    audio_window.py FILE.wav BELL_S T0 T1      # the window is [BELL_S+T0, BELL_S+T1]
"""
import array
import math
import sys
import wave


def rms_db(path, t0, t1):
    w = wave.open(path)
    rate, ch = w.getframerate(), w.getnchannels()
    w.setpos(int(max(t0, 0) * rate))
    raw = w.readframes(int((t1 - t0) * rate))
    s = array.array("h")
    s.frombytes(raw)
    if not s:
        return float("-inf")
    e = sum(x * x for x in s) / len(s)
    return 10 * math.log10(e / 32768.0 ** 2 + 1e-18)


if __name__ == "__main__":
    f, bell, a, b = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4])
    print(f"{rms_db(f, bell + a, bell + b):.1f}")
