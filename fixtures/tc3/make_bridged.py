"""Builds en_kjv_luk_bridged.zip from tC4's tC3 Luke fixture: Luke 1:1 and 1:2 become one
bridged verse "1-2", in the text (luk/1.json) and in the alignment data, the way tC3 files a
bridge (both keyed by the span; translationCore WordAlignmentHelpers.js
convertAlignmentDataToUSFM reads chapterAlignmentJSON["1-2"] with targetLanguageChapterJSON["1-2"]).
The target words' occurrences are counted over the span. tC3's invalid marker for 1:1 is left
out, so only the import's own word check can mark the record.

Run from this folder: python3 -I make_bridged.py
"""
import json
import zipfile

SOURCE = "../tc4/conformance-fixtures/import/tc3/multi/en_kjv_luk_book.zip"
TARGET = "en_kjv_luk_bridged.zip"
TEXT = "luk/1.json"
ALIGNED = "apps/translationCore/alignmentData/luk/1.json"
INVALID = "apps/translationCore/tools/wordAlignment/invalid/1/1.json"


def bridged(chapter, merge):
    out = {}
    for key, value in chapter.items():
        if key == "1":
            out["1-2"] = merge(chapter["1"], chapter["2"])
        elif key != "2":
            out[key] = value
    return out


def counts(record):
    seen = {}
    for word in target_words(record):
        seen[word["word"]] = word["occurrences"]
    return seen


def target_words(record):
    for group in record["alignments"]:
        yield from group["bottomWords"]
    yield from record["wordBank"]


def merge_records(first, second):
    first_counts, second_counts = counts(first), counts(second)
    first = json.loads(json.dumps(first))
    second = json.loads(json.dumps(second))
    for word in target_words(first):
        word["occurrences"] += second_counts.get(word["word"], 0)
    for word in target_words(second):
        earlier = first_counts.get(word["word"], 0)
        word["occurrence"] += earlier
        word["occurrences"] += earlier
    return {
        "alignments": first["alignments"] + second["alignments"],
        "wordBank": first["wordBank"] + second["wordBank"],
    }


with zipfile.ZipFile(SOURCE) as source, zipfile.ZipFile(TARGET, "w", zipfile.ZIP_DEFLATED) as target:
    for info in source.infolist():
        if info.filename == INVALID:
            continue
        data = source.read(info.filename)
        if info.filename == TEXT:
            data = json.dumps(bridged(json.loads(data), lambda a, b: f"{a} {b}"), ensure_ascii=False, indent=2).encode()
        elif info.filename == ALIGNED:
            data = json.dumps(bridged(json.loads(data), merge_records), ensure_ascii=False).encode()
        info.date_time = (2026, 10, 28, 0, 0, 0)
        target.writestr(info, data)
