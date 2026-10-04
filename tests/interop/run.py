"""Cross-renderer interop harness (AGENTS.md section 9).

    python tests/interop/run.py phase0      # labels, outline, merge and highlight written by pdf-cli
    python tests/interop/run.py phase2      # bookmarks edited as the app edits them
    python tests/interop/run.py phase3      # page labels edited as the app edits them
    python tests/interop/run.py phase4      # files combined and pages inserted as the app does it
    python tests/interop/run.py phase4-local  # three real files from the local corpus, combined
    python tests/interop/run.py phase5      # every annotation type on every page geometry; edits; repair
    python tests/interop/run.py phase5-local  # repair on the local corpus files that need it

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


def annotation_checks(pdf: Path, report: Report, work: Path, pages: set[int] | None = None) -> None:
    """`pages` (1-based) limits the checks to annotations on those pages."""
    pdfium_info = info_pdfium(pdf)
    pdfjs_info = info_pdfjs(pdf)
    wanted = lambda a: a["subtype"] not in ("Popup", "Link", "Widget") and (pages is None or a["page"] in pages)
    annots = [a for a in pdfium_info["annotations"] if wanted(a)]
    jsannots = [a for a in pdfjs_info["annotations"] if wanted(a)]
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
        region = display_area(pdf, page - 1, a)
        for name in ENGINES:
            on, off = renders[(name, page, True)], renders[(name, page, False)]
            if on.shape != off.shape:
                report.check(False, f"{label}: {name} renders differ in size")
                continue
            x0, y0, x1, y1 = rect_to_pixels(pdf, page - 1, region, on.shape)
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


def display_area(pdf: Path, page_index: int, annot: dict) -> list[float]:
    """Where an annotation may be drawn, in user space: its /Rect, and for a sticky note
    (/Text) on a rotated page also that Rect turned about its upper-left corner. PDF
    32000-1 12.5.6.4 has text annotations behave as if NoRotate were set, which Acrobat and
    MuPDF do (the icon stays upright at that corner); PDFium and pdf.js draw it in /Rect,
    turned with the page. Either is a visible note."""
    x0, y0, x1, y1 = annot["rect"]
    if annot["subtype"] != "Text":
        return [x0, y0, x1, y1]
    import pypdfium2 as pdfium

    doc = pdfium.PdfDocument(str(pdf))
    rotate = doc[page_index].get_rotation()
    doc.close()
    if rotate == 0:
        return [x0, y0, x1, y1]
    import math

    t = math.radians(rotate)
    cos, sin = round(math.cos(t)), round(math.sin(t))
    # Counter-clockwise in user space (y up), about (x0, y1), as MuPDF's fz_rotate.
    pts = [(x - x0, y - y1) for x, y in ((x0, y0), (x1, y0), (x0, y1), (x1, y1))]
    turned = [(x0 + px * cos - py * sin, y1 + px * sin + py * cos) for px, py in pts]
    xs = [p[0] for p in turned] + [x0, x1]
    ys = [p[1] for p in turned] + [y0, y1]
    return [min(xs), min(ys), max(xs), max(ys)]


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
        run([cli, "annot", "markup", src, str(dst), "--page", "1", "--text", "quick brown fox", "--opacity", "0.6", "--author", "Lectrix Harness", "--note", "Harness note"])
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


def labels_fixture() -> bytes:
    """Twelve pages with labels written by another "app": a number tree with /Kids and
    /Limits, a /Type key and a UTF-16BE prefix ("Äh-"). Same fixture as
    crates/pdf-core/tests/labels.rs."""
    page = "<< /Type /Page /Parent 2 0 R /Resources << >> >>"
    kids = " ".join(f"{6 + i} 0 R" for i in range(12))
    return assemble_pdf([
        "<< /Type /Catalog /Pages 2 0 R /PageLabels 3 0 R >>",
        f"<< /Type /Pages /Kids [{kids}] /Count 12 /MediaBox [0 0 612.000 792] >>",
        "<< /Kids [4 0 R 5 0 R] >>",
        "<< /Limits [0 4] /Nums [0 << /Type /PageLabel /P (Cover) >> 1 << /S /r >>  4 << /S /D /St 1 >>] >>",
        "<< /Limits [9 11] /Nums [9 << /S /A /P <FEFF00C40068002D> /St 3 >> 11 << /S /D /P (Index ) /St 120 >>] >>",
    ] + [page] * 12)


def label_checks(pdf: Path, report: Report, expected: list[str] | None) -> None:
    """Both engines read `expected` (None: the file has no labels)."""
    for engine, data in (("PDFium", info_pdfium(pdf)), ("pdf.js", info_pdfjs(pdf))):
        got = data["labels"]
        if expected is None:
            # pdf.js reports no labels as null, PDFium as an empty label per page.
            ok = got is None or all(label == "" for label in got)
            report.check(ok, f"{pdf.name}: {engine} finds no page labels" + ("" if ok else f" (got {got[:12]})"))
        else:
            detail = "" if got == expected else f"\n       got      {got}\n       expected {expected}"
            report.check(got == expected, f"{pdf.name}: {engine} page labels match{detail}")


def phase3(report: Report) -> None:
    """Page labels set the way the app sets them (pdf-cli labels edit goes through the same
    session and journal), read back by PDFium (Edge, Chrome) and pdf.js (Firefox). The
    expected labels are written out here, not computed with Lectrix's code."""
    work = OUT / "phase3"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    cli = pdf_cli()

    # 1. A document without labels: roman front matter, arabic body, an appendix lettered
    # from Y past Z, and an index labeled with a non-ASCII prefix only.
    base, made = work / "base.pdf", work / "app-labels.pdf"
    run([cli, "gen", str(base), "--pages", "30"])
    rules = ["1:roman-lower", "5:decimal", "20:letters-upper:Anhang :25", "28:none:Índice"]
    run([cli, "labels", "edit", str(base), str(made)] + [a for r in rules for a in ("--rule", r)])
    expected = (
        ["i", "ii", "iii", "iv"]
        + [str(n) for n in range(1, 16)]
        + ["Anhang " + x for x in ("Y", "Z", "AA", "BB", "CC", "DD", "EE", "FF")]
        + ["Índice"] * 3
    )
    print(made.name)
    qpdf_check(made, report)
    label_checks(made, report, expected)

    # 2. Labels another app wrote, as they are, and after the app moves one range.
    source, edited = work / "other-app-labels.pdf", work / "other-app-labels-edited.pdf"
    source.write_bytes(labels_fixture())
    print(source.name)
    label_checks(source, report, ["Cover", "i", "ii", "iii", "1", "2", "3", "4", "5", "Äh-C", "Äh-D", "Index 120"])
    # The panel sends every rule; the decimal range now starts at page 6.
    rules = ["1:none:Cover", "2:roman-lower", "6:decimal", "10:letters-upper:Äh-:3", "12:decimal:Index :120"]
    run([cli, "labels", "edit", str(source), str(edited)] + [a for r in rules for a in ("--rule", r)])
    print(edited.name)
    qpdf_check(edited, report)
    label_checks(edited, report, ["Cover", "i", "ii", "iii", "iv", "1", "2", "3", "4", "Äh-C", "Äh-D", "Index 120"])

    # 3. "Remove all labels."
    removed = work / "labels-removed.pdf"
    run([cli, "labels", "edit", str(source), str(removed)])
    print(removed.name)
    qpdf_check(removed, report)
    label_checks(removed, report, None)


