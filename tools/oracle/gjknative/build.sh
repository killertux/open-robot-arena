#!/bin/sh
# build.sh <out dir>: gjkn, the original's distance query (FUN_0055c390) and Johnson reduction
# (FUN_0055f100) run natively. Needs gcc with -m32 (no 32-bit libc: the harness is freestanding)
# and binutils; the kernel must run 32-bit ELF.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
OUT=${1:?absolute output dir}
mkdir -p "$OUT"
gcc -m32 -O1 -ffreestanding -nostdlib -fno-pic -fno-stack-protector -fno-builtin -c "$HERE/gjkn.c" -o "$OUT/gjkn.o"
ld -m elf_i386 -static -e _start -Ttext=0x08048000 "$OUT/gjkn.o" -o "$OUT/gjkn"
python3 "$HERE/pack_img.py" "${RA2_EXE:-$HERE/../../../local/game/Robot Arena 2.exe}" "$OUT/img.bin"
echo "built $OUT/gjkn and $OUT/img.bin"
