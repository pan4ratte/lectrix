// Writes the release notes for a version from CHANGELOG.md (ADR 0011): what is under its
// "## <version>" heading, up to the next version. The Release workflow puts them in the
// GitHub release. It fails when the version has no heading or nothing under it, and when
// changes are still listed under "## Unreleased": they would ship without being listed.
// release-assets.mjs puts the latest versions' changes into latest.json too, so the update
// notice can show them before updating (`recentChanges`).
//
//   node .github/scripts/release-notes.mjs <version> <out file>
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { pathToFileURL } from 'node:url';

/**
 * @param {string} changelog the text of CHANGELOG.md
 * @param {string} version
 * @returns {string} the notes, in Markdown
 */
export function releaseNotes(changelog, version) {
	const lines = changelog.replace(/<!--[\s\S]*?-->/g, '').split(/\r?\n/);
	/** @param {string} title */
	const under = (title) => {
		const start = lines.findIndex((line) => line.trim() === `## ${title}`);
		if (start < 0) return null;
		const next = lines.findIndex((line, i) => i > start && line.startsWith('## '));
		return lines
			.slice(start + 1, next < 0 ? lines.length : next)
			.join('\n')
			.trim();
	};
	if (under('Unreleased')) {
		throw new Error(`CHANGELOG.md lists changes under "## Unreleased": move them under "## ${version}".`);
	}
	const notes = under(version);
	if (notes === null) {
		throw new Error(`CHANGELOG.md has no "## ${version}" heading. Add one with this version's changes (docs/releasing.md).`);
	}
	if (!notes) throw new Error(`CHANGELOG.md lists nothing under "## ${version}".`);
	return `${notes}\n`;
}

/** How many versions latest.json carries: enough for a copy several updates behind, while
 * the file every check downloads stays small. */
export const RECENT_VERSIONS = 10;

/**
 * The changes of the newest `count` released versions, each under its "## <version>"
 * heading, newest first, as the app's changelog parser reads them. Not "## Unreleased".
 * @param {string} changelog the text of CHANGELOG.md
 * @param {number} count
 * @returns {string}
 */
export function recentChanges(changelog, count = RECENT_VERSIONS) {
	const lines = changelog.replace(/<!--[\s\S]*?-->/g, '').split(/\r?\n/);
	/** @type {string[][]} */
	const versions = [];
	/** @type {string[] | null} */
	let current = null;
	for (const line of lines) {
		if (line.startsWith('## ')) {
			current = line.trim() === '## Unreleased' ? null : [line.trim()];
			if (current) versions.push(current);
		} else if (current) {
			current.push(line);
		}
	}
	const text = versions
		.slice(0, count)
		.map((v) => v.join('\n').trim())
		.join('\n\n');
	return `${text}\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
	const [version, out] = process.argv.slice(2);
	if (!version || !out) {
		console.error('Usage: node .github/scripts/release-notes.mjs <version> <out file>');
		process.exit(2);
	}
	try {
		const notes = releaseNotes(readFileSync('CHANGELOG.md', 'utf8'), version);
		mkdirSync(dirname(out), { recursive: true });
		writeFileSync(out, notes);
		console.log(notes);
	} catch (e) {
		console.error(`::error::${e instanceof Error ? e.message : String(e)}`);
		process.exit(1);
	}
}
