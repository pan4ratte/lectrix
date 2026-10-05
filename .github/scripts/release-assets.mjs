// Release assets for .github/workflows/release.yml (ADR 0011).
//
//   node .github/scripts/release-assets.mjs collect <rust target> <version> <out dir>
//     Copies the installers `tauri build` made for <rust target> into <out dir>/installers,
//     their updater signatures into <out dir>/signatures, and writes
//     <out dir>/updater-<rust target>.json: the platforms this build adds to latest.json.
//
//   node .github/scripts/release-assets.mjs manifest <version> <fragments dir> <out file>
//     Merges the fragments into the updater manifest (latest.json).
//
// The release is v<version> of $GITHUB_REPOSITORY; asset URLs point there.
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';

const repository = process.env.GITHUB_REPOSITORY ?? 'pan4ratte/lectrix';

/** Updater platform names: `{os}-{arch}`, as tauri-plugin-updater looks them up. */
function platformOf(target) {
	const arch = target.split('-')[0];
	if (target.includes('windows')) return { os: 'windows', arch };
	if (target.includes('apple-darwin')) return { os: 'darwin', arch };
	if (target.includes('linux')) return { os: 'linux', arch };
	throw new Error(`unknown target ${target}`);
}

/** The bundle type the updater uses on each OS when it doesn't know how it was installed. */
const DEFAULT_BUNDLE = { windows: 'nsis', darwin: 'app', linux: 'appimage' };

/** What each bundler output is, and the name it gets in the release. */
function classify(dir, name, version, arch) {
	if (dir === 'nsis' && name.endsWith('-setup.exe')) return { bundle: 'nsis', asset: name };
	if (dir === 'msi' && name.endsWith('.msi')) return { bundle: 'msi', asset: name };
	if (dir === 'appimage' && name.endsWith('.AppImage')) return { bundle: 'appimage', asset: name };
	if (dir === 'deb' && name.endsWith('.deb')) return { bundle: 'deb', asset: name };
	if (dir === 'rpm' && name.endsWith('.rpm')) return { bundle: 'rpm', asset: name };
	if (dir === 'dmg' && name.endsWith('.dmg')) return { bundle: null, asset: name };
	// Both Mac builds make "Lectrix.app.tar.gz": name them apart.
	if (dir === 'macos' && name.endsWith('.app.tar.gz')) {
		return { bundle: 'app', asset: name.replace(/\.app\.tar\.gz$/, `_${version}_${arch}.app.tar.gz`) };
	}
	return null;
}

function collect(target, version, out) {
	const bundleDir = join('target', target, 'release', 'bundle');
	if (!existsSync(bundleDir)) throw new Error(`${bundleDir} not found: run tauri build --target ${target}`);
	const { os, arch } = platformOf(target);
	const installers = join(out, 'installers');
	const signatures = join(out, 'signatures');
	mkdirSync(installers, { recursive: true });
	mkdirSync(signatures, { recursive: true });
	const platforms = {};
	for (const dir of readdirSync(bundleDir)) {
		const path = join(bundleDir, dir);
		if (!statSync(path).isDirectory()) continue;
		for (const name of readdirSync(path)) {
			const file = join(path, name);
			if (!statSync(file).isFile()) continue;
			const kind = classify(dir, name, version, arch);
			if (!kind) continue;
			copyFileSync(file, join(installers, kind.asset));
			console.log(`${kind.asset} (${dir})`);
			if (!kind.bundle) continue;
			const sig = `${file}.sig`;
			// The default bundle must be updatable; others (deb, rpm) may come without one.
			if (!existsSync(sig)) {
				if (kind.bundle !== DEFAULT_BUNDLE[os]) continue;
				throw new Error(`${sig} missing: is TAURI_SIGNING_PRIVATE_KEY set?`);
			}
			copyFileSync(sig, join(signatures, `${kind.asset}.sig`));
			const entry = {
				signature: readFileSync(sig, 'utf8').trim(),
				url: `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(kind.asset)}`
			};
			platforms[`${os}-${arch}-${kind.bundle}`] = entry;
			if (kind.bundle === DEFAULT_BUNDLE[os]) platforms[`${os}-${arch}`] = entry;
		}
	}
	if (!platforms[`${os}-${arch}`]) throw new Error(`no ${DEFAULT_BUNDLE[os]} update bundle in ${bundleDir}`);
	writeFileSync(join(out, `updater-${target}.json`), JSON.stringify(platforms, null, 2));
}

function manifest(version, fragments, outFile) {
	const platforms = {};
	for (const name of readdirSync(fragments, { recursive: true })) {
		const file = join(fragments, String(name));
		if (!basename(file).startsWith('updater-') || !file.endsWith('.json')) continue;
		Object.assign(platforms, JSON.parse(readFileSync(file, 'utf8')));
	}
	if (Object.keys(platforms).length === 0) throw new Error(`no updater fragments in ${fragments}`);
	const latest = {
		version,
		notes: `Lectrix ${version}`,
		pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
		platforms
	};
	writeFileSync(outFile, JSON.stringify(latest, null, 2));
	console.log(`latest.json: ${Object.keys(platforms).sort().join(', ')}`);
}

const [command, ...args] = process.argv.slice(2);
if (command === 'collect' && args.length === 3) collect(args[0], args[1], args[2]);
else if (command === 'manifest' && args.length === 3) manifest(args[0], args[1], args[2]);
else {
	console.error('usage: release-assets.mjs collect <target> <version> <out> | manifest <version> <fragments> <out>');
	process.exit(2);
}
