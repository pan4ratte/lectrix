// Pages exported as images (section 6.14, ADR 0034): File > Export > Pages as images… opens
// the dialog, with a preview of a page as it will be written; Export… asks where (the Save
// dialog, answered by LECTRIX_DIALOG), writes a file a page, and a notification says so,
// with a button that shows the files in their folder (LECTRIX_FOLDER_LOG). Files already
// there are asked about. A page's right-click menu and Organize pages open the dialog for
// that page and for the pages selected. Copy page as image puts a page on the clipboard.
// The Snapshot tool frames an area, which its bar copies or saves through the dialog.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { tabWalk, unnamedControls } from '../lib/a11y.mjs';
import { OUT, atRest, keys, launch, organizePages, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const step = (browser) => browser.execute(() => document.querySelector('[data-export-dialog]')?.getAttribute('data-step') ?? null);
const notices = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[role="status"], [role="alert"]')].map((n) => n.textContent.trim()).join('\n')
	);
const previewReady = (browser) =>
	browser.execute(() => {
		const img = document.querySelector('[data-export-preview-image]');
		return img instanceof HTMLImageElement && img.complete && img.naturalWidth > 0 ? img.naturalWidth : 0;
	});

/** Picks `label` from the dialog's dropdown named `name`. */
async function choose(browser, name, label) {
	await (await browser.$(`[data-export-dialog] button[aria-label="${name}"]`)).click();
	await (await browser.$(`[role="option"]=${label}`)).click();
}

/** A folder of its own, empty. */
function folder(name) {
	const dir = join(OUT, name);
	rmSync(dir, { recursive: true, force: true });
	mkdirSync(dir, { recursive: true });
	return dir;
}

