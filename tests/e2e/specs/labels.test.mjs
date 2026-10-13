// Page labels (AGENTS.md section 6.3), set in a dialog over the grid of pages being
// organized (section 6.7), and offered nowhere while a document is read: a range of pages
// typed in the dialog and shown live before it is
// applied, the pages selected in the grid numbered with a style and a prefix while the
// others keep their labels, the numbering of the page before carried on, undo and redo,
// save, reopen, and the page box finding a page by its label.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, organizePages, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

async function waitForStatus(browser, text, message) {
	await browser.waitUntil(async () => (await statusText(browser)).includes(text), {
		timeout: 5000,
		timeoutMsg: `${message}: the window shows ${JSON.stringify(await statusText(browser))}`
	});
}

/** What the grid of pages says under each page: its label and its place. */
async function captions(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[data-organize-view] [role=option]')]
			.sort((a, b) => Number(a.getAttribute('aria-posinset')) - Number(b.getAttribute('aria-posinset')))
			.map((c) => c.querySelector('[data-page-caption]')?.textContent.trim())
	);
}

async function waitForCaptions(browser, expected, message) {
	await browser.waitUntil(async () => JSON.stringify(await captions(browser)) === JSON.stringify(expected), {
		timeout: 5000,
		timeoutMsg: `${message}: the pages say ${JSON.stringify(await captions(browser))}`
	});
}

const page = (n) => `[data-organize-view] [role=option][aria-posinset="${n}"] [data-picture]`;

/** Selects pages `from` to `to` (1-based) in the grid. */
async function selectPages(browser, from, to = from) {
	await (await browser.$(page(from))).click();
	if (to !== from) {
		await browser.performActions([
			{ type: 'key', id: 'keyboard', actions: [{ type: 'keyDown', value: '' }] }
		]);
		await (await browser.$(page(to))).click();
		await browser.releaseActions();
	}
}

const DIALOG = '[data-labels-dialog]';
const organizing = (browser) => browser.execute(() => document.querySelector('[data-organize-view]') !== null);

async function waitForDialog(browser) {
	const dialog = await browser.$(DIALOG);
	await dialog.waitForDisplayed({ timeoutMsg: 'the Page labels dialog did not open' });
	return dialog;
}


/** The bar's Labels button, in the grid. */
async function labelsFromBar(browser) {
	await (await browser.$('[data-organize-bar] button[aria-label="Page labels"]')).click();
	return waitForDialog(browser);
}

/** Picks `option` in the dialog's style list (Dropdown.svelte: a button opening a list). */
async function chooseStyle(browser, option) {
	await (await browser.$(`${DIALOG} [data-select-trigger]`)).click();
	await (await browser.$(`//*[@role="option"][normalize-space()="${option}"]`)).click();
}

/** The dialog's choice whose words include `text`. */
async function pick(browser, text) {
	await (await (await browser.$(DIALOG)).$(`label*=${text}`)).click();
}

const preview = async (browser) => (await (await browser.$('[data-labels-preview]')).getText()).replace(/\s+/g, ' ');

async function apply(browser) {
	await (await browser.$(`${DIALOG} button[type=submit]`)).click();
	await (await browser.$(DIALOG)).waitForExist({ reverse: true, timeoutMsg: 'the dialog stayed after Apply' });
}

/** The labels on disk, as `pdf-cli info` prints its rules. */
function labelRules(path) {
	return cli('info', path)
		.split('\n')
		.filter((l) => l.startsWith('  from page'))
		.map((l) => l.trim());
}

const PLAIN = ['1', '2', '3', '4', '5', '6', '7', '8'];

