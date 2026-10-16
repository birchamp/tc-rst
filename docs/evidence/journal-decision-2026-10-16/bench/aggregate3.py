import re, json, glob, os, statistics as st, sys
rows = {}; loads = {}; loads_end = {}
def add(k, v): rows.setdefault(k, []).append(v)
def dur(s):
    n, u = re.match(r'([\d.]+)(ms|s|µs)', s).groups()
    return float(n) * {'ms': 1, 's': 1000, 'µs': 0.001}[u]
for D in sys.argv[1:]:
  for f in sorted(glob.glob(os.path.join(D, '*.txt'))):
    base = os.path.basename(f)[:-4]
    step = base.split('-', 1)[1] if re.match(r'r\d+-', base) else base
    txt = open(f).read()
    if not re.search(r'^end ', txt, re.M): continue
    m = re.search(r'^start \S+ ([\d.]+)', txt, re.M); loads.setdefault(step, []).append(float(m.group(1)))
    m = re.search(r'^end \S+ ([\d.]+)', txt, re.M); loads_end.setdefault(step, []).append(float(m.group(1)))
    pm = re.search(r'process: real ([\d.]+) s, user ([\d.]+) s, sys ([\d.]+) s', txt)
    if pm:
        add(step + ' | process wall ms', 1000 * float(pm.group(1)))
        add(step + ' | process user+sys ms', 1000 * (float(pm.group(2)) + float(pm.group(3))))
    for line in re.findall(r'^\{.*\}$', txt, re.M):
        for k, v in json.loads(line).items():
            if isinstance(v, (int, float)) and k not in ('rep',): add(step + ' | ' + k, v)
    m = re.search(r'\[fold\].*?: ([\d.]+) ms  \(runs: ([^)]*)\)', txt)
    if m:
        add(step + ' | fold median-of-3 ms', float(m.group(1)))
        for r in m.group(2).split(','): add(step + ' | fold every run ms', float(r))
    m = re.search(r'cold open (\S+), the fold after the last file (\S+), cpu over all threads (\d+) ms', txt)
    if m:
        add(step + ' | cold open wall ms', dur(m.group(1).rstrip(','))); add(step + ' | after-last-file ms', dur(m.group(2).rstrip(','))); add(step + ' | cold open cpu ms', float(m.group(3)))
        m = re.search(r'load (\S+), cpu over all threads (\d+) ms', txt)
        if m: add(step + ' | load wall ms', dur(m.group(1).rstrip(','))); add(step + ' | load cpu ms', float(m.group(2)))
        m = re.search(r'^fold (\S+), cpu (\d+) ms', txt, re.M)
        if m: add(step + ' | fold wall ms', dur(m.group(1).rstrip(','))); add(step + ' | fold cpu ms', float(m.group(2)))
    m = re.search(r'peak RSS during the streamed cold opens (\d+) MiB', txt)
    if m: add(step + ' | open peak RSS MiB', float(m.group(1)))
    for m in re.finditer(r'^projection (\S+), cpu over all threads (\d+) ms, (\d+) files, (\d+) bytes', txt, re.M):
        add(step + ' | projection wall ms', dur(m.group(1).rstrip(','))); add(step + ' | projection cpu ms', float(m.group(2)))
        add(step + ' | projected files', float(m.group(3))); add(step + ' | projected bytes', float(m.group(4)))
print('| run | measure | n | median | min | max | runs |')
print('|---|---|---:|---:|---:|---:|---|')
for k in sorted(rows):
    v = rows[k]; a, b = k.split(' | ')
    print(f'| {a} | {b} | {len(v)} | {st.median(v):.0f} | {min(v):.0f} | {max(v):.0f} | {", ".join(f"{x:.0f}" for x in v)} |')
print()
for k, v in sorted(loads.items()):
    print(f'load (1-min avg) before {k}: ' + ', '.join(f'{x:.2f}' for x in v) + ' | after: ' + ', '.join(f'{x:.2f}' for x in loads_end[k]))
