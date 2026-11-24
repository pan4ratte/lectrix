"""What the README's screenshots need that Lectrix and the webview can't make (screenshots.mjs
runs it).

    python tests/readme/prepare.py outline BOOK OUT
        BOOK with its bookmarks' titles cleaned of the asterisks the book gives its
        sections, and every chapter closed, so the Bookmarks panel starts as a reader
        would leave it.

    python tests/readme/prepare.py edit OLD NEW
        A later version of OLD (pages of section 25.1) for Compare: a few words changed,
        as an editor would, and the version in the footers raised.

    python tests/readme/prepare.py frame PNG WEBP THEME
        The screenshot PNG with rounded corners, transparent outside them, and a thin
        outline, as a window has on screen, written as WEBP; THEME (light or dark) is the
        app's, and decides the outline's colour so it shows on GitHub's page either way.
        The README can't style its images (GitHub drops style attributes), and the webview
        captures without transparency, so this is drawn here.

Needs PyMuPDF and Pillow (pip install pymupdf pillow). Not part of Lectrix or its tests: it
only prepares the screenshots.
"""

import re
import sys

import pymupdf
from PIL import Image, ImageChops, ImageDraw


def outline(book, out):
    doc = pymupdf.open(book)
    # The titles' no-break spaces become plain ones: PyMuPDF would write them as byte 0xA0,
    # which PDFDocEncoding reads as a euro sign.
    toc = [
        [level, title.replace(" ", " ").rstrip("*").rstrip(), page]
        for level, title, page in doc.get_toc(simple=True)
    ]
    # collapse=1: every entry below the top level starts closed.
    doc.set_toc(toc, collapse=1)
    doc.save(out)


# (page index in OLD, or None for every page; a word or words as they stand; what replaces
# them). Words are matched as the page draws them: within a TJ array, strings apart by a
# small offset (kerning) are one word, and a large negative offset is the gap between words
# (the book's justified lines have no space characters).
EDITS = [
    (0, "much", "far"),
    (0, "difficult", "real"),
    (0, "important", "major"),
    (0, "counted", "tallied"),
    (1, "imagine", "suppose"),
    (None, "col11992/1.13", "col11992/1.14"),
]

# A content stream's tokens: strings, (literal) or <hex>; array brackets; names; numbers;
# operators.
TOKEN = re.compile(
    rb"\((?:\\.|[^\\)])*\)|<[0-9A-Fa-f\s]*>|\[|\]|/[^\s/\[\]()<>{}%]+|[-+]?(?:\d+\.?\d*|\.\d+)|[A-Za-z'\"*]+",
    re.S,
)
# An offset in a TJ array this negative or more is a gap between words (about a space's
# width, more in a justified line), not kerning (tens of units either way).
WORD_GAP = -100
ESCAPES = {b"n": 10, b"r": 13, b"t": 9, b"b": 8, b"f": 12, b"(": 40, b")": 41, b"\\": 92}


def literal_bytes(token):
    """The bytes a (literal) or <hex> string stands for."""
    if token.startswith(b"<"):
        digits = re.sub(rb"\s", b"", token[1:-1])
        return bytes.fromhex((digits + b"0" * (len(digits) % 2)).decode())
    body, out, i = token[1:-1], bytearray(), 0
    while i < len(body):
        c = body[i : i + 1]
        if c != b"\\":
            out += c
            i += 1
            continue
        nxt = body[i + 1 : i + 2]
        if nxt in ESCAPES:
            out.append(ESCAPES[nxt])
            i += 2
        elif nxt.isdigit():
            octal = re.match(rb"[0-7]{1,3}", body[i + 1 : i + 4]).group()
            out.append(int(octal, 8) & 0xFF)
            i += 1 + len(octal)
        elif nxt in (b"\r", b"\n"):
            i += 2
        else:
            out += nxt
            i += 2
    return bytes(out)


def to_unicode(doc, font_xref):
    """A simple font's codes and the text each stands for: from its ToUnicode CMap, or
    else from its encoding (the book's headings and footers are in MacRomanEncoding)."""
    kind, ref = doc.xref_get_key(font_xref, "ToUnicode")
    if kind != "xref":
        encoding = doc.xref_get_key(font_xref, "Encoding")[1]
        codec = {"/MacRomanEncoding": "mac_roman", "/WinAnsiEncoding": "cp1252"}.get(encoding)
        return {code: bytes([code]).decode(codec, "replace") for code in range(32, 256)} if codec else {}
    cmap = doc.xref_stream(int(ref.split()[0])).decode("latin-1")
    codes = {}
    for section in re.findall(r"beginbfchar(.*?)endbfchar", cmap, re.S):
        for code, text in re.findall(r"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>", section):
            codes[int(code, 16)] = bytes.fromhex(text).decode("utf-16-be")
    for section in re.findall(r"beginbfrange(.*?)endbfrange", cmap, re.S):
        for low, high, text in re.findall(r"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>", section):
            first = int(text, 16)
            for offset, code in enumerate(range(int(low, 16), int(high, 16) + 1)):
                codes[code] = chr(first + offset)
    return codes


