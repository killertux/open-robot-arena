/*
 * ora_dpnet.c - a DirectPlay 8 replacement dpnet.dll for Robot Arena 2 under
 * Wine (Open Robot Arena, docs/38 section 14).
 *
 * Wine's builtin dpnet.dll stubs IDirectPlay8Peer::Host, Connect and EnumHosts
 * (FIXME "stub", nothing on the wire), so the original's multiplayer lobby
 * never opens under Wine. This DLL implements the part of the DirectPlay 8 API
 * the game uses (IDirectPlay8Peer, IDirectPlay8Address and their class
 * factory), written from the public DirectPlay 8 interface (dplay8.h,
 * dpaddr.h): method order, structures, message ids, flags and result codes.
 * The session itself runs in a separate process, `ora-dpnet-bridge`
 * (engine/crates/ra2net/src/bridge.rs), reached over a loopback TCP pipe:
 * it speaks the Open Robot Arena session transport, so the original and the
 * port play each other on a LAN.
 *
 * Built freestanding with gcc -m32 and GNU ld's i386pe emulation (build.sh):
 * no C library, imports from kernel32 and ws2_32 only.
 */

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef int i32;
typedef u32 HRESULT;
typedef unsigned short WCHAR;
typedef void *HANDLE;
typedef u32 SOCKET;

#define WINAPI __attribute__((stdcall))
#define NULL ((void *)0)

/* ---- imports ---------------------------------------------------------- */

typedef struct { void *a, *b; i32 c, d; void *e, *f; } CRITICAL_SECTION;

WINAPI HANDLE GetProcessHeap(void);
WINAPI void *HeapAlloc(HANDLE, u32, u32);
WINAPI i32 HeapFree(HANDLE, u32, void *);
WINAPI HANDLE CreateThread(void *, u32, u32 (WINAPI *)(void *), void *, u32, u32 *);
WINAPI HANDLE CreateEventA(void *, i32, i32, const char *);
WINAPI i32 SetEvent(HANDLE);
WINAPI i32 ResetEvent(HANDLE);
WINAPI u32 WaitForSingleObject(HANDLE, u32);
WINAPI i32 CloseHandle(HANDLE);
WINAPI void InitializeCriticalSection(CRITICAL_SECTION *);
WINAPI void EnterCriticalSection(CRITICAL_SECTION *);
WINAPI void LeaveCriticalSection(CRITICAL_SECTION *);
WINAPI void DeleteCriticalSection(CRITICAL_SECTION *);
WINAPI u32 GetCurrentThreadId(void);
WINAPI u32 GetTickCount(void);
WINAPI i32 MultiByteToWideChar(u32, u32, const char *, i32, WCHAR *, i32);
WINAPI i32 WideCharToMultiByte(u32, u32, const WCHAR *, i32, char *, i32, const char *, i32 *);
WINAPI u32 GetEnvironmentVariableA(const char *, char *, u32);
WINAPI void OutputDebugStringA(const char *);
WINAPI HANDLE CreateFileA(const char *, u32, u32, void *, u32, u32, HANDLE);
WINAPI i32 WriteFile(HANDLE, const void *, u32, u32 *, void *);
WINAPI i32 InterlockedIncrement(volatile i32 *);
WINAPI i32 InterlockedDecrement(volatile i32 *);

typedef struct { u16 family; u16 port; u32 addr; u8 zero[8]; } SOCKADDR_IN;
WINAPI i32 WSAStartup(u16, void *);
WINAPI SOCKET socket(i32, i32, i32);
WINAPI i32 connect(SOCKET, const void *, i32);
WINAPI i32 send(SOCKET, const void *, i32, i32);
WINAPI i32 recv(SOCKET, void *, i32, i32);
WINAPI i32 closesocket(SOCKET);
WINAPI i32 shutdown(SOCKET, i32);
WINAPI i32 setsockopt(SOCKET, i32, i32, const void *, i32);
WINAPI u32 inet_addr(const char *);
WINAPI u16 htons(u16);

#define CP_UTF8 65001
#define INFINITE 0xffffffffu
#define INVALID_SOCKET 0xffffffffu

/* ---- results, ids, flags (dplay8.h) ------------------------------------ */

#define S_OK 0u
#define S_FALSE 1u
#define E_NOTIMPL 0x80004001u
#define E_NOINTERFACE 0x80004002u
#define E_POINTER 0x80004003u
#define E_FAIL 0x80004005u
#define E_OUTOFMEMORY 0x8007000Eu
#define E_INVALIDARG 0x80070057u
#define CLASS_E_NOAGGREGATION 0x80040110u
#define CLASS_E_CLASSNOTAVAILABLE 0x80040111u
#define DPNSUCCESS_PENDING 0x0015800Eu
#define DPNERR_BUFFERTOOSMALL 0x80158100u
#define DPNERR_HOSTREJECTEDCONNECTION 0x80158260u
#define DPNERR_INVALIDPLAYER 0x80158400u
#define DPNERR_DOESNOTEXIST 0x801580D0u
#define DPNERR_UNINITIALIZED 0x80158590u
#define DPNERR_ALREADYINITIALIZED 0x801580A0u
#define DPNERR_NOCONNECTION 0x80158300u
#define DPNERR_USERCANCEL 0x80158570u
#define DPNERR_UNSUPPORTED 0x80158580u
#define DPNERR_CONNECTIONLOST 0x801500C8u

#define DPN_MSGID_ASYNC_OP_COMPLETE 0xFFFF0003u
#define DPN_MSGID_CONNECT_COMPLETE 0xFFFF0005u
#define DPN_MSGID_CREATE_PLAYER 0xFFFF0007u
#define DPN_MSGID_DESTROY_PLAYER 0xFFFF0009u
#define DPN_MSGID_ENUM_HOSTS_QUERY 0xFFFF000Au
#define DPN_MSGID_ENUM_HOSTS_RESPONSE 0xFFFF000Bu
#define DPN_MSGID_INDICATE_CONNECT 0xFFFF000Eu
#define DPN_MSGID_PEER_INFO 0xFFFF0010u
#define DPN_MSGID_RECEIVE 0xFFFF0011u
#define DPN_MSGID_RETURN_BUFFER 0xFFFF0013u
#define DPN_MSGID_SEND_COMPLETE 0xFFFF0014u
#define DPN_MSGID_TERMINATE_SESSION 0xFFFF0016u

#define DPNSEND_SYNC 0x80000000u
#define DPNSEND_NOCOMPLETE 0x0002u
#define DPNCONNECT_SYNC 0x80000000u
#define DPNENUMHOSTS_SYNC 0x80000000u
#define DPNPLAYER_LOCAL 0x2u
#define DPNPLAYER_HOST 0x4u
#define DPNINFO_NAME 0x1u
#define DPNINFO_DATA 0x2u
#define DPNDESTROYPLAYERREASON_NORMAL 1u
#define DPNDESTROYPLAYERREASON_SESSIONTERMINATED 3u

#define DPNA_DATATYPE_STRING 1u
#define DPNA_DATATYPE_DWORD 2u
#define DPNA_DATATYPE_GUID 3u
#define DPNA_DATATYPE_BINARY 4u
#define DPNA_DATATYPE_STRING_ANSI 5u

typedef struct { u32 d1; u16 d2, d3; u8 d4[8]; } GUID;

static const GUID CLSID_DirectPlay8Peer = {0x286F484D, 0x375E, 0x4458, {0xA2, 0x72, 0xB1, 0x38, 0xE2, 0xF8, 0x0A, 0x6A}};
static const GUID CLSID_DirectPlay8Address = {0x934A9523, 0xA3CA, 0x4BC5, {0xAD, 0xA0, 0xD6, 0xD9, 0x5D, 0x97, 0x94, 0x21}};
static const GUID IID_IDirectPlay8Peer = {0x5102DACF, 0x241B, 0x11D3, {0xAE, 0xA7, 0x00, 0x60, 0x97, 0xB0, 0x14, 0x11}};
static const GUID IID_IDirectPlay8Address = {0x83783300, 0x4063, 0x4C8A, {0x9D, 0xB3, 0x82, 0x83, 0x0A, 0x7F, 0xEB, 0x31}};
static const GUID IID_IUnknown = {0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
static const GUID IID_IClassFactory = {0x00000001, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}};
static const GUID CLSID_DP8SP_TCPIP = {0xEBFE7BA0, 0x628D, 0x11D2, {0xAE, 0x0F, 0x00, 0x60, 0x97, 0xB0, 0x14, 0x11}};
/* The one adapter EnumServiceProviders(TCPIP) lists ("all adapters"). */
static const GUID GUID_ORA_ADAPTER = {0x4F524144, 0x504E, 0x4554, {0x41, 0x44, 0x41, 0x50, 0x54, 0x45, 0x52, 0x31}};

typedef HRESULT (WINAPI *PFNDPNMESSAGEHANDLER)(void *, u32, void *);

typedef struct {
    u32 dwSize, dwFlags;
    GUID guidInstance, guidApplication;
    u32 dwMaxPlayers, dwCurrentPlayers;
    WCHAR *pwszSessionName, *pwszPassword;
    void *pvReservedData;
    u32 dwReservedDataSize;
    void *pvApplicationReservedData;
    u32 dwApplicationReservedDataSize;
} DPN_APPLICATION_DESC; /* 0x48 */

