// Picking a tool glides the active tool's mark to it (AGENTS.md section 8): one mark in the
// annotation toolbar, on the pressed tool, that passes between the two tools on its way
// instead of jumping, and lands on the tool picked. With reduced motion (as on CI's Windows
// runners, whose visual effects are off) it goes there at once instead.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

test('picking a tool glides the mark to it', async () => {
	const path = sample('toolbar-mark.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const toolbar = '[role=toolbar][aria-label="Annotation tools"]';
		const where = () =>
			browser.execute((sel) => {
				const bar = document.querySelector(sel);
				const mark = bar.querySelector('.tool-mark').getBoundingClientRect();
				const at = (label) => bar.querySelector(`[aria-label="${label}"]`).getBoundingClientRect().left;
				return { mark: mark.left, width: mark.width, select: at('Select'), pen: at('Pen') };
			}, toolbar);
		const start = await where();
		assert.ok(start.width > 0, 'the mark is shown');
		assert.ok(Math.abs(start.mark - start.select) < 1, `the mark starts on Select: ${JSON.stringify(start)}`);

		// Pen (P), several tools to the right: watch the mark every frame on its way there.
		const lefts = await browser.executeAsync((sel, done) => {
			const mark = document.querySelector(`${sel} .tool-mark`);
			const seen = [];
			const t0 = performance.now();
			const tick = () => {
				seen.push(mark.getBoundingClientRect().left);
				if (performance.now() - t0 < 400) requestAnimationFrame(tick);
				else done(seen);
			};
			document.querySelector(`${sel} [aria-label="Pen"]`).click();
			requestAnimationFrame(tick);
		}, toolbar);

		const end = await where();
		assert.ok(Math.abs(end.mark - end.pen) < 1, `the mark lands on Pen: ${JSON.stringify(end)}`);
		const between = lefts.filter((x) => x > start.select + 2 && x < end.pen - 2);
		const reduced = await browser.execute(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
		if (reduced) {
			assert.equal(between.length, 0, `reduced motion: the mark went to Pen at once: ${JSON.stringify(lefts)}`);
		} else {
			assert.ok(between.length > 0, `the mark passed between the tools: ${JSON.stringify(lefts)}`);
		}
		const pressed = await browser.execute(
			(sel) => document.querySelector(`${sel} [aria-pressed=true]`).getAttribute('aria-label'),
			toolbar
		);
		assert.equal(pressed, 'Pen');
	} finally {
		await stop();
	}
});
