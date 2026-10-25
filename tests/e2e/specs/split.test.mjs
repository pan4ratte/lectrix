// Documents side by side in one window (AGENTS.md section 6.1, split view): a tab goes
// beside the one shown from its menu or dragged to an edge of the pages, as one joined tab;
// each pane has a header and its own place; the view bar's lock scrolls them together,
// keeping their distance, and panes of one document also share zoom and rotation; the
// joined tab's menu and the view bar turn that lock, and synchronized zoom, on and off.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const TABS = '[role=tablist][aria-label="Open documents"] [data-tab]';
const tabNames = (browser) => browser.execute((s) => [...document.querySelectorAll(s)].map((t) => t.textContent.replace(/\s+/g, ' ').trim()), TABS);
const menuItem = (browser, name) => browser.$(`//*[@role="menuitem"][contains(., "${name}")]`);

/** Each pane: its header's name, its scroll position, whether it has the focus, and its
 * first page's size on screen. */
const panes = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[data-split-pane]')].map((pane) => {
			const page = pane.querySelector('[data-page]')?.getBoundingClientRect();
			return {
				name: pane.querySelector('.split-header span')?.textContent?.trim() ?? '',
				top: pane.querySelector('.viewer-scroll')?.scrollTop ?? -1,
				focused: pane.classList.contains('split-pane-focused'),
				page: page ? [Math.round(page.width), Math.round(page.height)] : null
			};
		})
	);

async function scrollPane(browser, index, deltaY) {
	const scrollers = await browser.$$('[data-split-pane] .viewer-scroll');
	await scrollers[index].moveTo();
	await browser.action('wheel').scroll({ deltaY, origin: scrollers[index] }).perform();
	await browser.pause(400);
}

/** Drags from one point to another in small steps, as a hand would. */
async function drag(browser, from, to, { atTarget } = {}) {
	let action = browser.action('pointer').move({ x: from.x, y: from.y, origin: 'viewport' }).down();
	for (let i = 1; i <= 10; i++) {
		action = action
			.move({ x: Math.round(from.x + ((to.x - from.x) * i) / 10), y: Math.round(from.y + ((to.y - from.y) * i) / 10), origin: 'viewport' })
			.pause(16);
	}
	await action.perform(true);
	if (atTarget) await atTarget();
	await browser.action('pointer').up().perform();
}

test('two documents side by side scroll together while locked, keeping their distance', async () => {
	const report = sample('side-report.pdf', 20);
	const draft = sample('side-draft.pdf', 20);
	const { browser, stop } = await launch([report, draft]);
	try {
		await waitForDocument(browser);
		// From the other tab's menu: beside the one shown, as one tab.
		await (await browser.$('.doc-tab*=side-report.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Show beside the current tab')).click();
		await browser.waitUntil(async () => (await panes(browser)).length === 2, { timeoutMsg: 'the documents are not side by side' });
		assert.deepEqual(await tabNames(browser), ['side-draft.pdf side-report.pdf']);
		let shown = await panes(browser);
		assert.deepEqual(
			shown.map((p) => [p.name, p.focused]),
			[
				['side-draft.pdf', false],
				['side-report.pdf', true]
			],
			'the document that joined has the focus'
		);

		// Unlocked, each scrolls by itself.
		await waitForDocument(browser);
		await scrollPane(browser, 0, 2400);
		shown = await panes(browser);
		assert.ok(shown[0].top > 1000 && shown[1].top < 50, `unlocked: ${shown.map((p) => p.top)}`);
		const apart = shown[0].top - shown[1].top;

		// Locked, the other follows as far as the one scrolled goes.
		const lock = await browser.$('[data-sync-scrolling]');
		assert.equal(await lock.getAttribute('aria-pressed'), 'false');
		await lock.click();
		assert.equal(await lock.getAttribute('aria-pressed'), 'true');
		await scrollPane(browser, 1, 1200);
		shown = await panes(browser);
		assert.ok(shown[1].top > 1000, `the scrolled pane did not move: ${shown.map((p) => p.top)}`);
		assert.ok(Math.abs(shown[0].top - shown[1].top - apart) < 40, `the distance changed: ${apart} then ${shown[0].top - shown[1].top}`);
		// Either one leads.
		await scrollPane(browser, 0, -600);
		shown = await panes(browser);
		assert.ok(Math.abs(shown[0].top - shown[1].top - apart) < 40, `the distance changed when the other led: ${shown.map((p) => p.top)}`);

		// The header's button gives a document its tab again.
		await (await browser.$('button[aria-label="Show side-draft.pdf in a tab of its own"]')).click();
		await browser.waitUntil(async () => (await tabNames(browser)).length === 2, { timeoutMsg: 'the pane did not get a tab of its own' });
		assert.deepEqual(await tabNames(browser), ['side-report.pdf', 'side-draft.pdf']);
		assert.equal((await panes(browser)).length, 0, 'a tab of one document still shows panes');
	} finally {
		await stop();
	}
});

