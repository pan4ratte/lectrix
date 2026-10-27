// Compare (section 6.13), as FineReader's Compare Documents: from the start screen, two
// documents opened into the two panes, compared; the differences listed in their groups and
// marked on both documents' pages; stepped through, ignored; and saved as an annotated copy
// and as a report. The documents compared are only read.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { unnamedControls } from '../lib/a11y.mjs';
import { OUT, keys, launch, waitForDocument } from '../lib/app.mjs';
import { cli } from '../lib/pdfcli.mjs';

const notices = (browser) =>
	browser.execute(() => [...document.querySelectorAll('[role="status"]')].map((n) => n.textContent.trim()).join('\n'));

/** Two versions of a short agreement: a word changed, a sentence replaced by a new page's
 * text, a spelling, and the running header. */
function versions() {
	const old = join(OUT, 'compare-old.pdf');
	const nw = join(OUT, 'compare-new.pdf');
	for (const p of [old, nw]) rmSync(p, { force: true });
	cli(
		'gen-text',
		old,
		'--header',
		'Supply agreement',
		'--page',
		'1. The supplier delivers the goods by Friday.|2. The buyer pays within thirty days.|Late payment carries interest.',
		'--page',
		'3. Either party may end this agreement in writing.|The colour of the goods is grey.'
	);
	cli(
		'gen-text',
		nw,
		'--header',
		'Supply agreement, second draft',
		'--page',
		'1. The supplier delivers the goods by Monday.|2. The buyer pays within thirty days.',
		'--page',
		'Annex: prices are fixed for one year.',
		'--page',
		'3. Either party may end this agreement in writing.|The color of the goods is grey.'
	);
	return { old, nw };
}

const rows = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[data-compare-row]')].map((r) => r.getAttribute('aria-label') ?? '')
	);

