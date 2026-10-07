// Turning a page in the document (AGENTS.md section 6.1): until the new image arrives, the
// page and its thumbnail keep showing the old pixels, turned with the page, never squeezed
// into the page's new shape.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

test('a page turned in the document is never shown squeezed', async () => {
	const path = sample('rotate-page.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const canvases = '.page[data-page="0"] canvas, [data-thumb="0"] canvas';
		await browser.waitUntil(
			() => browser.execute((sel) => [...document.querySelectorAll(sel)].every((c) => c.width > 1), canvases),
			{ timeoutMsg: 'page 1 and its thumbnail were not drawn' }
		);
		// Pages render again once the first fit settles; turn the page after that.
		await browser.pause(800);

		// Every frame from the click until both are drawn again: the pixels' proportions
		// against the box they are laid out in (before it is turned).
		const watch = await browser.executeAsync((sel, done) => {
			const seen = { frames: 0, worst: 1, turned: 0 };
			const t0 = performance.now();
			const tick = () => {
				for (const c of document.querySelectorAll(sel)) {
					if (c.width <= 1 || c.offsetHeight === 0) continue;
					const shown = c.offsetWidth / c.offsetHeight;
					const drawn = c.width / c.height;
					seen.worst = Math.max(seen.worst, shown / drawn, drawn / shown);
					if (c.style.transform) seen.turned++;
				}
				seen.frames++;
				if (performance.now() - t0 < 1500) requestAnimationFrame(tick);
				else done(seen);
			};
			document.querySelector('[aria-label="Rotate current page clockwise"]').click();
			requestAnimationFrame(tick);
		}, canvases);

		assert.ok(watch.frames > 20, `watched ${watch.frames} frames`);
		assert.ok(watch.worst < 1.05, `pixels were squeezed by ${watch.worst.toFixed(2)}x`);
		const final = await browser.execute(
			(sel) => [...document.querySelectorAll(sel)].map((c) => ({ w: c.width, h: c.height, t: c.style.transform })),
			canvases
		);
		for (const c of final) {
			assert.ok(c.w > c.h, `the page is drawn landscape at the end: ${JSON.stringify(final)}`);
			assert.equal(c.t, '', 'and no longer turned by CSS');
		}
	} finally {
		await stop();
	}
});
