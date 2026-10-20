#!/usr/bin/env python3
"""Adds rebuild-only interface text to crates/tc-ui/assets/i18n-rebuild.

Usage: add-rebuild-keys.py KEYS.json

KEYS.json is one object from a dotted key to its English text. Each key goes into en.json and
into every language's pending list, both kept sorted, as `tc-api/tests/refusals.rs` expects.
A key already in en.json is replaced in en.json only.
"""
import json
import pathlib
import sys

root = pathlib.Path(__file__).resolve().parents[2] / "crates/tc-ui/assets/i18n-rebuild"


def write(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


new = json.loads(pathlib.Path(sys.argv[1]).read_text())
english = json.loads((root / "en.json").read_text())
fresh = [key for key in new if key not in english]
english.update(new)
write(root / "en.json", dict(sorted(english.items())))
for pending in sorted(root.glob("*.pending.json")):
    keys = json.loads(pending.read_text())
    keys = sorted(set(keys) | set(fresh))
    write(pending, keys)
