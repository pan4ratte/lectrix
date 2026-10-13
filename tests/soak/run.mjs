// The soak run: the tests many times over, to find the ones that fail only some of the
// time. A normal run of the tests shows such a test only on the day it happens to fail; this
// runs every pass to its end whatever fails, and then lists each test that failed in any
// of them, with how often and why. Fix those, run it again, and CI stops tripping on them.
//
//   node tests/soak/run.mjs [--passes 10] [--suite all|e2e|rust] [--like-ci] [e2e test files]
//
// --suite e2e   the end-to-end tests (tests/e2e; needs what they need: the release app,
//               pdf-cli, tauri-driver, msedgedriver). Test files, as paths from tests/e2e,
//               limit a pass to them. --like-ci shows the app as CI's runners do.
// --suite rust  cargo test --workspace --no-fail-fast
//
// Writes each pass's output and a summary to target/test-output/soak, the summary to the
// job's page when run by GitHub Actions (.github/workflows/soak.yml), and exits with 1 if
// any test failed in any pass.

import { spawnSync } from 'node:child_process';
import { appendFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const OUT = join(ROOT, 'target', 'test-output', 'soak');
const E2E = join(ROOT, 'tests', 'e2e');
const E2E_OUT = join(ROOT, 'target', 'test-output', 'e2e');
// (As a URL: on Windows, node takes a reporter's path with a drive letter for a scheme.)
const REPORTER = pathToFileURL(join(ROOT, 'tests', 'soak', 'reporter.mjs')).href;

function options(argv) {
	const opts = { passes: 10, suite: 'all', likeCi: false, files: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === '--passes') opts.passes = Number(argv[++i]);
		else if (arg === '--suite') opts.suite = argv[++i];
		else if (arg === '--like-ci') opts.likeCi = true;
		else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
		else opts.files.push(arg);
	}
	if (!Number.isInteger(opts.passes) || opts.passes < 1) throw new Error('--passes takes a whole number, 1 or more');
	if (!['all', 'e2e', 'rust'].includes(opts.suite)) throw new Error('--suite takes all, e2e or rust');
	return opts;
}

/** Runs a command to its end, writes everything it printed to `log`, and returns it with
 * the exit code (null if it could not be started). */
function run(command, args, { cwd, env, log }) {
	const done = spawnSync(command, args, { cwd, env, encoding: 'utf8', maxBuffer: 1024 ** 3 });
	const output = `${done.stdout ?? ''}${done.stderr ?? ''}${done.error ? `\n${done.error}\n` : ''}`;
	writeFileSync(log, output);
	return { output, code: done.error ? null : done.status };
}

/** One pass of the end-to-end tests: every test with whether it passed. */
function e2ePass(pass, opts) {
	const results = join(OUT, `e2e-pass-${pass}.jsonl`);
	rmSync(results, { force: true });
	// The driver's log of a pass is some 200 MB: each pass starts it afresh, as `npm test` does.
	rmSync(join(E2E_OUT, 'driver-session.log'), { force: true });
	const { code } = run(
		process.execPath,
		[
			'--test',
			'--test-concurrency=1',
			'--test-reporter=spec',
			'--test-reporter-destination=stdout',
			`--test-reporter=${REPORTER}`,
			`--test-reporter-destination=${results}`,
			...(opts.files.length > 0 ? opts.files : ['specs/*.test.mjs'])
		],
		{ cwd: E2E, env: { ...process.env, ...(opts.likeCi ? { LECTRIX_E2E_LIKE_CI: '1' } : {}) }, log: join(OUT, `e2e-pass-${pass}.log`) }
	);
	const tests = existsSync(results)
		? readFileSync(results, 'utf8')
				.split('\n')
				.filter((line) => line.trim() !== '')
				.map((line) => JSON.parse(line))
				.map((t) => ({ id: `${t.file ? basename(t.file) : 'tests/e2e'}: ${t.name}`, ok: t.ok, error: t.error }))
		: [];
	// A pass that reported nothing, or failed without a failed test, is a failure of its own.
	if (tests.length === 0 || (code !== 0 && tests.every((t) => t.ok))) {
		tests.push({ id: 'tests/e2e: the run itself', ok: false, error: `node --test exited with ${code}; see ${basename(results, '.jsonl')}.log` });
	}
	// What a failed pass left on screen, before the next pass draws over it.
	if (tests.some((t) => !t.ok) && existsSync(E2E_OUT)) {
		const kept = join(OUT, `e2e-pass-${pass}`);
		mkdirSync(kept, { recursive: true });
		for (const name of readdirSync(E2E_OUT)) {
			if (name.endsWith('.png')) cpSync(join(E2E_OUT, name), join(kept, name));
		}
	}
	return tests;
}