typedef struct { u32 dwSize, dwInfoFlags; WCHAR *pwszName; void *pvData; u32 dwDataSize, dwPlayerFlags; } DPN_PLAYER_INFO;
typedef struct { u32 dwBufferSize; u8 *pBufferData; } DPN_BUFFER_DESC;
typedef struct { u32 dwFlags; GUID guid; WCHAR *pwszName; void *pvReserved; u32 dwReserved; } DPN_SERVICE_PROVIDER_INFO;

/* ---- little helpers --------------------------------------------------- */

void *memcpy(void *d, const void *s, u32 n) { u8 *a = d; const u8 *b = s; while (n--) *a++ = *b++; return d; }
void *memset(void *d, int c, u32 n) { u8 *a = d; while (n--) *a++ = (u8)c; return d; }
void *memmove(void *d, const void *s, u32 n)
{
    u8 *a = d; const u8 *b = s;
    if (a < b) { while (n--) *a++ = *b++; } else { a += n; b += n; while (n--) *--a = *--b; }
    return d;
}
int memcmp(const void *x, const void *y, u32 n) { const u8 *a = x, *b = y; for (; n; n--, a++, b++) if (*a != *b) return *a - *b; return 0; }

static void *mem_alloc(u32 n) { return HeapAlloc(GetProcessHeap(), 8 /* HEAP_ZERO_MEMORY */, n ? n : 1); }
static void mem_free(void *p) { if (p) HeapFree(GetProcessHeap(), 0, p); }
static int guid_eq(const GUID *a, const GUID *b) { return memcmp(a, b, sizeof(GUID)) == 0; }
static u32 wlen(const WCHAR *s) { u32 n = 0; if (s) while (s[n]) n++; return n; }
static u32 slen(const char *s) { u32 n = 0; if (s) while (s[n]) n++; return n; }
static WCHAR *wdup(const WCHAR *s) { u32 n = wlen(s); WCHAR *d = mem_alloc((n + 1) * 2); if (s) memcpy(d, s, n * 2); return d; }
static int wieq(const WCHAR *a, const char *b)
{
    for (;; a++, b++) {
        WCHAR x = *a; char y = *b;
        if (x >= 'A' && x <= 'Z') x += 32;
        if (y >= 'A' && y <= 'Z') y += 32;
        if (x != (WCHAR)(u8)y) return 0;
        if (!x) return 1;
    }
}
/* UTF-16 -> UTF-8, NUL-terminated, into a fresh buffer. */
static char *to_utf8(const WCHAR *w, u32 *len)
{
    i32 n = WideCharToMultiByte(CP_UTF8, 0, w ? w : (const WCHAR *)L"", -1, NULL, 0, NULL, NULL);
    char *s = mem_alloc(n > 0 ? n : 1);
    if (n > 0) WideCharToMultiByte(CP_UTF8, 0, w ? w : (const WCHAR *)L"", -1, s, n, NULL, NULL);
    if (len) *len = slen(s);
    return s;
}
static WCHAR *from_utf8(const char *s, u32 n)
{
    char *z = mem_alloc(n + 1);
    memcpy(z, s, n);
    i32 m = MultiByteToWideChar(CP_UTF8, 0, z, -1, NULL, 0);
    WCHAR *w = mem_alloc((m > 0 ? m : 1) * 2);
    if (m > 0) MultiByteToWideChar(CP_UTF8, 0, z, -1, w, m);
    mem_free(z);
    return w;
}

/* ---- log (ORA_DPNET_LOG=<windows path>) ------------------------------- */

static HANDLE g_log = 0;
static CRITICAL_SECTION g_log_cs;

static void log_init(void)
{
    char path[260];
    InitializeCriticalSection(&g_log_cs);
    u32 n = GetEnvironmentVariableA("ORA_DPNET_LOG", path, sizeof path);
    if (n > 0 && n < sizeof path)
        g_log = CreateFileA(path, 0x40000000 /* GENERIC_WRITE */, 1 | 2, NULL, 2 /* CREATE_ALWAYS */, 0x80, 0);
    if (g_log == (HANDLE)-1) g_log = 0;
}

static char *put_u(char *o, u32 v, u32 base)
{
    char t[12]; int i = 0;
    do { u32 d = v % base; t[i++] = (char)(d < 10 ? '0' + d : 'a' + d - 10); v /= base; } while (v);
    while (i) *o++ = t[--i];
    return o;
}

/* %s %u %x %p (%x of a pointer), %S (wide). */
static void logf(const char *fmt, ...)
{
    char buf[512], *o = buf, *end = buf + sizeof buf - 16;
    __builtin_va_list ap;
    __builtin_va_start(ap, fmt);
    o = put_u(o, GetTickCount(), 10); *o++ = ' ';
    for (; *fmt && o < end; fmt++) {
        if (*fmt != '%') { *o++ = *fmt; continue; }
        fmt++;
        if (*fmt == 's') { const char *s = __builtin_va_arg(ap, const char *); if (!s) s = "(null)"; while (*s && o < end) *o++ = *s++; }
        else if (*fmt == 'S') { const WCHAR *s = __builtin_va_arg(ap, const WCHAR *); if (s) while (*s && o < end) { *o++ = *s < 128 ? (char)*s : '?'; s++; } }
        else if (*fmt == 'u') o = put_u(o, __builtin_va_arg(ap, u32), 10);
        else if (*fmt == 'x' || *fmt == 'p') o = put_u(o, __builtin_va_arg(ap, u32), 16);
        else *o++ = *fmt;
    }
    __builtin_va_end(ap);
    *o++ = '\n'; *o = 0;
    if (g_log) {
        u32 w;
        EnterCriticalSection(&g_log_cs);
        WriteFile(g_log, buf, (u32)(o - buf), &w, NULL);
        LeaveCriticalSection(&g_log_cs);
    }
}

/* ---- IDirectPlay8Address ---------------------------------------------- */

typedef struct Comp { struct Comp *next; WCHAR *name; u8 *data; u32 size, type; } Comp;

typedef struct Address {
    void **vtbl;
    volatile i32 ref;
    GUID sp; int has_sp;
    GUID device; int has_device;
    Comp *comps;
    u8 *user; u32 user_size;
} Address;

static void **g_addr_vtbl;

static void addr_clear(Address *a)
{
    Comp *c = a->comps;
    while (c) { Comp *n = c->next; mem_free(c->name); mem_free(c->data); mem_free(c); c = n; }
    a->comps = NULL;
    a->has_sp = a->has_device = 0;
    mem_free(a->user); a->user = NULL; a->user_size = 0;
}

static Comp *addr_find(Address *a, const WCHAR *name)
{
    for (Comp *c = a->comps; c; c = c->next)
        if (wlen(c->name) == wlen(name)) {
            u32 i; for (i = 0; name[i] && (c->name[i] | 32) == (name[i] | 32); i++) {}
            if (!name[i] && !c->name[i]) return c;
        }
    return NULL;
}

static HRESULT addr_add(Address *a, const WCHAR *name, const void *data, u32 size, u32 type)
{
    if (wieq(name, "provider") && type == DPNA_DATATYPE_GUID && size == 16) { memcpy(&a->sp, data, 16); a->has_sp = 1; return S_OK; }
    if (wieq(name, "device") && type == DPNA_DATATYPE_GUID && size == 16) { memcpy(&a->device, data, 16); a->has_device = 1; return S_OK; }
    Comp *c = addr_find(a, name);
    if (!c) { c = mem_alloc(sizeof *c); c->name = wdup(name); c->next = a->comps; a->comps = c; }
    mem_free(c->data);
    c->data = mem_alloc(size);
    memcpy(c->data, data, size);
    c->size = size; c->type = type;
    return S_OK;
}

static Address *addr_new(void)
{
    Address *a = mem_alloc(sizeof *a);
    a->vtbl = g_addr_vtbl; a->ref = 1;
    return a;
}

/* A component as a narrow string (hostname: WCHAR or ANSI). */
static char *addr_get_str(Address *a, const char *key)
{
    for (Comp *c = a->comps; c; c = c->next)
        if (wieq(c->name, key)) {
            if (c->type == DPNA_DATATYPE_STRING) { WCHAR *w = mem_alloc(c->size + 2); memcpy(w, c->data, c->size); char *s = to_utf8(w, NULL); mem_free(w); return s; }
            if (c->type == DPNA_DATATYPE_STRING_ANSI) { char *s = mem_alloc(c->size + 1); memcpy(s, c->data, c->size); return s; }
        }
    return NULL;
}
static u32 addr_get_dword(Address *a, const char *key, u32 dflt)
{
    for (Comp *c = a->comps; c; c = c->next)
        if (wieq(c->name, key) && c->type == DPNA_DATATYPE_DWORD && c->size == 4) return *(u32 *)c->data;
    return dflt;
}

static WINAPI HRESULT A_QueryInterface(Address *a, const GUID *riid, void **ppv)
{
    if (!ppv) return E_POINTER;
    if (guid_eq(riid, &IID_IUnknown) || guid_eq(riid, &IID_IDirectPlay8Address)) { InterlockedIncrement(&a->ref); *ppv = a; return S_OK; }
    *ppv = NULL;
    return E_NOINTERFACE;
}
static WINAPI u32 A_AddRef(Address *a) { return InterlockedIncrement(&a->ref); }
static WINAPI u32 A_Release(Address *a)
{
    i32 r = InterlockedDecrement(&a->ref);
    if (r == 0) { addr_clear(a); mem_free(a); }
    return r;
}

