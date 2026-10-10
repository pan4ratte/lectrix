import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { recentChanges, releaseNotes } from '../../../../.github/scripts/release-notes.mjs';

import {
	changesSince,
	compareVersions,
	hasChanges,
	loadChangelog,
	localize,
	parseChangelog,
	SECTIONS,
	spans,
	translationFile,
	VERSION,
	type Release
} from './changelog.ts';

const SAMPLE = `# Changelog

<!-- How to write it: docs/releasing.md. -->

## Unreleased

### General

- Something new.

## 1.1.0

A short intro
over two lines.

### Viewer and tabs

- **View > Page display** turns pages
  like a book.
- Faster.

### Settings

- A new switch.

## 1.0.0

### General

- First release.
`;

describe('spans', () => {
	it('splits bold from plain text', () => {
		expect(spans('Use **Help > What’s new** to see it.')).toEqual([
			{ text: 'Use ', bold: false },
			{ text: 'Help > What’s new', bold: true },
			{ text: ' to see it.', bold: false }
		]);
	});

	it('keeps an unpaired marker as text', () => {
		expect(spans('a ** b')).toEqual([{ text: 'a ** b', bold: false }]);
	});
});

describe('parseChangelog', () => {
	const releases = parseChangelog(SAMPLE);

	it('reads versions newest first, with Unreleased as no version', () => {
		expect(releases.map((r) => r.version)).toEqual([null, '1.1.0', '1.0.0']);
	});

	it('reads the intro, the sections and their items, joining wrapped lines', () => {
		const release = releases[1]!;
		expect(release.intro).toEqual([[{ text: 'A short intro over two lines.', bold: false }]]);
		expect(release.sections.map((s) => s.title)).toEqual(['Viewer and tabs', 'Settings']);
		expect(release.sections[0]!.items).toEqual([
			[
				{ text: 'View > Page display', bold: true },
				{ text: ' turns pages like a book.', bold: false }
			],
			[{ text: 'Faster.', bold: false }]
		]);
	});

	it('leaves out comments and the title', () => {
		expect(JSON.stringify(releases)).not.toContain('docs/releasing.md');
		expect(JSON.stringify(releases)).not.toContain('Changelog');
	});

	it('reads Windows line endings', () => {
		expect(parseChangelog(SAMPLE.replace(/\n/g, '\r\n'))).toEqual(releases);
	});

	it('tells an empty Unreleased from one with changes', () => {
		expect(hasChanges(releases[0]!)).toBe(true);
		expect(hasChanges(parseChangelog('## Unreleased\n\n## 1.0.0\n')[0]!)).toBe(false);
	});
});

describe('translations', () => {
	it('picks the file for the language, then for its base language', () => {
		const names = ['/CHANGELOG.md', '/CHANGELOG_RU.md', '/CHANGELOG_PT-BR.md', '/CHANGELOG_DE.md'];
		expect(translationFile('ru', names)).toBe('/CHANGELOG_RU.md');
		expect(translationFile('pt-BR', names)).toBe('/CHANGELOG_PT-BR.md');
		expect(translationFile('de-AT', names)).toBe('/CHANGELOG_DE.md');
		expect(translationFile('fr', names)).toBeNull();
		expect(translationFile('en', names)).toBeNull();
	});

	it('shows a version the translation lacks in English', () => {
		const russian = parseChangelog('## 1.0.0\n\n### Общее\n\n- Первый выпуск.\n');
		const shown = localize(parseChangelog(SAMPLE), russian);
		expect(shown.map((r) => r.version)).toEqual([null, '1.1.0', '1.0.0']);
		expect(shown[1]!.sections[0]!.title).toBe('Viewer and tabs');
		expect(shown[2]!.sections[0]!.title).toBe('Общее');
	});
});

