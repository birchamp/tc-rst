"""Two builds of foldbench side by side on one corpus, interleaved, one open per process.

usage: python3 -I ab.py CORPUS BEFORE AFTER ROUNDS OUT.tsv

BEFORE and AFTER may differ in their fold cache's encoding, so BEFORE opens a copy of CORPUS
whose packs and pending files are hard links to CORPUS's (the same bytes, the same page
cache) and which keeps its own index.bin. Each round runs, for each build in turn (the order
alternates by round), with every file read once just before:

  first           no cache: the first open (it writes the build's cache)
  cached          the cache it wrote
  psalms_glibc    one open with the cache, then 20 projections of Psalms
  psalms_jemalloc the same with jemalloc 5.3 (LD_PRELOAD)

Each row: round, build, mode, wall ms of the open, CPU ms of the open over all threads,
peak RSS of the open MiB, RSS with the fold open MiB, Psalms median and p95 ms, Psalms CPU
median ms, peak RSS while Psalms was projected MiB, RSS after MiB, the 1-minute load average
before and after.
"""
import os
import re
import shutil
import subprocess
import sys

corpus, before, after, rounds, out = (sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]),
                                      sys.argv[5])
JEMALLOC = "/usr/lib/x86_64-linux-gnu/libjemalloc.so.2"
copy = corpus.rstrip("/") + "-before"
if not os.path.exists(copy):
    os.makedirs(copy)
    shutil.copy(os.path.join(corpus, "project"), copy)
    for sub in ("packs", "pending"):
        os.makedirs(os.path.join(copy, sub))
        for name in sorted(os.listdir(os.path.join(corpus, sub))):
            os.link(os.path.join(corpus, sub, name), os.path.join(copy, sub, name))
dirs = {"before": copy, "after": corpus}
binaries = {"before": before, "after": after}


def load():
    return float(open("/proc/loadavg").read().split()[0])


def ms(text):
    m = re.fullmatch(r"([\d.]+)(ns|µs|us|ms|s)", text)
    value, unit = float(m.group(1)), m.group(2)
    return value * {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1e3}[unit]


def warm(d):
    for sub in ("packs", "pending"):
        for name in os.listdir(os.path.join(d, sub)):
            with open(os.path.join(d, sub, name), "rb") as f:
                while f.read(1 << 20):
                    pass


def run(build, env):
    d = dirs[build]
    warm(d)
    l0 = load()
    p = subprocess.run([binaries[build], "run", d, "4"], capture_output=True, text=True,
                       env=dict(os.environ, FOLDBENCH_RUNS="1", **env))
    l1 = load()
    assert p.returncode == 0, p.stderr
    return p.stdout, p.stderr, l0, l1


def first(build):
    index = os.path.join(dirs[build], "index.bin")
    if os.path.exists(index):
        os.remove(index)
    o, e, l0, l1 = run(build, {"FOLDBENCH_FIRST_ONLY": "1"})
    m = re.search(r"first open without a cache (\d+) ms, the fold after the last file \d+ ms, "
                  r"cpu over all threads (\d+) ms, peak RSS (\d+) MiB", o)
    assert m, o + e
    return [m.group(1), m.group(2), m.group(3), "", "", "", "", "", "", l0, l1]


def cached(build):
    o, e, l0, l1 = run(build, {"FOLDBENCH_CACHED_ONLY": "1"})
    assert "not used" not in e, e
    m = re.search(r"cold open with the cache (\S+), the fold after the last file \S+, cpu over "
                  r"all threads (\d+) ms", e)
    r = re.search(r"peak RSS of each cold open with the cache \[(\d+)\] MiB, RSS with the fold "
                  r"open (\d+) MiB", o)
    assert m and r, o + e
    return [f"{ms(m.group(1)):.0f}", m.group(2), r.group(1), r.group(2), "", "", "", "", "", l0,
            l1]


def psalms(build, preload):
    env = {"FOLDBENCH_PSALMS_ONLY": "1"}
    if preload:
        env["LD_PRELOAD"] = JEMALLOC
    o, e, l0, l1 = run(build, env)
    assert "not used" not in e, e
    m = re.search(r"cold open with the cache (\S+), the fold after the last file \S+, cpu over "
                  r"all threads (\d+) ms", e)
    r = re.search(r"peak RSS of each cold open with the cache \[(\d+)\] MiB, RSS with the fold "
                  r"open (\d+) MiB", o)
    s = re.search(r"\| Psalms projection: [^|]*\| \d+ \| ([\d.]+) ms \| ([\d.]+) ms \|", o)
    c = re.search(r"CPU time of a projection, all threads: median (\d+) ms", o)
    p = re.search(r"peak RSS while Psalms was projected (\d+) MiB; RSS with the fold open after "
                  r"Psalms was projected (\d+) MiB", o)
    assert m and r and s and c and p, o + e
    return [f"{ms(m.group(1)):.0f}", m.group(2), r.group(1), r.group(2), s.group(1), s.group(2),
            c.group(1), p.group(1), p.group(2), l0, l1]


with open(out, "a") as f:
    for rnd in range(rounds):
        order = ["before", "after"] if rnd % 2 == 0 else ["after", "before"]
        for mode, action in [("first", first), ("cached", cached),
                             ("psalms_glibc", lambda b: psalms(b, False)),
                             ("psalms_jemalloc", lambda b: psalms(b, True))]:
            for build in order:
                line = "\t".join(str(x) for x in [rnd, build, mode] + action(build))
                print(line, flush=True)
                f.write(line + "\n")
                f.flush()