test('one document shown twice: each view its own place, the same zoom while locked', async () => {
	const path = sample('side-twice.pdf', 10);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await (await browser.$('.doc-tab*=side-twice.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Show twice, side by side')).click();
		await browser.waitUntil(async () => (await panes(browser)).length === 2, { timeoutMsg: 'the document is not shown twice' });
		assert.deepEqual(
			(await panes(browser)).map((p) => p.name),
			['side-twice.pdf', 'side-twice.pdf (2)']
		);
		await waitForDocument(browser);
		// Unlocked, zooming one leaves the other.
		await browser.keys(['Control', '=']);
		await browser.waitUntil(async () => {
			const [a, b] = await panes(browser);
			return a.page && b.page && b.page[0] > a.page[0] + 20;
		}, { timeoutMsg: 'zooming did not act on the pane in focus alone' });
		// Locked, the other takes the zoom of the one in focus.
		await (await browser.$('[data-sync-scrolling]')).click();
		await browser.waitUntil(async () => {
			const [a, b] = await panes(browser);
			return a.page && b.page && Math.abs(a.page[0] - b.page[0]) <= 2;
		}, { timeoutMsg: 'the other view did not take the zoom' });
		// Ctrl+W closes the view in focus; the document stays open in the other.
		await browser.keys(['Control', 'w']);
		await browser.waitUntil(async () => (await panes(browser)).length === 0, { timeoutMsg: 'Ctrl+W did not close the second view' });
		assert.deepEqual(await tabNames(browser), ['side-twice.pdf']);
		await waitForDocument(browser);
	} finally {
		await stop();
	}
});

test("the joined tab's menu synchronizes scrolling and zoom", async () => {
	const report = sample('side-zoom-report.pdf', 10);
	const draft = sample('side-zoom-draft.pdf', 10);
	const { browser, stop } = await launch([report, draft]);
	const check = (name) => browser.$(`//*[@role="menuitemcheckbox"][contains(., "${name}")]`);
	/** Opens the joined tab's menu at `name`: whether it is checked, and the item. */
	const menuCheck = async (name) => {
		await (await browser.$('.doc-tab*=side-zoom-draft.pdf')).click({ button: 'right' });
		const item = await check(name);
		await item.waitForDisplayed({ timeoutMsg: `the tab's menu has no ${name}` });
		return { checked: await item.getAttribute('aria-checked'), item };
	};
	/** Clicks `name` in the joined tab's menu: whether it was checked. */
	const toggle = async (name) => {
		const { checked, item } = await menuCheck(name);
		await item.click();
		return checked;
	};
	const widths = async () => (await panes(browser)).map((p) => p.page?.[0] ?? 0);
	try {
		await waitForDocument(browser);
		await (await browser.$('.doc-tab*=side-zoom-report.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Show beside the current tab')).click();
		await browser.waitUntil(async () => (await panes(browser)).length === 2, { timeoutMsg: 'the documents are not side by side' });
		await waitForDocument(browser);

		// Synchronize scrolling there is the view bar's lock.
		assert.equal(await toggle('Synchronize scrolling'), 'false');
		await browser.waitUntil(async () => (await (await browser.$('[data-sync-scrolling]')).getAttribute('aria-pressed')) === 'true', {
			timeoutMsg: "the tab's menu did not lock the scrolling"
		});
		assert.equal(await toggle('Synchronize scrolling'), 'true');
		assert.equal(await (await browser.$('[data-sync-scrolling]')).getAttribute('aria-pressed'), 'false');

		// Unsynchronized, zooming one leaves the other.
		await browser.keys(['Control', '=']);
		await browser.waitUntil(async () => {
			const [a, b] = await widths();
			return b > a + 20;
		}, { timeoutMsg: 'zooming did not act on the pane in focus alone' });

		// Synchronized, the other document takes the zoom of the one in focus at once, and
		// of the one zoomed after. The view bar's button shows it on.
		const zoomButton = await browser.$('[data-sync-zoom]');
		assert.equal(await zoomButton.getAttribute('aria-pressed'), 'false');
		assert.equal(await toggle('Synchronize zoom'), 'false');
		await browser.waitUntil(async () => (await zoomButton.getAttribute('aria-pressed')) === 'true', {
			timeoutMsg: "the view bar's button does not show synchronized zoom on"
		});
		await browser.waitUntil(async () => {
			const [a, b] = await widths();
			return a > 0 && Math.abs(a - b) <= 2;
		}, { timeoutMsg: 'the other document did not take the zoom' });
		await browser.keys(['Control', '=']);
		await browser.pause(600);
		let [a, b] = await widths();
		assert.ok(Math.abs(a - b) <= 2, `the zoom went apart: ${a} and ${b}`);
		// A fit is a fit in each pane.
		await browser.keys(['Control', '0']);
		await browser.pause(600);
		[a, b] = await widths();
		assert.ok(Math.abs(a - b) <= 2, `fit width went apart: ${a} and ${b}`);

		// Off again with the button, each zooms by itself; the tab's menu shows it off.
		await zoomButton.click();
		assert.equal(await zoomButton.getAttribute('aria-pressed'), 'false');
		await browser.keys(['Control', '-']);
		await browser.waitUntil(async () => {
			const [a, b] = await widths();
			return Math.abs(a - b) > 20;
		}, { timeoutMsg: 'zooming still acted on both panes' });
		assert.equal((await menuCheck('Synchronize zoom')).checked, 'false');
		await browser.keys('Escape');
	} finally {
		await stop();
	}
});

