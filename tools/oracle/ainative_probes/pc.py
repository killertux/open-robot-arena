import AI, plus, Arenas
a = AI.running_ai[0]
b = AI.running_ai[1]
A = Arenas.currentArena
def t(label, f):
    try:
        out.write('%s\t%r\n' % (label, f()))
    except:
        import traceback
        out.write('%s\tEXC\n' % label)
        traceback.print_exc(None, out)
out.write('time %r\n' % plus.getTimeElapsed())
t('start2', lambda: A.GetStartPointLocation(2))
t('plus.loc5', lambda: plus.getLocation(5))
t('a.lastdone', lambda: a.GetLastDamageDone())
t('b.lastrecv', lambda: b.GetLastDamageReceived())