def links_fixture() -> bytes:
    """Three text pages written by another "app", with links on page 1: an explicit
    destination to page 3, a GoTo action to the named destination "chap2" (page 2, in a
    name tree), and a web link."""
    def content(n: int) -> str:
        text = f"BT /F1 24 Tf 72 720 Td (Linked page {n}) Tj ET BT /F1 12 Tf 72 690 Td (The quick brown fox jumps over the lazy dog on page {n}.) Tj ET"
        return f"<< /Length {len(text)} >>\nstream\n{text}\nendstream"

    def link(y: int, target: str) -> str:
        return f"<< /Type /Annot /Subtype /Link /Rect [72 {y} 300 {y + 20}] /Border [0 0 0] {target} >>"

    page = "<< /Type /Page /Parent 2 0 R /Contents {} 0 R {}>>"
    return assemble_pdf([
        "<< /Type /Catalog /Pages 2 0 R /Names << /Dests 3 0 R >> >>",
        "<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R] /Count 3 /MediaBox [0 0 612 792] /Resources << /Font << /F1 10 0 R >> >> >>",
        "<< /Names [(chap2) [5 0 R /XYZ 0 792 null]] >>",
        page.format(7, "/Annots [11 0 R 12 0 R 13 0 R] "),
        page.format(8, ""),
        page.format(9, ""),
        content(1),
        content(2),
        content(3),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        link(600, "/Dest [6 0 R /XYZ 0 792 null]"),
        link(570, "/A << /S /GoTo /D (chap2) >>"),
        link(540, "/A << /S /URI /URI (https://example.org/) >>"),
    ])