def words(stream):
    """The strings a content stream draws, grouped into words: (start, end, font, codes) for
    each, start and end its place in the stream."""
    out, font, previous, in_array, group = [], None, None, False, []

    def close():
        if group:
            out.append((group[0][0], group[-1][1], font, b"".join(g[2] for g in group)))
            group.clear()

    for token in TOKEN.finditer(stream):
        t = token.group()
        if t[:1] in (b"(", b"<"):
            group.append((token.start(), token.end(), literal_bytes(t)))
            if not in_array:
                close()
        elif t == b"[":
            in_array = True
        elif t == b"]":
            close()
            in_array = False
        elif in_array and re.fullmatch(rb"[-+]?[\d.]+", t):
            if float(t) <= WORD_GAP:
                close()
        elif t == b"Tf" and previous is not None:
            font = previous
        if t.startswith(b"/"):
            previous = t[1:].decode()
        elif not re.fullmatch(rb"[-+]?[\d.]+", t):
            previous = None
    return out


def edit(old, new):
    doc = pymupdf.open(old)
    made = {words: 0 for _, words, _ in EDITS}
    for page in doc:
        page.clean_contents()
        xref = page.get_contents()[0]
        stream = doc.xref_stream(xref)
        fonts = {name: to_unicode(doc, font_xref) for font_xref, _, _, _, name, *_ in page.get_fonts()}
        edits = [(w, replacement) for index, w, replacement in EDITS if index in (None, page.number)]
        out, at = bytearray(), 0
        for start, end, font, raw in words(stream):
            codes = fonts.get(font, {})
            text = "".join(codes.get(b, "�") for b in raw)
            changed = text
            for w, replacement in edits:
                pattern = r"(?<!\w)" + re.escape(w) + r"(?!\w)"
                if re.search(pattern, changed):
                    changed = re.sub(pattern, replacement, changed)
                    made[w] += 1
            if changed == text:
                continue
            back = {t: c for c, t in codes.items()}
            missing = sorted({ch for ch in changed if ch not in back})
            if missing:
                sys.exit(f"edit: the font {font} on page {page.number + 1} can't draw {missing}")
            out += stream[at:start] + b"<" + bytes(back[ch] for ch in changed).hex().encode() + b">"
            at = end
        out += stream[at:]
        doc.update_stream(xref, bytes(out))
    for index, w, _ in EDITS:
        if made[w] == 0 or (index is not None and made[w] > 1):
            sys.exit(f"edit: “{w}” was found {made[w]} times, not once")
    doc.save(new, garbage=1)


# The screenshots are captured at 1.5 times their CSS size (screenshots.mjs): corners of
# 12 CSS pixels (about 6 as GitHub shows them, half size) and an outline of one.
RADIUS = 18
OUTLINE = 1.5
OUTLINE_COLOR = {"light": (0, 0, 0, 40), "dark": (255, 255, 255, 40)}
# Shapes are drawn this many times larger and scaled down, for smooth curves.
SMOOTH = 4


def frame(png, webp, theme):
    image = Image.open(png).convert("RGBA")
    big = (image.width * SMOOTH, image.height * SMOOTH)
    box = (0, 0, big[0] - 1, big[1] - 1)
    mask = Image.new("L", big, 0)
    ImageDraw.Draw(mask).rounded_rectangle(box, RADIUS * SMOOTH, fill=255)
    outline = Image.new("RGBA", big, (0, 0, 0, 0))
    ImageDraw.Draw(outline).rounded_rectangle(
        box, RADIUS * SMOOTH, outline=OUTLINE_COLOR[theme], width=round(OUTLINE * SMOOTH)
    )
    image.alpha_composite(outline.resize(image.size, Image.LANCZOS))
    image.putalpha(ImageChops.multiply(image.getchannel("A"), mask.resize(image.size, Image.LANCZOS)))
    image.save(webp, "WEBP", quality=90, alpha_quality=100, method=6)


if __name__ == "__main__":
    commands = {"outline": (outline, 2), "edit": (edit, 2), "frame": (frame, 3)}
    command, arity = commands.get(sys.argv[1] if len(sys.argv) > 1 else "", (None, 0))
    if command is None or len(sys.argv) != 2 + arity:
        sys.exit(__doc__)
    command(*sys.argv[2:])
