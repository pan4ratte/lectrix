// Page labels (AGENTS.md section 6.3), beside the grid of pages being organized (section
// 6.7): the roman front matter preset, a prefix shown live before it is applied, a new
// range with another style, the list and the grid selecting for each other, undo and
// redo, save, reopen, and the page box finding a page by its label.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

async function waitForStatus(browser, text, message) {
	await browser.waitUntil(async () => (await statusText(browser)).includes(text), {
		timeout: 5000,
		timeoutMsg: `${message}: the window shows ${JSON.stringify(await statusText(browser))}`
	});
}

/** The label ranges listed in the panel, as "pages labels". */
async function rows(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[aria-label="Label ranges"] [role=option]')]
			.sort((a, b) => a.offsetTop - b.offsetTop)
			.map((r) => r.textContent.replace(/\s+/g, ' ').trim())
	);
}

async function waitForRows(browser, expected, message) {
	await browser.waitUntil(async () => JSON.stringify(await rows(browser)) === JSON.stringify(expected), {
		timeout: 5000,
		timeoutMsg: `${message}: got ${JSON.stringify(await rows(browser))}`
	});
}

/** What the grid of pages says under page `page` (1-based): its label and its place. */
async function caption(browser, page) {
	return browser.execute(
		(n) => document.querySelector(`[data-organize-view] [role=option][aria-posinset="${n}"] [data-page-caption]`)?.textContent.trim(),
		page
	);
}

async function waitForCaption(browser, page, text, message) {
	await browser.waitUntil(async () => (await caption(browser, page)) === text, {
		timeout: 5000,
		timeoutMsg: `${message}: page ${page} says ${JSON.stringify(await caption(browser, page))}`
	});
}

/** Selects page `page` (1-based) in the grid: what "the selected page" then means. */
async function selectPage(browser, page) {
	await (await browser.$(`[data-organize-view] [role=option][aria-posinset="${page}"] [data-picture]`)).click();
}

const selectedCount = async (browser) => (await browser.$('[data-organize-count]')).getText();

/** Document > Page labels: the grid of pages, with the labels beside it. */
async function showLabels(browser) {
	await (await browser.$('button=Document')).click();
	await (await browser.$('div*=Page labels')).click();
	await (await browser.$('[data-organize-labels]')).waitForDisplayed({ timeoutMsg: 'the page labels did not show' });
}

async function field(browser, label) {
	return (await browser.$(`label*=${label}`)).$('input, [data-select-trigger]');
}

/** Picks `option` in the labelled dropdown (Dropdown.svelte: a button opening a list). */
async function choose(browser, label, option) {
	await (await field(browser, label)).click();
	await (await browser.$(`//*[@role="option"][normalize-space()="${option}"]`)).click();
}

/** The labels on disk, as `pdf-cli info` prints its rules. */
function labelRules(path) {
	return cli('info', path)
		.split('\n')
		.filter((l) => l.startsWith('  from page'))
		.map((l) => l.trim());
}