def link_checks(pdf: Path, report: Report, expected: list) -> None:
    """Both engines find the links of `pdf` as (page, target) pairs: `page` 1-based, the
    target a 0-based page index or the web address."""
    for engine, data in (("PDFium", info_pdfium(pdf)), ("pdf.js", info_pdfjs(pdf))):
        got = sorted(((l["page"], l["uri"] if l["uri"] else l["target"]) for l in data["links"]), key=str)
        want = sorted(expected, key=str)
        report.check(got == want, f"{pdf.name}: {engine} links lead to the right pages" + ("" if got == want else f"\n       got      {got}\n       expected {want}"))


def phase4(report: Report) -> None:
    """Combining files and inserting pages the way the app does it (pdf-cli merge and
    insert run the same engine; insert goes through the session and journal). Three
    sources: one with an outline, one with labels, one with a highlight and links."""
    work = OUT / "phase4"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    cli = pdf_cli()
    p = lambda name: str(work / name)

    run([cli, "gen", p("outline-base.pdf"), "--pages", "6", "--title", "Outline sample"])
    items = ["0+:1:Front", "1:2:Preface", "0:3:Chapter", "1:5:Section"]
    run([cli, "outline", "set", p("outline-base.pdf"), p("outline.pdf")] + [a for i in items for a in ("--item", i)])
    run([cli, "gen", p("labels-base.pdf"), "--pages", "5", "--title", "Labels sample"])
    run([cli, "labels", "set", p("labels-base.pdf"), p("labels.pdf"), "--rule", "1:roman-lower", "--rule", "3:decimal"])
    (work / "links.pdf").write_bytes(links_fixture())
    run([cli, "annot", "markup", p("links.pdf"), p("annotated.pdf"), "--page", "1", "--text", "quick brown fox", "--opacity", "0.6", "--author", "Lectrix Harness", "--note", "Harness note"])
    sources = [p("outline.pdf"), p("labels.pdf"), p("annotated.pdf")]

    # 1. Everything, in order, with the default options.
    merged = work / "merged.pdf"
    run([cli, "merge", str(merged)] + sources)
    print(merged.name)
    qpdf_check(merged, report)
    outline_and_label_checks(
        merged,
        report,
        pages=14,
        labels=[str(n) for n in range(1, 7)] + ["i", "ii", "1", "2", "3"] + ["1", "2", "3"],
        outline=[
            ("Outline sample", 0, [("Front", 0, [("Preface", 1, [])]), ("Chapter", 2, [("Section", 4, [])])]),
            ("Labels sample", 6, []),
            ("annotated", 11, []),
        ],
    )
    link_checks(merged, report, [(12, 13), (12, 12), (12, "https://example.org/")])
    case = work / "merged"
    case.mkdir(exist_ok=True)
    annotation_checks(merged, report, case)

    # 2. Picked pages: the annotated page turned and first, a page its link leads to, two
    # outline pages and one labeled page. The link to the page left out goes; a bookmark
    # whose page is gone stays as a heading when it has children.
    picked = work / "picked.pdf"
    run([cli, "merge", str(picked)] + sources + ["--pages", "3:1@90,3:3,1:2-3,2:4"])
    print(picked.name)
    qpdf_check(picked, report)
    outline_and_label_checks(
        picked,
        report,
        pages=5,
        labels=["1", "3", "2", "3", "2"],
        outline=[
            ("annotated", 0, []),
            ("Outline sample", 2, [("Front", None, [("Preface", 2, [])]), ("Chapter", 3, [])]),
            ("Labels sample", 4, []),
        ],
    )
    link_checks(picked, report, [(1, 1), (1, "https://example.org/")])
    case = work / "picked"
    case.mkdir(exist_ok=True)
    annotation_checks(picked, report, case)

    # 3. Inserting the annotated file before page 3 of the labeled one: the inserted pages
    # (no labels of their own) continue the roman numbering, and links still work.
    inserted = work / "inserted.pdf"
    run([cli, "insert", p("labels.pdf"), str(inserted), "--from", p("annotated.pdf"), "--at", "3"])
    print(inserted.name)
    qpdf_check(inserted, report)
    outline_and_label_checks(
        inserted,
        report,
        pages=8,
        labels=["i", "ii", "iii", "iv", "v", "1", "2", "3"],
        outline=[("annotated", 2, [])],
    )
    link_checks(inserted, report, [(3, 4), (3, 3), (3, "https://example.org/")])
    case = work / "inserted"
    case.mkdir(exist_ok=True)
    annotation_checks(inserted, report, case)


