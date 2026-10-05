// Stitching (AGENTS.md section 6.4): combine three files (one with bookmarks, one
// with labels, one with a highlight) after removing, turning and moving pages, with undo;
// then insert pages from a file into an open document, undo and redo it, and save.

import assert from 'node:assert/strict';
import { existsSync, rmSync, writeFileSync } from 'node:fs';
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
		// Pages 2-3 of the file, after page 1.
		await (await browser.$('input[aria-label="Pages to insert"]')).setValue('2-3');
		await (await browser.$('input[aria-label="Page number"]')).setValue('1');
		await (await browser.$('button=Insert')).click();
		const status = () => browser.execute(() => [...document.querySelectorAll('[data-view-bar], footer')].map((e) => e.textContent).join(' '));
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
