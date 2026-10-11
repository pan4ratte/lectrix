import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, sep } from 'node:path';

import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import type { Plugin } from 'vite';
import { defineConfig } from 'vitest/config';

const host = process.env.TAURI_DEV_HOST;

/**
 * Records which npm packages end up in the app's client bundle, for the license list
 * (tests/licenses/notices.py writes THIRD_PARTY_LICENSES.md from it).
 */
function bundledPackages(): Plugin {
	return {
		name: 'lectrix-bundled-packages',
		apply: 'build',
		generateBundle() {
			if (this.environment.config.consumer !== 'client') return;
			const packages = new Map<string, { name: string; version: string; dir: string }>();
			for (const id of this.getModuleIds()) {
				const path = id.split('?')[0]!;
				const at = path.lastIndexOf(`node_modules${sep}`) >= 0 ? path.lastIndexOf(`node_modules${sep}`) : path.lastIndexOf('node_modules/');
				if (at < 0) continue;
				const rest = path.slice(at + 'node_modules/'.length).split(/[\\/]/);
				const name = rest[0]!.startsWith('@') ? `${rest[0]}/${rest[1]}` : rest[0]!;
				const dir = join(path.slice(0, at), 'node_modules', ...name.split('/'));
				if (packages.has(dir)) continue;
				const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')) as { version: string };
				packages.set(dir, { name, version: manifest.version, dir });
			}
			const out = join(import.meta.dirname, 'target', 'frontend-packages.json');
			mkdirSync(dirname(out), { recursive: true });
			writeFileSync(out, JSON.stringify([...packages.values()].sort((a, b) => a.name.localeCompare(b.name)), null, '\t'));
		}
	};
}

/**
 * The changelogs at the repository root (CHANGELOG.md and its translations), for What's
 * new: `virtual:changelogs` gives, for each file ("/CHANGELOG.md"), a function that imports
 * its text, so each is read the first time it is shown. The files can't be imported where
 * they are: SvelteKit's dev server serves nothing from the repository root, and Vite
 * refuses a `?raw` import of a file allowed by name (`server.fs.allow`), which it checks
 * with its query too.
 */
function changelogs(): Plugin {
	const index = '\0virtual:changelogs';
	const names = () => readdirSync(import.meta.dirname).filter((name) => /^CHANGELOG.*\.md$/.test(name));
	return {
		name: 'lectrix-changelogs',
		resolveId(id) {
			if (id === 'virtual:changelogs' || id.startsWith('virtual:changelogs/')) return `\0${id}`;
		},
		load(id) {
			if (id === index) {
				const entries = names().map(
					(name) => `${JSON.stringify(`/${name}`)}: () => import(${JSON.stringify(`virtual:changelogs/${name}`)}).then((m) => m.default)`
				);
				return `export default { ${entries.join(', ')} };`;
			}
			if (!id.startsWith(`${index}/`)) return;
			// Only the changelogs: the dev server takes the name from a request.
			const name = id.slice(index.length + 1);
			if (!names().includes(name)) return;
			const file = join(import.meta.dirname, name);
			this.addWatchFile(file);
			return `export default ${JSON.stringify(readFileSync(file, 'utf8'))};`;
		}
	};
}

export default defineConfig({
	plugins: [
		bundledPackages(),
		changelogs(),
		tailwindcss(),
		sveltekit({
			preprocess: vitePreprocess(),
			// Tauri serves a static SPA; SSR is disabled in src/routes/+layout.ts.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	clearScreen: false,
	build: {
		// Vite's 500 kB warning is about downloads; Lectrix loads its one bundle from disk,
		// where splitting it would only add requests at startup. Warn if it grows a lot.
		chunkSizeWarningLimit: 800,
		// Rolldown notes when JavaScript plugins take most of a build. Here they always do:
		// compiling Svelte is the build's work.
		rolldownOptions: { checks: { bundlerTimings: false } }
	},
	server: {
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
		watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**', '**/third_party/**'] }
	},
	test: {
		include: ['src/**/*.test.ts']
	}
});
