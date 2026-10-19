#!/usr/bin/env python3
"""Divides the reference laptop's foldbench medians by this machine's, per class of work.

  python3 -I .audit/tools/laptop-ratios.py LAPTOP_EVIDENCE_DIR HERE_DIR OUT.json [--commit SHA]

LAPTOP_EVIDENCE_DIR is docs/evidence/storage-fold-windows-g0-2026-10-17 (rows-warm.tsv,
everything.txt, app-N.txt: foldbench at storage 58b6874 on a Dell Latitude 7490, i5-8350U, no
SHA, no AVX-512, Defender on). HERE_DIR holds what .audit/tools/laptop-ratios.sh wrote on this
machine with the same foldbench binary: c-N.txt, a-N.txt, p-N.txt, app-N.txt, every.txt and
app1h-N.txt (the app's case with one malloc arena). Each ratio is laptop median / this machine's
fastest round (other jobs slow the rest, and the fastest round makes the ratio larger, so the
laptop estimates it gives are not low); a class takes the largest ratio of its rows. Memory is
the laptop's working set with the fold open over this machine's resident set with one heap,
never below 1. Standard library only.
"""
import json
import re
import statistics
import sys
from pathlib import Path


def median(values):
    return statistics.median(values)


def row_ms(text, prefix):
    """The median column of the markdown row that starts with `prefix`."""
    for line in text.splitlines():
        if line.startswith("| " + prefix):
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            return float(cells[2].split()[0])
    return None


def read(path):
    return Path(path).read_text(errors="replace")


def laptop(evidence):
    rows = [l.split("\t") for l in read(evidence / "rows-warm.tsv").splitlines()[1:]]
    out = {}
    out["open_cache_a"] = median([float(r[2]) for r in rows if r[1] == "a_cache_warm"])
    out["open_nocache_c"] = median([float(r[2]) for r in rows if r[1] == "c_nocache_warm"])
    out["psalms_projection"] = median(
        [float(re.search(r"median ([0-9.]+)", r[2]).group(1)) for r in rows if r[1] == "psalms"]
    )
    out["ws_open_mib"] = median(
        [float(re.search(r"ws_open (\d+)", r[2]).group(1)) for r in rows if r[1] == "app"]
    )
    every = read(evidence / "everything.txt")
    out["sha"] = row_ms(every, "SHA-256 of every file once")
    out["read_verify_prepare"] = row_ms(every, "Read, verify and prepare")
    out["full_fold"] = row_ms(every, "Full fold of the prepared changes")
    out["open_cache_every"] = row_ms(every, "Cold open with the fold cache: read the cache")
    out["open_nocache_every"] = row_ms(every, "Cold open without a cache: read, verify, prepare and fold")
    apps = [read(p) for p in sorted(evidence.glob("app-*.txt"))]
    for key, prefix in [
        ("usfm", "Psalms USFM alone"),
        ("chapter", "One Psalms chapter's USFM"),
        ("conflicts", "Conflict list and counts from scratch"),
        ("projection_app", "Psalms projection"),
    ]:
        out[key] = median([row_ms(a, prefix) for a in apps if row_ms(a, prefix) is not None])
    return out


