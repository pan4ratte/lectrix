// Organizing pages (AGENTS.md section 6.7): the document's pages as a grid in place of the
// viewer, without the side panes; a page moved, turned and deleted there, each at once and
// undoable; the question before deleting and what is said after; the bar under the pointer
// with the page's label and buttons; larger and smaller pictures from the bar, the wheel
// and a pinch; back to reading, and the saved file.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, outlineLines, sample } from '../lib/pdfcli.mjs';

/** The pages of the grid in order, by their keys (a page keeps its key wherever it goes). */
async function keys(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[data-organize-view] [data-key][role=option]')]
			.sort((a, b) => a.offsetTop - b.offsetTop || a.offsetLeft - b.offsetLeft)
			.map((c) => Number(c.dataset.key))
	);
}

async function waitForKeys(browser, expected, message) {
	await browser.waitUntil(async () => JSON.stringify(await keys(browser)) === JSON.stringify(expected), {
		timeout: 5000,
		timeoutMsg: `${message}: got ${JSON.stringify(await keys(browser))}, expected ${JSON.stringify(expected)}`
	});
}

async function clickPage(browser, index) {
	await (await browser.$(`[data-organize-view] [role=option][aria-posinset="${index + 1}"]`)).click();
}

const organizing = (browser) => browser.execute(() => document.querySelector('[data-organize-view]') !== null);
const reading = (browser) => browser.execute(() => document.querySelector('.viewer-scroll') !== null);
const bodyText = (browser) => browser.execute(() => document.body.innerText);

const panes = (browser) =>
	browser.execute(() => document.querySelectorAll('[data-pane], button[aria-label$=" pane"]').length);

/** The width of the first page's cell, which follows the size of the pictures. */
const cellWidth = (browser) =>
	browser.execute(() => document.querySelector('[data-organize-view] [role=option]').getBoundingClientRect().width);

/** Ctrl+wheel over the middle of page `index` (a touchpad pinch arrives the same way). */
async function wheelOver(browser, index, deltaY) {
	await browser.execute(
		(position, d) => {
			const cell = document.querySelector(`[data-organize-view] [role=option][aria-posinset="${position}"]`);
			const r = cell.getBoundingClientRect();
			const x = r.left + r.width / 2;
			const y = r.top + r.height / 2;
			cell.dispatchEvent(new WheelEvent('wheel', { deltaY: d, ctrlKey: true, clientX: x, clientY: y, bubbles: true, cancelable: true }));
		},
		index + 1,
		deltaY
	);
}

async function waitForWidth(browser, test, message) {
	let width = 0;
	await browser.waitUntil(
		async () => {
			const first = await cellWidth(browser);
			await browser.pause(200);
			width = await cellWidth(browser);
			// (At rest: a glide is over.)
			return first === width && test(width);
		},
		{ timeout: 5000, timeoutMsg: `${message}: the cell is ${await cellWidth(browser)} px wide` }
	);
	return width;
}

async function startOrganizing(browser) {
	await (await browser.$('[aria-label="Organize pages"]')).click();
	await browser.waitUntil(() => organizing(browser), { timeoutMsg: 'the grid did not show' });
}

