// Stitching (AGENTS.md section 6.4): combine three files (one with bookmarks, one
// with labels, one with a highlight) after removing, turning and moving pages, with undo;
// then insert pages from a file into an open document, undo and redo it, and save.
// And the grid's pointer: a rectangle drawn over it selects pages, a picture drags its page,
// and the page under it shows a bar with its label and buttons.

import assert from 'node:assert/strict';
import { existsSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { Key } from 'webdriverio';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, outlineLines, sample } from '../lib/pdfcli.mjs';

/** The cells of the Combine grid in order, as "file page". */
async function cells(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[data-key][role=option]')]
			.sort((a, b) => a.offsetTop - b.offsetTop || a.offsetLeft - b.offsetLeft)
			.map((c) => c.getAttribute('aria-label').replace(/^\d+ of \d+: /, ''))
	);
}

async function waitForCells(browser, expected, message) {
	await browser.waitUntil(async () => JSON.stringify(await cells(browser)) === JSON.stringify(expected), {
		timeout: 5000,
		timeoutMsg: `${message}: got ${JSON.stringify(await cells(browser))}`
	});
}

async function clickCell(browser, index) {
	const cell = await browser.$(`[role=option][aria-posinset="${index + 1}"]`);
	await cell.click();
}

function info(path) {
	return cli('info', path);
}

/** Bookmarks on disk as "title -> page N", in tree order. */
function bookmarks(path) {
	return outlineLines(path).map((l) =>
		l
			.trim()
			.replace(/^[+-]\s+/, '')
			.replace(/ at .*$/, '')
	);
}

test('combine three files after removing, turning and moving pages', async () => {
	const a = sample('combine-a.pdf', 3);
	const b = sample('combine-b.pdf', 4);
	const c = sample('combine-c.pdf', 2);
	cli('outline', 'set', a, a.replace('.pdf', '-o.pdf'), '--item', '0:2:Chapter A');
	cli('labels', 'set', b, b.replace('.pdf', '-l.pdf'), '--rule', '1:roman-lower');
	cli('annot', 'markup', c, c.replace('.pdf', '-h.pdf'), '--page', '1', '--text', 'quick brown fox');
	const files = [a, b, c].map((p, i) => p.replace('.pdf', ['-o', '-l', '-h'][i] + '.pdf'));
	const out = join(OUT, 'combined.pdf');
	rmSync(out, { force: true });

	const { browser, stop } = await launch([], { dialogs: [files, out] });
	try {
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('div[role=tab][data-tip="Combine files"]')).waitForDisplayed();
		await (await browser.$('button*=Add files')).click();
		await waitForCells(
			browser,
			[
				'combine-a-o.pdf, page 1',
				'combine-a-o.pdf, page 2',
				'combine-a-o.pdf, page 3',
				'combine-b-l.pdf, page i (1)',
				'combine-b-l.pdf, page ii (2)',
				'combine-b-l.pdf, page iii (3)',
				'combine-b-l.pdf, page iv (4)',
				'combine-c-h.pdf, page 1',
				'combine-c-h.pdf, page 2'
			],
			'files added'
		);
		await browser.saveScreenshot(join(OUT, 'combine-view.png'));

		// Remove b's first page, turn a's first page, move c's first page to the front.
		await clickCell(browser, 3);
		await browser.keys('Delete');
		await clickCell(browser, 0);
		await browser.keys('r');
		await clickCell(browser, 6);
		for (let i = 0; i < 6; i++) await browser.keys(['Alt', 'Shift', 'ArrowLeft']);
		const arranged = [
			'combine-c-h.pdf, page 1',
			'combine-a-o.pdf, page 1, turned 90°',
			'combine-a-o.pdf, page 2',
			'combine-a-o.pdf, page 3',
			'combine-b-l.pdf, page ii (2)',
			'combine-b-l.pdf, page iii (3)',
			'combine-b-l.pdf, page iv (4)',
			'combine-c-h.pdf, page 2'
		];
		await waitForCells(browser, arranged, 'arranged');
		const fileList = await browser.execute(() => document.querySelector('[aria-label=Files]')?.textContent ?? '');
		assert.match(fileList.replace(/\s+/g, ' '), /3 of 4 pages/);

		// Undo the last move and redo it.
		await browser.keys(['Control', 'z']);
		await waitForCells(browser, [arranged[1], arranged[0], ...arranged.slice(2)], 'undo');
		await browser.keys(['Control', 'y']);
		await waitForCells(browser, arranged, 'redo');

		await (await browser.$('button=Combine…')).click();
		await waitForDocument(browser);
		await browser.waitUntil(
			async () => (await browser.execute(() => document.body.innerText)).includes('Combined 8 pages from 3 files'),
			{ timeout: 10_000, timeoutMsg: 'no confirmation shown' }
		);
		await browser.saveScreenshot(join(OUT, 'combine-done.png'));
	} finally {
		await stop();
	}

	const report = info(out);
	assert.match(report, /^pages: 8$/m);
	assert.match(report, /rotated pages \(page:degrees\): 2:90$/m);
	assert.match(report, /first labels: 1 1 2 3 ii iii iv 2$/m);
	assert.match(report, /annotation p1: Highlight/);
	// Bookmarks nested under each file, in the order their pages come.
	assert.deepEqual(bookmarks(out), [
		'Lectrix sample -> page 1',
		'Lectrix sample -> page 2',
		'Chapter A -> page 3',
		'Lectrix sample -> page 5'
	]);
});

