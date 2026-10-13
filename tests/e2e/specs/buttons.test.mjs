// Coloured buttons (AGENTS.md section 8, accessibility): the primary button, filled in the
// accent, and the destructive one, in the red of errors, keep their text readable under the
// pointer, in the light theme and in the dark one.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch } from '../lib/app.mjs';

/** The colours a button shows, as [r, g, b]: its fill and its text. */
function colours(browser, selector) {
	return browser.execute((sel) => {
		const button = document.querySelector(sel);
		const style = getComputedStyle(button);
		// Whatever way the colour is written (rgb(), color(srgb ...)), a canvas gives its bytes.
		const bytes = (colour) => {
			const context = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
			context.fillStyle = colour;
			context.fillRect(0, 0, 1, 1);
			return [...context.getImageData(0, 0, 1, 1).data].slice(0, 3);
		};
		return { fill: bytes(style.backgroundColor), text: bytes(style.color), hovered: button.matches(':hover') };
	}, selector);
}

/** WCAG contrast of two colours. */
function contrast(a, b) {
	const luminance = (c) => {
		const [r, g, bl] = c.map((v) => (v / 255 <= 0.03928 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4));
		return 0.2126 * r + 0.7152 * g + 0.0722 * bl;
	};
	const [x, y] = [luminance(a), luminance(b)];
	return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

/** At rest and under the pointer, in both themes: the button stays filled in its colour,
 * and its text reads at 4.5:1 or more. */
async function check(browser, selector, what) {
	for (const theme of ['light', 'dark']) {
		await browser.execute((t) => (document.documentElement.dataset.theme = t), theme);
		// Away from it, then on it.
		await (await browser.$('header')).moveTo();
		await browser.pause(250);
		const rest = await colours(browser, selector);
		assert.equal(rest.hovered, false);
		await (await browser.$(selector)).moveTo();
		await browser.pause(250);
		const over = await colours(browser, selector);
		assert.equal(over.hovered, true, `${what}: the pointer is not on the button`);
		const said = `${what}, ${theme}: ${JSON.stringify({ rest, over })}`;
		assert.ok(contrast(rest.fill, rest.text) >= 4.5, `at rest its text does not read. ${said}`);
		assert.ok(contrast(over.fill, over.text) >= 4.5, `under the pointer its text does not read. ${said}`);
		// Under the pointer it is still the coloured button: a shade of the fill it has at rest.
		assert.ok(contrast(over.fill, rest.fill) < 1.6, `under the pointer it loses its colour. ${said}`);
		assert.notDeepEqual(over.fill, rest.fill, `under the pointer it does not change. ${said}`);
	}
}

test('the primary and the destructive button keep their text readable under the pointer, in both themes', async () => {
	const { browser, stop } = await launch([]);
	try {
		// The Combine view, with no files yet, has a primary button in the middle.
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('button.button-primary*=Add files')).waitForDisplayed();
		await check(browser, 'section[aria-label="Combine files"] button.button-primary:not(:disabled)', 'the primary button');

		// Settings > General ends with the destructive Restore defaults….
		await browser.keys(['Control', ',']);
		const restore = await browser.$('button.button-danger');
		await restore.waitForExist({ timeoutMsg: 'Settings has no destructive button' });
		await restore.scrollIntoView();
		await check(browser, 'button.button-danger', 'the destructive button');
	} finally {
		await stop();
	}
});
