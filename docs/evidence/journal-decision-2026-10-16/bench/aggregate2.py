import re, json, glob, os, statistics as st, sys
rows = {}; loads = {}
def add(k, v): rows.setdefault(k, []).append(v)
def dur(s):
    n, u = re.match(r'([\d.]+)(ms|s|µs)', s).groups()
    return float(n) * {'ms': 1, 's': 1000, 'µs': 0.001}[u]
for D in sys.argv[1:]:
  for f in sorted(glob.glob(os.path.join(D, 'r*-*.txt'))):
    rnd, step = os.path.basename(f)[:-4].split('-', 1)
    txt = open(f).read()
    if not re.search(r'^end ', txt, re.M): continue
    m = re.search(r'^start \S+ ([\d.]+)', txt, re.M)
    loads.setdefault(step, []).append(float(m.group(1)))
    pm = re.search(r'process: real ([\d.]+) s, user ([\d.]+) s, sys ([\d.]+) s', txt)
    if pm: add(step + ' | process user+sys ms', 1000 * (float(pm.group(2)) + float(pm.group(3))))
    for line in re.findall(r'^\{.*\}$', txt, re.M):
        for k, v in json.loads(line).items():
            if isinstance(v, (int, float)) and k not in ('rep',): add(step + ' | ' + k, v)
    m = re.search(r'loadprof one thread: (\d+) files, (\d+) bytes, (\d+) lines.*?read (\d+) ms, file SHA-256 \(P2\) (\d+) ms, line split\+SHA-256 \(P4\) (\d+) ms, full read_pack/read_pending\+prepare (\d+) ms .*?= (-?\d+) ms', txt, re.S)
    if m:
        for k, g in zip(['files', 'bytes', 'lines', 'read_ms', 'P2_sha_ms', 'P4_sha_ms', 'full_ms', 'parse_checks_prepare_ms'], m.groups()): add(step + ' | ' + k, float(g))
    m = re.search(r'\[fold\].*?: ([\d.]+) ms  \(runs: ([^)]*)\)', txt)
    if m:
        for r in m.group(2).split(','): add(step + ' | fold every run ms', float(r))
    m = re.search(r'cold open (\S+), the fold after the last file (\S+), cpu over all threads (\d+) ms', txt)
    if m:
        add(step + ' | cold open wall ms', dur(m.group(1).rstrip(','))); add(step + ' | after-last-file ms', dur(m.group(2).rstrip(','))); add(step + ' | cold open cpu ms', float(m.group(3)))
        m = re.search(r'load (\S+), cpu over all threads (\d+) ms', txt)
        add(step + ' | load wall ms', dur(m.group(1).rstrip(','))); add(step + ' | load cpu ms', float(m.group(2)))
        m = re.search(r'^fold (\S+), cpu (\d+) ms', txt, re.M)
        add(step + ' | fold wall ms', dur(m.group(1).rstrip(','))); add(step + ' | fold cpu ms', float(m.group(2)))
print('| measure | n | median | min | max | runs |')
print('|---|---:|---:|---:|---:|---|')
for k in sorted(rows):
    v = rows[k]
    print(f'| {k} | {len(v)} | {st.median(v):.0f} | {min(v):.0f} | {max(v):.0f} | {", ".join(f"{x:.0f}" for x in v)} |')
print()
for k, v in sorted(loads.items()):
    print(f'load before {k}: ' + ', '.join(f'{x:.2f}' for x in v))