test('number pages in the dialog, preview, undo, save and reopen', async () => {
	const path = sample('labels.pdf', 8);

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		// While reading, nothing offers page labels: not the Document menu, nor the menu of
		// a page in the Pages panel. They belong to the grid of pages.
		await (await browser.$('button=Document')).click();
		const documentMenu = await browser.$('[role=menu]');
		await documentMenu.waitForDisplayed();
		assert.doesNotMatch(await documentMenu.getText(), /label/i);
		await browser.keys('Escape');
		await documentMenu.waitForExist({ reverse: true });
		await (await browser.$('[role=option][aria-label="Page 2"]')).click({ button: 'right' });
		const pageMenu = await browser.$('[role=menu]');
		await pageMenu.waitForDisplayed({ timeoutMsg: 'a thumbnail has no menu' });
		assert.doesNotMatch(await pageMenu.getText(), /label/i);
		await browser.keys('Escape');
		await pageMenu.waitForExist({ reverse: true });

		// In the grid, with nothing selected, the dialog asks for a range, starting at the
		// page the document was at.
		await organizePages(browser);
		await browser.waitUntil(async () => (await captions(browser)).length === 8, { timeoutMsg: 'not every page is in the grid' });
		let dialog = await labelsFromBar(browser);
		assert.match(await dialog.getText(), /Selected pages \(none are selected\)/);
		assert.match(await dialog.getText(), /All 8 pages/);
		assert.equal(await (await browser.$(`${DIALOG} input[aria-label="First page"]`)).getValue(), '1');
		assert.equal(await (await browser.$(`${DIALOG} input[aria-label="Last page"]`)).getValue(), '1');

		// Pages 1 to 3 in roman numerals: the dialog says what they become, and the grid's
		// pages show it, before anything is applied.
		await (await browser.$(`${DIALOG} input[aria-label="Last page"]`)).setValue('3');
		await chooseStyle(browser, 'i, ii, iii');
		await browser.waitUntil(async () => (await preview(browser)) === 'Preview: i, ii, iii', {
			timeoutMsg: `the preview says ${await preview(browser)}`
		});
		await waitForCaptions(browser, ['i (1)', 'ii (2)', 'iii (3)', '4', '5', '6', '7', '8'], 'live preview');
		await browser.saveScreenshot(join(OUT, 'labels-dialog.png'));
		// A range that is none says why, and can't be applied.
		await (await browser.$(`${DIALOG} input[aria-label="Last page"]`)).setValue('9');
		await (await browser.$(`${DIALOG} [role=alert]`)).waitForDisplayed({ timeoutMsg: 'pages 1 to 9 of 8 were taken' });
		assert.equal(await (await browser.$(`${DIALOG} button[type=submit]`)).isEnabled(), false);
		// Cancel leaves the labels alone, and the grid as it was.
		await (await browser.$('button=Cancel')).click();
		await (await browser.$(DIALOG)).waitForExist({ reverse: true, timeoutMsg: 'Cancel did not close the dialog' });
		assert.equal(await organizing(browser), true, 'giving the dialog up left the grid');
		assert.doesNotMatch(await statusText(browser), /Unsaved changes/);
		await waitForCaptions(browser, PLAIN, 'the cancelled preview stayed');

		// With pages selected, the dialog is for those.
		await selectPages(browser, 1, 3);
		dialog = await labelsFromBar(browser);
		assert.match(await dialog.getText(), /Selected: 3 pages \(1–3\)/);
		await chooseStyle(browser, 'i, ii, iii');
		await apply(browser);
		// The pages after them keep their labels: page 4 is still 4.
		await waitForCaptions(browser, ['i (1)', 'ii (2)', 'iii (3)', '4', '5', '6', '7', '8'], 'roman front matter');
		assert.match(await statusText(browser), /Unsaved changes/);

		// A prefix and a first number, from the grid's menu.
		await selectPages(browser, 4, 6);
		await (await browser.$(page(5))).click({ button: 'right' });
		await (await browser.$('[role=menu]')).waitForDisplayed();
		await (await browser.$('div*=Page labels…')).click();
		await waitForDialog(browser);
		// A new section starts as 1, 2, 3 from 1, whatever the pages have now.
		assert.equal((await (await browser.$(`${DIALOG} [data-select-trigger]`)).getText()).trim(), '1, 2, 3');
		assert.equal(await (await browser.$('#labels-first')).getValue(), '1');
		await (await browser.$('#labels-prefix')).setValue('Ch-');
		await browser.waitUntil(async () => (await preview(browser)) === 'Preview: Ch-1, Ch-2, Ch-3', {
			timeoutMsg: `the preview says ${await preview(browser)}`
		});
		// A first number that is none says so.
		await (await browser.$('#labels-first')).setValue('0');
		assert.match(await (await browser.$(`${DIALOG} [role=alert]`)).getText(), /1 or higher/);
		await (await browser.$('#labels-first')).setValue('1');
		await apply(browser);
		await waitForCaptions(browser, ['i (1)', 'ii (2)', 'iii (3)', 'Ch-1 (4)', 'Ch-2 (5)', 'Ch-3 (6)', '7', '8'], 'prefix');

		// The last two pages lettered A, B.
		await selectPages(browser, 7, 8);
		await labelsFromBar(browser);
		await chooseStyle(browser, 'A, B, C');
		await apply(browser);
		const lettered = ['i (1)', 'ii (2)', 'iii (3)', 'Ch-1 (4)', 'Ch-2 (5)', 'Ch-3 (6)', 'A (7)', 'B (8)'];
		await waitForCaptions(browser, lettered, 'lettered');
		await browser.saveScreenshot(join(OUT, 'labels-organize.png'));

		// Page 7 carries on the numbering of the page before it; page 8 keeps its label.
		await selectPages(browser, 7);
		await labelsFromBar(browser);
		await pick(browser, 'Continue the numbering');
		await browser.waitUntil(async () => (await preview(browser)) === 'Preview: Ch-4', {
			timeoutMsg: `the preview says ${await preview(browser)}`
		});
		await apply(browser);
		const carried = ['i (1)', 'ii (2)', 'iii (3)', 'Ch-1 (4)', 'Ch-2 (5)', 'Ch-3 (6)', 'Ch-4 (7)', 'B (8)'];
		await waitForCaptions(browser, carried, 'carried on');

		// Each Apply is one step to undo (focus on the grid: Ctrl+Z in a field undoes typing).
		await browser.keys(['Control', 'z']);
		await waitForCaptions(browser, lettered, 'undo');
		await browser.keys(['Control', 'y']);
		await waitForCaptions(browser, carried, 'redo');
		await browser.keys(['Control', 'z']);
		await waitForCaptions(browser, lettered, 'undo again');

		// The first page has no page before it to carry on from.
		await selectPages(browser, 1);
		dialog = await labelsFromBar(browser);
		assert.equal(await (await browser.$(`${DIALOG} input[type=radio][value=continue]`)).isEnabled(), false);
		// Esc gives the dialog up and leaves the grid as it is: the dialog was opened from it.
		await browser.keys('Escape');
		await (await browser.$(DIALOG)).waitForExist({ reverse: true, timeoutMsg: 'Esc did not close the dialog' });
		assert.equal(await organizing(browser), true, 'Esc in the dialog left the grid');
		await waitForCaptions(browser, lettered, 'the cancelled dialog changed the labels');

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
		// The dialog starts at the page the document was at, as a new section: the grid
		// shows that page numbered 1 meanwhile, and the others as stored.
		await organizePages(browser);
		await browser.waitUntil(async () => (await captions(browser)).length === 8, { timeoutMsg: 'not every page is in the grid' });
		await labelsFromBar(browser);
		assert.equal(await (await browser.$(`${DIALOG} input[aria-label="First page"]`)).getValue(), '5');
		assert.equal(await (await browser.$(`${DIALOG} input[aria-label="Last page"]`)).getValue(), '5');
		await waitForCaptions(browser, ['i (1)', 'ii (2)', 'iii (3)', 'Ch-1 (4)', '1 (5)', 'Ch-3 (6)', 'A (7)', 'B (8)'], 'reopened');

		// Every page numbered 1, 2, 3 again: the document needs no labels at all.
		await pick(browser, 'All 8 pages');
		await apply(browser);
		await waitForCaptions(browser, PLAIN, 'numbered plainly');
		await (await browser.$('button=Done')).click();
		await waitForDocument(browser);
		await browser.keys(['Control', 's']);
		await waitForStatus(browser, 'All changes saved', 'saving');
	} finally {
		await stop();
	}

	assert.deepEqual(labelRules(path), [], 'a document numbered 1, 2, 3 kept its page labels');
});
