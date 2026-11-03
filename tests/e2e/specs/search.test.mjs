// Find in document (AGENTS.md section 6.1): the bar comes and goes animated, Ctrl+F takes
// the selected text, the field suggests the words that contain what is typed with their
// counts, marks beside the scrollbar show where the matches are, and going to the next
// match glides there; Settings turn the marks and the glide off.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, statusText, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const PAGES = 9;

/** Converts page points of page `index` (view space at zoom 1) to viewport pixels. */
async function pagePoint(browser, index, x, y) {
	const r = await browser.execute(
		(i) => document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect().toJSON(),
		index
	);
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

const field = (browser) => browser.$('[data-search-input]');

/** What the search bar says: "3 of 9", "No results"... */
const searchStatus = (browser) =>
	browser.execute(() => document.querySelector('[role="search"] [aria-live]')?.textContent.trim() ?? '');

async function waitForStatus(browser, text, why) {
	await browser.waitUntil(async () => (await searchStatus(browser)) === text, {
		timeoutMsg: `${why}: the search bar says "${await searchStatus(browser)}", not "${text}"`
	});
}

/** The suggestions listed under the field: each word and its count. */
const suggestions = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[data-search-suggestions] [role="option"]')].map((o) => {
			const [word, count] = o.querySelectorAll(':scope > span');
			return `${word.textContent} ${count.textContent}`;
		})
	);

/** The scroll position frame by frame while `act` runs and a moment after, and the positions
 * passed on the way. */
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
	const seen = await browser.execute(() => window.__glide.seen);
	const from = seen[0];
	const to = seen.at(-1);
	return { from, to, between: seen.filter((top) => top !== from && top !== to) };
}

/** Turns a switch of a Settings group on or off. */
async function setSwitch(browser, group, id, on) {
	await browser.keys(['Control', ',']);
	await (await browser.$(`button[role="tab"]*=${group}`)).click();
	const toggle = await browser.$(`#setting-${id}`);
	await toggle.waitForExist({ timeoutMsg: `Settings has no ${id}` });
	if (((await toggle.getAttribute('aria-checked')) === 'true') !== on) await toggle.click();
	await browser.waitUntil(async () => ((await toggle.getAttribute('aria-checked')) === 'true') === on, {
		timeoutMsg: `${id} did not turn ${on ? 'on' : 'off'}`
	});
	await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
	// (Bits UI gives the page its pointer events back a moment after the dialog has gone; until
	// then WebDriver finds the search field not interactable.)
	await browser.waitUntil(
		() => browser.execute(() => document.querySelector('[role="dialog"]') === null && document.body.style.pointerEvents !== 'none'),
		{ timeoutMsg: 'Settings did not close' }
	);
}

