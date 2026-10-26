# Feature specifications

What each feature of Lectrix does, in detail. [AGENTS.md](../../AGENTS.md) holds the rest of the specification: scope, constraints, and the rules every change follows (section 10), with the other sections in short. Their detail is in [architecture.md](../architecture.md) (sections 3 and 4), [interop-profile.md](../interop-profile.md) (section 5, the annotation interoperability contract), [document-model.md](../document-model.md) (section 7), [ui.md](ui.md) (section 8) and [testing.md](../testing.md) (section 9). These files keep the section numbers they had there (ADR 0022), so a reference to "section 6.5" anywhere leads to the file below.

| Section | File | Covers |
| --- | --- | --- |
| 6.1 | [viewer.md](viewer.md) | Opening, tabs, scrolling, zoom, navigation, text selection, search, links, scroll modes, view rotation, full screen, visual adjustments, the Pages panel, the start screen, recent files |
| 6.2 | [bookmarks.md](bookmarks.md) | The outline: adding, editing, following, the inspector |
| 6.3 | [page-labels.md](page-labels.md) | The Page labels dialog and how labels are written |
| 6.4 | [combine.md](combine.md) | Combining files, and inserting pages from a file |
| 6.5 | [annotations.md](annotations.md) | The annotation tools, colours, the bar, the inspector, the annotation list |
| 6.6 | [settings.md](settings.md) | Settings, About, updates, What's new, releases, installers |
| 6.7 | [organize.md](organize.md) | Organizing a document's pages in a grid |
| 6.8 | [unlock.md](unlock.md) | Removing a file's password and restrictions, in the file or a copy |
| 6.9 | [protect.md](protect.md) | Giving a file a password, restrictions or both, as a tool or a document's setting |
| 6.10 | [properties.md](properties.md) | A document's properties: its file, description, security and fonts |
| 6.11 | [compress.md](compress.md) | Writing a PDF smaller: image settings, presets, removals, the result and its preview |
| 6.12 | [print.md](print.md) | Printing: the Print dialog, its preview, and how pages reach the printer |
| 8.1–8.4 | [ui.md](ui.md) | The window layout, the app's own controls, motion, shortcuts on other keyboards and on macOS |

Each file states the behavior first, then its **settled details**: choices made during the build with the user's agreement. Change them only with that agreement, and write a change in behavior into its file in the same commit as the code.
