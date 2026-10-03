// Open a file, change it, save in place, and reopen it (AGENTS.md section 9 critical flow).

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

/** Width and height of the first page on screen. */
async function firstPageShape(browser) {
	return browser.execute(() => {
		const r = document.querySelector('.page')?.getBoundingClientRect();
		return r ? { width: r.width, height: r.height } : null;
	});
}

async function statusText(browser) {
	return browser.execute(() => document.querySelector('footer')?.textContent ?? '');
}

test('open, rotate a page, save, reopen', async () => {
	const path = sample('open-save.pdf', 3);

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const before = await firstPageShape(browser);
		assert.ok(before && before.height > before.width, 'portrait page');

		await (await browser.$('.page')).click({ button: 'right' });
		await (await browser.$('div=Rotate page clockwise')).click();
		await browser.waitUntil(async () => (await statusText(browser)).includes('Unsaved changes'), {
			timeoutMsg: 'the rotation did not mark the document as changed'
		});
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), {
			timeout: 10_000,
			timeoutMsg: 'saving did not finish'
		});
	} finally {
		await stop();
	}

	// Saved on disk: page 1 has /Rotate 90; checked without the app.
	assert.match(cli('info', path), /rotated pages \(page:degrees\): 1:90\r?\n/);

	({ browser, stop } = await launch([path]));
	try {
		await waitForDocument(browser);
		const after = await firstPageShape(browser);
		assert.ok(after && after.width > after.height, 'the page reopens rotated');
		assert.match(await statusText(browser), /All changes saved/);
	} finally {
		await stop();
	}
});