def here_values(here):
    def per_file(pattern, regex):
        vals = []
        for p in sorted(here.glob(pattern)):
            m = re.search(regex, read(p))
            if m:
                vals.append(float(m.group(1)))
        return vals

    out = {}
    out["open_nocache_c"] = per_file("c-*.txt", r"first open without a cache (\d+) ms")
    out["open_cache_a"] = [
        float(m) for p in sorted(here.glob("a-*.err"))
        for m in re.findall(r"cold open with the cache ([0-9.]+)ms", read(p).replace("µs", "us"))
    ] or per_file("a-*.txt", r"Cold open with the fold cache: read the cache[^|]*\| \d+ \| ([0-9.]+) ms")
    out["psalms_projection"] = per_file("p-*.txt", r"Psalms projection: [^|]*\| \d+ \| ([0-9.]+) ms")
    for key, prefix in [
        ("usfm", "Psalms USFM alone"),
        ("chapter", "One Psalms chapter's USFM"),
        ("conflicts", "Conflict list and counts from scratch"),
        ("projection_app", "Psalms projection"),
    ]:
        out[key] = [v for v in (row_ms(read(p), prefix) for p in sorted(here.glob("app-*.txt"))) if v]
    out["usfm"] += [v for v in (row_ms(read(p), "Psalms USFM alone") for p in sorted(here.glob("p-*.txt"))) if v]
    every = read(here / "every.txt")
    out["sha"] = [row_ms(every, "SHA-256 of every file once")]
    out["read_verify_prepare"] = [row_ms(every, "Read, verify and prepare")]
    out["full_fold"] = [row_ms(every, "Full fold of the prepared changes")]
    out["open_cache_every"] = [row_ms(every, "Cold open with the fold cache: read the cache")]
    out["open_nocache_every"] = [row_ms(every, "Cold open without a cache: read, verify, prepare and fold")]
    out["rss_one_heap_mib"] = per_file("app1h-*.txt", r"RSS with the fold open (\d+) MiB")
    return out


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    commit = "unknown"
    if "--commit" in sys.argv:
        commit = sys.argv[sys.argv.index("--commit") + 1]
        args = [a for a in args if a != commit]
    evidence, here, out_path = Path(args[0]), Path(args[1]), Path(args[2])
    lap = laptop(evidence)
    now = here_values(here)
    rows = []

    def add(name, cls, lap_key, here_key=None):
        values = [v for v in now[here_key or lap_key] if v]
        best = min(values)
        rows.append({
            "row": name, "class": cls, "laptop_ms": lap[lap_key], "here_best_ms": best,
            "here_median_ms": median(values), "here_runs": len(values),
            "ratio": lap[lap_key] / best,
        })

    add("SHA-256 of every file once", "sha", "sha")
    add("(a) open with the fold cache, one per process", "open_cache", "open_cache_a")
    add("cold open with the fold cache (every part)", "open_cache", "open_cache_every")
    add("(c) first open without a cache, one per process", "open_nocache", "open_nocache_c")
    add("cold open without a cache (every part)", "open_nocache", "open_nocache_every")
    add("read, verify and prepare", "open_nocache", "read_verify_prepare")
    add("Psalms projection (psalms-only run)", "multi", "psalms_projection")
    add("Psalms projection (app run)", "multi", "projection_app")
    add("full fold of the prepared changes", "multi", "full_fold")
    add("Psalms USFM alone", "single", "usfm")
    add("conflict list and counts from scratch", "single", "conflicts")
    checks = []
    c = lap["chapter"] / min(now["chapter"])
    checks.append({"row": "one Psalms chapter's USFM (microseconds, a check, not in a class)", "ratio": c})
    ratios = {}
    for cls in ("sha", "open_cache", "open_nocache", "multi", "single"):
        ratios[cls] = round(max(r["ratio"] for r in rows if r["class"] == cls), 2)
    one_heap = min(now["rss_one_heap_mib"])
    memory = lap["ws_open_mib"] / one_heap
    ratios["memory"] = round(max(1.0, memory), 2)
    ratios["disk"] = None
    doc = {
        "what": "this machine's speed against the reference laptop, from the laptop's own foldbench",
        "foldbench_commit": "58b6874e92ed993c4a7692eea31e336fa4d29dd0",
        "storage_commit_of_this_run": commit,
        "laptop": "Dell Latitude 7490, i5-8350U 4 cores / 8 threads, 7.9 GiB, WD Blue SATA SSD, Windows 10 19045, Defender on, no SHA, no AVX-512",
        "here": "see machine.txt next to this file",
        "method": "laptop median / this machine's fastest of 5 rounds; a class takes the largest ratio of its rows; memory = laptop working set with the fold open / this machine's RSS with one malloc arena, at least 1; disk has no ratio until the laptop runs `perfreport flush`",
        "rows": rows,
        "checks": checks,
        "memory": {"laptop_ws_open_mib": lap["ws_open_mib"], "here_rss_one_heap_mib": one_heap, "raw_ratio": round(memory, 3)},
        "ratios": ratios,
    }
    out_path.write_text(json.dumps(doc, indent=2) + "\n")
    for r in rows:
        print(f"{r['row']:<55} laptop {r['laptop_ms']:>9.1f}  here best {r['here_best_ms']:>8.1f} median {r['here_median_ms']:>8.1f}  ratio {r['ratio']:.2f}  [{r['class']}]")
    print("classes:", ratios, "memory raw", round(memory, 3))


main()