/* x-directplay:/provider=%7B...%7D;hostname=...;port=... */
static void url_append(char **o, char *end, const char *s) { while (*s && *o < end) *(*o)++ = *s++; }
static void guid_text(char *t, const GUID *g)
{
    static const char hx[] = "0123456789ABCDEF";
    const u8 *b = (const u8 *)g; char *o = t;
    u32 v = g->d1; for (int i = 28; i >= 0; i -= 4) *o++ = hx[(v >> i) & 15];
    *o++ = '-'; for (int i = 12; i >= 0; i -= 4) *o++ = hx[(g->d2 >> i) & 15];
    *o++ = '-'; for (int i = 12; i >= 0; i -= 4) *o++ = hx[(g->d3 >> i) & 15];
    *o++ = '-'; for (int i = 8; i < 10; i++) { *o++ = hx[b[i] >> 4]; *o++ = hx[b[i] & 15]; }
    *o++ = '-'; for (int i = 10; i < 16; i++) { *o++ = hx[b[i] >> 4]; *o++ = hx[b[i] & 15]; }
    *o = 0;
}
static u32 addr_url(Address *a, char *buf, u32 cap)
{
    char tmp[1024], *o = tmp, *end = tmp + sizeof tmp - 1, g[40];
    url_append(&o, end, "x-directplay:/");
    int first = 1;
    if (a->has_sp) { guid_text(g, &a->sp); url_append(&o, end, "provider=%7B"); url_append(&o, end, g); url_append(&o, end, "%7D"); first = 0; }
    if (a->has_device) { guid_text(g, &a->device); url_append(&o, end, first ? "device=%7B" : ";device=%7B"); url_append(&o, end, g); url_append(&o, end, "%7D"); first = 0; }
    for (Comp *c = a->comps; c; c = c->next) {
        char *n = to_utf8(c->name, NULL);
        if (!first) url_append(&o, end, ";");
        first = 0;
        url_append(&o, end, n); url_append(&o, end, "=");
        mem_free(n);
        if (c->type == DPNA_DATATYPE_DWORD && c->size == 4) {
            char t[12]; *put_u(t, *(u32 *)c->data, 10) = 0; url_append(&o, end, t);
        } else if (c->type == DPNA_DATATYPE_STRING) {
            WCHAR *w = mem_alloc(c->size + 2); memcpy(w, c->data, c->size);
            char *v = to_utf8(w, NULL); url_append(&o, end, v); mem_free(v); mem_free(w);
        } else if (c->type == DPNA_DATATYPE_STRING_ANSI) {
            char *v = mem_alloc(c->size + 1); memcpy(v, c->data, c->size); url_append(&o, end, v); mem_free(v);
        }
    }
    *o = 0;
    u32 n = (u32)(o - tmp) + 1;
    if (buf && cap >= n) memcpy(buf, tmp, n);
    return n;
}

static WINAPI HRESULT A_BuildFromURLW(Address *a, WCHAR *url) { (void)a; (void)url; logf("addr BuildFromURLW (unsupported)"); return E_NOTIMPL; }
static WINAPI HRESULT A_BuildFromURLA(Address *a, char *url) { (void)a; (void)url; logf("addr BuildFromURLA (unsupported)"); return E_NOTIMPL; }
static WINAPI HRESULT A_Duplicate(Address *a, Address **out)
{
    if (!out) return E_POINTER;
    Address *d = addr_new();
    d->sp = a->sp; d->has_sp = a->has_sp; d->device = a->device; d->has_device = a->has_device;
    for (Comp *c = a->comps; c; c = c->next) addr_add(d, c->name, c->data, c->size, c->type);
    *out = d;
    return S_OK;
}
static WINAPI HRESULT A_SetEqual(Address *a, Address *b)
{
    if (!b) return E_POINTER;
    addr_clear(a);
    a->sp = b->sp; a->has_sp = b->has_sp; a->device = b->device; a->has_device = b->has_device;
    for (Comp *c = b->comps; c; c = c->next) addr_add(a, c->name, c->data, c->size, c->type);
    return S_OK;
}
static WINAPI HRESULT A_IsEqual(Address *a, Address *b)
{
    char x[1024], y[1024];
    if (!b) return E_POINTER;
    addr_url(a, x, sizeof x); addr_url(b, y, sizeof y);
    return slen(x) == slen(y) && memcmp(x, y, slen(x)) == 0 ? S_OK : S_FALSE;
}
static WINAPI HRESULT A_Clear(Address *a) { addr_clear(a); return S_OK; }
static WINAPI HRESULT A_GetURLA(Address *a, char *buf, u32 *size)
{
    if (!size) return E_POINTER;
    u32 n = addr_url(a, NULL, 0);
    if (!buf || *size < n) { *size = n; return DPNERR_BUFFERTOOSMALL; }
    addr_url(a, buf, *size); *size = n;
    return S_OK;
}
static WINAPI HRESULT A_GetURLW(Address *a, WCHAR *buf, u32 *size)
{
    char tmp[1024];
    if (!size) return E_POINTER;
    u32 n = addr_url(a, tmp, sizeof tmp);
    if (!buf || *size < n) { *size = n; return DPNERR_BUFFERTOOSMALL; }
    for (u32 i = 0; i < n; i++) buf[i] = (u8)tmp[i];
    *size = n;
    return S_OK;
}
static WINAPI HRESULT A_GetSP(Address *a, GUID *g) { if (!g) return E_POINTER; if (!a->has_sp) return DPNERR_DOESNOTEXIST; *g = a->sp; return S_OK; }
static WINAPI HRESULT A_GetUserData(Address *a, void *buf, u32 *size)
{
    if (!size) return E_POINTER;
    if (!a->user) return DPNERR_DOESNOTEXIST;
    if (!buf || *size < a->user_size) { *size = a->user_size; return DPNERR_BUFFERTOOSMALL; }
    memcpy(buf, a->user, a->user_size); *size = a->user_size;
    return S_OK;
}
static WINAPI HRESULT A_SetSP(Address *a, const GUID *g) { if (!g) return E_POINTER; a->sp = *g; a->has_sp = 1; return S_OK; }
static WINAPI HRESULT A_SetUserData(Address *a, const void *data, u32 size)
{
    mem_free(a->user); a->user = NULL; a->user_size = 0;
    if (data && size) { a->user = mem_alloc(size); memcpy(a->user, data, size); a->user_size = size; }
    return S_OK;
}
static WINAPI HRESULT A_GetNumComponents(Address *a, u32 *n)
{
    if (!n) return E_POINTER;
    u32 k = a->has_sp + a->has_device;
    for (Comp *c = a->comps; c; c = c->next) k++;
    *n = k;
    return S_OK;
}
static HRESULT comp_out(const void *data, u32 size, u32 type, void *buf, u32 *bufsize, u32 *ptype)
{
    if (!bufsize || !ptype) return E_POINTER;
    *ptype = type;
    if (!buf || *bufsize < size) { *bufsize = size; return DPNERR_BUFFERTOOSMALL; }
    memcpy(buf, data, size); *bufsize = size;
    return S_OK;
}
static WINAPI HRESULT A_GetComponentByName(Address *a, const WCHAR *name, void *buf, u32 *size, u32 *type)
{
    if (!name) return E_POINTER;
    if (wieq(name, "provider") && a->has_sp) return comp_out(&a->sp, 16, DPNA_DATATYPE_GUID, buf, size, type);
    if (wieq(name, "device") && a->has_device) return comp_out(&a->device, 16, DPNA_DATATYPE_GUID, buf, size, type);
    Comp *c = addr_find(a, name);
    if (!c) return DPNERR_DOESNOTEXIST;
    return comp_out(c->data, c->size, c->type, buf, size, type);
}
static WINAPI HRESULT A_GetComponentByIndex(Address *a, u32 index, WCHAR *name, u32 *namelen, void *buf, u32 *size, u32 *type)
{
    static const WCHAR provider[] = {'p','r','o','v','i','d','e','r',0}, device[] = {'d','e','v','i','c','e',0};
    const WCHAR *n = NULL; const void *data = NULL; u32 dsize = 0, dtype = 0;
    if (!namelen) return E_POINTER;
    if (a->has_sp && index == 0) { n = provider; data = &a->sp; dsize = 16; dtype = DPNA_DATATYPE_GUID; }
    else {
        index -= a->has_sp;
        if (a->has_device && index == 0) { n = device; data = &a->device; dsize = 16; dtype = DPNA_DATATYPE_GUID; }
        else {
            index -= a->has_device;
            Comp *c = a->comps;
            while (c && index) { c = c->next; index--; }
            if (!c) return DPNERR_DOESNOTEXIST;
            n = c->name; data = c->data; dsize = c->size; dtype = c->type;
        }
    }
    u32 nl = wlen(n) + 1;
    if (!name || *namelen < nl || !buf || !size || *size < dsize) { *namelen = nl; if (size) *size = dsize; return DPNERR_BUFFERTOOSMALL; }
    memcpy(name, n, nl * 2); *namelen = nl;
    memcpy(buf, data, dsize); *size = dsize;
    if (type) *type = dtype;
    return S_OK;
}
static WINAPI HRESULT A_AddComponent(Address *a, const WCHAR *name, const void *data, u32 size, u32 type)
{
    if (!name || (!data && size)) return E_POINTER;
    return addr_add(a, name, data, size, type);
}
static WINAPI HRESULT A_GetDevice(Address *a, GUID *g) { if (!g) return E_POINTER; if (!a->has_device) return DPNERR_DOESNOTEXIST; *g = a->device; return S_OK; }
static WINAPI HRESULT A_SetDevice(Address *a, const GUID *g) { if (!g) return E_POINTER; a->device = *g; a->has_device = 1; return S_OK; }
static WINAPI HRESULT A_BuildFromDPADDRESS(Address *a, void *p, u32 n) { (void)a; (void)p; (void)n; return E_NOTIMPL; }

