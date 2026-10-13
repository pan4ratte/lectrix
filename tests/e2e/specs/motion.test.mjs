// Motion (AGENTS.md section 8): a notification grows into view where it belongs and shrinks
// away there, while the others make way, and has a slight shadow; a dialog rises into place
// as it fades in, and goes the same way back; a progress bar is seen to be at work. (The
// Debug menu shows each on demand.)

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch } from '../lib/app.mjs';

/** Picks Debug > `submenu` > `item`. */
async function debugMenu(browser, submenu, item) {
	await (await browser.$('button=Debug')).click();
	await (await browser.$(`//*[@role="menuitem"][contains(., "${submenu}")]`)).click();
	await (await browser.$(`//*[@role="menuitem"][normalize-space(.)="${item}"]`)).click();
}

/** Notes, on every frame for `ms`, how tall each notification's place is, where the first
 * one's lower edge is, and where the last notification's middle is and how wide it shows.
 * `watched` gives what was seen. */
async function watch(browser, ms) {
	await browser.execute((length) => {
		const seen = { heights: [], bottoms: [], middles: [], widths: [] };
		window.__seen = seen;
		const started = performance.now();
		const frame = () => {
			const slots = [...document.querySelectorAll('.notice-slot')];
			seen.heights.push(slots.map((s) => Math.round(s.getBoundingClientRect().height)));
			seen.bottoms.push(slots.length ? Math.round(slots[0].getBoundingClientRect().bottom) : null);
			const last = slots[slots.length - 1]?.querySelector('.notice').getBoundingClientRect();
			seen.middles.push(last ? Math.round((last.top + last.bottom) / 2) : null);
			seen.widths.push(last ? Math.round(last.width) : null);
			if (performance.now() - started < length) requestAnimationFrame(frame);
			else seen.done = true;
		};
		requestAnimationFrame(frame);
	}, ms);
}

async function watched(browser) {
	await browser.waitUntil(() => browser.execute(() => window.__seen?.done === true), { timeout: 5000, timeoutMsg: 'the frames were not noted' });
	return browser.execute(() => window.__seen);
}

/** The values of `list` without repeats that follow each other. */
const steps = (list) => list.filter((v, i) => i === 0 || v !== list[i - 1]);

const notices = (browser) => browser.execute(() => document.querySelectorAll('.notice-slot').length);

/** The frames of `seen` on which `count` notifications had a place. */
const framesWith = (seen, count, list) => list.filter((_, i) => seen.heights[i].length === count);

/** The notification stayed where it belongs (its middle within 2 px of `middle`) and only
 * changed size: its widths go one way, `direction` 1 for growing and -1 for shrinking. */
function assertZoomed(middles, widths, middle, direction, what) {
	const off = middles.filter((m) => Math.abs(m - middle) > 2);
	assert.deepEqual(off, [], `${what} moved on the way: ${JSON.stringify(steps(middles))}, at rest ${middle}`);
	const sizes = steps(widths);
	assert.ok(sizes.length >= 4, `${what} did not change size gradually: ${JSON.stringify(sizes)}`);
	assert.deepEqual(sizes, [...sizes].sort((a, b) => direction * (a - b)), `${what} did not only ${direction > 0 ? 'grow' : 'shrink'}`);
}

