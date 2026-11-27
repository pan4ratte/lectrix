# 0037: Selected text looked up on websites fixed in Rust

- Status: accepted (at the user's request, 2026-11-27; revised twice after the user's
  testing, 2026-11-28)
- Date: 2026-11-27

## Context

The user asked for items in the right-click menu of selected text that translate it
(Google Translate, DeepL) or search for it (Google, DuckDuckGo, Wikipedia, Google
Scholar, Semantic Scholar, JSTOR), each switched on or off in Settings, with an indicator
that it leads to an external website.

AGENTS.md section 2 allows no network at runtime but the update check, and hands the
system's browser only a web link the user clicks in a document; section 6.1 has Rust open
only an address a link in the document carries (`open_link`), so the webview can have no
other page opened. A lookup's address carries text the document does not hold as a link,
so it fits neither rule as written.

## Decision

1. **A command of its own, `look_up`,** takes a website (`WebLookup`, an enum), the text
   and a language tag, and builds the address in Rust (`lookup.rs`): a fixed prefix per
   website and the text percent-encoded after it (for DeepL, with the escapes of point 6). The
   language goes into the address only as two or three letters (with a region for
   Chinese), else English. So the webview can still have no page opened
   but a search or translation on one of eight websites, whatever it sends. Lectrix
   fetches nothing: the address goes to the default browser as a link's does, through
   `open_in_browser` (so `LECTRIX_BROWSER_LOG` catches it in tests).
2. **It needs the copy permission**, since it takes the text out of the document, as
   copying does; the log names the website, never the text.
3. **Addresses stay within 8,192 characters**, SumatraPDF's limit for the same feature:
   the shell hands the address to the browser on a command line of up to 32,767
   characters, and browsers take far more than that limit. A longer text loses its end,
   at a space where it can, and a notice says so, with Don't show again, which turns off
   a switch in Settings (`lookup_shortened_notice`).
4. **Translations go into the language Settings gives** (`lookup_language`), the
   system's (the webview's first language) until another is picked, and Wikipedia is the
   one in that language: a reader in another language than the document's most likely
   wants their own. One setting for every website; the list has the 34 languages Google
   Translate and DeepL both translate into.
5. **Settings stores the websites switched on** (`web_lookups`), as the quick tools are:
   DeepL, Google and Google Scholar to begin with, at the user's choice, so the menu
   stays short.
6. **DeepL's address names a source language:** `https://www.deepl.com/translator#en/<lang>/`
   (`#de/en/` when translating into English), Traditional Chinese as `zh-hant`, and a
   slash, a bar or a backslash in the text with a backslash before it, then the text
   percent-encoded. Tried in Edge on 2026-11-28 by reading DeepL's page: with `auto` or
   `-` as the source (SumatraPDF writes `auto`), or none, the page stays empty; with any
   language named, it detects the text's own (French under `#en/de/` or `#ru/de/` comes
   out German), except when the source is the target, where it takes neither (`#en/en/`
   translated into German). A slash or a bar ends the text there, encoded or not, and a
   lone backslash is dropped; a backslash before each keeps them all.
7. **Indicator:** each item ends with Lucide's external link icon, as the bookmark
   inspector's Open in browser button has, and its accessible name adds "opens in your
   browser".

## Alternatives considered

- **Building the address in the frontend and opening it with a general "open URL"
  command:** the webview could then open any page; the reason `open_link` checks the
  document is lost.
- **A submenu (Look up >):** fewer items at once, but a click more each time, and the
  user asked for the items in the menu.
- **Leaving the language out of the address**, so each website uses the one last picked
  on it: whether Google Translate does without `tl` is not documented, and DeepL's
  fragment needs one.
- **Storing the websites switched off**, so a website a later version adds is offered
  until switched off (the first version): with only three on by default, a new website
  would better start off too.
- **Sending a long text in a form (POST) through the browser**, as SumatraPDF can for
  its user-defined lookups: no length limit, but a page of Lectrix's own would have to
  post the form, which the webview may not open.
