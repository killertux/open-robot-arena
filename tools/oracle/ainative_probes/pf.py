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
for v in (0.2, 0.5, 0.7, 1.0, 1.5, 2.5, 43.4, 43.6, 99.9, -0.2, -0.5, -0.7, -1.5, -2.5, -43.4, -43.6, 100.0):
    a.Input('Forward', 0, v)
    out.write('input %r -> status %r\n' % (v, a.GetInputStatus('Forward', 0)))
a.Input('Forward', 0, 0)
a.Input('Spin', 0, 1)
out.write('spin1 %r\n' % a.GetInputStatus('Spin', 0))
a.Input('Spin', 0, 1)
out.write('spin2 %r\n' % a.GetInputStatus('Spin', 0))
a.Input('Spin', 0, 0)
out.write('spin0 %r\n' % a.GetInputStatus('Spin', 0))
