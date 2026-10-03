"""Cross-renderer interop harness (AGENTS.md section 9).

    python tests/interop/run.py phase0      # build the Phase 0 outputs with pdf-cli and check them
    python tests/interop/run.py phase2      # bookmarks edited as the app edits them
    python tests/interop/run.py check FILE  # render/visibility checks for one existing file

For every annotation in a checked file, each annotated page is rendered by three
independent engines -- MuPDF (pdf-cli, the same MuPDF build the app uses), PDFium
(pypdfium2) and pdf.js (pdfjs-dist) -- with and without annotations. Inside the
annotation's /Rect the two renders must differ in every engine, otherwise the annotation is
invisible there. Engines are then compared with each other, and diff images are saved for
any failure under target/test-output/interop/.

Structural checks: `qpdf --check` (any warning fails), and for labels/outlines the values
read back by pdf.js and PDFium must equal the expected values.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OUT = ROOT / "target" / "test-output" / "interop"
SCALE = 2.0

# Visibility: a pixel counts as changed when any channel differs by more than this.
CHANGE_THRESHOLD = 32
# An annotation is visible when at least this share of its /Rect pixels changed (and at
# least MIN_CHANGED_PIXELS).
MIN_CHANGED_SHARE = 0.02
MIN_CHANGED_PIXELS = 20
# Cross-engine tolerance: changed-pixel share may differ by at most this factor, and the
# mean colour of the changed pixels by at most this distance (0-441).
COVERAGE_RATIO_LIMIT = 2.0
COLOR_DISTANCE_LIMIT = 60.0


def exe(name: str) -> str:
    found = shutil.which(name)
    if found:
        return found
    for candidate in [
        Path(os.environ.get("ProgramFiles", "C:/Program Files")).glob("qpdf*/bin/qpdf.exe"),
    ]:
        for path in candidate:
            if name == "qpdf":
                return str(path)
    raise SystemExit(f"{name} not found on PATH")


def pdf_cli() -> str:
    path = ROOT / "target" / "debug" / ("pdf-cli.exe" if os.name == "nt" else "pdf-cli")
    if not path.exists():
        raise SystemExit("build pdf-cli first: cargo build -p pdf-cli")
    return str(path)


def run(args: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(args, check=True, capture_output=True, encoding="utf-8", **kw)


# --- engines -----------------------------------------------------------------------------


def render_mupdf(pdf: Path, page: int, annots: bool, out: Path) -> Path:
    args = [pdf_cli(), "render", str(pdf), str(out), "--page", str(page), "--scale", str(SCALE)]
    if not annots:
        args.append("--no-annotations")
    run(args)
    return out


def render_pdfium(pdf: Path, page: int, annots: bool, out: Path) -> Path:
    run([sys.executable, str(HERE / "pdfium_tool.py"), "render", str(pdf), str(page), str(SCALE), "on" if annots else "off", str(out)])
    return out


def render_pdfjs(pdf: Path, page: int, annots: bool, out: Path) -> Path:
    run(["node", str(HERE / "pdfjs_tool.mjs"), "render", str(pdf), str(page), str(SCALE), "on" if annots else "off", str(out)], cwd=HERE)
    return out


ENGINES = {"mupdf": render_mupdf, "pdfium": render_pdfium, "pdfjs": render_pdfjs}


def info_pdfium(pdf: Path) -> dict:
    return json.loads(run([sys.executable, str(HERE / "pdfium_tool.py"), "info", str(pdf)], env={**os.environ, "PYTHONIOENCODING": "utf-8"}).stdout)


def info_pdfjs(pdf: Path) -> dict:
    return json.loads(run(["node", str(HERE / "pdfjs_tool.mjs"), "info", str(pdf)], cwd=HERE).stdout)


# --- geometry (mirrors crates/pdf-core/src/geometry.rs) ---------------------------------


def page_transform(pdf: Path, page_index: int):
    import pypdfium2 as pdfium

    doc = pdfium.PdfDocument(str(pdf))
    page = doc[page_index]
    media = page.get_mediabox()
    crop = page.get_cropbox()
    rotate = page.get_rotation()
    doc.close()
    x0, y0 = max(crop[0], media[0]), max(crop[1], media[1])
    x1, y1 = min(crop[2], media[2]), min(crop[3], media[3])
    w, h = x1 - x0, y1 - y0

    def to_view(x: float, y: float) -> tuple[float, float]:
        # Unrotated view: origin top-left of the visible box, y down.
        u, v = x - x0, y1 - y
        if rotate == 90:
            return h - v, u
        if rotate == 180:
            return w - u, h - v
        if rotate == 270:
            return v, w - u
        return u, v

    view_width = h if rotate in (90, 270) else w
    return to_view, view_width


def rect_to_pixels(pdf: Path, page_index: int, rect: list[float], shape) -> tuple[int, int, int, int]:
    to_view, view_width = page_transform(pdf, page_index)
    # Engines disagree on /UserUnit (MuPDF and pdf.js scale the page by it, PDFium ignores
    # it), so take each render's effective scale from its width.
    scale = shape[1] / view_width
    pts = [to_view(rect[0], rect[1]), to_view(rect[2], rect[1]), to_view(rect[0], rect[3]), to_view(rect[2], rect[3])]
    xs = [p[0] * scale for p in pts]
    ys = [p[1] * scale for p in pts]
    hgt, wid = shape[0], shape[1]
    return (
        max(0, int(min(xs))),
        max(0, int(min(ys))),
        min(wid, int(np.ceil(max(xs)))),
        min(hgt, int(np.ceil(max(ys)))),
    )


# --- checks ------------------------------------------------------------------------------


@dataclass
class Report:
    failures: list[str] = field(default_factory=list)
    passes: list[str] = field(default_factory=list)

    def check(self, ok: bool, message: str) -> bool:
        (self.passes if ok else self.failures).append(message)
        print(("  PASS " if ok else "  FAIL ") + message)
        return ok


def qpdf_check(pdf: Path, report: Report) -> None:
    proc = subprocess.run([exe("qpdf"), "--check", str(pdf)], capture_output=True, encoding="utf-8")
    # Exit 0: clean. 3: warnings. 2: errors. Any warning fails (section 9).
    detail = (proc.stdout + proc.stderr).strip().splitlines()
    report.check(proc.returncode == 0, f"{pdf.name}: qpdf --check exit {proc.returncode}" + ("" if proc.returncode == 0 else f" ({detail[-1]})"))


def load(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB")).astype(np.int16)


def annotation_checks(pdf: Path, report: Report, work: Path) -> None:
    pdfium_info = info_pdfium(pdf)
    pdfjs_info = info_pdfjs(pdf)
    annots = [a for a in pdfium_info["annotations"] if a["subtype"] not in ("Popup", "Link", "Widget")]
    jsannots = [a for a in pdfjs_info["annotations"] if a["subtype"] not in ("Popup", "Link", "Widget")]
    report.check(len(annots) == len(jsannots), f"{pdf.name}: PDFium sees {len(annots)} annotations, pdf.js {len(jsannots)}")
    for a in jsannots:
        report.check(bool(a.get("hasAppearance")), f"{pdf.name}: pdf.js finds an appearance stream for {a['subtype']} on page {a['page']}")

    renders: dict[tuple[str, int, bool], np.ndarray] = {}
    for page in sorted({a["page"] for a in annots}):
        for name, fn in ENGINES.items():
            for on in (True, False):
                out = work / f"p{page}-{name}-{'on' if on else 'off'}.png"
                renders[(name, page, on)] = load(fn(pdf, page, on, out))

    for n, a in enumerate(annots):
        page = a["page"]
        label = f"{pdf.name}: {a['subtype']} #{n + 1} on page {page}"
        stats = {}
        for name in ENGINES:
            on, off = renders[(name, page, True)], renders[(name, page, False)]
            if on.shape != off.shape:
                report.check(False, f"{label}: {name} renders differ in size")
                continue
            x0, y0, x1, y1 = rect_to_pixels(pdf, page - 1, a["rect"], on.shape)
            region_on, region_off = on[y0:y1, x0:x1], off[y0:y1, x0:x1]
            changed = np.abs(region_on - region_off).max(axis=2) > CHANGE_THRESHOLD
            area = max(1, changed.size)
            share = changed.sum() / area
            mean_color = region_on[changed].mean(axis=0) if changed.any() else np.array([0, 0, 0])
            stats[name] = (share, mean_color, (x0, y0, x1, y1))
            visible = changed.sum() >= MIN_CHANGED_PIXELS and share >= MIN_CHANGED_SHARE
            ok = report.check(visible, f"{label}: visible in {name} ({share:.0%} of /Rect pixels changed)")
            if not ok:
                save_diff(work / f"invisible-{n + 1}-{name}.png", region_on, region_off)

        names = [k for k in ENGINES if k in stats]
        for i in range(len(names)):
            for j in range(i + 1, len(names)):
                (sa, ca, ra), (sb, cb, rb) = stats[names[i]], stats[names[j]]
                ratio = max(sa, sb) / max(min(sa, sb), 1e-9)
                dist = float(np.linalg.norm(ca - cb))
                ok = report.check(
                    min(sa, sb) > 0 and ratio <= COVERAGE_RATIO_LIMIT and dist <= COLOR_DISTANCE_LIMIT,
                    f"{label}: {names[i]} vs {names[j]} agree (coverage ratio {ratio:.2f}, colour distance {dist:.0f})",
                )
                if not ok:
                    a_img = renders[(names[i], page, True)]
                    b_img = renders[(names[j], page, True)]
                    x0, y0, x1, y1 = ra
                    save_diff(work / f"engines-{n + 1}-{names[i]}-{names[j]}.png", a_img[y0:y1, x0:x1], b_img[y0:y1, x0:x1])


def save_diff(path: Path, a: np.ndarray, b: np.ndarray) -> None:
    h = max(a.shape[0], b.shape[0])
    pad = lambda img: np.pad(img, ((0, h - img.shape[0]), (0, 0), (0, 0)), constant_values=255)
    a, b = pad(a), pad(b)
    w = min(a.shape[1], b.shape[1])
    diff = 255 - np.clip(np.abs(a[:, :w] - b[:, :w]) * 4, 0, 255)
    strip = np.concatenate([a, b, diff], axis=1).astype(np.uint8)
    Image.fromarray(strip).save(path)
    print(f"       diff image: {path}")


def strip_outline(items, page_key="page"):
    return [(i["title"], i[page_key], strip_outline(i["children"])) for i in items]


def outline_and_label_checks(pdf: Path, report: Report, labels=None, outline=None, pages=None) -> None:
    for engine, data in (("PDFium", info_pdfium(pdf)), ("pdf.js", info_pdfjs(pdf))):
        if pages is not None:
            report.check(data["pages"] == pages, f"{pdf.name}: {engine} counts {data['pages']} pages (expected {pages})")
        if labels is not None:
            got = data["labels"] or []
            report.check(got == labels, f"{pdf.name}: {engine} page labels match" + ("" if got == labels else f" (got {got[:12]}...)"))
        if outline is not None:
            got = strip_outline(data["outline"])
            report.check(got == outline, f"{pdf.name}: {engine} outline matches" + ("" if got == outline else f"\n       got {got}\n       expected {outline}"))


# --- suites ------------------------------------------------------------------------------


def phase0(report: Report) -> None:
    work = OUT / "phase0"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    cli = pdf_cli()
    p = lambda name: str(work / name)

    run([cli, "gen", p("base.pdf"), "--pages", "20", "--title", "Base sample"])
    run([cli, "gen", p("second.pdf"), "--pages", "5", "--title", "Second sample"])

    # 1. Page labels.
    run([cli, "labels", "set", p("base.pdf"), p("labels.pdf"), "--rule", "1:roman-lower", "--rule", "5:decimal", "--rule", "18:decimal:A-:1"])
    roman = ["i", "ii", "iii", "iv"]
    expected_labels = roman + [str(n) for n in range(1, 14)] + ["A-1", "A-2", "A-3"]
    print("labels.pdf")
    qpdf_check(work / "labels.pdf", report)
    outline_and_label_checks(work / "labels.pdf", report, labels=expected_labels, pages=20)

    # 2. Outline (3 levels, Unicode titles, open and closed items).
    items = ["0+:1:Front matter", "1:2:Préface — ünïcödé 日本", "0+:5:Chapter 1", "1:6:Section 1.1", "2:7:Deep item", "0:18:Appendix"]
    run([cli, "outline", "set", p("base.pdf"), p("outline.pdf")] + [a for i in items for a in ("--item", i)])
    expected_outline = [
        ("Front matter", 0, [("Préface — ünïcödé 日本", 1, [])]),
        ("Chapter 1", 4, [("Section 1.1", 5, [("Deep item", 6, [])])]),
        ("Appendix", 17, []),
    ]
    print("outline.pdf")
    qpdf_check(work / "outline.pdf", report)
    outline_and_label_checks(work / "outline.pdf", report, outline=expected_outline)

    # 3. Merge (labels file + outline file + plain file).
    run([cli, "merge", p("merged.pdf"), p("labels.pdf"), p("outline.pdf"), p("second.pdf")])
    merged_labels = expected_labels + [str(n) for n in range(1, 21)] + [str(n) for n in range(1, 6)]
    shift = lambda tree, k: [(t, pg + k, shift(c, k)) for t, pg, c in tree]
    merged_outline = [
        ("Base sample", 0, []),
        ("Base sample", 20, shift(expected_outline, 20)),
        ("Second sample", 40, []),
    ]
    print("merged.pdf")
    qpdf_check(work / "merged.pdf", report)
    outline_and_label_checks(work / "merged.pdf", report, labels=merged_labels, outline=merged_outline, pages=45)

    # 4. Highlight with appearance stream, on normal, rotated, cropped and UserUnit pages.
    variants = {
        "normal": [],
        "rot90": ["--rotate", "90"],
        "rot180": ["--rotate", "180"],
        "rot270": ["--rotate", "270"],
        "crop": ["--crop", "100,150,500,700"],
        "crop-rot90": ["--crop", "100,150,500,700", "--rotate", "90"],
        "userunit": ["--user-unit", "2"],
    }
    for name, extra in variants.items():
        src, dst = p(f"gen-{name}.pdf"), work / f"highlight-{name}.pdf"
        run([cli, "gen", src, "--pages", "2"] + extra)
        run([cli, "annot", "markup", src, str(dst), "--page", "1", "--text", "quick brown fox", "--opacity", "0.6", "--author", "Folio Harness", "--note", "Harness note"])
        print(dst.name)
        qpdf_check(dst, report)
        case = work / name
        case.mkdir(exist_ok=True)
        annotation_checks(dst, report, case)


def assemble_pdf(objects: list[str]) -> bytes:
    """A PDF written by hand (objects 1..n, classic xref), not by MuPDF."""
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n{body}\nendobj\n".encode("latin-1")
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for o in offsets:
        out += f"{o:010} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    return bytes(out)


def outline_fixture() -> bytes:
    """Six pages and a three-level outline written by another "app": explicit, named (via a
    name tree) and action targets, a UTF-16 title. Same fixture as
    crates/pdf-core/tests/outline.rs; outline items are objects 11 to 21."""
    page = "<< /Type /Page /Parent 2 0 R /Resources << >> >>"
    return assemble_pdf([
        "<< /Type /Catalog /Pages 2 0 R /Outlines 3 0 R /Names << /Dests 4 0 R >> >>",
        "<< /Type /Pages /Kids [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R] /Count 6 /MediaBox [0 0 612.000 792] >>",
        "<< /Type /Outlines /First 11 0 R /Last 21 0 R /Count 7 >>",
        "<< /Names [(chap2) [6 0 R /FitH 700.50] (intro) << /D [5 0 R /XYZ 72.000 720 0] >>] >>",
        page, page, page, page, page, page,
        "<< /Title (Part I) /Parent 3 0 R /First 12 0 R /Last 16 0 R /Next 17 0 R /Count 3 /Dest [5 0 R /XYZ 72.000 720.0 null] >>",
        "<< /Title (Chapter 1) /Parent 11 0 R /First 13 0 R /Last 14 0 R /Next 15 0 R /Count -2 /Dest (intro) >>",
        "<< /Title (Section 1.1) /Parent 12 0 R /Next 14 0 R /A << /S /GoTo /D (chap2) >> >>",
        "<< /Title (Section 1.2) /Parent 12 0 R /Prev 13 0 R /A << /S /URI /URI (https://example.org/a%20b) >> >>",
        "<< /Title (Chapter 2) /Parent 11 0 R /Prev 12 0 R /Next 16 0 R /Dest [6 0 R /Fit] /C [0.000 0 1] /F 3 /Foo /Bar >>",
        "<< /Title (Chapter 3) /Parent 11 0 R /Prev 15 0 R /A << /S /GoToR /F (other.pdf) /D [0 /Fit] >> >>",
        "<< /Title (Part II) /Parent 3 0 R /Prev 11 0 R /Next 20 0 R /First 18 0 R /Last 19 0 R /Count -2 /Dest [7 0 R /XYZ null 500.25 null] >>",
        "<< /Title (Chapter 4) /Parent 17 0 R /Next 19 0 R /A << /S /JavaScript /JS (app.alert\\(1\\)) >> >>",
        "<< /Title (Chapter 5) /Parent 17 0 R /Prev 18 0 R /Dest [8 0 R /FitR 10 20 300.5 400] >>",
        "<< /Title <FEFF041F04400438043B043E04360435043D04380435> /Parent 3 0 R /Prev 17 0 R /Next 21 0 R /Dest [9 0 R /XYZ 0 792 0] >>",
        "<< /Title (Index) /Parent 3 0 R /Prev 20 0 R /Dest [10 0 R /XYZ 0 792 null] >>",
    ])


ANY = object()  # an expected value every engine may read its own way


def matches(got, expected) -> bool:
    if expected is ANY:
        return True
    if isinstance(expected, (list, tuple)):
        return isinstance(got, (list, tuple)) and len(got) == len(expected) and all(matches(g, e) for g, e in zip(got, expected))
    return got == expected


def find_item(items, title):
    for item in items:
        if item["title"] == title:
            return item
        found = find_item(item["children"], title)
        if found:
            return found
    return None


def phase2(report: Report) -> None:
    """Bookmarks edited the way the app edits them (pdf-cli outline edit goes through the
    same session and journal), read back by PDFium (Edge, Chrome) and pdf.js (Firefox)."""
    work = OUT / "phase2"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    cli = pdf_cli()
    source, edited = work / "outline-source.pdf", work / "outline-edited.pdf"
    source.write_bytes(outline_fixture())
    ops = [
        "rename:15:Chapter Two",
        "move:19:11:3",
        "delete:21",
        "add:-:1:4:0:100:Nouveau — 新しい",
        "retarget:14:2:36:50",
        "add:12:2:2:0:0:Section 1.3",
        "open:17",
        "open:12",
    ]
    run([cli, "outline", "edit", str(source), str(edited)] + [a for op in ops for a in ("--op", op)])
    print(edited.name)
    qpdf_check(edited, report)
    expected = [
        ("Part I", 0, [
            ("Chapter 1", 0, [("Section 1.1", 1, []), ("Section 1.2", 1, []), ("Section 1.3", 1, [])]),
            ("Chapter Two", 1, []),
            # A GoToR action: PDFium reports the remote file's page index, pdf.js none.
            ("Chapter 3", ANY, []),
            ("Chapter 5", 3, []),
        ]),
        ("Nouveau — 新しい", 3, []),
        ("Part II", 2, [("Chapter 4", None, [])]),
        ("Приложение", 4, []),
    ]
    for engine, data in (("PDFium", info_pdfium(edited)), ("pdf.js", info_pdfjs(edited))):
        items = data["outline"]
        got = strip_outline(items)
        report.check(matches(got, expected), f"{edited.name}: {engine} tree, titles and target pages match" + ("" if matches(got, expected) else f"\n       got {got}\n       expected {expected}"))
        for title, want in (("Part I", True), ("Chapter 1", True), ("Part II", True)):
            item = find_item(items, title)
            report.check(item is not None and item["open"] == want, f"{edited.name}: {engine} shows '{title}' {'expanded' if want else 'collapsed'}")
        # New and retargeted bookmarks: /XYZ left top in user space, null zoom.
        for title, left, top in (("Nouveau — 新しい", 0, 692), ("Section 1.2", 36, 742), ("Section 1.3", 0, 792)):
            item = find_item(items, title)
            view = (item or {}).get("view") or []
            ok = len(view) >= 2 and abs(view[0] - left) < 0.01 and abs(view[1] - top) < 0.01
            report.check(ok, f"{edited.name}: {engine} puts '{title}' at {left},{top} (got {view[:2]})")


def main(argv: list[str]) -> int:
    report = Report()
    if argv[:1] == ["phase0"]:
        phase0(report)
    elif argv[:1] == ["phase2"]:
        phase2(report)
    elif argv[:1] == ["check"] and len(argv) == 2:
        pdf = Path(argv[1]).resolve()
        work = OUT / "check" / pdf.stem
        work.mkdir(parents=True, exist_ok=True)
        qpdf_check(pdf, report)
        annotation_checks(pdf, report, work)
    else:
        print(__doc__)
        return 2
    print(f"\n{len(report.passes)} passed, {len(report.failures)} failed")
    return 1 if report.failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
