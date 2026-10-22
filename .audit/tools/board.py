"""Print the roadmap board from the work-item cards in .audit/roadmap/units and from git.

Run it in the repository: python3 -I .audit/tools/board.py [--check] [--json] [--invariants] [--ref REF]

The cards are the hand-written truth (state, owner, branch, next step); git is the truth about
what is merged. The board is never committed: it is printed, so it cannot go stale.
--check exits 1 when a card breaks the format or contradicts git or another card. Every error
names the invariant it holds, INV-TRACKER-TRUE (FUNNEL.md section 5), or the invariant the rule
belongs to (INV-FUNNEL-FIRST, INV-REVIEWED-BEFORE-MERGE).
"""
import json, os, re, subprocess, sys, time

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

FIELDS = ["id", "title", "track", "lane", "milestone", "state", "owner", "branch", "worktree",
          "depends", "needs_owner", "proves", "no_loss", "size", "merged", "updated", "next",
          "blocked_by"]
# Optional fields. `later` lists what a later step waits for: "S3 from step 4", "H12 from step 2".
OPTIONAL = {"later"}
LISTS = {"depends", "needs_owner", "proves", "later"}
TRACKS = {"A", "B"}
LANES = {"integration", "storage", "app", "checks", "domain", "shell"}
MILESTONES = {"M0", "M1", "M2", "M3", "M4", "M5"}
ACTIVE = ["claimed", "building", "verifying", "fixing", "gating", "merging"]
STATES = ["todo"] + ACTIVE + ["done", "blocked", "parked"]
PROVES = {f"P{n}" for n in range(1, 11)}
# Building a work item starts from its funnel card (FUNNEL.md section 1).
NEEDS_FUNNEL = set(ACTIVE[1:]) | {"done"}
# These funnel-card lines must hold text once a card is being built (INV-FUNNEL-FIRST).
FUNNEL_LINES = re.compile(r"^(\s*)- (Types[^:]*|One entry point[^:]*|Removed[^:]*|Left to checks[^:]*|Proof[^:]*):\s*(.*)$")
LATER = re.compile(r"^(\S+) from step (\d+[a-z]?)$")
STOP_LINE = re.compile(r"STOP until the ruling files? exists? \(([^)]*)\)")
STEP_EVENT = re.compile(r"^step (\d+[a-z]?)$")
# A session counts as gone after this long with no commit (TRACKING.md "pick up an item").
GONE_AFTER_S = 24 * 3600
SWEEP_WEEK_S = 8 * 24 * 3600


def parse_value(key, raw):
    value = raw.split(" #", 1)[0].strip() if not raw.lstrip().startswith("#") else ""
    if key in LISTS:
        if not (value.startswith("[") and value.endswith("]")):
            raise ValueError(f"{key} must be a list like [A, B], not {value!r}")
        inner = value[1:-1].strip()
        return [v.strip().strip('"').strip("'") for v in inner.split(",") if v.strip()] if inner else []
    if key == "no_loss":
        if value not in ("true", "false"):
            raise ValueError(f"no_loss must be true or false, not {value!r}")
        return value == "true"
    return value


def parse_card_text(text):
    """Return (fields, body, errors) for one card's text."""
    errors = []
    m = re.match(r"---\n(.*?)\n---\n(.*)", text, re.S)
    if not m:
        return {}, text, ["has no front matter between two '---' lines at the top"]
    fields = {}
    for n, line in enumerate(m.group(1).splitlines(), 2):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        km = re.match(r"([a-z_]+):\s?(.*)$", line)
        if not km:
            errors.append(f"line {n}: not 'key: value': {line!r}")
            continue
        key, raw = km.group(1), km.group(2)
        if key not in FIELDS and key not in OPTIONAL:
            errors.append(f"line {n}: unknown field {key!r}")
            continue
        if key in fields:
            errors.append(f"line {n}: field {key!r} appears twice")
            continue
        try:
            fields[key] = parse_value(key, raw)
        except ValueError as e:
            errors.append(f"line {n}: {e}")
    for key in FIELDS:
        if key not in fields:
            errors.append(f"missing field {key!r}")
    return fields, m.group(2), errors