test('the search bar suggests words, marks its matches and glides to them', async () => {
	const path = sample('search.pdf', PAGES);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const reduced = await browser.execute(() => matchMedia('(prefers-reduced-motion: reduce)').matches);

		// Page 3's "brown" (the marker line, 96 pt from the top), double-clicked.
		await browser.keys(['Control', 'g']);
		await browser.keys(['3', 'Enter']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('3 of'), { timeoutMsg: 'did not reach page 3' });
		const brown = await pagePoint(browser, 2, 106, 92);
		await browser.action('pointer').move({ ...brown, origin: 'viewport' }).down().up().pause(40).down().up().perform();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.page rect.fill-selection') !== null), {
			timeoutMsg: 'the double-click selected nothing'
		});

		// Ctrl+F: the bar comes in animated, with the selected word, and starts at it.
		await browser.execute(() => {
			window.__barAnimated = null;
			new MutationObserver((_, observer) => {
				const bar = document.querySelector('[role="search"]');
				if (!bar) return;
				observer.disconnect();
				// Svelte starts the intro once the bar is in place.
				requestAnimationFrame(() => (window.__barAnimated = bar.getAnimations().length > 0));
			}).observe(document.body, { childList: true, subtree: true });
		});
		await browser.keys(['Control', 'f']);
		await (await field(browser)).waitForDisplayed({ timeoutMsg: 'Ctrl+F did not open the search bar' });
		assert.equal(await (await field(browser)).getValue(), 'brown', 'the field did not take the selected word');
		await waitForStatus(browser, `3 of ${PAGES}`, 'the search of the selected word');
		if (!reduced) assert.equal(await browser.execute(() => window.__barAnimated), true, 'the search bar did not come in animated');

		// A mark beside the scrollbar for each match, one of them the match shown.
		await browser.waitUntil(
			() => browser.execute(() => document.querySelectorAll('[data-search-marks] > div').length >= 9),
			{ timeoutMsg: 'the matches were not marked beside the scrollbar' }
		);
		assert.equal(await browser.execute(() => document.querySelectorAll('[data-search-mark-current]').length), 1);

		// Next match: the view glides to page 4's.
		const glide = await watch(browser, () => browser.keys('Enter'));
		await waitForStatus(browser, `4 of ${PAGES}`, 'Enter');
		assert.ok(glide.to > glide.from, `the next match did not scroll down: ${glide.from} to ${glide.to}`);
		if (!reduced) assert.ok(glide.between.length > 0, 'the next match was jumped to instead of glided to');

		// Typed: the words that contain it, each with its matches. Only the marker line has
		// "ow", once a page.
		await (await field(browser)).setValue('ow');
		await browser.waitUntil(async () => (await suggestions(browser)).length > 0, { timeoutMsg: 'no suggestions for "ow"' });
		assert.deepEqual(await suggestions(browser), [`brown ${PAGES}`]);
		// The filler text has several words with "volup", the most matches first.
		await (await field(browser)).setValue('volup');
		await browser.waitUntil(async () => (await suggestions(browser)).length > 1, { timeoutMsg: 'too few suggestions for "volup"' });
		const listed = (await suggestions(browser)).map((s) => {
			const [, word, count] = /^(\S+) (\d+)$/.exec(s);
			return { word, count: Number(count) };
		});
		for (const { word } of listed) assert.ok(word.toLowerCase().includes('volup'), `"${word}" was suggested for "volup"`);
		for (let i = 1; i < listed.length; i++) assert.ok(listed[i].count <= listed[i - 1].count, `not most matches first: ${JSON.stringify(listed)}`);
		// Down and Enter search for one, and the search finds as many matches as it said.
		await browser.keys(['ArrowDown', 'ArrowDown', 'Enter']);
		const picked = listed[1];
		await browser.waitUntil(async () => (await (await field(browser)).getValue()) === picked.word, {
			timeoutMsg: 'the suggestion picked with the keys was not searched for'
		});
		await browser.waitUntil(async () => /^\d+ of \d+$/.test(await searchStatus(browser)), { timeoutMsg: `"${picked.word}" was not searched for` });
		assert.ok((await searchStatus(browser)).endsWith(` of ${picked.count}`), `"${picked.word}" (${picked.count}) found ${await searchStatus(browser)}`);
		assert.equal(await browser.execute(() => document.querySelector('[data-search-suggestions] [role="option"]')), null, 'the list stayed open');

		// Settings: no marks, and no glide.
		await setSwitch(browser, 'Viewing', 'searchMarkers', false);
		assert.equal(await browser.execute(() => document.querySelector('[data-search-marks]')), null, 'the marks stayed with the setting off');
		await setSwitch(browser, 'Viewing', 'smoothSearchScroll', false);
		// Typed, a search moves nothing: the bar counts the matches, and Enter goes to one.
		// (One match a page, so the next one is off screen.)
		const scrollTop = () => browser.execute(() => document.querySelector('.viewer-scroll').scrollTop);
		const resting = await scrollTop();
		await (await field(browser)).setValue('brown');
		await waitForStatus(browser, String(PAGES), 'typing "brown"');
		await browser.pause(300);
		assert.equal(await scrollTop(), resting, 'typing a search moved the view');
		await browser.keys('Enter');
		await browser.waitUntil(async () => /^\d+ of 9$/.test(await searchStatus(browser)), { timeoutMsg: 'Enter did not go to a match' });
		const jump = await watch(browser, () => browser.keys('Enter'));
		assert.ok(jump.to !== jump.from, 'the next match did not scroll');
		assert.deepEqual(jump.between, [], 'the next match is still glided to with the setting off');

		// Esc closes the bar, and the field keeps its text: opened again, the bar searches it
		// again, leaving the view where it is. (A second Esc lets go of the selected word.)
		await browser.keys('Escape');
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="search"]') === null), {
			timeoutMsg: 'Esc did not close the search bar'
		});
		await browser.keys('Escape');
		const closed = await scrollTop();
		await browser.keys(['Control', 'f']);
		await (await field(browser)).waitForDisplayed({ timeoutMsg: 'Ctrl+F did not open the search bar again' });
		assert.equal(await (await field(browser)).getValue(), 'brown', 'the field did not keep its text');
		await waitForStatus(browser, String(PAGES), 'the search bar opened again');
		assert.equal(await scrollTop(), closed, 'opening the search bar again moved the view');
		await browser.keys('Enter');
		await browser.waitUntil(async () => /^\d+ of 9$/.test(await searchStatus(browser)), { timeoutMsg: 'Enter after opening again did not go to a match' });
	} finally {
		await stop();
	}
});