test('a tab dragged to an edge of the pages goes beside them', async () => {
	const left = sample('side-left.pdf', 3);
	const right = sample('side-right.pdf', 3);
	const { browser, stop } = await launch([left, right]);
	try {
		await waitForDocument(browser);
		// side-right.pdf is shown; side-left.pdf dragged to the right edge of its pages.
		const from = await browser.execute((s) => {
			const r = [...document.querySelectorAll(s)].find((t) => t.textContent.includes('side-left.pdf')).getBoundingClientRect();
			return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
		}, TABS);
		const to = await browser.execute(() => {
			const r = document.querySelector('.viewer-scroll').getBoundingClientRect();
			return { x: Math.round(r.right - 40), y: Math.round(r.top + r.height / 2) };
		});
		await drag(browser, from, to, {
			atTarget: async () => {
				// The column it would take shows; the tab on screen stays (the one it goes beside).
				assert.equal(await browser.execute(() => document.querySelector('[data-split-drop]')?.getAttribute('data-split-drop') ?? null), '1');
			}
		});
		await browser.waitUntil(async () => (await panes(browser)).length === 2, { timeoutMsg: 'the dropped tab did not go beside the pages' });
		assert.deepEqual(
			(await panes(browser)).map((p) => p.name),
			['side-right.pdf', 'side-left.pdf']
		);
		assert.equal(await browser.execute(() => document.querySelector('[data-split-drop]')), null, 'the drop place stayed');
	} finally {
		await stop();
	}
});

test('the tab on screen dragged to an edge goes beside the one shown before it', async () => {
	const files = ['side-one.pdf', 'side-two.pdf', 'side-three.pdf'].map((name) => sample(name, 3));
	const { browser, stop } = await launch(files);
	try {
		await waitForDocument(browser);
		// side-one.pdf shown, then side-three.pdf: the last shown, dragged to the left edge.
		await (await browser.$('.doc-tab*=side-one.pdf')).click();
		await waitForDocument(browser);
		await (await browser.$('.doc-tab*=side-three.pdf')).click();
		await waitForDocument(browser);
		const from = await browser.execute((s) => {
			const r = [...document.querySelectorAll(s)].find((t) => t.textContent.includes('side-three.pdf')).getBoundingClientRect();
			return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
		}, TABS);
		const to = await browser.execute(() => {
			const r = document.querySelector('.viewer-scroll').getBoundingClientRect();
			return { x: Math.round(r.left + 40), y: Math.round(r.top + r.height / 2) };
		});
		await drag(browser, from, to, {
			atTarget: async () => {
				// The tab on screen stays there meanwhile.
				assert.ok((await browser.$('[role=tab][aria-selected="true"]').getText()).includes('side-three.pdf'), 'another tab came on screen during the drag');
			}
		});
		await browser.waitUntil(async () => (await panes(browser)).length === 2, { timeoutMsg: 'the dragged tab did not go beside the one shown before' });
		assert.deepEqual(
			(await panes(browser)).map((p) => p.name),
			['side-three.pdf', 'side-one.pdf']
		);
		// One tab, in the place of the one it went beside.
		assert.deepEqual(await tabNames(browser), ['side-three.pdf side-one.pdf', 'side-two.pdf']);
	} finally {
		await stop();
	}
});
