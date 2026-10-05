// Annotations (AGENTS.md section 6.5): highlight selected text, place a note and
// type its text, draw with the pen, type a text box, move the note, delete and undo from
// the list, save, check on disk with pdf-cli, reopen; Settings (author, appearance); and
// the repair command on annotations another app wrote with problems.

import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

/** The annotations on disk, as `pdf-cli annot list` prints them (without object numbers). */
function onDisk(path) {
	return cli('annot', 'list', path)
		.split('\n')
		.filter((l) => l.startsWith('p'))
		.map((l) => l.replace(/ object \d+/, '').replace(/ rect \[[^\]]*\]/, ''));
}

/** Rows of the annotation list: type and badge text. */
async function listRows(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[aria-label="Annotations"][role=listbox] [role=option]')].map((r) =>
			r.textContent.replace(/\s+/g, ' ').trim()
		)
	);
}

async function waitForRowCount(browser, n, message) {
	await browser.waitUntil(async () => (await listRows(browser)).length === n, {
		timeout: 8000,
		timeoutMsg: `${message}: list shows ${JSON.stringify(await listRows(browser))}`
	});
}

/** Converts page points of page `index` (view space at zoom 1) to viewport pixels. */
/** Opens the annotation pane and waits until it has finished sliding in. */
async function showAnnotations(browser) {
	await (await browser.$('button[aria-label="Show annotations"]')).click();
	await browser.waitUntil(
		() =>
			browser.execute(() => {
				const pane = document.querySelector('aside[aria-label="Annotations"]');
				return pane !== null && pane.getAnimations({ subtree: true }).length === 0;
			}),
		{ timeoutMsg: 'the annotation pane did not open' }
	);
}

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
			y: Math.round(from.y + ((to.y - from.y) * i) / steps + (i % 2 ? 6 : -6)),
			origin: 'viewport',
			duration: 20
		});
	}
	await action.move({ ...to, origin: 'viewport' }).up().perform();
}

async function click(browser, at) {
	await browser.action('pointer').move({ ...at, origin: 'viewport' }).down().up().perform();
}

