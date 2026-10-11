// PDF/A files (AGENTS.md section 5.4): a file that claims PDF/A opens for reading only, with
// a banner that says so and offers to enable editing; Settings can open such files ready to
// edit instead. Saving bookmarks keeps the claim; saving an annotation takes it out of the
// file, and says so.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, outlineLines, sample } from '../lib/pdfcli.mjs';

const banner = (browser) => browser.$('[data-pdfa-banner]');
const toolEnabled = async (browser, label) => (await browser.$(`button[aria-label="${label}"]`)).isEnabled();

/** Viewport pixels of a point of page `index`, given in page points. */
async function pagePoint(browser, index, x, y) {
	const r = await browser.execute(
		(i) => document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect().toJSON(),
		index
	);
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

async function save(browser) {
	await browser.keys(['Control', 's']);
	await browser.waitUntil(async () => /All changes saved/.test(await statusText(browser)), {
		timeoutMsg: 'the document was not saved'
	});
}

test('a PDF/A file opens for reading only until editing is enabled', async () => {
	const path = sample('pdfa.pdf', 3, ['--pdfa', '1b']);
	assert.match(cli('info', path), /claims: PDF\/A-1b/);

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await (await banner(browser)).waitForDisplayed({ timeoutMsg: 'no banner for a PDF/A file' });
		const text = await (await banner(browser)).getText();
		assert.match(text, /PDF\/A standard \(PDF\/A-1b\)/);
		assert.match(text, /opened for reading only/);

		// Nothing that changes the document is on offer.
		assert.equal(await toolEnabled(browser, 'Select'), true);
		for (const tool of ['Highlight', 'Note', 'Pen', 'Text box']) {
			assert.equal(await toolEnabled(browser, tool), false, `${tool} is disabled`);
		}
		// A shortcut that would change it says why it does not, and how to change that.
		await browser.keys(['Control', 'b']);
		const told = await browser.$('//*[@role="status"][contains(., "is open for reading only, because it is a PDF/A file")]');
		await told.waitForDisplayed({ timeoutMsg: 'Ctrl+B did not say why it did nothing' });
		assert.equal(await (await browser.$('input.tree-rename')).isExisting(), false, 'no bookmark was added');
		assert.match(await statusText(browser), /All changes saved/);

		await (await (await banner(browser)).$('button=Enable editing')).click();
		await (await banner(browser)).waitForExist({ reverse: true, timeoutMsg: 'the banner stayed after Enable editing' });
		assert.equal(await toolEnabled(browser, 'Highlight'), true);

		// Now it is a document like any other, and saving keeps its claim.
		await browser.keys(['Control', 'b']);
		const rename = await browser.$('input.tree-rename');
		await rename.waitForDisplayed({ timeoutMsg: 'Ctrl+B added no bookmark after Enable editing' });
		await browser.keys([...'Archived', 'Enter']);
		await browser.waitUntil(async () => /Unsaved changes/.test(await statusText(browser)), {
			timeoutMsg: 'the bookmark did not change the document'
		});
		await save(browser);
		assert.equal(await (await banner(browser)).isExisting(), false, 'saving does not bring the banner back');
		// A bookmark can't break the standard: the file still claims it.
		assert.match(cli('info', path), /claims: PDF\/A-1b/);
		assert.equal(
			await (await browser.$('//*[@role="status"][contains(., "no longer claims")]')).isExisting(),
			false,
			'nothing was said about the claim'
		);

		// A note may: saving it takes the claim out of the file, and says so.
		await browser.keys('n');
		const at = await pagePoint(browser, 0, 300, 200);
		await browser.action('pointer').move({ ...at, origin: 'viewport' }).down().up().perform();
		await (await browser.$('aside[aria-label="Annotation comment"] textarea')).waitForDisplayed({
			timeoutMsg: 'the note was not placed'
		});
		await browser.keys([...'Checked']);
		await browser.keys(['Escape']);
		await browser.waitUntil(async () => /Unsaved changes/.test(await statusText(browser)), {
			timeoutMsg: 'the note did not change the document'
		});
		await save(browser);
		const removed = await browser.$('//*[@role="status"][contains(., "pdfa.pdf no longer claims to be PDF/A-1b.")]');
		await removed.waitForDisplayed({ timeoutMsg: 'saving a note did not say that the claim was removed' });
	} finally {
		await stop();
	}
	assert.deepEqual(outlineLines(path).map((l) => l.trim().split(' ->')[0]), ['Archived']);
	const info = cli('info', path);
	assert.doesNotMatch(info, /claims:/);
	assert.match(info, /annotation p1: Text/);
});