def parse_card(path):
    with open(path, encoding="utf-8") as f:
        return parse_card_text(f.read())


def section(body, heading):
    """The text of one `## <heading>` section of a card body, or ''."""
    parts = body.split(f"\n## {heading}", 1)
    return parts[1].split("\n## ", 1)[0] if len(parts) == 2 else ""


def log_lines(body):
    """Each `- <date> | <who> | <event> | ...` line of the Log section as a dict."""
    out = []
    for line in section(body, "Log").splitlines():
        if not line.startswith("- "):
            continue
        cells = [c.strip() for c in line[2:].split("|")]
        if len(cells) < 3:
            continue
        dm = re.match(r"(\d{4}-\d{2}-\d{2})", cells[0])
        out.append({"date": dm.group(1) if dm else "", "who": cells[1], "event": cells[2],
                    "rest": cells[3:]})
    return out


def step_key(s):
    m = re.match(r"(\d+)([a-z]?)$", s)
    return (int(m.group(1)), m.group(2)) if m else (0, "")


def git(*args):
    r = subprocess.run(["git", "-C", REPO, *args], capture_output=True, text=True)
    return r.returncode, r.stdout.strip()


def is_ancestor(sha, ref):
    """True, False, or None when git cannot tell (unknown commit, no repository)."""
    code, _ = git("cat-file", "-e", f"{sha}^{{commit}}")
    if code != 0:
        return None
    code, _ = git("merge-base", "--is-ancestor", sha, ref)
    return code == 0


def load(units_dir, use_git=True):
    """Read every card. An active card whose branch exists here is read from that branch:
    the owner keeps the live copy there (TRACKING.md "Where the truth lives")."""
    cards, problems = {}, []
    for name in sorted(os.listdir(units_dir)):
        if not name.endswith(".md") or name.startswith("_"):
            continue
        path = os.path.join(units_dir, name)
        fields, body, errors = parse_card(path)
        stem = name[:-3]
        from_branch = None
        if use_git and fields.get("state") in ACTIVE and fields.get("branch") not in (None, "", "none"):
            rel = os.path.relpath(path, REPO).replace(os.sep, "/")
            code, text = git("show", f"{fields['branch']}:{rel}")
            if code == 0 and text:
                fields, body, errors = parse_card_text(text + "\n")
                from_branch = fields.get("branch")
        for e in errors:
            problems.append(("error", stem, e))
        if fields.get("id") and fields["id"] != stem:
            problems.append(("error", stem, f"id {fields['id']!r} does not match the file name"))
        fields["_body"] = body
        fields["_from_branch"] = from_branch
        fields["_log"] = log_lines(body)
        cards[stem] = fields
    return cards, problems


def roadmap_depends(roadmap_path, ids):
    """For each ROADMAP.md table row that starts with a card id, the card ids its
    'Depends on' cell names. 'every other card' stands for every card but that one."""
    out = {}
    if not roadmap_path or not os.path.isfile(roadmap_path):
        return out
    with open(roadmap_path, encoding="utf-8") as f:
        lines = f.read().splitlines()
    depends_col = None
    for line in lines:
        if not line.startswith("|"):
            depends_col = None
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] == "ID":
            depends_col = next((i for i, c in enumerate(cells) if c.startswith("Depends")), None)
            continue
        if depends_col is None or not cells or cells[0] not in ids or len(cells) <= depends_col:
            continue
        cell = cells[depends_col]
        if cell.strip() == "every other card":
            out[cells[0]] = {i for i in ids if i != cells[0]}
            continue
        found = {i for i in ids if re.search(rf"(?<![A-Za-z0-9-]){re.escape(i)}(?![A-Za-z0-9-])", cell)}
        out[cells[0]] = found - {cells[0]}
    return out