test('create, edit, delete, undo, save and reopen annotations', async () => {
	const path = sample('annotations.pdf', 3);

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		await (await browser.$('p*=No annotations yet')).waitForDisplayed();

		// Highlight: H, then select text on the marker line (sample pages put it 96 pt from
		// the top, from 36 pt in).
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		assert.match(await statusText(browser), /Unsaved changes/);

		// Note: N, click, type its text in the inspector. Escape first deselects the highlight,
		// whose inspector would otherwise cover the spot on the page (the annotation pane
		// leaves the page narrower).
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 400, 150));
		await waitForRowCount(browser, 2, 'note');
		const note = await browser.$('aside[aria-label="Annotation properties"] textarea');
		await note.waitForDisplayed();
		await browser.waitUntil(async () => (await browser.execute(() => document.activeElement?.tagName)) === 'TEXTAREA', {
			timeoutMsg: 'the note text is not focused'
		});
		await browser.keys([...'Hello from the note']);
		await browser.keys(['Control', 'Enter']);

		// Pen: P, a stroke.
		await browser.keys('p');
		await drag(browser, await pagePoint(browser, 0, 100, 300), await pagePoint(browser, 0, 300, 360));
		await waitForRowCount(browser, 3, 'drawing');

		// Text box: T, click, type, Ctrl+Enter.
		await browser.keys('t');
		await click(browser, await pagePoint(browser, 0, 72, 450));
		await (await browser.$('textarea[data-annotation-editor]')).waitForDisplayed();
		await browser.keys([...'Typed in a box']);
		await browser.keys(['Control', 'Enter']);
		await waitForRowCount(browser, 4, 'text box');

		// Select tool, then drag the note somewhere else.
		await browser.keys('Escape');
		await browser.keys('Escape');
		await drag(browser, await pagePoint(browser, 0, 410, 160), await pagePoint(browser, 0, 470, 230));
		await browser.pause(500);

		// Delete the drawing from the list, then undo.
		const rows = await listRows(browser);
		const drawing = rows.findIndex((r) => r.startsWith('Drawing'));
		assert.ok(drawing >= 0, JSON.stringify(rows));
		await (await browser.$$('[aria-label="Annotations"][role=listbox] [role=option]'))[drawing].click();
		await browser.execute(() => document.querySelector('[aria-label="Annotations"][role=listbox]').focus());
		await browser.keys('Delete');
		await waitForRowCount(browser, 3, 'delete');
		await browser.keys(['Control', 'z']);
		await waitForRowCount(browser, 4, 'undo delete');

		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), {
			timeoutMsg: 'not saved'
		});
	} finally {
		await stop();
	}

	const saved = onDisk(path);
	assert.equal(saved.length, 4, saved.join('\n'));
	assert.ok(saved.some((l) => l.includes('Highlight')), saved.join('\n'));
	assert.ok(saved.some((l) => l.includes('Text') && l.includes('Hello from the note')), saved.join('\n'));
	assert.ok(saved.some((l) => l.includes('Ink')), saved.join('\n'));
	assert.ok(saved.some((l) => l.includes('FreeText') && l.includes('Typed in a box')), saved.join('\n'));
	assert.ok(!saved.some((l) => l.includes('needs-repair')), saved.join('\n'));
	// The author is the Windows user name (no Settings in this ephemeral run).
	assert.ok(saved.every((l) => !l.includes('author ""')), saved.join('\n'));
	// The note moved: its icon is now around (470, 230).
	const list = cli('annot', 'list', path);
	const noteRect = /Text rect \[([\d.]+) ([\d.]+)/.exec(list);
	assert.ok(noteRect && Math.abs(Number(noteRect[1]) - 460) < 15 && Math.abs(Number(noteRect[2]) - 220) < 15, list);

	({ browser, stop } = await launch([path]));
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		await waitForRowCount(browser, 4, 'reopened');

		// Settings: another author and the dark appearance, applied at once.
		await browser.keys(['Control', ',']);
		const author = await browser.$('label*=Author name');
		await (await author.$('input')).setValue('E2E Tester');
		await (await browser.$('label*=Dark')).click();
		await (await browser.$('button=Save')).click();
		await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.dataset.theme)) === 'dark', {
			timeoutMsg: 'the dark appearance did not apply'
		});
		await browser.keys(['Control', 'g']);
		await browser.keys(['2', 'Enter']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('2 of'), { timeoutMsg: 'not on page 2' });
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.page[data-page="1"]') !== null));
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 1, 40, 92), await pagePoint(browser, 1, 200, 92), 1);
		await waitForRowCount(browser, 5, 'highlight with the new author');
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });
		await browser.saveScreenshot(join(OUT, 'annotations-dark.png'));
	} finally {
		await stop();
	}
	assert.ok(onDisk(path).some((l) => l.startsWith('p2') && l.includes('author "E2E Tester"')), onDisk(path).join('\n'));
});

