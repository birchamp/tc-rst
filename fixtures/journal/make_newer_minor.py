"""Writes newer-minor.ndjson: change lines as a newer translationCore (format minor 1) and a
malformed minor-0 writer would write them. Each line is canonical JSON (keys sorted, no
whitespace, raw UTF-8), a tab, and the SHA-256 of the JSON bytes in lowercase hex.

Run from the repository root:  python3 -I fixtures/journal/make_newer_minor.py
"""

import base64
import hashlib
import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent


def device(kind, fill):
    raw = bytes([kind]) + bytes([fill]) * 15
    return "d" + base64.b32encode(raw).decode().lower().rstrip("=")


LOCAL = device(1, 1)
BATCH = {"id": "11" * 16, "n": 1}
TEXT = {
    "last": 1,
    "layout": "\n\\p\n",
    "marker": {"lead": " ", "term": "space"},
    "words": "Pablo, siervo de Dios",
}


def change(v, record, cause=None, seq=1, **extra):
    body = {
        "at": 1760520000000,
        "batch": dict(BATCH),
        "cause": cause if cause is not None else {"edit": {}},
        "device": LOCAL,
        "parents": [],
        "record": record,
        "seq": seq,
        "v": v,
    }
    body.update(extra)
    return body


def verse(value):
    return {"verse": {"book": "TIT", "chapter": 1, "value": value, "verse": 1}}


CASES = [
    # 1. A known kind with fields minor 0 does not know, at three levels.
    change(1, verse({"text": dict(TEXT, note="a newer field")}), sig="ed25519:…",
           batch={"id": "11" * 16, "n": 1, "origin": "x"}),
    # 2. The same record and value as minor 0 writes it.
    change(0, verse({"text": dict(TEXT)}), seq=2),
    # 3. A record kind minor 0 does not know.
    change(1, {"noteThread": {"book": "TIT", "thread": "a1", "value": {"title": "Pablo"}}}, seq=3),
    # 4. A record kind minor 0 does not know, without a value member.
    change(1, {"noteThread": {"book": "TIT", "thread": "a2"}}, seq=4),
    # 5. A cause minor 0 does not know.
    change(1, verse({"text": dict(TEXT)}), cause={"signedEdit": {"by": "jdoe"}}, seq=5),
    # 6. A known cause with a source variant minor 0 does not know.
    change(1, verse({"text": dict(TEXT)}),
           cause={"external": {"source": {"gitea": {"pr": 12}}, "unknownBase": False}}, seq=6),
    # 7. A verse value variant minor 0 cannot hold.
    change(1, verse({"split": {"at": 3}}), seq=7),
    # 8. A minor-0 change with an unknown field: malformed.
    change(0, verse({"text": dict(TEXT, note="x")}), seq=8),
    # 9. A minor-0 change of an unknown kind: malformed.
    change(0, {"noteThread": {"book": "TIT", "thread": "a3", "value": 1}}, seq=9),
    # 10. A minor-0 change with an unknown cause: malformed.
    change(0, verse({"text": dict(TEXT)}), cause={"signedEdit": {}}, seq=10),
    # 11. A book code minor 0 does not know.
    change(1, {"bookHeader": {"book": "TOB", "value": "\\id TOB\n"}}, seq=11),
]


def line(body):
    text = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    data = text.encode("utf-8")
    return data + b"\t" + hashlib.sha256(data).hexdigest().encode() + b"\n"


(HERE / "newer-minor.ndjson").write_bytes(b"".join(line(body) for body in CASES))
