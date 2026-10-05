# 0009: Google Sans is the UI font, bundled with the app

- Status: accepted (the user's decision, 2026-10-05)
- Date: 2026-10-05

## Context

AGENTS.md section 8 set the UI font stack to Segoe UI Variable, Segoe UI, then the
system UI fonts, so that Lectrix would look like other Windows 11 apps. The Lectrix
brand kit (the Swan icon, 2026-10-05) names Google Sans as the brand typeface, and the
user asked for it to be the app's main font.

Google Sans is not installed on Windows, and Lectrix makes no network requests, so the
font has to ship with the app. It is © 2025 The Google Sans Project Authors under the
SIL Open Font License 1.1, with no Reserved Font Name. The OFL allows the font to be
bundled with any software and to be modified (subset) under the same name when no name
is reserved. The font stays a separate work under the OFL; it does not change the
license of Lectrix's code.

## Decision

1. **Bundle Google Sans 14.000 in the frontend** (`src/lib/assets/fonts/`): the brand
   kit's Latin, Cyrillic and Greek build, upright and italic, about 200 KB each. Both
   are variable fonts covering weights 400 to 700, so the UI's existing weights keep
   working.
2. **Keep the Segoe UI stack as the fallback** in `--font-sans`. Characters outside the
   bundled build (CJK, Arabic, Hebrew, Indic scripts and others, for example in bookmark
   titles or file names) are drawn in Segoe UI or the system font for that script, one
   character at a time.
3. **`font-display: block`.** The files load from the app itself in a few milliseconds.
   Waiting for them avoids text appearing in Segoe UI first and then changing width.
4. **Licensing.** The OFL text ships next to the font files, `tests/licenses/notices.py`
   lists the font with its license text in `THIRD_PARTY_LICENSES.md`, and
   `THIRD_PARTY_NOTICES.md` has a row for it. The license check accepts OFL-1.1 for
   fonts only, never for code.
5. **Test.** `tests/e2e/specs/about.test.mjs` checks that the font loads under the app's
   CSP (`font-src 'self'`) and is the body font.

The annotation text box editor keeps Helvetica, because it shows what the saved
FreeText annotation will look like (section 5.1, rule 7).

## Alternatives considered

- **The full build (about 1.6 MB per style)** adds Armenian, Georgian, Hebrew, Indic,
  Thai, Khmer and Ethiopic. It was not chosen because the UI's own text is English, and
  the Segoe UI fallback already draws those scripts in user content. It can be switched
  in by replacing the two files.
- **Keeping Segoe UI** stays closest to native Windows 11 apps, but the user chose the
  brand typeface.
- **Loading the font from Google Fonts** would break the offline rule (section 2).