static void *addr_vtbl_[] = {
    A_QueryInterface, A_AddRef, A_Release, A_BuildFromURLW, A_BuildFromURLA, A_Duplicate, A_SetEqual,
    A_IsEqual, A_Clear, A_GetURLW, A_GetURLA, A_GetSP, A_GetUserData, A_SetSP, A_SetUserData,
    A_GetNumComponents, A_GetComponentByName, A_GetComponentByIndex, A_AddComponent, A_GetDevice,
    A_SetDevice, A_BuildFromDPADDRESS,
};

static Address *addr_for(const char *host, u32 port)
{
    static const WCHAR khost[] = {'h','o','s','t','n','a','m','e',0}, kport[] = {'p','o','r','t',0};
    Address *a = addr_new();
    a->sp = CLSID_DP8SP_TCPIP; a->has_sp = 1;
    if (host && *host) { WCHAR *w = from_utf8(host, slen(host)); addr_add(a, khost, w, (wlen(w) + 1) * 2, DPNA_DATATYPE_STRING); mem_free(w); }
    if (port) addr_add(a, kport, &port, 4, DPNA_DATATYPE_DWORD);
    return a;
}

/* ---- IDirectPlay8Peer ------------------------------------------------- */

#define MAX_PLAYERS 64

typedef struct { u32 id, flags; WCHAR *name; char *addr; } Player;

typedef struct Peer {
    void **vtbl;
    volatile i32 ref;
    CRITICAL_SECTION cs;      /* state below */
    CRITICAL_SECTION send_cs; /* the pipe's write side */
    int initialized;
    void *ctx;
    PFNDPNMESSAGEHANDLER handler;
    SOCKET pipe;
    HANDLE reader;
    u32 reader_tid;
    volatile int closing;
    /* session */
    int role; /* 0 none, 1 host, 2 joiner */
    u32 local_id, host_id;
    Player players[MAX_PLAYERS];
    int nplayers;
    WCHAR *my_name;
    DPN_APPLICATION_DESC desc;
    WCHAR *session_name, *password;
    u32 port;
    /* async operations */
    u32 next_handle;
    u32 enum_handle; void *enum_ctx;
    u32 connect_handle; void *connect_ctx; int connect_sync;
    HANDLE ev_reply; u32 reply_hr, reply_local;
    int waiting; /* 1 host, 2 connect */
} Peer;

static void **g_peer_vtbl;

static HRESULT indicate(Peer *p, u32 id, void *msg)
{
    if (!p->handler) return S_OK;
    return p->handler(p->ctx, id, msg);
}

static Player *find_player(Peer *p, u32 id)
{
    for (int i = 0; i < p->nplayers; i++) if (p->players[i].id == id) return &p->players[i];
    return NULL;
}

static void drop_player(Peer *p, u32 id)
{
    for (int i = 0; i < p->nplayers; i++)
        if (p->players[i].id == id) {
            mem_free(p->players[i].name); mem_free(p->players[i].addr);
            p->players[i] = p->players[--p->nplayers];
            return;
        }
}

/* The pipe ---------------------------------------------------------------- */

typedef struct { u8 *b; u32 n, cap; } Buf;
static void b_raw(Buf *o, const void *d, u32 n)
{
    if (o->n + n > o->cap) { u32 c = (o->cap + n) * 2 + 64; u8 *nb = mem_alloc(c); memcpy(nb, o->b, o->n); mem_free(o->b); o->b = nb; o->cap = c; }
    memcpy(o->b + o->n, d, n); o->n += n;
}
static void b_u8(Buf *o, u8 v) { b_raw(o, &v, 1); }
static void b_u16(Buf *o, u16 v) { b_raw(o, &v, 2); }
static void b_u32(Buf *o, u32 v) { b_raw(o, &v, 4); }
static void b_str(Buf *o, const char *s) { u32 n = slen(s); if (n > 65535) n = 65535; b_u16(o, (u16)n); b_raw(o, s, n); }
static void b_wstr(Buf *o, const WCHAR *w) { char *s = to_utf8(w, NULL); b_str(o, s); mem_free(s); }
static void b_bytes(Buf *o, const void *d, u32 n) { b_u32(o, n); b_raw(o, d, n); }
static void b_begin(Buf *o, u8 op) { o->b = NULL; o->n = o->cap = 0; b_u32(o, 0); b_u8(o, op); }

static int pipe_send(Peer *p, Buf *o)
{
    int ok = 1;
    *(u32 *)o->b = o->n - 4;
    EnterCriticalSection(&p->send_cs);
    for (u32 off = 0; off < o->n;) {
        i32 r = send(p->pipe, (const char *)o->b + off, (i32)(o->n - off), 0);
        if (r <= 0) { ok = 0; break; }
        off += (u32)r;
    }
    LeaveCriticalSection(&p->send_cs);
    mem_free(o->b);
    return ok;
}

typedef struct { const u8 *p; u32 n; int bad; } Rd;
static const u8 *r_take(Rd *r, u32 n) { if (r->bad || r->n < n) { r->bad = 1; return NULL; } const u8 *v = r->p; r->p += n; r->n -= n; return v; }
static u16 r_u16(Rd *r) { const u8 *v = r_take(r, 2); return v ? (u16)(v[0] | v[1] << 8) : 0; }
static u32 r_u32(Rd *r) { const u8 *v = r_take(r, 4); return v ? (u32)(v[0] | v[1] << 8 | v[2] << 16 | (u32)v[3] << 24) : 0; }
/* A string as a fresh NUL-terminated UTF-8 copy. */
static char *r_str(Rd *r) { u32 n = r_u16(r); const u8 *v = r_take(r, n); char *s = mem_alloc(n + 1); if (v) memcpy(s, v, n); return s; }
static const u8 *r_bytes(Rd *r, u32 *n) { *n = r_u32(r); return r_take(r, *n); }

static int read_full(SOCKET s, void *buf, u32 n)
{
    for (u32 off = 0; off < n;) {
        i32 r = recv(s, (char *)buf + off, (i32)(n - off), 0);
        if (r <= 0) return 0;
        off += (u32)r;
    }
    return 1;
}

/* Bridge notes ------------------------------------------------------------ */

static void note_player(Peer *p, Rd *r)
{
    u32 id = r_u32(r), flags = r_u32(r);
    char *name = r_str(r), *addr = r_str(r);
    if (r->bad) { mem_free(name); mem_free(addr); return; }
    EnterCriticalSection(&p->cs);
    Player *pl = find_player(p, id);
    if (!pl && p->nplayers < MAX_PLAYERS) { pl = &p->players[p->nplayers++]; memset(pl, 0, sizeof *pl); pl->id = id; }
    if (pl) { mem_free(pl->name); mem_free(pl->addr); pl->name = from_utf8(name, slen(name)); pl->addr = addr; addr = NULL; pl->flags = flags; }
    if (flags & DPNPLAYER_LOCAL) p->local_id = id;
    if (flags & DPNPLAYER_HOST) p->host_id = id;
    LeaveCriticalSection(&p->cs);
    logf("CREATE_PLAYER %x flags %x %s", id, flags, name);
    struct { u32 dwSize, dpnidPlayer; void *pvPlayerContext; } m = {12, id, NULL};
    indicate(p, DPN_MSGID_CREATE_PLAYER, &m);
    mem_free(name); mem_free(addr);
}

static void destroy_player(Peer *p, u32 id, u32 reason)
{
    if (!find_player(p, id)) return;
    logf("DESTROY_PLAYER %x reason %u", id, reason);
    struct { u32 dwSize, dpnidPlayer; void *pvPlayerContext; u32 dwReason; } m = {16, id, NULL, reason};
    indicate(p, DPN_MSGID_DESTROY_PLAYER, &m);
    EnterCriticalSection(&p->cs);
    drop_player(p, id);
    LeaveCriticalSection(&p->cs);
}

/* Every player gone, the local one last (DirectPlay's Close and the end of a
 * session both destroy each player). */
static void destroy_all(Peer *p, u32 reason)
{
    for (;;) {
        u32 id = 0;
        EnterCriticalSection(&p->cs);
        for (int i = 0; i < p->nplayers; i++) if (p->players[i].id != p->local_id) { id = p->players[i].id; break; }
        if (!id && p->nplayers) id = p->players[0].id;
        LeaveCriticalSection(&p->cs);
        if (!id) break;
        destroy_player(p, id, reason);
    }
}

static void note_receive(Peer *p, Rd *r)
{
    u32 from = r_u32(r), n;
    const u8 *d = r_bytes(r, &n);
    if (r->bad) return;
    u8 *copy = mem_alloc(n);
    memcpy(copy, d, n);
    struct { u32 dwSize, dpnidSender; void *pvPlayerContext; u8 *pReceiveData; u32 dwReceiveDataSize; void *hBufferHandle; u32 dwReceiveFlags; } m =
        {0x1c, from, NULL, copy, n, copy, 0};
    HRESULT hr = indicate(p, DPN_MSGID_RECEIVE, &m);
    /* PENDING: the game keeps the buffer until ReturnBuffer (RA2 does). */
    if (hr != DPNSUCCESS_PENDING) mem_free(copy);
}