test('File > Export > Pages as images… previews a page, writes a file a page, and asks about files already there', async () => {
	const doc = sample('export.pdf', 5);
	const dir = folder('export-images');
	const shownLog = join(OUT, 'export-folder-log.txt');
	rmSync(shownLog, { force: true });
	const target = join(dir, 'pages.jpg');
	const { browser, stop } = await launch([doc], {
		// The first export, then the same again (files already there), then the page alone.
		dialogs: [target, target, join(dir, 'one.png')],
		env: { LECTRIX_FOLDER_LOG: shownLog }
	});
	try {
		await waitForDocument(browser);
		await (await browser.$('button=File')).click();
		await (await browser.$('div*=Export')).click();
		await (await browser.$('div*=Pages as images')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'the Export dialog did not open' });
		const dialog = await browser.$('[data-export-dialog]');
		assert.match(await dialog.getText(), /Export export\.pdf as images/);

		// The preview: Letter at 150 dpi.
		await browser.waitUntil(async () => (await previewReady(browser)) === 1275, {
			timeout: 30_000,
			timeoutMsg: 'the preview did not show the page at 150 dpi'
		});
		assert.match(await (await browser.$('[data-export-preview-size]')).getText(), /1.275 × 1.650 pixels/);

		// Pages 2 and 3, as JPEG at 72 dpi: the preview follows.
		await (await browser.$('[data-export-range]')).setValue('2-3');
		await choose(browser, 'Format', 'JPEG');
		await choose(browser, 'Resolution', '72 dpi');
		await browser.waitUntil(async () => (await previewReady(browser)) === 612, {
			timeout: 30_000,
			timeoutMsg: 'the preview did not follow the resolution'
		});
		assert.match(await dialog.getText(), /Quality/, 'JPEG has no quality');
		assert.match(await (await browser.$('[data-export-naming-example]')).getText(), /export-2\.jpg/);
		await browser.saveScreenshot(join(OUT, 'export-dialog.png'));

		// A resolution too large for the format says so, and Export waits.
		await choose(browser, 'Format', 'PNG');
		await choose(browser, 'Resolution', 'Custom');
		await (await browser.$('[data-export-dpi]')).setValue('2400');
		await (await browser.$('[data-export-too-large]')).waitForDisplayed({ timeoutMsg: 'a page too large was not pointed out' });
		assert.match(await (await browser.$('[data-export-too-large]')).getText(), /too large for PNG\. Choose \d+ dpi or less/);
		assert.equal(await (await browser.$('[data-export-run]')).isEnabled(), false, 'Export is offered for a page too large');
		await choose(browser, 'Format', 'JPEG');
		await choose(browser, 'Resolution', '72 dpi');

		// Every Tab stop is named and shows focus (AGENTS.md section 8).
		const stops = (await tabWalk(browser, { max: 40 })).filter((s) => s.tag !== 'none');
		for (const s of stops) {
			assert.ok(s.name, `an unnamed Tab stop (${s.tag} .${s.cls})`);
			assert.ok(s.visibleFocus, `focus not visible on "${s.name}"`);
		}
		assert.deepEqual(await unnamedControls(browser), [], 'controls without an accessible name');
		const names = stops.map((s) => s.name);
		for (const name of ['Format', 'Resolution', 'Colour', 'Quality', 'Annotations', 'Cancel', 'Export…']) {
			assert.ok(names.includes(name), `Tab reaches ${name}: ${JSON.stringify(names)}`);
		}

		// Export…: the Save dialog names pages.jpg, and the files are named after it.
		await (await browser.$('[data-export-run]')).click();
		await browser.waitUntil(async () => (await step(browser)) === null, { timeout: 60_000, timeoutMsg: 'exporting did not finish' });
		await browser.waitUntil(
			async () => (await notices(browser)).includes('Exported 2 pages of export.pdf as JPEG images to export-images.'),
			{ timeoutMsg: `nothing said the pages were exported: ${await notices(browser)}` }
		);
		assert.deepEqual(readdirSync(dir).sort(), ['pages-2.jpg', 'pages-3.jpg']);
		const jpeg = readFileSync(join(dir, 'pages-2.jpg'));
		assert.deepEqual([...jpeg.subarray(0, 2)], [0xff, 0xd8], 'not a JPEG file');

		// The notification's button shows the first file in its folder.
		await (await browser.$('button=Open file location')).click();
		await browser.waitUntil(() => existsSync(shownLog) && readFileSync(shownLog, 'utf8').includes('pages-2.jpg'), {
			timeoutMsg: 'the files were not shown in their folder'
		});

		// The same again: both files are there, kept when asked to.
		await (await browser.$('button=File')).click();
		await (await browser.$('div*=Export')).click();
		await (await browser.$('div*=Pages as images')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'the dialog did not open again' });
		await browser.waitUntil(async () => (await previewReady(browser)) === 612, {
			timeout: 30_000,
			timeoutMsg: 'the dialog did not keep the choices made'
		});
		await (await browser.$('[data-export-range]')).setValue('2-3');
		await (await browser.$('[data-export-run]')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'confirm', { timeoutMsg: 'files already there were not asked about' });
		assert.match(await dialog.getText(), /Both files are already in export-images: pages-2\.jpg and pages-3\.jpg\./);
		await (await browser.$('[data-export-skip]')).click();
		await browser.waitUntil(async () => (await notices(browser)).includes('Nothing was exported'), {
			timeoutMsg: 'nothing said every file was kept'
		});

		// A page's right-click menu: that page alone, named as the Save dialog said.
		await atRest(browser, '.page[data-page="0"]');
		const r = await browser.execute(() => document.querySelector('.page[data-page="0"]').getBoundingClientRect().toJSON());
		const at = { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + Math.min(r.height / 2, 100)) };
		await browser.action('pointer').move({ ...at, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await browser.$('//*[@role="menuitem"][contains(., "Export page as image")]')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'the page menu did not open the dialog' });
		assert.match(await dialog.getText(), /This page \(1\)/);
		await choose(browser, 'Format', 'PNG');
		await (await browser.$('[data-export-run]')).click();
		await browser.waitUntil(async () => (await notices(browser)).includes('Exported a page of export.pdf to one.png'), {
			timeout: 60_000,
			timeoutMsg: `nothing said the page was exported: ${await notices(browser)}`
		});
		assert.ok(existsSync(join(dir, 'one.png')));
	} finally {
		await stop();
	}
});

