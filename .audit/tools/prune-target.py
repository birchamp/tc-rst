"""Remove build artifacts that no build still needs.

Usage: python3 -I .audit/tools/prune-target.py <target dir> [min_age_minutes]

Cargo keeps every hashed artifact it ever built, so a target that builds for days fills the
disk (25 GB for the storage worktree on 2026-10-17). In target/*/deps and target/*/examples
this removes two kinds of files:

- Orphans: every file of an artifact whose dep-info (.d) names a worktree under
  .claude/worktrees that no longer exists, whatever its age.
- Superseded files: when files share a name and extension and differ only by hash, every one
  but the newest, once it is older than min_age_minutes (default and floor: 180). One crate
  can have several live builds at once (another profile, other features), and removing one
  while it is in use made rustc pick the wrong crate (2026-10-18); three hours keeps them.

A removed artifact that is still wanted is rebuilt.
"""
import collections, os, re, sys, time

WORKTREE = re.compile(r"(/home/user/tc-rst/\.claude/worktrees/[^/\s]+)/")

root = sys.argv[1]
min_age = 60 * max(float(sys.argv[2]) if len(sys.argv) > 2 else 180, 180)
now = time.time()
freed = removed = 0


def remove(p):
    global freed, removed
    try:
        size = os.stat(p).st_size
        os.remove(p)
    except FileNotFoundError:
        return  # a build running beside this one removed it first
    freed += size
    removed += 1


def orphaned(dep_info):
    try:
        with open(dep_info, encoding="utf-8", errors="replace") as f:
            m = WORKTREE.search(f.read(4096))
    except OSError:
        return False
    return bool(m) and not os.path.isdir(m.group(1))


for profile in os.listdir(root):
    for sub in ("deps", "examples"):
        d = os.path.join(root, profile, sub)
        if not os.path.isdir(d):
            continue
        stems = collections.defaultdict(list)
        for f in os.listdir(d):
            m = re.match(r"^(.*-[0-9a-f]{16})(\..*)?$", f)
            if m:
                stems[m.group(1)].append(os.path.join(d, f))
        for stem, files in stems.items():
            if orphaned(os.path.join(d, stem + ".d")):
                for p in files:
                    remove(p)
        groups = collections.defaultdict(list)
        for f in os.listdir(d):
            p = os.path.join(d, f)
            m = re.match(r"^(.*)-([0-9a-f]{16})(\..*)?$", f)
            if not m:
                continue
            try:
                st = os.stat(p)
            except FileNotFoundError:
                continue
            groups[(m.group(1), m.group(3) or "")].append((st.st_mtime, p))
        for files in groups.values():
            files.sort(reverse=True)
            for mtime, p in files[1:]:
                if now - mtime >= min_age:
                    remove(p)
print(f"removed {removed} files, {freed / 1e9:.2f} GB")
