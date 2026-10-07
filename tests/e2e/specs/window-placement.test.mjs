// The window's place between runs (AGENTS.md section 8): Lectrix starts at the normal size
// and position it had last time, maximized if it was, and Restore then goes back to that
// normal size. LECTRIX_STATE_FILE gives these runs a state file of their own.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch } from '../lib/app.mjs';

const STATE = join(OUT, 'window-state.json');
// Above the minimum size (640 x 480) at 100% to 150% scale, and within a 1024 x 768 screen.
const NORMAL = { x: 100, y: 60, width: 990, height: 750 };

const saved = () => JSON.parse(readFileSync(STATE, 'utf8')).window;

/** Where the page is on screen and its size, in physical pixels. The size is the window's
 * inner size, as saved; the position is the page's, inside the window's invisible resize
 * border, so it is compared between runs rather than with the saved outer position. */
function rect(browser) {
	return browser.execute(() => {
		const r = devicePixelRatio;
		return { x: screenX * r, y: screenY * r, width: innerWidth * r, height: innerHeight * r };
	});
}

function assertNear(actual, expected, what, keys = ['x', 'y', 'width', 'height']) {
	for (const k of keys) {
		assert.ok(Math.abs(actual[k] - expected[k]) <= 2, `${what}: ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
	}
}

async function maximizeButton(browser) {
	const button = await browser.$('button[aria-label="Maximize"], button[aria-label="Restore"]');
	await button.waitForClickable({ timeout: 15000 });
	return button;
}

/** Closes the window with its own button and waits for Lectrix to quit. */
async function quit(browser, exitedByItself) {
	await (await browser.$('button[aria-label="Close"]')).click();
	assert.ok(await exitedByItself(), 'Lectrix did not quit');
}

test('the window comes back where it was, maximized if it was', async () => {
	rmSync(STATE, { force: true });
	writeFileSync(STATE, JSON.stringify({ window: { bounds: NORMAL, maximized: false } }));
	const env = { LECTRIX_STATE_FILE: STATE };

	// The saved normal place; then maximized, which keeps the normal bounds.
	let first;
	let run = await launch([], { env });
	try {
		await maximizeButton(run.browser);
		first = await rect(run.browser);
		assertNear(first, NORMAL, 'started at the saved size', ['width', 'height']);
		await (await maximizeButton(run.browser)).click();
		await run.browser.waitUntil(async () => (await (await maximizeButton(run.browser)).getAttribute('aria-label')) === 'Restore', {
			timeoutMsg: 'the window did not maximize'
		});
		await quit(run.browser, run.exitedByItself);
	} catch (e) {
		await run.stop();
		throw e;
	}
	assert.ok(existsSync(STATE));
	assert.equal(saved().maximized, true, 'remembered as maximized');
	// Exactly: the window was put there from these bounds, and maximizing keeps them.
	assert.deepEqual(saved().bounds, NORMAL, 'maximizing kept the normal bounds');

	// Maximized again at the next start; Restore goes back to the saved normal place.
	run = await launch([], { env });
	try {
		const button = await maximizeButton(run.browser);
		assert.equal(await button.getAttribute('aria-label'), 'Restore', 'started maximized');
		await button.click();
		await run.browser.waitUntil(async () => (await button.getAttribute('aria-label')) === 'Maximize', {
			timeoutMsg: 'the window did not restore'
		});
		await run.browser.pause(300);
		assertNear(await rect(run.browser), first, 'restored to the place of the first run');
		await quit(run.browser, run.exitedByItself);
	} catch (e) {
		await run.stop();
		throw e;
	}
	assert.equal(saved().maximized, false, 'remembered as a normal window');
	assert.deepEqual(saved().bounds, NORMAL);
});
