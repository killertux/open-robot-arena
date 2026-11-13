# Installed into the ORIGINAL's Python 2.2 before a match starts (capture_boxstart.py, AILOG=1): every
# Input() the shipped brains make and the tactic every Evaluate() picks, with the game clock, in a list
# __builtin__._ra2_ailog = [(time, player, 'I', name, index, value) | (time, player, 'E', tactic, priority, upside_down)]
# that capture_boxstart.py reads back after the fight (docs/29 12bg).
import AI, plus, __builtin__
if not hasattr(__builtin__, '_ra2_ailog'):
    __builtin__._ra2_ailog = []
    _log = __builtin__._ra2_ailog
    _oi = AI.SuperAI.Input
    _oe = AI.SuperAI.Evaluate
    def _Input(self, name, index, value, _oi=_oi):
        _log.append((plus.getTimeElapsed(), self.GetID(), 'I', name, index, value))
        return _oi(self, name, index, value)
    def _Evaluate(self, _oe=_oe):
        r = _oe(self)
        _log.append((plus.getTimeElapsed(), self.GetID(), 'E', self.tactics[0].name, self.tactics[0].priority, self.IsUpsideDown()))
        return r
    AI.SuperAI.Input = _Input
    AI.SuperAI.Evaluate = _Evaluate
out.write('installed')
