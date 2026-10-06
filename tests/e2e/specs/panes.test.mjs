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
		// place, in the view bar's row, once the pane is closed: also on every frame while the
		// pane slides closed and open again.
		const hide = await box(browser, 'button[aria-label="Hide left pane"]');
		const pages = await box(browser, tabSelector('Pages'));
		assert.ok(hide && pages && Math.abs(hide.y - pages.y) <= 1 && hide.x < pages.x, `the button is at the start of the row: ${JSON.stringify({ hide, pages })}`);
		for (const name of ['Hide left pane', 'Show left pane']) {
			const places = await browser.executeAsync((label, done) => {
				const seen = [];
				document.querySelector(`button[aria-label="${label}"]`).click();
				const started = performance.now();
				const frame = () => {
					const r = document.querySelector('button[aria-label$="left pane"]')?.getBoundingClientRect();
					if (r) seen.push(`${Math.round(r.left)},${Math.round(r.top)}`);
					if (performance.now() - started < 300) requestAnimationFrame(frame);
					else done([...new Set(seen)]);
				};
				requestAnimationFrame(frame);
			}, name);
			assert.deepEqual(places, [`${Math.round(hide.left)},${Math.round(hide.top)}`], `${name}: the button stays still while the pane slides`);
		}
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

test('the Pages panel makes thumbnails smaller, larger, or as wide as the panel', async () => {
	const { browser, stop } = await launch([sample('thumbs.pdf', 3)]);
	try {
		await waitForDocument(browser);
		const thumb = () =>
			browser.execute(() => {
				const image = document.querySelector('[data-thumb="0"] > span');
				const list = document.querySelector('[aria-label="Page thumbnails"]');
				return image && list ? { width: image.getBoundingClientRect().width, list: list.clientWidth } : null;
			});
		const button = (label) => browser.$(`button[aria-label="${label}"]`);
		await browser.waitUntil(async () => (await thumb()) !== null, { timeoutMsg: 'no thumbnails' });
		assert.equal((await thumb()).width, 112, 'thumbnails start 112 px wide');

		// Larger glides to the next size in 140 ms (at once if the system asks for reduced
		// motion): the widths seen frame by frame pass between the two.
		const glide = await browser.executeAsync((done) => {
			const width = () => document.querySelector('[data-thumb="0"] > span').getBoundingClientRect().width;
			const seen = [];
			document.querySelector('button[aria-label="Larger thumbnails"]').click();
			const start = performance.now();
			const step = () => {
				seen.push(width());
				if (performance.now() - start < 400) requestAnimationFrame(step);
				else done({ seen, reduced: matchMedia('(prefers-reduced-motion: reduce)').matches });
			};
			requestAnimationFrame(step);
		});
		assert.equal(glide.seen.at(-1), 144, `Larger made them ${glide.seen.at(-1)} px`);
		if (!glide.reduced) assert.ok(glide.seen.some((w) => w > 112 && w < 144), `no glide: ${glide.seen.join(', ')}`);
		await (await button('Smaller thumbnails')).click();
		await (await button('Smaller thumbnails')).click();
		await browser.waitUntil(async () => (await thumb()).width === 88, { timeoutMsg: `Smaller made them ${(await thumb()).width} px` });

		// The layout switch: one column, then a grid where pages 1 and 2 sit side by side.
		const tops = () =>
			browser.execute(() => [0, 1].map((i) => document.querySelector(`[data-thumb="${i}"]`).getBoundingClientRect().top));
		const layoutSwitch = await button('Thumbnails in a grid');
		assert.equal(await layoutSwitch.getAttribute('aria-pressed'), 'false');
		const [a1, b1] = await tops();
		assert.ok(b1 > a1, 'one column puts page 2 under page 1');
		await layoutSwitch.click();
		assert.equal(await layoutSwitch.getAttribute('aria-pressed'), 'true');
		await browser.waitUntil(async () => {
			const [a, b] = await tops();
			return a === b;
		}, { timeoutMsg: `the grid did not put pages 1 and 2 side by side: ${await tops()}` });
		await layoutSwitch.click();

		// Fit: as wide as the panel, less the space beside them; Smaller leaves it.
		const fit = await button('Fit thumbnails to the panel width');
		await fit.click();
		assert.equal(await fit.getAttribute('aria-pressed'), 'true');
		await browser.waitUntil(async () => {
			const t = await thumb();
			return t.width === t.list - 32;
		}, { timeoutMsg: `fitted thumbnails are ${JSON.stringify(await thumb())}` });
		assert.equal(await (await button('Larger thumbnails')).isEnabled(), false, 'Larger is not disabled at the panel width');
		await (await button('Smaller thumbnails')).click();
		assert.equal(await fit.getAttribute('aria-pressed'), 'false', 'Smaller kept fitting the panel');
		await browser.waitUntil(async () => {
			const t = await thumb();
			return t.width < t.list - 32;
		}, { timeoutMsg: `Smaller from the panel width left them ${JSON.stringify(await thumb())}` });
	} finally {
		await stop();
	}
});
