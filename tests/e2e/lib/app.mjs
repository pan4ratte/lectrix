// Launches Lectrix under tauri-driver and returns a WebdriverIO browser for its webview.
//
// Needs: the release app (`npx tauri build --no-bundle`), `tauri-driver` (cargo install
// tauri-driver --locked) and an msedgedriver matching the installed WebView2 runtime
// (tests/e2e/fetch-edgedriver.ps1). LECTRIX_APP, TAURI_DRIVER and MSEDGEDRIVER override
// the default locations.
//
// A Lectrix the user has open (the installed one, a development copy) is left alone: the
// tests end only copies of the app under test, and run beside it as an instance of their
// own (LECTRIX_EPHEMERAL).
//
// LECTRIX_E2E_LIKE_CI=1 (`npm run test:ci`) shows the app as CI's Windows runners do, so
// that what only fails there fails here first: reduced motion, and a window of their size.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createConnection, createServer } from 'node:net';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { remote } from 'webdriverio';

import { DRIVER_LOG, trimDriverLog } from './driverlog.mjs';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
export const OUT = join(ROOT, 'target', 'test-output', 'e2e');
const PORT = 4444;

const app = process.env.LECTRIX_APP ?? join(ROOT, 'target', 'release', 'lectrix.exe');
const tauriDriver = process.env.TAURI_DRIVER ?? 'tauri-driver';
const edgeDriver = process.env.MSEDGEDRIVER ?? join(ROOT, 'target', 'e2e-tools', 'msedgedriver.exe');

/** As CI's Windows runners show the app: their visual effects are off, which the webview
 * reports as reduced motion, and their screen is 1024 x 768, where the window's page is
 * this large. */
const LIKE_CI = process.env.LECTRIX_E2E_LIKE_CI === '1';
const CI_PAGE = { width: 1028, height: 779 };

function waitForPort(port, timeoutMs) {
	const deadline = Date.now() + timeoutMs;
	return new Promise((done, fail) => {
		const attempt = () => {
			const socket = createConnection({ host: '127.0.0.1', port }, () => {
				socket.end();
				done();
			});
			socket.on('error', () => {
				socket.destroy();
				if (Date.now() > deadline) fail(new Error(`nothing listening on port ${port}`));
				else setTimeout(attempt, 100);
			});
		};
		attempt();
	});
}

/**
 * A port nothing is listening on, for msedgedriver. A fixed port is not enough: in CI
 * the previous session's processes sometimes still hold it for a moment after they
 * were ended, and the next msedgedriver then cannot bind it.
 */
function freePort() {
	return new Promise((done, fail) => {
		const server = createServer();
		server.once('error', fail);
		server.listen(0, '127.0.0.1', () => {
			const { port } = server.address();
			server.close(() => done(port));
		});
	});
}

/**
 * tauri-driver cannot pass arguments to msedgedriver, but it runs any executable: a small
 * wrapper adds a verbose log of every session (target/test-output/e2e/driver-session.log,
 * uploaded by CI when the tests fail; `npm test` starts it afresh, `driverlog.mjs`).
 */
function verboseDriver() {
	const wrapper = join(dirname(edgeDriver), 'msedgedriver-verbose.cmd');
	// The driver only ever appends: a log that has grown large is dropped first.
	trimDriverLog();
	writeFileSync(wrapper, `@"${edgeDriver}" %* --verbose --append-log "--log-path=${DRIVER_LOG}"
`);
	return wrapper;
}

const IMAGE = basename(app);