static void note_enum_found(Peer *p, Rd *r)
{
    char *ip = r_str(r);
    u32 port = r_u16(r);
    DPN_APPLICATION_DESC d;
    memset(&d, 0, sizeof d);
    d.dwSize = sizeof d;
    d.dwFlags = r_u32(r);
    d.dwMaxPlayers = r_u32(r);
    d.dwCurrentPlayers = r_u32(r);
    const u8 *g = r_take(r, 16); if (g) memcpy(&d.guidInstance, g, 16);
    g = r_take(r, 16); if (g) memcpy(&d.guidApplication, g, 16);
    char *session = r_str(r);
    u32 n;
    const u8 *reply = r_bytes(r, &n);
    u32 rtt = r_u32(r);
    if (!r->bad && p->enum_handle) {
        WCHAR *wname = from_utf8(session, slen(session));
        d.pwszSessionName = wname;
        u8 *data = mem_alloc(n);
        memcpy(data, reply, n);
        Address *sender = addr_for(ip, port), *device = addr_for(NULL, 0);
        struct { u32 dwSize; Address *pAddressSender, *pAddressDevice; DPN_APPLICATION_DESC *pApplicationDescription; void *pvResponseData; u32 dwResponseDataSize; void *pvUserContext; u32 dwRoundTripLatencyMS; } m =
            {0x20, sender, device, &d, data, n, p->enum_ctx, rtt};
        logf("ENUM_HOSTS_RESPONSE %s:%u %s players %u/%u", ip, port, session, d.dwCurrentPlayers, d.dwMaxPlayers);
        indicate(p, DPN_MSGID_ENUM_HOSTS_RESPONSE, &m);
        A_Release(sender); A_Release(device);
        mem_free(data); mem_free(wname);
    }
    mem_free(ip); mem_free(session);
}

static void note_admit(Peer *p, Rd *r)
{
    u32 token = r_u32(r), n;
    char *name = r_str(r), *addr = r_str(r);
    const u8 *cd = r_bytes(r, &n);
    if (r->bad) { mem_free(name); mem_free(addr); return; }
    u8 *data = mem_alloc(n);
    memcpy(data, cd, n);
    /* The joiner's address as a TCP/IP address object. */
    char host[64]; u32 port = 0, i = 0;
    for (; addr[i] && addr[i] != ':' && i < sizeof host - 1; i++) host[i] = addr[i];
    host[i] = 0;
    if (addr[i] == ':') for (i++; addr[i] >= '0' && addr[i] <= '9'; i++) port = port * 10 + (u32)(addr[i] - '0');
    Address *pa = addr_for(host, port), *dev = addr_for(NULL, p->port);
    struct { u32 dwSize; void *pvUserConnectData; u32 dwUserConnectDataSize; void *pvReplyData; u32 dwReplyDataSize; void *pvReplyContext; void *pvPlayerContext; Address *pAddressPlayer, *pAddressDevice; } m =
        {0x24, data, n, NULL, 0, NULL, NULL, pa, dev};
    HRESULT hr = indicate(p, DPN_MSGID_INDICATE_CONNECT, &m);
    logf("INDICATE_CONNECT %s from %s -> %x (reply %u bytes)", name, addr, hr, m.dwReplyDataSize);
    Buf o; b_begin(&o, 0x08);
    b_u32(&o, token); b_u8(&o, hr == S_OK);
    b_bytes(&o, m.pvReplyData, m.pvReplyData ? m.dwReplyDataSize : 0);
    pipe_send(p, &o);
    if (m.pvReplyData) {
        struct { u32 dwSize; HRESULT hResultCode; void *pvBuffer, *pvUserContext; } rb = {16, S_OK, m.pvReplyData, m.pvReplyContext};
        indicate(p, DPN_MSGID_RETURN_BUFFER, &rb);
    }
    A_Release(pa); A_Release(dev);
    mem_free(data); mem_free(name); mem_free(addr);
}

static void note_enum_ask(Peer *p, Rd *r)
{
    u32 token = r_u32(r), n;
    char *addr = r_str(r);
    const u8 *ud = r_bytes(r, &n);
    if (r->bad) { mem_free(addr); return; }
    Address *sender = addr_for(addr, 0), *dev = addr_for(NULL, p->port);
    struct { u32 dwSize; Address *pAddressSender, *pAddressDevice; void *pvReceivedData; u32 dwReceivedDataSize, dwMaxResponseDataSize; void *pvResponseData; u32 dwResponseDataSize; void *pvResponseContext; } m =
        {0x24, sender, dev, n ? (void *)ud : NULL, n, 1000, NULL, 0, NULL};
    HRESULT hr = indicate(p, DPN_MSGID_ENUM_HOSTS_QUERY, &m);
    Buf o; b_begin(&o, 0x09);
    b_u32(&o, token); b_u8(&o, hr == S_OK);
    b_bytes(&o, m.pvResponseData, m.pvResponseData ? m.dwResponseDataSize : 0);
    pipe_send(p, &o);
    if (m.pvResponseData) {
        struct { u32 dwSize; HRESULT hResultCode; void *pvBuffer, *pvUserContext; } rb = {16, S_OK, m.pvResponseData, m.pvResponseContext};
        indicate(p, DPN_MSGID_RETURN_BUFFER, &rb);
    }
    A_Release(sender); A_Release(dev);
    mem_free(addr);
}

static void note_connected(Peer *p, Rd *r)
{
    u32 hr = r_u32(r), local = r_u32(r), host = r_u32(r), n;
    const u8 *reply = r_bytes(r, &n);
    if (r->bad) return;
    u8 *data = n ? mem_alloc(n) : NULL;
    if (n) memcpy(data, reply, n);
    if (hr == S_OK) { p->local_id = local; p->host_id = host; p->role = 2; }
    struct { u32 dwSize; u32 hAsyncOp; void *pvUserContext; HRESULT hResultCode; void *pvApplicationReplyData; u32 dwApplicationReplyDataSize; u32 dpnidLocal; } m =
        {0x1c, p->connect_sync ? 0 : p->connect_handle, p->connect_ctx, hr, data, n, local};
    logf("CONNECT_COMPLETE hr %x local %x host %x reply %u", hr, local, host, n);
    indicate(p, DPN_MSGID_CONNECT_COMPLETE, &m);
    mem_free(data);
    p->connect_handle = 0;
    if (p->waiting == 2) { p->reply_hr = hr; p->reply_local = local; p->waiting = 0; SetEvent(p->ev_reply); }
}

static void dispatch(Peer *p, const u8 *body, u32 n)
{
    Rd r = {body + 1, n - 1, 0};
    switch (body[0]) {
    case 0x81: { /* Hosted */
        u32 hr = r_u32(&r), local = r_u32(&r);
        if (p->waiting == 1) { p->reply_hr = hr; p->reply_local = local; p->waiting = 0; SetEvent(p->ev_reply); }
        break;
    }
    case 0x82: note_connected(p, &r); break;
    case 0x83: note_player(p, &r); break;
    case 0x84: { u32 id = r_u32(&r), reason = r_u32(&r); if (!r.bad) destroy_player(p, id, reason); break; }
    case 0x85: {
        u32 id = r_u32(&r); char *name = r_str(&r);
        EnterCriticalSection(&p->cs);
        Player *pl = find_player(p, id);
        if (pl) { mem_free(pl->name); pl->name = from_utf8(name, slen(name)); }
        LeaveCriticalSection(&p->cs);
        if (pl) { struct { u32 dwSize, dpnidPeer; void *pvPlayerContext; } m = {12, id, NULL}; indicate(p, DPN_MSGID_PEER_INFO, &m); }
        mem_free(name);
        break;
    }
    case 0x86: note_receive(p, &r); break;
    case 0x87: {
        u32 handle = r_u32(&r), hr = r_u32(&r);
        if (handle) {
            struct { u32 dwSize; u32 hAsyncOp; void *pvUserContext; HRESULT hResultCode; u32 dwSendTime, dwFirstFrameRTT, dwFirstFrameRetryCount; } m = {0x1c, handle, NULL, hr, 0, 0, 0};
            indicate(p, DPN_MSGID_SEND_COMPLETE, &m);
        }
        break;
    }
    case 0x88: { /* Terminated */
        u32 hr = r_u32(&r);
        logf("TERMINATE_SESSION %x", hr);
        struct { u32 dwSize; HRESULT hResultCode; void *pvTerminateData; u32 dwTerminateDataSize; } m = {16, hr, NULL, 0};
        indicate(p, DPN_MSGID_TERMINATE_SESSION, &m);
        destroy_all(p, DPNDESTROYPLAYERREASON_SESSIONTERMINATED);
        p->role = 0;
        break;
    }
    case 0x89: note_admit(p, &r); break;
    case 0x8a: note_enum_ask(p, &r); break;
    case 0x8b: note_enum_found(p, &r); break;
    default: logf("unknown note %x", body[0]);
    }
}

static WINAPI u32 reader_main(void *arg)
{
    Peer *p = arg;
    u32 len;
    while (!p->closing && read_full(p->pipe, &len, 4)) {
        if (len == 0 || len > (64u << 20)) break;
        u8 *body = mem_alloc(len);
        if (!read_full(p->pipe, body, len)) { mem_free(body); break; }
        dispatch(p, body, len);
        mem_free(body);
    }
    logf("pipe closed (closing=%u)", (u32)p->closing);
    if (p->waiting) { p->reply_hr = E_FAIL; p->waiting = 0; SetEvent(p->ev_reply); }
    return 0;
}

