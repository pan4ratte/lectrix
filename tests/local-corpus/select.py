"""Chooses real-world test files from a survey and copies them into the local corpus.

    python tests/local-corpus/survey.py "<library>"          # first: profile every file
    python tests/local-corpus/select.py                      # then: pick and copy

The local corpus (AGENTS.md section 9) holds files the user owns but cannot publish, so
`files/` and `manifest.json` here are git-ignored. Source files are only read (copied);
nothing is ever written next to them. Each category takes a deterministic pick from the
survey (the smallest file that shows the case, or an extreme), so re-running on the same
library selects the same files.
"""

import argparse
import hashlib
import json
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
DEFAULT_SURVEY = ROOT / "target" / "test-output" / "survey" / "library.jsonl"


def annotations(r, kind):
    return r["annotations"].get(kind, 0)


def rotated(r):
    return sum(r["rotated_pages"].values())


# (tag, predicate, sort key: lowest first, how many)
CATEGORIES = [
    ("repaired", lambda r: r["repaired"], lambda r: r["bytes"], 1),
    ("encrypted", lambda r: r["encrypted"], lambda r: r["bytes"], 1),
    ("encrypted-large", lambda r: r["encrypted"], lambda r: -r["pages"], 1),
    ("forms", lambda r: r["form_fields"] > 0, lambda r: r["bytes"], 1),
    ("forms-many-widgets", lambda r: r["form_fields"] > 0, lambda r: -annotations(r, "Widget"), 1),
    ("rtl", lambda r: r["sample_rtl_chars"] > 50, lambda r: r["bytes"], 1),
    ("cjk", lambda r: r["sample_cjk_chars"] > 50, lambda r: r["bytes"], 1),
    ("rotated-some", lambda r: 0 < rotated(r) < r["pages"], lambda r: r["bytes"], 1),
    ("rotated-all-90", lambda r: r["rotated_pages"].get("90", 0) == r["pages"], lambda r: r["bytes"], 1),
    ("rotated-180", lambda r: r["rotated_pages"].get("180", 0) > 0, lambda r: r["bytes"], 1),
    ("rotated-270", lambda r: r["rotated_pages"].get("270", 0) > 0, lambda r: r["bytes"], 1),
    ("offset-cropbox", lambda r: r["offset_crop_pages"] > 0, lambda r: r["bytes"], 1),
    ("user-unit", lambda r: r["user_unit_pages"] > 0, lambda r: r["bytes"], 1),
    ("scanned", lambda r: r["sample_text_chars"] < 20, lambda r: r["bytes"], 1),
    ("scanned-large", lambda r: r["sample_text_chars"] < 20, lambda r: -r["pages"], 1),
    ("scanned-annotated", lambda r: r["sample_text_chars"] < 20 and annotations(r, "Ink"), lambda r: r["bytes"], 1),
    ("most-pages", lambda r: True, lambda r: -r["pages"], 1),
    ("most-bookmarks", lambda r: True, lambda r: -r["outline_items"], 1),
    ("large-rotated-cropped", lambda r: r["pages"] >= 1000 and rotated(r) and r["offset_crop_pages"], lambda r: r["bytes"], 1),
    ("labels-and-deep-outline", lambda r: r["label_rules"] >= 3 and r["outline_depth"] >= 3, lambda r: r["bytes"], 1),
    ("most-notes", lambda r: True, lambda r: -annotations(r, "Text"), 1),
    ("slow-first-page", lambda r: True, lambda r: -r["first_page_ms"], 2),
    ("largest-file", lambda r: True, lambda r: -r["bytes"], 1),
    ("calibre-made", lambda r: (r.get("producer") or "").startswith("calibre"), lambda r: r["bytes"], 1),
] + [
    (f"annot-{kind.lower()}", (lambda k: lambda r: annotations(r, k) > 0)(kind), lambda r: r["bytes"], 1)
    for kind in ["Highlight", "Underline", "StrikeOut", "Squiggly", "Text", "Ink", "FreeText", "Stamp", "Square", "Circle", "Caret", "FileAttachment"]
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--survey", type=pathlib.Path, default=DEFAULT_SURVEY)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    rows = [json.loads(line) for line in args.survey.open(encoding="utf-8")]
    usable = [r for r in rows if not r.get("error") and not r.get("needs_password") and r.get("pages")]

    picked = {}  # source path -> list of tags
    for tag, keep, key, count in CATEGORIES:
        matches = sorted((r for r in usable if keep(r)), key=lambda r: (key(r), r["path"]))
        if not matches:
            print(f"  (no file for {tag})")
        for r in matches[:count]:
            picked.setdefault(r["path"], []).append(tag)

    by_path = {r["path"]: r for r in usable}
    total = sum(by_path[p]["bytes"] for p in picked)
    print(f"selected {len(picked)} files, {total / 1e6:.0f} MB")
    if args.dry_run:
        for path, tags in picked.items():
            print(f"  {', '.join(tags)}: {by_path[path]['pages']} pages, {by_path[path]['bytes'] / 1e6:.1f} MB")
        return 0

    files = HERE / "files"
    files.mkdir(exist_ok=True)
    manifest = []
    for n, (path, tags) in enumerate(sorted(picked.items(), key=lambda kv: kv[1][0]), 1):
        name = f"{n:02d}-{tags[0]}.pdf"
        target = files / name
        source_hash = sha256(path)
        if not target.exists() or sha256(target) != source_hash:
            shutil.copyfile(path, target)  # reads the source; writes only the copy
        if sha256(target) != source_hash:
            print(f"copy mismatch for {name}", file=sys.stderr)
            return 1
        manifest.append({"file": name, "tags": tags, "sha256": source_hash, "source": path, "survey": by_path[path]})
    (HERE / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"wrote {len(manifest)} files to {files} and manifest.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