test('pages are moved, turned and deleted in the grid, and the document follows', async () => {
	const base = sample('organize-base.pdf', 6);
	const path = join(OUT, 'organize.pdf');
	cli('outline', 'set', base, path, '--item', '0:3:Chapter');

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		assert.ok((await panes(browser)) > 0, 'no side pane while reading');
		await startOrganizing(browser);
		assert.equal(await reading(browser), false, 'the viewer is still there');
		// The side panes are for reading: neither they nor their buttons are here.
		await browser.waitUntil(async () => (await panes(browser)) === 0, { timeoutMsg: 'a side pane is still there' });
		// (The grid lays its cells out once it knows its width.)
		await browser.waitUntil(async () => (await keys(browser)).length === 6, { timeoutMsg: 'not every page is in the grid' });
		const k = await keys(browser);
		assert.match(await (await browser.$('[data-organize-count]')).getText(), /^6 pages$/);

		// The last page, two places earlier; undo takes one step back, redo brings it again.
		await clickPage(browser, 5);
		assert.match(await (await browser.$('[data-organize-count]')).getText(), /^1 of 6 pages selected$/);
		await browser.keys(['Alt', 'Shift', 'ArrowLeft']);
		await browser.keys(['Alt', 'Shift', 'ArrowLeft']);
		const moved = [k[0], k[1], k[2], k[5], k[3], k[4]];
		await waitForKeys(browser, moved, 'moved two places');
		assert.match(await statusText(browser), /Unsaved changes$/);
		await browser.keys(['Control', 'z']);
		await waitForKeys(browser, [k[0], k[1], k[2], k[3], k[5], k[4]], 'undo');
		await browser.keys(['Control', 'y']);
		await waitForKeys(browser, moved, 'redo');

		// The moved page is still the selected one: turn it.
		await browser.keys('r');

		// Under the pointer, a page's label gives way to a bar with the label and the page's
		// buttons, centered under the picture.
		const second = '[data-organize-view] [role=option][aria-posinset="2"]';
		await (await browser.$(`${second} [data-picture]`)).moveTo();
		const bar = await browser.$(`${second} [data-tool]`);
		await bar.waitForDisplayed({ timeoutMsg: 'no bar under the pointer' });
		const place = await browser.execute((sel) => {
			const box = (e) => {
				const r = e.getBoundingClientRect();
				return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
			};
			const cell = document.querySelector(sel);
			return {
				cell: box(cell),
				picture: box(cell.querySelector('[data-picture]')),
				bar: box(cell.querySelector('[data-tool]')),
				text: cell.querySelector('[data-tool]').textContent.trim(),
				buttons: [...cell.querySelectorAll('[data-tool] button')].map((b) => b.getAttribute('aria-label'))
			};
		}, second);
		assert.equal(place.text, '2', 'the bar does not show the page’s label');
		assert.deepEqual(place.buttons, ['Rotate counter-clockwise', 'Rotate clockwise', 'Delete']);
		assert.ok(place.bar.top >= place.picture.bottom - 1, `the bar is over the picture: ${JSON.stringify(place)}`);
		assert.ok(place.bar.bottom <= place.cell.bottom + 1, `the bar is under its cell: ${JSON.stringify(place)}`);
		const middle = (b) => (b.left + b.right) / 2;
		assert.ok(Math.abs(middle(place.bar) - middle(place.cell)) <= 1, `the bar is not centered: ${JSON.stringify(place)}`);
		await browser.saveScreenshot(join(OUT, 'organize-bar.png'));

		// Larger and smaller pictures: the bar's buttons and Ctrl with = and - (a step of
		// the sizes), the wheel (a notch) and a pinch (as far as the fingers go).
		const start = await cellWidth(browser);
		await (await browser.$('button[aria-label="Larger pages"]')).click();
		const larger = await waitForWidth(browser, (w) => w > start, 'Larger pages');
		await browser.keys(['Control', '-']);
		assert.equal(await waitForWidth(browser, (w) => w < larger, 'Ctrl+-'), start);
		await wheelOver(browser, 1, -120);
		const notch = await waitForWidth(browser, (w) => w > start, 'a wheel notch');
		for (let i = 0; i < 4; i++) await wheelOver(browser, 1, 4);
		const pinched = await waitForWidth(browser, (w) => w < notch, 'a pinch');
		assert.ok(pinched > notch * 0.7, `a pinch of a few pixels went from ${notch} to ${pinched}`);
		await browser.keys(['Control', '-']);
		await browser.keys(['Control', '-']);
		await browser.keys(['Control', '-']);
		const smallest = await waitForWidth(browser, (w) => w < start, 'the smallest size');
		assert.equal(await (await browser.$('button[aria-label="Smaller pages"]')).isEnabled(), false);
		await wheelOver(browser, 1, 120);
		assert.equal(await waitForWidth(browser, () => true, 'past the smallest size'), smallest);
		await (await browser.$('button[aria-label="Larger pages"]')).click();
		await (await browser.$('button[aria-label="Larger pages"]')).click();
		assert.equal(await waitForWidth(browser, (w) => w > smallest, 'back to the first size'), start);
		assert.deepEqual(await keys(browser), moved, 'changing the size moved a page');

		// Delete the third page, which a bookmark leads to. Cancel first.
		await clickPage(browser, 2);
		await browser.keys('Delete');
		const dialog = await browser.$('[role=alertdialog]');
		await dialog.waitForDisplayed();
		assert.match(await dialog.getText(), /Delete page 3\?/);
		assert.match(await dialog.getText(), /Links and bookmarks that lead to it are deleted with it/);
		await browser.saveScreenshot(join(OUT, 'organize-delete.png'));
		await (await browser.$('button=Cancel')).click();
		await dialog.waitForDisplayed({ reverse: true });
		assert.deepEqual(await keys(browser), moved);

		await browser.keys('Delete');
		await (await browser.$('[role=alertdialog]')).waitForDisplayed();
		await (await browser.$('button=Delete')).click();
		const left = [k[0], k[1], k[5], k[3], k[4]];
		await waitForKeys(browser, left, 'deleted');
		await browser.waitUntil(async () => (await bodyText(browser)).includes('Deleted 1 page.'), {
			timeoutMsg: 'nothing said about the deletion'
		});
		const said = await bodyText(browser);
		assert.match(said, /1 bookmark that led to it was deleted too/);
		assert.match(said, /stays inside the file, where it can be recovered, until you save with Save As \(optimized\)/);
		assert.equal(await (await browser.$('button=Save As (optimized)…')).isExisting(), true);
		await browser.saveScreenshot(join(OUT, 'organize-view.png'));

		// Esc with a page selected deselects; Esc again goes back to reading.
		await browser.keys('Escape');
		assert.equal(await organizing(browser), true, 'Esc left with a page selected');
		assert.match(await (await browser.$('[data-organize-count]')).getText(), /^5 pages$/);
		await browser.keys('Escape');
		await browser.waitUntil(() => reading(browser), { timeoutMsg: 'Esc did not go back to reading' });
		await waitForDocument(browser);
		assert.match(await statusText(browser), /of 5/);
		await browser.waitUntil(async () => (await panes(browser)) > 0, { timeoutMsg: 'the side panes did not come back' });

		// Undo and redo work while reading too, and the page count follows.
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(async () => /of 6/.test(await statusText(browser)), { timeoutMsg: 'undo did not bring the page back' });
		await browser.keys(['Control', 'y']);
		await browser.waitUntil(async () => /of 5/.test(await statusText(browser)), { timeoutMsg: 'redo did not delete it again' });

		// A double-click on a page opens it for reading.
		await startOrganizing(browser);
		await (await browser.$('[data-organize-view] [role=option][aria-posinset="4"] [data-picture]')).doubleClick();
		await browser.waitUntil(() => reading(browser), { timeoutMsg: 'a double-click did not open the page' });
		await waitForDocument(browser);
		await browser.waitUntil(async () => /\b4 of 5/.test(await statusText(browser)), {
			timeoutMsg: `not on page 4: ${await statusText(browser)}`
		});

		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => /All changes saved$/.test(await statusText(browser)), { timeoutMsg: 'not saved' });
	} finally {
		await stop();
	}

	const report = cli('info', path);
	assert.match(report, /^pages: 5$/m);
	// The sixth page went to the fourth place, was turned, and is the third now.
	assert.match(report, /rotated pages \(page:degrees\): 3:90$/m);
	assert.deepEqual(outlineLines(path), [], 'the bookmark to the deleted page is gone');
});
