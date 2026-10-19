# 6.11 Compress

Part of the Lectrix specification. [AGENTS.md](../../AGENTS.md) holds the rules every change follows and [README.md](README.md) lists these files; sections keep the numbers they share, so "section 5.4" is in AGENTS.md and "section 6.5" in [annotations.md](annotations.md).

A PDF written smaller, in the file itself or in a copy (ADR 0027): its images downsampled and recompressed as the user chooses, its fonts subset, what the user ticks removed, and its structure packed. Made for documents made on a computer; scans compress best with OCR's JBIG2 and MRC, which come later (`docs/status.md`, "Planned tools").

- **Where it is:** **Compress…** on the start screen, after Protect…, with a shrinking arrows icon (section 6.1), and **File > Compress…**, after Protect…, at any time (a command without a default shortcut, section 6.6). With a document shown, it compresses that document's file; otherwise, and from the start screen, it asks for one PDF in the Open dialog ("Compress a PDF"). A document with unsaved changes asks first: "Save your changes first? report.pdf has unsaved changes. Compressing works on the file as it is saved, so save them first." (**Save**, Cancel). A document that is no file yet (section 7) is saved first the same way.
- **The password.** A protected file asks for it in the password dialog ("report.pdf is protected. Enter its password to compress it.", the button **Continue**); any password that opens it will do, as for Unlock (ADR 0025). A document open in a tab needs none.
- **The dialog**, titled "Compress report.pdf", in three steps in one window.
  1. **Settings.**
     - Under the title, the file's size and pages ("48.2 MB · 312 pages"), and a bar of what that size is made of, each part named with its size below it: Images, Fonts, Page content, Attachments, Thumbnails, Other apps' data, Other (structure, metadata, colour profiles). Parts under 1% of the file are left out of the bar but kept in its list.
     - **Preset**, a dropdown: **Lossless** ("Nothing visible changes: images packed tighter without losing a pixel, fonts subset, structure compacted"), **High quality** ("For print: images at 300 dpi, JPEG quality 85"), **Balanced** ("For reading on screen and sharing: images at 150 dpi, JPEG quality 75"), **Smallest** ("For email: images at 96 dpi, JPEG quality 50"), and **Custom**, which it reads as soon as a setting below differs from the preset chosen.
     - **Images**:
       - **Downsample images** (a checkbox) **above** [225] dpi **to** [150] dpi: two number fields, the first never below the second. A field left empty or out of range (9 to 2400 dpi) says so under itself and disables Compress. **Method**: Bicubic (sharp) or Average (smooth), a dropdown.
       - **Compression**: JPEG, JPEG 2000, or Keep as it is (each image keeps its kind: lossless stays lossless, JPEG stays JPEG).
       - **Quality**: a slider from 1 to 100, with its value; "Low" and "High" at its ends.
       - **Colour**: Keep colours, Greyscale, or Black and white, a dropdown. It changes images only: text, drawings and annotations keep their colours.
       - Black-and-white images are always written losslessly. In this simple form they are downsampled to twice the resolution set, at least 300 dpi, above one and a half times that, said in a line under the fields ("Black-and-white images: 300 dpi").
       - **Separate settings for colour, greyscale and black-and-white images**, a disclosure (closed at first, remembered). Opened, the settings above are three groups: **Colour images** and **Greyscale images**, each with its downsampling, method, compression and quality, and **Black-and-white images** with its downsampling and method ("Always lossless: CCITT Group 4 or ZIP, whichever is smaller"). Colour stays one setting, for all images.
     - **Fonts and structure**: **Subset embedded fonts** ("Keeps only the characters the document uses. Fonts that form fields type with stay whole."), **Compress the file's structure** ("Packs its objects together. Needs PDF 1.5, which every current reader opens."). Both ticked in every preset.
     - **Remove**, shown only with what the file has, each with its count or size: **Page thumbnails** (ticked in every preset), **Other apps' editing data** ("Illustrator and Photoshop data the PDF doesn't show", ticked in every preset), **Attached files** ("2 files, 5.0 MB"), **JavaScript** ("Forms that calculate or check their fields stop doing so."), **Tags for accessibility** ("Screen readers and reflow use them. Remove them only if no one needs to read the file aloud."). The last three are never ticked by a preset.
     - Under them, as they apply: "This file is signed. Its signatures will not be valid in the compressed file, because the whole file is rewritten." and "This file claims compliance with PDF/A-1b. The compressed file will not claim it."
     - **Compress** (the main button) and **Cancel** (also Esc).
  2. **Compressing.** The settings give way to a progress bar and what is being done: "Reading pages… 34 of 312", "Compressing images… 120 of 410", "Subsetting fonts…", "Writing…". **Stop** ends it; nothing is written, and the settings come back as they were.
  3. **The result.** "48.2 MB → 6.1 MB, 87% smaller", and below it what was done ("410 images: 380 recompressed, 212 downsampled. 14 fonts subset. Removed: 312 thumbnails."; images Lectrix can't rewrite safely are counted: "6 left as they were"). A preview shows a page as it was and as it will be, side by side (the dialog widens to 1000 px for it; each pane is the size of the page at 100%, whatever the zoom, up to half the dialog's width and as tall as the window leaves room for, 300 to 720 px, and scrolls beyond that), at **100%**, **200%** or **400%** (a segmented control), the two scrolled together; it starts on the page with the largest image changed, and **‹** and **›** go through the pages. **Save** (the main button), **Save as a copy**, **Back to settings** (the settings as they were), and **Cancel** (also Esc), which writes nothing.
     - If the result is not smaller: "report.pdf is already compact. Compressed with these settings it would be 48.3 MB, no smaller than it is." with **Back to settings** and **Close**; nothing can be saved.
