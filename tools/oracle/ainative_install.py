# Installed into the ORIGINAL's Python 2.2 (scratch tool; nothing under local/game is touched):
# logs every native plus.AI / plus.Arena / plus.* query the brains make to C:\ra2oracle\ainative.txt
# as  time <TAB> player <TAB> name <TAB> args <TAB> result
import AI, plus, Arenas, __builtin__
if not hasattr(__builtin__, '_ra2_ainative'):
    __builtin__._ra2_ainative = open('C:\\ra2oracle\\ainative.txt', 'w')
    _log = __builtin__._ra2_ainative
    _busy = [0]

    def _short(x):
        s = repr(x)
        if len(s) > 400: s = s[:400]
        return s

    def _wrap_m(cls, name):
        orig = getattr(cls, name)
        def w(self, *a):
            r = apply(orig, (self,) + a)
            if not _busy[0]:
                _busy[0] = 1
                try:
                    try:
                        pid = self.GetID()
                    except:
                        pid = '-'
                    extra = ''
                    if name in ('GetSpeed', 'GetTurning'):
                        extra = '\t' + _short((plus.AI.GetDirection(self), plus.AI.GetLocation(self), plus.AI.GetHeading(self, 0), plus.AI.IsUpsideDown(self)))
                    _log.write('%r\t%s\t%s\t%s\t%s%s\n' % (plus.getTimeElapsed(), pid, name, _short(a), _short(r), extra))
                    _log.flush()
                except:
                    pass
                _busy[0] = 0
            return r
        setattr(cls, name, w)

    def _wrap_f(mod, name):
        orig = getattr(mod, name)
        def w(*a):
            r = apply(orig, a)
            if not _busy[0]:
                _busy[0] = 1
                try:
                    _log.write('%r\tplus\tplus.%s\t%s\t%s\n' % (plus.getTimeElapsed(), name, _short(a), _short(r)))
                    _log.flush()
                except:
                    pass
                _busy[0] = 0
            return r
        setattr(mod, name, w)

    for _n in ('GetLocation', 'GetDirection', 'GetHeading', 'GetHeadingTo', 'GetHeadingToID',
               'GetDistanceTo', 'GetDistanceToID', 'GetSpeed', 'GetTurning', 'IsUpsideDown',
               'GetEnemies', 'GetNumComponents', 'GetLastDamageReceived', 'GetLastDamageDone',
               'GetZoneContents', 'GetInputStatus', 'Input'):
        try:
            _wrap_m(AI.SuperAI, _n)
        except:
            _log.write('wrapfail %s\n' % _n)
    for _n in ('getLocation', 'isHuman', 'getPlayers', 'isDefeated', 'isEliminated', 'getWeight', 'isUpsideDown'):
        try:
            _wrap_f(plus, _n)
        except:
            _log.write('wrapfail plus.%s\n' % _n)
    for _n in ('ToGrid', 'FromGrid', 'IsStraightPathClear'):
        try:
            _wrap_m(Arenas.SuperArena, _n)
        except:
            _log.write('wrapfail arena %s\n' % _n)
out.write('installed')
