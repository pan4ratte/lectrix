// Going to the next or previous page with the view bar's buttons, and to a bookmark's
// place (AGENTS.md sections 6.1 and 6.2): the view glides there, a row shown alone comes
// in, and Settings > Viewing > Motion turns each off by itself.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, statusText, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const pageBox = (browser) => browser.execute(() => document.querySelector('[data-page-box]').value);

async function waitForPageBox(browser, value, why) {
	await browser.waitUntil(async () => (await pageBox(browser)) === value, {
		timeoutMsg: `${why}: the page box shows ${await pageBox(browser)}, not ${value}`
	});
}

/**
 * What `act` makes the view do: its scroll position frame by frame, from before `act` until
 * a moment after, and the positions passed on the way (neither where it was nor where it
 * ended).
 */
async function watch(browser, act) {
	await browser.execute(() => {
		const scroller = document.querySelector('.viewer-scroll');
		const seen = [scroller.scrollTop];
		window.__glide = { seen, done: false };
		const start = performance.now();
		const step = () => {
			seen.push(scroller.scrollTop);
			if (performance.now() - start < 1500) requestAnimationFrame(step);
			else window.__glide.done = true;
		};
		requestAnimationFrame(step);
	});
	await act();
	await browser.waitUntil(() => browser.execute(() => window.__glide.done), { timeoutMsg: 'the view was not watched to the end' });
	const { seen, reduced } = await browser.execute(() => ({
		seen: window.__glide.seen,
		reduced: matchMedia('(prefers-reduced-motion: reduce)').matches
	}));
	const from = seen[0];
	const to = seen.at(-1);
	return { from, to, between: seen.filter((top) => top !== from && top !== to), reduced };
}

/** Turns a switch of Settings > Viewing on or off. */
async function setMotion(browser, id, on) {
	await browser.keys(['Control', ',']);
	await (await browser.$('button[role="tab"]*=Viewing')).click();
	const toggle = await browser.$(`#setting-${id}`);
	await toggle.waitForExist({ timeoutMsg: `Settings has no ${id}` });
	if (((await toggle.getAttribute('aria-checked')) === 'true') !== on) await toggle.click();
	await browser.waitUntil(async () => ((await toggle.getAttribute('aria-checked')) === 'true') === on, {
		timeoutMsg: `${id} did not turn ${on ? 'on' : 'off'}`
	});
	await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
	await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
		timeoutMsg: 'Settings did not close'
	});
}

async function goToPage(browser, page) {
	await browser.keys(['Control', 'g']);
	await browser.keys([...String(page), 'Enter']);
	await browser.waitUntil(async () => (await statusText(browser)).includes(`${page} of`), {
		timeoutMsg: `did not reach page ${page}`
	});
}

/** Adds a bookmark for the current view, named after its page. */
async function addBookmark(browser) {
	await browser.keys(['Control', 'b']);
	const input = await browser.$('input.tree-rename');
	await input.waitForDisplayed({ timeout: 5000 });
	await browser.keys('Enter');
	await input.waitForExist({ reverse: true });
}

const button = (browser, label) => browser.$(`button[aria-label="${label}"]`);

