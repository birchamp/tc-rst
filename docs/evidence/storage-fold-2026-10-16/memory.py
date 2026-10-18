"""Memory of the open and of the app's case, a build before the memory fixes against this one.

usage: python3 -I memory.py CORPUS BEFORE AFTER ROUNDS OUT.tsv

CORPUS must hold the fold cache (`index.bin`), which both builds read and write byte for byte
alike. BEFORE and AFTER are foldbench binaries. Each round runs, in an order that alternates by
round, every file read once before each process:

- `first`: the first open without a cache (FOLDBENCH_FIRST_ONLY with `index.bin` moved aside;
  the cache it writes must equal the one moved aside, which is then put back);
- `psalms`: one open with the cache, then 20 projections of Psalms (FOLDBENCH_PSALMS_ONLY);
- `app` (AFTER only): `foldbench app`, one open with the cache, then the incremental steps,
  the conflict lists, 20 projections of Psalms and the snapshots.

each with glibc malloc and with jemalloc 5.3 (LD_PRELOAD).

Each row: round, build, case, allocator, wall ms of the open, peak RSS of the open MiB, RSS with
the fold open MiB, Psalms median ms, Psalms p95 ms, peak RSS after the open MiB, RSS at the end
MiB, the 1-minute load average before and after. A figure a case does not give is `-`.
"""
import filecmp
import os
import re
import shutil
import subprocess
import sys

corpus, before, after, rounds, out = sys.argv[1:4] + [int(sys.argv[4]), sys.argv[5]]
JEMALLOC = "/usr/lib/x86_64-linux-gnu/libjemalloc.so.2"
INDEX = os.path.join(corpus, "index.bin")
KEPT = os.path.join(corpus, "index.bin.kept")


def load():
    return float(open("/proc/loadavg").read().split()[0])


def warm():
    for sub in ("packs", "pending"):
        for name in os.listdir(os.path.join(corpus, sub)):
            with open(os.path.join(corpus, sub, name), "rb") as f:
                while f.read(1 << 20):
                    pass


def mib(text):
    return re.sub(r" MiB$", "", text)


def run(binary, case, allocator):
    env = dict(os.environ)
    if allocator == "jemalloc":
        env["LD_PRELOAD"] = JEMALLOC
    args = [binary, "run", corpus, "4"]
    if case == "first":
        env["FOLDBENCH_FIRST_ONLY"] = "1"
        os.replace(INDEX, KEPT)
    elif case == "psalms":
        env.update(FOLDBENCH_RUNS="1", FOLDBENCH_PSALMS_ONLY="1")
    else:
        args = [binary, "app", corpus, "4"]
    warm()
    l0 = load()
    p = subprocess.run(args, capture_output=True, text=True, env=env)
    l1 = load()
    assert p.returncode == 0, p.stderr
    o = p.stdout
    row = ["-"] * 7
    if case == "first":
        m = re.search(r"first open without a cache (\d+) ms, .* peak RSS (\d+) MiB", o)
        assert m, o
        assert filecmp.cmp(INDEX, KEPT, shallow=False), "the cache differs"
        os.replace(KEPT, INDEX)
        row[0], row[1] = m.group(1), m.group(2)
    elif case == "psalms":
        r = re.search(r'peak RSS of each cold open with the cache \["(\d+) MiB"\], RSS with the '
                      r"fold open (\d+) MiB", o)
        q = re.search(r"peak RSS while Psalms was projected (\d+) MiB; RSS with the fold open "
                      r"after Psalms was projected (\d+) MiB", o)
        assert r and q, o
        row[1], row[2], row[5], row[6] = r.group(1), r.group(2), q.group(1), q.group(2)
    else:
        r = re.search(r"open with the fold cache ([\d.]+)(m?s), peak RSS (\d+) MiB; RSS with the "
                      r"fold open (\d+) MiB", o)
        q = re.search(r"snapshots (\d+) MiB; RSS with the fold open after Psalms was projected "
                      r"(\d+) MiB", o)
        assert r and q, o
        wall = float(r.group(1)) * (1.0 if r.group(2) == "ms" else 1e3)
        row[0], row[1], row[2] = f"{wall:.0f}", r.group(3), r.group(4)
        row[5], row[6] = q.group(1), q.group(2)
    if case != "first":
        s = re.search(r"\| Psalms projection: [^|]*\| \d+ \| ([\d.]+) ms \| ([\d.]+) ms \|", o)
        assert s, o
        row[3], row[4] = s.group(1), s.group(2)
    if case == "first" and row[0] == "-":
        raise AssertionError(o)
    return row + [l0, l1]


ways = [(b, c, a) for a in ("glibc", "jemalloc") for c in ("first", "psalms", "app")
        for b in ("before", "after") if not (c == "app" and b == "before")]
binaries = {"before": before, "after": after}
with open(out, "a") as f:
    for rnd in range(rounds):
        for build, case, allocator in (ways if rnd % 2 == 0 else ways[::-1]):
            row = run(binaries[build], case, allocator)
            line = "\t".join(str(x) for x in [rnd, build, case, allocator] + row)
            print(line, flush=True)
            f.write(line + "\n")
            f.flush()
if os.path.exists(KEPT):
    shutil.move(KEPT, INDEX)
