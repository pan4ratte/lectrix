// Combining two 500-page files (AGENTS.md section 10, Phase 4): it completes with a
// progress bar, and stopping it leaves no file behind and an existing file untouched.
// FOLIO_COMBINE_PAGE_DELAY_MS slows the copy down so the bar can be watched and Stop
// pressed: unslowed, two generated 500-page files combine in well under a second.

import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

/** Records what the progress dialog shows, every 10 ms, into window.__progress. */
function watchProgress(browser) {
	return browser.execute(() => {
		const seen = [];
		window.__progress = seen;
		window.__progressTimer = setInterval(() => {
			const bar = document.querySelector('[role=progressbar]');
			if (!bar) return;
			const text = bar.closest('[role=alertdialog]')?.textContent ?? '';
			const entry = `${bar.getAttribute('aria-valuenow') ?? '-'} ${text.includes('Writing') ? 'writing' : 'copying'}`;
			if (seen[seen.length - 1] !== entry) seen.push(entry);
		}, 10);
	});
}

test('two 500-page files combine with progress, and Stop writes nothing', async () => {
	const a = sample('big-a.pdf', 500);
	const b = sample('big-b.pdf', 500);
	const done = join(OUT, 'big-combined.pdf');
	const stopped = join(OUT, 'big-stopped.pdf');
	rmSync(done, { force: true });
	// A file already at the second target must survive a stopped merge.
	writeFileSync(stopped, 'previous contents');

	const { browser, stop } = await launch([], {
		dialogs: [[a, b], done, stopped],
		env: { FOLIO_COMBINE_PAGE_DELAY_MS: '3' }
	});
	try {
		await (await browser.$('button*=Combine files')).click();
		await (await browser.$('button*=Add files')).click();
		await browser.waitUntil(
			// The count is formatted for the system's locale (1,000 / 1 000 / 1.000).
			async () => /1\D?000 pages from 2 files/.test(await browser.execute(() => document.body.innerText)),
			{ timeout: 20_000, timeoutMsg: 'the files were not added' }
		);

		// 1. Combine and watch the progress bar.
		await watchProgress(browser);
		const started = Date.now();
		await (await browser.$('button=Combine…')).click();
		await waitForDocument(browser);
		await browser.waitUntil(
			async () => (await browser.execute(() => document.body.innerText)).includes('into big-combined.pdf'),
			{ timeout: 30_000, timeoutMsg: 'no confirmation' }
		);
		const elapsed = Date.now() - started;
		const seen = await browser.execute(() => window.__progress);
		const counts = seen.filter((s) => s.endsWith('copying')).map((s) => Number(s.split(' ')[0]));
		assert.ok(counts.length >= 5, `the bar moved through several values: ${JSON.stringify(seen)}`);
		assert.ok(counts.every((n, i) => i === 0 || n >= counts[i - 1]), 'the bar only moves forward');
		assert.ok(seen.some((s) => s.endsWith('writing')), 'writing the file was shown');
		console.log(`combined 1,000 pages (slowed by 3 ms a page) in ${elapsed} ms; bar values: ${counts.length}`);

		// 2. Combine again into the second file and stop halfway.
		await (await browser.$('div[role=tab][title="Combine files"]')).click();
		await (await browser.$('button=Combine…')).click();
		const bar = await browser.$('[role=progressbar]');
		await bar.waitForExist({ timeout: 10_000 });
		await browser.waitUntil(async () => Number((await bar.getAttribute('aria-valuenow')) ?? 0) > 100, {
			timeout: 10_000,
			timeoutMsg: 'copying did not get going'
		});
		await browser.saveScreenshot(join(OUT, 'combine-progress.png'));
		await (await browser.$('button=Stop')).click();
		await browser.waitUntil(
			async () => (await browser.execute(() => document.body.innerText)).includes('Combining was stopped'),
			{ timeout: 10_000, timeoutMsg: 'stopping was not confirmed' }
		);
		await browser.saveScreenshot(join(OUT, 'combine-stopped.png'));
	} finally {
		await stop();
	}

	assert.match(cli('info', done), /^pages: 1000$/m);
	assert.equal(readFileSync(stopped, 'utf8'), 'previous contents', 'the existing file is untouched');
	const leftovers = readdirSync(OUT).filter((n) => n.endsWith('.folio-tmp'));
	assert.deepEqual(leftovers, [], 'no partial file is left');
	assert.ok(existsSync(done));
});
