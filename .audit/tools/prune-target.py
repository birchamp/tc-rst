"""Remove build artifacts that a newer build of the same target has superseded.

Usage: python3 -I .audit/tools/prune-target.py <worktree>/target [min_age_minutes]

Cargo keeps every hashed artifact it ever built, so a worktree that builds for days fills the
disk (25 GB for the storage worktree on 2026-10-17). In target/*/deps and target/*/examples,
files that share a name and extension differ only by hash; this keeps the newest of each and
removes the rest when they are older than min_age_minutes (default 30), so a build that is
running keeps what it just made. Run it only while no rustc process is compiling in that
worktree. A removed artifact that is still wanted is rebuilt.
"""
import collections, os, re, sys, time

root = sys.argv[1]
min_age = 60 * float(sys.argv[2]) if len(sys.argv) > 2 else 1800
now = time.time()
freed = removed = 0
for profile in os.listdir(root):
    for sub in ("deps", "examples"):
        d = os.path.join(root, profile, sub)
        if not os.path.isdir(d):
            continue
        groups = collections.defaultdict(list)
        for f in os.listdir(d):
            p = os.path.join(d, f)
            m = re.match(r"^(.*)-([0-9a-f]{16})(\..*)?$", f)
            if not m or not os.path.isfile(p):
                continue
            st = os.stat(p)
            groups[(m.group(1), m.group(3) or "")].append((st.st_mtime, st.st_size, p))
        for files in groups.values():
            files.sort(reverse=True)
            for mtime, size, p in files[1:]:
                if now - mtime >= min_age:
                    os.remove(p)
                    freed += size
                    removed += 1
print(f"removed {removed} files, {freed / 1e9:.2f} GB")