test('insert pages from a file, undo, redo and save', async () => {
	const doc = sample('insert-into.pdf', 3);
	const from = sample('insert-from.pdf', 4);

	const { browser, stop } = await launch([doc], { dialogs: [from] });
	try {
		await waitForDocument(browser);
		await (await browser.$('button=Document')).click();
		await (await browser.$('div*=Insert pages from file')).click();
		const dialog = await browser.$('[role=dialog]');
		await dialog.waitForDisplayed();
		assert.match(await dialog.getText(), /Insert pages from insert-from\.pdf/);
		// Where they go is said, not asked: after the page the document is at, in the grid
		// of pages, which is shown for it.
		assert.match(await dialog.getText(), /Into insert-into\.pdf, after page 1\./);
		assert.equal(await (await browser.$('input[aria-label="Page number"]')).isExisting(), false);
		assert.equal(await browser.execute(() => document.querySelector('[data-organize-view]') !== null), true);
		// Pages 2-3 of the file.
		await (await browser.$('input[aria-label="Pages to insert"]')).setValue('2-3');
		await (await browser.$('button=Insert')).click();
		// The inserted pages are selected there. Then back to reading.
		await browser.waitUntil(
			async () => (await (await browser.$('[data-organize-count]')).getText()) === '2 of 5 pages selected',
			{ timeout: 5000, timeoutMsg: 'the document did not grow to 5 pages' }
		);
		await (await browser.$('button=Done')).click();
		await waitForDocument(browser);
		const status = () => statusText(browser);
		assert.match(await status(), /of 5/);
		await (await browser.$('.page')).click();
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(async () => (await status()).includes('of 3'), { timeoutMsg: 'undo' });
		await browser.keys(['Control', 'y']);
		await browser.waitUntil(async () => (await status()).includes('of 5'), { timeoutMsg: 'redo' });
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await status()).includes('All changes saved'), {
			timeoutMsg: 'saving'
		});
	} finally {
		await stop();
	}
	const report = info(doc);
	assert.match(report, /^pages: 5$/m);
	assert.deepEqual(bookmarks(doc), ['Lectrix sample -> page 2']);
});

/** A one-page PDF with a signed signature field (a /Sig field with a value), written by
 * hand: enough for Lectrix to see the document as signed. */
function signedPdf(path) {
	const text = 'BT /F1 24 Tf 72 700 Td (Signed page) Tj ET';
	const objects = [
		'<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /SigFlags 3 >> >>',
		'<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>',
		'<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Annots [4 0 R] /Resources << /Font << /F1 7 0 R >> >> >>',
		'<< /Type /Annot /Subtype /Widget /FT /Sig /T (Signature1) /Rect [0 0 0 0] /P 3 0 R /V 5 0 R /F 132 >>',
		'<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange [0 0 0 0] /Contents <00> >>',
		`<< /Length ${text.length} >>\nstream\n${text}\nendstream`,
		'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'
	];
	let out = '%PDF-1.7\n';
	const offsets = [];
	objects.forEach((body, i) => {
		offsets.push(out.length);
		out += `${i + 1} 0 obj\n${body}\nendobj\n`;
	});
	const xref = out.length;
	out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
	for (const o of offsets) out += `${String(o).padStart(10, '0')} 00000 n \n`;
	out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
	writeFileSync(path, out, 'latin1');
}

