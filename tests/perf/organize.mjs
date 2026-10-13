// How long the page organizing grid takes to follow an edit (AGENTS.md section 6.7): a
// page is moved one place with the keyboard, and the grid is watched frame by frame for
// the new order and for the pictures drawn again (each edit gives the document a new
// revision, which is part of what names a page image, so every picture on screen is
// rendered anew; a page keeps its old picture until then).
//
//     node tests/perf/organize.mjs FILE [FILE...]
//
// Needs what the end-to-end tests need (tests/e2e/lib/app.mjs): the release app,
// tauri-driver and msedgedriver. The files are only read.

import { basename } from 'node:path';

import { launch, organizePages, waitForDocument } from '../e2e/lib/app.mjs';

/** Waits until every cell on screen shows a picture, and returns how many there are. */
async function drawn(browser) {
	let count = 0;
	await browser.waitUntil(
		async () => {
			count = await browser.execute(() => {
				const cells = [...document.querySelectorAll('[data-organize-view] [role=option]')];
				return cells.length > 0 && cells.every((c) => c.dataset.drawn) ? cells.length : 0;
			});
			return count > 0;
		},
		{ timeout: 120_000, timeoutMsg: 'the grid was not drawn' }
	);
	return count;
}

/** Watches the grid from the next key press: when its order changes, and when every cell
 * that was on screen shows another picture than before. */
function watch(browser) {
	return browser.execute(() => {
		const cells = () => [...document.querySelectorAll('[data-organize-view] [role=option]')];
		const order = () => cells().map((c) => c.dataset.key + '@' + c.getAttribute('aria-posinset')).join(' ');
		const before = new Map(cells().map((c) => [c.dataset.key, c.dataset.drawn]));
		const startOrder = order();
		const result = { moved: null, redrawn: null };
		window.__organizePerf = result;
		window.addEventListener(
			'keydown',
			() => {
				const t0 = performance.now();
				const tick = () => {
					const now = performance.now() - t0;
					if (result.moved === null && order() !== startOrder) result.moved = now;
					if (result.moved !== null) {
						const pending = cells().filter((c) => before.has(c.dataset.key) && c.dataset.drawn === before.get(c.dataset.key));
						if (pending.length === 0) {
							result.redrawn = now;
							return;
						}
					}
					if (now < 60_000) requestAnimationFrame(tick);
				};
				requestAnimationFrame(tick);
			},
			{ capture: true, once: true }
		);
	});
}

async function measure(file) {
	const { browser, stop } = await launch([file]);
	try {
		await waitForDocument(browser);
		await organizePages(browser);
		const cells = await drawn(browser);
		const pages = await (await browser.$('[data-organize-count]')).getText();
		// The second page on screen, one place later.
		await (await browser.$('[data-organize-view] [role=option]:nth-of-type(2) [data-picture]')).click();
		const runs = [];
		for (let i = 0; i < 3; i++) {
			await drawn(browser);
			await watch(browser);
			await browser.keys(['Alt', 'Shift', 'ArrowRight']);
			await browser.waitUntil(() => browser.execute(() => window.__organizePerf.redrawn !== null), {
				timeout: 120_000,
				timeoutMsg: 'the grid was not drawn again'
			});
			runs.push(await browser.execute(() => window.__organizePerf));
		}
		const ms = (pick) => runs.map((r) => Math.round(pick(r))).join(', ');
		console.log(
			`${basename(file)}: ${pages.trim()}, ${cells} pictures on screen; order follows after ${ms((r) => r.moved)} ms, pictures drawn again after ${ms((r) => r.redrawn)} ms`
		);
	} finally {
		await stop();
	}
}

const files = process.argv.slice(2);
if (files.length === 0) {
	console.error('usage: node tests/perf/organize.mjs FILE [FILE...]');
	process.exit(2);
}
for (const file of files) await measure(file);
