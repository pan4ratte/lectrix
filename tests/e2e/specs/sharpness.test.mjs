// Pages are shown pixel for pixel as Rust rendered them (AGENTS.md section 3): at rest, an
// image of the page's own scale lands one to one on the screen's pixels, unresampled. An
// image a few percent larger, shrunk by the webview, smeared every glyph edge over its
// neighbours and made text softer than other readers show it.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

/**
 * Compares a square in the middle of the viewer, as the screen shows it, with the pixels
 * of the topmost page image under it: how many of the image's inked (not white) pixels
 * there are, and how many pixels differ on screen.
 */
async function compareWithScreen(browser) {
	const region = await browser.execute(() => {
		const dpr = devicePixelRatio;
		const view = document.querySelector('.viewer-scroll').getBoundingClientRect();
		const cx = view.left + view.width / 2;
		const cy = view.top + view.height / 2;
		const contains = (r) => r.left <= cx && r.top <= cy && r.right >= cx && r.bottom >= cy;
		// The topmost image there: tiles come after the image beneath them.
		const canvases = [...document.querySelectorAll('[data-page-pixels] canvas')].filter((c) => c.width > 1);
		const canvas = canvases.findLast((c) => contains(c.getBoundingClientRect()));
		if (!canvas) return null;
		canvas.dataset.compared = '';
		const r = canvas.getBoundingClientRect();
		// A square as near the middle as the image and the viewer both hold it (a tile is
		// smaller than the viewer), in device pixels.
		const x0 = Math.ceil(Math.max(r.left, view.left) * dpr);
		const y0 = Math.ceil(Math.max(r.top, view.top) * dpr);
		const x1 = Math.floor(Math.min(r.right, view.right) * dpr);
		const y1 = Math.floor(Math.min(r.bottom, view.bottom) * dpr);
		const size = Math.min(Math.floor(160 * dpr), x1 - x0, y1 - y0);
		const x = Math.min(Math.max(Math.round(cx * dpr) - Math.floor(size / 2), x0), x1 - size);
		const y = Math.min(Math.max(Math.round(cy * dpr) - Math.floor(size / 2), y0), y1 - size);
		return { x, y, size, left: r.left * dpr, top: r.top * dpr, sx: canvas.width / (r.width * dpr), sy: canvas.height / (r.height * dpr) };
	});
	assert.ok(region, 'no page image covers the middle of the viewer');
	const shot = await browser.takeScreenshot();
	return browser.executeAsync(
		(png, region, done) => {
			const canvas = document.querySelector('canvas[data-compared]');
			delete canvas.dataset.compared;
			const { x, y, size, left, top, sx, sy } = region;
			const rendered = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
			const image = new Image();
			image.onload = () => {
				const screen = new OffscreenCanvas(size, size).getContext('2d');
				screen.drawImage(image, -x, -y);
				const shown = screen.getImageData(0, 0, size, size).data;
				let inked = 0;
				let differ = 0;
				for (let j = 0; j < size; j++) {
					for (let i = 0; i < size; i++) {
						const ci = Math.floor((x + i + 0.5 - left) * sx);
						const cj = Math.floor((y + j + 0.5 - top) * sy);
						const a = (cj * canvas.width + ci) * 4;
						const b = (j * size + i) * 4;
						if (rendered[a] < 250 || rendered[a + 1] < 250 || rendered[a + 2] < 250) inked++;
						const d = Math.max(
							Math.abs(rendered[a] - shown[b]),
							Math.abs(rendered[a + 1] - shown[b + 1]),
							Math.abs(rendered[a + 2] - shown[b + 2])
						);
						if (d > 2) differ++;
					}
				}
				done({ inked, differ, ratio: sx });
			};
			image.onerror = () => done({ error: 'the screenshot could not be decoded' });
			image.src = `data:image/png;base64,${png}`;
		},
		shot,
		region
	);
}

/** Waits for zooming to end and the pages of the new zoom to be drawn: an image shown one
 * to one. Not a failure by itself: the comparison says what the screen shows instead. */
async function settle(browser) {
	await browser.pause(400);
	await browser.waitUntil(
		() =>
			browser.execute(() => {
				const dpr = devicePixelRatio;
				// Every image of the page's own scale is drawn: its canvas has as many pixels
				// as the screen gives it.
				const canvases = [...document.querySelectorAll('[data-page-pixels] canvas')];
				return canvases.some(
					(c) => c.width > 1 && Math.abs(c.width - c.getBoundingClientRect().width * dpr) < 0.01
				);
			}),
		{ timeout: 10_000 }
	).catch(() => {});
	await browser.pause(300);
}

test('pages are shown pixel for pixel as they were rendered', async () => {
	const path = sample('sharpness.pdf', 3);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const cases = [];
		// As the document opens (fit width), one zoom step in, and far enough in for tiles.
		for (const steps of [0, 1, 4]) {
			for (let i = 0; i < steps - (cases.at(-1)?.steps ?? 0); i++) await browser.keys(['Control', '=']);
			await settle(browser);
			const result = await compareWithScreen(browser);
			assert.equal(result.error, undefined, result.error);
			cases.push({ steps, ...result });
		}
		for (const c of cases) {
			assert.ok(c.inked > 100, `${c.steps} zoom steps in: the compared square has no text (${c.inked} inked pixels)`);
			assert.ok(
				c.differ <= c.inked / 100,
				`${c.steps} zoom steps in: ${c.differ} pixels on screen differ from the rendered image ` +
					`(${c.inked} inked; image pixels per screen pixel ${c.ratio.toFixed(4)})`
			);
		}
	} finally {
		await stop();
	}
});
