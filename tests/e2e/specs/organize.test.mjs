// Organizing pages (AGENTS.md section 6.7): the document's pages as a grid in place of the
// viewer, without the side panes; a page moved, turned and deleted there, each at once and
// undoable; the question before deleting and what is said after; the bar under the pointer
// with the page's label and buttons; larger and smaller pictures from the bar, the wheel
// and a pinch, up to one page filling the view; back to reading, and the saved file. Then
// pages from a file, put where the menu says, and pages into a new document, which is not
// a file until it is saved.

import assert from 'node:assert/strict';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, atRest, launch, organizePages, statusText, waitForDocument } from '../lib/app.mjs';
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

/** Ctrl+wheel over the middle of page `index` (a touchpad pinch arrives the same way). With
 * `index` null, over the page nearest the middle of the view: the grid has cells only for
 * the pages near its view, and at the largest size, with a page to a row, which pages
 * those are depends on where the view came to rest. */
async function wheelOver(browser, index, deltaY) {
	await browser.execute(
		(position, d) => {
			const cells = [...document.querySelectorAll('[data-organize-view] [role=option]')];
			const view = document.querySelector('[data-organize-view] [role=listbox]').getBoundingClientRect();
			const off = (c) => {
				const box = c.getBoundingClientRect();
				return Math.abs((box.top + box.bottom) / 2 - (view.top + view.bottom) / 2);
			};
			const cell = position === null ? cells.sort((a, b) => off(a) - off(b))[0] : cells.find((c) => c.getAttribute('aria-posinset') === String(position));
			if (!cell) throw new Error(`the grid has no cell for page ${position ?? 'in view'}; it has ${cells.map((c) => c.getAttribute('aria-posinset')).join(', ')}`);
			const r = cell.getBoundingClientRect();
			const x = r.left + r.width / 2;
			const y = r.top + r.height / 2;
			cell.dispatchEvent(new WheelEvent('wheel', { deltaY: d, ctrlKey: true, clientX: x, clientY: y, bubbles: true, cancelable: true }));
		},
		index === null ? null : index + 1,
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
	// The view bar has no button for it: the Document menu has the command.
	assert.equal(await (await browser.$('[data-view-bar] [aria-label="Organize pages"]')).isExisting(), false);
	await organizePages(browser);
}

/** Where the grid's cells are in its view: the room left and right of the first row, how
 * many cells that row has, and whether a cell fits the view's height. */
const layout = (browser) =>
	browser.execute(() => {
		const grid = document.querySelector('[data-organize-view] [role=listbox]').getBoundingClientRect();
		const cells = [...document.querySelectorAll('[data-organize-view] [role=option]')].map((c) => c.getBoundingClientRect());
		const top = Math.min(...cells.map((c) => c.top));
		const row = cells.filter((c) => Math.abs(c.top - top) < 1);
		return {
			left: Math.min(...row.map((c) => c.left)) - grid.left,
			// (Without the scrollbar, when there is one.)
			right: grid.left + document.querySelector('[data-organize-view] [role=listbox]').clientWidth - Math.max(...row.map((c) => c.right)),
			inRow: row.length,
			cellHeight: row[0].height,
			viewHeight: grid.height
		};
	});

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

		// The bar: Insert, Extract and Labels say what they are, and Done comes last.
		const named = await browser.execute(() =>
			[...document.querySelectorAll('[data-organize-bar] button')].map((b) => b.textContent.trim())
		);
		assert.deepEqual(named.filter(Boolean), ['Insert', 'Extract', 'Labels', 'Done']);
		assert.equal(named.at(-1), 'Done', 'Done is not at the end of the bar');
		// The pages are centered in the view.
		const centered = await layout(browser);
		assert.ok(Math.abs(centered.left - centered.right) <= 1.5, `the grid is not centered: ${JSON.stringify(centered)}`);

		// The last page, two places earlier; undo takes one step back, redo brings it again.
		await clickPage(browser, 5);
		assert.match(await (await browser.$('[data-organize-count]')).getText(), /^1 of 6 pages selected$/);
		// A selected page has the accent's border around its whole block, not its picture.
		const marked = await browser.execute(() => {
			const cell = document.querySelector('[data-organize-view] [role=option][aria-selected="true"]');
			return {
				block: getComputedStyle(cell).boxShadow,
				picture: getComputedStyle(cell.querySelector('[data-picture]')).outlineStyle
			};
		});
		assert.match(marked.block, /inset/, `the selected block has no border: ${marked.block}`);
		assert.equal(marked.picture, 'none', 'the selected picture is outlined');
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
				buttons: [...cell.querySelectorAll('[data-tool] button')].map((b) => b.getAttribute('aria-label')),
				sizes: [...cell.querySelectorAll('[data-tool] button')].map((b) => {
					const r = b.getBoundingClientRect();
					return [r.width, r.height, b.querySelector('svg').getBoundingClientRect().width];
				})
			};
		}, second);
		assert.equal(place.text, '2', 'the bar does not show the page’s label');
		assert.deepEqual(place.buttons, ['Rotate counter-clockwise', 'Rotate clockwise', 'Delete']);
		// Its buttons are a bar's: 32 px, with 16 px icons.
		assert.deepEqual(place.sizes, [[32, 32, 16], [32, 32, 16], [32, 32, 16]]);
		assert.ok(place.bar.top >= place.picture.bottom - 1, `the bar is over the picture: ${JSON.stringify(place)}`);
		// It keeps clear of the block's lower edge.
		assert.ok(place.bar.bottom <= place.cell.bottom - 6, `the bar sits on the edge of its block: ${JSON.stringify(place)}`);
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

		// Larger, step by step, as far as it goes: one page fills the view, in the middle.
		const largerButton = await browser.$('button[aria-label="Larger pages"]');
		for (let i = 0; i < 20 && (await largerButton.isEnabled()); i++) await largerButton.click();
		assert.equal(await largerButton.isEnabled(), false, 'the pictures never reached their largest size');
		await waitForWidth(browser, (w) => w > start * 2, 'the largest size');
		const one = await layout(browser);
		assert.equal(one.inRow, 1, `more than one page in a row at the largest size: ${JSON.stringify(one)}`);
		assert.ok(one.cellHeight <= one.viewHeight, `a page does not fit the view: ${JSON.stringify(one)}`);
		assert.ok(Math.abs(one.left - one.right) <= 1.5, `the page is not centered: ${JSON.stringify(one)}`);
		// The wheel goes no further.
		const largest = await cellWidth(browser);
		await wheelOver(browser, null, -120);
		assert.equal(await waitForWidth(browser, () => true, 'past the largest size'), largest);
		await browser.saveScreenshot(join(OUT, 'organize-one-page.png'));
		const smallerButton = await browser.$('button[aria-label="Smaller pages"]');
		for (let i = 0; i < 20 && (await cellWidth(browser)) > start; i++) {
			await smallerButton.click();
			await waitForWidth(browser, () => true, 'a step smaller');
		}
		assert.equal(await cellWidth(browser), start, 'the steps down did not come back to the first size');
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

		// A double-click on a page opens it for reading. (The grid lays its cells out once it
		// knows its width, and puts the current page in view: the page is clicked where it
		// rests.)
		await startOrganizing(browser);
		const fourth = '[data-organize-view] [role=option][aria-posinset="4"] [data-picture]';
		await atRest(browser, fourth);
		await (await browser.$(fourth)).doubleClick();
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