test('Organize pages exports the pages selected, and Esc closes the dialog without exporting', async () => {
	const doc = sample('export-organize.pdf', 4);
	const { browser, stop } = await launch([doc]);
	try {
		await waitForDocument(browser);
		await organizePages(browser);
		const cell = (n) => `[data-organize-view] [role=option][aria-posinset="${n}"] [data-picture]`;
		await atRest(browser, cell(2));
		await (await browser.$(cell(2))).click();
		await browser.keys(['Shift', 'ArrowRight']);
		assert.match(await (await browser.$('[data-organize-count]')).getText(), /^2 of 4 pages selected$/);
		await (await browser.$('[data-organize-export]')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'Export did not open the dialog' });
		const dialog = await browser.$('[data-export-dialog]');
		assert.match(await dialog.getText(), /The 2 selected pages/);
		assert.equal(
			await browser.execute(() => document.querySelector('[data-export-dialog] input[value="selected"]')?.checked ?? false),
			true,
			'the selected pages are not the ones chosen'
		);
		await keys(browser, ['Escape']);
		await browser.waitUntil(async () => (await step(browser)) === null, { timeoutMsg: 'Esc did not close the dialog' });
	} finally {
		await stop();
	}
});

/** What the clipboard holds as an image: the bitmap's size, and whether a PNG is there too. */
function clipboardImage() {
	const script = [
		'Add-Type -AssemblyName System.Windows.Forms',
		'$i = [System.Windows.Forms.Clipboard]::GetImage()',
		"if ($i) { '{0}x{1}' -f $i.Width, $i.Height } else { 'none' }",
		"[System.Windows.Forms.Clipboard]::ContainsData('PNG')"
	].join('; ');
	const [size, png] = execFileSync('powershell', ['-NoProfile', '-STA', '-Command', script], { encoding: 'utf8' })
		.trim()
		.split(/\s+/);
	return { size, png: png === 'True' };
}

test('Copy page as image puts the page right-clicked on the clipboard, at 150 dpi', async () => {
	const doc = sample('copy-page.pdf', 3);
	const { browser, stop } = await launch([doc]);
	try {
		await waitForDocument(browser);
		await atRest(browser, '.page[data-page="0"]');
		const r = await browser.execute(() => document.querySelector('.page[data-page="0"]').getBoundingClientRect().toJSON());
		const at = { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + Math.min(r.height / 2, 100)) };
		await browser.action('pointer').move({ ...at, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await browser.$('//*[@role="menuitem"][contains(., "Copy page as image")]')).click();
		await browser.waitUntil(async () => (await notices(browser)).includes('Copied page 1 as an image.'), {
			timeout: 30_000,
			timeoutMsg: `nothing said the page was copied: ${await notices(browser)}`
		});
		// Letter at 150 dpi, as a bitmap and as a PNG.
		assert.deepEqual(clipboardImage(), { size: '1275x1650', png: true });

		// Edit has it too, for the current page.
		await (await browser.$('button=Edit')).click();
		const item = await browser.$('//*[@role="menuitem"][contains(., "Copy page as image")]');
		assert.notEqual(await item.getAttribute('aria-disabled'), 'true', 'Edit > Copy page as image is disabled');
		await keys(browser, ['Escape']);
	} finally {
		await stop();
	}
});

