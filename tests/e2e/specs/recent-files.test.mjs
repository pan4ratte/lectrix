// Recent files and the remembered view of each file (AGENTS.md section 6.1, Phase 6).
// E2E runs keep app state in memory (FOLIO_EPHEMERAL), so this happens in one session:
// close a document, open it again from the start screen's recent list.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const pageBox = (browser) => browser.execute(() => document.querySelector('#page-box')?.value ?? null);
const zoomText = (browser) =>
	browser.execute(() => document.querySelector('[aria-label^="Zoom level"]')?.getAttribute('aria-label') ?? null);

test('a closed document reopens from the recent list where it was left', async () => {
	const first = sample('recent-a.pdf', 8);
	const second = sample('recent-b.pdf', 2);
	const { browser, stop } = await launch([first, second]);
	try {
		await waitForDocument(browser);
		// The second file is active; go back to the first, then to page 6 and zoom in twice.
		await browser.keys(['Control', 'Tab']);
		await browser.waitUntil(async () => (await browser.getTitle()).includes('recent-a') || (await pageBox(browser)) !== null);
		await browser.keys(['Control', 'g']);
		await browser.keys(['Control', 'a']);
		await browser.keys(['6', 'Enter']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '6', { timeoutMsg: 'did not go to page 6' });
		await browser.keys(['Escape']);
		await browser.keys(['Control', '=']);
		await browser.keys(['Control', '=']);
		await browser.pause(300);
		const zoom = await zoomText(browser);

		// Close both (the view is remembered on closing), and reopen the first from the list.
		await browser.keys(['Control', 'w']);
		await browser.keys(['Control', 'w']);
		const entry = await browser.$('button*=recent-a.pdf');
		await entry.waitForDisplayed({ timeoutMsg: 'recent-a.pdf is not in the recent list' });
		const order = await browser.execute(() =>
			[...document.querySelectorAll('[aria-label="Recent files"] li')].map((li) => li.textContent?.trim() ?? '')
		);
		assert.ok(order[0]?.includes('recent-b.pdf') && order[1]?.includes('recent-a.pdf'), `newest first: ${order.join(' | ')}`);

		await entry.click();
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await pageBox(browser)) === '6', {
			timeoutMsg: 'the document did not reopen on page 6'
		});
		assert.equal(await zoomText(browser), zoom, 'the zoom is remembered too');
	} finally {
		await stop();
	}
});
