// Zooming keeps the pages on screen (AGENTS.md sections 3 and 6.1): in the middle of a
// document, gliding zoom steps and wheel notches must not drop and re-create the page being
// read, which lost its pixels and showed it blank for a moment, every frame of a zoom.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

test('zooming in the middle of a document keeps the page on screen drawn', async () => {
	const path = sample('zoom-middle.pdf', 120);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await browser.keys(['Control', 'g']);
		await browser.keys([...'100', 'Enter']);
		const drawn = () =>
			browser.execute(() => {
				const canvas = document.querySelector('.page[data-page="99"] canvas');
				return canvas !== null && canvas.width > 1;
			});
		await browser.waitUntil(drawn, { timeoutMsg: 'page 100 was not drawn' });

		// Every frame from now on: page 100 is the same element, and has pixels.
		await browser.execute(() => {
			const page = document.querySelector('.page[data-page="99"]');
			const watch = { frames: 0, replaced: 0, blank: 0, running: true };
			window.__zoomWatch = watch;
			const tick = () => {
				const now = document.querySelector('.page[data-page="99"]');
				if (now !== page) watch.replaced++;
				else if (!(now.querySelector('canvas')?.width > 1)) watch.blank++;
				watch.frames++;
				if (watch.running) requestAnimationFrame(tick);
			};
			requestAnimationFrame(tick);
		});

		for (const key of ['-', '-', '=', '=', '=']) {
			await browser.keys(['Control', key]);
			await browser.pause(400);
		}
		const at = await browser.execute(() => {
			const r = document.querySelector('.viewer-scroll').getBoundingClientRect();
			return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
		});
		for (const deltaY of [120, 120, -120, -120]) {
			await browser.execute(
				(x, y, d) =>
					document
						.elementFromPoint(x, y)
						.dispatchEvent(
							new WheelEvent('wheel', { deltaY: d, ctrlKey: true, clientX: x, clientY: y, bubbles: true, cancelable: true })
						),
				at.x,
				at.y,
				deltaY
			);
			await browser.pause(400);
		}

		const watch = await browser.execute(() => {
			window.__zoomWatch.running = false;
			return window.__zoomWatch;
		});
		assert.ok(watch.frames > 30, `watched ${watch.frames} frames`);
		assert.equal(watch.replaced, 0, `page 100 was dropped and re-created in ${watch.replaced} frames`);
		assert.equal(watch.blank, 0, `page 100 showed without pixels in ${watch.blank} frames`);
	} finally {
		await stop();
	}
});
