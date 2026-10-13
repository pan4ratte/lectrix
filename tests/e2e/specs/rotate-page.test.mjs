// Turning pages (AGENTS.md section 6.1). The Pages panel turns a page as it is shown: on
// screen only, never in the file, remembered for the file, and there for a file open for
// reading only. A page turned in the document (in the grid of pages, or by undoing that)
// keeps showing its old pixels, turned with the page, until the new image arrives: never
// squeezed into the page's new shape.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, rotateFirstPage, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

const hash = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

const CLOCKWISE = 'button[aria-label="Rotate current page clockwise"]';
const COUNTER = 'button[aria-label="Rotate current page counter-clockwise"]';

/** How page `index` is shown: its box in the viewer and how its layer is turned, and the
 * same for its thumbnail. Null for what is not on screen. */
function shown(browser, index) {
	return browser.execute((i) => {
		const of = (box, layer) => {
			if (!box || !layer) return null;
			const r = box.getBoundingClientRect();
			return { width: r.width, height: r.height, left: r.left, top: r.top, turn: layer.style.transform || 'none' };
		};
		const page = document.querySelector(`.page[data-page="${i}"]`);
		const thumb = document.querySelector(`[data-thumb="${i}"] > span`);
		return {
			page: of(page, page?.querySelector('[data-page-layer]')),
			thumb: of(thumb, thumb?.querySelector('span'))
		};
	}, index);
}

async function until(browser, index, what, check) {
	let seen;
	await browser.waitUntil(async () => check((seen = await shown(browser, index))), { timeout: 8000 }).catch(() => {
		throw new Error(`${what}: ${JSON.stringify(seen)}`);
	});
	return seen;
}

const landscape = (b) => b !== null && b.width > b.height;
const portrait = (b) => b !== null && b.height > b.width;
const turnedBy = (b, degrees) => b !== null && b.turn.includes(`rotate(${degrees}deg)`);

/** The selection's rectangles, in viewport pixels. */
function selected(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('.page rect.fill-selection')].map((el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		})
	);
}

test('a page turned from the Pages panel turns on screen only, and is remembered for its file', async () => {
	const state = join(OUT, 'turned-pages-state.json');
	rmSync(state, { force: true });
	const env = { LECTRIX_STATE_FILE: state };
	const path = sample('turned.pdf', 3);
	const before = hash(path);

	let run = await launch([path], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		const upright = await shown(browser, 0);
		assert.ok(portrait(upright.page) && portrait(upright.thumb), `page 1 opens upright: ${JSON.stringify(upright)}`);
		assert.equal(upright.page.turn, 'none');

		// The current page, clockwise: it lies on its side in the viewer and in its thumbnail,
		// fitted to the width again, and the page after it stays as it was.
		await (await browser.$(CLOCKWISE)).click();
		const turned = await until(
			browser,
			0,
			'the page did not turn',
			(s) => landscape(s.page) && turnedBy(s.page, 90) && landscape(s.thumb) && turnedBy(s.thumb, 90)
		);
		const fits = await browser.execute(() => {
			const scroller = document.querySelector('.viewer-scroll');
			return { content: scroller.scrollWidth, view: scroller.clientWidth };
		});
		assert.ok(fits.content <= fits.view, `the turned page is wider than the view: ${JSON.stringify(fits)}`);
		assert.ok(turned.page.width > fits.view * 0.9, `the turned page was not fitted to the width: ${turned.page.width} of ${fits.view}`);
		const second = await shown(browser, 1);
		assert.ok(portrait(second.thumb) && second.thumb.turn === 'none', `page 2 turned too: ${JSON.stringify(second)}`);
		await browser.saveScreenshot(join(OUT, 'turned-page.png'));

		// Nothing of this is a change to the document.
		assert.match(await statusText(browser), /All changes saved/);

		// The pointer finds the text where it is now: the marker line (96 pt from the page's
		// top, from 36 pt in) runs down the screen, near the page's right edge. A double-click
		// on it selects a word there.
		const k = turned.page.width / 792;
		const word = { x: Math.round(turned.page.left + (792 - 92) * k), y: Math.round(turned.page.top + 100 * k) };
		await browser.action('pointer').move({ ...word, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		await browser.waitUntil(async () => (await selected(browser)).length === 1, {
			timeoutMsg: `a double-click on the turned text selected nothing (at ${JSON.stringify(word)})`
		});
		const [mark] = await selected(browser);
		assert.ok(mark.bottom - mark.top > mark.right - mark.left, `the selected word does not run down the screen: ${JSON.stringify(mark)}`);
		assert.ok(
			word.x >= mark.left - 2 && word.x <= mark.right + 2 && word.y >= mark.top - 2 && word.y <= mark.bottom + 2,
			`the selection is not where the pointer was: ${JSON.stringify({ word, mark })}`
		);

		// Another page, from its thumbnail's menu, the other way.
		await (await browser.$('[data-thumb="1"]')).click({ button: 'right' });
		await (await browser.$('[role=menu]')).waitForDisplayed({ timeoutMsg: 'the thumbnail has no menu' });
		await (await browser.$('div*=Rotate page counter-clockwise')).click();
		await until(browser, 1, 'page 2 did not turn', (s) => landscape(s.thumb) && turnedBy(s.thumb, 270));

		assert.match(await statusText(browser), /All changes saved/);
		assert.equal(hash(path), before, 'the file changed');
		await browser.execute(() => void window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }));
		assert.ok(await run.exitedByItself(), 'Lectrix did not quit (did it ask to save?)');
	} catch (e) {
		await run.stop();
		throw e;
	}
	assert.equal(hash(path), before, 'the file changed on the way out');
	assert.doesNotMatch(cli('info', path), /rotated pages/);
	const stored = Object.values(JSON.parse(readFileSync(state, 'utf8')).views).map((v) => v.view.pageTurns);
	assert.deepEqual(stored, [{ 0: 90, 1: 270 }]);

	// The next time, the pages are turned as they were left; turned back, they are as the
	// file has them.
	run = await launch([path], { env });
	try {
		const { browser } = run;
		await waitForDocument(browser);
		await until(browser, 0, 'the turn was not remembered', (s) => landscape(s.page) && turnedBy(s.page, 90) && landscape(s.thumb));
		await until(browser, 1, 'the turn of page 2 was not remembered', (s) => landscape(s.thumb) && turnedBy(s.thumb, 270));
		await (await browser.$(COUNTER)).click();
		await until(browser, 0, 'the page did not turn back', (s) => portrait(s.page) && s.page.turn === 'none' && portrait(s.thumb));
		assert.match(await statusText(browser), /All changes saved/);
	} finally {
		await run.stop();
	}
	assert.equal(hash(path), before, 'the file changed the second time');
});

