// Visual adjustments (AGENTS.md section 6.1): the button among the page controls opens a
// panel with brightness, contrast, sepia, inverted colours and greyscale. They change how the
// document's pages look on screen, never the file, and Lectrix remembers them for that file.
// Settings give every document its adjustments, until a document has its own (section 6.6).

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

/** The filter on the first page's layer and on its thumbnail, how much of the sepia paper
 * tone lies over each (null for none), and what the button says. */
function look(browser) {
	return browser.execute((button) => {
		const b = document.querySelector(button);
		return {
			page: document.querySelector('.page[data-page="0"] [data-page-layer]')?.style.filter ?? null,
			thumbnail: document.querySelector('[data-thumb="0"] > span > span')?.style.filter ?? null,
			tone: document.querySelector('.page[data-page="0"] [data-page-tone]')?.style.opacity ?? null,
			thumbTone: document.querySelector('[data-thumb="0"] [data-page-tone]')?.style.opacity ?? null,
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
	const NONE = { page: '', thumbnail: '', tone: null, thumbTone: null };

	let run = await launch([path], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		assert.deepEqual(await look(browser), { ...NONE, button: 'Visual adjustments', dot: false });
		const white = await marginColour(browser);
		assert.ok(white.every((c) => c > 240), `the page is white as it opens: ${white}`);
		assert.ok(await (await browser.$(`[data-view-bar] ${BUTTON}`)).isExisting(), 'the button is among the page controls');

		// Inverted: the page and its thumbnail turn dark at once.
		await openPanel(browser);
		const [invert, greyscale] = await browser.$$(`${PANEL} [role=switch]`);
		assert.equal(await (await (await browser.$(PANEL)).$('button=Reset')).isEnabled(), false, 'nothing to reset yet');

		// Sepia, the third slider, from none to all of it: an e-book's sepia. The tone of old
		// paper lies over the page and its thumbnail, as much of it as the slider says, and
		// the white paper takes it (red most, blue least); the filter has nothing of it.
		assert.match(await (await browser.$(PANEL)).getText(), /Sepia\s*0%/);
		await browser.execute((panel) => document.querySelectorAll(`${panel} input[type=range]`)[2].focus(), PANEL);
		await browser.keys(['ArrowRight']);
		const some = await until(browser, 'the sepia slider did nothing', (s) => s.tone === '0.05');
		assert.equal(some.page, '', 'sepia is in the filter');
		assert.equal(some.dot, true, 'the button does not say that sepia is on');
		await browser.keys(['End']);
		const toned = await until(browser, 'End did not give all the sepia', (s) => s.tone === '1');
		assert.equal(toned.thumbTone, toned.tone, 'the thumbnails look as the pages do');
		assert.match(await (await browser.$(PANEL)).getText(), /Sepia\s*100%/);
		const paper = await marginColour(browser);
		assert.ok(
			paper[0] > paper[1] + 8 && paper[1] > paper[2] + 20 && paper[0] > 225 && paper[2] < 205,
			`the paper is not sepia: ${paper}`
		);
		await browser.saveScreenshot(join(OUT, 'visual-adjustments-sepia.png'));
		// Half of it is a lighter paper.
		await browser.keys(['Home']);
		for (let i = 0; i < 10; i++) await browser.keys(['ArrowRight']);
		await until(browser, 'the sepia slider did not come to 50%', (s) => s.tone === '0.5');
		const lighter = await marginColour(browser);
		assert.ok(lighter[2] > paper[2] + 15 && lighter[0] > lighter[2] + 10, `half the sepia is not a lighter paper: ${lighter} against ${paper}`);
		// Back to a level that the rest of the test keeps: 30%.
		for (let i = 0; i < 4; i++) await browser.keys(['ArrowLeft']);
		await until(browser, 'the sepia slider did not come to 30%', (s) => s.tone === '0.3');
		await invert.click();
		// The tone lies over the inverted page: its paper stays dark.
		const inverted = await until(browser, 'Invert colours did nothing', (s) => s.page === 'invert(1) hue-rotate(180deg)');
		assert.equal(inverted.thumbnail, inverted.page, 'the thumbnails look as the pages do');
		assert.equal(inverted.tone, '0.3');
		const dark = await marginColour(browser);
		assert.ok(dark.every((c) => c < 15), `the inverted page is dark: ${dark}`);

		// Brightness from the keyboard, a step at a time; greyscale.
		await browser.execute((panel) => document.querySelector(`${panel} input[type=range]`).focus(), PANEL);
		await browser.keys(['ArrowLeft']);
		await browser.keys(['ArrowLeft']);
		await until(browser, 'the brightness slider did nothing', (s) => s.page.endsWith('brightness(0.9)'));
		assert.match(await (await browser.$(PANEL)).getText(), /Brightness\s*90%/);
		await greyscale.click();
		await until(browser, 'Greyscale did nothing', (s) => s.page === ADJUSTED && s.tone === '0.3');

		// Esc closes the panel, and nothing else (with a tool picked, the window's own Esc
		// would put the Select tool back); the button says something is adjusted.
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true, timeoutMsg: 'Esc did not close the panel' });
		await browser.keys(['h']);
		await openPanel(browser);
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true, timeoutMsg: 'Esc did not close the panel again' });
		assert.equal(await (await browser.$('button[aria-label="Highlight"]')).getAttribute('aria-pressed'), 'true', 'Esc also put the tool back');
		assert.deepEqual(await look(browser), {
			page: ADJUSTED,
			thumbnail: ADJUSTED,
			tone: '0.3',
			thumbTone: '0.3',
			button: 'Visual adjustments: on',
			dot: true
		});

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
	// The document's own adjustments are what is remembered for it.
	const stored = Object.values(JSON.parse(readFileSync(state, 'utf8')).views).map((v) => v.view.ownAdjustments);
	assert.deepEqual(stored, [{ brightness: 90, contrast: 100, invert: true, grayscale: true, sepia: 30 }]);

	// The next time: that file opens as it was left, another file as it is.
	run = await launch([path, other], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		const tab = (name) => browser.$(`[role="tab"]*=${basename(name)}`);
		await (await tab(other)).click();
		await until(browser, 'another file took the adjustments', (s) => s.page === '' && s.tone === null && !s.dot);
		await (await tab(path)).click();
		await until(browser, 'the adjustments were not remembered', (s) => s.page === ADJUSTED && s.tone === '0.3' && s.dot);

		// Reset puts the pages back as they are.
		await openPanel(browser);
		await (await (await browser.$(PANEL)).$('button=Reset')).click();
		await until(browser, 'Reset left something adjusted', (s) => s.page === '' && s.thumbnail === '' && s.tone === null && s.thumbTone === null && !s.dot);
		assert.match(await (await browser.$(PANEL)).getText(), /Brightness\s*100%/);
		assert.match(await (await browser.$(PANEL)).getText(), /Sepia\s*0%/);
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

/** Opens Settings > Appearance, where the adjustments of every document are. */
async function openDefaults(browser) {
	await browser.keys(['Control', ',']);
	await (await browser.$('button[role="tab"]*=Appearance')).click();
	await (await browser.$('#setting-pageAdjustments-sepia')).waitForExist({ timeoutMsg: 'Settings has no sepia for every document' });
}

async function closeSettings(browser) {
	await browser.keys(['Escape']);
	await (await browser.$('#setting-pageAdjustments-sepia')).waitForExist({ reverse: true, timeoutMsg: 'Settings did not close' });
	// (The window behind it is dimmed a moment longer, as the dialog fades out.)
	await (await browser.$('.dialog-overlay')).waitForExist({ reverse: true, timeoutMsg: 'the window stayed dimmed' });
}

/** Moves a slider of Settings with the keys: focus on it, then `presses`. */
async function slide(browser, selector, presses) {
	await browser.execute((sel) => document.querySelector(sel).focus(), selector);
	for (const key of presses) await browser.keys([key]);
}

test('Settings give every document its adjustments; a document adjusted by itself keeps its own; both can be reset', async () => {
	const state = join(OUT, 'visual-adjustments-defaults-state.json');
	rmSync(state, { force: true });
	const env = { LECTRIX_STATE_FILE: state };
	const first = sample('adjusted-first.pdf', 2);
	const second = sample('adjusted-second.pdf', 2);
	const tab = (browser, path) => browser.$(`[role="tab"]*=${basename(path)}`);
	const SEPIA = '#setting-pageAdjustments-sepia';
	const RESET = '#setting-resetPageAdjustments';

	let run = await launch([first, second], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		await until(browser, 'a document opens adjusted', (s) => s.page === '' && s.tone === null && !s.dot);

		// Settings > Appearance > Visual adjustments: sepia for every document, 40%.
		await openDefaults(browser);
		assert.equal(await (await browser.$(RESET)).isEnabled(), false, 'nothing to reset yet');
		await slide(browser, SEPIA, Array(8).fill('ArrowRight'));
		await browser.waitUntil(async () => (await (await browser.$(SEPIA)).getValue()) === '40', { timeoutMsg: 'the slider did not come to 40%' });
		await browser.saveScreenshot(join(OUT, 'visual-adjustments-settings.png'));
		await closeSettings(browser);
		// Both documents are shown so, and the button says something is adjusted.
		await until(browser, 'the open document did not take the sepia of Settings', (s) => s.tone === '0.4' && s.thumbTone === '0.4' && s.dot);
		await (await tab(browser, first)).click();
		await until(browser, 'the other document did not take the sepia of Settings', (s) => s.tone === '0.4' && s.dot);

		// The panel shows what the document has, and has nothing of its own to reset.
		await openPanel(browser);
		assert.match(await (await browser.$(PANEL)).getText(), /Sepia\s*40%/);
		assert.equal(await (await (await browser.$(PANEL)).$('button=Reset')).isEnabled(), false, 'a document that follows Settings has something to reset');
		// This document inverted: it has adjustments of its own, from what it showed.
		const [invert] = await browser.$$(`${PANEL} [role=switch]`);
		await invert.click();
		await until(browser, 'Invert colours did nothing', (s) => s.page === 'invert(1) hue-rotate(180deg)' && s.tone === '0.4');
		assert.equal(await (await (await browser.$(PANEL)).$('button=Reset')).isEnabled(), true);
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true });

		// Settings changed again: the other document follows, this one keeps its own.
		await openDefaults(browser);
		await slide(browser, SEPIA, ['End']);
		await browser.waitUntil(async () => (await (await browser.$(SEPIA)).getValue()) === '100');
		await closeSettings(browser);
		await until(browser, 'a document with its own adjustments followed Settings', (s) => s.page === 'invert(1) hue-rotate(180deg)' && s.tone === '0.4');
		await (await tab(browser, second)).click();
		await until(browser, 'the other document did not follow Settings', (s) => s.page === '' && s.tone === '1');

		await browser.execute(() => void window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }));
		assert.ok(await run.exitedByItself(), 'Lectrix did not quit');
	} catch (e) {
		await run.stop();
		throw e;
	}
	// Only the document adjusted by itself has adjustments remembered for it.
	const stored = JSON.parse(readFileSync(state, 'utf8'));
	const own = Object.values(stored.views).map((v) => v.view.ownAdjustments).filter((a) => a !== null);
	assert.deepEqual(own, [{ brightness: 100, contrast: 100, invert: true, grayscale: false, sepia: 40 }]);
	assert.deepEqual(stored.settings.page_adjustments, { brightness: 100, contrast: 100, invert: false, grayscale: false, sepia: 100 });

	// The next time: each as it was left.
	run = await launch([first, second], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		await (await tab(browser, second)).click();
		await until(browser, 'Settings’ adjustments were not remembered', (s) => s.page === '' && s.tone === '1' && s.dot);
		await (await tab(browser, first)).click();
		await until(browser, 'the document’s own adjustments were not remembered', (s) => s.page === 'invert(1) hue-rotate(180deg)' && s.tone === '0.4');

		// Reset in the panel: back to the adjustments of Settings.
		await openPanel(browser);
		await (await (await browser.$(PANEL)).$('button=Reset')).click();
		await until(browser, 'Reset did not go back to Settings’ adjustments', (s) => s.page === '' && s.tone === '1' && s.dot);
		assert.match(await (await browser.$(PANEL)).getText(), /Sepia\s*100%/);
		await browser.keys(['Escape']);
		await (await browser.$(PANEL)).waitForExist({ reverse: true });

		// Reset in Settings: every document is shown as it is again.
		await openDefaults(browser);
		const reset = await browser.$(RESET);
		assert.equal(await reset.isEnabled(), true);
		await reset.click();
		await browser.waitUntil(async () => (await (await browser.$(SEPIA)).getValue()) === '0', { timeoutMsg: 'Reset left the sepia of Settings' });
		assert.equal(await reset.isEnabled(), false);
		await closeSettings(browser);
		await until(browser, 'Reset in Settings left a document adjusted', (s) => s.page === '' && s.tone === null && !s.dot);
		const white = await marginColour(browser);
		assert.ok(white.every((c) => c > 240), `the page is white again: ${white}`);
	} finally {
		await run.stop();
	}
});
