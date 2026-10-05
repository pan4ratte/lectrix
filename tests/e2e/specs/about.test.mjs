// Branding (AGENTS.md sections 6.6 and 8, ADR 0009): the bundled Google Sans loads under
// the app's CSP and is the UI font, and the About dialog shows the app icon.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch } from '../lib/app.mjs';

test('the UI uses the bundled font and About shows the icon', async () => {
	const { browser, stop } = await launch();
	try {
		await (await browser.$('button=Help')).click();
		await (await browser.$('//*[@role="menuitem"][contains(., "About")]')).click();
		const icon = await browser.$('[role=dialog] img');
		await icon.waitForDisplayed({ timeoutMsg: 'the About dialog shows no icon' });
		await browser.waitUntil(() => browser.execute(() => document.fonts.status === 'loaded'));

		const state = await browser.execute(() => {
			const img = document.querySelector('[role=dialog] img');
			const faces = [...document.fonts].filter((f) => f.family.replace(/["']/g, '') === 'Google Sans');
			return {
				iconWidth: img?.naturalWidth ?? 0,
				iconAlt: img?.getAttribute('alt'),
				bodyFont: getComputedStyle(document.body).fontFamily,
				faces: faces.map((f) => `${f.style} ${f.status}`)
			};
		});
		await browser.saveScreenshot(join(OUT, 'about.png'));

		assert.ok(state.iconWidth > 0, 'the icon image loads');
		assert.equal(state.iconAlt, '', 'the icon is decorative: the dialog title names the app');
		assert.match(state.bodyFont, /^["']?Google Sans/, `UI font: ${state.bodyFont}`);
		// The italic face loads only when italic text is shown; the regular one is in use.
		assert.ok(state.faces.includes('normal loaded'), `Google Sans faces: ${state.faces.join(', ')}`);
	} finally {
		await stop();
	}
});
