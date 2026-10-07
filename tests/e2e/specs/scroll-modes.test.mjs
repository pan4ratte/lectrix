// Scroll modes (AGENTS.md section 6.1): single page, single page continuous, two pages and
// two pages continuous, picked from the view bar; turning rows with the keyboard, the wheel
// and the previous/next buttons; the cover page; and the Settings default for files opened
// afterwards.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

/** The pages in the DOM, with where they are on screen. */
const mounted = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('.page[data-page]')]
			.map((el) => {
				const r = el.getBoundingClientRect();
				return { page: Number(el.getAttribute('data-page')), top: Math.round(r.top), left: Math.round(r.left) };
			})
			.sort((a, b) => a.page - b.page)
	);

const pageBox = (browser) => browser.execute(() => document.querySelector('[data-page-box]').value);

async function waitForPageBox(browser, value, why) {
	await browser.waitUntil(async () => (await pageBox(browser)) === value, {
		timeoutMsg: `${why}: the page box shows ${await pageBox(browser)}, not ${value}`
	});
}

/** Picks a scroll mode, or the cover page, from the view bar's page display menu. */
async function pick(browser, label) {
	await (await browser.$('[data-page-display]')).click();
	await browser.waitUntil(async () => (await browser.$$('[role="menuitemradio"]')).length === 4, {
		timeoutMsg: 'the page display menu did not open'
	});
	for (const item of await browser.$$('[role="menuitemradio"], [role="menuitemcheckbox"]')) {
		if ((await item.getText()).replace('✓', '').trim() === label) {
			await item.click();
			return;
		}
	}
	throw new Error(`no "${label}" in the page display menu`);
}

const focusPages = (browser) => browser.execute(() => document.querySelector('.viewer-scroll').focus());

test('scroll modes lay pages out, and turn a row at a time where one is shown', async () => {
	const path = sample('scroll-modes.pdf', 9);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const label = await (await browser.$('[data-page-display]')).getAttribute('aria-label');
		assert.equal(label, 'Page display: Single page, continuous', 'the default after installation');

		// Two pages: 1 and 2 side by side, nothing else.
		await pick(browser, 'Two pages');
		await browser.waitUntil(async () => (await mounted(browser)).map((p) => p.page).join() === '0,1', {
			timeoutMsg: 'two pages did not show pages 1 and 2 alone'
		});
		let pages = await mounted(browser);
		assert.equal(pages[0].top, pages[1].top, 'pages 1 and 2 are side by side');
		assert.ok(pages[1].left > pages[0].left, 'page 2 is right of page 1');
		await waitForPageBox(browser, '1', 'two pages');

		// Page Down at the end of the row turns to the next.
		await focusPages(browser);
		for (let i = 0; i < 6 && (await pageBox(browser)) !== '3'; i++) {
			await browser.keys(['PageDown']);
			await browser.pause(150);
		}
		await waitForPageBox(browser, '3', 'Page Down');
		assert.deepEqual(
			(await mounted(browser)).map((p) => p.page),
			[2, 3]
		);

		// A wheel notch past the end turns too; one back from the top returns.
		const wheel = (deltaY) =>
			browser.execute((d) => {
				const scroller = document.querySelector('.viewer-scroll');
				scroller.scrollTop = d > 0 ? scroller.scrollHeight : 0;
				scroller.dispatchEvent(new WheelEvent('wheel', { deltaY: d, bubbles: true, cancelable: true }));
			}, deltaY);
		await wheel(120);
		await waitForPageBox(browser, '5', 'a wheel notch down');
		await wheel(-120);
		await waitForPageBox(browser, '3', 'a wheel notch up');

		// The next page button turns a row; End goes to the last, page 9 alone on the left.
		await (await browser.$('button[aria-label="Next page"]')).click();
		await waitForPageBox(browser, '5', 'Next page');
		await focusPages(browser);
		await browser.keys(['End']);
		await waitForPageBox(browser, '9', 'End');
		assert.ok(await (await browser.$('button[aria-label="Next page"]')).getAttribute('disabled') !== null);

		// The cover alone: page 1 has a row of its own, then 2 and 3, and so on.
		await browser.keys(['Home']);
		await waitForPageBox(browser, '1', 'Home');
		await pick(browser, 'Cover page alone');
		await browser.waitUntil(async () => (await mounted(browser)).map((p) => p.page).join() === '0', {
			timeoutMsg: 'the cover did not show alone'
		});
		await focusPages(browser);
		await browser.keys(['ArrowRight']);
		await waitForPageBox(browser, '2', 'Right with the cover alone');
		assert.deepEqual(
			(await mounted(browser)).map((p) => p.page),
			[1, 2]
		);

		// Two pages, continuous: rows go on below, the cover kept.
		await pick(browser, 'Two pages, continuous');
		await browser.waitUntil(async () => (await mounted(browser)).some((p) => p.page === 3), {
			timeoutMsg: 'two pages continuous did not show the row below'
		});
		pages = await mounted(browser);
		const at = (n) => pages.find((p) => p.page === n);
		assert.equal(at(1).top, at(2).top, 'pages 2 and 3 share a row');
		assert.ok(at(3).top > at(1).top, 'pages 4 and 5 follow below');

		// Single page: one page, Left goes back.
		await pick(browser, 'Single page');
		await browser.waitUntil(async () => (await mounted(browser)).length === 1, {
			timeoutMsg: 'single page showed more than one page'
		});
		const before = await pageBox(browser);
		await focusPages(browser);
		await browser.keys(['ArrowLeft']);
		await waitForPageBox(browser, String(Number(before) - 1), 'Left in single page');
	} finally {
		await stop();
	}
});

test('Settings choose how documents opened afterwards are laid out', async () => {
	const first = sample('scroll-modes-first.pdf', 4);
	const second = sample('scroll-modes-second.pdf', 6);
	const { browser, stop } = await launch([first], { dialogs: [second] });
	try {
		await waitForDocument(browser);
		await browser.keys(['Control', ',']);
		await (await browser.$('#setting-scrollMode')).click();
		await (await browser.$('[role="option"]=Two pages, continuous')).click();
		await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Settings did not close'
		});
		// The open document keeps its layout.
		assert.equal(
			await (await browser.$('[data-page-display]')).getAttribute('aria-label'),
			'Page display: Single page, continuous'
		);

		await browser.keys(['Control', 'o']);
		await browser.waitUntil(
			async () =>
				(await (await browser.$('[data-page-display]')).getAttribute('aria-label')) ===
				'Page display: Two pages, continuous',
			{ timeoutMsg: 'the file opened afterwards did not take the Settings choice' }
		);
		await browser.waitUntil(async () => (await mounted(browser)).length >= 4, {
			timeoutMsg: 'the second file did not show its rows'
		});
		const pages = await mounted(browser);
		assert.equal(pages[0].top, pages[1].top, 'pages 1 and 2 share a row');
		assert.ok(pages[2].top > pages[0].top, 'pages 3 and 4 follow below');
	} finally {
		await stop();
	}
});
