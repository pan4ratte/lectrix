// Visual adjustments (AGENTS.md section 6.1): the button among the page controls opens a
// panel with brightness, contrast, inverted colours and greyscale. They change how the
// document's pages look on screen, never the file, and Lectrix remembers them for that file.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, rmSync } from 'node:fs';
import { basename, join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const BUTTON = '[data-visual-adjustments]';
const PANEL = '[role=dialog][aria-label="Visual adjustments"]';

const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

/** The filter on the first page's layer and on its thumbnail, and what the button says. */
function look(browser) {
	return browser.execute((button) => {
		const b = document.querySelector(button);
		return {
			page: document.querySelector('.page[data-page="0"] [data-page-layer]')?.style.filter ?? null,
			thumbnail: document.querySelector('[data-thumb="0"] > span > span')?.style.filter ?? null,
			button: b?.getAttribute('aria-label') ?? null,
			dot: b?.querySelector('.adjust-dot') != null
		};
	}, BUTTON);
}

/** The colour on screen at the top-left corner of the first page (its margin), from a
 * screenshot: what the user sees there, filter included. */
async function marginColour(browser) {
	const shot = await browser.takeScreenshot();
	return browser.executeAsync((png, done) => {
		const image = new Image();
		image.onload = () => {
			const r = document.querySelector('.page[data-page="0"]').getBoundingClientRect();
			const k = image.width / innerWidth;
			const canvas = document.createElement('canvas');
			canvas.width = image.width;
			canvas.height = image.height;
			const context = canvas.getContext('2d');
			context.drawImage(image, 0, 0);
			const top = Math.max(r.top, document.querySelector('.viewer-scroll').getBoundingClientRect().top);
			done([...context.getImageData(Math.round((r.left + 12) * k), Math.round((top + 12) * k), 1, 1).data.slice(0, 3)]);
		};
		image.src = `data:image/png;base64,${png}`;
	}, shot);
}

async function openPanel(browser) {
	await (await browser.$(BUTTON)).click();
	await (await browser.$(PANEL)).waitForDisplayed({ timeoutMsg: 'the panel did not open' });
}

async function until(browser, what, check) {
	let seen;
	await browser.waitUntil(async () => check((seen = await look(browser))), { timeout: 8000 }).catch(() => {
		throw new Error(`${what}: ${JSON.stringify(seen)}`);
	});
	return seen;
}

test('visual adjustments change how the pages look, not the file, and are remembered for it', async () => {
	const state = join(OUT, 'visual-adjustments-state.json');
	rmSync(state, { force: true });
	const env = { LECTRIX_STATE_FILE: state };
	const path = sample('adjusted.pdf', 3);
	const other = sample('not-adjusted.pdf', 2);
	const before = hash(path);
	const ADJUSTED = 'grayscale(1) invert(1) hue-rotate(180deg) brightness(0.9)';

	let run = await launch([path], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		assert.deepEqual(await look(browser), { page: '', thumbnail: '', button: 'Visual adjustments', dot: false });
		const white = await marginColour(browser);
		assert.ok(white.every((c) => c > 240), `the page is white as it opens: ${white}`);
		assert.ok(await (await browser.$(`[data-view-bar] ${BUTTON}`)).isExisting(), 'the button is among the page controls');

		// Inverted: the page and its thumbnail turn dark at once.
		await openPanel(browser);
		const [invert, greyscale] = await browser.$$(`${PANEL} [role=switch]`);
		assert.equal(await (await (await browser.$(PANEL)).$('button=Reset')).isEnabled(), false, 'nothing to reset yet');
		await invert.click();
		const inverted = await until(browser, 'Invert colours did nothing', (s) => s.page === 'invert(1) hue-rotate(180deg)');
		assert.equal(inverted.thumbnail, inverted.page, 'the thumbnails look as the pages do');
		const dark = await marginColour(browser);
		assert.ok(dark.every((c) => c < 15), `the inverted page is dark: ${dark}`);

		// Brightness from the keyboard, a step at a time; greyscale.
		await browser.execute((panel) => document.querySelector(`${panel} input[type=range]`).focus(), PANEL);
		await browser.keys(['ArrowLeft']);
		await browser.keys(['ArrowLeft']);
		await until(browser, 'the brightness slider did nothing', (s) => s.page.endsWith('brightness(0.9)'));
		assert.match(await (await browser.$(PANEL)).getText(), /Brightness\s*90%/);
		await greyscale.click();
		await until(browser, 'Greyscale did nothing', (s) => s.page === ADJUSTED);

		// Esc closes the panel, and nothing else (with a tool picked, the window's own Esc
		// would put the Select tool back); the button says something is adjusted.
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true, timeoutMsg: 'Esc did not close the panel' });
		await browser.keys(['h']);
		await openPanel(browser);
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true, timeoutMsg: 'Esc did not close the panel again' });
		assert.equal(await (await browser.$('button[aria-label="Highlight"]')).getAttribute('aria-pressed'), 'true', 'Esc also put the tool back');
		assert.deepEqual(await look(browser), { page: ADJUSTED, thumbnail: ADJUSTED, button: 'Visual adjustments: on', dot: true });

		// Nothing of this is a change to the document.
		assert.match(await statusText(browser), /All changes saved/);
		assert.equal(hash(path), before, 'the file changed');

		await browser.execute(() => void window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }));
		assert.ok(await run.exitedByItself(), 'Lectrix did not quit (did it ask to save?)');
	} catch (e) {
		await run.stop();
		throw e;
	}
	assert.equal(hash(path), before, 'the file changed on the way out');
	const stored = Object.values(JSON.parse(readFileSync(state, 'utf8')).views).map((v) => v.view.adjustments);
	assert.deepEqual(stored, [{ brightness: 90, contrast: 100, invert: true, grayscale: true }]);

	// The next time: that file opens as it was left, another file as it is.
	run = await launch([path, other], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		const tab = (name) => browser.$(`[role="tab"]*=${basename(name)}`);
		await (await tab(other)).click();
		await until(browser, 'another file took the adjustments', (s) => s.page === '' && !s.dot);
		await (await tab(path)).click();
		await until(browser, 'the adjustments were not remembered', (s) => s.page === ADJUSTED && s.dot);

		// Reset puts the pages back as they are.
		await openPanel(browser);
		await (await (await browser.$(PANEL)).$('button=Reset')).click();
		await until(browser, 'Reset left something adjusted', (s) => s.page === '' && s.thumbnail === '' && !s.dot);
		assert.match(await (await browser.$(PANEL)).getText(), /Brightness\s*100%/);
		const white = await marginColour(browser);
		assert.ok(white.every((c) => c > 240), `the page is white again: ${white}`);
	} finally {
		await run.stop();
	}
});

