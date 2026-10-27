// The start screen's tool cards (section 6.1, ADR 0030): 204 px wide, three to a row, and
// the cards of a last row that is not full stretch to fill it.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

/** The tool cards' widths, to the pixel, with `extra` copies of the last one added first. */
function widths(browser, extra) {
	return browser.execute((n) => {
		const grid = document.querySelector('.start-tools');
		for (let i = 0; i < n; i++) grid?.append(grid.lastElementChild?.cloneNode(true) ?? '');
		return [...document.querySelectorAll('.start-tool')].map((card) => Math.round(card.getBoundingClientRect().width));
	}, extra);
}

test('a last row of tool cards that is not full is filled', async () => {
	const { browser, stop } = await launch();
	try {
		await browser.waitUntil(async () => (await browser.$$('.start-tool')).length > 0, {
			timeout: 15000,
			timeoutMsg: 'the start screen shows no tools'
		});
		const count = (await browser.$$('.start-tool')).length;
		// Copies of a card make a last row of one, then two, then a full one, whatever the
		// number of tools.
		const first = (3 - (count % 3) + 1) % 3;
		let cards = await widths(browser, first);
		assert.equal(cards.length % 3, 1);
		assert.deepEqual(cards.slice(0, -1), cards.slice(0, -1).map(() => 204), 'full rows keep the cards’ width');
		assert.equal(cards.at(-1), 3 * 204 + 2 * 12, 'one card on the last row spans it');

		cards = await widths(browser, 1);
		assert.deepEqual(cards.slice(-2), [312, 312], 'two cards on the last row share it');

		cards = await widths(browser, 1);
		assert.deepEqual(cards, cards.map(() => 204), 'a full last row keeps the cards’ width');
	} finally {
		await stop();
	}
});

/** How far the start screen and the list of recent files each scroll, the height of the
 * room the list has, and the room above the content and under the list. */
function scrolling(browser) {
	return browser.execute(() => {
		const screen = document.querySelector('.start-screen');
		const list = document.querySelector('.recent-scroll');
		const box = screen?.getBoundingClientRect();
		return {
			screen: screen ? screen.scrollHeight - screen.clientHeight : -1,
			list: list ? list.scrollHeight - list.clientHeight : -1,
			listHeight: document.querySelector('.recent-area')?.clientHeight ?? 0,
			above: box ? Math.round(document.querySelector('.start-content').getBoundingClientRect().top - box.top) : -1,
			below: box && list ? Math.round(box.bottom - list.getBoundingClientRect().bottom) : -1
		};
	});
}

test('a long list of recent files scrolls by itself beside the tools, the start screen under them', async () => {
	const files = Array.from({ length: 20 }, (_, i) => sample(`recent-${i + 1}.pdf`, 1));
	const { browser, stop } = await launch(files);
	try {
		await waitForDocument(browser);
		await (await browser.$('button[aria-label="Lectrix: start screen"]')).click();
		await browser.waitUntil(async () => (await browser.$$('.recent-row')).length === files.length, {
			timeoutMsg: 'the start screen does not list the files as recent'
		});

		// Under the tools, in CI's window: the whole list shows, and the start screen scrolls.
		let s = await scrolling(browser);
		assert.equal(s.list, 0, `the list does not scroll by itself (${JSON.stringify(s)})`);
		assert.ok(s.screen > 0, `the start screen scrolls (${JSON.stringify(s)})`);

		// Beside the tools, as in a larger window: only the list scrolls, and it ends as far
		// above the start screen's bottom as the content starts below its top.
		await browser.execute(() => (document.documentElement.style.zoom = '0.8'));
		await browser.waitUntil(() => browser.execute(() => getComputedStyle(document.querySelector('.start-body')).display === 'grid'), {
			timeoutMsg: 'the recent files did not go beside the tools'
		});
		s = await scrolling(browser);
		assert.ok(s.list > 0, `the list scrolls (${JSON.stringify(s)})`);
		assert.equal(s.screen, 0, 'the start screen does not scroll');
		assert.ok(s.listHeight >= 180, `the list keeps 180 px (${JSON.stringify(s)})`);
		assert.ok(Math.abs(s.below - s.above) <= 1, `as much room under the list as above the content (${JSON.stringify(s)})`);

		// The list starts level with the tools, its heading 10 px above it, as under them (in
		// CSS pixels: the boxes are measured at the zoom set above).
		const level = await browser.execute(() => {
			const box = (selector) => document.querySelector(selector).getBoundingClientRect();
			const zoom = Number(document.documentElement.style.zoom) || 1;
			return {
				list: Math.round(box('.recent-scroll').top - box('.start-tool').top),
				heading: Math.round((box('.recent-scroll').top - box('.recent-head').bottom) / zoom)
			};
		});
		assert.deepEqual(level, { list: 0, heading: 10 }, 'the list starts level with the tools, its heading just above it');
	} finally {
		await stop();
	}
});

test('the start screen scrolls only along an axis its content overflows', async () => {
	const { browser, stop } = await launch();
	try {
		await browser.waitUntil(async () => (await browser.$$('.start-tool')).length > 0, { timeout: 15000 });
		const axes = () =>
			browser.execute(() => {
				const style = getComputedStyle(document.querySelector('.start-screen'));
				return { x: style.overflowX, y: style.overflowY };
			});
		// CI's window holds it all.
		assert.deepEqual(await axes(), { x: 'hidden', y: 'hidden' });
		// Zoomed in, its content is wider and taller than the window.
		await browser.execute(() => (document.documentElement.style.zoom = '2'));
		await browser.waitUntil(async () => (await axes()).x === 'auto' && (await axes()).y === 'auto', {
			timeoutMsg: `the start screen does not scroll when its content is larger (${JSON.stringify(await axes())})`
		});
	} finally {
		await stop();
	}
});