/** One pass of the Rust tests: every test with whether it passed, named with its target
 * (`pdf-core tests\session.rs: test_name`). */
function rustPass(pass) {
	const log = join(OUT, `rust-pass-${pass}.log`);
	const { output, code } = run('cargo', ['test', '--workspace', '--no-fail-fast', '--color', 'never'], { cwd: ROOT, env: process.env, log });
	const tests = [];
	const panics = new Map();
	let target = 'cargo test';
	for (const line of output.split(/\r?\n/)) {
		const running = /^\s*(?:Running|Doc-tests)\s+(.+?)(?:\s+\(.*[\\/]([\w-]+?)-[0-9a-f]{16}(?:\.exe)?\))?\s*$/.exec(line);
		if (running) {
			target = running[2] && !running[1].includes(running[2]) ? `${running[2]} ${running[1]}` : running[1];
			continue;
		}
		const test = /^test (.+?) \.\.\. (ok|FAILED|ignored.*)$/.exec(line);
		if (test && !test[2].startsWith('ignored')) tests.push({ id: `${target}: ${test[1]}`, name: test[1], ok: test[2] === 'ok' });
		// The line after "thread '<test>' panicked at <place>:" says why.
		const panic = /^thread '(.+?)' (?:\(\d+\) )?panicked at (.+?):$/.exec(line);
		if (panic) panics.set(panic[1], { at: panic[2], why: null });
		else {
			const open = [...panics.values()].find((p) => p.why === null);
			if (open) open.why = line.trim();
		}
	}
	for (const t of tests) {
		const panic = panics.get(t.name);
		if (!t.ok) t.error = panic ? `${panic.why} (${panic.at})` : `see ${basename(log)}`;
	}
	if (tests.length === 0 || (code !== 0 && tests.every((t) => t.ok))) {
		tests.push({ id: 'cargo test: the run itself', ok: false, error: `cargo test exited with ${code}; see ${basename(log)}` });
	}
	return tests;
}

/** Runs `passes` passes of a suite. Returns the count of tests in a pass and, for each test
 * that failed in any pass, in how many it failed and the reasons given. */
function soak(name, passes, pass) {
	const failed = new Map();
	let count = 0;
	for (let n = 1; n <= passes; n++) {
		const started = Date.now();
		const tests = pass(n);
		count = Math.max(count, tests.length);
		const bad = tests.filter((t) => !t.ok);
		for (const t of bad) {
			const entry = failed.get(t.id) ?? { passes: [], errors: [] };
			entry.passes.push(n);
			if (t.error && !entry.errors.includes(t.error)) entry.errors.push(t.error);
			failed.set(t.id, entry);
		}
		const took = Math.round((Date.now() - started) / 1000);
		console.log(`${name} pass ${n} of ${passes}: ${tests.length - bad.length} passed, ${bad.length} failed (${took} s)`);
		for (const t of bad) console.log(`  failed: ${t.id}${t.error ? `\n          ${t.error}` : ''}`);
	}
	return { name, passes, count, failed };
}

const cell = (text) => text.replaceAll('|', '\\|').replaceAll('\n', ' ');

/** The summary, as Markdown: a table of the tests that failed, most often first. */
function summary(runs) {
	const lines = ['# Soak run', ''];
	for (const { name, passes, count, failed } of runs) {
		lines.push(`## ${name}: ${passes} ${passes === 1 ? 'pass' : 'passes'} of ${count} tests`, '');
		if (failed.size === 0) {
			lines.push('Every test passed in every pass.', '');
			continue;
		}
		lines.push('| Test | Failed in | Why |', '| --- | --- | --- |');
		for (const [id, f] of [...failed].sort((a, b) => b[1].passes.length - a[1].passes.length || a[0].localeCompare(b[0]))) {
			lines.push(`| ${cell(id)} | ${f.passes.length} of ${passes} (pass ${f.passes.join(', ')}) | ${cell(f.errors.join(' / '))} |`);
		}
		lines.push('');
	}
	return lines.join('\n');
}

const opts = options(process.argv.slice(2));
mkdirSync(OUT, { recursive: true });
const runs = [];
if (opts.suite !== 'rust') runs.push(soak('End-to-end tests', opts.passes, (n) => e2ePass(n, opts)));
if (opts.suite !== 'e2e') runs.push(soak('Rust tests', opts.passes, (n) => rustPass(n)));

const text = summary(runs);
writeFileSync(join(OUT, 'summary.md'), text);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${text}\n`);
console.log(`\n${text}`);
console.log(`Output of every pass: ${relative(ROOT, OUT)}`);
process.exit(runs.some((r) => r.failed.size > 0) ? 1 : 0);