def validate(cards, ref, rulings_dir, use_git=True, roadmap_path=None, evidence_dir=None, now=None):
    problems = []
    def err(cid, msg, inv="INV-TRACKER-TRUE"): problems.append(("error", cid, f"{inv}: {msg}"))
    def warn(cid, msg): problems.append(("warning", cid, msg))
    rulings = os.listdir(rulings_dir) if os.path.isdir(rulings_dir) else []
    def has_ruling(h): return any(re.search(rf"-{re.escape(h)}(-.*)?\.md$", r) for r in rulings)
    now = time.time() if now is None else now
    for cid, c in cards.items():
        if not c.get("state"):
            continue
        for key, allowed in (("track", TRACKS), ("lane", LANES), ("milestone", MILESTONES),
                             ("state", set(STATES))):
            if c.get(key) and c[key] not in allowed:
                err(cid, f"{key} {c[key]!r} is not one of {sorted(allowed)}")
        for p in c.get("proves", []):
            if p not in PROVES:
                err(cid, f"proves {p!r} is not one of P1..P10")
        for d in c.get("depends", []):
            if d not in cards:
                err(cid, f"depends on {d!r}, which has no card")
        state = c["state"]
        body = c.get("_body", "")
        log = c.get("_log", [])
        if state in ACTIVE and c.get("owner") in (None, "", "none"):
            err(cid, f"state is {state} but owner is none")
        if state == "todo" and c.get("owner") not in (None, "", "none"):
            err(cid, "state is todo but an owner is set: claim it (state claimed) or clear owner")
        if state == "blocked" and c.get("blocked_by") in (None, "", "none"):
            err(cid, "state is blocked but blocked_by is none")
        funnel = section(body, "Funnel card")
        if state in NEEDS_FUNNEL and "INV-" not in funnel:
            err(cid, f"state is {state} but the funnel card names no invariant (INV-...)")
        if state in NEEDS_FUNNEL:
            flines = funnel.splitlines()
            for i, line in enumerate(flines):
                fm = FUNNEL_LINES.match(line)
                if not fm or fm.group(3).strip():
                    continue
                nxt = next((l for l in flines[i + 1:] if l.strip()), "")
                if len(nxt) - len(nxt.lstrip()) <= len(fm.group(1)):
                    err(cid, f"state is {state} but the funnel card's '{fm.group(2)}' is empty", "INV-FUNNEL-FIRST")
        if state == "done":
            if c.get("merged") in (None, "", "none"):
                err(cid, "state is done but merged is none")
            elif use_git:
                anc = is_ancestor(c["merged"], ref)
                if anc is None:
                    warn(cid, f"merged commit {c['merged']} is not in this repository")
                elif not anc:
                    err(cid, f"merged commit {c['merged']} is not an ancestor of {ref}")
            if not any(l["event"] == "verify" for l in log):
                err(cid, f"{cid} is done with no verify line in its log (FUNNEL.md section 8)", "INV-REVIEWED-BEFORE-MERGE")
            for d in c.get("depends", []):
                ds = cards.get(d, {}).get("state")
                if d in cards and ds != "done":
                    err(cid, f"done but depends on {d}, which is {ds}")
        elif c.get("merged") not in (None, "", "none"):
            err(cid, f"merged is set but state is {state}")
        if state in ACTIVE:
            unmet = [d for d in c.get("depends", []) if d in cards and cards[d].get("state") != "done"]
            for d in unmet:
                if state in ("gating", "merging"):
                    err(cid, f"state is {state} with {d} not done: the merge cannot land")
                else:
                    warn(cid, f"active with {d} not done: the merge waits")
        if use_git and state in ACTIVE and c.get("branch") not in (None, "", "none"):
            code, _ = git("rev-parse", "--verify", "--quiet", f"refs/heads/{c['branch']}")
            if code != 0:
                warn(cid, f"branch {c['branch']!r} does not exist here (another machine?)")
            else:
                code, ts = git("log", "-1", "--format=%ct", c["branch"])
                if code == 0 and ts.isdigit():
                    c["_last_commit"] = time.strftime("%Y-%m-%d %H:%M", time.localtime(int(ts)))
                    age = now - int(ts)
                    if age > GONE_AFTER_S:
                        warn(cid, f"claimed by {c.get('owner')} but {c['branch']} last moved "
                                  f"{age / 86400:.1f} days ago: decide whether the session is gone (TRACKING.md \"pick up\")")
        for h in c.get("needs_owner", []):
            if has_ruling(h):
                warn(cid, f"a ruling for {h} exists in rulings/: remove it from needs_owner")
        for ids in STOP_LINE.findall(body):
            for h in [x.strip() for x in ids.split(",")]:
                if has_ruling(h):
                    warn(cid, f"the card says 'STOP until the ruling file exists ({h})' but rulings/ holds one: rewrite the step to the ruling")
        gates = []
        for entry in c.get("later", []):
            lm = LATER.match(entry)
            if not lm:
                err(cid, f"later entry {entry!r} is not '<card or H-id> from step <n>'")
                continue
            what, step = lm.group(1), lm.group(2)
            if what not in cards and not re.match(r"H\d+$", what):
                err(cid, f"later names {what!r}, which is neither a card nor an H-id")
                continue
            gates.append((what, step))
        c["_later"] = gates
        logged = [STEP_EVENT.match(l["event"]).group(1) for l in log if STEP_EVENT.match(l["event"])]
        last_step = max(logged, key=step_key) if logged else None
        for what, step in gates:
            if last_step is None or step_key(last_step) < step_key(step):
                continue
            if what in cards and cards[what].get("state") != "done":
                err(cid, f"logged step {last_step} but {what} is not done")
            if what not in cards and not has_ruling(what):
                err(cid, f"logged step {last_step} but rulings/ has no {what} file")
        if not re.match(r"\d{4}-\d{2}-\d{2}$", c.get("updated", "")):
            err(cid, f"updated {c.get('updated')!r} is not a date YYYY-MM-DD")
        if not log:
            err(cid, "has no log line")
        else:
            last = max((l["date"] for l in log if l["date"]), default="")
            if last and re.match(r"\d{4}-\d{2}-\d{2}$", c.get("updated", "")) and c["updated"] < last:
                warn(cid, f"updated {c['updated']} is older than the last log line ({last})")
    active = [c for c in cards.values() if c.get("state") in ACTIVE]
    for key in ("branch", "worktree"):
        by = {}
        for c in active:
            if c.get(key) not in (None, "", "none"):
                by.setdefault(c[key], []).append(c)
        for value, cs in by.items():
            owners = {x.get("owner") for x in cs}
            if len(owners) > 1:
                a, b = cs[0], cs[1]
                err(a["id"], f"{key} {value} is claimed by {a['id']} ({a.get('owner')}) and {b['id']} ({b.get('owner')})")
    by_lane = {}
    for c in active:
        by_lane.setdefault(c.get("lane"), []).append(c)
    for lane, cs in by_lane.items():
        owners = {x.get("owner") for x in cs}
        if len(owners) > 1:
            warn(cs[0]["id"], f"lane {lane} has two owners at once: " + ", ".join(f"{x['id']} ({x.get('owner')})" for x in cs))
    if "DONE" in cards:
        missing = [i for i in cards if i != "DONE" and i not in cards["DONE"].get("depends", [])]
        if missing:
            err("DONE", "DONE must depend on every other card; missing " + ", ".join(sorted(missing)))
    for cid, wanted in roadmap_depends(roadmap_path, set(cards)).items():
        have = set(cards[cid].get("depends", []))
        if wanted != have:
            err(cid, f"ROADMAP.md's row says it depends on {sorted(wanted)} but the card says {sorted(have)}")
    if evidence_dir is not None:
        folders = sorted(d for d in (os.listdir(evidence_dir) if os.path.isdir(evidence_dir) else []) if re.match(r"\d{4}-\d{2}-\d{2}$", d))
        for m in sorted(MILESTONES):
            cs = [c for c in cards.values() if c.get("milestone") == m]
            if cs and all(c.get("state") == "done" for c in cs):
                last = max(c.get("updated", "") for c in cs)
                if not any(d >= last for d in folders):
                    warn("SWEEP" if "SWEEP" in cards else cs[0]["id"],
                         f"{m} ended: rerun the evaluation into evidence/<date>/ (integration lane)")
    sweep = cards.get("SWEEP")
    if sweep and sweep.get("state") == "done" and sweep.get("_log"):
        last = max((l["date"] for l in sweep["_log"] if l["date"]), default="")
        if last:
            age = now - time.mktime(time.strptime(last, "%Y-%m-%d"))
            if age > SWEEP_WEEK_S:
                warn("SWEEP", f"SWEEP: last weekly line ({last}) is older than 8 days; run the weekly sweep")
    seen, stack = set(), set()
    def visit(cid, path):
        if cid in stack:
            err(cid, "dependency cycle: " + " -> ".join(path + [cid]))
            return
        if cid in seen or cid not in cards:
            return
        stack.add(cid)
        for d in cards[cid].get("depends", []):
            visit(d, path + [cid])
        stack.discard(cid)
        seen.add(cid)
    for cid in cards:
        visit(cid, [])
    return problems