test('quick tools over selected text, the annotation bar, and where the toolbar sits', async () => {
	const path = sample('quick-tools.pdf', 2);

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);

		// A double-click selects a word, which brings up the quick tools; Esc puts them away.
		const word = await pagePoint(browser, 0, 40, 92);
		await browser.action('pointer').move({ ...word, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		let quick = await browser.$('[role=toolbar][aria-label="Quick tools"]');
		await quick.waitForDisplayed({ timeoutMsg: 'a double-click selected no word' });
		await browser.keys('Escape');
		await quick.waitForExist({ reverse: true, timeoutMsg: 'Esc left the quick tools' });

		// Text selected with the Select tool brings up the quick tools; Highlight marks it.
		const released = await pagePoint(browser, 0, 200, 92);
		await drag(browser, await pagePoint(browser, 0, 40, 92), released, 1);
		quick = await browser.$('[role=toolbar][aria-label="Quick tools"]');
		await quick.waitForDisplayed({ timeoutMsg: 'no quick tools over the selection' });
		// The bar sits above where the pointer was released, centred on it.
		const bar = await browser.execute(() => {
			const r = document.querySelector('[role=toolbar][aria-label="Quick tools"]').getBoundingClientRect();
			return { center: r.left + r.width / 2, bottom: r.bottom };
		});
		assert.ok(Math.abs(bar.center - released.x) <= 2, `bar centred at ${bar.center}, released at ${released.x}`);
		assert.ok(bar.bottom < released.y, `bar bottom ${bar.bottom}, released at ${released.y}`);
		await (await quick.$('button[aria-label="Highlight"]')).click();
		await waitForRowCount(browser, 1, 'highlight from the quick tools');
		await quick.waitForExist({ reverse: true, timeoutMsg: 'the quick tools stayed after marking' });

		// The new highlight is selected: its bar, not the inspector. The bar turns it into an
		// underline, then makes it pink.
		const highlightBar = await browser.$('[role=toolbar][aria-label="Highlight actions"]');
		await highlightBar.waitForDisplayed({ timeoutMsg: 'no bar for the new highlight' });
		assert.equal(await (await browser.$('aside[aria-label="Annotation properties"]')).isExisting(), false);
		await (await highlightBar.$('button[aria-label="Underline"]')).click();
		const underlineBar = await browser.$('[role=toolbar][aria-label="Underline actions"]');
		await underlineBar.waitForDisplayed({ timeoutMsg: 'the highlight did not become an underline' });
		await (await underlineBar.$('button[aria-label="Pink"]')).click();
		await browser.waitUntil(
			async () => (await (await underlineBar.$('button[aria-label="Pink"]')).getAttribute('aria-checked')) === 'true',
			{ timeoutMsg: 'the underline did not turn pink' }
		);
		assert.match((await listRows(browser))[0], /^Underline/);

		// A double-click opens the inspector with the cursor in the note.
		const on = await pagePoint(browser, 0, 120, 92);
		await browser
			.action('pointer')
			.move({ ...on, origin: 'viewport' })
			.down()
			.up()
			.pause(60)
			.down()
			.up()
			.perform();
		await browser.waitUntil(
			() =>
				browser.execute(
					() => document.activeElement?.closest('aside[aria-label="Annotation properties"]') !== null && document.activeElement?.tagName === 'TEXTAREA'
				),
			{ timeoutMsg: 'the double-click did not focus the note' }
		);
		await browser.keys([...'Quick note']);
		await browser.keys(['Control', 'Enter']);
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });

		// Settings: the annotation toolbar moves to the top, shown only near it.
		await browser.keys(['Control', ',']);
		await (await browser.$('label*=Top')).click();
		await (await browser.$('label*=When the pointer is near')).click();
		await (await browser.$('button=Save')).click();
		const toolbar = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		await browser.waitUntil(
			() =>
				browser.execute(() => {
					const bar = document.querySelector('[role=toolbar][aria-label="Annotation tools"]');
					const area = bar.parentElement.getBoundingClientRect();
					return bar.getBoundingClientRect().top - area.top < 40;
				}),
			{ timeoutMsg: 'the toolbar did not move to the top' }
		);
		const middle = await pagePoint(browser, 0, 300, 400);
		await browser.action('pointer').move({ ...middle, origin: 'viewport' }).perform();
		await browser.waitUntil(async () => (await toolbar.getCSSProperty('opacity')).value === 0, {
			timeoutMsg: 'the toolbar did not hide away from the top'
		});
		const top = await browser.execute(() => {
			const area = document.querySelector('[role=toolbar][aria-label="Annotation tools"]').parentElement.getBoundingClientRect();
			return { x: Math.round(area.left + area.width / 2 + 200), y: Math.round(area.top + 20) };
		});
		await browser.action('pointer').move({ ...top, origin: 'viewport' }).perform();
		await browser.waitUntil(async () => (await toolbar.getCSSProperty('opacity')).value === 1, {
			timeoutMsg: 'the toolbar did not show near the top'
		});

		// Settings: docked in the bar above the pages instead, beside the page and zoom
		// controls, always shown; the floating options don't apply to it.
		await browser.keys(['Control', ',']);
		await (await browser.$('label*=In the bar above the pages')).click();
		assert.equal(await (await browser.$('label*=When the pointer is near')).$('button').getAttribute('data-disabled'), '');
		await (await browser.$('button=Save')).click();
		await browser.waitUntil(
			() =>
				browser.execute(() => {
					const bar = document.querySelector('[role=toolbar][aria-label="Annotation tools"]');
					const pages = document.querySelector('.viewer-scroll');
					return (
						bar?.closest('[data-view-bar]')?.querySelector('[aria-label="Page and zoom"]') != null &&
						pages !== null &&
						bar.getBoundingClientRect().bottom <= pages.getBoundingClientRect().top
					);
				}),
			{ timeoutMsg: 'the toolbar did not dock above the pages' }
		);
		await browser.action('pointer').move({ ...middle, origin: 'viewport' }).perform();
		await browser.pause(500);
		const panel = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		assert.equal((await panel.getCSSProperty('opacity')).value, 1, 'the panel stays shown');
		// The active tool has an accent border on every side.
		const pressed = await browser.execute(
			() => getComputedStyle(document.querySelector('[aria-label="Annotation tools"] [aria-pressed="true"]')).boxShadow
		);
		assert.match(pressed, /inset 0px 0px 0px 1px$|^rgb\([^)]*\) 0px 0px 0px 1px inset$/, `active tool: ${pressed}`);
	} finally {
		await stop();
	}
	const saved = onDisk(path);
	assert.equal(saved.length, 1, saved.join('\n'));
	assert.match(saved[0], /Underline .*text "Quick note"/);
});

