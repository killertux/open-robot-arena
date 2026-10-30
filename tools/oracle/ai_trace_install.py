# Installed into the ORIGINAL's Python 2.2 before a match starts (capture_boxstart.py,
# AITRACE=1): logs every drive command of the shipped brains to C:\ra2oracle\aitrace.txt
# as  time <TAB> player <TAB> call <TAB> args <TAB> result.
import AI, plus, __builtin__
if not hasattr(__builtin__, '_ra2_aitrace'):
    __builtin__._ra2_aitrace = open('C:\\ra2oracle\\aitrace.txt', 'w')
    _log = __builtin__._ra2_aitrace

    def _short(x):
        s = repr(x)
        if len(s) > 160: s = s[:160]
        return s

    def _wrap(name):
        orig = getattr(AI.SuperAI, name)
        def w(self, *a):
            try:
                before = _short(a)
            except:
                before = '?'
            r = apply(orig, (self,) + a)
            try:
                _log.write('%r\t%s\t%s\t%s\t%s\n' % (plus.getTimeElapsed(), self.GetID(), name, before, _short(r)))
                _log.flush()
            except:
                pass
            return r
        setattr(AI.SuperAI, name, w)

    for _n in ('Throttle', 'Turn', 'DriveToWaypoints', 'DriveToLocation', 'AimToHeading',
               'IsStraightPathClear', 'GetPath', 'GetHeadingTo', 'GetHeadingToID'):
        _wrap(_n)
import Tactics, traceback
def _wrap_exec(cls):
    orig = cls.Execute
    def w(self):
        try:
            r = orig(self)
        except:
            _log.write('%r\t%s\t%s EXCEPTION\n' % (plus.getTimeElapsed(), self.ai.GetID(), cls.name))
            traceback.print_exc(None, _log)
            _log.flush()
            raise
        try:
            _log.write('%r\t%s\t%s.Execute\t()\t%s\n' % (plus.getTimeElapsed(), self.ai.GetID(), cls.name, _short(r)))
            _log.flush()
        except:
            pass
        return r
    cls.Execute = w
for _c in (Tactics.Invert, Tactics.Unstuck, Tactics.Engage):
    _wrap_exec(_c)
for _n in ('IsUpsideDown', 'Evaluate'):
    _wrap(_n)
out.write('installed')