test('combining a signed file warns first', async () => {
	const signed = join(OUT, 'signed.pdf');
	signedPdf(signed);
	const plain = sample('combine-plain.pdf', 2);
	const out = join(OUT, 'combined-signed.pdf');
	rmSync(out, { force: true });

	const { browser, stop } = await launch([], { dialogs: [[signed, plain], out] });
	try {
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('button*=Add files')).click();
		await waitForCells(browser, ['signed.pdf, page 1', 'combine-plain.pdf, page 1', 'combine-plain.pdf, page 2'], 'files added');

		// Cancel: nothing is written.
		await (await browser.$('button=Combine…')).click();
		const dialog = await browser.$('[role=alertdialog]');
		await dialog.waitForDisplayed();
		assert.match(await dialog.getText(), /The signature in signed\.pdf will not be valid in the combined file/);
		await (await browser.$('button=Cancel')).click();
		await dialog.waitForDisplayed({ reverse: true });
		assert.equal(existsSync(out), false);

		// Combine anyway.
		await (await browser.$('button=Combine…')).click();
		await (await browser.$('button=Combine anyway')).click();
		await waitForDocument(browser);
	} finally {
		await stop();
	}
	assert.match(info(out), /^pages: 3$/m);
});

/** Where cell `index` and its picture are, in viewport pixels. */
async function cellBoxes(browser, index) {
	return browser.execute((i) => {
		const cell = document.querySelector(`[role=option][aria-posinset="${i + 1}"]`);
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		};
		return { cell: box(cell), picture: box(cell.querySelector('[data-picture]')) };
	}, index);
}

const middle = (r) => ({ x: Math.round((r.left + r.right) / 2), y: Math.round((r.top + r.bottom) / 2) });
/** A point in a cell's caption: under its picture, clear of it. */
const inCaption = ({ cell }) => ({ x: Math.round((cell.left + cell.right) / 2), y: Math.round(cell.bottom - 8) });

/** The selected cells, as 0-based places in the grid. */
async function selectedCells(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[role=option][aria-selected=true]')]
			.map((c) => Number(c.getAttribute('aria-posinset')) - 1)
			.sort((a, b) => a - b)
	);
}

/** Presses at `from` and moves to `to` in steps; the button stays down. */
async function pressAndMove(browser, from, to, steps = 6) {
	let action = browser.action('pointer').move({ ...from, origin: 'viewport' }).down();
	for (let i = 1; i <= steps; i++) {
		action = action.move({
			x: Math.round(from.x + ((to.x - from.x) * i) / steps),
			y: Math.round(from.y + ((to.y - from.y) * i) / steps),
			origin: 'viewport',
			duration: 20
		});
	}
	await action.perform(true);
}

const release = (browser) => browser.action('pointer').up().perform();