LOCAL_CORPUS = ROOT / "tests" / "local-corpus" / "files"


def qpdf_status(pdf: Path) -> int:
    """qpdf --check exit code: 0 clean, 3 warnings, 2 errors."""
    return subprocess.run([exe("qpdf"), "--check", str(pdf)], capture_output=True).returncode


def phase4_local(report: Report) -> None:
    """Combines three real files from the git-ignored local corpus (tests/local-corpus):
    one with an outline and links, one with page labels, bookmarks, many links and rotated
    pages, one with highlights made by another app. What each engine reads in the result
    must equal what it reads in the sources, moved to the pages' new places."""
    names = ["08-calibre-made.pdf", "24-rotated-some.pdf", "03-annot-highlight.pdf"]
    sources = [LOCAL_CORPUS / n for n in names]
    if not all(f.exists() for f in sources):
        print(f"local corpus not found in {LOCAL_CORPUS}; skipping")
        return
    work = OUT / "phase4-local"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    out = work / "combined.pdf"
    run([pdf_cli(), "merge", str(out)] + [str(f) for f in sources])
    print(out.name)
    # Not worse than the worst source (some real files already have qpdf warnings).
    worst = max(qpdf_status(f) for f in sources)
    got = qpdf_status(out)
    order = {0: 0, 3: 1, 2: 2}
    report.check(order.get(got, 2) <= order.get(worst, 2), f"{out.name}: qpdf --check exit {got} (worst source: {worst})")

    def shift_tree(items, offset):
        return [(i["title"], None if i["page"] is None else i["page"] + offset, shift_tree(i["children"], offset)) for i in items]

    for engine, read in (("PDFium", info_pdfium), ("pdf.js", info_pdfjs)):
        infos = [read(f) for f in sources]
        result = read(out)
        offsets = []
        total = 0
        for info in infos:
            offsets.append(total)
            total += info["pages"]
        report.check(result["pages"] == total, f"{out.name}: {engine} counts {result['pages']} pages (expected {total})")

        labels = []
        for info in infos:
            own = info["labels"] or []
            # A file without labels is numbered 1, 2, 3... from its first page.
            labels += own if any(own) else [str(n) for n in range(1, info["pages"] + 1)]
        got_labels = result["labels"] or []
        report.check(got_labels == labels, f"{out.name}: {engine} keeps every page's label" + ("" if got_labels == labels else f" (first difference at page {next((i for i, (a, b) in enumerate(zip(got_labels, labels)) if a != b), '?')})"))

        expected_links = sorted(
            ((l["page"] + offsets[k], l["uri"] if l["uri"] else (None if l["target"] is None else l["target"] + offsets[k])) for k, info in enumerate(infos) for l in info["links"]),
            key=str,
        )
        got_links = sorted(((l["page"], l["uri"] if l["uri"] else l["target"]) for l in result["links"]), key=str)
        internal = sum(1 for _, t in expected_links if isinstance(t, int))
        report.check(got_links == expected_links, f"{out.name}: {engine} finds all {len(expected_links)} links ({internal} internal) leading to the same pages as before" + ("" if got_links == expected_links else f" (got {len(got_links)}; first difference {next((a, b) for a, b in zip(got_links + [None] * len(expected_links), expected_links) if a != b)})"))

        # Each source's bookmarks, under a top-level bookmark for the file.
        tops = result["outline"]
        ok = len(tops) == len(infos)
        for k, (top, info) in enumerate(zip(tops, infos)):
            ok = ok and top["page"] == offsets[k] and strip_outline(top["children"]) == shift_tree(info["outline"], offsets[k])
        report.check(ok, f"{out.name}: {engine} nests each file's bookmarks, with their targets moved along")

    case = work / "annotations"
    case.mkdir(exist_ok=True)
    annotation_checks(out, report, case)


