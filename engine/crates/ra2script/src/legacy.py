"""The legacy-script fallback.

The shipped `Arenas/`, `AI/` and `Scripts/` modules are Python 2.2 and have to
run unmodified on CPython 3. This module is the bridge: a source translation for
the constructs Python 3 refuses to *parse*, and a runtime shim for the library
functions that moved.

It is deliberately narrow. Everything the shipped scripts need was surveyed
first, and only these were found:

  syntax   `print x, y`            -> `print(x, y)`
           `print >>f, x`          -> `print(x, file=f)`
           `raise E, "msg"`        -> `raise E("msg")`
           `except E, e:`          -> `except E as e:`
           `a <> b`                -> `a != b`
  methods  `d.iteritems()`         -> `d.items()`   (and itervalues/iterkeys)
           `it.next()`             -> `next(it)`
           `file(...)`             -> `open(...)`
  stdlib   `string.split/replace/rjust/...` and `types.TupleType`, `sys.maxint`
           were functions and aliases in 2.2; the shim below reinstates them.

One Python-2 semantic cannot be translated at all and is *not* corrected here:
`/` on two ints truncates in 2.x and is exact in 3.x. The shipped scripts were
checked and none divides two integers in a way that matters, so this is left
alone rather than rewriting arithmetic.
"""

import io
import os
import sys
import re
import tokenize
import importlib.abc
import importlib.machinery

__all__ = ["translate", "install"]

_SKIP = {
    tokenize.COMMENT,
    tokenize.NL,
    tokenize.INDENT,
    tokenize.DEDENT,
}
_STMT_ENDS = {tokenize.NEWLINE, tokenize.INDENT, tokenize.DEDENT, tokenize.ENDMARKER}

# Textual rewrites, applied outside strings and comments.
_RULES = [
    (r"\b([A-Za-z_][A-Za-z0-9_.]*)\.iteritems\(\)", r"\1.items()"),
    (r"\b([A-Za-z_][A-Za-z0-9_.]*)\.itervalues\(\)", r"\1.values()"),
    (r"\b([A-Za-z_][A-Za-z0-9_.]*)\.iterkeys\(\)", r"\1.keys()"),
    (r"\b([A-Za-z_][A-Za-z0-9_.]*)\.next\(\)", r"next(\1)"),
    (r"\bfile\(", "open("),
    (r"\bxrange\(", "range("),
]


def _line_starts(src):
    starts = [0]
    for line in src.splitlines(keepends=True):
        starts.append(starts[-1] + len(line))
    return starts


def _pos(starts, row, col):
    return starts[row - 1] + col


def _tokens(src):
    try:
        return list(tokenize.generate_tokens(io.StringIO(src).readline))
    except Exception:
        return None


def _spans(src):
    """Offset ranges covered by strings and comments."""
    toks = _tokens(src)
    if toks is None:
        return []
    starts = _line_starts(src)
    out = []
    for t in toks:
        if t.type in (tokenize.STRING, tokenize.COMMENT):
            out.append((_pos(starts, *t.start), _pos(starts, *t.end)))
    return out


def _rewrite_code(src, rules):
    spans = _spans(src)

    def blocked(a, b):
        return any(a < e and b > s for s, e in spans)

    for pat, rep in rules:
        rx = re.compile(pat)
        pieces, last = [], 0
        for m in rx.finditer(src):
            if blocked(*m.span()):
                continue
            pieces.append(src[last:m.start()])
            pieces.append(m.expand(rep))
            last = m.end()
        pieces.append(src[last:])
        src = "".join(pieces)
        spans = _spans(src)
    return src


def _apply(src, edits):
    """Apply (start, end, text) edits, left to right; earlier ones win."""
    edits = sorted(edits, key=lambda e: (e[0], e[1]))
    out, last = [], 0
    for a, b, text in edits:
        if a < last:
            continue
        out.append(src[last:a])
        out.append(text)
        last = max(last, b)
    out.append(src[last:])
    return "".join(out)