describe('releaseNotes', () => {
	it('gives what is under the version, without comments', () => {
		const released = SAMPLE.replace(/## Unreleased[\s\S]*?(?=## 1\.1\.0)/, '');
		const notes = releaseNotes(released, '1.1.0');
		expect(notes.startsWith('A short intro')).toBe(true);
		expect(notes).toContain('### Settings\n\n- A new switch.\n');
		expect(notes).not.toContain('First release');
	});

	it('accepts an empty Unreleased', () => {
		expect(releaseNotes(SAMPLE.replace('### General\n\n- Something new.\n', ''), '1.1.0')).toContain('Faster.');
	});

	it('refuses changes left under Unreleased, and a version without notes', () => {
		expect(() => releaseNotes(SAMPLE, '1.1.0')).toThrow(/Unreleased/);
		expect(() => releaseNotes('## 1.2.0\n\n## 1.1.0\n\n- x\n', '1.2.0')).toThrow(/nothing/);
		expect(() => releaseNotes('## 1.1.0\n\n- x\n', '1.2.0')).toThrow(/no "## 1.2.0"/);
	});
});

describe('before updating', () => {
	it('orders versions, a pre-release before its release', () => {
		expect(compareVersions('1.2.0', '1.10.0')).toBeLessThan(0);
		expect(compareVersions('1.2.0-beta.2', '1.2.0-beta.10')).toBeLessThan(0);
		expect(compareVersions('1.2.0-beta.2', '1.2.0')).toBeLessThan(0);
		expect(compareVersions('1.2.0', '1.2.0')).toBe(0);
	});

	it('latest.json carries the newest released versions, without Unreleased', () => {
		const notes = recentChanges(SAMPLE, 1);
		expect(notes.startsWith('## 1.1.0')).toBe(true);
		expect(notes).not.toContain('Something new');
		expect(notes).not.toContain('1.0.0');
		expect(recentChanges(SAMPLE)).toContain('## 1.0.0');
	});

	it('shows the versions newer than the one installed', () => {
		const notes = recentChanges(SAMPLE);
		expect(changesSince(notes, '0.9.0').map((r) => r.version)).toEqual(['1.1.0', '1.0.0']);
		expect(changesSince(notes, '1.0.0').map((r) => r.version)).toEqual(['1.1.0']);
		expect(changesSince(notes, '1.1.0')).toEqual([]);
		// What 1.0.0 put in latest.json.
		expect(changesSince('Lectrix 1.0.0', '0.9.0')).toEqual([]);
		expect(changesSince(null, '0.9.0')).toEqual([]);
	});
});

// ----- the real files -----

const root = process.cwd();
const read = (name: string) => readFileSync(join(root, name), 'utf8');

/** What the app can't show, or the writing guide rules out (docs/releasing.md). */
function formattingProblems(markdown: string): string[] {
	const problems: string[] = [];
	const lines = markdown.replace(/<!--[\s\S]*?-->/g, '').split(/\r?\n/);
	lines.forEach((line, i) => {
		const at = `line ${i + 1}: ${line}`;
		if (line.startsWith('#') && !/^#{1,3} \S/.test(line)) problems.push(`heading deeper than ###, ${at}`);
		if (line.startsWith('# ') && i > 0) problems.push(`a second title, ${at}`);
		if (/^\s+\S/.test(line)) problems.push(`indented (nested lists and wrapped items aren't shown), ${at}`);
		if (line.startsWith('* ')) problems.push(`list item with *, use -, ${at}`);
		if (/\]\(|!\[|`|<|(^|[^*])\*([^*]|$)|(^|\W)_\w/.test(line.replace(/^\* /, '')))
			problems.push(`formatting other than **bold**, ${at}`);
	});
	return problems;
}

describe('CHANGELOG.md', () => {
	const text = read('CHANGELOG.md');
	const releases = parseChangelog(text);
	const headings = text.split(/\r?\n/).filter((l) => l.startsWith('## '));

	it('has version headings, and Unreleased only at the top', () => {
		headings.forEach((heading, i) => {
			const title = heading.slice(3);
			if (title === 'Unreleased') expect(i, 'Unreleased is the first heading').toBe(0);
			else expect(title, `"${heading}" is a version`).toMatch(VERSION);
		});
	});

	it('lists versions newest first', () => {
		const versions = releases.flatMap((r) => (r.version ? [r.version] : []));
		const sorted = [...versions].sort((a, b) => compareVersions(b, a));
		expect(versions).toEqual(sorted);
		expect(new Set(versions).size).toBe(versions.length);
	});

	it('has the version in package.json', () => {
		const { version } = JSON.parse(read('package.json')) as { version: string };
		expect(
			releases.some((r) => r.version === version),
			`CHANGELOG.md has no "## ${version}": rename "## Unreleased" (docs/releasing.md)`
		).toBe(true);
	});

	it('uses the sections, in their order, each with changes', () => {
		for (const release of releases) {
			const titles = release.sections.map((s) => s.title);
			for (const title of titles) expect(SECTIONS, `section "${title}"`).toContain(title);
			const order = titles.map((t) => SECTIONS.indexOf(t as (typeof SECTIONS)[number]));
			expect(order, `sections of ${release.version ?? 'Unreleased'}`).toEqual([...order].sort((a, b) => a - b));
			expect(new Set(titles).size).toBe(titles.length);
			for (const section of release.sections) expect(section.items.length, section.title).toBeGreaterThan(0);
			if (release.version) expect(hasChanges(release), release.version).toBe(true);
		}
	});

	it('uses only what the app shows', () => {
		expect(formattingProblems(text)).toEqual([]);
	});

	it('is bundled with the app', async () => {
		expect(await loadChangelog('en')).toEqual(releases);
	});
});

describe('translated changelogs', () => {
	const english = parseChangelog(read('CHANGELOG.md'));
	const files = readdirSync(root).filter((name) => /^CHANGELOG_[A-Z]{2,3}(-[A-Z0-9]+)?\.md$/.test(name));

	/** The shape a translation must keep: its sections' and items' counts. */
	const shape = (release: Release) => release.sections.map((s) => s.items.length);

	it('are named CHANGELOG_<LANGUAGE>.md', () => {
		const odd = readdirSync(root).filter((name) => /^changelog/i.test(name) && name !== 'CHANGELOG.md');
		expect(odd.filter((name) => !files.includes(name))).toEqual([]);
	});

	for (const file of files) {
		it(`${file} mirrors CHANGELOG.md`, () => {
			const text = read(file);
			expect(formattingProblems(text)).toEqual([]);
			for (const release of parseChangelog(text)) {
				const source = english.find((r) => r.version === release.version);
				expect(source, `${file} has ${release.version ?? 'Unreleased'}, CHANGELOG.md doesn't`).toBeDefined();
				expect(shape(release), `${file}, ${release.version ?? 'Unreleased'}`).toEqual(shape(source!));
			}
		});
	}
});
