"""PDFium leg of the interop harness (via pypdfium2).

    python pdfium_tool.py info   <file.pdf>                       -> JSON on stdout
    python pdfium_tool.py render <file.pdf> <page> <scale> <on|off> <out.png>

`page` is 1-based. `on|off` controls whether annotation appearances are drawn.
"""

import json
import sys

import pypdfium2 as pdfium
import pypdfium2.raw as raw


def outline(pdf):
    # get_toc yields a flat, depth-annotated list; rebuild the tree.
    root = {"children": []}
    stack = [(-1, root)]
    for item in pdf.get_toc(max_depth=64):
        dest = item.get_dest()
        count = item.get_count()
        node = {
            "title": item.get_title(),
            "page": dest.get_index() if dest else None,
            # Expanded state: the sign of /Count (0 for an item without children).
            "open": None if count == 0 else count > 0,
            # View coordinates of the destination ([left, top, zoom] for /XYZ).
            "view": list(dest.get_view()[1]) if dest else None,
            "children": [],
        }
        while stack[-1][0] >= item.level:
            stack.pop()
        stack[-1][1]["children"].append(node)
        stack.append((item.level, node))
    return root["children"]


SUBTYPES = {
    getattr(raw, name): name.removeprefix("FPDF_ANNOT_").title().replace("_", "")
    for name in dir(raw)
    if name.startswith("FPDF_ANNOT_") and isinstance(getattr(raw, name), int)
}


def annotations(pdf):
    out = []
    for i in range(len(pdf)):
        page = pdf[i]
        count = raw.FPDFPage_GetAnnotCount(page)
        for k in range(count):
            annot = raw.FPDFPage_GetAnnot(page, k)
            subtype = raw.FPDFAnnot_GetSubtype(annot)
            rect = raw.FS_RECTF()
            raw.FPDFAnnot_GetRect(annot, rect)
            has_ap = bool(raw.FPDFAnnot_GetAP(annot, raw.FPDF_ANNOT_APPEARANCEMODE_NORMAL, None, 0) > 2)
            out.append(
                {
                    "page": i + 1,
                    "subtype": SUBTYPES.get(subtype, str(subtype)),
                    "rect": [rect.left, rect.bottom, rect.right, rect.top],
                    "hasAppearance": has_ap,
                }
            )
            raw.FPDFPage_CloseAnnot(annot)
    return out


def info(path):
    pdf = pdfium.PdfDocument(path)
    result = {
        "pages": len(pdf),
        "labels": [pdf.get_page_label(i) for i in range(len(pdf))],
        "outline": outline(pdf),
        "annotations": annotations(pdf),
    }
    pdf.close()
    return result


def render(path, page_number, scale, draw_annots, out):
    pdf = pdfium.PdfDocument(path)
    page = pdf[page_number - 1]
    bitmap = page.render(scale=scale, draw_annots=draw_annots, fill_color=(255, 255, 255, 255))
    bitmap.to_pil().convert("RGB").save(out)
    pdf.close()


def main(argv):
    cmd, path, *rest = argv
    if cmd == "info":
        sys.stdout.write(json.dumps(info(path)))
    elif cmd == "render":
        page, scale, mode, out = rest
        render(path, int(page), float(scale), mode == "on", out)
    else:
        raise SystemExit(f"unknown command {cmd}")


if __name__ == "__main__":
    main(sys.argv[1:])