# --- Annotations -----------------------------------------------------------------------

MANUAL = ROOT / "target" / "test-output" / "manual" / "phase5"

# Page geometries every annotation type is checked on (AGENTS.md sections 5.1 and 9).
GEOMETRIES = {
    "normal": [],
    "rot90": ["--rotate", "90"],
    "rot180": ["--rotate", "180"],
    "rot270": ["--rotate", "270"],
    "crop": ["--crop", "100,150,500,700"],
    "crop-rot90": ["--crop", "100,150,500,700", "--rotate", "90"],
    "userunit": ["--user-unit", "2"],
}


def list_annotations(pdf: Path) -> list[dict]:
    """`pdf-cli annot list`, parsed: page (1-based), id, subtype, author, text, problems."""
    out = []
    for line in run([pdf_cli(), "annot", "list", str(pdf)]).stdout.splitlines():
        words = line.split()
        if len(words) < 4 or words[1] != "object":
            continue
        author = line.split(' author "', 1)[1].split('"', 1)[0] if ' author "' in line else ""
        text = line.split(' text "', 1)[1].rsplit('"', 1)[0] if ' text "' in line else ""
        problems = line.split(" needs-repair ", 1)[1] if " needs-repair " in line else ""
        out.append({"page": int(words[0][1:]), "id": int(words[2]), "subtype": words[3], "author": author, "text": text, "problems": problems})
    return out


