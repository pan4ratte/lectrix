// Side panes (AGENTS.md section 8): each pane's button sits at the outer end of its panel
// row and stays in that place while the pane is closed; panels move between the panes by
// dragging their tabs or with Ctrl+Shift+Left/Right.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { focused } from '../lib/a11y.mjs';
import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

/** The centre and left edge of the first element matching `selector`, or null. */
function box(browser, selector) {
	return browser.execute((s) => {
		const el = document.querySelector(s);
		if (!el) return null;
		const r = el.getBoundingClientRect();
		return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), left: r.left, top: r.top };
	}, selector);
}

/** The panels' names in each pane's row, in order. */
function rows(browser) {
	return browser.execute(() => {
		const names = (pane) =>
			[...document.querySelectorAll(`aside[aria-label="${pane}"] [data-panel-tab]`)].map((t) => t.getAttribute('aria-label'));
		return { left: names('Left pane'), right: names('Right pane') };
	});
}

/** Waits until no pane is sliding. */
function settled(browser) {
	return browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')), {
		timeoutMsg: 'a pane kept sliding'
	});
}

/** Drags from one point to another in small steps, as a hand would. */
async function drag(browser, from, to, { atTarget } = {}) {
	let action = browser.action('pointer').move({ x: from.x, y: from.y, origin: 'viewport' }).down();
	for (let i = 1; i <= 8; i++) {
		action = action
			.move({ x: Math.round(from.x + ((to.x - from.x) * i) / 8), y: Math.round(from.y + ((to.y - from.y) * i) / 8), origin: 'viewport' })
			.pause(16);
	}
	await action.perform(true);
	if (atTarget) await atTarget();
	await browser.action('pointer').up().perform();
}

const tabSelector = (name) => `[data-panel-tab][aria-label="${name}"]`;

test('pane buttons keep their place; panels move between the panes', async () => {
	const { browser, stop } = await launch([sample('panes.pdf', 3)]);
	try {
		await waitForDocument(browser);
		assert.deepEqual(await rows(browser), { left: ['Pages', 'Bookmarks', 'Page labels'], right: [] });

		// The left pane's button is in its panel row, before the tabs, and stays in the same
		// place, in the view bar's row, once the pane is closed.
		const hide = await box(browser, 'aside[aria-label="Left pane"] button[aria-label="Hide left pane"]');
		const pages = await box(browser, tabSelector('Pages'));
		assert.ok(hide && pages && Math.abs(hide.y - pages.y) <= 1 && hide.x < pages.x, 'the button is at the start of the row');
		await (await browser.$('button[aria-label="Hide left pane"]')).click();
		await browser.waitUntil(() => browser.execute(() => !document.querySelector('aside[aria-label="Left pane"]')), {
			timeoutMsg: 'the left pane did not close'
		});
		const show = await box(browser, 'button[aria-label="Show left pane"]');
		assert.ok(show && Math.abs(show.x - hide.x) <= 1 && Math.abs(show.y - hide.y) <= 1, `the button kept its place: ${JSON.stringify({ hide, show })}`);
		await (await browser.$('button[aria-label="Show left pane"]')).click();
		await (await browser.$(tabSelector('Pages'))).waitForDisplayed();

		// Page labels dropped on the closed right pane's button: the pane opens with it.
		await drag(browser, await box(browser, tabSelector('Page labels')), await box(browser, 'button[aria-label="Show right pane"]'));
		await browser.waitUntil(async () => (await rows(browser)).right.length === 2, { timeoutMsg: 'the drop did not move the panel' });
		await settled(browser);
		assert.deepEqual(await rows(browser), { left: ['Pages', 'Bookmarks'], right: ['Annotations', 'Page labels'] });
		assert.equal(
			await browser.execute((s) => document.querySelector(s)?.getAttribute('aria-selected'), tabSelector('Page labels')),
			'true',
			'the moved panel is shown'
		);

		// Along a row: Annotations dropped before Pages in the left pane. The right pane keeps
		// Page labels.
		await drag(browser, await box(browser, tabSelector('Annotations')), {
			x: (await box(browser, tabSelector('Pages'))).left + 2,
			y: (await box(browser, tabSelector('Pages'))).y
		});
		await browser.waitUntil(async () => (await rows(browser)).left.length === 3, {
			timeoutMsg: 'Annotations did not move left'
		}).catch(async (e) => {
			throw new Error(`${e.message}: ${JSON.stringify(await rows(browser))}`);
		});
		assert.deepEqual(await rows(browser), { left: ['Annotations', 'Pages', 'Bookmarks'], right: ['Page labels'] });

		// From the keyboard: Ctrl+Shift+Left at the right pane's start goes to the left pane's
		// end; the pane left empty closes and its button goes.
		await (await browser.$(tabSelector('Page labels'))).click();
		await browser.keys(['Control', 'Shift', 'ArrowLeft']);
		await browser.waitUntil(async () => (await rows(browser)).right.length === 0, { timeoutMsg: 'Ctrl+Shift+Left did not move the panel' });
		assert.deepEqual((await rows(browser)).left, ['Annotations', 'Pages', 'Bookmarks', 'Page labels']);
		assert.equal((await focused(browser)).name, 'Page labels', 'the moved tab keeps focus');
		assert.equal(await (await browser.$('button[aria-label="Show right pane"]')).isExisting(), false);

		// Dragged, a panel finds a place to drop at the empty pane's end.
		await drag(browser, await box(browser, tabSelector('Bookmarks')), { x: 0, y: 0 }, {
			atTarget: async () => {
				const slot = await box(browser, '[data-panel-drop="right"]');
				assert.ok(slot, 'a drop place shows for the empty right pane');
				await browser.action('pointer').move({ x: slot.x, y: slot.y, origin: 'viewport' }).pause(16).perform(true);
			}
		});
		await browser.waitUntil(async () => (await rows(browser)).right.length === 1, { timeoutMsg: 'the drop on the empty pane did not move the panel' });
		assert.deepEqual(await rows(browser), { left: ['Annotations', 'Pages', 'Page labels'], right: ['Bookmarks'] });
	} finally {
		await stop();
	}
});
