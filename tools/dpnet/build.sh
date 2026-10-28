#!/bin/sh
# Build the DirectPlay 8 replacement dpnet.dll (docs/38 section 14) with the
# host's gcc -m32 and GNU ld's i386pe emulation; imports resolve against
# Wine's own PE DLLs (kernel32, ws2_32).
#
#   tools/dpnet/build.sh [OUT.dll]      (default: tools/dpnet/out/dpnet.dll)
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
OUT=${1:-$HERE/out/dpnet.dll}
WINE_DLLS=${WINE_DLLS:-$(ls -d /var/lib/flatpak/app/org.winehq.Wine/x86_64/stable-23.08/*/files/lib32/wine/i386-windows 2>/dev/null | head -1)}
[ -f "$WINE_DLLS/kernel32.dll" ] || { echo "build.sh: set WINE_DLLS to Wine's i386-windows dir" >&2; exit 2; }
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$(dirname "$OUT")"
gcc -m32 -O2 -std=gnu11 -Wall -Wextra -Wno-unused-parameter -fshort-wchar -fleading-underscore -fno-ident \
    -ffreestanding -fno-builtin -fno-pic -fno-stack-protector -fno-asynchronous-unwind-tables \
    -mno-sse -mno-mmx -c "$HERE/ora_dpnet.c" -o "$TMP/ora_dpnet.o"
ld -m i386pe --dll --dynamicbase -e _DllMain --enable-stdcall-fixup --subsystem windows \
    --image-base 0x6e000000 -o "$OUT" "$TMP/ora_dpnet.o" "$HERE/dpnet.def" \
    "$WINE_DLLS/kernel32.dll" "$WINE_DLLS/ws2_32.dll"
echo "built $OUT"