def annotate_every_type(cli: str, src: Path, work: Path, name: str) -> Path:
    """One annotation of each type on its own page (so no two share a /Rect area), the way
    the app writes them: highlight, underline, strikeout, squiggly over the marker text,
    a sticky note, a drawing, a text box, and an area highlight (Alt-drag)."""
    steps = [
        ["annot", "markup", "--page", "1", "--kind", "highlight", "--text", "quick brown fox", "--opacity", "0.6", "--note", "Highlight note — ünïcödé"],
        ["annot", "markup", "--page", "2", "--kind", "underline", "--text", "quick brown fox", "--color", "0B8043"],
        ["annot", "markup", "--page", "3", "--kind", "strikeout", "--text", "quick brown fox", "--color", "D50000"],
        ["annot", "markup", "--page", "4", "--kind", "squiggly", "--text", "quick brown fox", "--color", "1A73E8"],
        ["annot", "note", "--page", "5", "--at", "120,90", "--text", "Sticky note text"],
        ["annot", "ink", "--page", "6", "--width", "2.5", "--color", "1A73E8", "--opacity", "0.8",
         "--stroke", ";".join(f"{100 + x},{220 + round(25 * __import__('math').sin(x / 12), 2)}" for x in range(0, 200, 2)),
         "--stroke", "120,300;160,340;200,300"],
        ["annot", "text", "--page", "7", "--rect", "72,200,300,210", "--text", "Text box written by Lectrix\nSecond line", "--size", "14", "--color", "C62828"],
        ["annot", "markup", "--page", "8", "--kind", "highlight", "--rect", "60,60,260,160", "--opacity", "0.5"],
    ]
    current = src
    for k, step in enumerate(steps):
        nxt = work / f"{name}-step{k + 1}.pdf"
        # Every command takes INPUT OUTPUT right after the subcommand.
        run([cli, step[0], step[1], str(current), str(nxt)] + step[2:] + ["--author", "Lectrix Harness"])
        current = nxt
    final = work / f"{name}.pdf"
    shutil.copyfile(current, final)
    return final