/** The process ids of every running copy of Lectrix, whichever file it runs. */
function lectrixPids() {
	const list = spawnSync('tasklist', ['/FI', `IMAGENAME eq ${IMAGE}`, '/FO', 'CSV', '/NH'], { encoding: 'utf8' });
	return [...(list.stdout ?? '').matchAll(/^"[^"]*","(\d+)"/gm)].map((m) => Number(m[1]));
}

/** The file each running copy of Lectrix runs, by process id. (PowerShell, which takes a
 * moment: asked only when a copy is running before a test starts.) */
function lectrixPaths() {
	const script = `Get-CimInstance Win32_Process -Filter "Name='${IMAGE}'" | ForEach-Object { "$($_.ProcessId)|$($_.ExecutablePath)" }`;
	const out = spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8' });
	const paths = new Map();
	for (const line of (out.stdout ?? '').split(/\r?\n/)) {
		const [pid, path] = line.split('|');
		if (pid && path) paths.set(Number(pid), path.trim());
	}
	return paths;
}

const sameFile = (a, b) => resolve(a).toLowerCase() === resolve(b).toLowerCase();

function end(pids) {
	for (const pid of pids) spawnSync('taskkill', ['/PID', String(pid), '/F', '/T'], { stdio: 'ignore' });
}

/** Copies of Lectrix that are not the tests': the user's own. Never ended. */
let foreign = new Set();
/** The copy this launch started. */
let started = [];

/**
 * Before a launch: ends what an earlier run left of the app under test, and notes every
 * other copy of Lectrix as the user's. A copy whose file Windows won't tell is the user's.
 */
function clearStrays() {
	foreign = new Set();
	started = [];
	const pids = lectrixPids();
	if (pids.length === 0) return;
	const paths = lectrixPaths();
	const strays = pids.filter((pid) => paths.has(pid) && sameFile(paths.get(pid), app));
	end(strays);
	foreign = new Set(pids.filter((pid) => !strays.includes(pid)));
}

/** The running copies that are the tests' own. */
const ownPids = () => lectrixPids().filter((pid) => !foreign.has(pid));

/** Ends the copy this launch started (before that is known: every copy that is not the
 * user's). */
function killApp() {
	end(started.length > 0 ? started : ownPids());
}

function appRunning() {
	const running = lectrixPids();
	return (started.length > 0 ? started : ownPids()).some((pid) => running.includes(pid));
}

/** Like CI: what the app is started with. msedgedriver hands a session's arguments to
 * Lectrix, not to its webview, so the browser's switch goes through LECTRIX_BROWSER_ARGS;
 * LECTRIX_WINDOW_SIZE sizes a window that has no remembered place (a test that brings a
 * state file with one keeps it, as on CI). */
const LIKE_CI_ENV = {
	LECTRIX_BROWSER_ARGS: '--force-prefers-reduced-motion',
	LECTRIX_WINDOW_SIZE: `${CI_PAGE.width}x${CI_PAGE.height}`
};

let saidLikeCi = false;

/** Like CI: checks that the app is shown as asked, and says so once. A screen too small
 * for the window is said, not failed: the tests still run. */
async function checkLikeCi(browser) {
	const seen = await browser.execute(() => ({
		reduced: matchMedia('(prefers-reduced-motion: reduce)').matches,
		width: innerWidth,
		height: innerHeight
	}));
	if (!seen.reduced) throw new Error('LECTRIX_E2E_LIKE_CI: the webview does not report reduced motion');
	if (saidLikeCi) return;
	saidLikeCi = true;
	const sized = Math.abs(seen.width - CI_PAGE.width) <= 2 && Math.abs(seen.height - CI_PAGE.height) <= 2;
	console.error(
		`like CI: reduced motion, page ${seen.width} x ${seen.height}` +
			(sized ? '' : ` (CI's is ${CI_PAGE.width} x ${CI_PAGE.height}: the window did not get that size here)`)
	);
}

/**
 * Ends tauri-driver with everything it started. Killing only tauri-driver leaves the
 * msedgedriver behind its wrapper running.
 */
async function killDriver(driver, exited) {
	spawnSync('taskkill', ['/PID', String(driver.pid), '/F', '/T'], { stdio: 'ignore' });
	await exited;
}

/**
 * Starts Lectrix with `files` (PDF paths) open and returns `{ browser, stop }`. Recent
 * files and remembered views stay out of the user's app data (LECTRIX_EPHEMERAL).
 *
 * The files go through LECTRIX_OPEN: on Windows, tauri-driver hands launch arguments to
 * WebView2 instead of the app. `dialogs` answers the app's file dialogs in order
 * (LECTRIX_DIALOG): each answer is a path, a list of paths, or null for Cancel. `env` adds
 * environment variables.
 */
export async function launch(files = [], { dialogs, env = {} } = {}) {
	for (const [what, path] of [
		['the release app', app],
		['msedgedriver', edgeDriver]
	]) {
		if (!existsSync(path)) throw new Error(`${what} not found at ${path}`);
	}
	mkdirSync(OUT, { recursive: true });
	clearStrays();
	const nativePort = await freePort();
	const driverArgs = ['--port', String(PORT), '--native-port', String(nativePort), '--native-driver', verboseDriver()];
	const driver = spawn(tauriDriver, driverArgs, {
		env: {
			...process.env,
			LECTRIX_EPHEMERAL: '1',
			LECTRIX_OPEN: files.join(';'),
			...(dialogs ? { LECTRIX_DIALOG: dialogs.map((a) => [a ?? []].flat().join('|')).join(';') } : {}),
			...(LIKE_CI ? LIKE_CI_ENV : {}),
			...env
		},
		stdio: ['ignore', 'inherit', 'inherit']
	});
	const exited = new Promise((done) => driver.once('exit', done));
	try {
		await waitForPort(PORT, 15_000);
		const browser = await remote({
			hostname: '127.0.0.1',
			port: PORT,
			logLevel: 'warn',
			// Fail fast: a session that cannot start in a minute will not start on retry.
			connectionRetryCount: 0,
			capabilities: {
				browserName: 'wry',
				'tauri:options': { application: app }
			}
		});
		started = ownPids();
		await ensureAppPage(browser);
		if (LIKE_CI) await checkLikeCi(browser);
		const stop = async () => {
			try {
				await browser.deleteSession();
			} finally {
				await killDriver(driver, exited);
				killApp();
			}
		};
		/** Ends Lectrix the way a crash would: no chance to clean up. */
		const crash = async () => {
			killApp();
			await browser.deleteSession().catch(() => {});
			await killDriver(driver, exited);
		};
		/**
		 * Waits for Lectrix to finish quitting by itself, then cleans up (the session is
		 * already gone). Resolves to whether Lectrix quit in time; it is ended either way.
		 */
		const exitedByItself = async () => {
			const deadline = Date.now() + 15_000;
			while (appRunning() && Date.now() < deadline) await new Promise((r) => setTimeout(r, 200));
			const quit = !appRunning();
			await browser.deleteSession().catch(() => {});
			await killDriver(driver, exited);
			killApp();
			return quit;
		};
		return { browser, stop, crash, exitedByItself };
	} catch (e) {
		await killDriver(driver, exited);
		killApp();
		throw e;
	}
}

const APP_URL = 'http://tauri.localhost/';

/**
 * msedgedriver navigates the webview to about:blank when a session starts. Usually Lectrix's
 * own navigation to its page comes after that, but on a slow machine (CI) it can come
 * first and be wiped out. Then load the page again: the frontend picks up the documents
 * Rust already has open (list_open_documents).
 */
async function ensureAppPage(browser) {
	const onApp = async () => (await browser.execute(() => location.href)).startsWith(APP_URL);
	const loaded = await browser.waitUntil(onApp, { timeout: 3000, interval: 100 }).catch(() => false);
	if (!loaded) await browser.url(APP_URL);
	// The page is there before the app in it is: until its title bar shows, nothing hears a
	// key, and a test that starts with a shortcut (Ctrl+, on the start screen) would lose it
	// on a slow runner. Not an error here: a test's own checks say what is missing.
	await browser
		.waitUntil(() => browser.execute(() => document.querySelector('header') !== null), { timeout: 15_000, interval: 100 })
		.catch(() => {});
}

/** Waits until the first page of the active document has an image, and the layout has
 * settled (the viewer fits the page from its first size measurement, which can arrive a few
 * frames after the first image; keys pressed before it would race the fit). On failure,
 * saves a screenshot and prints what the window shows (CI uploads target/test-output/e2e). */
export async function waitForDocument(browser) {
	try {
		await browser.waitUntil(
			() => browser.execute(() => document.querySelector('.page canvas, .page img') !== null),
			{ timeout: 20_000, timeoutMsg: 'the document did not show a page' }
		);
		await browser.executeAsync((done) => requestAnimationFrame(() => requestAnimationFrame(() => done())));
	} catch (e) {
		const shot = join(OUT, `failure-${Date.now()}.png`);
		await browser.saveScreenshot(shot).catch(() => {});
		const state = await browser
			.execute(() => ({
				text: document.body.innerText.slice(0, 600),
				pages: document.querySelectorAll('.page').length,
				viewport: [innerWidth, innerHeight, devicePixelRatio]
			}))
			.catch((err) => String(err));
		console.error(`window state: ${JSON.stringify(state)}; screenshot ${shot}`);
		throw e;
	}
}

/**
 * What the window says about the active document: the bar at the top (the page box's
 * position for screen readers, "iv (4 of 8)"), then the save state the tab shows by its
 * marker, in words: "Saving…", "Unsaved changes" or "All changes saved".
 */
export async function statusText(browser) {
	return browser.execute(() => {
		const bar = document.querySelector('[data-view-bar]')?.textContent ?? '';
		const tab = document.querySelector('[role="tab"][aria-selected="true"]');
		const state = tab?.querySelector('[data-saving]')
			? 'Saving…'
			: tab?.querySelector('[data-unsaved]')
				? 'Unsaved changes'
				: 'All changes saved';
		return `${bar.replace(/\s+/g, ' ')} ${state}`;
	});
}

/**
 * Waits until the element at `selector` is on screen and holds still: a grid lays its cells
 * out once it knows its width and then brings a page into view, and a click sent meanwhile
 * lands on whatever has come under it.
 */
export async function atRest(browser, selector) {
	const where = () =>
		browser.execute((sel) => {
			const r = document.querySelector(sel)?.getBoundingClientRect();
			return r && r.width > 0 ? [r.left, r.top, r.width, r.height].join() : null;
		}, selector);
	await browser.waitUntil(
		async () => {
			const before = await where();
			await browser.pause(150);
			return before !== null && before === (await where());
		},
		{ timeoutMsg: `${selector} did not come to rest` }
	);
}

/** Document > Organize pages: the document's pages as a grid, in the viewer's place
 * (AGENTS.md section 6.7). */
export async function organizePages(browser) {
	await (await browser.$('button=Document')).click();
	await (await browser.$('div*=Organize pages')).click();
	await (await browser.$('[data-organize-view]')).waitForDisplayed({ timeoutMsg: 'the grid of pages did not show' });
}

/**
 * Turns the first page in the document, a change to the file like any other for the tests
 * that need one: in the grid of pages (AGENTS.md section 6.7), then back to reading.
 */
export async function rotateFirstPage(browser) {
	await organizePages(browser);
	const cell = '[data-organize-view] [role=option][aria-posinset="1"]';
	await atRest(browser, cell);
	const first = await browser.$(cell);
	await first.click();
	await browser.keys('r');
	await browser.waitUntil(
		() => browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"] [data-unsaved]') !== null),
		{ timeoutMsg: 'turning a page in the grid did not change the document' }
	);
	await (await (await browser.$('[data-organize-bar]')).$('button=Done')).click();
	await waitForDocument(browser);
}

/** Presses keys with modifiers, e.g. keys(browser, ['Control', 'b']). */
export async function keys(browser, combo) {
	await browser.keys(combo);
}

/**
 * Clicks a control that makes Lectrix quit (the window's Close button, "Don't save" on the
 * last unsaved document). The window may be gone before the driver has answered the click
 * ("invalid session id: session deleted as the browser has closed the connection", or
 * "target frame detached ... unable to send message to renderer", both seen on CI): that
 * is the click doing its work. Any other error is thrown; whether Lectrix quit is the
 * caller's to check (exitedByItself).
 */
export async function clickToQuit(element) {
	await element.click().catch((e) => {
		if (!/invalid session id|disconnected|no such window|browser has closed|target frame detached/i.test(String(e?.message ?? e))) throw e;
	});
}