def readiness(cards):
    out = {}
    for cid, c in cards.items():
        state = c.get("state")
        if state != "todo":
            out[cid] = state
            continue
        unmet = []
        for d in c.get("depends", []):
            ds = cards.get(d, {})
            if ds.get("state") == "done":
                continue
            if ds.get("state") == "blocked":
                unmet.append(f"{d} (blocked: {ds.get('blocked_by', '')})")
            elif ds.get("state") == "parked":
                unmet.append(f"{d} (parked: {ds.get('next', '')})")
            else:
                unmet.append(d)
        if c.get("needs_owner"):
            out[cid] = "waits for owner: " + ", ".join(c["needs_owner"])
        elif unmet:
            out[cid] = "waits for " + ", ".join(unmet)
        else:
            gates = c.get("_later") or [tuple(LATER.match(e).groups()) for e in c.get("later", []) if LATER.match(e)]
            if gates:
                first = min(step_key(s) for _, s in gates)
                upto = first[0] - 1 if first[1] == "" else first[0]
                out[cid] = f"ready to step {upto}; then " + ", ".join(f"{w} from step {s}" for w, s in gates)
            else:
                out[cid] = "ready"
    return out


def invariants(cards):
    """Every INV- name with the cards that use it: the registry FUNNEL.md section 1 asks for."""
    out = {}
    for cid, c in cards.items():
        for name in sorted(set(re.findall(r"INV-[A-Z0-9][A-Z0-9-]*", c.get("_body", "")))):
            out.setdefault(name.rstrip("-"), set()).add(cid)
    return out