test('a notification grows into view and shrinks away where it belongs, the others make way, and it has a shadow', async () => {
	const { browser, stop } = await launch([], { env: { LECTRIX_DEBUG: '1' } });
	try {
		await (await browser.$('button=Debug')).waitForClickable({ timeout: 15000 });

		// The first one: its place opens over several frames, from nothing to its height.
		await watch(browser, 1200);
		await debugMenu(browser, 'Notification', 'Information');
		const coming = await watched(browser);
		const heights = steps(coming.heights.filter((h) => h.length === 1).map((h) => h[0]));
		const full = heights[heights.length - 1];
		assert.ok(full > 40, `the notification has no height: ${JSON.stringify(heights)}`);
		assert.ok(heights.length >= 4, `its place did not open gradually: ${JSON.stringify(heights)}`);
		assert.deepEqual(heights, [...heights].sort((a, b) => a - b), 'its place did not only grow');
		const look = await browser.execute(() => {
			const notice = document.querySelector('.notice-slot .notice');
			const slot = getComputedStyle(notice.parentElement);
			const r = notice.getBoundingClientRect();
			return { shadow: getComputedStyle(notice).boxShadow, opacity: slot.opacity, scale: slot.scale, middle: Math.round((r.top + r.bottom) / 2) };
		});
		assert.notEqual(look.shadow, 'none', 'the notification has no shadow');
		assert.deepEqual([look.opacity, look.scale], ['1', 'none'], 'the notification did not come to rest');
		// It does not slide in: it is where it belongs from the first frame, and grows there.
		assertZoomed(framesWith(coming, 1, coming.middles), framesWith(coming, 1, coming.widths), look.middle, 1, 'the notification');

		// A second one under it: the first makes way over several frames.
		await watch(browser, 1200);
		await debugMenu(browser, 'Notification', 'Error');
		const second = await watched(browser);
		assert.equal(await notices(browser), 2);
		const under = await browser.execute(() => {
			const r = document.querySelector('.notice-slot:last-child .notice').getBoundingClientRect();
			return Math.round((r.top + r.bottom) / 2);
		});
		assertZoomed(framesWith(second, 2, second.middles), framesWith(second, 2, second.widths), under, 1, 'the second notification');
		const rising = steps(second.bottoms.filter((b) => b !== null));
		assert.ok(rising.length >= 4, `the first notification jumped out of the way: ${JSON.stringify(rising)}`);
		assert.deepEqual(rising, [...rising].sort((a, b) => b - a), 'the first notification did not only rise');
		await browser.saveScreenshot(join(OUT, 'motion-notifications.png'));

		// The second one dismissed: its place closes over several frames, and the first comes
		// back down with it.
		await watch(browser, 1200);
		await (await browser.$('.notice-slot:last-child button[aria-label="Dismiss"]')).click();
		const going = await watched(browser);
		assert.equal(await notices(browser), 1);
		const closing = steps(going.heights.filter((h) => h.length === 2).map((h) => h[1]));
		assert.ok(closing.length >= 4, `its place did not close gradually: ${JSON.stringify(closing)}`);
		assert.deepEqual(closing, [...closing].sort((a, b) => b - a), 'its place did not only shrink');
		assertZoomed(framesWith(going, 2, going.middles), framesWith(going, 2, going.widths), under, -1, 'the notification dismissed');
		const sinking = steps(going.bottoms.filter((b) => b !== null));
		assert.equal(sinking[sinking.length - 1], rising[0], 'the first notification did not come back to its place');

		// The update notice, above them, comes the same way.
		await watch(browser, 1200);
		await debugMenu(browser, 'Update notice', 'Update available');
		const notice = await watched(browser);
		const opening = steps(notice.heights.filter((h) => h.length === 2).map((h) => h[0]));
		assert.ok(opening.length >= 4, `the update notice’s place did not open gradually: ${JSON.stringify(opening)}`);
		assert.notEqual(
			await browser.execute(() => getComputedStyle(document.querySelector('#update-title').closest('.notice')).boxShadow),
			'none',
			'the update notice has no shadow'
		);
	} finally {
		await stop();
	}
});

/** Notes, on every frame for `ms`, how far in the dialog and the dimming behind it are
 * (their opacity), and the animations they have had. */
async function watchDialog(browser, ms) {
	await browser.execute((length) => {
		const seen = { dialog: [], dim: [], names: [] };
		window.__seen = seen;
		const started = performance.now();
		const frame = () => {
			const dialog = document.querySelector('.dialog-content');
			const dim = document.querySelector('.dialog-overlay');
			if (dialog && dim) {
				seen.dialog.push(Number(getComputedStyle(dialog).opacity));
				seen.dim.push(Number(getComputedStyle(dim).opacity));
				for (const name of [getComputedStyle(dialog).animationName, getComputedStyle(dim).animationName]) {
					if (!seen.names.includes(name)) seen.names.push(name);
				}
			}
			if (performance.now() - started < length) requestAnimationFrame(frame);
			else seen.done = true;
		};
		requestAnimationFrame(frame);
	}, ms);
}