test('set, preview, undo, save and reopen page labels', async () => {
	const path = sample('labels.pdf', 8);

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		// The labels are beside the grid of pages, which the command shows.
		await showLabels(browser);
		assert.equal(await browser.execute(() => document.querySelector('.viewer-scroll') !== null), false, 'still reading');
		await (await browser.$('p*=No page labels yet')).waitForDisplayed();
		await waitForRows(browser, ['1–8 1 – 8'], 'no labels');
		await waitForCaption(browser, 4, '4', 'no labels');

		// Roman front matter, then 1, 2, 3 from page 4: the page selected in the grid.
		await selectPage(browser, 4);
		await (await browser.$('button[aria-label="More label actions"]')).click();
		await (await browser.$('div*=Roman front matter')).click();
		await waitForRows(browser, ['1–3 i – iii', '4–8 1 – 5'], 'preset');
		await waitForCaption(browser, 4, '1 (4)', 'preset');
		await waitForCaption(browser, 2, 'ii (2)', 'preset');
		assert.match(await statusText(browser), /Unsaved changes/);

		// A prefix shows under the pages while typing, before it is applied.
		const prefix = await field(browser, 'Prefix');
		await prefix.click();
		await browser.keys([...'Ch-']);
		await waitForCaption(browser, 4, 'Ch-1 (4)', 'live preview');
		await waitForRows(browser, ['1–3 i – iii', '4–8 Ch-1 – Ch-5'], 'preview in the list');
		// Escape cancels it (and does not leave the grid)...
		await browser.keys('Escape');
		await waitForCaption(browser, 4, '1 (4)', 'cancelled preview');
		assert.equal(await (await browser.$('[data-organize-view]')).isExisting(), true, 'Esc in a field left the grid');
		// ...and Enter applies it.
		await browser.keys([...'Ch-', 'Enter']);
		await waitForRows(browser, ['1–3 i – iii', '4–8 Ch-1 – Ch-5'], 'prefix applied');
		await waitForCaption(browser, 4, 'Ch-1 (4)', 'prefix applied');

		// A new range at page 7, lettered A, B.
		await selectPage(browser, 7);
		await (await browser.$('button[aria-label="New range from the selected page"]')).click();
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 1 – 2'], 'new range');
		await choose(browser, 'Style', 'A, B, C');
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'lettered');
		await waitForCaption(browser, 7, 'A (7)', 'lettered');
		await browser.saveScreenshot(join(OUT, 'labels-organize.png'));

		// A range picked in the list selects its pages in the grid, and the pages selected
		// in the grid decide the range the editor shows.
		await (await browser.$('[aria-label="Label ranges"] [role=option][data-start="3"]')).click();
		await browser.waitUntil(async () => (await selectedCount(browser)) === '3 of 8 pages selected', {
			timeoutMsg: `the range's pages are not selected: ${await selectedCount(browser)}`
		});
		assert.equal(await (await field(browser, 'Prefix')).getValue(), 'Ch-');
		await selectPage(browser, 2);
		await browser.waitUntil(async () => (await (await field(browser, 'Prefix')).getValue()) === '', {
			timeoutMsg: 'the editor did not follow the page selected in the grid'
		});

		// Undo and redo the style (focus on the grid: Ctrl+Z in a field undoes typing).
		await browser.keys(['Control', 'z']);
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 1 – 2'], 'undo');
		await browser.keys(['Control', 'y']);
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'redo');

		// The bar's button puts the labels away and shows them again.
		const toggle = await browser.$('[data-organize-bar] button[aria-label="Page labels"]');
		assert.equal(await toggle.getAttribute('aria-pressed'), 'true');
		await toggle.click();
		await (await browser.$('[data-organize-labels]')).waitForExist({ reverse: true, timeoutMsg: 'the labels stayed' });
		await toggle.click();
		await (await browser.$('[data-organize-labels]')).waitForDisplayed();

		// Back to reading: the page box shows the label.
		await (await browser.$('button=Done')).click();
		await waitForDocument(browser);
		await waitForStatus(browser, 'i (1 of 8)', 'back to reading');
		await browser.keys(['Control', 's']);
		await waitForStatus(browser, 'All changes saved', 'saving');
	} finally {
		await stop();
	}

	assert.deepEqual(labelRules(path), [
		'from page 1: LowerRoman prefix "" start 1',
		'from page 4: Decimal prefix "Ch-" start 1',
		'from page 7: UpperLetters prefix "" start 1'
	]);

	({ browser, stop } = await launch([path]));
	try {
		await waitForDocument(browser);
		// The page box takes a label.
		await browser.keys(['Control', 'g']);
		await browser.keys([...'Ch-2', 'Enter']);
		await waitForStatus(browser, 'Ch-2 (5 of 8)', 'page box by label');
		// The labels as stored; the grid opens at the page the document is at.
		await showLabels(browser);
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'reopened');
		await waitForCaption(browser, 5, 'Ch-2 (5)', 'reopened');
	} finally {
		await stop();
	}
});
