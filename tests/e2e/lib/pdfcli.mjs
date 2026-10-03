// pdf-cli helpers: generate input files and inspect what the app saved.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';

import { OUT, ROOT } from './app.mjs';

function cliPath() {
	if (process.env.PDF_CLI) return process.env.PDF_CLI;
	for (const profile of ['release', 'debug']) {
		const path = join(ROOT, 'target', profile, 'pdf-cli.exe');
		if (existsSync(path)) return path;
	}
	throw new Error('pdf-cli not found: build it with cargo build -p pdf-cli');
}

export function cli(...args) {
	return execFileSync(cliPath(), args, { encoding: 'utf8' });
}

/** A fresh generated sample in target/test-output/e2e/<name>. */
export function sample(name, pages = 5) {
	mkdirSync(OUT, { recursive: true });
	const path = join(OUT, name);
	rmSync(path, { force: true });
	cli('gen', path, '--pages', String(pages));
	return path;
}

/** The outline as `pdf-cli outline show` prints it, one line per bookmark, without ids. */
export function outlineLines(path) {
	return cli('outline', 'show', path)
		.split('\n')
		.filter((l) => l.includes('->'))
		.map((l) => l.replace(/\[\d+\] /, '').trimEnd());
}