test('a rectangle selects the pages it touches, and a picture still drags its page', async () => {
	const a = sample('grid-a.pdf', 3);
	const b = sample('grid-b.pdf', 4);
	const { browser, stop } = await launch([], { dialogs: [[a, b]] });
	try {
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('button*=Add files')).click();
		const names = [1, 2, 3].map((n) => `grid-a.pdf, page ${n}`).concat([1, 2, 3, 4].map((n) => `grid-b.pdf, page ${n}`));
		await waitForCells(browser, names, 'files added');
		assert.deepEqual(await selectedCells(browser), []);

		// From the caption of the second page up into the picture of the third: both
		// pictures are touched, their neighbours are not.
		const second = await cellBoxes(browser, 1);
		const third = await cellBoxes(browser, 2);
		await pressAndMove(browser, inCaption(second), middle(third.picture));
		assert.equal(await (await browser.$('.page-grid-marquee')).isExisting(), true, 'no rectangle is drawn');
		await browser.saveScreenshot(join(OUT, 'combine-rectangle.png'));
		assert.deepEqual(await selectedCells(browser), [1, 2]);
		await release(browser);
		assert.equal(await (await browser.$('.page-grid-marquee')).isExisting(), false);
		assert.deepEqual(await selectedCells(browser), [1, 2]);

		// With Ctrl, a rectangle adds to the selection.
		const fifth = await cellBoxes(browser, 4);
		await browser.action('key').down(Key.Ctrl).perform(true);
		await pressAndMove(browser, inCaption(fifth), middle(fifth.picture));
		await release(browser);
		await browser.action('key').up(Key.Ctrl).perform();
		assert.deepEqual(await selectedCells(browser), [1, 2, 4]);

		// Esc while a rectangle is drawn puts the selection back as it was.
		const first = await cellBoxes(browser, 0);
		await pressAndMove(browser, inCaption(first), middle(first.picture));
		assert.deepEqual(await selectedCells(browser), [0]);
		await browser.keys('Escape');
		assert.deepEqual(await selectedCells(browser), [1, 2, 4]);
		await release(browser);
		assert.deepEqual(await selectedCells(browser), [1, 2, 4]);

		// A click on a caption, without a drag, selects that page alone.
		const sixth = await cellBoxes(browser, 5);
		await browser.action('pointer').move({ ...inCaption(sixth), origin: 'viewport' }).down().up().perform();
		assert.deepEqual(await selectedCells(browser), [5]);

		// A drag that starts on a picture moves the page: the sixth to the front.
		await pressAndMove(browser, middle(sixth.picture), { x: Math.round(first.cell.left + 4), y: middle(first.picture).y });
		assert.equal(await (await browser.$('.page-grid-marquee')).isExisting(), false, 'a picture started a rectangle');
		await release(browser);
		await waitForCells(browser, [names[5], ...names.slice(0, 5), names[6]], 'moved by dragging its picture');
		assert.deepEqual(await selectedCells(browser), [0]);
	} finally {
		await stop();
	}
});

test('the page under the pointer shows a bar with its label and buttons, in the label’s place', async () => {
	const a = sample('combine-bar.pdf', 3);
	const { browser, stop } = await launch([], { dialogs: [[a]] });
	try {
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('button=Add files…')).click();
		await waitForCells(browser, ['combine-bar.pdf, page 1', 'combine-bar.pdf, page 2', 'combine-bar.pdf, page 3'], 'pages added');

		const second = '[role=option][aria-posinset="2"]';
		await (await browser.$(`${second} [data-picture]`)).moveTo();
		await (await browser.$(`${second} [data-tool]`)).waitForDisplayed({ timeoutMsg: 'no bar under the pointer' });
		const place = await browser.execute((sel) => {
			const box = (e) => {
				const r = e.getBoundingClientRect();
				return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
			};
			const cell = document.querySelector(sel);
			// The second line of the caption: the file's name, which the bar leaves alone.
			const name = [...cell.children].at(-2);
			return {
				cell: box(cell),
				picture: box(cell.querySelector('[data-picture]')),
				bar: box(cell.querySelector('[data-tool]')),
				name: { ...box(name), text: name.textContent.trim() },
				text: cell.querySelector('[data-tool]').textContent.trim(),
				buttons: [...cell.querySelectorAll('[data-tool] button')].map((b) => b.getAttribute('aria-label'))
			};
		}, second);
		assert.equal(place.text, '2', 'the bar does not show the page’s label');
		assert.deepEqual(place.buttons, ['Turn counter-clockwise', 'Turn clockwise', 'Remove']);
		assert.equal(place.name.text, 'combine-bar.pdf');
		assert.ok(place.bar.top >= place.picture.bottom - 1, `the bar is over the picture: ${JSON.stringify(place)}`);
		assert.ok(place.bar.bottom <= place.name.top + 1, `the bar is over the file’s name: ${JSON.stringify(place)}`);
		const middle = (b) => (b.left + b.right) / 2;
		assert.ok(Math.abs(middle(place.bar) - middle(place.cell)) <= 1, `the bar is not centered: ${JSON.stringify(place)}`);
		await browser.saveScreenshot(join(OUT, 'combine-bar.png'));

		// Its buttons act on that page alone.
		await (await browser.$(`${second} [data-tool] button[aria-label="Remove"]`)).click();
		await waitForCells(browser, ['combine-bar.pdf, page 1', 'combine-bar.pdf, page 3'], 'the bar’s Remove');
	} finally {
		await stop();
	}
});