test('full screen has the visual adjustments in its toolbar', async () => {
	const { browser, stop } = await launch([sample('adjusted-full.pdf', 2)]);
	try {
		await waitForDocument(browser);
		await browser.keys(['F11']);
		const button = await browser.$(`[role=toolbar][aria-label="Page, zoom and annotation tools"] ${BUTTON}`);
		await button.waitForDisplayed({ timeoutMsg: 'no visual adjustments in the floating toolbar' });
		// The page controls slide open in the toolbar as full screen starts.
		await browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')));
		await button.click();
		const panel = await browser.$(PANEL);
		await panel.waitForDisplayed({ timeoutMsg: 'the panel did not open' });
		// Above a toolbar at the bottom of the screen, and wholly on screen.
		const box = await browser.execute((sel) => {
			const r = document.querySelector(sel).getBoundingClientRect();
			return { top: r.top, bottom: r.bottom, height: innerHeight };
		}, PANEL);
		assert.ok(box.top >= 0 && box.bottom <= box.height, `the panel is cut off: ${JSON.stringify(box)}`);
		// Esc closes the panel, not full screen.
		await browser.keys(['Escape']);
		await panel.waitForExist({ reverse: true });
		assert.equal(await browser.execute(() => document.querySelector('header') === null), true, 'Esc left full screen');
	} finally {
		await stop();
	}
});