/** Viewport pixels of a point of page `index`, in page points (a Letter page). */
async function pagePoint(browser, index, x, y) {
	const r = await browser.execute(
		(i) => document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect().toJSON(),
		index
	);
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

async function drag(browser, from, to, steps = 8) {
	let action = browser.action('pointer').move({ ...from, origin: 'viewport' }).down();
	for (let i = 1; i <= steps; i++) {
		action = action.move({
			x: Math.round(from.x + ((to.x - from.x) * i) / steps),
			y: Math.round(from.y + ((to.y - from.y) * i) / steps),
			origin: 'viewport'
		});
	}
	await action.up().perform();
}

/** Whether `got` ("450x150") is within a few pixels of `width` x `height`. */
function near(got, width, height) {
	const [w, h] = got.split('x').map(Number);
	return Math.abs(w - width) <= 4 && Math.abs(h - height) <= 4;
}

test('The Snapshot tool frames an area, copies it to the clipboard, and saves it as an image', async () => {
	const doc = sample('snapshot.pdf', 2);
	const dir = folder('export-snapshot');
	const { browser, stop } = await launch([doc], { dialogs: [join(dir, 'area.png')] });
	try {
		await waitForDocument(browser);
		await atRest(browser, '.page[data-page="0"]');
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Snapshot"]')).click();

		// 216 x 72 points from (72, 72): at 150 dpi, 450 x 150 pixels.
		await drag(browser, await pagePoint(browser, 0, 72, 72), await pagePoint(browser, 0, 288, 144));
		await (await browser.$('[data-snapshot-area]')).waitForExist({ timeoutMsg: 'no area was framed' });
		const copy = await browser.$('[data-snapshot-copy]');
		await copy.waitForDisplayed({ timeoutMsg: 'the area has no bar' });
		await browser.saveScreenshot(join(OUT, 'snapshot-area.png'));
		await copy.click();
		await browser.waitUntil(async () => (await notices(browser)).includes('Copied the area as an image.'), {
			timeout: 30_000,
			timeoutMsg: `nothing said the area was copied: ${await notices(browser)}`
		});
		const copied = clipboardImage();
		assert.ok(near(copied.size, 450, 150), `the clipboard has ${copied.size}`);
		assert.ok(copied.png, 'no PNG on the clipboard');

		// Its lower handle makes it twice as tall; then Save as image… saves it as it is.
		await drag(browser, await pagePoint(browser, 0, 180, 144), await pagePoint(browser, 0, 180, 216));
		await (await browser.$('[data-snapshot-save]')).click();
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'Save as image… opened no dialog' });
		const dialog = await browser.$('[data-export-dialog]');
		assert.match(await dialog.getText(), /Save an area of snapshot\.pdf as an image/);
		assert.match(await dialog.getText(), /The area framed on page 1/);
		await choose(browser, 'Format', 'PNG');
		await choose(browser, 'Resolution', '150 dpi');
		await browser.waitUntil(async () => Math.abs((await previewReady(browser)) - 450) <= 4, {
			timeout: 30_000,
			timeoutMsg: 'the preview does not show the area'
		});
		await (await browser.$('[data-export-run]')).click();
		await browser.waitUntil(async () => (await notices(browser)).includes('Saved the area as area.png in export-snapshot.'), {
			timeout: 60_000,
			timeoutMsg: `nothing said the area was saved: ${await notices(browser)}`
		});
		const png = readFileSync(join(dir, 'area.png'));
		const size = `${png.readUInt32BE(16)}x${png.readUInt32BE(20)}`;
		assert.ok(near(size, 450, 300), `the file is ${size}`);

		// Esc lets the area go, and then the tool.
		await keys(browser, ['Escape']);
		await (await browser.$('[data-snapshot-area]')).waitForExist({ reverse: true, timeoutMsg: 'Esc left the area' });
		await keys(browser, ['Escape']);
		await browser.waitUntil(
			async () =>
				(await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Select"]')).getAttribute(
					'aria-pressed'
				)) === 'true',
			{ timeoutMsg: 'a second Esc did not pick the Select tool' }
		);
	} finally {
		await stop();
	}
});
