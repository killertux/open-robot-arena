"""The original's action list (docs/29 12ba): 0x4ad100 addAction = append (action, 0) then the sort 0x530700; 0x4ad130
removeAction = erase the first entry of that action (0x530ca0) then the same sort. The sort is the MSVC7 std::sort over
8-byte (action, key) entries by key: quicksort partitions (0x531710) while a range is longer than 16, then insertion sorts."""

def _ins_front(a, f, i):
    """Insertion of a[i] as in the sort's first 16 (0x530757 .. / 0x5301e7 loop): a guarded insert."""
    v = a[i]
    if v[1] < a[f][1]:
        a[f + 1:i + 1] = a[f:i]
        a[f] = v
    else:
        j = i
        while v[1] < a[j - 1][1]:
            a[j] = a[j - 1]
            j -= 1
        a[j] = v

def _ins_unguarded(a, i):
    v = a[i]
    j = i
    while v[1] < a[j - 1][1]:
        a[j] = a[j - 1]
        j -= 1
    a[j] = v

def _pivot_key(a, f, l):
    n = l - f
    ka, kb, kc = a[f][1], a[f + n // 2][1], a[l - 1][1]
    if ka < kb:
        if kb < kc:
            return kb
        return ka if kc <= ka else kc
    if ka < kc:
        return ka
    return kb if kc <= kb else kc

def _qloop(a, f, l):
    while l - f > 16:
        p = _pivot_key(a, f, l)
        i, j = f, l
        while True:
            while a[i][1] < p:
                i += 1
            j -= 1
            while p < a[j][1]:
                j -= 1
            if j <= i:
                break
            a[i], a[j] = a[j], a[i]
            i += 1
        if (i - f) < (l - i):
            _qloop(a, f, i)
            f = i
        else:
            _qloop(a, i, l)
            l = i

def sort_actions(a):
    n = len(a)
    if n <= 16:
        for i in range(1, n):
            _ins_front(a, 0, i)
        return
    _qloop(a, 0, n)
    for i in range(1, 16):
        _ins_front(a, 0, i)
    for i in range(16, n):
        _ins_unguarded(a, i)

def add(a, x):
    a.append((x, 0)); sort_actions(a)

def remove(a, x):
    for i, e in enumerate(a):
        if e[0] == x:
            del a[i]; break
    sort_actions(a)