test('Rust refuses to change a document it opened for reading only', async () => {
	const path = sample('pdfa-refused.pdf', 2, ['--pdfa', '3a']);
	const { browser, stop } = await launch([path]);
	// The webview asking Rust directly, as no control of the app does while the banner shows.
	// The first document opened after a start has id 1.
	const rotate = () =>
		browser.executeAsync((done) => {
			window.__TAURI_INTERNALS__
				.invoke('apply_operation', { id: 1, operation: { kind: 'rotatePages', pages: [0], degrees: 90 } })
				.then(
					() => done('applied'),
					(e) => done(String(e?.message ?? e))
				);
		});
	try {
		await waitForDocument(browser);
		await (await banner(browser)).waitForDisplayed({ timeoutMsg: 'no banner for a PDF/A file' });
		assert.match(await rotate(), /open for reading only/);
		await (await (await banner(browser)).$('button=Enable editing')).click();
		await (await banner(browser)).waitForExist({ reverse: true, timeoutMsg: 'the banner stayed after Enable editing' });
		assert.equal(await rotate(), 'applied');
	} finally {
		await stop();
	}
});

test('the Debug menu rehearses the PDF/A banner on any document', async () => {
	const path = sample('pdfa-rehearsal.pdf', 2);
	const { browser, stop } = await launch([path], { env: { LECTRIX_DEBUG: '1' } });
	const debugItem = async (item) => {
		await (await browser.$('button=Debug')).click();
		await (await browser.$(`//*[@role="menuitem"][normalize-space(.)="${item}"]`)).click();
	};
	try {
		await waitForDocument(browser);
		assert.equal(await (await banner(browser)).isExisting(), false, 'a file without the claim has no banner');
		await debugItem('PDF/A file, for reading only');
		await (await banner(browser)).waitForDisplayed({ timeoutMsg: 'the rehearsal showed no banner' });
		assert.match(await (await banner(browser)).getText(), /\(PDF\/A-1b\)/);
		assert.equal(await toolEnabled(browser, 'Highlight'), false);

		await debugItem('Clear all');
		await (await banner(browser)).waitForExist({ reverse: true, timeoutMsg: 'Clear all left the banner' });
		assert.equal(await toolEnabled(browser, 'Highlight'), true);
	} finally {
		await stop();
	}
});

test('Settings can open PDF/A files ready to edit', async () => {
	const first = sample('pdfa-first.pdf', 2, ['--pdfa', '2u']);
	const second = sample('pdfa-second.pdf', 2, ['--pdfa', '2u']);
	const { browser, stop } = await launch([first], { dialogs: [second] });
	try {
		await waitForDocument(browser);
		await (await banner(browser)).waitForDisplayed({ timeoutMsg: 'no banner for a PDF/A file' });

		await browser.keys(['Control', ',']);
		const setting = await browser.$('#setting-pdfaReadOnly');
		await setting.waitForDisplayed({ timeoutMsg: 'Settings has no PDF/A switch under General' });
		assert.equal(await setting.getAttribute('aria-checked'), 'true', 'on after installation');
		await setting.click();
		await browser.waitUntil(async () => (await setting.getAttribute('aria-checked')) === 'false', {
			timeoutMsg: 'the switch did not turn off'
		});
		await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Settings did not close'
		});
		// The document that is open stays as it was opened.
		assert.equal(await (await banner(browser)).isDisplayed(), true);

		await browser.keys(['Control', 'o']);
		await browser.waitUntil(
			async () => ((await (await browser.$('[role="tab"][aria-selected="true"]')).getText()) ?? '').includes('pdfa-second.pdf'),
			{ timeoutMsg: 'the second file did not open' }
		);
		await waitForDocument(browser);
		assert.equal(await (await banner(browser)).isExisting(), false, 'opened ready to edit: no banner');
		assert.equal(await toolEnabled(browser, 'Highlight'), true);
	} finally {
		await stop();
	}
});