static int open_pipe(Peer *p)
{
    char spec[64];
    u32 n = GetEnvironmentVariableA("ORA_DPNET_BRIDGE", spec, sizeof spec);
    if (n == 0 || n >= sizeof spec) { memcpy(spec, "127.0.0.1:27160", 16); }
    char ip[64]; u32 i = 0, port = 0;
    for (; spec[i] && spec[i] != ':' && i < sizeof ip - 1; i++) ip[i] = spec[i];
    ip[i] = 0;
    if (spec[i] == ':') for (i++; spec[i] >= '0' && spec[i] <= '9'; i++) port = port * 10 + (u32)(spec[i] - '0');
    SOCKADDR_IN sa; memset(&sa, 0, sizeof sa);
    sa.family = 2; sa.port = htons((u16)port); sa.addr = inet_addr(ip);
    p->pipe = socket(2, 1, 6);
    if (p->pipe == INVALID_SOCKET) return 0;
    i32 one = 1;
    setsockopt(p->pipe, 6, 1 /* TCP_NODELAY */, &one, 4);
    if (connect(p->pipe, &sa, sizeof sa) != 0) { logf("bridge %s not reachable", spec); closesocket(p->pipe); p->pipe = INVALID_SOCKET; return 0; }
    logf("bridge %s connected", spec);
    p->closing = 0;
    p->reader = CreateThread(NULL, 0, reader_main, p, 0, &p->reader_tid);
    return 1;
}

static void close_pipe(Peer *p)
{
    if (p->pipe == INVALID_SOCKET) return;
    p->closing = 1;
    shutdown(p->pipe, 2);
    if (p->reader && GetCurrentThreadId() != p->reader_tid) WaitForSingleObject(p->reader, 5000);
    if (p->reader) CloseHandle(p->reader);
    p->reader = 0;
    closesocket(p->pipe);
    p->pipe = INVALID_SOCKET;
}

/* Wait for the bridge's answer to a Host or a synchronous Connect. */
static HRESULT wait_reply(Peer *p, u32 ms)
{
    if (WaitForSingleObject(p->ev_reply, ms) != 0) { p->waiting = 0; return E_FAIL; }
    return p->reply_hr;
}

/* Methods -------------------------------------------------------------------- */

static WINAPI HRESULT P_QueryInterface(Peer *p, const GUID *riid, void **ppv)
{
    if (!ppv) return E_POINTER;
    if (guid_eq(riid, &IID_IUnknown) || guid_eq(riid, &IID_IDirectPlay8Peer)) { InterlockedIncrement(&p->ref); *ppv = p; return S_OK; }
    *ppv = NULL;
    return E_NOINTERFACE;
}
static WINAPI u32 P_AddRef(Peer *p) { return InterlockedIncrement(&p->ref); }
static WINAPI HRESULT P_Close(Peer *p, u32 flags);
static WINAPI u32 P_Release(Peer *p)
{
    i32 r = InterlockedDecrement(&p->ref);
    if (r == 0) {
        P_Close(p, 0);
        CloseHandle(p->ev_reply);
        DeleteCriticalSection(&p->cs); DeleteCriticalSection(&p->send_cs);
        mem_free(p->my_name);
        mem_free(p);
    }
    return r;
}

static WINAPI HRESULT P_Initialize(Peer *p, void *ctx, PFNDPNMESSAGEHANDLER handler, u32 flags)
{
    logf("Initialize ctx %p handler %p flags %x", (u32)ctx, (u32)handler, flags);
    if (!handler) return E_INVALIDARG;
    if (p->initialized) return DPNERR_ALREADYINITIALIZED;
    p->ctx = ctx; p->handler = handler;
    if (!open_pipe(p)) return E_FAIL;
    p->initialized = 1;
    return S_OK;
}

static WINAPI HRESULT P_EnumServiceProviders(Peer *p, const GUID *sp, const GUID *app, DPN_SERVICE_PROVIDER_INFO *buf, u32 *size, u32 *count, u32 flags)
{
    static const WCHAR tcpip[] = {'D','i','r','e','c','t','P','l','a','y','8',' ','T','C','P','/','I','P',' ','S','e','r','v','i','c','e',' ','P','r','o','v','i','d','e','r',0};
    static const WCHAR adapter[] = {'L','o','c','a','l',' ','A','r','e','a',' ','C','o','n','n','e','c','t','i','o','n',' ','-',' ','I','P','v','4',0};
    (void)app; (void)flags;
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    if (!size || !count) return E_POINTER;
    const WCHAR *name; const GUID *guid;
    if (!sp) { name = tcpip; guid = &CLSID_DP8SP_TCPIP; }
    else if (guid_eq(sp, &CLSID_DP8SP_TCPIP)) { name = adapter; guid = &GUID_ORA_ADAPTER; }
    else return DPNERR_DOESNOTEXIST;
    u32 need = sizeof(DPN_SERVICE_PROVIDER_INFO) + (wlen(name) + 1) * 2;
    logf("EnumServiceProviders sp %x size %u", sp ? sp->d1 : 0, *size);
    if (!buf || *size < need) { *size = need; *count = 0; return DPNERR_BUFFERTOOSMALL; }
    memset(buf, 0, sizeof *buf);
    buf->guid = *guid;
    buf->pwszName = (WCHAR *)(buf + 1);
    memcpy(buf->pwszName, name, (wlen(name) + 1) * 2);
    *size = need; *count = 1;
    return S_OK;
}

static WINAPI HRESULT P_CancelAsyncOperation(Peer *p, u32 handle, u32 flags)
{
    logf("CancelAsyncOperation %x flags %x", handle, flags);
    if (p->enum_handle && (handle == p->enum_handle || (!handle && (flags & (0x2 | 0x8000))))) {
        u32 h = p->enum_handle;
        Buf o; b_begin(&o, 0x04); pipe_send(p, &o);
        p->enum_handle = 0;
        struct { u32 dwSize; u32 hAsyncOp; void *pvUserContext; HRESULT hResultCode; } m = {16, h, p->enum_ctx, DPNERR_USERCANCEL};
        indicate(p, DPN_MSGID_ASYNC_OP_COMPLETE, &m);
    }
    return S_OK;
}

static WINAPI HRESULT P_Connect(Peer *p, const DPN_APPLICATION_DESC *desc, Address *host, Address *device,
                               void *sec, void *cred, const void *cdata, u32 cdsize, void *pctx, void *actx, u32 *handle, u32 flags)
{
    (void)desc; (void)sec; (void)cred; (void)pctx;
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    if (!host || !device) return E_POINTER;
    char *hostname = addr_get_str(host, "hostname");
    u32 port = addr_get_dword(host, "port", 0), local_port = addr_get_dword(device, "port", 0);
    logf("Connect %s:%u (device port %u) flags %x cdata %u", hostname ? hostname : "", port, local_port, flags, cdsize);
    char *pw = desc && desc->pwszPassword ? to_utf8(desc->pwszPassword, NULL) : NULL;
    p->connect_sync = (flags & DPNCONNECT_SYNC) != 0;
    p->connect_ctx = actx;
    p->connect_handle = p->connect_sync ? 0 : ++p->next_handle;
    if (handle) *handle = p->connect_handle;
    p->port = local_port;
    Buf o; b_begin(&o, 0x02);
    b_str(&o, hostname ? hostname : ""); b_u16(&o, (u16)port); b_u16(&o, (u16)local_port);
    b_wstr(&o, p->my_name); b_str(&o, pw ? pw : "");
    b_bytes(&o, cdata, cdata ? cdsize : 0);
    mem_free(hostname); mem_free(pw);
    if (p->connect_sync) { ResetEvent(p->ev_reply); p->waiting = 2; }
    if (!pipe_send(p, &o)) { p->waiting = 0; return E_FAIL; }
    if (!p->connect_sync) return DPNSUCCESS_PENDING;
    return wait_reply(p, 30000);
}

static WINAPI HRESULT P_SendTo(Peer *p, u32 to, const DPN_BUFFER_DESC *bufs, u32 nbufs, u32 timeout, void *actx, u32 *handle, u32 flags)
{
    (void)timeout; (void)actx;
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    if (!p->role) return DPNERR_NOCONNECTION;
    u32 h = (flags & (DPNSEND_SYNC | DPNSEND_NOCOMPLETE)) ? 0 : ++p->next_handle;
    if (handle) *handle = h;
    u32 total = 0;
    for (u32 i = 0; i < nbufs; i++) total += bufs[i].dwBufferSize;
    Buf o; b_begin(&o, 0x05);
    b_u32(&o, h); b_u32(&o, to); b_u32(&o, flags); b_u32(&o, total);
    for (u32 i = 0; i < nbufs; i++) b_raw(&o, bufs[i].pBufferData, bufs[i].dwBufferSize);
    if (!pipe_send(p, &o)) return DPNERR_CONNECTIONLOST;
    return h ? DPNSUCCESS_PENDING : S_OK;
}

static WINAPI HRESULT P_GetSendQueueInfo(Peer *p, u32 id, u32 *msgs, u32 *bytes, u32 flags)
{
    (void)p; (void)id; (void)flags;
    if (msgs) *msgs = 0;
    if (bytes) *bytes = 0;
    return S_OK;
}