test('the counter takes a match number, and can follow the view', async () => {
	const path = sample('search-follow.pdf', PAGES);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await browser.keys(['Control', 'f']);
		await (await field(browser)).waitForDisplayed({ timeoutMsg: 'Ctrl+F did not open the search bar' });
		await (await field(browser)).setValue('brown');
		await waitForStatus(browser, String(PAGES), 'typing "brown"');

		// A number typed in the counter goes to that match; one past the last is refused.
		const counter = await browser.$('[data-search-counter]');
		await counter.click();
		await browser.keys(['7', 'Enter']);
		await waitForStatus(browser, `7 of ${PAGES}`, 'match 7 typed in the counter');
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.page[data-page="6"] .fill-search-current') !== null), {
			timeoutMsg: "the view did not go to page 7's match"
		});
		assert.equal(await browser.execute(() => document.activeElement?.matches('[data-search-input]')), true, 'the focus did not go back to the field');
		await counter.click();
		await browser.keys(['1', '2', 'Enter']);
		await browser.waitUntil(async () => (await counter.getAttribute('aria-invalid')) === 'true', { timeoutMsg: 'match 12 of 9 was not refused' });
		assert.equal(await searchStatus(browser), `7 of ${PAGES}`);
		await browser.keys('Escape');
		assert.equal(await counter.getValue(), '7', 'Esc did not put the number back');
		assert.equal(await browser.execute(() => document.querySelector('[role="search"]') !== null), true, 'Esc in the counter closed the bar');

		// Static (the default): scrolled away, the counter stays, and next goes on from it.
		const follow = await browser.$('[data-search-follow]');
		assert.equal(await follow.getAttribute('aria-pressed'), 'false');
		/** Scrolls the view so that its top is `share` of the way down page `index`'s box, and
		 * returns once the view has taken the scroll: its scroll event comes with the next
		 * frame, and until then the counter still counts what going to the page showed. */
		const scrollTo = async (index, share) => {
			// Go to page: its page is mounted at the top of the view.
			await (await browser.$('[data-page-box]')).click();
			await browser.keys([...String(index + 1), 'Enter']);
			await browser.waitUntil(() => browser.execute((i) => document.querySelector(`.page[data-page="${i}"]`) !== null, index), {
				timeoutMsg: `page ${index + 1} was not gone to`
			});
			await browser.execute(
				(i, s) => {
					const scroller = document.querySelector('.viewer-scroll');
					const r = document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect();
					scroller.scrollTop += r.top - scroller.getBoundingClientRect().top + r.height * s;
					return new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(null))));
				},
				index,
				share
			);
		};
		// Page 3's top: only its match is on screen (96 pt down a page taller than the view).
		await scrollTo(2, 0);
		await browser.pause(700);
		assert.equal(await searchStatus(browser), `7 of ${PAGES}`, 'the static counter followed the view');

		// Following: the counter counts the match on screen, and next goes on from it.
		await follow.click();
		await browser.waitUntil(async () => (await follow.getAttribute('aria-pressed')) === 'true', { timeoutMsg: 'the switch did not turn on' });
		await waitForStatus(browser, `3 of ${PAGES}`, 'the counter following the view on page 3');
		await (await field(browser)).click();
		await browser.keys('Enter');
		await waitForStatus(browser, `4 of ${PAGES}`, 'next after following to page 3');
		// No match on screen (between page 5's and page 6's): the counter counts page 5's,
		// passed; previous goes to it, next past it.
		await browser.pause(700);
		await scrollTo(4, 0.2);
		await waitForStatus(browser, `5 of ${PAGES}`, 'the counter between two matches');
		assert.equal(await browser.execute(() => document.querySelector('.page .fill-search-current')), null, 'a match passed was shown as the current one');
		await (await field(browser)).click();
		await browser.keys(['Shift', 'Enter']);
		await waitForStatus(browser, `5 of ${PAGES}`, 'previous from between pages 5 and 6');
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.page[data-page="4"] .fill-search-current') !== null), {
			timeoutMsg: "previous did not show page 5's match"
		});
		await browser.pause(700);
		await scrollTo(4, 0.2);
		await waitForStatus(browser, `5 of ${PAGES}`, 'the counter between two matches again');
		await (await field(browser)).click();
		await browser.keys('Enter');
		await waitForStatus(browser, `6 of ${PAGES}`, 'next from between pages 5 and 6');
	} finally {
		await stop();
	}
});