/** A file written by hand, as another app might: a highlight with no appearance and its
 * corners in the spec's counter-clockwise order, and a note without /NM, /M or /P. */
function problemsFile() {
	const objects = [
		'<< /Type /Catalog /Pages 2 0 R >>',
		'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
		'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Annots [4 0 R 5 0 R] >>',
		'<< /Type /Annot /Subtype /Highlight /Rect [72 640 120 650] /C [1 0.9 0] /T (Other App) /Contents (Old highlight) /QuadPoints [72 630 300 630 300 660 72 660] >>',
		'<< /Type /Annot /Subtype /Text /Rect [400 650 420 670] /C [1 0.8 0] /F 4 /T (Other App) /Contents (A note) >>'
	];
	let out = '%PDF-1.7\n';
	const offsets = [];
	objects.forEach((body, i) => {
		offsets.push(out.length);
		out += `${i + 1} 0 obj\n${body}\nendobj\n`;
	});
	const xref = out.length;
	out += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
	for (const o of offsets) out += `${String(o).padStart(10, '0')} 00000 n \n`;
	out += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
	const path = join(OUT, 'problems.pdf');
	writeFileSync(path, out, 'latin1');
	return path;
}

test('repair annotations another app wrote', async () => {
	const path = problemsFile();
	assert.ok(onDisk(path).every((l) => l.includes('needs-repair')));

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		await waitForRowCount(browser, 2, 'other app');
		assert.ok((await listRows(browser)).every((r) => r.includes('Needs repair')));

		await (await browser.$('button*=Repair 2 annotations')).click();
		const confirm = await browser.$('button=Repair');
		await confirm.waitForDisplayed();
		await confirm.click();
		await browser.waitUntil(async () => (await listRows(browser)).every((r) => !r.includes('Needs repair')), {
			timeoutMsg: `still needs repair: ${JSON.stringify(await listRows(browser))}`
		});
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(async () => (await listRows(browser)).every((r) => r.includes('Needs repair')), {
			timeoutMsg: 'undo did not bring the problems back'
		});
		await browser.keys(['Control', 'y']);
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });
	} finally {
		await stop();
	}
	const saved = onDisk(path);
	assert.equal(saved.length, 2);
	assert.ok(saved.every((l) => !l.includes('needs-repair')), saved.join('\n'));
	assert.ok(saved.some((l) => l.includes('text "Old highlight"') && l.includes('author "Other App"')), saved.join('\n'));
});