const activeTab = (browser) =>
	browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.trim() ?? '');

test('the start screen’s Organize pages asks for a file and shows its pages as a grid', async () => {
	const doc = sample('organize-start.pdf', 3);
	// The file dialog: given up, then the file.
	const { browser, stop } = await launch([], { dialogs: [null, [doc]] });
	try {
		const tile = await browser.$('[data-tool="organize"]');
		await tile.waitForDisplayed({ timeoutMsg: 'the start screen has no Organize pages' });
		assert.equal((await tile.getText()).trim(), 'Organize pages…');
		await browser.saveScreenshot(join(OUT, 'organize-start.png'));
		await tile.click();
		await browser.pause(300);
		assert.equal(await organizing(browser), false, 'a file given up was organized');
		assert.equal(await tile.isDisplayed(), true, 'giving the file up left the start screen');

		await tile.click();
		await browser.waitUntil(() => organizing(browser), { timeoutMsg: 'the file’s pages did not show as a grid' });
		assert.match(await activeTab(browser), /organize-start\.pdf/);
		await browser.waitUntil(async () => (await keys(browser)).length === 3, { timeoutMsg: 'not every page is in the grid' });
		// Done goes on to reading it.
		await (await browser.$('button=Done')).click();
		await waitForDocument(browser);
		assert.match(await statusText(browser), /of 3/);
	} finally {
		await stop();
	}
});

