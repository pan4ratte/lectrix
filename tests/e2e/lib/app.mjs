// Launches Folio under tauri-driver and returns a WebdriverIO browser for its webview.
//
// Needs: the release app (`npx tauri build --no-bundle`), `tauri-driver` (cargo install
// tauri-driver --locked) and an msedgedriver matching the installed WebView2 runtime
// (tests/e2e/fetch-edgedriver.ps1). FOLIO_APP, TAURI_DRIVER and MSEDGEDRIVER override
// the default locations.

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { createConnection } from 'node:net';
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
 * Ends any Folio still running. Folio is single-instance: a leftover process would take
 * over the next launch, which then exits before WebDriver can attach.
 */
function killStrayApps() {
	spawnSync('taskkill', ['/IM', 'folio.exe', '/F', '/T'], { stdio: 'ignore' });
}

/**
 * Starts Folio with `files` (PDF paths) open and returns `{ browser, stop }`. Recent
 * files and remembered views stay out of the user's app data (FOLIO_EPHEMERAL).
 *
 * The files go through FOLIO_OPEN: on Windows, tauri-driver hands launch arguments to
 * WebView2 instead of the app.
 */
export async function launch(files = []) {
	for (const [what, path] of [
		['the release app', app],
		['msedgedriver', edgeDriver]
	]) {
		if (!existsSync(path)) throw new Error(`${what} not found at ${path}`);
	}
	mkdirSync(OUT, { recursive: true });
	killStrayApps();
	const driver = spawn(tauriDriver, ['--port', String(PORT), '--native-driver', edgeDriver], {
		env: { ...process.env, FOLIO_EPHEMERAL: '1', FOLIO_OPEN: files.join(';') },
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
		const stop = async () => {
			try {
				await browser.deleteSession();
			} finally {
				driver.kill();
				await exited;
				killStrayApps();
			}
		};
		return { browser, stop };
	} catch (e) {
		driver.kill();
		await exited;
		killStrayApps();
		throw e;
	}
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
