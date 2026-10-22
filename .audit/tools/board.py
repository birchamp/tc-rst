"""Print the roadmap board from the work-item cards in .audit/roadmap/units and from git.

Run it in the repository: python3 -I .audit/tools/board.py [--check] [--json] [--ref REF]

The cards are the hand-written truth (state, owner, branch, next step); git is the truth about
what is merged. The board is never committed: it is printed, so it cannot go stale.
--check exits 1 when a card breaks the format or contradicts git or another card.
"""
import json, os, re, subprocess, sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

FIELDS = ["id", "title", "track", "lane", "milestone", "state", "owner", "branch", "worktree",
          "depends", "needs_owner", "proves", "no_loss", "size", "merged", "updated", "next",
          "blocked_by"]
LISTS = {"depends", "needs_owner", "proves"}
TRACKS = {"A", "B"}
LANES = {"integration", "storage", "app", "checks", "domain", "shell"}
MILESTONES = {"M0", "M1", "M2", "M3", "M4", "M5"}
ACTIVE = ["claimed", "building", "verifying", "fixing", "gating", "merging"]
STATES = ["todo"] + ACTIVE + ["done", "blocked", "parked"]
PROVES = {f"P{n}" for n in range(1, 11)}
# Building a work item starts from its funnel card (FUNNEL.md section 1).
NEEDS_FUNNEL = set(ACTIVE[1:]) | {"done"}


def parse_value(key, raw):
    value = raw.split(" #", 1)[0].strip() if not raw.lstrip().startswith("#") else ""
    if key in LISTS:
        if not (value.startswith("[") and value.endswith("]")):
            raise ValueError(f"{key} must be a list like [A, B], not {value!r}")
        inner = value[1:-1].strip()
        return [v.strip() for v in inner.split(",") if v.strip()] if inner else []
    if key == "no_loss":
        if value not in ("true", "false"):
            raise ValueError(f"no_loss must be true or false, not {value!r}")
        return value == "true"
    return value


def parse_card(path):
    """Return (fields, body, errors) for one card file."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
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
        if key not in FIELDS:
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


def load(units_dir):
    cards, problems = {}, []
    for name in sorted(os.listdir(units_dir)):
        if not name.endswith(".md") or name.startswith("_"):
            continue
        path = os.path.join(units_dir, name)
        fields, body, errors = parse_card(path)
        stem = name[:-3]
        for e in errors:
            problems.append(("error", stem, e))
        if fields.get("id") and fields["id"] != stem:
            problems.append(("error", stem, f"id {fields['id']!r} does not match the file name"))
        fields["_body"] = body
        cards[stem] = fields
    return cards, problems


def validate(cards, ref, rulings_dir, use_git=True):
    problems = []
    def err(cid, msg): problems.append(("error", cid, msg))
    def warn(cid, msg): problems.append(("warning", cid, msg))
    rulings = os.listdir(rulings_dir) if os.path.isdir(rulings_dir) else []
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
        if state in ACTIVE and c.get("owner") in (None, "", "none"):
            err(cid, f"state is {state} but owner is none")
        if state == "todo" and c.get("owner") not in (None, "", "none"):
            err(cid, "state is todo but an owner is set: claim it (state claimed) or clear owner")
        if state == "blocked" and c.get("blocked_by") in (None, "", "none"):
            err(cid, "state is blocked but blocked_by is none")
        if state in NEEDS_FUNNEL and "INV-" not in c.get("_body", "").split("## Funnel card", 1)[-1].split("\n## ", 1)[0]:
            err(cid, f"state is {state} but the funnel card names no invariant (INV-...)")
        if state == "done":
            if c.get("merged") in (None, "", "none"):
                err(cid, "state is done but merged is none")
            elif use_git:
                anc = is_ancestor(c["merged"], ref)
                if anc is None:
                    warn(cid, f"merged commit {c['merged']} is not in this repository")
                elif not anc:
                    err(cid, f"merged commit {c['merged']} is not an ancestor of {ref}")
        elif c.get("merged") not in (None, "", "none"):
            err(cid, f"merged is set but state is {state}")
        if use_git and state in ACTIVE and c.get("branch") not in (None, "", "none"):
            code, _ = git("rev-parse", "--verify", "--quiet", f"refs/heads/{c['branch']}")
            if code != 0:
                warn(cid, f"branch {c['branch']!r} does not exist here (another machine?)")
        for h in c.get("needs_owner", []):
            if any(re.search(rf"-{re.escape(h)}(-.*)?\.md$", r) for r in rulings):
                warn(cid, f"a ruling for {h} exists in rulings/: remove it from needs_owner")
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
        unmet = [d for d in c.get("depends", []) if cards.get(d, {}).get("state") != "done"]
        if c.get("needs_owner"):
            out[cid] = "waits for owner: " + ", ".join(c["needs_owner"])
        elif unmet:
            out[cid] = "waits for " + ", ".join(unmet)
        else:
            out[cid] = "ready"
    return out


def render(cards, problems, ref):
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
        r = [c["id"] for c in rows if c.get("track") == track and ready.get(c.get("id")) == "ready"]
        out.append(f"**Ready on track {track}:** " + (", ".join(r) if r else "none"))
    out += ["", "| Id | Title | Lane | Milestone | State | Owner | Next |", "|---|---|---|---|---|---|---|"]
    for c in rows:
        cid = c.get("id", "?")
        state = ready.get(cid, c.get("state"))
        nxt = c.get("blocked_by") if c.get("state") == "blocked" else c.get("next", "")
        out.append(f"| {cid} | {c.get('title', '')} | {c.get('lane', '')} | {c.get('milestone', '')} | "
                   f"{state} | {c.get('owner', '')} | {nxt} |")
    errs = [p for p in problems if p[0] == "error"]
    warns = [p for p in problems if p[0] == "warning"]
    out += ["", f"## Contradictions ({len(errs)} errors, {len(warns)} warnings)", ""]
    out += [f"- {kind}: {cid}: {msg}" for kind, cid, msg in errs + warns] or ["- none"]
    return "\n".join(out) + "\n"


def main(argv):
    check = "--check" in argv
    as_json = "--json" in argv
    ref = argv[argv.index("--ref") + 1] if "--ref" in argv else "HEAD"
    units_dir = os.path.join(REPO, ".audit", "roadmap", "units")
    rulings_dir = os.path.join(REPO, ".audit", "roadmap", "rulings")
    cards, problems = load(units_dir)
    problems += validate(cards, ref, rulings_dir)
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
        sys.stdout.write(render(cards, problems, ref))
    return 1 if check and any(p[0] == "error" for p in problems) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