static WINAPI HRESULT P_Host(Peer *p, const DPN_APPLICATION_DESC *desc, Address **devices, u32 ndevices, void *sec, void *cred, void *pctx, u32 flags)
{
    (void)sec; (void)cred; (void)pctx;
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    if (!desc) return E_POINTER;
    u32 port = ndevices && devices && devices[0] ? addr_get_dword(devices[0], "port", 0) : 0;
    p->desc = *desc;
    mem_free(p->session_name); mem_free(p->password);
    p->session_name = wdup(desc->pwszSessionName);
    p->password = (desc->dwFlags & 0x80) && desc->pwszPassword ? wdup(desc->pwszPassword) : NULL;
    p->desc.pwszSessionName = p->session_name;
    p->desc.pwszPassword = p->password;
    p->desc.pvReservedData = p->desc.pvApplicationReservedData = NULL;
    p->desc.dwReservedDataSize = p->desc.dwApplicationReservedDataSize = 0;
    /* A fresh instance GUID. */
    u32 t = GetTickCount();
    p->desc.guidInstance.d1 = t ^ 0x4f524121; p->desc.guidInstance.d2 = (u16)(t >> 7); p->desc.guidInstance.d3 = 0x4000 | (u16)(t & 0xfff);
    p->port = port;
    logf("Host %S port %u max %u flags %x", desc->pwszSessionName, port, desc->dwMaxPlayers, flags);
    Buf o; b_begin(&o, 0x01);
    b_u16(&o, (u16)port); b_u32(&o, desc->dwMaxPlayers ? desc->dwMaxPlayers : 64);
    b_wstr(&o, desc->pwszSessionName); b_wstr(&o, p->password); b_wstr(&o, p->my_name);
    ResetEvent(p->ev_reply); p->waiting = 1;
    if (!pipe_send(p, &o)) { p->waiting = 0; return E_FAIL; }
    HRESULT hr = wait_reply(p, 10000);
    if (hr == S_OK) { p->role = 1; p->local_id = p->host_id = p->reply_local; }
    logf("Host -> %x local %x", hr, p->local_id);
    return hr;
}

static WINAPI HRESULT P_GetApplicationDesc(Peer *p, DPN_APPLICATION_DESC *buf, u32 *size, u32 flags)
{
    (void)flags;
    if (!size) return E_POINTER;
    if (!p->role) return DPNERR_NOCONNECTION;
    u32 nl = (wlen(p->session_name) + 1) * 2, pl = p->password ? (wlen(p->password) + 1) * 2 : 0;
    u32 need = sizeof *buf + nl + pl;
    if (!buf || *size < need) { *size = need; return DPNERR_BUFFERTOOSMALL; }
    *buf = p->desc;
    buf->dwCurrentPlayers = (u32)p->nplayers;
    buf->pwszSessionName = (WCHAR *)(buf + 1);
    memcpy(buf->pwszSessionName, p->session_name ? p->session_name : (const WCHAR *)L"", nl);
    buf->pwszPassword = NULL;
    if (pl) { buf->pwszPassword = (WCHAR *)((u8 *)(buf + 1) + nl); memcpy(buf->pwszPassword, p->password, pl); }
    *size = need;
    return S_OK;
}

static WINAPI HRESULT P_SetApplicationDesc(Peer *p, const DPN_APPLICATION_DESC *d, u32 flags)
{
    (void)flags;
    if (!d) return E_POINTER;
    p->desc.dwMaxPlayers = d->dwMaxPlayers;
    return S_OK;
}

static WINAPI HRESULT P_Unsupported(Peer *p) { (void)p; logf("unsupported peer method"); return DPNERR_UNSUPPORTED; }

static WINAPI HRESULT P_SetPeerInfo(Peer *p, const DPN_PLAYER_INFO *info, void *actx, u32 *handle, u32 flags)
{
    (void)actx; (void)flags;
    if (!info) return E_POINTER;
    if (handle) *handle = 0;
    if (info->dwInfoFlags & DPNINFO_NAME) {
        mem_free(p->my_name);
        p->my_name = wdup(info->pwszName);
        logf("SetPeerInfo %S", p->my_name);
        if (p->role) {
            EnterCriticalSection(&p->cs);
            Player *me = find_player(p, p->local_id);
            if (me) { mem_free(me->name); me->name = wdup(p->my_name); }
            LeaveCriticalSection(&p->cs);
            Buf o; b_begin(&o, 0x06); b_wstr(&o, p->my_name); pipe_send(p, &o);
        }
    }
    return S_OK;
}

static WINAPI HRESULT P_GetPeerInfo(Peer *p, u32 id, DPN_PLAYER_INFO *buf, u32 *size, u32 flags)
{
    (void)flags;
    if (!size) return E_POINTER;
    EnterCriticalSection(&p->cs);
    Player *pl = find_player(p, id);
    if (!pl) { LeaveCriticalSection(&p->cs); return DPNERR_INVALIDPLAYER; }
    u32 nl = (wlen(pl->name) + 1) * 2, need = sizeof *buf + nl;
    if (!buf || *size < need) { LeaveCriticalSection(&p->cs); *size = need; return DPNERR_BUFFERTOOSMALL; }
    buf->dwSize = sizeof *buf;
    buf->dwInfoFlags = DPNINFO_NAME | DPNINFO_DATA;
    buf->pwszName = (WCHAR *)(buf + 1);
    memcpy(buf->pwszName, pl->name ? pl->name : (const WCHAR *)L"", nl);
    buf->pvData = NULL; buf->dwDataSize = 0;
    buf->dwPlayerFlags = pl->flags;
    LeaveCriticalSection(&p->cs);
    *size = need;
    return S_OK;
}

static WINAPI HRESULT P_GetPeerAddress(Peer *p, u32 id, Address **out, u32 flags)
{
    (void)flags;
    if (!out) return E_POINTER;
    EnterCriticalSection(&p->cs);
    Player *pl = find_player(p, id);
    char host[64] = "127.0.0.1"; u32 port = p->port;
    if (pl && pl->addr && *pl->addr) {
        u32 i = 0; for (; pl->addr[i] && pl->addr[i] != ':' && i < 63; i++) host[i] = pl->addr[i];
        host[i] = 0;
    }
    LeaveCriticalSection(&p->cs);
    if (!pl) return DPNERR_INVALIDPLAYER;
    *out = addr_for(host, port);
    return S_OK;
}

static WINAPI HRESULT P_GetLocalHostAddresses(Peer *p, Address **arr, u32 *count, u32 flags)
{
    (void)flags;
    if (!count) return E_POINTER;
    if (!p->role) return DPNERR_NOCONNECTION;
    if (!arr || *count < 1) { *count = 1; return DPNERR_BUFFERTOOSMALL; }
    arr[0] = addr_for("127.0.0.1", p->port);
    *count = 1;
    return S_OK;
}

static WINAPI HRESULT P_Close(Peer *p, u32 flags)
{
    logf("Close flags %x role %u", flags, (u32)p->role);
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    if (p->enum_handle) P_CancelAsyncOperation(p, p->enum_handle, 0);
    if (p->pipe != INVALID_SOCKET) { Buf o; b_begin(&o, 0x07); pipe_send(p, &o); }
    close_pipe(p);
    destroy_all(p, DPNDESTROYPLAYERREASON_NORMAL);
    p->role = 0; p->local_id = p->host_id = 0;
    p->initialized = 0; p->handler = NULL; p->ctx = NULL;
    return S_OK;
}

static WINAPI HRESULT P_EnumHosts(Peer *p, const DPN_APPLICATION_DESC *desc, Address *host, Address *device, const void *ud, u32 udsize,
                                 u32 count, u32 interval, u32 timeout, void *actx, u32 *handle, u32 flags)
{
    (void)desc; (void)device; (void)ud; (void)udsize; (void)count; (void)interval; (void)timeout;
    if (!p->initialized) return DPNERR_UNINITIALIZED;
    char *hostname = host ? addr_get_str(host, "hostname") : NULL;
    u32 port = host ? addr_get_dword(host, "port", 0) : 0;
    logf("EnumHosts %s:%u count %x flags %x", hostname ? hostname : "(broadcast)", port, count, flags);
    if (p->enum_handle) P_CancelAsyncOperation(p, p->enum_handle, 0);
    p->enum_ctx = actx;
    p->enum_handle = ++p->next_handle;
    if (handle) *handle = p->enum_handle;
    Buf o; b_begin(&o, 0x03);
    b_str(&o, hostname ? hostname : ""); b_u16(&o, (u16)port);
    mem_free(hostname);
    if (!pipe_send(p, &o)) return E_FAIL;
    return DPNSUCCESS_PENDING;
}

static WINAPI HRESULT P_DestroyPeer(Peer *p, u32 id, const void *d, u32 n, u32 flags) { (void)p; (void)id; (void)d; (void)n; (void)flags; return DPNERR_UNSUPPORTED; }

static WINAPI HRESULT P_ReturnBuffer(Peer *p, void *handle, u32 flags)
{
    (void)p; (void)flags;
    mem_free(handle);
    return S_OK;
}

static WINAPI HRESULT P_GetPlayerContext(Peer *p, u32 id, void **ctx, u32 flags)
{
    (void)flags;
    if (!ctx) return E_POINTER;
    *ctx = NULL;
    return find_player(p, id) ? S_OK : DPNERR_INVALIDPLAYER;
}

