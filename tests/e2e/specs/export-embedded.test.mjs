// The images in a document exported (section 6.14, ADR 0035): File > Export > Images in the
// document… opens the dialog, which shows the images on the pages chosen, each once; Export…
// asks where (the Save dialog, answered by LECTRIX_DIALOG) and writes a file an image, JPEG
// as the document keeps it and the rest as PNG; a notification says so. Files already
// there are asked about.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { tabWalk, unnamedControls } from '../lib/a11y.mjs';
import { OUT, launch, waitForDocument } from '../lib/app.mjs';
import { cli } from '../lib/pdfcli.mjs';

const step = (browser) => browser.execute(() => document.querySelector('[data-images-dialog]')?.getAttribute('data-step') ?? null);
const notices = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[role="status"], [role="alert"]')].map((n) => n.textContent.trim()).join('\n')
	);
const summary = (browser) => browser.execute(() => document.querySelector('[data-images-summary]')?.textContent.trim() ?? '');
/** The thumbnails shown, and how many of them have loaded. */
const thumbnails = (browser) =>
	browser.execute(() => {
		const all = [...document.querySelectorAll('[data-images-thumbnail] img')];
		return { shown: all.length, loaded: all.filter((i) => i.complete && i.naturalWidth > 0).length };
	});

/** Clicks what `selector` finds, found again if the menu is redrawn meanwhile (as it is
 * while a document finishes opening). */
async function clickAgain(browser, selector) {
	for (let tries = 0; ; tries++) {
		try {
			await (await browser.$(selector)).click();
			return;
		} catch (e) {
			if (tries >= 4 || !String(e).includes('stale')) throw e;
			await browser.pause(100);
		}
	}
}

async function openDialog(browser) {
	await clickAgain(browser, 'button=File');
	// A redrawn menu closes the submenu; a click on Export only ever opens it, so it is
	// clicked again until the item can be clicked.
	for (let tries = 0; ; tries++) {
		await clickAgain(browser, 'div*=Export');
		try {
			const item = await browser.$('div*=Images in the document');
			await item.waitForExist({ timeout: 1500 });
			await item.click();
			break;
		} catch (e) {
			if (tries >= 4) throw e;
		}
	}
	await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'the Images dialog did not open' });
}

test('File > Export > Images in the document… shows the images, writes a file each, and asks about files already there', async () => {
	mkdirSync(OUT, { recursive: true });
	const doc = join(OUT, 'images.pdf');
	rmSync(doc, { force: true });
	cli('gen-images', doc);
	const dir = join(OUT, 'export-embedded');
	rmSync(dir, { recursive: true, force: true });
	mkdirSync(dir, { recursive: true });
	const target = join(dir, 'pics.png');
	const { browser, stop } = await launch([doc], { dialogs: [target, target] });
	try {
		await waitForDocument(browser);
		await openDialog(browser);
		const dialog = await browser.$('[data-images-dialog]');
		assert.match(await dialog.getText(), /Export the images in images\.pdf/);

		// Every image on the three pages, each once: its JPEG as it is, the rest as PNG.
		await browser.waitUntil(async () => /^12 images: 1 JPEG and 11 PNG, about /.test(await summary(browser)), {
			timeout: 30_000,
			timeoutMsg: `the images were not found: ${await summary(browser)}`
		});
		await browser.waitUntil(async () => (await thumbnails(browser)).loaded >= 6, {
			timeout: 30_000,
			timeoutMsg: `the thumbnails did not load: ${JSON.stringify(await thumbnails(browser))}`
		});
		assert.equal((await thumbnails(browser)).shown, 12);
		await browser.saveScreenshot(join(OUT, 'images-dialog.png'));

		// The third page alone.
		await (await browser.$('[data-images-range]')).setValue('3');
		await browser.waitUntil(async () => /^5 images: 1 JPEG and 4 PNG/.test(await summary(browser)), {
			timeout: 30_000,
			timeoutMsg: `the images did not follow the pages: ${await summary(browser)}`
		});
		assert.match(await (await browser.$('[data-images-naming-example]')).getText(), /images-3-1\.jpg/);

		// Every Tab stop is named and shows focus (AGENTS.md section 8).
		const stops = (await tabWalk(browser, { max: 30 })).filter((s) => s.tag !== 'none');
		for (const s of stops) {
			assert.ok(s.name, `an unnamed Tab stop (${s.tag} .${s.cls})`);
			assert.ok(s.visibleFocus, `focus not visible on "${s.name}"`);
		}
		assert.deepEqual(await unnamedControls(browser), [], 'controls without an accessible name');
		const names = stops.map((s) => s.name);
		for (const name of ['Smallest image exported', 'Cancel', 'Export…']) {
			assert.ok(names.includes(name), `Tab reaches ${name}: ${JSON.stringify(names)}`);
		}

		// Export…: the files are named after the name the Save dialog gave.
		await (await browser.$('[data-images-run]')).click();
		await browser.waitUntil(async () => (await step(browser)) === null, { timeout: 60_000, timeoutMsg: 'exporting did not finish' });
		await browser.waitUntil(async () => (await notices(browser)).includes('Exported 5 images from images.pdf to export-embedded.'), {
			timeoutMsg: `nothing said the images were exported: ${await notices(browser)}`
		});
		const files = ['pics-3-1.jpg', 'pics-3-2.png', 'pics-3-3.png', 'pics-3-4.png', 'pics-3-5.png'];
		assert.deepEqual(readdirSync(dir).sort(), files);
		assert.deepEqual([...readFileSync(join(dir, 'pics-3-1.jpg')).subarray(0, 2)], [0xff, 0xd8], 'not a JPEG file');
		assert.deepEqual([...readFileSync(join(dir, 'pics-3-2.png')).subarray(1, 4)], [0x50, 0x4e, 0x47], 'not a PNG file');

		// The same again: the files are there, replaced when asked to. The notification is
		// closed first, so that "Export" finds the menu's item, not "Exported…".
		await (await browser.$('button[aria-label="Dismiss"]')).click();
		await browser.waitUntil(async () => !(await notices(browser)).includes('Exported'), { timeoutMsg: 'the notification stayed' });
		await openDialog(browser);
		await (await browser.$('[data-images-range]')).setValue('3');
		await browser.waitUntil(async () => /^5 images/.test(await summary(browser)), { timeout: 30_000, timeoutMsg: 'the images were not found again' });
		await (await browser.$('[data-images-run]')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'confirm', { timeoutMsg: 'files already there were not asked about' });
		assert.match(await (await browser.$('[data-images-dialog]')).getText(), /All 5 files are already in export-embedded: pics-3-1\.jpg, pics-3-2\.png, pics-3-3\.png and 2 more\./);
		await (await browser.$('[data-images-replace]')).click();
		await browser.waitUntil(async () => (await step(browser)) === null, { timeout: 60_000, timeoutMsg: 'exporting again did not finish' });
		assert.deepEqual(readdirSync(dir).sort(), files, 'no temporary file is left');
	} finally {
		await stop();
	}
});
