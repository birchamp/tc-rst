"""Interleaved opens of a corpus saved by `foldbench save`, one open per process.

usage: python3 -I opens.py CORPUS FOLDBENCH OTHER_FOLDBENCH ROUNDS OUT.tsv

FOLDBENCH is the build measured; OTHER_FOLDBENCH another build of the same fold cache
version (INDEX_VERSION). Each round:

  c_nocache_warm             no cache, every file read once just before: the first open
                             (writes CORPUS/index.bin)
  a_cache_warm               the cache, files in the page cache
  b_cache_cold               the cache, files and cache evicted from the page cache
  c_nocache_cold             no cache, files evicted (writes the cache again, which must
                             equal the first byte for byte)
  other_build_nocache_warm   the other build's first open, which writes its own cache; the
                             round fails unless that cache equals FOLDBENCH's byte for byte
  d_other_builds_cache_warm  FOLDBENCH opens with the other build's cache

Each row: round, mode, wall ms, the fold's work after the last file ms, CPU ms over all
threads, peak RSS MiB, RSS with the fold open MiB (opens with the cache), the 1-minute load
average before and after, and the page-cache check.
"""
import os
import re
import subprocess
import sys
import time

corpus, head, other, rounds, out = (sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]),
                                    sys.argv[5])
index = os.path.join(corpus, "index.bin")
cachectl = [sys.executable, "-I", os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                               "cachectl.py"), corpus]


def load():
    return float(open("/proc/loadavg").read().split()[0])


def ms(text):
    m = re.fullmatch(r"([\d.]+)(ns|µs|us|ms|s)", text)
    value, unit = float(m.group(1)), m.group(2)
    return value * {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1e3}[unit]


def warm():
    return subprocess.run(cachectl + ["read"], capture_output=True, text=True).stdout.strip()


def evict():
    subprocess.run(cachectl + ["evict"], check=True)
    resident = subprocess.run(cachectl + ["resident"], capture_output=True, text=True).stdout
    assert resident.startswith("resident 0 "), resident
    return resident.strip()


def first(binary):
    env = dict(os.environ, FOLDBENCH_FIRST_ONLY="1", FOLDBENCH_RUNS="1")
    before = load()
    p = subprocess.run([binary, "run", corpus, "4"], capture_output=True, text=True, env=env)
    after = load()
    assert p.returncode == 0, p.stderr
    m = re.search(r"first open without a cache (\d+) ms, the fold after the last file (\d+) ms, "
                  r"cpu over all threads (\d+) ms, peak RSS (\d+) MiB", p.stdout)
    assert m, p.stdout + p.stderr
    return dict(wall=float(m.group(1)), tail=float(m.group(2)), cpu=float(m.group(3)),
                peak=float(m.group(4)), rss="", before=before, after=after)


def cached(binary):
    env = dict(os.environ, FOLDBENCH_CACHED_ONLY="1", FOLDBENCH_RUNS="1")
    before = load()
    p = subprocess.run([binary, "run", corpus, "4"], capture_output=True, text=True, env=env)
    after = load()
    assert p.returncode == 0, p.stderr
    assert "not used" not in p.stderr, p.stderr
    m = re.search(r"cold open with the cache (\S+), the fold after the last file (\S+), cpu over "
                  r"all threads (\d+) ms", p.stderr)
    r = re.search(r"peak RSS of each cold open with the cache \[(\d+)\] MiB, RSS with the fold "
                  r"open (\d+) MiB", p.stdout)
    assert m and r, p.stdout + p.stderr
    return dict(wall=ms(m.group(1)), tail=ms(m.group(2)), cpu=float(m.group(3)),
                peak=float(r.group(1)), rss=float(r.group(2)), before=before, after=after)


def remove_index():
    if os.path.exists(index):
        os.remove(index)


with open(out, "a") as f:
    def record(rnd, mode, row, note=""):
        line = "\t".join(str(x) for x in [rnd, mode, f"{row['wall']:.0f}", f"{row['tail']:.0f}",
                                          f"{row['cpu']:.0f}", row["peak"], row["rss"],
                                          row["before"], row["after"], note])
        print(line, flush=True)
        f.write(line + "\n")
        f.flush()

    for rnd in range(rounds):
        remove_index()
        w = warm()
        record(rnd, "c_nocache_warm", first(head), w)
        head_index = open(index, "rb").read()

        w = warm()
        record(rnd, "a_cache_warm", cached(head), w)

        e = evict()
        record(rnd, "b_cache_cold", cached(head), e)

        remove_index()
        e = evict()
        record(rnd, "c_nocache_cold", first(head), e)
        assert open(index, "rb").read() == head_index

        remove_index()
        w = warm()
        record(rnd, "other_build_nocache_warm", first(other), w)
        same = open(index, "rb").read() == head_index
        assert same, "the other build's cache differs"
        w = warm()
        record(rnd, "d_other_builds_cache_warm", cached(head), f"index identical: {same}; {w}")
        time.sleep(2)
