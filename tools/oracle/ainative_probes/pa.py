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
t('a.enemies', lambda: a.GetEnemies())
t('b.enemies', lambda: b.GetEnemies())
t('plus.defeated0', lambda: plus.isDefeated(0))
t('plus.defeated1', lambda: plus.isDefeated(1))
t('plus.elim0', lambda: plus.isEliminated(0))
t('plus.elim1', lambda: plus.isEliminated(1))
t('plus.loc1', lambda: plus.getLocation(1))
t('a.distID1', lambda: a.GetDistanceToID(1))
t('a.hdgToID1', lambda: a.GetHeadingToID(1, 0))
t('plus.players', lambda: plus.getPlayers())
t('plus.upside1', lambda: plus.isUpsideDown(1))
t('plus.weight1', lambda: plus.getWeight(1))
t('plus.matchover', lambda: plus.isMatchOver())
