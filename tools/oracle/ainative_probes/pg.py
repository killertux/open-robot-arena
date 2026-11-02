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
P = a.GetLocation()
Q = b.GetLocation()
out.write('P %r Q %r\n' % (P, Q))
t('ray.top', lambda: plus.rayTest((P[0], P[1] + 3.0, P[2]), (P[0], P[1] - 3.0, P[2])))
t('ray.floor', lambda: plus.rayTest((P[0], P[1] + 3.0, P[2]), (P[0], -10.0, P[2])))
t('ray.floorQ', lambda: plus.rayTest((Q[0], Q[1] + 3.0, Q[2]), (Q[0], -10.0, Q[2])))
t('ray.below', lambda: plus.rayTest((P[0], -3.0, P[2]), (P[0], -5.0, P[2])))
t('ray.wallA', lambda: plus.rayTest((-20.0, P[1], P[2]), (P[0], P[1], P[2])))
t('ray.wallB', lambda: plus.rayTest((20.0, Q[1], Q[2]), (Q[0], Q[1], Q[2])))
t('ray.AtoB', lambda: plus.rayTest((P[0], P[1], P[2]), (Q[0], Q[1], Q[2])))
t('ray.nothing', lambda: plus.rayTest((0.0, 100.0, 0.0), (0.0, 101.0, 0.0)))
t('ray.zero', lambda: plus.rayTest((0.0, 5.0, 0.0), (0.0, 5.0, 0.0)))
t('a.isenemy', lambda: (a.IsEnemy(0), a.IsEnemy(1), a.IsEnemy(5)))