def phase5(report: Report) -> None:
    """Every annotation type on normal, rotated, cropped and UserUnit pages; edits; and the
    repair command on annotations "another app" wrote with each problem section 5.3 lists.

    Rendered at 4x: strikeouts, underlines and squiggles are lines under 1 pt thick, and at
    2x their anti-aliased edges (which rasterizers spread differently) outweigh the line
    itself in the colour comparison."""
    global SCALE
    SCALE = 4.0
    work = OUT / "phase5"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(MANUAL, ignore_errors=True)
    MANUAL.mkdir(parents=True, exist_ok=True)
    cli = pdf_cli()

    # 1. Every type on every page geometry.
    for name, extra in GEOMETRIES.items():
        src = work / f"gen-{name}.pdf"
        run([cli, "gen", str(src), "--pages", "8"] + extra)
        out = annotate_every_type(cli, src, work, f"types-{name}")
        print(out.name)
        qpdf_check(out, report)
        listed = list_annotations(out)
        report.check(len(listed) == 8 and not any(a["problems"] for a in listed), f"{out.name}: 8 annotations, none needs repair")
        case = work / name
        case.mkdir(exist_ok=True)
        annotation_checks(out, report, case)
        if name in ("normal", "rot90", "crop", "userunit"):
            shutil.copyfile(out, MANUAL / f"types-{name}.pdf")

    # 2. Edits on the normal file: recolour and re-note the highlight, turn the underline
    # into a highlight and the squiggly into an underline, move the note, resize the
    # drawing, retype the text box, delete the strikeout.
    src = work / "types-normal.pdf"
    ids = {a["subtype"]: a for a in list_annotations(src) if a["page"] != 8}
    edits = [
        ["--page", "1", "--id", str(ids["Highlight"]["id"]), "--color", "FF80AB", "--contents", "Edited note"],
        ["--page", "2", "--id", str(ids["Underline"]["id"]), "--type", "highlight"],
        ["--page", "4", "--id", str(ids["Squiggly"]["id"]), "--type", "underline"],
        ["--page", "5", "--id", str(ids["Text"]["id"]), "--bounds", "300,400,320,420"],
        ["--page", "6", "--id", str(ids["Ink"]["id"]), "--bounds", "80,400,400,560", "--width", "4"],
        ["--page", "7", "--id", str(ids["FreeText"]["id"]), "--contents", "Retyped, and longer: it wraps onto more lines than before in this box", "--size", "16"],
    ]
    current = src
    for k, e in enumerate(edits):
        nxt = work / f"edited-step{k + 1}.pdf"
        run([cli, "annot", "edit", str(current), str(nxt)] + e)
        current = nxt
    edited = work / "edited.pdf"
    run([cli, "annot", "delete", str(current), str(edited), "--page", "3", "--id", str(ids["StrikeOut"]["id"])])
    print(edited.name)
    qpdf_check(edited, report)
    after = list_annotations(edited)
    report.check(len(after) == 7 and not any(a["subtype"] == "StrikeOut" for a in after), f"{edited.name}: the strikeout is gone, 7 annotations left")
    report.check(any(a["text"] == "Edited note" for a in after), f"{edited.name}: the highlight's note was edited")
    retyped = {a["page"]: a["subtype"] for a in after if a["page"] in (2, 4)}
    report.check(retyped == {2: "Highlight", 4: "Underline"}, f"{edited.name}: the underline became a highlight and the squiggly an underline (got {retyped})")
    case = work / "edited"
    case.mkdir(exist_ok=True)
    annotation_checks(edited, report, case)
    shutil.copyfile(edited, MANUAL / "edited.pdf")

    # 3. Repair: annotations another app wrote with every problem repair fixes.
    broken = work / "problems.pdf"
    broken.write_bytes(problems_fixture())
    before = list_annotations(broken)
    print(broken.name)
    report.check(sum(1 for a in before if a["problems"]) == 8, f"{broken.name}: 8 annotations need repair (got {sum(1 for a in before if a['problems'])})")
    scratch = Report()
    scratch_dir = work / "problems-before"
    scratch_dir.mkdir(exist_ok=True)
    print("  (before repair, for information:)")
    annotation_checks(broken, scratch, scratch_dir)
    print(f"  before repair: {len(scratch.failures)} of {len(scratch.passes) + len(scratch.failures)} checks fail")
    repaired = work / "repaired.pdf"
    log = run([cli, "annot", "repair", str(broken), str(repaired)]).stdout
    print(repaired.name)
    qpdf_check(repaired, report)
    fixed = list_annotations(repaired)
    report.check(not any(a["problems"] for a in fixed), f"{repaired.name}: nothing needs repair any more")
    key = lambda a: (a["page"], a["id"], a["subtype"], a["author"], a["text"])
    report.check(sorted(map(key, fixed)) == sorted(map(key, before)), f"{repaired.name}: same annotations, authors and texts as before")
    report.check(log.count("annotation object") >= 8, f"{repaired.name}: every change is reported with its object number")
    case = work / "repaired"
    case.mkdir(exist_ok=True)
    annotation_checks(repaired, report, case)
    shutil.copyfile(broken, MANUAL / "problems-before-repair.pdf")
    shutil.copyfile(repaired, MANUAL / "problems-repaired.pdf")