test('a dialog rises into place as it fades in, over the window as it dims, and goes the same way', async () => {
	const { browser, stop } = await launch([], { env: { LECTRIX_DEBUG: '1' } });
	try {
		await (await browser.$('button=Help')).waitForClickable({ timeout: 15000 });
		await watchDialog(browser, 1500);
		await (await browser.$('button=Help')).click();
		await (await browser.$('//*[@role="menuitem"][starts-with(normalize-space(.), "About")]')).click();
		const coming = await watched(browser);
		assert.deepEqual(coming.names, ['dialog-in', 'dialog-dim']);
		for (const [what, values] of [
			['the dialog', coming.dialog],
			['the dimming', coming.dim]
		]) {
			const shown = steps(values);
			assert.ok(shown.length >= 4 && shown[0] < 0.5, `${what} did not fade in: ${JSON.stringify(shown)}`);
			assert.equal(shown[shown.length - 1], 1, `${what} did not come to rest`);
		}
		// At rest it is in the middle of the window.
		const place = await browser.execute(() => {
			const r = document.querySelector('.dialog-content').getBoundingClientRect();
			return { x: (r.left + r.right) / 2 - window.innerWidth / 2, y: (r.top + r.bottom) / 2 - window.innerHeight / 2 };
		});
		assert.ok(Math.abs(place.x) <= 1 && Math.abs(place.y) <= 1, `the dialog is not centered: ${JSON.stringify(place)}`);

		// Closed, it fades out over several frames, and is then gone.
		await watchDialog(browser, 1200);
		await browser.keys('Escape');
		const going = await watched(browser);
		assert.deepEqual(going.names.slice(-2), ['dialog-out', 'dialog-undim']);
		for (const [what, values] of [
			['the dialog', going.dialog],
			['the dimming', going.dim]
		]) {
			const shown = steps(values);
			assert.ok(shown.length >= 4, `${what} did not fade out: ${JSON.stringify(shown)}`);
			assert.deepEqual(shown, [...shown].sort((a, b) => b - a), `${what} did not only fade`);
			assert.ok(shown[shown.length - 1] < 0.5, `${what} went before it had faded: ${JSON.stringify(shown)}`);
		}
		assert.equal(await (await browser.$('.dialog-content')).isExisting(), false, 'the dialog stayed');

		// A question keeps its words and its buttons while it goes.
		await (await browser.$('button=Debug')).click();
		await (await browser.$('//*[@role="menuitem"][contains(., "Crash recovery")]')).click();
		await (await browser.$('//*[@role="menuitem"][normalize-space(.)="One document"]')).click();
		const question = await browser.$('[role=alertdialog]');
		await question.waitForDisplayed({ timeoutMsg: 'no question' });
		const asked = await browser.execute(() => {
			const dialog = document.querySelector('[role=alertdialog]');
			window.__texts = [];
			const started = performance.now();
			const frame = () => {
				const now = document.querySelector('[role=alertdialog]');
				if (now) window.__texts.push(now.textContent.trim());
				if (performance.now() - started < 1000) requestAnimationFrame(frame);
			};
			requestAnimationFrame(frame);
			return dialog.textContent.trim();
		});
		assert.ok(asked.length > 20, `the question says nothing: ${asked}`);
		await browser.keys('Escape');
		await question.waitForExist({ reverse: true, timeout: 2000, timeoutMsg: 'the question stayed' });
		const texts = await browser.execute(() => [...new Set(window.__texts)]);
		assert.deepEqual(texts, [asked], 'the question changed while it went');
	} finally {
		await stop();
	}
});

test('a progress bar is at work: a part of it runs while nothing is known, and the filled part glides with a sheen', async () => {
	const { browser, stop } = await launch([], { env: { LECTRIX_DEBUG: '1' } });
	try {
		await (await browser.$('button=Debug')).waitForClickable({ timeout: 15000 });
		await debugMenu(browser, 'Update notice', 'Checking for updates');
		const running = await browser.$('[role=progressbar] .progress-indeterminate');
		await running.waitForExist({ timeoutMsg: 'no bar while checking' });
		const places = [];
		for (let i = 0; i < 5; i++) {
			places.push(await browser.execute(() => Math.round(document.querySelector('.progress-indeterminate').getBoundingClientRect().left)));
			await browser.pause(120);
		}
		assert.ok(new Set(places).size >= 3, `the bar stands still while checking: ${JSON.stringify(places)}`);

		await debugMenu(browser, 'Update notice', 'Downloading');
		const fill = await browser.$('[role=progressbar] .progress-fill');
		await fill.waitForExist({ timeoutMsg: 'no filled part while downloading' });
		const bar = await browser.execute(() => {
			const filled = document.querySelector('.progress-fill');
			const style = getComputedStyle(filled);
			return { glide: `${style.transitionProperty} ${style.transitionDuration}`, sheen: getComputedStyle(filled, '::after').animationName };
		});
		assert.equal(bar.glide, 'width 0.14s');
		assert.equal(bar.sheen, 'progress-sheen');
		// The filled part grows, and the bar says how far it is.
		const widths = [];
		for (let i = 0; i < 6; i++) {
			widths.push(await browser.execute(() => document.querySelector('.progress-fill')?.getBoundingClientRect().width ?? -1));
			await browser.pause(150);
		}
		const grown = widths.filter((w) => w >= 0);
		assert.ok(new Set(grown).size >= 3, `the filled part does not grow: ${JSON.stringify(widths)}`);
		assert.deepEqual(grown, [...grown].sort((a, b) => a - b));
		await browser.saveScreenshot(join(OUT, 'motion-progress.png'));
	} finally {
		await stop();
	}
});
