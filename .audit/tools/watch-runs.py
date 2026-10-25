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

def free_gb(path):
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize // 2**30

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
        subprocess.run(["git", "-C", REPO, "fetch", "-q", "origin"], timeout=120)
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
        r = remote_windows()
        if r is not None:
            for ref, sha in r.items():
                if rseen.get(ref) != sha:
                    print(f"REMOTE {ref} {sha}", flush=True)
            rseen = r
        # Builds live on /home, but the root drive filled once (2026-10-21), so watch both.
        drive = "/" if free_gb("/") < free_gb("/home") else "/home"
        free = free_gb(drive)
        if free < 60 and time.time() - last_prune > 1800:
            last_prune = time.time()
            import glob
            targets = [REPO + "/target"] + glob.glob(REPO + "/.claude/worktrees/*/target") + glob.glob(REPO + "/.worktrees/*/target")
            freed = []
            busy = busy_paths()
            for t in targets:
                root = os.path.dirname(t)
                if any(b == root or b.startswith(root + '/') for b in busy):
                    freed.append(os.path.basename(root) + ': skipped, a build is running'); continue
                try:
                    out = subprocess.run(["python3", "-I", REPO + "/.audit/tools/prune-target.py", t], capture_output=True, text=True, timeout=900).stdout.strip().splitlines()
                    if out: freed.append(os.path.basename(os.path.dirname(t)) + ": " + out[-1])
                except Exception as e:
                    freed.append(f"{t}: {e}")
            after = free_gb(drive)
            print(f"DISK {drive} was {free}G free; pruned ({'; '.join(freed)}); now {after}G", flush=True)
            if after < 40:
                print(f"DISK {drive} still only {after}G free after pruning: act", flush=True)
    i += 1
    time.sleep(20)
