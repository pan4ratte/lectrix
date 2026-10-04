// Launches Folio under tauri-driver and returns a WebdriverIO browser for its webview.
//
// Needs: the release app (`npx tauri build --no-bundle`), `tauri-driver` (cargo install
// tauri-driver --locked) and an msedgedriver matching the installed WebView2 runtime
// (tests/e2e/fetch-edgedriver.ps1). FOLIO_APP, TAURI_DRIVER and MSEDGEDRIVER override
// the default locations.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createConnection, createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { remote } from 'webdriverio';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
export const OUT = join(ROOT, 'target', 'test-output', 'e2e');
const PORT = 4444;

const app = process.env.FOLIO_APP ?? join(ROOT, 'target', 'release', 'folio.exe');
const tauriDriver = process.env.TAURI_DRIVER ?? 'tauri-driver';
const edgeDriver = process.env.MSEDGEDRIVER ?? join(ROOT, 'target', 'e2e-tools', 'msedgedriver.exe');

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
 * uploaded by CI when the tests fail).
 */
function verboseDriver() {
	const wrapper = join(dirname(edgeDriver), 'msedgedriver-verbose.cmd');
	const log = join(OUT, 'driver-session.log');
	writeFileSync(wrapper, `@"${edgeDriver}" %* --verbose --append-log "--log-path=${log}"
`);
	return wrapper;
}

/**
 * Ends any Folio still running. Folio is single-instance: a leftover process would take
 * over the next launch, which then exits before WebDriver can attach.
 */
function killStrayApps() {
	spawnSync('taskkill', ['/IM', 'folio.exe', '/F', '/T'], { stdio: 'ignore' });
}

function appRunning() {
	const list = spawnSync('tasklist', ['/FI', 'IMAGENAME eq folio.exe', '/NH'], { encoding: 'utf8' });
	return list.stdout.toLowerCase().includes('folio.exe');
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
 * Starts Folio with `files` (PDF paths) open and returns `{ browser, stop }`. Recent
 * files and remembered views stay out of the user's app data (FOLIO_EPHEMERAL).
 *
 * The files go through FOLIO_OPEN: on Windows, tauri-driver hands launch arguments to
 * WebView2 instead of the app. `dialogs` answers the app's file dialogs in order
 * (FOLIO_DIALOG): each answer is a path, a list of paths, or null for Cancel. `env` adds
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
	killStrayApps();
	const nativePort = await freePort();
	const driverArgs = ['--port', String(PORT), '--native-port', String(nativePort), '--native-driver', verboseDriver()];
	const driver = spawn(tauriDriver, driverArgs, {
		env: {
			...process.env,
			FOLIO_EPHEMERAL: '1',
			FOLIO_OPEN: files.join(';'),
			...(dialogs ? { FOLIO_DIALOG: dialogs.map((a) => [a ?? []].flat().join('|')).join(';') } : {}),
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
		await ensureAppPage(browser);
		const stop = async () => {
			try {
				await browser.deleteSession();
			} finally {
				await killDriver(driver, exited);
				killStrayApps();
			}
		};
		/** Ends Folio the way a crash would: no chance to clean up. */
		const crash = async () => {
			killStrayApps();
			await browser.deleteSession().catch(() => {});
			await killDriver(driver, exited);
		};
		/**
		 * Waits for Folio to finish quitting by itself, then cleans up (the session is
		 * already gone). Resolves to whether Folio quit in time; it is ended either way.
		 */
		const exitedByItself = async () => {
			const deadline = Date.now() + 15_000;
			while (appRunning() && Date.now() < deadline) await new Promise((r) => setTimeout(r, 200));
			const quit = !appRunning();
			await browser.deleteSession().catch(() => {});
			await killDriver(driver, exited);
			killStrayApps();
			return quit;
		};
		return { browser, stop, crash, exitedByItself };
	} catch (e) {
		await killDriver(driver, exited);
		killStrayApps();
		throw e;
	}
}

const APP_URL = 'http://tauri.localhost/';

/**
 * msedgedriver navigates the webview to about:blank when a session starts. Usually Folio's
 * own navigation to its page comes after that, but on a slow machine (CI) it can come
 * first and be wiped out. Then load the page again: the frontend picks up the documents
 * Rust already has open (list_open_documents).
 */
async function ensureAppPage(browser) {
	const onApp = async () => (await browser.execute(() => location.href)).startsWith(APP_URL);
	const loaded = await browser.waitUntil(onApp, { timeout: 3000, interval: 100 }).catch(() => false);
	if (!loaded) await browser.url(APP_URL);
}

/** Waits until the first page of the active document has an image. On failure, saves a
 * screenshot and prints what the window shows (CI uploads target/test-output/e2e). */
export async function waitForDocument(browser) {
	try {
		await browser.waitUntil(
			() => browser.execute(() => document.querySelector('.page canvas, .page img') !== null),
			{ timeout: 20_000, timeoutMsg: 'the document did not show a page' }
		);
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

/** Presses keys with modifiers, e.g. keys(browser, ['Control', 'b']). */
export async function keys(browser, combo) {
	await browser.keys(combo);
}
