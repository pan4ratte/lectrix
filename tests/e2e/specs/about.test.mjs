// Branding (AGENTS.md sections 6.6 and 8, ADR 0009): the bundled Google Sans loads under
// the app's CSP and is the UI font, and the start screen and the About dialog show the
// app icon.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch } from '../lib/app.mjs';

/** Natural width and alt text of the first image matching `selector`. */
function image(browser, selector) {
	return browser.execute((s) => {
		const img = document.querySelector(s);
		return { width: img?.naturalWidth ?? 0, alt: img?.getAttribute('alt') };
	}, selector);
}

test('the UI uses the bundled font, and the start screen and About show the icon', async () => {
	const { browser, stop } = await launch();
	try {
		// The icon beside the start screen's heading. Polled rather than held, since the
		// start screen may render again while the app starts.
		const startIcon = 'div:has(> div > h1) > img';
		await browser.waitUntil(async () => (await image(browser, startIcon)).width > 0, {
			timeout: 15000,
			timeoutMsg: 'the start screen shows no icon'
		});
		assert.equal((await image(browser, startIcon)).alt, '', 'the icon is decorative: the heading names the app');
		await browser.saveScreenshot(join(OUT, 'start-screen.png'));

		await (await browser.$('button=Help')).click();
		await (await browser.$('//*[@role="menuitem"][contains(., "About")]')).click();
		await browser.waitUntil(async () => (await image(browser, '[role=dialog] img')).width > 0, {
			timeoutMsg: 'the About dialog shows no icon'
		});
		await browser.waitUntil(() => browser.execute(() => document.fonts.status === 'loaded'));

		const about = await image(browser, '[role=dialog] img');
		const state = await browser.execute(() => {
			const faces = [...document.fonts].filter((f) => f.family.replace(/["']/g, '') === 'Google Sans');
			return {
				bodyFont: getComputedStyle(document.body).fontFamily,
				faces: faces.map((f) => `${f.style} ${f.status}`)
			};
		});
		await browser.saveScreenshot(join(OUT, 'about.png'));

		assert.ok(about.width > 0, 'the About icon loads');
		assert.equal(about.alt, '', 'the icon is decorative: the dialog title names the app');
		assert.match(state.bodyFont, /^["']?Google Sans/, `UI font: ${state.bodyFont}`);
		// The italic face loads only when italic text is shown; the regular one is in use.
		assert.ok(state.faces.includes('normal loaded'), `Google Sans faces: ${state.faces.join(', ')}`);
	} finally {
		await stop();
	}
});