static WINAPI HRESULT P_GetCaps(Peer *p, u32 *caps, u32 flags)
{
    (void)p; (void)flags;
    if (!caps) return E_POINTER;
    /* DPN_CAPS: dwSize, dwFlags, connect timeout, connect retries, timeout until keep-alive. */
    if (caps[0] >= 20) { caps[1] = 0; caps[2] = 200; caps[3] = 14; caps[4] = 60000; }
    return S_OK;
}
static WINAPI HRESULT P_SetCaps(Peer *p, const void *caps, u32 flags) { (void)p; (void)caps; (void)flags; return S_OK; }
static WINAPI HRESULT P_SetSPCaps(Peer *p, const GUID *sp, const void *caps, u32 flags) { (void)p; (void)sp; (void)caps; (void)flags; return S_OK; }
static WINAPI HRESULT P_GetSPCaps(Peer *p, const GUID *sp, u32 *caps, u32 flags)
{
    (void)p; (void)sp; (void)flags;
    if (!caps) return E_POINTER;
    u32 n = caps[0];
    if (n > 4) memset(caps + 1, 0, n - 4);
    if (n >= 0x24) { caps[2] = 3; caps[3] = 1500; caps[4] = 0; caps[5] = 1472; caps[6] = 8; caps[7] = 4; caps[8] = 1; }
    return S_OK;
}
static WINAPI HRESULT P_GetConnectionInfo(Peer *p, u32 id, u32 *info, u32 flags)
{
    (void)p; (void)id; (void)flags;
    if (!info) return E_POINTER;
    u32 n = info[0];
    if (n > 4) memset(info + 1, 0, n - 4);
    return S_OK;
}
static WINAPI HRESULT P_RegisterLobby(Peer *p, u32 h, void *l, u32 flags) { (void)p; (void)h; (void)l; (void)flags; return S_OK; }
static WINAPI HRESULT P_TerminateSession(Peer *p, const void *d, u32 n, u32 flags)
{
    (void)d; (void)n; (void)flags;
    logf("TerminateSession");
    if (p->role != 1) return DPNERR_UNSUPPORTED;
    Buf o; b_begin(&o, 0x07); pipe_send(p, &o);
    destroy_all(p, DPNDESTROYPLAYERREASON_SESSIONTERMINATED);
    p->role = 0;
    return S_OK;
}

/* The groups the game never uses. */
static WINAPI HRESULT P_CreateGroup(Peer *p, const void *a, void *b, void *c, u32 *d, u32 e) { (void)a; (void)b; (void)c; (void)d; (void)e; return P_Unsupported(p); }
static WINAPI HRESULT P_DestroyGroup(Peer *p, u32 a, void *b, u32 *c, u32 d) { (void)a; (void)b; (void)c; (void)d; return P_Unsupported(p); }
static WINAPI HRESULT P_AddPlayerToGroup(Peer *p, u32 a, u32 b, void *c, u32 *d, u32 e) { (void)a; (void)b; (void)c; (void)d; (void)e; return P_Unsupported(p); }
static WINAPI HRESULT P_RemovePlayerFromGroup(Peer *p, u32 a, u32 b, void *c, u32 *d, u32 e) { (void)a; (void)b; (void)c; (void)d; (void)e; return P_Unsupported(p); }
static WINAPI HRESULT P_SetGroupInfo(Peer *p, u32 a, void *b, void *c, u32 *d, u32 e) { (void)a; (void)b; (void)c; (void)d; (void)e; return P_Unsupported(p); }
static WINAPI HRESULT P_GetGroupInfo(Peer *p, u32 a, void *b, u32 *c, u32 d) { (void)a; (void)b; (void)c; (void)d; return P_Unsupported(p); }
static WINAPI HRESULT P_EnumPlayersAndGroups(Peer *p, u32 *ids, u32 *count, u32 flags)
{
    if (!count) return E_POINTER;
    if (!(flags & 1 /* DPNENUM_PLAYERS */)) { *count = 0; return S_OK; }
    EnterCriticalSection(&p->cs);
    u32 n = (u32)p->nplayers;
    if (!ids || *count < n) { *count = n; LeaveCriticalSection(&p->cs); return DPNERR_BUFFERTOOSMALL; }
    for (u32 i = 0; i < n; i++) ids[i] = p->players[i].id;
    *count = n;
    LeaveCriticalSection(&p->cs);
    return S_OK;
}
static WINAPI HRESULT P_EnumGroupMembers(Peer *p, u32 a, u32 *b, u32 *c, u32 d) { (void)a; (void)b; (void)c; (void)d; return P_Unsupported(p); }
static WINAPI HRESULT P_GetGroupContext(Peer *p, u32 a, void **b, u32 c) { (void)a; (void)b; (void)c; return P_Unsupported(p); }

static void *peer_vtbl_[] = {
    P_QueryInterface, P_AddRef, P_Release, P_Initialize, P_EnumServiceProviders, P_CancelAsyncOperation,
    P_Connect, P_SendTo, P_GetSendQueueInfo, P_Host, P_GetApplicationDesc, P_SetApplicationDesc,
    P_CreateGroup, P_DestroyGroup, P_AddPlayerToGroup, P_RemovePlayerFromGroup, P_SetGroupInfo,
    P_GetGroupInfo, P_EnumPlayersAndGroups, P_EnumGroupMembers, P_SetPeerInfo, P_GetPeerInfo,
    P_GetPeerAddress, P_GetLocalHostAddresses, P_Close, P_EnumHosts, P_DestroyPeer, P_ReturnBuffer,
    P_GetPlayerContext, P_GetGroupContext, P_GetCaps, P_SetCaps, P_SetSPCaps, P_GetSPCaps,
    P_GetConnectionInfo, P_RegisterLobby, P_TerminateSession,
};

static Peer *peer_new(void)
{
    static const WCHAR dflt[] = {'P','l','a','y','e','r',0};
    Peer *p = mem_alloc(sizeof *p);
    p->vtbl = g_peer_vtbl; p->ref = 1;
    p->pipe = INVALID_SOCKET;
    InitializeCriticalSection(&p->cs);
    InitializeCriticalSection(&p->send_cs);
    p->ev_reply = CreateEventA(NULL, 0, 0, NULL);
    p->my_name = wdup(dflt);
    return p;
}

/* ---- class factory and exports --------------------------------------- */

typedef struct { void **vtbl; const GUID *clsid; } Factory;

static WINAPI HRESULT F_QueryInterface(Factory *f, const GUID *riid, void **ppv)
{
    if (!ppv) return E_POINTER;
    if (guid_eq(riid, &IID_IUnknown) || guid_eq(riid, &IID_IClassFactory)) { *ppv = f; return S_OK; }
    *ppv = NULL;
    return E_NOINTERFACE;
}
static WINAPI u32 F_AddRef(Factory *f) { (void)f; return 2; }
static WINAPI u32 F_Release(Factory *f) { (void)f; return 1; }
static WINAPI HRESULT F_CreateInstance(Factory *f, void *outer, const GUID *riid, void **ppv)
{
    if (!ppv) return E_POINTER;
    *ppv = NULL;
    if (outer) return CLASS_E_NOAGGREGATION;
    HRESULT hr;
    if (f->clsid == &CLSID_DirectPlay8Peer) {
        Peer *p = peer_new();
        hr = P_QueryInterface(p, riid, ppv);
        P_Release(p);
        logf("CreateInstance(Peer) -> %x %p", hr, (u32)*ppv);
    } else {
        Address *a = addr_new();
        hr = A_QueryInterface(a, riid, ppv);
        A_Release(a);
    }
    return hr;
}
static WINAPI HRESULT F_LockServer(Factory *f, i32 lock) { (void)f; (void)lock; return S_OK; }

static void *factory_vtbl[] = {F_QueryInterface, F_AddRef, F_Release, F_CreateInstance, F_LockServer};
static Factory g_peer_factory = {factory_vtbl, &CLSID_DirectPlay8Peer};
static Factory g_addr_factory = {factory_vtbl, &CLSID_DirectPlay8Address};

WINAPI HRESULT DllGetClassObject(const GUID *clsid, const GUID *riid, void **ppv)
{
    if (!ppv) return E_POINTER;
    *ppv = NULL;
    Factory *f = guid_eq(clsid, &CLSID_DirectPlay8Peer) ? &g_peer_factory
               : guid_eq(clsid, &CLSID_DirectPlay8Address) ? &g_addr_factory : NULL;
    if (!f) return CLASS_E_CLASSNOTAVAILABLE;
    return F_QueryInterface(f, riid, ppv);
}

WINAPI HRESULT DllCanUnloadNow(void) { return S_FALSE; }
WINAPI HRESULT DllRegisterServer(void) { return S_OK; }
WINAPI HRESULT DllUnregisterServer(void) { return S_OK; }

/* DirectPlay8Create (DirectX 8's export; COM is what RA2 uses). */
WINAPI HRESULT DirectPlay8Create(const GUID *iid, void **ppv, void *outer)
{
    if (outer) return CLASS_E_NOAGGREGATION;
    if (guid_eq(iid, &IID_IDirectPlay8Peer)) return F_CreateInstance(&g_peer_factory, NULL, iid, ppv);
    if (guid_eq(iid, &IID_IDirectPlay8Address)) return F_CreateInstance(&g_addr_factory, NULL, iid, ppv);
    return E_NOINTERFACE;
}

WINAPI i32 DllMain(void *inst, u32 reason, void *reserved)
{
    (void)inst; (void)reserved;
    if (reason == 1 /* DLL_PROCESS_ATTACH */) {
        u8 wsa[512];
        g_peer_vtbl = peer_vtbl_;
        g_addr_vtbl = addr_vtbl_;
        log_init();
        WSAStartup(0x0202, wsa);
        logf("ora_dpnet loaded");
    }
    return 1;
}