test('pages come from files where the menu says, and go into a new document', async () => {
	const doc = sample('organize-into.pdf', 4);
	const from = sample('organize-from.pdf', 3);
	const more = sample('organize-more.pdf', 2);
	const out = join(OUT, 'organize-extract.pdf');
	rmSync(out, { force: true });

	// The file dialogs: given up, then two files to insert, then where to save the new
	// document the extracted pages went into.
	const { browser, stop } = await launch([doc], { dialogs: [null, [from, more], out] });
	try {
		await waitForDocument(browser);
		// While reading, the command shows the grid for the pages to go into. Giving the
		// file up goes back to reading.
		await (await browser.$('button=Document')).click();
		await (await browser.$('div*=Insert pages from file')).click();
		await browser.waitUntil(async () => (await reading(browser)) && !(await organizing(browser)), {
			timeoutMsg: 'giving the file up did not go back to reading'
		});

		await startOrganizing(browser);
		await browser.waitUntil(async () => (await keys(browser)).length === 4, { timeoutMsg: 'not every page is in the grid' });
		const k = await keys(browser);

		// The menu of the third page, from the keyboard: it is for the selected pages.
		await clickPage(browser, 2);
		await browser.keys(['Shift', 'F10']);
		const menu = await browser.$('[role=menu]');
		await menu.waitForDisplayed({ timeoutMsg: 'Shift+F10 opened no menu' });
		await browser.saveScreenshot(join(OUT, 'organize-menu.png'));
		const items = await browser.execute(() => [...document.querySelectorAll('[role=menu] [role=menuitem]')].map((i) => i.textContent.trim()));
		assert.deepEqual(
			items.map((i) => i.replace(/(Shift\+R|R|Del|Enter)$/, '').trim()),
			[
				'Rotate clockwise',
				'Rotate counter-clockwise',
				'Delete page…',
				'Extract page into a new document',
				'Insert pages from file before…',
				'Insert pages from file after…',
				'Page labels…',
				'Read from this page'
			]
		);
		await (await browser.$('div*=Insert pages from file before')).click();
		const dialog = await browser.$('[role=dialog]');
		await dialog.waitForDisplayed();
		// Two files: every page of each, in their order; the dialog lists them and does not
		// ask which pages.
		const asked = await dialog.getText();
		assert.match(asked, /Insert pages from 2 files/);
		assert.match(asked, /Into organize-into\.pdf, after page 2\./);
		assert.match(asked, /organize-from\.pdf\s+3 pages\s+organize-more\.pdf\s+2 pages/);
		assert.equal(await (await browser.$('input[aria-label="Pages to insert"]')).isExisting(), false);
		await (await browser.$('[role=dialog] button[type=submit]')).click();
		const count = await browser.$('[data-organize-count]');
		await browser.waitUntil(async () => (await count.getText()) === '5 of 9 pages selected', {
			timeoutMsg: `the pages did not arrive: ${await count.getText()}`
		});
		const after = await keys(browser);
		assert.deepEqual([after[0], after[1], after[7], after[8]], k, 'the document’s own pages moved otherwise');
		await browser.waitUntil(async () => (await bodyText(browser)).includes('Inserted 5 pages from 2 files.'), {
			timeoutMsg: 'nothing said about the insertion'
		});
		// Undo takes the files back one at a time: the second one's two pages first.
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(async () => (await keys(browser)).length === 7, { timeoutMsg: 'undo did not take a file back' });
		assert.deepEqual(await keys(browser), [after[0], after[1], after[2], after[3], after[4], after[7], after[8]]);
		await browser.keys(['Control', 'y']);
		await browser.waitUntil(async () => (await keys(browser)).length === 9, { timeoutMsg: 'redo did not bring it again' });

		// A right click on a page that is not selected selects it for the menu.
		await (await browser.$('[data-organize-view] [role=option][aria-posinset="1"] [data-picture]')).click({ button: 'right' });
		await (await browser.$('[role=menu]')).waitForDisplayed();
		assert.equal(await count.getText(), '1 of 9 pages selected');
		await browser.keys('Escape');
		await (await browser.$('[role=menu]')).waitForDisplayed({ reverse: true });

		// The first file's pages into a new document, with the changes not saved yet: it
		// opens in a tab of its own with its pages as a grid, without asking where to save,
		// and the document keeps its pages.
		await clickPage(browser, 2);
		await browser.keys(['Shift', 'ArrowRight']);
		await browser.keys(['Shift', 'ArrowRight']);
		assert.equal(await count.getText(), '3 of 9 pages selected');
		await (await browser.$('button[aria-label="Extract selected pages"]')).click();
		await browser.waitUntil(async () => (await activeTab(browser)).includes('Pages from organize-into.pdf'), {
			timeoutMsg: `the new document did not open: ${await activeTab(browser)}`
		});
		await browser.waitUntil(async () => (await (await browser.$('[data-organize-count]')).getText()) === '3 pages', {
			timeoutMsg: 'the new document does not show its pages as a grid'
		});
		assert.equal(await organizing(browser), true);
		// It is no file yet: marked unsaved, and its tab says so.
		assert.match(await statusText(browser), /Unsaved changes$/);
		assert.equal(
			await browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"]').getAttribute('data-tip')),
			'Not saved yet'
		);
		const said = await bodyText(browser);
		assert.match(said, /Extracted 3 pages into a new document\./);
		assert.match(said, /It is not saved yet: save it to keep it\./);
		assert.equal(existsSync(out), false, 'a file was written before saving');
		await browser.saveScreenshot(join(OUT, 'organize-extracted.png'));

		// Its pages can be put in order before it is saved: the last one first.
		const made = await keys(browser);
		await clickPage(browser, 2);
		await browser.keys(['Alt', 'Shift', 'ArrowLeft']);
		await browser.keys(['Alt', 'Shift', 'ArrowLeft']);
		await waitForKeys(browser, [made[2], made[0], made[1]], 'moved in the new document');

		// Save asks where: then it is that file.
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await activeTab(browser)).includes('organize-extract.pdf'), {
			timeoutMsg: `saving did not name the document: ${await activeTab(browser)}`
		});
		await browser.waitUntil(async () => /All changes saved$/.test(await statusText(browser)), { timeoutMsg: 'not saved' });
		assert.equal(await organizing(browser), true, 'saving left the grid');

		// One more page into a new document, closed without saving: Lectrix asks first.
		await clickPage(browser, 0);
		await (await browser.$('button[aria-label="Extract selected pages"]')).click();
		await browser.waitUntil(async () => (await activeTab(browser)).includes('Pages from organize-extract.pdf'), {
			timeoutMsg: `the second new document did not open: ${await activeTab(browser)}`
		});
		await browser.keys(['Control', 'w']);
		const ask = await browser.$('[role=alertdialog]');
		await ask.waitForDisplayed({ timeoutMsg: 'closing a document that was never saved did not ask' });
		assert.match(await ask.getText(), /Pages from organize-extract\.pdf has not been saved yet/);
		await (await browser.$('button=Don’t save')).click();
		await browser.waitUntil(async () => (await activeTab(browser)).includes('organize-extract.pdf'), {
			timeoutMsg: `the unsaved document did not close: ${await activeTab(browser)}`
		});

		await browser.keys(['Control', 'Shift', 'Tab']);
		await browser.waitUntil(async () => (await activeTab(browser)).includes('organize-into.pdf'), {
			timeoutMsg: `not back at the document: ${await activeTab(browser)}`
		});
		await browser.waitUntil(() => organizing(browser), { timeoutMsg: 'the document left the grid' });
		assert.equal(await (await browser.$('[data-organize-count]')).getText(), '3 of 9 pages selected');
	} finally {
		await stop();
	}

	assert.match(cli('info', out), /^pages: 3$/m);
	assert.match(cli('info', doc), /^pages: 4$/m, 'the document was saved, or changed on disk');
});
