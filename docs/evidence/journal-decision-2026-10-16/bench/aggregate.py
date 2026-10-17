import re, json, glob, os, statistics as st, sys
D = sys.argv[1]
rows = {}
def add(k, v):
    rows.setdefault(k, []).append(v)
loads = {}
for f in sorted(glob.glob(os.path.join(D, 'r*-*.txt'))):
    name = os.path.basename(f)[:-4]
    rnd, step = name.split('-', 1)
    txt = open(f).read()
    if not re.search(r'^end ', txt, re.M): continue
    m = re.search(r'^start \S+ ([\d.]+)', txt, re.M)
    if m: loads.setdefault(step, []).append(float(m.group(1)))
    pm = re.search(r'process: real ([\d.]+) s, user ([\d.]+) s, sys ([\d.]+) s', txt)
    if pm: add(step + ' | process user+sys s', float(pm.group(2)) + float(pm.group(3)))
    if step == 'tc4-foldonly':
        m = re.search(r'\[fold\].*?: ([\d.]+) ms  \(runs: ([^)]*)\)', txt)
        add(step + ' | fold median-of-3 ms', float(m.group(1)))
        for r in m.group(2).split(','): add(step + ' | fold every run ms', float(r))
    elif step == 'tc4-open':
        j = json.loads(re.search(r'^\{.*\}$', txt, re.M).group(0))
        for k in ('scan_ms', 'fold_ms', 'total_ms', 'scan_cpu_ms', 'fold_cpu_ms', 'maxrss_mib'):
            add(step + ' | ' + k, j[k])
    elif step.endswith('project'):
        m = re.search(r'projection ([\d.]+)(m?s), cpu over all threads (\d+) ms', txt)
        v = float(m.group(1)) * (1 if m.group(2) == 'ms' else 1000)
        add(step + ' | projection wall ms', v); add(step + ' | projection cpu ms', float(m.group(3)))
    else:
        def dur(s):
            n, u = re.match(r'([\d.]+)(ms|s|µs)', s).groups()
            return float(n) * {'ms': 1, 's': 1000, 'µs': 0.001}[u]
        m = re.search(r'cold open (\S+), the fold after the last file (\S+), cpu over all threads (\d+) ms', txt)
        add(step + ' | cold open wall ms', dur(m.group(1).rstrip(','))); add(step + ' | after-last-file ms', dur(m.group(2).rstrip(','))); add(step + ' | cold open cpu ms', float(m.group(3)))
        m = re.search(r'load (\S+), cpu over all threads (\d+) ms', txt)
        add(step + ' | load wall ms', dur(m.group(1).rstrip(','))); add(step + ' | load cpu ms', float(m.group(2)))
        m = re.search(r'^fold (\S+), cpu (\d+) ms', txt, re.M)
        add(step + ' | fold wall ms', dur(m.group(1).rstrip(','))); add(step + ' | fold cpu ms', float(m.group(2)))
        m = re.search(r'peak RSS during the streamed cold opens (\d+) MiB', txt)
        add(step + ' | open peak RSS MiB', float(m.group(1)))
print('| measure | n | median | min | max | runs |')
print('|---|---:|---:|---:|---:|---|')
for k in sorted(rows):
    v = rows[k]
    print(f'| {k} | {len(v)} | {st.median(v):.0f} | {min(v):.0f} | {max(v):.0f} | {", ".join(f"{x:.0f}" for x in v)} |')
print()
for k, v in sorted(loads.items()):
    print(f'load before {k}: ' + ', '.join(f'{x:.2f}' for x in v))