test('Compare opens two documents, lists and marks their differences, and saves a copy and a report', async () => {
	const { old, nw } = versions();
	const marked = join(OUT, 'compare-old (differences).pdf');
	const report = join(OUT, 'compare report.pdf');
	for (const p of [marked, report]) rmSync(p, { force: true });
	const before = readFileSync(old);
	const { browser, stop } = await launch([], { dialogs: [[old], [nw], [marked], [report]] });
	try {
		const tile = await browser.$('[data-tool="compare"]');
		await tile.waitForDisplayed({ timeoutMsg: 'the start screen has no Compare' });
		await tile.click();
		const tab = await browser.$('[data-compare-tab]');
		await tab.waitForDisplayed({ timeoutMsg: 'Compare has no tab of its own' });
		assert.equal(await tab.getAttribute('aria-selected'), 'true');

		// Both slots empty; each opens its file from the Open dialog.
		for (const slot of [0, 1]) {
			const open = await browser.$(`[data-compare-slot="${slot}"] .button-primary`);
			await open.waitForClickable({ timeoutMsg: `slot ${slot} has no Open button` });
			await open.click();
		}
		await browser.waitUntil(
			() =>
				browser.execute(
					() =>
						document.querySelector('[data-compare-pane="old"] .page img, [data-compare-pane="old"] .page canvas') !== null &&
						document.querySelector('[data-compare-pane="new"] .page img, [data-compare-pane="new"] .page canvas') !== null
				),
			{ timeout: 20_000, timeoutMsg: 'the two documents did not show their pages' }
		);
		await browser.saveScreenshot(join(OUT, 'compare-opened.png'));

		// Nothing to list yet: the Differences tab is unavailable.
		assert.equal(await (await browser.$('[data-compare-panel-tab="differences"]')).isEnabled(), false, 'Differences opens before comparing');

		await (await browser.$('[data-compare-run]')).click();
		await browser.waitUntil(async () => (await rows(browser)).length > 0, {
			timeout: 30_000,
			timeoutMsg: 'comparing listed no differences'
		});
		const listed = await rows(browser);
		assert.equal(listed.length, 6, JSON.stringify(listed));
		assert.ok(listed.some((r) => r.startsWith('Changed: Friday → Monday')), JSON.stringify(listed));
		assert.ok(listed.some((r) => r.startsWith('Changed: colour → color')), JSON.stringify(listed));
		const sections = await browser.execute(() =>
			[...document.querySelectorAll('.compare-section')].map((s) => s.textContent.replace(/\s+/g, ' ').trim())
		);
		assert.deepEqual(sections, ['Body text 3', 'Headers and footers 3']);
		// The first difference is selected and marked in both documents.
		await browser.waitUntil(
			() => browser.execute(() => document.querySelectorAll('[data-diff-mark].diff-current').length >= 2),
			{ timeout: 5000, timeoutMsg: 'the selected difference is not marked in both documents' }
		);
		await browser.pause(800);
		await browser.saveScreenshot(join(OUT, 'compare-differences.png'));

		// The next difference, from the keyboard.
		const position = () => browser.execute(() => document.querySelector('[data-compare-panes]')?.previousElementSibling?.textContent ?? '');
		await browser.execute(() => document.querySelector('.compare-list')?.focus());
		await keys(browser, ['Control', 'ArrowRight']);
		await browser.waitUntil(async () => /2 of 6/.test(await position()), { timeout: 5000, timeoutMsg: 'Ctrl+Right did not go on' });

		// Ignored: out of the list.
		await keys(browser, ['Delete']);
		await browser.waitUntil(async () => (await rows(browser)).length === 5, { timeout: 5000, timeoutMsg: 'Delete did not ignore' });
		assert.match(await (await browser.$('#compare-differences')).getText(), /1 ignored/);

		// The filter: only the kinds picked are listed and marked, until it is cleared.
		const shown = await rows(browser);
		const changed = shown.filter((r) => r.startsWith('Changed')).length;
		assert.ok(changed > 0 && changed < shown.length, JSON.stringify(shown));
		await (await browser.$('button[aria-label="Filter the differences"]')).click();
		await (await browser.$('[aria-label="Filters"] button[aria-label="Changed"]')).click();
		await browser.waitUntil(async () => (await rows(browser)).length === changed, { timeout: 5000, timeoutMsg: 'the filter did not leave the changes alone' });
		assert.ok((await rows(browser)).every((r) => r.startsWith('Changed')));
		const marksShown = await browser.execute(() => document.querySelectorAll('[data-diff-mark]').length);
		await (await browser.$('button=Clear filters')).click();
		await browser.waitUntil(async () => (await rows(browser)).length === shown.length, { timeout: 5000, timeoutMsg: 'Clear filters did not list them all' });
		await browser.waitUntil(() => browser.execute((n) => document.querySelectorAll('[data-diff-mark]').length > n, marksShown), {
			timeout: 5000,
			timeoutMsg: 'the pages kept the marks the filter left out away'
		});
		await (await browser.$('button[aria-label="Filter the differences"]')).click();

		// The panel's tabs are the side panes' kind: icons with tooltips, the arrow keys
		// moving between them.
		const differencesTab = await browser.$('[data-compare-panel-tab="differences"]');
		assert.equal(await differencesTab.getAttribute('aria-label'), 'Differences');
		await differencesTab.click();
		await keys(browser, ['ArrowLeft']);
		await browser.waitUntil(async () => (await browser.$('#compare-options')).isExisting(), { timeoutMsg: 'ArrowLeft did not show the Compare tab' });
		assert.equal(await browser.execute(() => document.activeElement?.getAttribute('data-compare-panel-tab')), 'compare');
		await keys(browser, ['ArrowRight']);
		await browser.waitUntil(async () => (await browser.$('#compare-differences')).isExisting(), { timeoutMsg: 'ArrowRight did not show Differences' });

		// The bar's contents are centered, as the view bar's.
		const off = await browser.execute(() => {
			const bar = document.querySelector('[role="toolbar"][aria-label="Compare"]');
			const items = [...bar.children].map((c) => c.getBoundingClientRect()).filter((r) => r.width > 0);
			const b = bar.getBoundingClientRect();
			return Math.abs((items[0].left + items.at(-1).right) / 2 - (b.left + b.right) / 2);
		});
		assert.ok(off <= 1, `the bar's contents are ${off} px off its centre`);

		assert.deepEqual(await unnamedControls(browser), [], 'controls without an accessible name');

		// The first document with the differences marked, and the report.
		await (await browser.$('[data-compare-pane="old"] .compare-header button')).click();
		await browser.waitUntil(() => existsSync(marked), { timeout: 15_000, timeoutMsg: 'the annotated copy was not written' });
		await browser.waitUntil(async () => /with 5 differences marked/.test(await notices(browser)), {
			timeout: 5000,
			timeoutMsg: 'no notification of the copy'
		});
		const info = cli('info', marked);
		assert.equal((info.match(/annotation p\d+: (Highlight|Text)/g) ?? []).length, 5, info);
		assert.deepEqual(readFileSync(old), before, 'the document compared changed');
		// The copy opens in a tab of its own, shown; Compare's tab takes the view back.
		await browser.waitUntil(
			async () => (await browser.$('[role=tab][aria-selected="true"]').getText().catch(() => '')).includes('compare-old (differences).pdf'),
			{ timeout: 10_000, timeoutMsg: 'the copy did not open in a tab' }
		);
		await waitForDocument(browser);
		await (await browser.$('[data-compare-tab]')).click();
		await (await browser.$('button[aria-label="Save the report"]')).waitForClickable({ timeoutMsg: 'Compare did not come back as it was' });

		await (await browser.$('button[aria-label="Save the report"]')).click();
		await browser.waitUntil(() => existsSync(report), { timeout: 15_000, timeoutMsg: 'the report was not written' });
		// The report opens in a tab of its own, shown.
		await browser.waitUntil(
			async () => (await browser.$('[role=tab][aria-selected="true"]').getText().catch(() => '')).includes('compare report.pdf'),
			{ timeout: 10_000, timeoutMsg: 'the report did not open in a tab' }
		);
		await waitForDocument(browser);

		// Closing the Compare tab closes the view; the report stays open.
		await (await browser.$('[data-compare-tab] [data-close]')).click();
		await (await browser.$('[data-compare-tab]')).waitForExist({ reverse: true, timeoutMsg: 'closing Compare left its tab' });
		assert.ok((await browser.$('[role=tab][aria-selected="true"]').getText()).includes('compare report.pdf'));
	} finally {
		await stop();
	}
});
