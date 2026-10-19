"""Usage: python3 -I -u .audit/tools/watch-runs.py <workflows dir> '{"wf_...": "lane"}'

Emit one line per finished gate, verify, merge or mark agent in the two long workflows,
plus new Windows branches on origin and low disk. Run with python3 -I -u."""
import json, os, subprocess, sys, time

WF = sys.argv[1]  # the session's subagents/workflows folder
RUNS = json.loads(sys.argv[2])  # {"<run id>": "<label>"}
WATCH = ("gate:", "gate2:", "verify:", "merge:", "mark:", "review:", "check:", "fix:", "push:", "build:", "prove:")
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

CACHE = os.path.expanduser("~/.cache/tc-watch"); os.makedirs(CACHE, exist_ok=True)
STATE = os.path.join(CACHE, "watch-state.json")
labels, offsets = {}, {}
try:
    reported = set(json.load(open(STATE)))
    first = False
except (OSError, ValueError):
    reported = set()
    first = True

def save():
    tmp = STATE + ".tmp"
    with open(tmp, "w") as f:
        json.dump(sorted(reported), f)
    os.replace(tmp, STATE)

def remote_windows():
    try:
        out = subprocess.run(["git", "-C", REPO, "ls-remote", "--heads", "origin"], capture_output=True, text=True, timeout=60).stdout
    except Exception:
        return None
    return {l.split()[1]: l.split()[0][:7] for l in out.splitlines() if "windows-memory" in l or "windows-hazards" in l}


def busy_paths():
    busy = set()
    for pid in os.listdir('/proc'):
        if not pid.isdigit(): continue
        try:
            exe = os.path.basename(os.readlink(f'/proc/{pid}/exe'))
            if not exe.startswith(('cargo', 'rustc', 'rustdoc', 'cc', 'ld', 'node')): continue
            busy.add(os.readlink(f'/proc/{pid}/cwd'))
            busy.update(a for a in open(f'/proc/{pid}/cmdline').read().split('\0') if '/target' in a)
        except OSError:
            pass
    return busy

def du(path):
    total = 0
    for d, _, fs in os.walk(path):
        for f in fs:
            try: total += os.lstat(os.path.join(d, f)).st_size
            except OSError: pass
    return total

def relocate_idle(free):
    """Move idle build folders (over 5 GB, not yet links) to the root drive, where /var/tmp has room."""
    import glob, shutil
    busy = busy_paths()
    base = "/var/tmp/tc-rst-target"
    os.makedirs(base, mode=0o700, exist_ok=True)
    moved = []
    for t in [REPO + "/target"] + glob.glob(REPO + "/.claude/worktrees/*/target"):
        root = os.path.dirname(t)
        if os.path.islink(t) or not os.path.isdir(t): continue
        if any(b == root or b.startswith(root + '/') for b in busy): continue
        size = du(t)
        if size < 5 * 2**30: continue
        name = "main" if root == REPO else os.path.basename(root)
        shutil.rmtree(t, ignore_errors=True)
        os.makedirs(os.path.join(base, name), exist_ok=True)
        os.symlink(os.path.join(base, name), t)
        moved.append(f"{name} ({size // 2**30} GB)")
    return moved

WIN_DIR = CACHE
WIN_STATE = os.path.join(WIN_DIR, "win-tested.txt")
WIN_RUN = os.path.join(WIN_DIR, "win-run.json")
WIN_LOG = os.path.join(WIN_DIR, "win-gates.log")

