# 0005: An index of installed fonts replaces font-kit

- Status: accepted (Phase 2; binding choice confirmed by the user on 2026-10-03)
- Date: 2026-10-03

## Context

When a PDF uses a font it does not embed, MuPDF asks a system font hook for it. Lectrix sends
the base-14 names and their unresolvable aliases straight to MuPDF's built-in fonts
(`fonts.rs`, Phase 1). Every other name goes to the `mupdf` crate's `system-fonts`
feature, which asks `font-kit`.

On Windows, `font-kit` has no index. For a PostScript name such as `TimesNewRomanPSMT` it
loads every installed font in turn and compares names (about 1.3 s per name, measured in
Phase 1); a family name such as `Times New Roman` is found quickly, but only after that
scan has failed. The answer is cached for that name only. In the user's 554-file library,
18 files took over 1 s to show their first page, and the slow part was this lookup
(`docs/progress.md`, Phase 1 local corpus). Section 2 sets a 1 s target for the first
page.

## Decision

1. **Build an index of installed fonts once, on the background warm-up thread**, and use
   it instead of `font-kit`. Each entry holds a face's PostScript name, its family names
   (every language), weight, stretch, style, file path and face index. Lookups that
   arrive while the index is being built wait for it.
2. **Read the index from DirectWrite's system font set** (`IDWriteFactory3::
   GetSystemFontSet`, then `GetPropertyValues` per face). DirectWrite keeps these
   properties in its font cache, so no font file is opened to build the index. Only the
   face that a lookup selects is read from disk, once per process.
3. **Match the way the crate did, so documents render with the same fonts**:
   - an exact PostScript name match first;
   - otherwise strip a trailing `MT`, `PS` or `IdentityH` (in that order, as the crate
     does) and look the rest up as a family name, ignoring case and accepting any
     language's name (what DirectWrite's `FindFamilyName` does for `font-kit`), then pick
     a face with the CSS Fonts 3 §5.2 algorithm (stretch, then style, then weight) for the
     requested bold and italic, as `font-kit` does;
   - when MuPDF needs exact metrics, refuse a face that is not bold or italic as
     requested;
   - CJK requests use the same per-ordering family lists as the crate, serif first, then
     sans, and CJK script fallback is served the same way. Other scripts keep falling
     back to MuPDF.
4. **Platform split.** The index and the matching are portable (`fonts/index.rs`).
   Listing the installed fonts goes through a `SystemFonts` trait in a new
   `pdf-core/src/platform/` module, with a DirectWrite implementation for Windows. Other
   platforms list no fonts until Phase 6, which adds fontconfig (Linux) and Core Text
   (macOS). This module is separate from the app crate's `platform` module because
   `pdf-core` must not depend on the app crate.
5. **Binding: the `windows` crate** (0.62.2, MIT OR Apache-2.0), with the
   `Win32_Graphics_DirectWrite` feature. It is already in the dependency tree through
   Tauri's `webview2-com`, so no new crate is downloaded; the feature adds some compile
   time. DirectWrite is a COM API, which `windows-sys` cannot call.
6. **Turn off the `mupdf` crate's `system-fonts` feature**, which removes `font-kit`,
   `dwrote` and their dependencies from the build.

## Consequences

- First pages of documents with non-embedded fonts no longer wait for a font scan; the
  cost is building the index once at startup, off the UI thread.
- Font data is read into memory and kept for the life of the process, as the crate did,
  so MuPDF can share the bytes. This is bounded by the distinct fonts documents ask for.
- Windows 10 1709 or later is needed for the font set properties used (Lectrix supports
  1809 and later).
- macOS and Linux builds render non-embedded, non-base-14 fonts with MuPDF's substitutes
  until Phase 6. This is listed as a platform gap.

## Alternatives considered

- **Keep `font-kit` and cache answers on disk.** Still slow the first time for each name
  on each machine, and the cache needs invalidating when fonts are installed or removed.
- **Read the registry's list of installed fonts.** It gives display names and files but
  no PostScript names; getting those means opening every font file (the same scan).
- **Parse every installed font file's name table ourselves.** Correct and portable, but
  it reads hundreds of files (tens of megabytes) on every start; DirectWrite already
  keeps this information.
- **Hand-written COM declarations over `windows-sys`.** No new crate features, but about
  300 lines of unsafe vtable code to maintain (the user chose the `windows` crate).