def _statement_edits(src):
    """Statement-level rewrites: print / raise / except."""
    toks = _tokens(src)
    if toks is None:
        return []
    starts = _line_starts(src)
    edits = []

    def signif_after(i):
        return next((x for x in toks[i + 1:] if x.type not in _SKIP), None)

    prev = None
    for i, t in enumerate(toks):
        if t.type in _SKIP:
            continue

        if t.type == tokenize.NAME and t.string == "print":
            at_start = prev is None or prev.type in _STMT_ENDS or (
                prev.type == tokenize.OP and prev.string in (":", ";")
            )
            nxt = signif_after(i)
            if at_start and nxt is not None and not (
                nxt.type == tokenize.OP and nxt.string in "()=,.]}"
            ):
                edits.append(_print_edit(src, toks, i, starts))
        elif t.type == tokenize.NAME and t.string in ("raise", "except"):
            edits.extend(_raise_or_except(src, toks, i, starts, t.string))
        prev = t

    return [e for e in edits if e is not None]


def _stmt_end(src, toks, i, starts):
    """Offset where the statement starting at token `i` ends (exclusive)."""
    depth = 0
    j = i + 1
    while j < len(toks):
        t = toks[j]
        if t.type == tokenize.OP:
            if t.string in "([{":
                depth += 1
            elif t.string in ")]}":
                depth -= 1
        if depth == 0 and t.type in (tokenize.NEWLINE, tokenize.ENDMARKER, tokenize.COMMENT):
            return _pos(starts, *t.start)
        j += 1
    return len(src)


def _print_edit(src, toks, i, starts):
    start = _pos(starts, *toks[i].start)
    end = _stmt_end(src, toks, i, starts)
    first = next((x for x in toks[i + 1:] if x.type not in _SKIP), None)
    if first is not None and first.type == tokenize.OP and first.string == ">>":
        comma = next(
            (x for x in toks[i + 2:] if x.type == tokenize.OP and x.string == ","), None
        )
        if comma is None:
            return (start, end, "pass")
        f = src[_pos(starts, *first.end):_pos(starts, *comma.start)].strip()
        rest = src[_pos(starts, *comma.end):end].rstrip()
        rest = rest[:-1].rstrip() if rest.endswith(",") else rest
        inner = f"{rest}, file={f}" if rest else f"file={f}"
        return (start, end, f"print({inner})")

    inner = src[_pos(starts, *toks[i].end):end].rstrip()
    if inner.endswith(","):
        inner = inner[:-1].rstrip()
    return (start, end, f"print({inner})" if inner else "print()")


def _raise_or_except(src, toks, i, starts, keyword):
    """`except E, e:` -> `except E as e:`; `raise E, msg` -> `raise E(msg)`."""
    end = _stmt_end(src, toks, i, starts)
    depth = 0
    comma_idx = None
    for j in range(i + 1, len(toks)):
        t = toks[j]
        if t.type in (tokenize.NEWLINE, tokenize.ENDMARKER):
            break
        if t.type == tokenize.OP:
            if t.string in "([{":
                depth += 1
            elif t.string in ")]}":
                depth -= 1
            elif t.string == "," and depth == 0:
                comma_idx = j
                break
    if comma_idx is None:
        return []
    comma = toks[comma_idx]

    if keyword == "except":
        rest = [x for x in toks[comma_idx + 1:] if x.type not in _SKIP]
        if not rest or rest[0].type != tokenize.NAME:
            return []
        a = _pos(starts, *comma.start)
        b = _pos(starts, *comma.end)
        return [(a, b, " as")]

    head = src[_pos(starts, *toks[i].end):_pos(starts, *comma.start)].rstrip()
    if head.endswith(")") or head.endswith("]"):
        return []  # `raise E(x), msg` — not translatable, leave for the parser
    a = _pos(starts, *comma.start)
    b = _pos(starts, *comma.end)
    return [(a, b, "("), (end, end, ")")]