- **Save** writes the result over the file itself (the atomic replace of section 7) and opens it in a tab: "report.pdf is now 6.1 MB (it was 48.2 MB)." If it is open in a tab already without unsaved changes, that tab reads it again at once, where it was; the change is not reported as one by another program. A tab with unsaved changes is left as it is, and warns that its file changed on disk (section 7).
- **Save as a copy** shows the Save dialog ("Save a compressed copy"), in the original's folder with "report (compressed).pdf". The original is only read; the copy opens in a tab: "Saved report (compressed).pdf: 6.1 MB, 87% smaller. The original file is unchanged." A Save dialog given up comes back to the result. A copy saved over the original is the same as Save.
- **PDF/A.** The compressed file no longer claims PDF/A (section 5.4): the notification that a save removed the claim follows the one above.
- **A file that can't be opened** (damaged, not a PDF, protected with a certificate) says so in an error notification, as opening it does.

## What is done to images

- **Where an image is drawn.** Every page is read, through forms, patterns, soft masks and annotation appearances, to find the resolution each image is drawn at; an image drawn in several places keeps the resolution of the largest. Images nothing draws keep their resolution.
- **Downsampling** an image drawn finer than the threshold brings it to the resolution set, never larger than it was. Bicubic is a Catmull-Rom filter widened to the reduction; Average is the mean of the area each new pixel covers.
- **Kinds of images.** An image is colour, greyscale or black and white (one bit a pixel, and image masks) as stored. A colour image whose pixels are all grey is treated, and stored, as greyscale; a greyscale one stored losslessly with nothing but black and white is stored as one bit a pixel. Neither loses anything.
- **Compression.** With JPEG or JPEG 2000 chosen, a colour or greyscale image is encoded so, at the quality set, unless it looks drawn rather than photographed (a few colours, or mostly flat areas: screenshots, charts, logos), which stays lossless (Flate with PNG predictors) and sharp. JPEG is baseline, colour halved both ways below quality 90. With Keep as it is, JPEG images are re-encoded at the quality set only when downsampled or changed in colour, and lossless images are packed tighter without changing a pixel.
- **Black and white** is written as CCITT Group 4 or Flate, whichever is smaller, both lossless. Images turned black and white by the Colour setting are split at Otsu's threshold.
- **Soft masks and stencil masks** follow their image in resolution and are always written losslessly.
- **Only smaller.** An image is replaced only if what replaces it is smaller; a JPEG re-encoded at its own size and colours only if that saves a tenth of it, since each re-encoding loses a little.
- **Left as they were:** images with colour-key masks, in separations, DeviceN or Lab, and JBIG2 images. Inline images are left too: they are part of the page's contents.
- **Pages, annotations and form fields are not rewritten**: an image is replaced where it is, under its own object number.

## Settled details

- A tool on a file, like Unlock: it works on the file as saved, and Save, Save as a copy or Cancel is asked once the result is known (the user's decision, 2026-10-19). Save As (optimized) stays as it is: lossless and immediate.
- FineReader's simple settings (resolution, compression, quality, colour), with Acrobat's separate settings for colour, greyscale and black-and-white images behind a disclosure (the user's decision, 2026-10-19).
- Colour mode changes images only (the user's decision, 2026-10-19).
- The result is computed before anything is written, with its size and a before-and-after preview (the user's decision, 2026-10-19).
- Removals are offered only for what the file has (the user's decision, 2026-10-19); XMP metadata is not offered.
- A compressed file no longer claims PDF/A (the user's decision, 2026-10-19).
- JBIG2 and MRC wait for OCR, which brings Leptonica (the user's decision, 2026-10-19).
- The dialog opens with the settings used last, and the preset they came from (the user's decision, 2026-10-19).
- Encryption is kept as it is: the compressed file opens with the same passwords and has the same restrictions. A file whose restrictions forbid changes can be compressed all the same, with any password that opens it, as Unlock allows (ADR 0025).
- Stamps and other annotations' appearances are compressed like any page content; the annotations themselves are not touched.