def render(cards, problems, ref, rulings_dir=None):
    ready = readiness(cards)
    order = {m: i for i, m in enumerate(sorted(MILESTONES))}
    rows = sorted(cards.values(), key=lambda c: (order.get(c.get("milestone"), 9), c.get("lane", ""), c.get("id", "")))
    total = len(cards)
    done = sum(c.get("state") == "done" for c in cards.values())
    _, head = git("rev-parse", "--short", ref)
    out = ["# Board (generated, do not commit)", "",
           f"From the cards in `.audit/roadmap/units` and git at `{ref}` ({head or 'no git'}).", "",
           f"**{done} of {total} work items done.** "
           + ", ".join(f"{m}: {sum(c.get('state') == 'done' for c in cards.values() if c.get('milestone') == m)}"
                       f"/{sum(c.get('milestone') == m for c in cards.values())}" for m in sorted(MILESTONES)), ""]
    for track in sorted(TRACKS):
        r = [c["id"] for c in rows if c.get("track") == track and str(ready.get(c.get("id"))).startswith("ready")]
        out.append(f"**Ready on track {track}:** " + (", ".join(r) if r else "none"))
    active = [c for c in rows if c.get("state") in ACTIVE]
    out += ["", "**Worktrees in use** (never touch one; README \"Rules that never bend\"):"]
    out += [f"- {c['id']} | {c.get('owner', '')} | {c.get('branch', '')} | {c.get('worktree', '')} | last commit {c.get('_last_commit', 'unknown')}"
            for c in active] or ["- none"]
    out += ["", "| Id | Title | Lane | Milestone | State | Owner | Next |", "|---|---|---|---|---|---|---|"]
    for c in rows:
        cid = c.get("id", "?")
        state = ready.get(cid, c.get("state"))
        if c.get("_from_branch"):
            state = f"{state} (from {c['_from_branch']})"
        nxt = c.get("blocked_by") if c.get("state") == "blocked" else c.get("next", "")
        if c.get("state") in ACTIVE and c.get("needs_owner"):
            nxt = f"{nxt} waits for owner: {', '.join(c['needs_owner'])}"
        out.append(f"| {cid} | {c.get('title', '')} | {c.get('lane', '')} | {c.get('milestone', '')} | "
                   f"{state} | {c.get('owner', '')} | {nxt} |")
    if rulings_dir and os.path.isdir(rulings_dir):
        out += ["", "## Rulings (`rulings/`)", ""]
        out += [f"- {r}" for r in sorted(os.listdir(rulings_dir)) if r.endswith(".md")] or ["- none"]
    errs = [p for p in problems if p[0] == "error"]
    warns = [p for p in problems if p[0] == "warning"]
    out += ["", f"## Contradictions ({len(errs)} errors, {len(warns)} warnings)", ""]
    out += [f"- {kind}: {cid}: {msg}" for kind, cid, msg in errs + warns] or ["- none"]
    return "\n".join(out) + "\n"


