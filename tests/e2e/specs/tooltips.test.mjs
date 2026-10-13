// Tooltips are the app's own (AGENTS.md section 8): no control has a `title` for the system
// to draw. A control's tooltip shows below it once the pointer rests there, with a tail
// pointing at the control, or above it where there is no room below; a press takes it away
// until the pointer leaves; Tab brings the focused control's; disabled controls show theirs.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

/** The control tooltip on screen: its text, its box, and its tail's box; null without one. */
function tooltip(browser) {
	return browser.execute(() => {
		const tip = document.querySelector('.tip-fixed');
		if (!tip || getComputedStyle(tip).visibility === 'hidden') return null;
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
		};
		const tail = tip.querySelector('.tip-tail-edge');
		return {
			text: tip.innerText.trim(),
			role: tip.getAttribute('role'),
			box: box(tip.querySelector('.tip-text')),
			tail: tail ? box(tail) : null,
			window: { w: innerWidth, h: innerHeight }
		};
	});
}

/** Puts the pointer on the middle of `selector` and returns the control's box. */
async function hover(browser, selector) {
	const box = await browser.execute((sel) => {
		const r = document.querySelector(sel).getBoundingClientRect();
		return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
	}, selector);
	const x = Math.round((box.left + box.right) / 2);
	const y = Math.round((box.top + box.bottom) / 2);
	await browser.action('pointer').move({ x, y, origin: 'viewport' }).perform();
	return box;
}

async function shown(browser, text) {
	await browser.waitUntil(async () => (await tooltip(browser))?.text === text, {
		timeoutMsg: `no tooltip “${text}”: ${JSON.stringify(await tooltip(browser))}`
	});
	return tooltip(browser);
}

/** The tooltip is beside `control` without covering it, inside the window, and its tail
 * reaches out of it towards the control, stopping short of it. */
function assertPointsAt(tip, control, what) {
	const detail = `${what}: ${JSON.stringify({ tip, control })}`;
	assert.equal(tip.role, 'tooltip', detail);
	assert.ok(tip.tail, `a tail, ${detail}`);
	assert.ok(tip.box.left >= 0 && tip.box.right <= tip.window.w && tip.box.top >= 0 && tip.box.bottom <= tip.window.h, `inside the window, ${detail}`);
	const middle = (control.left + control.right) / 2;
	assert.ok(tip.tail.left < middle && tip.tail.right > middle, `the tail is at the control's middle, ${detail}`);
	if (tip.box.top >= control.bottom) {
		assert.ok(tip.tail.top < tip.box.top - 3 && tip.tail.top >= control.bottom, `the tail points up, ${detail}`);
	} else {
		assert.ok(tip.box.bottom <= control.top, `above the control, ${detail}`);
		assert.ok(tip.tail.bottom > tip.box.bottom + 3 && tip.tail.bottom <= control.top, `the tail points down, ${detail}`);
	}
}

test('controls show the app’s own tooltips, with a tail', async () => {
	const { browser, stop } = await launch([sample('tooltips.pdf', 3)]);
	try {
		await waitForDocument(browser);
		assert.equal(await browser.execute(() => document.querySelectorAll('[title]').length), 0, 'nothing has a system tooltip');

		// Resting on a button of the view bar: its tooltip below it, centred on it.
		const zoomIn = await hover(browser, 'button[aria-label^="Zoom in"]');
		assert.equal(await tooltip(browser), null, 'not at once');
		const tip = await shown(browser, 'Zoom in (Ctrl+=)');
		assertPointsAt(tip, zoomIn, 'zoom in');
		assert.ok(tip.box.top >= zoomIn.bottom, 'below the button');
		assert.ok(Math.abs((tip.box.left + tip.box.right) / 2 - (zoomIn.left + zoomIn.right) / 2) <= 1, 'centred on the button');

		// A press takes it away, and it stays away while the pointer stays on the button.
		const press = { x: Math.round((zoomIn.left + zoomIn.right) / 2), y: Math.round((zoomIn.top + zoomIn.bottom) / 2), origin: 'viewport' };
		await browser.action('pointer').move(press).down().up().perform();
		await browser.pause(900);
		assert.equal(await tooltip(browser), null, 'gone after a press');
		const zoomOut = await hover(browser, 'button[aria-label^="Zoom out"]');
		assertPointsAt(await shown(browser, 'Zoom out (Ctrl+-)'), zoomOut, 'zoom out');

		// Tab brings the tooltip of the control it lands on, which screen readers get as its
		// description; another key takes it away.
		await browser.execute(() => document.querySelector('button[aria-label="Next page"]').focus());
		await browser.keys(['Tab']);
		const focused = await browser.execute(() => ({
			id: document.activeElement.id,
			tip: document.activeElement.dataset.tip ?? '',
			description: document.activeElement.getAttribute('aria-description')
		}));
		assert.equal(focused.id, 'page-box');
		assert.match(focused.tip, /^Type a page number/);
		assert.equal(focused.description, focused.tip);
		await shown(browser, focused.tip);
		await browser.keys(['Shift']);
		assert.equal(await tooltip(browser), null, 'gone after a key');
		// A button whose tooltip only repeats its name gets no description.
		await browser.execute(() => document.querySelector('button[aria-label="Next page"]').focus());
		assert.equal(await browser.execute(() => document.activeElement.getAttribute('aria-description')), null);

		// The annotation tools: the tooltip keeps clear of the tool, whichever side has room.
		const tool = '[role=toolbar][aria-label="Annotation tools"] [aria-label="Highlight"]';
		const highlight = await hover(browser, tool);
		assertPointsAt(await shown(browser, 'Highlight (H)'), highlight, 'highlight tool');

		// A disabled field says why: the first range's start can't be changed. (The page
		// labels are beside the grid of pages.)
		await (await browser.$('button[aria-label="Organize pages"]')).click();
		await (await browser.$('button[aria-label="Page labels"]')).click();
		const why = 'The first range always starts at page 1.';
		const field = `input[data-tip="${why}"]`;
		await (await browser.$(field)).waitForDisplayed({ timeoutMsg: 'the first range is not shown' });
		// (Once the labels have slid in: the field is measured where it stays.)
		await browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')), {
			timeoutMsg: 'the page labels kept sliding'
		});
		assert.equal(await browser.execute((sel) => document.querySelector(sel).disabled, field), true);
		const start = await hover(browser, field);
		assertPointsAt(await shown(browser, why), start, 'disabled field');
	} finally {
		await stop();
	}
});
