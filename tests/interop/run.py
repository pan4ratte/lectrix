"""Cross-renderer interop harness (AGENTS.md section 9).

    python tests/interop/run.py phase0      # build the Phase 0 outputs with pdf-cli and check them
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


def main(argv: list[str]) -> int:
    report = Report()
    if argv[:1] == ["phase0"]:
        phase0(report)
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