def main(argv):
    check = "--check" in argv
    as_json = "--json" in argv
    ref = argv[argv.index("--ref") + 1] if "--ref" in argv else "HEAD"
    roadmap = os.path.join(REPO, ".audit", "roadmap")
    units_dir = os.path.join(roadmap, "units")
    rulings_dir = os.path.join(roadmap, "rulings")
    cards, problems = load(units_dir)
    problems += validate(cards, ref, rulings_dir, roadmap_path=os.path.join(roadmap, "ROADMAP.md"),
                         evidence_dir=os.path.join(roadmap, "evidence"))
    if "--invariants" in argv:
        for name, ids in sorted(invariants(cards).items()):
            print(f"{name}\t{', '.join(sorted(ids))}")
        return 0
    if as_json:
        ready = readiness(cards)
        print(json.dumps({"ref": ref, "items": [{k: v for k, v in c.items() if not k.startswith("_")}
                                                 | {"board_state": ready.get(cid)} for cid, c in cards.items()],
                          "problems": [{"kind": k, "id": i, "message": m} for k, i, m in problems]}, indent=2))
    elif check:
        for kind, cid, msg in problems:
            print(f"{kind}: {cid}: {msg}")
        print(f"{len(cards)} cards, {sum(p[0] == 'error' for p in problems)} errors, "
              f"{sum(p[0] == 'warning' for p in problems)} warnings")
    else:
        sys.stdout.write(render(cards, problems, ref, rulings_dir))
    return 1 if check and any(p[0] == "error" for p in problems) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