def unit_running(unit):
    """A VM run is a transient systemd user unit, so it outlives this watcher's restarts."""
    try:
        state = subprocess.run(["systemctl", "--user", "is-active", unit], capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception:
        return True
    return state in ("active", "activating", "deactivating")

def windows_gate_tick():
    """Run the Windows gates in the VM on the newest main, one detached run at a time; report each result once."""
    if os.path.exists(WIN_RUN):
        run = json.load(open(WIN_RUN))
        if unit_running(run["unit"]): return
        out = open(WIN_LOG).read() if os.path.exists(WIN_LOG) else ""
        lines = [l for l in out.splitlines() if l.strip() and not any(k in l for k in ("store now", "post-quantum", "upgraded", "pq.html"))]
        import re as _re
        gate_lines = [l.strip() for l in lines if _re.match(r"^(fmt|clippy|test|wasm)\s+exit\s", l.strip()) or l.strip().startswith("tests:")]
        summary = "; ".join(gate_lines)
        ok = any(l.startswith("tests:") and ", 0 failed" in l for l in gate_lines) and all(" exit 0 " in l for l in gate_lines if not l.startswith("tests:"))
        fails = [l.strip() for l in lines if _re.search(r"panicked at|^error|FAILED$|\.\.\. FAILED", l.strip())][:8]
        print(f"WINDOWS {run['sha'][:7]} {'green' if ok else 'RED'}: {summary}" + ("" if ok else " | " + " / ".join(fails)), flush=True)
        open(WIN_STATE, "w").write(run["sha"]); os.remove(WIN_RUN)
        return
    try:
        sha = subprocess.run(["git", "-C", REPO, "rev-parse", "origin/claude/focused-ride-5uzj7p"], capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception:
        return
    last = open(WIN_STATE).read().strip() if os.path.exists(WIN_STATE) else ""
    if not sha or sha == last: return
    if last:
        changed = subprocess.run(["git", "-C", REPO, "diff", "--name-only", last, sha], capture_output=True, text=True, timeout=60).stdout.split()
        if not any(f.endswith((".rs", ".toml", ".lock", ".ps1")) or "/build-app" in f for f in changed):
            open(WIN_STATE, "w").write(sha)  # docs only since the last run: nothing for Windows to test
            return
    unit = f"tc-wingate-{sha[:10]}"
    subprocess.run(["systemd-run", "--user", "--collect", f"--unit={unit}", f"--property=StandardOutput=file:{WIN_LOG}", "--property=StandardError=inherit",
                    REPO + "/.audit/tools/winvm/run.sh", sha, "powershell -NoProfile -ExecutionPolicy Bypass -File .audit\\tools\\winvm\\gates.ps1"],
                   capture_output=True, text=True, timeout=60)
    json.dump({"unit": unit, "sha": sha}, open(WIN_RUN, "w"))

STATUS_SEEN = {"sha": None}

def status_tick():
    """Regenerate .audit/STATUS.md on each new main and push it, from a worktree of its own."""
    try:
        subprocess.run(["git", "-C", REPO, "fetch", "-q", "origin"], timeout=120)
        sha = subprocess.run(["git", "-C", REPO, "rev-parse", "origin/claude/focused-ride-5uzj7p"], capture_output=True, text=True, timeout=30).stdout.strip()
        if not sha or sha == STATUS_SEEN["sha"]: return
        wt = REPO + "/.claude/worktrees/status"
        if not os.path.isdir(wt):
            subprocess.run(["git", "-C", REPO, "worktree", "add", "-q", "--detach", wt, sha], timeout=120, check=True)
        g = lambda *a: subprocess.run(["git", "-C", wt, *a], capture_output=True, text=True, timeout=120)
        g("checkout", "-q", "--force", "--detach", sha)
        out = subprocess.run(["python3", "-I", wt + "/.audit/tools/status.py", sha], capture_output=True, text=True, timeout=120).stdout.strip()
        if out == "changed":
            g("add", ".audit/STATUS.md")
            g("-c", "user.name=Claude", "-c", "user.email=noreply@anthropic.com", "commit", "-q", "-m",
              "Regenerate the status list\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>")
            r = g("push", "-q", "origin", "HEAD:claude/focused-ride-5uzj7p")
            if r.returncode != 0: return  # main moved meanwhile; the next tick tries again
        STATUS_SEEN["sha"] = sha
    except Exception as e:
        print(f"STATUS error: {e}", flush=True)

rseen = remote_windows() or {}
last_prune = 0
i = 0
while True:
    for run, name in RUNS.items():
        path = os.path.join(WF, run, "journal.jsonl")
        try:
            with open(path) as f:
                f.seek(offsets.get(path, 0))
                for line in f:
                    if not line.endswith("\n"):
                        break
                    offsets[path] = offsets.get(path, 0) + len(line.encode())
                    try:
                        r = json.loads(line)
                    except ValueError:
                        continue
                    if r.get("type") == "started":
                        labels[r.get("agentId")] = r.get("label", "")
                    elif r.get("type") == "result":
                        key = run + ":" + str(r.get("agentId"))
                        if key in reported:
                            continue
                        reported.add(key)
                        if first:
                            continue
                        lab = labels.get(r.get("agentId"), "?")
                        if lab.startswith(WATCH):
                            v = r.get("result")
                            if isinstance(v, dict):
                                keys = {k: v[k] for k in ("green", "verdict", "merged", "ok", "pushed") if k in v}
                                n = len(v.get("problems", v.get("findings", [])) or [])
                                print(f"{name} {lab} {json.dumps(keys)} issues={n}", flush=True)
                            else:
                                print(f"{name} {lab} done: {str(v)[:120]!r}", flush=True)
                    elif r.get("type") in ("completed", "failed", "aborted") and not first:
                        print(f"{name} workflow {r.get('type')}", flush=True)
        except FileNotFoundError:
            pass
    first = False
    save()
    if i % 15 == 0:
        windows_gate_tick()
        status_tick()
        r = remote_windows()
        if r is not None:
            for ref, sha in r.items():
                if rseen.get(ref) != sha:
                    print(f"REMOTE {ref} {sha}", flush=True)
            rseen = r
        st = os.statvfs("/home")
        free = st.f_bavail * st.f_frsize // 2**30
        if free < 100:
            moved = relocate_idle(free)
            if moved:
                st = os.statvfs('/home'); now_free = st.f_bavail * st.f_frsize // 2**30
                print(f"DISK {free}G free on /home: moved idle build folders to /var/tmp: {', '.join(moved)}; now {now_free}G", flush=True)
                free = now_free
        if free < 60 and time.time() - last_prune > 1800:
            last_prune = time.time()
            import glob
            targets = [REPO + "/target"] + glob.glob(REPO + "/.claude/worktrees/*/target")
            freed = []
            busy = set()
            for pid in os.listdir('/proc'):
                if not pid.isdigit(): continue
                try:
                    exe = os.path.basename(os.readlink(f'/proc/{pid}/exe'))
                    if not exe.startswith(('cargo', 'rustc', 'rustdoc', 'cc', 'ld', 'node')): continue
                    busy.add(os.readlink(f'/proc/{pid}/cwd'))
                    busy.update(a for a in open(f'/proc/{pid}/cmdline').read().split('\0') if '/target' in a)
                except OSError:
                    pass
            for t in targets:
                root = os.path.dirname(t)
                if any(b == root or b.startswith(root + '/') for b in busy):
                    freed.append(os.path.basename(root) + ': skipped, a build is running'); continue
                try:
                    out = subprocess.run(["python3", "-I", REPO + "/.audit/tools/prune-target.py", t], capture_output=True, text=True, timeout=900).stdout.strip().splitlines()
                    if out: freed.append(os.path.basename(os.path.dirname(t)) + ": " + out[-1])
                except Exception as e:
                    freed.append(f"{t}: {e}")
            st = os.statvfs("/home")
            after = st.f_bavail * st.f_frsize // 2**30
            print(f"DISK was {free}G free; pruned ({'; '.join(freed)}); now {after}G", flush=True)
            if after < 40:
                print(f"DISK still only {after}G free after pruning: act", flush=True)
    i += 1
    time.sleep(20)
