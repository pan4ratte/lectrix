// Stitching (AGENTS.md section 6.4, Phase 4): combine three files (one with bookmarks, one
// with labels, one with a highlight) after removing, turning and moving pages, with undo;
// then insert pages from a file into an open document, undo and redo it, and save.

import assert from 'node:assert/strict';
import { rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';
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
		await (await browser.$('div[role=tab][title="Combine files"]')).waitForDisplayed();
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
		'Folio sample -> page 1',
		'Folio sample -> page 2',
		'Chapter A -> page 3',
		'Folio sample -> page 5'
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
		// Pages 2-3 of the file, after page 1.
		await (await browser.$('input[aria-label="Pages to insert"]')).setValue('2-3');
		await (await browser.$('input[aria-label="Page number"]')).setValue('1');
		await (await browser.$('button=Insert')).click();
		const status = () => browser.execute(() => document.querySelector('footer')?.textContent ?? '');
		await browser.waitUntil(async () => (await status()).includes('of 5'), {
			timeout: 5000,
			timeoutMsg: 'the document did not grow to 5 pages'
		});
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
	assert.deepEqual(bookmarks(doc), ['Folio sample -> page 2']);
});
