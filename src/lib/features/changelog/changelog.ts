// The changelog (CHANGELOG.md at the repository root; docs/releasing.md says how to write
// it), shown by Help > What's new (section 6.6). Each version lists its changes under the
// parts of the app they touch, in a fixed order. A translation (CHANGELOG_RU.md…) mirrors
// it; a version the translation lacks shows in English.

import changelogFiles from 'virtual:changelogs';

/** A run of text, bold or not: the only formatting the changelog uses. */
export interface Span {
	text: string;
	bold: boolean;
}

export interface ChangelogSection {
	title: string;
	items: Span[][];
}

export interface Release {
	/** Null for the changes not released yet ("## Unreleased", in its own words in a translation). */
	version: string | null;
	/** A sentence or two before the sections. */
	intro: Span[][];
	sections: ChangelogSection[];
}

/** The sections a version lists its changes under, in this order, as CHANGELOG.md names
 * them (a translation names them in its language, in the same order). A new tool gets a
 * section of its own, before Settings. */
export const SECTIONS = [
	'Start screen',
	'Viewer and tabs',
	'Annotations',
	'Bookmarks',
	'Page labels',
	'Combining files',
	'Organizing pages',
	'Settings',
	'Installation and updates',
	'General'
] as const;

export const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/** Splits `**bold**` from plain text. A `**` without a partner stays as it is. */
export function spans(text: string): Span[] {
	const out: Span[] = [];
	let at = 0;
	for (const match of text.matchAll(/\*\*(.+?)\*\*/g)) {
		if (match.index > at) out.push({ text: text.slice(at, match.index), bold: false });
		out.push({ text: match[1]!, bold: true });
		at = match.index + match[0].length;
	}
	if (at < text.length) out.push({ text: text.slice(at), bold: false });
	return out;
}

interface Draft {
	version: string | null;
	intro: string[];
	sections: { title: string; items: string[] }[];
}

/** Reads the changelog: `## <version>` (or `## Unreleased`), an optional paragraph, then
 * `### <section>` headings with `- ` items. Comments and the `#` title are left out;
 * a line that is neither a heading nor an item goes on with the paragraph or item above. */
export function parseChangelog(markdown: string): Release[] {
	const drafts: Draft[] = [];
	let release: Draft | null = null;
	let section: Draft['sections'][number] | null = null;
	// The paragraph or item a following line goes on with; a blank line ends it.
	let open: string[] | null = null;
	for (const raw of markdown.replace(/<!--[\s\S]*?-->/g, '').split(/\r?\n/)) {
		const line = raw.trim();
		if (raw.startsWith('## ')) {
			const title = line.slice(3).trim();
			release = { version: VERSION.test(title) ? title : null, intro: [], sections: [] };
			drafts.push(release);
			section = null;
			open = null;
		} else if (raw.startsWith('### ')) {
			if (!release) continue;
			section = { title: line.slice(4).trim(), items: [] };
			release.sections.push(section);
			open = null;
		} else if (/^[-*] /.test(raw)) {
			if (!section) continue;
			section.items.push(line.slice(2).trim());
			open = section.items;
		} else if (!line || raw.startsWith('#')) {
			open = null;
		} else if (open && open.length) {
			open[open.length - 1] += ` ${line}`;
		} else if (release && !section) {
			release.intro.push(line);
			open = release.intro;
		}
	}
	return drafts.map((d) => ({
		version: d.version,
		intro: d.intro.map(spans),
		sections: d.sections.map((s) => ({ title: s.title, items: s.items.map(spans) }))
	}));
}

/** The versions in `base` (the English changelog), each in its translation where
 * `translated` has that version. */
export function localize(base: Release[], translated: Release[]): Release[] {
	return base.map((release) => translated.find((t) => t.version === release.version) ?? release);
}

/** The file holding the translation for `locale`, among `names`: CHANGELOG_PT-BR.md for
 * pt-BR, else CHANGELOG_PT.md. */
export function translationFile(locale: string, names: readonly string[]): string | null {
	const tag = locale.toUpperCase();
	for (const candidate of [tag, tag.split('-')[0]]) {
		const name = `/CHANGELOG_${candidate}.md`;
		if (names.includes(name)) return name;
	}
	return null;
}

/** Semver precedence, enough for release versions: a pre-release comes before its release. */
export function compareVersions(a: string, b: string): number {
	const [coreA, preA] = a.split(/-(.*)/);
	const [coreB, preB] = b.split(/-(.*)/);
	const numsA = (coreA ?? '').split('.').map(Number);
	const numsB = (coreB ?? '').split('.').map(Number);
	for (let i = 0; i < 3; i++) if (numsA[i] !== numsB[i]) return (numsA[i] ?? 0) - (numsB[i] ?? 0);
	if (!preA || !preB) return (preA ? -1 : 0) - (preB ? -1 : 0);
	return preA.localeCompare(preB, 'en', { numeric: true });
}

/** The versions an update brings: those in its notes (latest.json, the latest versions'
 * changelog) newer than `current`, newest first. None for a release whose notes are not
 * a changelog (before 1.1.0). */
export function changesSince(notes: string | null, current: string): Release[] {
	return parseChangelog(notes ?? '').filter(
		(r) => r.version !== null && compareVersions(r.version, current) > 0 && hasChanges(r)
	);
}

/** True if `release` lists anything (an empty "Unreleased" doesn't). */
export function hasChanges(release: Release): boolean {
	return release.intro.length > 0 || release.sections.some((s) => s.items.length > 0);
}

// Bundled with the app, each read the first time it is shown (`changelogs` in vite.config.ts).
const files: Record<string, (() => Promise<string>) | undefined> = changelogFiles;

/** CHANGELOG.md as written (the Debug menu rehearses an update with it). */
export async function changelogMarkdown(): Promise<string> {
	return (await files['/CHANGELOG.md']?.()) ?? '';
}

/** The changelog, newest version first, in `locale` where it is translated. */
export async function loadChangelog(locale: string): Promise<Release[]> {
	const english = files['/CHANGELOG.md'];
	if (!english) return [];
	const base = parseChangelog(await english());
	const name = translationFile(locale, Object.keys(files));
	const translation = name ? files[name] : undefined;
	return translation ? localize(base, parseChangelog(await translation())) : base;
}
