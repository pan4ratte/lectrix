// Compress (section 6.11, ADR 0027), from the start screen and File > Compress…: the
// dialog shows what the file's size is made of and the settings, compresses with progress
// (Stop writes nothing), shows the result with a before-and-after preview, and saves it as
// a copy or over the file, which then opens in a tab (or the tab showing it reads it again).

import assert from 'node:assert/strict';
import { existsSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { tabWalk, unnamedControls } from '../lib/a11y.mjs';
import { OUT, keys, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli } from '../lib/pdfcli.mjs';

const notices = (browser) =>
	browser.execute(() => [...document.querySelectorAll('[role="status"]')].map((n) => n.textContent.trim()).join('\n'));
const activeTab = (browser) =>
	browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.trim() ?? '');
const step = (browser) => browser.execute(() => document.querySelector('[data-compress-dialog]')?.getAttribute('data-step') ?? null);

/** The generated sample whose images cover what Compress handles. */
function imageSample(name) {
	const path = join(OUT, name);
	rmSync(path, { force: true });
	cli('gen-images', path);
	return path;
}

/** Every Tab stop in the dialog is named and shows focus (AGENTS.md section 8); returns
 * their names. */
async function assertKeyboard(browser, where) {
	const stops = (await tabWalk(browser, { max: 60 })).filter((s) => s.tag !== 'none');
	for (const s of stops) {
		assert.ok(s.name, `${where}: an unnamed Tab stop (${s.tag} .${s.cls})`);
		assert.ok(s.visibleFocus, `${where}: focus not visible on "${s.name}"`);
	}
	assert.deepEqual(await unnamedControls(browser), [], `${where}: controls without an accessible name`);
	return stops.map((s) => s.name);
}

async function waitForStep(browser, wanted, timeoutMsg) {
	await browser.waitUntil(async () => (await step(browser)) === wanted, { timeout: 30_000, timeoutMsg });
}

test('Compress shows the file’s sizes and settings, compresses with progress, previews, and saves a copy', async () => {
	const doc = imageSample('compress-images.pdf');
	const out = join(OUT, 'compress-images (compressed).pdf');
	rmSync(out, { force: true });
	const before = statSync(doc).size;
	const { browser, stop } = await launch([], {
		dialogs: [[doc], [out]],
		// Slow enough to see the progress.
		env: { LECTRIX_COMPRESS_IMAGE_DELAY_MS: '150' }
	});
	try {
		const tile = await browser.$('[data-tool="compress"]');
		await tile.waitForDisplayed({ timeoutMsg: 'the start screen has no Compress' });
		assert.equal((await (await tile.$('.start-tool-label')).getText()).trim(), 'Compress');
		await tile.click();

		await waitForStep(browser, 'settings', 'the Compress dialog did not open');
		const text = await (await browser.$('[data-compress-dialog]')).getText();
		assert.match(text, /Compress compress-images\.pdf/);
		assert.match(text, /3 pages/);
		assert.match(text, /Images/, 'the sizes do not name the images');
		assert.match(text, /Balanced/, 'the dialog does not start with Balanced');
		await browser.pause(200);
		await browser.saveScreenshot(join(OUT, 'compress-settings.png'));
		const settings = await assertKeyboard(browser, 'Compress settings');
		for (const name of ['Preset', 'Downsampling method', 'Compression', 'Quality', 'Colour of images', 'Cancel', 'Compress']) {
			assert.ok(settings.includes(name), `the settings reach ${name}: ${JSON.stringify(settings)}`);
		}
		assert.ok(!settings.includes('File'), 'Tab left the Compress dialog');

		await (await browser.$('[data-compress-run]')).click();
		await waitForStep(browser, 'running', 'compressing did not start');
		await browser.waitUntil(
			async () => /Compressing images… \d+ of \d+/.test(await (await browser.$('[data-compress-progress]')).getText()),
			{ timeout: 30_000, timeoutMsg: 'the progress does not count images' }
		);
		await browser.saveScreenshot(join(OUT, 'compress-progress.png'));

		await waitForStep(browser, 'result', 'compressing did not finish');
		const headline = await (await browser.$('[data-compress-headline]')).getText();
		assert.match(headline, /→ .*% smaller/);
		// Both panes of the preview show their page.
		await browser.waitUntil(
			() =>
				browser.execute(
					() =>
						[...document.querySelectorAll('[data-compress-result] img')].filter((i) => i.complete && i.naturalWidth > 0)
							.length === 2
				),
			{ timeoutMsg: 'the preview does not show both pages' }
		);
		await browser.saveScreenshot(join(OUT, 'compress-result.png'));
		const result = await assertKeyboard(browser, 'Compress result');
		for (const name of ['Next page', 'The page as it is', 'The page compressed', 'Back to settings', 'Save as a copy', 'Save']) {
			assert.ok(result.includes(name), `the result reaches ${name}: ${JSON.stringify(result)}`);
		}

		await (await browser.$('[data-compress-copy]')).click();
		await waitForDocument(browser);
		assert.match(await activeTab(browser), /compress-images \(compressed\)\.pdf/);
		assert.match(await statusText(browser), /of 3/);
		await browser.waitUntil(async () => (await notices(browser)).includes('Saved compress-images (compressed).pdf:'), {
			timeoutMsg: 'nothing said the copy was saved'
		});
		assert.ok(existsSync(out), 'no copy was written');
		assert.ok(statSync(out).size * 5 < before, 'the copy is not much smaller');
		assert.equal(statSync(doc).size, before, 'the original was changed');
		assert.equal(await step(browser), null, 'the dialog is still open');
	} finally {
		await stop();
	}
});

