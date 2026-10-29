// Pin the C runtime rand() of the original: replaces _rand (0x63dcf2, the VS
// CRT LCG; the shipped exe has no srand/rand symbols, found by the constant
// 0x269ec3) with our own LCG from a fixed seed.  Load a fresh session before
// each match (state is per session).  Callers: a std::random_shuffle of
// collision triangles (0x546f70) and two others (0x620b50, 0x62c430).
'use strict';
let s = 1234;
Interceptor.replace(ptr('0x63dcf2'), new NativeCallback(function () {
  s = (Math.imul(s, 0x343fd) + 0x269ec3) >>> 0;
  return (s >>> 16) & 0x7fff;
}, 'int', []));
send({ev: 'ra2_seed', seed: 1234});
