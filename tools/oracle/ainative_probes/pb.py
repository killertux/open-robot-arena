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
t('ray.miss', lambda: A.RayTest((100.0, 100.0, 100.0), (100.0, 101.0, 100.0)))
t('ray.miss2', lambda: A.RayTest((0.0, 5.0, 0.0), (0.0, 5.5, 0.0)))
t('ray.hit', lambda: A.RayTest((0.0, 5.0, 0.0), (0.0, -5.0, 0.0)))
t('ray.zero', lambda: A.RayTest((0.0, 5.0, 0.0), (0.0, 5.0, 0.0)))
t('plus.ray', lambda: plus.rayTest((0.0, 5.0, 0.0), (0.0, -5.0, 0.0)))