test('Stop writes nothing; File > Compress… compresses the document shown, over its file', async () => {
	const doc = imageSample('compress-open.pdf');
	const before = statSync(doc).size;
	const { browser, stop } = await launch([doc], { env: { LECTRIX_COMPRESS_IMAGE_DELAY_MS: '300' } });
	try {
		await waitForDocument(browser);
		const tabs = () => browser.execute(() => document.querySelectorAll('[role="tab"]').length);
		const open = await tabs();

		// File > Compress… with the document shown: no Open dialog.
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await browser.$('[role="menuitem"]*=Compress…')).click();
		await waitForStep(browser, 'settings', 'the Compress dialog did not open for the document shown');

		await (await browser.$('[data-compress-run]')).click();
		await waitForStep(browser, 'running', 'compressing did not start');
		await (await browser.$('[data-compress-stop]')).click();
		await waitForStep(browser, 'settings', 'Stop did not bring the settings back');
		assert.equal(statSync(doc).size, before, 'Stop wrote the file');

		// Esc closes the dialog, writing nothing; Compress again, and Save.
		await keys(browser, ['Escape']);
		await browser.waitUntil(async () => (await step(browser)) === null, { timeoutMsg: 'Esc did not close the dialog' });
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await browser.$('[role="menuitem"]*=Compress…')).click();
		await waitForStep(browser, 'settings', 'the Compress dialog did not open again');
		await (await browser.$('[data-compress-run]')).click();
		await waitForStep(browser, 'result', 'compressing did not finish');
		await (await browser.$('[data-compress-save]')).click();
		await browser.waitUntil(async () => (await notices(browser)).includes('compress-open.pdf is now'), {
			timeoutMsg: 'nothing said the file was compressed'
		});
		assert.ok(statSync(doc).size * 5 < before, 'the file is not much smaller');
		assert.equal(await tabs(), open, 'the file opened in a second tab');
		// The watcher looks every 1.5 s: its own save is not a change by another program.
		await browser.pause(2000);
		assert.equal(
			await browser.execute(() => document.body.innerText.includes('Another program changed this file')),
			false,
			'compressing was reported as a change by another program'
		);
		await waitForDocument(browser);
		assert.match(await statusText(browser), /of 3/);
	} finally {
		await stop();
	}
});
