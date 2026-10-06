// Recent files and the remembered view of each file (AGENTS.md section 6.1).
// E2E runs keep app state in memory (LECTRIX_EPHEMERAL), so this happens in one session:
// close a document, open it again from the start screen's recent files (as a grid).

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
		// The second step, pressed while the first still glides, goes on from where that was
		// heading, and both land on presets.
		assert.match(zoom ?? '', /^Zoom level (50|75|100|125|150|200|300|400)%$/, `zoom after two steps: ${zoom}`);

		// Close both (the view is remembered on closing), and reopen the first from the list.
		await browser.keys(['Control', 'w']);
		await browser.keys(['Control', 'w']);
		await (await browser.$('button*=recent-a.pdf')).waitForDisplayed({ timeoutMsg: 'recent-a.pdf is not in the recent list' });
		// A table: name, when it was opened, size; newest first.
		const rows = await browser.execute(() =>
			[...document.querySelectorAll('table tbody tr')].map((tr) => [...tr.cells].slice(0, 3).map((td) => td.textContent?.trim() ?? ''))
		);
		const order = rows.map((r) => r[0]);
		assert.ok(order[0] === 'recent-b.pdf' && order[1] === 'recent-a.pdf', `newest first: ${order.join(' | ')}`);
		assert.match(rows[1][1], /^Today, \d{1,2}:\d{2}/, `opened: ${rows[1][1]}`);
		assert.match(rows[1][2], /^[\d.,]+ (bytes|KB)$/, `size: ${rows[1][2]}`);

		// The grid: each file's first page, then its name, then when it was opened and its size.
		await (await browser.$('button[aria-label="Grid"]')).click();
		await browser.waitUntil(
			() =>
				browser.execute(() => {
					const pages = [...document.querySelectorAll('.recent-card img.recent-page')];
					return pages.length === 2 && pages.every((img) => img.complete && img.naturalWidth > 0);
				}),
			{ timeoutMsg: 'the first pages did not show in the grid' }
		);
		const cards = await browser.execute(() =>
			[...document.querySelectorAll('.recent-card')].map((card) => ({
				name: card.querySelector('.recent-name')?.textContent?.trim() ?? '',
				details: card.querySelector('.recent-details')?.textContent?.replace(/\s+/g, ' ').trim() ?? '',
				ratio: (() => {
					const img = card.querySelector('img.recent-page');
					return img ? img.naturalHeight / img.naturalWidth : 0;
				})()
			}))
		);
		assert.deepEqual(
			cards.map((c) => c.name),
			['recent-b.pdf', 'recent-a.pdf'],
			'the grid is newest first'
		);
		assert.match(cards[1].details, /^Today, \d{1,2}:\d{2}.* · [\d.,]+ (bytes|KB)$/, `details: ${cards[1].details}`);
		// The sample pages are US Letter, 612 x 792 pt.
		assert.ok(Math.abs(cards[1].ratio - 792 / 612) < 0.02, `the preview has the page's shape: ${cards[1].ratio}`);

		const card = await (await browser.$('.recent-card*=recent-a.pdf')).$('button.recent-open');
		await card.click();
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await pageBox(browser)) === '6', {
			timeoutMsg: 'the document did not reopen on page 6'
		});
		assert.equal(await zoomText(browser), zoom, 'the zoom is remembered too');
	} finally {
		await stop();
	}
});
