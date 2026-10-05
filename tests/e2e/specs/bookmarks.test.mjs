// Bookmarks (AGENTS.md section 6.2): add with Ctrl+B, rename inline and with F2,
// nest by dragging, undo and redo, collapse, save, reopen.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { outlineLines, sample } from '../lib/pdfcli.mjs';

async function statusText(browser) {
	return browser.execute(() => [...document.querySelectorAll('[data-view-bar], footer')].map((e) => e.textContent).join(' '));
}

/** Visible tree rows as [title, level, expanded]. */
async function rows(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[role=treeitem]')]
			.sort((a, b) => a.offsetTop - b.offsetTop)
			.map((r) => [r.textContent.trim(), Number(r.getAttribute('aria-level')), r.getAttribute('aria-expanded')])
	);
}

async function waitForRows(browser, expected, message) {
	await browser.waitUntil(async () => JSON.stringify(await rows(browser)) === JSON.stringify(expected), {
		timeout: 5000,
		timeoutMsg: `${message}: got ${JSON.stringify(await rows(browser))}`
	});
}

async function row(browser, title) {
	return browser.$(`.tree-row*=${title}`);
}

/** Types a new title into the inline rename field and confirms it. */
async function typeTitle(browser, title) {
	const input = await browser.$('input.tree-rename');
	await input.waitForDisplayed({ timeout: 5000 });
	// The field opens with its text selected, so typing replaces it.
	await browser.keys([...title]);
	await browser.keys('Enter');
}

async function goToPage(browser, page) {
	await browser.keys(['Control', 'g']);
	await browser.keys([...String(page), 'Enter']);
	await browser.waitUntil(async () => (await statusText(browser)).includes(`${page} of`), {
		timeoutMsg: `did not reach page ${page}`
	});
}

test('add, rename, nest, undo, save and reopen bookmarks', async () => {
	const path = sample('bookmarks.pdf', 5);

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await (await browser.$('button[aria-label="Bookmarks"]')).click();
		await (await browser.$('p*=No bookmarks yet')).waitForDisplayed();

		// Ctrl+B with no text selected: titled after the page, open for renaming. (No click
		// on the page first: WebDriver would scroll it, and the bookmark records the view.)
		await browser.keys(['Control', 'b']);
		assert.equal(await (await browser.$('input.tree-rename')).getValue(), 'Page 1');
		await typeTitle(browser, 'Chapter One');
		await waitForRows(browser, [['Chapter One', 1, null]], 'first bookmark');
		assert.match(await statusText(browser), /Unsaved changes/);

		// A second one on page 3 goes after the selected one.
		await goToPage(browser, 3);
		await browser.keys(['Control', 'b']);
		await typeTitle(browser, 'Chapter Three');
		await waitForRows(browser, [['Chapter One', 1, null], ['Chapter Three', 1, null]], 'second bookmark');

		// F2 renames the selected bookmark.
		await (await row(browser, 'Chapter Three')).click();
		await browser.keys('F2');
		await typeTitle(browser, 'Chapter 3');
		await waitForRows(browser, [['Chapter One', 1, null], ['Chapter 3', 1, null]], 'renamed');

		// Drag "Chapter 3" onto the middle of "Chapter One": it becomes its child.
		const source = await row(browser, 'Chapter 3');
		const target = await row(browser, 'Chapter One');
		await browser
			.action('pointer')
			.move({ origin: source })
			.down()
			.move({ origin: 'pointer', x: 0, y: -8, duration: 50 })
			.move({ origin: target, duration: 100 })
			.up()
			.perform();
		await waitForRows(browser, [['Chapter One', 1, 'true'], ['Chapter 3', 2, null]], 'nested by dragging');

		// Undo and redo the move.
		await browser.keys(['Control', 'z']);
		await waitForRows(browser, [['Chapter One', 1, null], ['Chapter 3', 1, null]], 'undo');
		await browser.keys(['Control', 'y']);
		await waitForRows(browser, [['Chapter One', 1, 'true'], ['Chapter 3', 2, null]], 'redo');

		// Collapse "Chapter One" (saved with the document) and save.
		await (await (await row(browser, 'Chapter One')).$('[data-toggle]')).click();
		await waitForRows(browser, [['Chapter One', 1, 'false']], 'collapsed');
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), {
			timeout: 10_000,
			timeoutMsg: 'saving did not finish'
		});
	} finally {
		await stop();
	}

	// On disk: nested, collapsed ("+"), explicit destinations at the top of pages 1 and 3.
	assert.deepEqual(outlineLines(path), [
		'  + Chapter One -> page 1 at 0,0',
		'      Chapter 3 -> page 3 at 0,0'
	]);

	({ browser, stop } = await launch([path]));
	try {
		await waitForDocument(browser);
		await (await browser.$('button[aria-label="Bookmarks"]')).click();
		await waitForRows(browser, [['Chapter One', 1, 'false']], 'reopened collapsed');
		// Expanding is not an edit.
		await (await (await row(browser, 'Chapter One')).$('[data-toggle]')).click();
		await waitForRows(browser, [['Chapter One', 1, 'true'], ['Chapter 3', 2, null]], 'expanded');
		assert.match(await statusText(browser), /All changes saved/);
		// A click follows the bookmark.
		await (await row(browser, 'Chapter 3')).click();
		await browser.waitUntil(async () => (await statusText(browser)).includes("3 of 5"), {
			timeoutMsg: 'clicking the bookmark did not go to page 3'
		});
	} finally {
		await stop();
	}
});
