// Writes a Tauri config overlay with the MSI version for the version in package.json
// (ADR 0011). MSI versions are numbers only, so a pre-release becomes a fourth field:
// 1.0.0-beta.2 -> 1.0.0.2, 1.0.0 -> 1.0.0. Windows Installer compares only the first three
// fields, and Tauri's MSI allows same-version upgrades, so 1.0.0 still replaces its betas.
//
//   node .github/scripts/msi-version.mjs <out file>
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

export function msiVersion(version) {
	const match = /^(\d+)\.(\d+)\.(\d+)(?:-(?:[0-9A-Za-z-]+\.)*?(\d+))?$/.exec(version);
	if (!match || (version.includes('-') && match[4] === undefined)) {
		throw new Error(`${version}: use x.y.z, or a pre-release ending in a number (1.0.0-beta.1)`);
	}
	const [, major, minor, patch, build] = match.map((p) => (p === undefined ? undefined : Number(p)));
	if (major > 255 || minor > 255 || patch > 65535 || build > 65535) {
		throw new Error(`${version}: too large for an MSI version (255.255.65535.65535)`);
	}
	return build === undefined ? `${major}.${minor}.${patch}` : `${major}.${minor}.${patch}.${build}`;
}

const out = process.argv[2];
if (out) {
	const { version } = JSON.parse(readFileSync('package.json', 'utf8'));
	const wix = msiVersion(version);
	mkdirSync(dirname(out), { recursive: true });
	writeFileSync(out, JSON.stringify({ bundle: { windows: { wix: { version: wix } } } }, null, 2));
	console.log(`MSI version ${wix} for ${version}`);
}
