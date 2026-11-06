"""live_trials.py capture.jsonl STEPS SECS [ENV=VAL ...]: every trial of a tipab capture (frida/ra2_tipab.js) run in ours from
the original's own placed state (`tipprobe live`, TIPPROBE_LIVE_POSE=1: every body of the Scout at the live centre and, by its
inertia tensor, the live frame), and the outcome (tail / wheels / back after STEPS substeps) and the chassis' lean extremes
of both side by side (docs/29 12be).  Run from the repository root; needs examples/tipprobe built in engine/."""
import json, os, subprocess, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
SP = tempfile.mkdtemp()
J=sys.argv[1]; NS=int(sys.argv[2]); secs=sys.argv[3]
extra=sys.argv[4:]
L=[json.loads(l) for l in open(J)]
grid=json.load(open(J.replace('.jsonl','.json')))['grid']['trials']
places=[i for i,r in enumerate(L) if r[0]=='X' and r[2]=='place' and not (L[i-1][0]=='X' and L[i-1][2]=='place')]
dones=[i for i,r in enumerate(L) if r[0]=='X' and r[2]=='done']
env=dict(os.environ); env.update({'LC_ALL':'C','TIPPROBE_ROBOT':'0','TIPPROBE_LIVE_POSE':'1'})
for e in extra:
    k,v=e.split('=',1); env[k]=v
G='../local/game'
def rod_start(rows):
    """The first substep at which the second robot's piston rod (30-36 kg) moves 2 m/s against its chassis (232.1 kg): when the
    live trial's Fire took effect (the capture presses it from the host, so it is a few substeps after the nominal one)."""
    step, ch = 0, None
    for r in rows:
        if r[0] == 'A':
            step += 1
        if r[0] == 'B' and abs(r[3] - 232.116) < 0.01:
            ch = r
        if r[0] == 'B' and ch is not None and 30 < r[3] < 36:
            if sum((a - b) ** 2 for a, b in zip(r[5], ch[5])) ** .5 > 2:
                return step
    return None

def lean(rows):
    return [r[7][4] for r in rows if r[0]=='B' and abs(r[3]-163.49)<0.1 and len(r[7])==9]
tot=0;diff=0;out=[]
for ti,(t,(pi,di)) in enumerate(zip(grid,zip(places,dones))):
    live=lean(L[pi:di])
    D=SP+'/tt'; os.makedirs(D,exist_ok=True)
    subprocess.run(['python3','-I',HERE+'/live_state.py',J,str(pi),D+'/st.txt'],check=True)
    env['RA2_H17_DUMP']=D+'/d.jsonl'
    if 'TIPPROBE_LIVE_ALARM' in env and env.get('TIPPROBE_ALARM_FIRE') == 'rod':
        rs = rod_start(L[pi:di])
        env['TIPPROBE_ALARM_FIRE'] = str(max((rs or 1) - 1, 0) / 60.0)
    if 'drive_py' in t:
        env['TIPPROBE_DRIVE'] = ','.join('%d:%d:%d' % (round(s * 1.5), th, tu) for s, th, tu in t['drive_py'])
    else:
        env.pop('TIPPROBE_DRIVE', None)
    subprocess.run(['./target/release/examples/tipprobe',G+'/Arenas/'+os.environ.get('ARENA','box')+'.py',G+'/AI/Team0/Bot0.bot',G+'/AI/Team0/Bot1.bot','live',D+'/st.txt',secs],cwd=os.path.join(HERE,'..','..','..','engine'),env=env,capture_output=True)
    O=[json.loads(l) for l in open(D+'/d.jsonl')]
    x=[i for i,r in enumerate(O) if r[0]=='X'][0]
    ours=lean(O[x:])
    n=min(len(live),len(ours),NS)
    kind=lambda s: 'wheels' if s[n-1]>0.9 else ('back' if s[n-1]<-0.9 else 'tail')
    d=max(abs(live[i]-ours[i]) for i in range(n))
    if os.environ.get('LT_DUMP')==str(ti): print('LEAN live', [round(x,3) for x in live[:n:6]], '\nLEAN ours', [round(x,3) for x in ours[:n:4]])
    tot+=1; diff+= kind(live)!=kind(ours)
    print('%2d %-20s live %-6s min %.3f max %.3f | ours %-6s min %.3f max %.3f | max|d| %.3f'%(ti,t['label'],kind(live),min(live[:n]),max(live[:n]),kind(ours),min(ours[:n]),max(ours[:n]),d),flush=True)
print('trials',tot,'outcome differs',diff)