def problems_fixture() -> bytes:
    """Annotations "another app" wrote, by hand, each with problems section 5.3 lists.
    Page 1 is upright, page 2 is turned 90 degrees."""
    content = "BT /F1 12 Tf 72 700 Td (Annotations written by another app, with problems.) Tj ET"
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 12 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] >>",
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        f"<< /Length {len(content)} >>\nstream\n{content}\nendstream",
        # Highlight: quads in the spec's counter-clockwise order, no appearance, no metadata,
        # /Rect smaller than the quads.
        "<< /Type /Annot /Subtype /Highlight /Rect [72 640 120 650] /C [1 0.9 0] /T (Other App) /Contents (Old highlight) /QuadPoints [72 630 300 630 300 660 72 660] >>",
        # Underline: clockwise quads, no appearance.
        "<< /Type /Annot /Subtype /Underline /Rect [72 580 300 610] /C [0 0.6 0] /T (Other App) /F 4 /NM (u-1) /M (D:20200101000000Z) /P 3 0 R /QuadPoints [72 610 300 610 300 580 72 580] >>",
        # Ink: no appearance, /Rect smaller than the strokes.
        "<< /Type /Annot /Subtype /Ink /Rect [100 400 150 450] /C [0 0 1] /T (Other App) /BS << /W 3 >> /InkList [[100 400 180 520 260 400 340 520]] >>",
        # Sticky note: no appearance, no /NM, /M or /P.
        "<< /Type /Annot /Subtype /Text /Rect [400 650 420 670] /C [1 0.8 0] /F 4 /T (Other App) /Contents (A note from another app) >>",
        # Text box: no appearance.
        "<< /Type /Annot /Subtype /FreeText /Rect [72 300 300 340] /F 4 /NM (ft-1) /M (D:20200101000000Z) /P 3 0 R /T (Other App) /DA (/Helv 14 Tf 0.8 0 0 rg) /Contents (Typed by another app) >>",
        # Square: no appearance, no /F.
        "<< /Type /Annot /Subtype /Square /Rect [350 200 500 280] /C [0.6 0 0.6] /NM (sq-1) /M (D:20200101000000Z) /P 3 0 R /T (Other App) /BS << /W 2 >> >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate 90 /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [13 0 R 14 0 R] >>",
        # Page 2, turned: a highlight in counter-clockwise order with no appearance, and a
        # note without one.
        "<< /Type /Annot /Subtype /Highlight /Rect [72 690 300 715] /C [1 0.9 0] /T (Other App) /F 4 /NM (h-2) /M (D:20200101000000Z) /P 12 0 R /QuadPoints [72 690 300 690 300 715 72 715] >>",
        "<< /Type /Annot /Subtype /Text /Rect [400 500 420 520] /C [0.2 0.6 1] /F 4 /NM (n-2) /M (D:20200101000000Z) /T (Other App) /Contents (Note on a turned page) >>",
    ]
    return assemble_pdf(objects)


def phase5_local(report: Report) -> None:
    """Repair on the real files in the git-ignored local corpus that need it: an Acrobat
    highlight whose /Rect misses its quads, and a stamp without /NM and /P."""
    names = {"01-annot-caret.pdf", "04-annot-stamp.pdf"}
    sources = [LOCAL_CORPUS / n for n in sorted(names)]
    if not all(f.exists() for f in sources):
        print(f"local corpus not found in {LOCAL_CORPUS}; skipping")
        return
    work = OUT / "phase5-local"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True, exist_ok=True)
    for src in sources:
        before = list_annotations(src)
        needing = {a["id"] for a in before if a["problems"]}
        out = work / src.name
        run([pdf_cli(), "annot", "repair", str(src), str(out)])
        print(out.name)
        got, worst = qpdf_status(out), qpdf_status(src)
        order = {0: 0, 3: 1, 2: 2}
        report.check(order.get(got, 2) <= order.get(worst, 2), f"{out.name}: qpdf --check exit {got} (source: {worst})")
        after = list_annotations(out)
        report.check(not any(a["problems"] for a in after), f"{out.name}: nothing needs repair any more ({len(needing)} annotations were repaired)")
        key = lambda a: (a["page"], a["id"], a["subtype"], a["author"], a["text"])
        report.check(sorted(map(key, after)) == sorted(map(key, before)), f"{out.name}: same annotations, authors and texts as before")
        pages = {a["page"] for a in before if a["id"] in needing}
        case = work / src.stem
        case.mkdir(exist_ok=True)
        annotation_checks(out, report, case, pages=pages)


def main(argv: list[str]) -> int:
    # Titles in the checks are Unicode; Windows consoles (CI) default to a code page.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    report = Report()
    if argv[:1] == ["phase0"]:
        phase0(report)
    elif argv[:1] == ["phase2"]:
        phase2(report)
    elif argv[:1] == ["phase3"]:
        phase3(report)
    elif argv[:1] == ["phase4"]:
        phase4(report)
    elif argv[:1] == ["phase4-local"]:
        phase4_local(report)
    elif argv[:1] == ["phase5"]:
        phase5(report)
    elif argv[:1] == ["phase5-local"]:
        phase5_local(report)
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