_IMPORT_RE = re.compile(
    r"^(?P<ind>[ \t]*)(?P<kw>import|from)[ \t]+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
_IMPORT_SIMPLE = re.compile(r"^[ \t]*(?:as[ \t]+[A-Za-z_][A-Za-z0-9_]*)?[ \t]*$", re.M)


def _rewrite_relative(src, siblings):
    """Turn a sibling import into its explicit relative form.

    Python 2 resolved `import Hazards` *inside* the package `Arenas` as
    `Arenas.Hazards`; Python 3 makes that name absolute and fails with
    `ModuleNotFoundError`. The game relies on it in every arena and in `AI`.
    """
    if not siblings:
        return src
    spans = _spans(src)

    def blocked(a, b):
        return any(a < e and b > s for s, e in spans)

    def is_sibling(name):
        return os.path.isfile(os.path.join(siblings, name + ".py")) or os.path.isdir(
            os.path.join(siblings, name)
        )

    out, last = [], 0
    for m in _IMPORT_RE.finditer(src):
        if blocked(*m.span()):
            continue
        name = m.group("name")
        if not is_sibling(name):
            continue
        if m.group("kw") == "import":
            # Only the plain form; `import a, b` is left to the parser.
            rest = src[m.start("name") + len(name):]
            rest = rest[: rest.find("\n")] if "\n" in rest else rest
            tail = rest
            if "as" in tail:
                tail = tail[tail.index("as") + 2:]
            if not _IMPORT_SIMPLE.match(tail):
                continue
            out.append(src[last:m.start("kw")])
            out.append("from . import ")
            last = m.start("name")
        else:
            out.append(src[last:m.start("name")])
            out.append(".")
            last = m.start("name")
    if not out:
        return src
    out.append(src[last:])
    return "".join(out)


def _rewrite_sort(src):
    """`xs.sort(cmpfn)` -> `xs.sort(key=cmp_to_key(cmpfn))`.

    Python 2's `sort` took a comparison function positionally; Python 3 takes a
    key. `AI/SuperAI.Evaluate` and `Arenas.enumAvailable` both rely on the old
    form, and no runtime shim can reach a built-in method, so it is source-level.
    """
    spans = _spans(src)

    def blocked(a, b):
        return any(a < e and b > s for s, e in spans)

    needle = ".sort("
    out, last, i = [], 0, 0
    while True:
        i = src.find(needle, i)
        if i < 0:
            break
        start = i + len(needle)
        j, depth = start, 1
        while j < len(src) and depth:
            if src[j] == "(":
                depth += 1
            elif src[j] == ")":
                depth -= 1
            j += 1
        inner = src[start:j - 1]
        head = inner.split("(", 1)[0]
        if inner.strip() and "=" not in head and not blocked(i, j):
            out.append(src[last:start])
            out.append("key=cmp_to_key(")
            out.append(inner)
            out.append(")")
            last = j - 1
        i = j
    if not out:
        return src
    out.append(src[last:])
    return "".join(out)


def translate(src, siblings=None):
    """Rewrite the Python-2 constructs in `src`. Returns new source."""
    out = _rewrite_code(src, _RULES)
    out = _rewrite_relative(out, siblings)
    out = _rewrite_sort(out)
    out = out.replace("<>", "!=")
    return _apply(out, _statement_edits(out))


# ---------------------------------------------------------------------------
# runtime shim
# ---------------------------------------------------------------------------


def _install_shim():
    """Reinstate the 2.2 module functions and aliases the scripts import."""
    import builtins
    import functools
    import string
    import types

    def bind(mod, name, value):
        if not hasattr(mod, name):
            setattr(mod, name, value)

    # `string` was the string module's function library in 2.x.
    bind(string, "split", lambda s, *a: s.split(*a))
    bind(string, "splitfields", lambda s, *a: s.split(*a))
    bind(string, "join", lambda sep, seq: sep.join(seq))
    bind(string, "joinfields", lambda sep, seq: sep.join(seq))
    bind(string, "replace", lambda s, *a: s.replace(*a))
    bind(string, "rjust", lambda s, w, *f: s.rjust(w, *f))
    bind(string, "ljust", lambda s, w, *f: s.ljust(w, *f))
    bind(string, "strip", lambda s, *a: s.strip(*a))
    bind(string, "lower", lambda s: s.lower())
    bind(string, "upper", lambda s: s.upper())
    bind(string, "capitalize", lambda s: s.capitalize())
    bind(string, "atoi", lambda s, *a: int(s, *a))
    bind(string, "atof", lambda s: float(s))
    bind(string, "find", lambda s, sub, *a: s.find(sub, *a))
    bind(string, "count", lambda s, sub, *a: s.count(sub, *a))

    # `types` lost the old class aliases.
    for alias, py3 in (
        ("TupleType", tuple),
        ("ListType", list),
        ("DictType", dict),
        ("IntType", int),
        ("FloatType", float),
        ("StringType", str),
        ("UnicodeType", str),
        ("BooleanType", bool),
        ("NoneType", type(None)),
        ("ClassType", type),
        ("TypeType", type),
        ("FileType", io.IOBase),
    ):
        bind(types, alias, py3)

    # `builtins` names 2.2 relied on.
    bind(builtins, "xrange", range)
    bind(builtins, "unicode", str)
    bind(builtins, "long", int)
    bind(builtins, "basestring", str)
    bind(builtins, "raw_input", input)
    bind(builtins, "reduce", functools.reduce)
    bind(builtins, "apply", lambda f, a=(), kw=None: f(*a, **(kw or {})))
    bind(builtins, "StandardError", Exception)
    # `list.sort(cmp)` becomes `sort(key=cmp_to_key(cmp))` in the translation.
    bind(builtins, "cmp_to_key", functools.cmp_to_key)
    if not hasattr(sys, "maxint"):
        sys.maxint = sys.maxsize


# ---------------------------------------------------------------------------
# import hook
# ---------------------------------------------------------------------------


class _Loader(importlib.abc.Loader):
    def __init__(self, fullname, path):
        self.fullname = fullname
        self.path = path

    def create_module(self, spec):
        return None

    def exec_module(self, module):
        with open(self.path, "r", encoding="latin-1") as fh:
            src = fh.read()
        directory = os.path.dirname(os.path.abspath(self.path))
        # A module in a package gets Python 2's implicit relative imports.
        siblings = directory if os.path.isfile(os.path.join(directory, "__init__.py")) else None
        code = compile(translate(src, siblings), self.path, "exec")
        module.__file__ = self.path
        exec(code, module.__dict__)


class _Finder(importlib.abc.MetaPathFinder):
    """Gives the game's own scripts the translating loader, and nothing else."""

    def __init__(self, roots, excludes=()):
        self.roots = [os.path.abspath(r) for r in roots]
        self.excludes = [os.path.abspath(r) for r in excludes]

    def _legacy(self, path):
        p = os.path.abspath(path)
        if any(p.startswith(e + os.sep) for e in self.excludes):
            return False
        return any(p.startswith(r + os.sep) for r in self.roots)

    def find_spec(self, fullname, path=None, target=None):
        spec = importlib.machinery.PathFinder.find_spec(fullname, path, target)
        if spec is None or not (spec.origin or "").endswith(".py"):
            return None
        if not self._legacy(spec.origin):
            return None
        spec.loader = _Loader(fullname, spec.origin)
        spec.cached = None
        return spec


_installed = False


def install(roots, excludes=()):
    """Install the shim and the translating import hook.

    `roots` are the directories holding the game's own Python-2 scripts;
    `excludes` are directories inside them that must load normally (the shipped
    `Scripts/Lib` is a copy of the Python 2.2 standard library, and Python 3's
    own is used instead).
    """
    global _installed
    _install_shim()
    if _installed:
        return
    _installed = True
    sys.meta_path.insert(0, _Finder(list(roots), list(excludes)))
