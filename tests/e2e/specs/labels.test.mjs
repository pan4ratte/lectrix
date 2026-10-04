// Page labels (AGENTS.md section 6.3): the roman front matter preset, a prefix
// shown live before it is applied, a new range with another style, undo and redo, save,
// reopen, and the page box finding a page by its label.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

async function statusText(browser) {
	return browser.execute(() => document.querySelector('footer')?.textContent ?? '');
}

async function waitForStatus(browser, text, message) {
	await browser.waitUntil(async () => (await statusText(browser)).includes(text), {
		timeout: 5000,
		timeoutMsg: `${message}: status bar shows ${JSON.stringify(await statusText(browser))}`
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

async function goToPage(browser, page) {
	await browser.keys(['Control', 'g']);
	await browser.keys([...String(page), 'Enter']);
	await browser.waitUntil(async () => (await statusText(browser)).includes(`${page} of`), {
		timeoutMsg: `did not reach page ${page}`
	});
}

async function field(browser, label) {
	return (await browser.$(`label*=${label}`)).$('input, select');
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
		await (await browser.$('button[aria-label="Page labels"]')).click();
		await (await browser.$('p*=No page labels yet')).waitForDisplayed();
		await waitForRows(browser, ['1–8 1 – 8'], 'no labels');

		// Roman front matter, then 1, 2, 3 from page 4.
		await goToPage(browser, 4);
		await (await browser.$('button[aria-label="More label actions"]')).click();
		await (await browser.$('div*=Roman front matter')).click();
		await waitForRows(browser, ['1–3 i – iii', '4–8 1 – 5'], 'preset');
		await waitForStatus(browser, '1 (4 of 8)', 'preset');
		assert.match(await statusText(browser), /Unsaved changes/);

		// A prefix shows in the status bar while typing, before it is applied.
		const prefix = await field(browser, 'Prefix');
		await prefix.click();
		await browser.keys([...'Ch-']);
		await waitForStatus(browser, 'Ch-1 (4 of 8)', 'live preview');
		await waitForRows(browser, ['1–3 i – iii', '4–8 Ch-1 – Ch-5'], 'preview in the list');
		// Escape cancels it...
		await browser.keys('Escape');
		await waitForStatus(browser, '1 (4 of 8)', 'cancelled preview');
		// ...and Enter applies it.
		await browser.keys([...'Ch-', 'Enter']);
		await waitForRows(browser, ['1–3 i – iii', '4–8 Ch-1 – Ch-5'], 'prefix applied');

		// A new range at page 7, lettered A, B.
		await goToPage(browser, 7);
		await (await browser.$('button[aria-label="New range from the current page"]')).click();
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 1 – 2'], 'new range');
		await (await field(browser, 'Style')).selectByVisibleText('A, B, C');
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'lettered');
		await waitForStatus(browser, 'A (7 of 8)', 'lettered');

		// Undo and redo the style (focus on the page: Ctrl+Z in a field undoes typing).
		await (await browser.$('.page')).click();
		await browser.keys(['Control', 'z']);
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 1 – 2'], 'undo');
		await browser.keys(['Control', 'y']);
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'redo');

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
		await (await browser.$('button[aria-label="Page labels"]')).click();
		await waitForRows(browser, ['1–3 i – iii', '4–6 Ch-1 – Ch-3', '7–8 A – B'], 'reopened');
		// The page box takes a label.
		await browser.keys(['Control', 'g']);
		await browser.keys([...'Ch-2', 'Enter']);
		await waitForStatus(browser, 'Ch-2 (5 of 8)', 'page box by label');
	} finally {
		await stop();
	}
});
