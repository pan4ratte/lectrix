# Planned tools

Part of the status: [docs/status.md](../status.md) is the index a session reads, with one line for each item; this file has the plan in full. Keep both current.

Decided 2026-10-19, to be built in this order, from easiest to hardest. Each is a tool
on the start screen. The bar is quality competitive with Acrobat (and, for OCR, ABBYY
FineReader) wherever an open-source approach can reach it. Each feature's spec and
AGENTS.md section 1 change in the commit that builds it.

1. **Unlock:** built 2026-10-19 (section 6.8, ADR 0025). On the start screen and in the
   File menu: the password and restrictions removed, given any password that opens the
   file, in the file itself or a copy.
   Not yet tried by hand in Acrobat: the copy opening there without a password and with
   nothing forbidden (`docs/manual-checklist.md`, "Unlock").
   **Protect** and **Document properties** were added beside it the same day (sections
   6.9 and 6.10, ADR 0026): a password, restrictions or both, as a tool on a file or as a
   document's setting saved with it next, AES-256 only; and the document's file,
   description (editable), security and fonts in one dialog.
   Not yet tried by hand in Acrobat: each restriction as its Security tab lists it, a
   password outside ASCII, and a description changed in a PDF/A file passing Preflight
   (`docs/manual-checklist.md`, "Protect" and "Document properties").
2. **Compress:** built 2026-10-19 (section 6.11, ADR 0027). Presets, or resolution,
   compression, quality and colour for images (separately for colour, greyscale and
   black-and-white ones); fonts subset, removals offered only for what a file has,
   duplicates merged; the result shown with a before-and-after preview before it is
   saved over the file or as a copy. Images are replaced where they are, with a pipeline
   of Lectrix's own: MuPDF's `pdf_rewrite_images` rewrites every content stream and
   misjudges images in forms. Gaps under "Compress" in [known-gaps.md](known-gaps.md).
3. **Compare** two documents: a word-by-word diff of their structured text, shown side by
   side in split view (ADR 0023). It lines pages up across pages added or removed, has a
   visual mode for scans and drawings, a list of changes to step through and a summary
   report. It needs no new engine or dependency.
4. **OCR, with compression for scans:** the best result that is realistic with
   open-source engines, offline and still lightweight. The engine (Tesseract 5, which
   MuPDF supports, or PaddleOCR through ONNX Runtime; both Apache-2.0) is chosen by
   measuring accuracy, speed and size on scanned files from the local corpus, with
   preprocessing (deskew, despeckle, binarization). The invisible text layer is written
   into the existing pages: MuPDF's own OCR writer renders every page to an image, losing
   vector content and annotations. That layer has to select and search correctly in
   Acrobat, Edge, Firefox and Foxit. Compression for scans (JBIG2 for black-and-white
   pages, MRC for the rest) shares its image analysis with OCR (Leptonica, jbig2enc). OCR
   is under "Not in v1" in AGENTS.md section 1.
