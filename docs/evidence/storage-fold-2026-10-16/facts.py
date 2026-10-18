"""What holding every file's facts until O7's rules costs an open with the fold cache.

usage: python3 -I facts.py CORPUS FOLDBENCH ROUNDS OUT.tsv

CORPUS must hold FOLDBENCH's cache. Each round runs, in an order that alternates by round,
one open with the cache followed by 20 projections of Psalms (FOLDBENCH_PSALMS_ONLY), with
every file read once just before, four ways: holding each file's facts until O7's rules run
(as the open does) or dropping them as each file is read (FOLDBENCH_NO_O7), each with glibc
malloc and with jemalloc 5.3 (LD_PRELOAD).

Each row: round, facts (held or dropped), allocator, wall ms of the open, CPU ms of the open
over all threads, peak RSS of the open MiB, RSS with the fold open MiB, Psalms median and p95
ms, peak RSS while Psalms was projected MiB, RSS after MiB, the 1-minute load average before
and after.
"""
import os
import re
import subprocess
import sys

corpus, binary, rounds, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
JEMALLOC = "/usr/lib/x86_64-linux-gnu/libjemalloc.so.2"


def load():
    return float(open("/proc/loadavg").read().split()[0])


def ms(text):
    m = re.fullmatch(r"([\d.]+)(ns|µs|us|ms|s)", text)
    value, unit = float(m.group(1)), m.group(2)
    return value * {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1e3}[unit]


def warm():
    for sub in ("packs", "pending"):
        for name in os.listdir(os.path.join(corpus, sub)):
            with open(os.path.join(corpus, sub, name), "rb") as f:
                while f.read(1 << 20):
                    pass


def run(facts, allocator):
    env = dict(os.environ, FOLDBENCH_RUNS="1", FOLDBENCH_PSALMS_ONLY="1")
    if facts == "dropped":
        env["FOLDBENCH_NO_O7"] = "1"
    if allocator == "jemalloc":
        env["LD_PRELOAD"] = JEMALLOC
    warm()
    l0 = load()
    p = subprocess.run([binary, "run", corpus, "4"], capture_output=True, text=True, env=env)
    l1 = load()
    assert p.returncode == 0, p.stderr
    o, e = p.stdout, p.stderr
    assert "not used" not in e, e
    m = re.search(r"cold open with the cache (\S+), the fold after the last file \S+, cpu over "
                  r"all threads (\d+) ms", e)
    r = re.search(r"peak RSS of each cold open with the cache \[(\d+)\] MiB, RSS with the fold "
                  r"open (\d+) MiB", o)
    s = re.search(r"\| Psalms projection: [^|]*\| \d+ \| ([\d.]+) ms \| ([\d.]+) ms \|", o)
    q = re.search(r"peak RSS while Psalms was projected (\d+) MiB; RSS with the fold open after "
                  r"Psalms was projected (\d+) MiB", o)
    assert m and r and s and q, o + e
    return [f"{ms(m.group(1)):.0f}", m.group(2), r.group(1), r.group(2), s.group(1), s.group(2),
            q.group(1), q.group(2), l0, l1]


ways = [(f, a) for a in ("glibc", "jemalloc") for f in ("held", "dropped")]
with open(out, "a") as f:
    for rnd in range(rounds):
        for facts, allocator in (ways if rnd % 2 == 0 else ways[::-1]):
            line = "\t".join(str(x) for x in [rnd, facts, allocator] + run(facts, allocator))
            print(line, flush=True)
            f.write(line + "\n")
            f.flush()