test('a file open for reading only has its pages turned on screen all the same', async () => {
	const path = sample('turned-pdfa.pdf', 2, ['--pdfa', '1b']);
	const before = hash(path);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const banner = await browser.$('[data-pdfa-banner]');
		await banner.waitForDisplayed({ timeoutMsg: 'no banner for a PDF/A file' });
		assert.equal(await (await browser.$(CLOCKWISE)).isEnabled(), true);
		await (await browser.$(CLOCKWISE)).click();
		await until(browser, 0, 'the page did not turn', (s) => landscape(s.page) && turnedBy(s.page, 90));
		// Still open for reading only, and nothing was refused.
		assert.equal(await banner.isDisplayed(), true, 'the banner went');
		const refused = await browser.$('//*[@role="status"][contains(., "open for reading only")]');
		assert.equal(await refused.isExisting(), false, 'turning the page was refused');
		assert.match(await statusText(browser), /All changes saved/);
	} finally {
		await stop();
	}
	assert.equal(hash(path), before, 'the file changed');
});

test('a text box typed on a page turned on screen is upright there, and written so', async () => {
	const path = sample('turned-text-box.pdf', 2);
	const { browser, stop } = await launch([path]);
	/** The editor's box on screen and its text, or null while there is none. */
	const editor = () =>
		browser.execute(() => {
			const e = document.querySelector('textarea[data-annotation-editor]');
			if (!e) return null;
			const r = e.getBoundingClientRect();
			return { left: r.left, top: r.top, width: r.width, height: r.height, text: e.value };
		});
	try {
		await waitForDocument(browser);
		await (await browser.$(CLOCKWISE)).click();
		const turned = (await until(browser, 0, 'the page did not turn', (s) => landscape(s.page) && turnedBy(s.page, 90))).page;
		// (A fitted zoom glides to the turned page's width.)
		await browser.pause(300);
		const page = (await shown(browser, 0)).page;
		assert.ok(landscape(page), JSON.stringify({ turned, page }));

		// T, a click in the empty part of the page as it lies on screen, and the text.
		const k = page.width / 792;
		const at = { x: Math.round(page.left + 200 * k), y: Math.round(page.top + 520 * k) };
		await browser.keys('t');
		await browser.action('pointer').move({ ...at, origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(async () => (await editor()) !== null, { timeoutMsg: 'no editor for the text box' });
		const typing = await editor();
		// The box starts at the click and is 200 pt wide on screen, one line high.
		assert.ok(typing.width > 150 * k && typing.width > 4 * typing.height, `the editor is not upright: ${JSON.stringify(typing)}`);
		assert.ok(
			Math.abs(typing.left - at.x) < 3 && Math.abs(typing.top - at.y) < 3,
			`the editor is not where the page was clicked: ${JSON.stringify({ at, typing })}`
		);
		await browser.keys([...'Upright']);
		await browser.saveScreenshot(join(OUT, 'turned-text-box.png'));
		await browser.keys(['Control', 'Enter']);
		await browser.waitUntil(async () => /Unsaved changes/.test(await statusText(browser)), {
			timeoutMsg: 'the text box did not change the document'
		});

		// Opened again with a double-click, it is edited as it stands.
		await browser.keys('Escape');
		await browser.keys('Escape');
		const inside = { x: Math.round(at.x + 20 * k), y: Math.round(at.y + 6 * k) };
		await browser.action('pointer').move({ ...inside, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		await browser.waitUntil(async () => (await editor()) !== null, { timeoutMsg: 'a double-click did not open the text box' });
		const again = await editor();
		assert.equal(again.text, 'Upright');
		assert.ok(again.width > 4 * again.height, `the editor of the written box is not upright: ${JSON.stringify(again)}`);
		assert.ok(Math.abs(again.left - at.x) < 3 && Math.abs(again.top - at.y) < 3, JSON.stringify({ at, again }));
		await browser.keys('Escape');

		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => /All changes saved/.test(await statusText(browser)), { timeoutMsg: 'not saved' });
	} finally {
		await stop();
	}
	// The page is as it was in the file; the box carries the turn.
	assert.doesNotMatch(cli('info', path), /rotated pages/);
	assert.match(cli('annot', 'list', path), /^p1 object \d+ FreeText .* text "Upright" turned 90$/m);
});

test('a page turned in the document is never shown squeezed', async () => {
	const path = sample('rotate-page.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		// Turned in the grid of pages; back to reading, the page and its thumbnail are drawn
		// on their side.
		await rotateFirstPage(browser);
		const canvases = '.page[data-page="0"] canvas, [data-thumb="0"] canvas';
		const drawn = () =>
			browser.execute(
				(sel) => [...document.querySelectorAll(sel)].map((c) => ({ w: c.width, h: c.height, t: c.style.transform })),
				canvases
			);
		await browser.waitUntil(async () => (await drawn()).length >= 2 && (await drawn()).every((c) => c.w > c.h), {
			timeoutMsg: `page 1 and its thumbnail were not drawn landscape: ${JSON.stringify(await drawn())}`
		});
		// Pages render again once the first fit settles; undo after that.
		await browser.pause(800);

		// Every frame from before the undo until both are drawn again: the pixels' proportions
		// against the box they are laid out in (before it is turned).
		await browser.execute((sel) => {
			const seen = { frames: 0, worst: 1, turned: 0, done: false };
			window.__watch = seen;
			const t0 = performance.now();
			const tick = () => {
				for (const c of document.querySelectorAll(sel)) {
					if (c.width <= 1 || c.offsetHeight === 0) continue;
					const shown = c.offsetWidth / c.offsetHeight;
					const drawn = c.width / c.height;
					seen.worst = Math.max(seen.worst, shown / drawn, drawn / shown);
					if (c.style.transform) seen.turned++;
				}
				seen.frames++;
				if (performance.now() - t0 < 2500) requestAnimationFrame(tick);
				else seen.done = true;
			};
			requestAnimationFrame(tick);
		}, canvases);
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(() => browser.execute(() => window.__watch.done), { timeoutMsg: 'the watch did not end' });
		const watch = await browser.execute(() => window.__watch);

		assert.ok(watch.frames > 20, `watched ${watch.frames} frames`);
		assert.ok(watch.worst < 1.05, `pixels were squeezed by ${watch.worst.toFixed(2)}x`);
		const final = await drawn();
		for (const c of final) {
			assert.ok(c.h > c.w, `the page is drawn portrait at the end: ${JSON.stringify(final)}`);
			assert.equal(c.t, '', 'and no longer turned by CSS');
		}
	} finally {
		await stop();
	}
});
