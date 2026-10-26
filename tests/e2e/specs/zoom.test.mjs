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

test('a zoom typed in the zoom box applies on Enter', async () => {
	const path = sample('zoom-typed.pdf', 3);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const box = await browser.$('[data-zoom-box]');
		const shown = () => box.getValue();
		const expectShown = async (text, message) =>
			browser.waitUntil(async () => (await shown()) === text, { timeoutMsg: `${message}: the box shows ${JSON.stringify(await shown())}` });
		const pageWidth = () => browser.execute(() => document.querySelector('.page[data-page="0"]').getBoundingClientRect().width);

		// A click selects what the box shows, so typing replaces it; Enter zooms there and
		// hands the keyboard back to the pages.
		await box.click();
		await browser.keys([...'137', 'Enter']);
		await expectShown('137%', 'typed 137');
		// (Once the glide has ended.)
		await browser.pause(400);
		const at137 = await pageWidth();
		assert.equal(await browser.execute(() => document.activeElement?.classList.contains('viewer-scroll')), true, 'the pages do not have the focus');

		// What is no percentage is refused, and Esc puts the zoom back.
		await box.click();
		await browser.keys([...'abc', 'Enter']);
		assert.equal(await box.getAttribute('aria-invalid'), 'true', 'text was not refused');
		await browser.keys(['Escape']);
		await expectShown('137%', 'Esc');
		assert.equal(await box.getAttribute('aria-invalid'), 'false');

		// Beyond what there can be, it is the largest zoom; a percent sign may be typed.
		await box.click();
		await browser.keys([...'5000%', 'Enter']);
		await expectShown('1600%', 'typed 5000%');
		await box.click();
		await browser.keys([...'100', 'Enter']);
		await expectShown('100%', 'typed 100');
		await browser.pause(400);
		const ratio = at137 / (await pageWidth());
		assert.ok(Math.abs(ratio - 1.37) < 0.01, `137% is ${ratio} times 100%`);

		// Up in the box zooms in to the next preset, as + does.
		await box.click();
		await browser.keys(['ArrowUp']);
		await expectShown('125%', 'Up in the box');
		await browser.keys(['Escape']);

		// The chevron opens the fits and the presets.
		await (await browser.$('button[aria-label="Zoom options"]')).click();
		const menu = await browser.$('[role=menu]');
		await menu.waitForDisplayed({ timeoutMsg: 'the chevron opened no menu' });
		await (await browser.$('//*[@role="menuitem"][normalize-space(.)="200%"]')).click();
		await expectShown('200%', 'the 200% preset');
	} finally {
		await stop();
	}
});