test('the page buttons and bookmarks glide to their page, unless Settings turn that off', async () => {
	const path = sample('page-glide.pdf', 9);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);

		// Next page: the view passes through the positions between the two pages.
		let glide = await watch(browser, async () => (await button(browser, 'Next page')).click());
		await waitForPageBox(browser, '2', 'Next page');
		assert.ok(glide.to > glide.from, `Next page did not scroll down: ${glide.from} to ${glide.to}`);
		if (!glide.reduced) assert.ok(glide.between.length > 0, 'Next page jumped instead of gliding');

		// Pressed again while it glides, it goes on from the page it was heading to.
		await browser.execute(() => {
			const next = document.querySelector('button[aria-label="Next page"]');
			next.click();
			next.click();
		});
		await waitForPageBox(browser, '4', 'Next page twice');

		glide = await watch(browser, async () => (await button(browser, 'Previous page')).click());
		await waitForPageBox(browser, '3', 'Previous page');
		assert.ok(glide.to < glide.from, `Previous page did not scroll up: ${glide.from} to ${glide.to}`);
		if (!glide.reduced) assert.ok(glide.between.length > 0, 'Previous page jumped instead of gliding');

		// Turned off in Settings, the buttons go there at once.
		await setMotion(browser, 'smoothPageTurn', false);
		glide = await watch(browser, async () => (await button(browser, 'Next page')).click());
		await waitForPageBox(browser, '4', 'Next page without gliding');
		assert.deepEqual(glide.between, [], 'Next page still glides with the setting off');
		// Pressed twice at once, they go two pages without a glide too (as with the system's
		// reduced motion, which CI's runners have).
		await browser.execute(() => {
			const next = document.querySelector('button[aria-label="Next page"]');
			next.click();
			next.click();
		});
		await waitForPageBox(browser, '6', 'Next page twice without gliding');

		// Bookmarks at pages 1 and 7. Following one glides, the last screen of a long way.
		await (await button(browser, 'Bookmarks')).click();
		await goToPage(browser, 1);
		await addBookmark(browser);
		await goToPage(browser, 7);
		await addBookmark(browser);
		const row = (title) => browser.$(`.tree-row*=${title}`);
		glide = await watch(browser, async () => (await row('Page 1')).click());
		await waitForPageBox(browser, '1', 'the bookmark of page 1');
		assert.ok(glide.to < glide.from, `the bookmark did not scroll up: ${glide.from} to ${glide.to}`);
		if (!glide.reduced) {
			assert.ok(glide.between.length > 0, 'the bookmark was jumped to instead of glided to');
			const height = await browser.execute(() => document.querySelector('.viewer-scroll').clientHeight);
			const furthest = Math.max(...glide.between);
			assert.ok(furthest <= glide.to + height + 1, `the glide started ${furthest - glide.to} px away, more than a screen (${height})`);
		}
		// Alt+Left returns, as after any jump.
		await browser.execute(() => document.querySelector('.viewer-scroll').focus());
		await browser.keys(['Alt', 'ArrowLeft']);
		await waitForPageBox(browser, '7', 'Alt+Left after a bookmark');

		// Its own switch turns that off, and leaves the page buttons as they are (off).
		await setMotion(browser, 'smoothBookmarkScroll', false);
		glide = await watch(browser, async () => (await row('Page 1')).click());
		await waitForPageBox(browser, '1', 'the bookmark without gliding');
		assert.deepEqual(glide.between, [], 'the bookmark is still glided to with the setting off');
		await setMotion(browser, 'smoothPageTurn', true);
		glide = await watch(browser, async () => (await button(browser, 'Next page')).click());
		await waitForPageBox(browser, '2', 'Next page gliding again');
		if (!glide.reduced) assert.ok(glide.between.length > 0, 'Next page does not glide with its setting back on');

		// One row at a time: there is nothing to glide over, so the row turned to comes in.
		await (await browser.$('[data-page-display]')).click();
		for (const item of await browser.$$('[role="menuitemradio"]')) {
			if ((await item.getText()).replace('✓', '').trim() === 'Single page') {
				await item.click();
				break;
			}
		}
		await browser.waitUntil(async () => (await browser.$$('.page[data-page]')).length === 1, {
			timeoutMsg: 'single page did not show one page alone'
		});
		const entering = await browser.executeAsync((done) => {
			document.querySelector('button[aria-label="Next page"]').click();
			const layer = document.querySelector('.viewer-scroll > div');
			const start = performance.now();
			const step = () => {
				if (layer.getAnimations().length > 0) done(true);
				else if (performance.now() - start > 400) done(false);
				else requestAnimationFrame(step);
			};
			requestAnimationFrame(step);
		});
		await waitForPageBox(browser, '3', 'Next page in single page');
		if (!glide.reduced) assert.ok(entering, 'the next page did not come in animated');
	} finally {
		await stop();
	}
});
