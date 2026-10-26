// Annotations (AGENTS.md section 6.5): highlight selected text, place a note and
// type its text, draw with the pen, type a text box, move the note, delete and undo from
// the list, save, check on disk with pdf-cli, reopen; Settings (author, appearance); and
// the repair command on annotations another app wrote with problems.

import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { Key } from 'webdriverio';

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
/** The list's rows as text, each starting with its type (the icon's accessible name). */
async function listRows(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('[aria-label="Annotations"][role=listbox] [role=option]')].map((r) =>
			`${r.querySelector('.annotation-type-icon')?.getAttribute('aria-label') ?? ''} ${r.textContent}`.replace(/\s+/g, ' ').trim()
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
	await (await browser.$('button[aria-label="Show right pane"]')).click();
	await browser.waitUntil(
		() =>
			browser.execute(() => {
				const pane = document.querySelector('aside[aria-label="Right pane"]');
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

/**
 * Whether the page shows highlighter yellow around a point of page `index` (page points),
 * read from the pixels of the page's canvas there.
 */
async function yellowAt(browser, index, x, y) {
	const at = await pagePoint(browser, index, x, y);
	return browser.execute(
		(i, px, py) => {
			const canvases = [...document.querySelectorAll(`.page[data-page="${i}"] canvas`)].reverse();
			for (const canvas of canvases) {
				const r = canvas.getBoundingClientRect();
				if (canvas.width <= 1 || px < r.left || px >= r.right || py < r.top || py >= r.bottom) continue;
				const cx = Math.floor(((px - r.left) / r.width) * canvas.width);
				const cy = Math.floor(((py - r.top) / r.height) * canvas.height);
				const data = canvas.getContext('2d').getImageData(Math.max(0, cx - 4), Math.max(0, cy - 4), 9, 9).data;
				let yellow = 0;
				for (let k = 0; k < data.length; k += 4) if (data[k] > 200 && data[k + 1] > 180 && data[k + 2] < 200) yellow++;
				return yellow > 20;
			}
			return false;
		},
		index,
		at.x,
		at.y
	);
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

/** Opens the colour panel of the colour button inside `scope` (Acrobat's picker). */
async function openColours(browser, scope) {
	await (await scope.$('[data-color-button]')).click();
	const panel = await browser.$('.color-panel');
	await panel.waitForDisplayed({ timeoutMsg: 'the colour button opened no panel' });
	return panel;
}

const focusedLabel = (browser) => browser.execute(() => document.activeElement?.getAttribute('aria-label') ?? '');

/** Whether the cursor is in the comment of the selected row of the annotation list. */
const inListComment = (browser) =>
	browser.execute(() => document.activeElement?.matches('.annotation-row[aria-selected="true"] textarea.comment-field') === true);

/** Whether the cursor is in the note of the comment panel beside the annotation. */
const inPanelNote = (browser) =>
	browser.execute(
		() => document.activeElement?.closest('aside[aria-label="Annotation comment"]') !== null && document.activeElement?.tagName === 'TEXTAREA'
	);

const panelShown = (browser) => browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"]') !== null);

/** Closes Settings with its close button; every change in it applies at once. */
async function closeSettings(browser) {
	await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
	await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
		timeoutMsg: 'Settings did not close'
	});
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
		await (await browser.$('p=No annotations')).waitForDisplayed();

		// Highlight: H, then select text on the marker line (sample pages put it 96 pt from
		// the top, from 36 pt in).
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		assert.match(await statusText(browser), /Unsaved changes/);

		// Note: N, click, type its text. The annotation list is showing, so the cursor goes in
		// the note's comment there, and the page shows no comment panel (section 6.5). The new
		// highlight is not selected.
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 400, 150));
		await waitForRowCount(browser, 2, 'note');
		await browser.waitUntil(() => inListComment(browser), { timeoutMsg: 'the new note’s comment in the list is not focused' });
		assert.equal(await panelShown(browser), false, 'the new note opened a comment panel beside the list');
		await browser.keys([...'Hello from the note']);
		// Esc leaves the comment, keeping what was typed (the tooltip below shows it, and the
		// saved file has it); the row keeps its field.
		await browser.keys(['Escape']);
		await browser.waitUntil(async () => (await browser.execute(() => document.activeElement?.tagName)) !== 'TEXTAREA', {
			timeoutMsg: 'Esc did not leave the note'
		});
		assert.equal(
			await browser.execute(() => document.querySelector('.annotation-row[aria-selected="true"] textarea.comment-field')?.value),
			'Hello from the note',
			'Esc in the comment dropped what was typed'
		);

		// Pen: P, a stroke.
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
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

		// Select tool. Resting the pointer on the note shows its comment, and only that.
		await browser.keys('Escape');
		await browser.keys('Escape');
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 410, 160)), origin: 'viewport' }).perform();
		const tip = await browser.$('[role=tooltip]');
		await tip.waitForDisplayed({ timeoutMsg: 'no comment over the note' });
		assert.equal((await tip.getText()).trim(), 'Hello from the note');

		// Drag the note somewhere else.
		await drag(browser, await pagePoint(browser, 0, 410, 160), await pagePoint(browser, 0, 470, 230));
		await browser.pause(500);

		// Delete the drawing from the list, then undo.
		const rows = await listRows(browser);
		const drawing = rows.findIndex((r) => r.startsWith('Drawing'));
		assert.ok(drawing >= 0, JSON.stringify(rows));
		const drawingRow = (await browser.$$('[aria-label="Annotations"][role=listbox] [role=option]'))[drawing];
		await drawingRow.click();
		// Selected: the row has the comment panel's controls (a drawing's: colour, stroke width
		// and Delete), and the page shows no panel; its note field has the same text size as
		// the comments, and no scrollbar.
		const row = await browser.execute(() => {
			const selected = document.querySelector('.annotation-row[aria-selected="true"]');
			const field = selected.querySelector('.comment-field');
			// How far in the text starts: padding and edge, the same for the field and the text.
			const inset = (el) => {
				const s = getComputedStyle(el);
				const side = (name) => parseFloat(s[`padding${name}`]) + parseFloat(s[`border${name}Width`]) + (name === 'Top' ? parseFloat(s.marginTop) : 0);
				return [side('Left'), side('Top'), side('Right')].join(' ');
			};
			const comment = document.querySelector('.annotation-row p');
			return {
				controls: [...selected.querySelectorAll('button')].map((b) => (b.getAttribute('aria-label') ?? '').replace(/:.*/, '')),
				field: getComputedStyle(field).fontSize,
				text: getComputedStyle(comment).fontSize,
				fieldInset: inset(field),
				textInset: inset(comment),
				scrolls: field.scrollHeight > field.clientHeight + 1
			};
		});
		assert.deepEqual(row, {
			controls: ['Colour', 'Stroke width', 'Delete'],
			field: row.text,
			text: row.text,
			fieldInset: row.textInset,
			textInset: row.textInset,
			scrolls: false
		});
		assert.equal(await panelShown(browser), false, 'a row picked in the list opened a comment panel on the page');
		// Its Delete button deletes, as the Delete key and the menu do.
		await (await browser.$('.annotation-row[aria-selected="true"] button[aria-label="Delete"]')).click();
		await waitForRowCount(browser, 3, 'delete from the row');
		await browser.keys(['Control', 'z']);
		await waitForRowCount(browser, 4, 'undo delete from the row');
		await (await browser.$$('[aria-label="Annotations"][role=listbox] [role=option]'))[drawing].click();
		await browser.execute(() => document.querySelector('[aria-label="Annotations"][role=listbox]').focus());
		await browser.keys('Delete');
		await waitForRowCount(browser, 3, 'delete');
		await browser.keys(['Control', 'z']);
		await waitForRowCount(browser, 4, 'undo delete');

		// The same from the row's context menu; a drawing without a comment has none to copy.
		await (await browser.$$('[aria-label="Annotations"][role=listbox] [role=option]'))[drawing].click({ button: 'right' });
		const copyItem = await browser.$('//*[@role="menuitem"][contains(., "Copy comment")]');
		await copyItem.waitForDisplayed({ timeoutMsg: 'no context menu on the row' });
		assert.equal(await copyItem.getAttribute('data-disabled'), '');
		await (await browser.$('//*[@role="menuitem"][contains(., "Delete annotation")]')).click();
		await waitForRowCount(browser, 3, 'delete from the menu');
		await browser.keys(['Control', 'z']);
		await waitForRowCount(browser, 4, 'undo delete from the menu');

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

		// Settings: another author and the dark appearance, applied at once, with no Save.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('#setting-author')).setValue('E2E Tester');
		await (await browser.$('button[role="tab"]*=Appearance')).click();
		await (await (await browser.$('#setting-appearance')).$('[role="radio"]*=Dark')).click();
		await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.dataset.theme)) === 'dark', {
			timeoutMsg: 'the dark appearance did not apply while Settings was open'
		});
		await closeSettings(browser);
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
		// The colour every tool draws with comes first, before the tools, so it can be picked
		// before the text is marked; picking one leaves the selection and the bar as they are.
		const quickOrder = await browser.execute(() => {
			const bar = document.querySelector('[role=toolbar][aria-label="Quick tools"]');
			return [...bar.querySelectorAll('button')].map((b) => (b.hasAttribute('data-color-button') ? 'colour' : b.getAttribute('aria-label')));
		});
		assert.deepEqual(quickOrder.slice(0, 2), ['colour', 'Highlight'], `quick tools: ${quickOrder}`);
		const quickColours = await openColours(browser, quick);
		assert.equal(await (await quickColours.$('input[type=range]')).isExisting(), false, 'the quick tools offer an opacity, which belongs to each tool');
		await (await quickColours.$('button[aria-label="Green"]')).click();
		await browser.keys(['Escape']);
		await quickColours.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		assert.equal(await quick.isDisplayed(), true, 'picking a colour put the quick tools away');
		assert.match(await (await quick.$('[data-color-button]')).getAttribute('aria-label'), /^Colour: Green$/);
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

		// The new highlight is not selected: no bar, no inspector. A click selects it. With the
		// annotation list showing, the page gets neither its bar nor its comment panel: its row
		// in the list has their controls, which turn it into an underline, then make it pink.
		const highlightBar = await browser.$('[role=toolbar][aria-label="Highlight actions"]');
		assert.equal(await highlightBar.isExisting(), false, 'the new highlight was selected');
		assert.equal(await panelShown(browser), false);
		// (Away from where the double-click below lands, which would count this press as its first.)
		await click(browser, await pagePoint(browser, 0, 60, 92));
		const selectedRow = await browser.$('.annotation-row[aria-selected="true"]');
		await selectedRow.waitForDisplayed({ timeoutMsg: 'the click selected no row for the highlight' });
		await (await selectedRow.$('button[aria-label="Highlight"][aria-pressed="true"]')).waitForExist({ timeoutMsg: 'no type buttons in the selected row' });
		assert.equal(await browser.execute(() => document.querySelector('[data-floating-bar]') !== null), false, 'the highlight got a bar beside the list');
		assert.equal(await panelShown(browser), false, 'the highlight got a comment panel beside the list');
		await (await selectedRow.$('button[aria-label="Underline"]')).click();
		await browser.waitUntil(async () => /^Underline/.test((await listRows(browser))[0]), { timeoutMsg: 'the highlight did not become an underline' });
		await (await selectedRow.$('button[aria-label="Underline"][aria-pressed="true"]')).waitForExist({ timeoutMsg: 'the row did not stay selected' });
		// The row shows only the colour in use; its button opens the colours, on the one in use.
		assert.equal((await selectedRow.$$('.swatch')).length, 0, 'the row shows every colour');
		const colours = await openColours(browser, selectedRow);
		await browser.waitUntil(async () => (await focusedLabel(browser)) === 'Green', { timeoutMsg: 'the colours did not open on the one in use' });
		await (await colours.$('button[aria-label="Pink"]')).click();
		await browser.waitUntil(async () => (await (await colours.$('button[aria-label="Pink"]')).getAttribute('aria-checked')) === 'true', {
			timeoutMsg: 'the underline did not turn pink'
		});
		await browser.keys(['Escape']);
		await colours.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		assert.match(await focusedLabel(browser), /^Colour: Pink/, 'the focus did not return to the colour button');

		// With the annotation list showing, a double-click puts the cursor at the end of the
		// annotation's comment there, instead of opening its comment panel.
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
		await browser.waitUntil(() => inListComment(browser), { timeoutMsg: 'the double-click did not focus the comment in the list' });
		assert.equal(await panelShown(browser), false, 'the double-click opened the comment panel');
		await browser.keys([...'Quick note']);
		await browser.keys(['Control', 'Enter']);

		// The row still stands in for the comment panel and the bar.
		assert.equal(await browser.execute(() => document.querySelector('[data-floating-bar]') !== null), false, 'a bar shows beside the list');
		assert.equal(await panelShown(browser), false, 'the comment panel shows beside the list');
		assert.equal(await (await selectedRow.$('button[aria-label="Underline"]')).getAttribute('aria-pressed'), 'true', 'no type buttons in the selected row');
		assert.equal(await (await selectedRow.$('button[aria-label="Delete"]')).isExisting(), true, 'no Delete in the selected row');

		// The colour panel has the opacity slider too; it applies when let go.
		let colourPanel = await openColours(browser, selectedRow);
		const slider = await colourPanel.$('input[type=range][aria-label="Opacity"]');
		// (It was a highlight, made at the highlight's 40%.)
		assert.equal(await slider.getValue(), '0.4');
		await browser.execute(() => document.querySelector('.color-panel input[type=range]').focus());
		await browser.keys(['ArrowLeft']);
		await browser.keys(['Escape']);
		await colourPanel.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		colourPanel = await openColours(browser, selectedRow);
		assert.equal(await (await colourPanel.$('input[type=range][aria-label="Opacity"]')).getValue(), '0.35', 'the opacity was not applied');
		await browser.keys(['Escape']);
		await colourPanel.waitForExist({ reverse: true });

		// Properties, from the page's menu: the author (changed here) and the dates.
		await browser.action('pointer').move({ ...on, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await browser.$('//*[@role="menuitem"][contains(., "Properties")]')).click();
		const dialog = await browser.$('[role=dialog]');
		await dialog.waitForDisplayed({ timeoutMsg: 'Properties opened no dialog' });
		assert.match(await dialog.getText(), /Underline properties[\s\S]*Created[\s\S]*Modified/);
		const authorField = await dialog.$('input');
		await authorField.setValue('Reviewer');
		await (await dialog.$('button=Save')).click();
		await dialog.waitForExist({ reverse: true });
		await browser.waitUntil(async () => (await listRows(browser))[0]?.includes('Reviewer'), {
			timeoutMsg: `the author did not change: ${JSON.stringify(await listRows(browser))}`
		});

		// The colour given to the underline (pink, above) is what new annotations get,
		// whatever the tool.
		await browser.keys(['Escape']);
		await browser.keys('u');
		const tools = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		await (await tools.$('[data-color-button]')).waitForDisplayed({ timeoutMsg: 'no colour for the Underline tool' });
		assert.match(await (await tools.$('[data-color-button]')).getAttribute('aria-label'), /^Colour: Pink/, 'new underlines are not pink');
		await browser.keys(['Escape']);

		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });

		// The annotation toolbar starts docked in the bar above the pages, beside the page and
		// zoom controls. Settings: it floats over the pages instead, at the top, shown only
		// near it.
		assert.ok(
			await browser.execute(() => document.querySelector('[data-view-bar] [role=toolbar][aria-label="Annotation tools"]') !== null),
			'the toolbar does not start docked in the view bar'
		);
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		assert.equal(await (await browser.$('#setting-toolbarDock')).getAttribute('data-disabled'), null, 'where it is docked is not offered');
		await (await browser.$('#setting-toolbarStyle')).click();
		await (await browser.$('[role="option"]*=Floating over the pages')).click();
		await browser.waitUntil(async () => (await (await browser.$('#setting-toolbarDock')).getAttribute('data-disabled')) !== null, {
			timeoutMsg: 'where it is docked stays offered while the toolbar floats'
		});
		await (await browser.$('#setting-toolbarPosition')).click();
		await (await browser.$('[role="option"]*=Top')).click();
		await (await browser.$('label*=Show only when the pointer is near')).click();
		assert.equal(await (await browser.$('#setting-toolbarVisibility')).getAttribute('aria-checked'), 'true');
		await closeSettings(browser);
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
		// While something it opened is open, it stays, though the pointer has gone.
		const hoverColours = await openColours(browser, toolbar);
		await browser.action('pointer').move({ ...middle, origin: 'viewport' }).perform();
		await browser.pause(600);
		assert.equal(await hoverColours.isDisplayed(), true, 'the colours went when the pointer left');
		assert.equal((await toolbar.getCSSProperty('opacity')).value, 1, 'the toolbar hid with its colours open');
		await browser.keys(['Escape']);
		await hoverColours.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });

		// Settings: docked in the bar above the pages instead, beside the page and zoom
		// controls, always shown. The floating options stay: full screen floats it.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('#setting-toolbarStyle')).click();
		await (await browser.$('[role="option"]*=Docked beside the pages')).click();
		assert.equal(await (await browser.$('#setting-toolbarVisibility')).getAttribute('data-disabled'), null);
		assert.equal(await (await browser.$('#setting-toolbarPosition')).getAttribute('data-disabled'), null);
		await closeSettings(browser);
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
		// The active tool has an accent border on every side: the mark that glides between the
		// tools, lying on the pressed one.
		const pressed = await browser.execute(() => {
			const bar = document.querySelector('[aria-label="Annotation tools"]');
			const mark = bar.querySelector('.tool-mark');
			const a = mark.getBoundingClientRect();
			const b = bar.querySelector('[aria-pressed="true"]').getBoundingClientRect();
			const over = Math.abs(a.left - b.left) < 1 && Math.abs(a.top - b.top) < 1 && Math.abs(a.width - b.width) < 1;
			return { shadow: getComputedStyle(mark).boxShadow, over };
		});
		assert.match(pressed.shadow, /inset 0px 0px 0px 1px$|^rgb\([^)]*\) 0px 0px 0px 1px inset$/, `active tool: ${pressed.shadow}`);
		assert.ok(pressed.over, 'the mark lies on the active tool');

		// Settings: docked at the left of the pages instead, in a bar of its own from the view
		// bar down, with the tools stacked. What a tool's controls open goes beside the bar,
		// over the pages, and the stroke width's button shows the number alone.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('#setting-toolbarDock')).click();
		await (await browser.$('[role="option"]*=Left')).click();
		await closeSettings(browser);
		const stackedBar = () =>
			browser.execute(() => {
				const bar = document.querySelector('[role=toolbar][aria-label="Annotation tools"]');
				const pages = document.querySelector('.viewer-scroll')?.getBoundingClientRect();
				const viewBar = document.querySelector('[data-view-bar]')?.getBoundingClientRect();
				if (!bar || !pages || !viewBar) return null;
				const r = bar.getBoundingClientRect();
				const tools = [...bar.querySelectorAll('.tool-button')].map((b) => b.getBoundingClientRect());
				return {
					inViewBar: bar.closest('[data-view-bar]') !== null,
					orientation: bar.getAttribute('aria-orientation'),
					leftOfPages: r.right <= pages.left + 1,
					belowViewBar: r.top >= viewBar.bottom - 1,
					stacked: tools.every((t, i) => i === 0 || (t.top >= tools[i - 1].bottom && Math.abs(t.left - tools[0].left) < 1)),
					right: r.right
				};
			});
		await browser.waitUntil(async () => (await stackedBar())?.leftOfPages === true, { timeoutMsg: 'the toolbar did not dock at the left of the pages' });
		const atLeft = await stackedBar();
		assert.deepEqual(
			{ ...atLeft, right: 0 },
			{ inViewBar: false, orientation: 'vertical', leftOfPages: true, belowViewBar: true, stacked: true, right: 0 },
			`docked at the left: ${JSON.stringify(atLeft)}`
		);
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Pen"]')).click();
		const width = await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Stroke width"]');
		await width.waitForDisplayed({ timeoutMsg: 'no stroke width for the Pen tool' });
		// (Once it has slid open: until then its text is clipped.)
		await browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')));
		assert.match(await width.getText(), /^[\d.]+$/, 'the stacked stroke width shows more than its number');
		await width.click();
		const widths = await browser.$('.dropdown-list');
		await widths.waitForDisplayed({ timeoutMsg: 'the stroke widths did not open' });
		// (Once the list has been placed: it is laid out at the window's corner first.)
		const listLeft = () => browser.execute(() => document.querySelector('.dropdown-list').getBoundingClientRect().left);
		await browser
			.waitUntil(async () => (await listLeft()) >= atLeft.right, { timeout: 3000 })
			.catch(async () => assert.fail(`the stroke widths opened over the toolbar: ${await listLeft()} < ${atLeft.right}`));
		assert.match(await widths.getText(), /2 pt/, 'the list names the widths in full');
		await browser.keys(['Escape']);
		await widths.waitForExist({ reverse: true });
		const penColours = await openColours(browser, await browser.$('[role=toolbar][aria-label="Annotation tools"]'));
		const coloursLeft = () => browser.execute(() => document.querySelector('.color-panel').getBoundingClientRect().left);
		await browser
			.waitUntil(async () => (await coloursLeft()) >= atLeft.right, { timeout: 3000 })
			.catch(async () => assert.fail(`the colours opened over the toolbar: ${await coloursLeft()} < ${atLeft.right}`));
		await browser.keys(['Escape']);
		await penColours.waitForExist({ reverse: true });
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Select"]')).click();
		// Back above the pages, where the rest of this test expects it.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('#setting-toolbarDock')).click();
		await (await browser.$('[role="option"]*=Top')).click();
		await closeSettings(browser);
		await (await browser.$('[data-view-bar] [role=toolbar][aria-label="Annotation tools"]')).waitForExist({
			timeoutMsg: 'the toolbar did not go back above the pages'
		});

		// Settings: new markup opens its comment. With the annotation list showing, a highlight
		// then has the cursor in its comment there, and no comment panel.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('label*=Add a comment after marking text')).click();
		await closeSettings(browser);
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 2, 'highlight with its comment open');
		await browser.waitUntil(() => inListComment(browser), { timeoutMsg: 'the new highlight’s comment in the list is not focused' });
		assert.equal(await panelShown(browser), false, 'the new highlight opened a comment panel beside the list');
		// What is typed is kept when a click on the page deselects the highlight, which takes
		// the field away before it hears that it lost the focus: a click on the page's text
		// (which takes the focus at once), then one beside the text (where the field goes while
		// it still has the focus). A line of text every 15 pt from 120 pt down; its letters are
		// just above that.
		const commentShown = (text) =>
			browser.waitUntil(async () => (await listRows(browser)).some((r) => r.startsWith('Highlight') && r.endsWith(text)), {
				timeoutMsg: `the list does not show "${text}"`
			});
		await browser.keys([...'Comment']);
		await click(browser, await pagePoint(browser, 0, 60, 206));
		await commentShown('Comment');
		await browser.keys(['Escape']);
		await click(browser, await pagePoint(browser, 0, 120, 92));
		const openNote = await browser.$('.annotation-row[aria-selected="true"] textarea.comment-field');
		await openNote.waitForDisplayed({ timeoutMsg: 'a click on the commented highlight gave its row no comment field' });
		assert.equal(await panelShown(browser), false, 'a click on the commented highlight showed a comment panel beside the list');
		await openNote.click();
		await browser.keys(['Control', 'End']);
		await browser.keys([...' at once']);
		await click(browser, await pagePoint(browser, 0, 585, 206));
		await commentShown('Comment at once');
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });

		// Settings: without the last colour, new annotations start from the default again
		// (yellow, not the pink given to one above).
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('label*=Use the last colour and opacity')).click();
		await closeSettings(browser);
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Underline"]')).click();
		const docked = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		const toolColour = await openColours(browser, docked);
		assert.equal(await (await toolColour.$('button[aria-label="Yellow"]')).getAttribute('aria-checked'), 'true', 'new underlines kept the last colour');
		await browser.keys(['Escape']);
		await toolColour.waitForExist({ reverse: true });
		await browser.keys(['Escape']);

		// With the annotation list hidden, the comment shows in a panel beside the annotation:
		// a double-click opens it with the cursor in the note.
		await (await browser.$('button[aria-label="Hide right pane"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('aside[aria-label="Right pane"]') === null), {
			timeoutMsg: 'the right pane did not close'
		});
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Select"]')).click();
		// The page re-fits the wider view; back to its top, where the marks are.
		await browser.pause(300);
		await browser.execute(() => document.querySelector('.viewer-scroll').scrollTo(0, 0));
		await browser.pause(200);
		const marked = await pagePoint(browser, 0, 120, 92);
		await browser.action('pointer').move({ ...marked, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'without the list, the double-click did not open the comment panel' });
		assert.equal(await browser.execute(() => document.activeElement.value), 'Comment at once', 'the comment typed in the list is not in the panel');
		await browser.keys(['Control', 'Enter']);

		// The comment panel takes the bar's place, beside the highlight.
		assert.equal(await highlightBar.isExisting(), false, 'the bar stayed beside the comment panel');
		const panelBox = () =>
			browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"]').getBoundingClientRect().toJSON());
		const placed = await panelBox();
		assert.ok(placed.left > marked.x && Math.abs(placed.top - marked.y) < 60, `panel at ${JSON.stringify(placed)}, highlight at ${JSON.stringify(marked)}`);

		// A tail on its border points at the highlight, from the side facing it.
		assert.equal(
			await browser.execute(() => document.querySelector('[data-annotation-panel] .comment-tail') !== null),
			true,
			'no tail on the comment panel'
		);

		// It drags from anywhere but its controls, never past the edges of the view.
		const blank = { x: Math.round(placed.left + placed.width / 2), y: Math.round(placed.top + 7) };
		await browser
			.action('pointer')
			.move({ ...blank, origin: 'viewport' })
			.down()
			.move({ x: blank.x - 150, y: blank.y + 40, origin: 'viewport', duration: 60 })
			.move({ x: 1, y: 1, origin: 'viewport', duration: 60 })
			.up()
			.perform();
		const dragged = await panelBox();
		const viewer = await browser.execute(() => document.querySelector('.viewer-scroll').getBoundingClientRect().toJSON());
		assert.ok(Math.abs(dragged.left - (viewer.left + 8)) <= 2 && Math.abs(dragged.top - (viewer.top + 8)) <= 2, `dragged to ${JSON.stringify(dragged)} in ${JSON.stringify(viewer)}`);

		// Its bottom-right corner resizes it.
		const corner = { x: Math.round(dragged.right - 1), y: Math.round(dragged.bottom - 1) };
		await browser
			.action('pointer')
			.move({ ...corner, origin: 'viewport' })
			.down()
			.move({ x: corner.x + 60, y: corner.y + 50, origin: 'viewport', duration: 60 })
			.up()
			.perform();
		const grown = await panelBox();
		assert.ok(Math.abs(grown.width - dragged.width - 60) <= 2 && Math.abs(grown.height - dragged.height - 50) <= 2, `resized from ${JSON.stringify(dragged)} to ${JSON.stringify(grown)}`);
		assert.ok(Math.abs(grown.left - dragged.left) <= 1 && Math.abs(grown.top - dragged.top) <= 1, 'resizing from the corner moved the panel');

		// Closed and opened again, it comes back where it was put: with a comment, a click
		// shows the highlight's comment panel rather than its bar.
		// A spot on the page right of the panel (in a small window, as CI's, the page's
		// right half is under it).
		const page = await browser.execute(() => document.querySelector('.page[data-page="0"]').getBoundingClientRect().toJSON());
		const away = {
			x: Math.round((Math.max(grown.right, page.left) + Math.min(page.right, viewer.right)) / 2),
			y: (await pagePoint(browser, 0, 450, 200)).y
		};
		assert.ok(away.x > grown.right + 8 && away.x < page.right - 8 && away.y < viewer.bottom, `the blank spot ${JSON.stringify(away)} is under the panel or off screen`);
		const panelGone = (timeoutMsg) => browser.waitUntil(async () => !(await panelShown(browser)), { timeoutMsg });
		await click(browser, away);
		await panelGone('a click on the page left the comment panel open');
		await click(browser, marked);
		await (await browser.$('aside[aria-label="Annotation comment"]')).waitForDisplayed({ timeoutMsg: 'a click on the commented highlight showed no comment panel' });
		assert.equal(await highlightBar.isExisting(), false, 'a click on the commented highlight showed its bar');
		const back = await panelBox();
		assert.ok(Math.abs(back.left - grown.left) <= 1 && Math.abs(back.top - grown.top) <= 1, `back at ${JSON.stringify(back)}, put at ${JSON.stringify(grown)}`);
		assert.ok(Math.abs(back.width - grown.width) <= 1 && Math.abs(back.height - grown.height) <= 1, `back as ${JSON.stringify(back)}, sized ${JSON.stringify(grown)}`);
		// Its header has the bar's controls: the type buttons and Delete.
		assert.equal(
			await browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"] button[aria-label="Highlight"]')?.getAttribute('aria-pressed')),
			'true',
			'no type buttons in the comment panel'
		);
		// Esc from its controls closes it, and the bar takes its place.
		await browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"] button[aria-label="Delete"]').focus());
		await browser.keys(['Escape']);
		// (Looked up again: the bar found at the start of the test is long gone.)
		const barBack = await browser.$('[role=toolbar][aria-label="Highlight actions"]');
		await barBack.waitForDisplayed({ timeoutMsg: 'Esc on the comment panel did not bring the bar back' });
		assert.equal(await panelShown(browser), false, 'Esc left the comment panel open');
		// The bar shows only the colour in use, and its type buttons change the type.
		assert.equal((await barBack.$$('.swatch')).length, 0, 'the bar shows every colour');
		await (await barBack.$('button[aria-label="Underline"]')).click();
		const asUnderline = await browser.$('[role=toolbar][aria-label="Underline actions"]');
		await asUnderline.waitForDisplayed({ timeoutMsg: 'the bar did not turn the highlight into an underline' });
		await (await asUnderline.$('button[aria-label="Highlight"]')).click();
		await barBack.waitForDisplayed({ timeoutMsg: 'the bar did not turn the underline back into a highlight' });
		// The bar's Note button opens the panel again, with the cursor in the note.
		await (await barBack.$('button[aria-label="Note"]')).click();
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'the Note button did not focus the note' });
		assert.equal(await barBack.isExisting(), false, 'the bar stayed beside the comment panel it opened');
		await browser.keys(['Control', 'Enter']);
		await click(browser, away);
		await panelGone('a click on the page left the comment panel open');

		// Another annotation's panel takes the width given above, but no more height than its
		// comment needs: a new note, with no comment yet, gets a shorter one.
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 450, 260));
		await (await browser.$('aside[aria-label="Annotation comment"]')).waitForDisplayed({ timeoutMsg: 'the new note opened no comment panel' });
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'without the list, the new note’s panel is not focused' });
		const trimmed = await panelBox();
		assert.ok(Math.abs(trimmed.width - grown.width) <= 1, `the note's panel is ${trimmed.width} wide, the last size ${grown.width}`);
		assert.ok(trimmed.height < grown.height - 40, `the note's panel is ${trimmed.height} tall, the last size ${grown.height}`);
		await (await browser.$('aside[aria-label="Annotation comment"] button[aria-label="Delete"]')).click();
		await panelGone('Delete in the panel left the note');
	} finally {
		await stop();
	}
	const saved = onDisk(path);
	assert.equal(saved.length, 2, saved.join('\n'));
	assert.ok(saved.some((l) => /Underline .*text "Quick note"/.test(l)), saved.join('\n'));
	assert.ok(saved.some((l) => /Highlight .*text "Comment at once"/.test(l)), saved.join('\n'));
});

test('the colour panel opens under the pointer, and Settings changes its presets', async () => {
	const { browser, stop } = await launch([sample('colours.pdf', 1)]);
	try {
		await waitForDocument(browser);
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		const tools = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		const button = await tools.$('[data-color-button]');
		await button.waitForDisplayed({ timeoutMsg: 'no colour button for the Highlight tool' });
		const box = await browser.execute((el) => el.getBoundingClientRect().toJSON(), button);
		const on = { x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2) };
		// Away from the button and its panel: over the pages, below the bar the toolbar is in.
		const away = { x: on.x - 200, y: on.y + 250 };
		const rest = async () => {
			await browser.action('pointer').move({ ...away, origin: 'viewport' }).perform();
			await browser.pause(100);
			await browser.action('pointer').move({ ...on, origin: 'viewport', duration: 100 }).perform();
		};
		const leave = () => browser.action('pointer').move({ ...away, origin: 'viewport', duration: 100 }).perform();
		// (Looked up each time: a panel that went and came back is another element.)
		const panel = () => browser.$('.color-panel');

		// The pointer resting on the button opens the colours, and the focus stays put.
		await rest();
		await (await panel()).waitForDisplayed({ timeoutMsg: 'the pointer on the colour button opened no panel' });
		assert.equal(await browser.execute(() => document.querySelector('.color-panel').contains(document.activeElement)), false, 'the panel took the focus');
		// 18 presets, 6 px apart; the custom colour's button is on the opacity slider's row.
		const layout = await browser.execute(() => {
			const box = (sel) => document.querySelector(sel).getBoundingClientRect();
			const swatches = [...document.querySelectorAll('.color-panel [role=radiogroup] .swatch')].map((s) => s.getBoundingClientRect());
			const middle = (r) => r.top + r.height / 2;
			return {
				presets: swatches.length,
				across: Math.round(swatches[1].left - swatches[0].right),
				down: Math.round(swatches[6].top - swatches[0].bottom),
				row: Math.abs(middle(box('.color-panel .swatch-custom')) - middle(box('.color-panel input[type=range]'))) < 4,
				afterSlider: box('.color-panel .swatch-custom').left > box('.color-panel input[type=range]').right
			};
		});
		assert.deepEqual(layout, { presets: 18, across: 6, down: 6, row: true, afterSlider: true });
		// The custom colour's picker opens below that row.
		await (await (await panel()).$('.swatch-custom')).click();
		await (await (await panel()).$('.hue-square')).waitForDisplayed({ timeoutMsg: 'the custom colour opened no picker' });
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.color-panel').getAnimations({ subtree: true }).length === 0), {
			timeoutMsg: 'the picker did not finish opening'
		});
		assert.equal(
			await browser.execute(
				() => document.querySelector('.color-panel .hue-square').getBoundingClientRect().top > document.querySelector('.color-panel .swatch-custom').getBoundingClientRect().bottom
			),
			true,
			'the picker is not below the custom colour button'
		);
		// Something in it was pressed, so it stays when the pointer leaves, until Esc.
		await leave();
		await browser.pause(600);
		assert.equal(await (await panel()).isExisting(), true, 'the panel went though it was being used');
		await browser.keys(['Escape']);
		await (await panel()).waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		// Opened by the pointer and left alone, it goes when the pointer leaves.
		await rest();
		await (await panel()).waitForDisplayed({ timeoutMsg: 'the pointer opened no panel the second time' });
		await leave();
		await (await panel()).waitForExist({ reverse: true, timeoutMsg: 'the panel stayed after the pointer left' });

		// Settings: without opening on hover, the button opens its colours on a click only.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('label*=Open the colours when the pointer is on their button')).click();
		assert.equal(await (await browser.$('#setting-colorPanelOnHover')).getAttribute('aria-checked'), 'false');
		await closeSettings(browser);
		await rest();
		await browser.pause(600);
		assert.equal(await (await panel()).isExisting(), false, 'the pointer opened the panel though Settings turned that off');
		await button.click();
		await (await panel()).waitForDisplayed({ timeoutMsg: 'a click on the colour button opened no panel' });
		await browser.keys(['Escape']);
		await (await panel()).waitForExist({ reverse: true });

		// Settings: the first preset (yellow) gets another colour, which every panel then offers.
		const firstPreset = () => browser.execute(() => document.querySelector('#setting-presetColors [data-preset="0"]')?.getAttribute('aria-label'));
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		assert.equal(await firstPreset(), 'Yellow');
		assert.equal((await browser.$$('#setting-presetColors .swatch')).length, 18);
		assert.equal(await (await browser.$('#setting-resetPresetColors')).isEnabled(), false, 'Restore colours is offered with nothing changed');
		await (await browser.$('#setting-presetColors [data-preset="0"]')).click();
		const hex = await browser.$('.color-panel input[aria-label="Hex colour"]');
		await hex.waitForDisplayed({ timeoutMsg: 'the preset opened no colour picker' });
		await hex.click();
		await browser.keys(['Control', 'a']);
		await browser.keys([...'12AB34']);
		await browser.keys(['Enter']);
		await browser.waitUntil(async () => (await firstPreset()) === '#12AB34, in place of Yellow', { timeoutMsg: `the preset did not change: ${await firstPreset()}` });
		// Esc closes the picker, not Settings.
		await browser.keys(['Escape']);
		await (await panel()).waitForExist({ reverse: true, timeoutMsg: 'Esc left the preset’s picker open' });
		assert.equal(await (await browser.$('[role="dialog"]')).isExisting(), true, 'Esc in the picker closed Settings');
		await closeSettings(browser);
		await button.click();
		await (await panel()).waitForDisplayed();
		const own = await (await panel()).$('[role=radiogroup] .swatch');
		assert.equal(await own.getAttribute('aria-label'), '#12AB34');
		await own.click();
		await browser.waitUntil(async () => /^Colour: #12AB34/.test(await button.getAttribute('aria-label')), { timeoutMsg: 'the tool did not take the changed preset' });
		await browser.keys(['Escape']);
		await (await panel()).waitForExist({ reverse: true });

		// Restore colours puts yellow back.
		await browser.keys(['Control', ',']);
		await (await browser.$('button=Restore colours')).click();
		await browser.waitUntil(async () => (await firstPreset()) === 'Yellow', { timeoutMsg: 'Restore colours left the changed preset' });
		await closeSettings(browser);

		// The number keys pick the presets in their order: yellow, green, red, blue first.
		const colourIs = (name) =>
			browser.waitUntil(async () => new RegExp(`^Colour: ${name},`).test(await button.getAttribute('aria-label')), {
				timeoutMsg: `the colour is not ${name}`
			});
		await (await browser.$('[role=document]')).click();
		await browser.keys('2');
		await colourIs('Green');
		await browser.keys('4');
		await colourIs('Blue');
		// The colour is every tool's, so it is there with the Select tool too, before the
		// tools; the opacity is each tool's own, so its panel then has none.
		await (await tools.$('button[aria-label="Select"]')).click();
		await (await tools.$('button[aria-label="Select"][aria-pressed="true"]')).waitForExist();
		assert.match(await button.getAttribute('aria-label'), /^Colour: Blue$/);
		assert.equal(
			await browser.execute((el) => el.compareDocumentPosition(document.querySelector('[aria-label="Annotation tools"] .tool-button')) === Node.DOCUMENT_POSITION_FOLLOWING, button),
			true,
			'the colour is not before the tools'
		);
		await button.click();
		await (await panel()).waitForDisplayed({ timeoutMsg: 'the colour button opened no panel with the Select tool' });
		assert.equal(await (await (await panel()).$('input[type=range]')).isExisting(), false, 'an opacity with no tool to have it');
		await (await (await panel()).$('button[aria-label="Red"]')).click();
		await browser.keys(['Escape']);
		await (await panel()).waitForExist({ reverse: true });
		await browser.keys('n');
		await colourIs('Red');
		assert.match(await button.getAttribute('aria-label'), /opacity 100%$/);
	} finally {
		await stop();
	}
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

test('comments keep their line breaks, and looking at one changes nothing', async () => {
	// Acrobat separates a comment's lines with a lone CR.
	const [CR, LF] = [13, 10].map((c) => String.fromCharCode(c));
	const plain = sample('line-breaks-plain.pdf', 1);
	const path = plain.replace('-plain.pdf', '.pdf');
	cli('annot', 'note', plain, path, '--at', '400,150', '--text', ['First line', 'Second line'].join(CR));

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		const lines = (text) => text.split(LF).map((l) => l.trim()).filter(Boolean);

		// The list shows two lines.
		const shown = await browser.execute(() => document.querySelector('.annotation-row p').innerText);
		assert.deepEqual(lines(shown), ['First line', 'Second line']);

		// So does the tooltip.
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 410, 160)), origin: 'viewport' }).perform();
		const tip = await browser.$('[role=tooltip]');
		await tip.waitForDisplayed({ timeoutMsg: 'no comment over the note' });
		assert.deepEqual(lines(await browser.execute(() => document.querySelector('[role=tooltip]').innerText)), ['First line', 'Second line']);

		// Into the comment's field and out again: no edit.
		await (await browser.$('.annotation-row')).click();
		const field = await browser.$('.annotation-row .comment-field');
		await field.click();
		await browser.execute(() => document.querySelector('[aria-label="Annotations"][role=listbox]').focus());
		await browser.pause(300);
		assert.match(await statusText(browser), /All changes saved/);
	} finally {
		await stop();
	}
});

test('reply to an annotation from the list, edit the reply and delete it', async () => {
	const plain = sample('replies-plain.pdf', 1);
	const path = plain.replace('-plain.pdf', '.pdf');
	cli('annot', 'note', plain, path, '--at', '400,150', '--text', 'The note');

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		const menuItem = (name) => browser.$(`//*[@role="menuitem"][contains(., "${name}")]`);
		const thread = () => browser.execute(() => [...document.querySelectorAll('.reply-thread [data-reply] p')].map((p) => p.textContent.trim()));

		// Reply from the row's context menu: the field opens focused; Ctrl+Enter sends.
		await (await browser.$('.annotation-row')).click({ button: 'right' });
		await (await menuItem('Reply')).click();
		const field = await browser.$('textarea[aria-label="Reply to this annotation"]');
		await field.waitForDisplayed({ timeoutMsg: 'no reply field' });
		await browser.waitUntil(() => browser.execute(() => document.activeElement?.getAttribute('aria-label') === 'Reply to this annotation'), {
			timeoutMsg: 'the reply field is not focused'
		});
		await browser.keys([...'First reply']);
		await browser.keys(['Control', 'Enter']);
		await browser.waitUntil(async () => (await thread()).join('|') === 'First reply', { timeoutMsg: `no reply in the thread: ${await thread()}` });
		assert.match(await statusText(browser), /Unsaved changes/);
		// One row still: the reply is in the thread, not a row of its own.
		assert.equal((await listRows(browser)).length, 1);

		// Hovering the note still shows the note's own comment.
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 410, 160)), origin: 'viewport' }).perform();
		const tip = await browser.$('[role=tooltip]');
		await tip.waitForDisplayed({ timeoutMsg: 'no comment over the note' });
		assert.equal((await tip.getText()).trim(), 'The note');

		// Saved: the reply is a note linked to its parent.
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });
		const lines = cli('annot', 'list', path).split('\n').filter((l) => l.includes('object'));
		const parentId = /object (\d+) Text/.exec(lines.find((l) => l.includes('"The note"')) ?? '')?.[1];
		assert.ok(parentId, lines.join('\n'));
		assert.ok(lines.some((l) => l.includes('"First reply"') && l.includes(`reply-to ${parentId}`)), lines.join('\n'));

		// Edit the reply from its own context menu.
		await (await browser.$('.reply-thread [data-reply]')).click({ button: 'right' });
		await (await menuItem('Edit reply')).click();
		await browser.waitUntil(() => browser.execute(() => document.activeElement?.getAttribute('aria-label') === 'Reply'), {
			timeoutMsg: 'the reply is not open for editing'
		});
		await browser.keys(['Control', 'a']);
		await browser.keys([...'Edited reply']);
		await browser.keys(['Control', 'Enter']);
		await browser.waitUntil(async () => (await thread()).join('|') === 'Edited reply', { timeoutMsg: `the reply was not edited: ${await thread()}` });

		// Delete it, then undo.
		await (await browser.$('.reply-thread [data-reply]')).click({ button: 'right' });
		await (await menuItem('Delete reply')).click();
		await browser.waitUntil(async () => (await thread()).length === 0, { timeoutMsg: 'the reply was not deleted' });
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(async () => (await thread()).join('|') === 'Edited reply', { timeoutMsg: 'undo did not bring the reply back' });
	} finally {
		await stop();
	}
});

test('the list counts, searches, filters and sorts annotations', async () => {
	const { browser, stop } = await launch([sample('list.pdf', 3)]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		// A highlight over the marker line ("The quick brown fox..."), and a note with a comment.
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 400, 150));
		await waitForRowCount(browser, 2, 'note');
		await browser.waitUntil(async () => (await browser.execute(() => document.activeElement?.tagName)) === 'TEXTAREA', {
			timeoutMsg: 'the note text is not focused'
		});
		await browser.keys([...'Hello from the note']);
		await browser.keys(['Control', 'Enter']);
		// Nothing selected, so the rows show their comments as text.
		await click(browser, await pagePoint(browser, 0, 300, 600));
		await browser.waitUntil(async () => (await listRows(browser)).some((r) => r.includes('Hello from the note')), {
			timeoutMsg: 'the note kept no comment'
		});

		// The header counts them.
		const count = () => browser.execute(() => document.querySelector('aside[aria-label="Right pane"] [aria-live="polite"]')?.textContent?.trim());
		assert.equal(await count(), '2 annotations');

		// Search: the comments, as typed.
		await (await browser.$('button[aria-label="Search annotations"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.activeElement?.matches('input[type=search]') === true), {
			timeoutMsg: 'Search did not focus its field'
		});
		await browser.keys([...'HELLO']);
		await waitForRowCount(browser, 1, 'searching the comments for "hello"');
		assert.match((await listRows(browser))[0], /^Note/);
		assert.equal(await count(), '1 of 2 annotations');
		// With the switch, the text the annotations mark instead.
		await browser.keys(['Control', 'a']);
		await browser.keys([...'quick']);
		await waitForRowCount(browser, 0, 'no comment says "quick"');
		await (await browser.$('button[aria-label="Search the text annotations mark, not their comments"]')).click();
		await waitForRowCount(browser, 1, 'searching the marked text for "quick"');
		assert.match((await listRows(browser))[0], /^Highlight/);
		// Esc empties the field, then closes it.
		await browser.execute(() => document.querySelector('input[type=search]').focus());
		await browser.keys(['Escape']);
		await waitForRowCount(browser, 2, 'Esc emptied the search');
		await browser.keys(['Escape']);
		await (await browser.$('input[type=search]')).waitForExist({ reverse: true, timeoutMsg: 'Esc left the search open' });

		// Filter: the types as their icons, the colours, the authors as pills.
		await (await browser.$('button[aria-label="Filter annotations"]')).click();
		// The filters slide open, moving as they do: a click sent meanwhile lands where its
		// button was a moment before.
		await browser.waitUntil(
			() => browser.execute(() => document.querySelector('[role=group][aria-label="Filters"]')?.getAnimations().length === 0),
			{ timeoutMsg: 'the filters did not finish opening' }
		);
		const types = await browser.$('[role=group][aria-labelledby^="filter-types-"]');
		await (await types.$('button[aria-label="Note"]')).click();
		await waitForRowCount(browser, 1, 'the Note filter');
		assert.match((await listRows(browser))[0], /^Note/);
		assert.ok(await (await browser.$('[role=group][aria-labelledby^="filter-colours-"] button[aria-label="Yellow"]')).isExisting(), 'no yellow in the colours');
		assert.equal((await browser.$$('[role=group][aria-labelledby^="filter-authors-"] .filter-pill')).length, 1);
		const subheadings = await browser.execute(() => [...document.querySelectorAll('.filter-heading')].map((h) => h.textContent.trim()));
		assert.deepEqual(subheadings, ['Type', 'Colour', 'Author']);
		// What the filters leave out is not drawn on the page either, and a click there
		// finds nothing.
		await browser.waitUntil(async () => !(await yellowAt(browser, 0, 120, 92)), { timeoutMsg: 'the page still shows the highlight the filter leaves out' });
		await click(browser, await pagePoint(browser, 0, 120, 92));
		await browser.pause(200);
		assert.equal(await browser.execute(() => document.querySelector('.annotation-row[aria-selected="true"]') !== null), false, 'a click selected the hidden highlight');
		assert.equal(await browser.execute(() => document.querySelector('[data-floating-bar], [data-annotation-panel]') !== null), false, 'the hidden highlight got a bar or a comment panel');
		assert.equal((await listRows(browser)).length, 1);
		await browser.keys(['Escape']);
		await (await browser.$('button=Clear filters')).click();
		await waitForRowCount(browser, 2, 'Clear filters');
		await browser.waitUntil(() => yellowAt(browser, 0, 120, 92), { timeoutMsg: 'the highlight did not come back to the page with the filter cleared' });

		// An annotation made while a filter would leave it out clears the filter, so it
		// doesn't vanish as it is made: a highlight on the next line of text, under Note.
		await (await types.$('button[aria-label="Note"]')).click();
		await waitForRowCount(browser, 1, 'the Note filter again');
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 116), await pagePoint(browser, 0, 200, 116), 1);
		await waitForRowCount(browser, 3, 'a new highlight under a filter that would hide it');
		assert.equal(await (await types.$('button[aria-label="Note"]')).getAttribute('aria-pressed'), 'false', 'the filter stayed on');
		await browser.waitUntil(() => yellowAt(browser, 0, 120, 116), { timeoutMsg: 'the new highlight is not on the page' });
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);

		// Sort by author: one heading, the author's name, in place of the pages.
		const headings = () => browser.execute(() => [...document.querySelectorAll('.annotation-page')].map((h) => h.textContent.trim()));
		assert.deepEqual(await headings(), ['Page 1']);
		await (await browser.$('button[aria-label^="Sort by page, A–Z"]')).click();
		await (await browser.$('//*[@role="menuitemradio"][contains(., "Author")]')).click();
		await browser.waitUntil(async () => !(await headings()).includes('Page 1'), { timeoutMsg: 'sorting by author kept the page headings' });
		assert.equal((await headings()).length, 1);
		// Z to A: the order is a second choice in the same menu, kept for every sort.
		await (await browser.$('button[aria-label="Sort by author, A–Z"]')).click();
		await (await browser.$('//*[@role="menuitemradio"][contains(., "Z–A")]')).click();
		await (await browser.$('button[aria-label="Sort by author, Z–A"]')).waitForExist({ timeoutMsg: 'Z–A was not picked' });
		await (await browser.$('button[aria-label="Sort by author, Z–A"]')).click();
		await (await browser.$('//*[@role="menuitemradio"][contains(., "Page")]')).click();
		await (await browser.$('button[aria-label="Sort by page, Z–A (last page first)"]')).waitForExist({ timeoutMsg: 'the order did not stay Z–A' });
	} finally {
		await stop();
	}
});

test('the list shows the annotations of a page from its top, not in the order they were made', async () => {
	const plain = sample('list-order-plain.pdf', 1);
	const lower = plain.replace('-plain.pdf', '-lower.pdf');
	const path = plain.replace('-plain.pdf', '.pdf');
	cli('annot', 'note', plain, lower, '--at', '400,500', '--text', 'Made first, lower');
	cli('annot', 'note', lower, path, '--at', '400,150', '--text', 'Made second, higher');

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		await waitForRowCount(browser, 2, 'two notes');
		const rows = await listRows(browser);
		assert.match(rows[0], /Made second, higher/);
		assert.match(rows[1], /Made first, lower/);
	} finally {
		await stop();
	}
});

test('the annotation list keeps the sort last picked in every document', async () => {
	const paths = ['sort-kept-a', 'sort-kept-b'].map((name) => {
		const plain = sample(`${name}-plain.pdf`, 1);
		const path = plain.replace('-plain.pdf', '.pdf');
		cli('annot', 'note', plain, path, '--at', '400,500', '--text', `A note in ${name}`);
		return path;
	});
	const { browser, stop } = await launch(paths);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		await waitForRowCount(browser, 1, 'the note of the document shown');
		await (await browser.$('button[aria-label^="Sort by page, A–Z"]')).click();
		await (await browser.$('//*[@role="menuitemradio"][contains(., "Author")]')).click();
		await (await browser.$('button[aria-label="Sort by author, A–Z"]')).click();
		await (await browser.$('//*[@role="menuitemradio"][contains(., "Z–A")]')).click();
		await (await browser.$('button[aria-label="Sort by author, Z–A"]')).waitForExist({ timeoutMsg: 'the sort was not picked' });
		// The other document's list is sorted the same way (Settings keep the sort by default).
		await browser.keys(['Control', 'Tab']);
		await browser.waitUntil(async () => (await browser.$('[role=tab][aria-selected="true"]').getText()).includes('sort-kept-a'), {
			timeoutMsg: 'Ctrl+Tab did not show the other document'
		});
		await waitForDocument(browser);
		await waitForRowCount(browser, 1, 'the note of the other document');
		await (await browser.$('button[aria-label="Sort by author, Z–A"]')).waitForExist({ timeoutMsg: 'the other document’s list is not sorted as picked' });
	} finally {
		await stop();
	}
});

test('copying from the quick tools and the context menus, and a comment left empty', async () => {
	const path = sample('copy-menus.pdf', 2);

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const quick = () => browser.$('[role=toolbar][aria-label="Quick tools"]');
		const selected = () => browser.execute(() => document.querySelectorAll('rect.fill-selection').length > 0);
		const menuItem = (label) => browser.$(`//*[@role="menuitem"][contains(., "${label}")]`);
		const barShown = () => browser.execute(() => document.querySelector('[role=toolbar][aria-label="Highlight actions"]') !== null);

		// Copy on the quick tools shows a tick, then the bar closes; the text stays selected.
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await (await quick()).waitForDisplayed({ timeoutMsg: 'no quick tools over the selection' });
		await (await (await quick()).$('button[aria-label="Copy"]')).click();
		await (await browser.$('[role=toolbar][aria-label="Quick tools"] button[aria-label="Copied"]')).waitForExist({
			timeoutMsg: 'Copy showed no tick'
		});
		await (await quick()).waitForExist({ reverse: true, timeoutMsg: 'the quick tools stayed after Copy' });
		assert.equal(await selected(), true, 'Copy took the selection away');

		// The page's menu on selected text: Copy selection, and no annotation's items.
		const onSelection = await pagePoint(browser, 0, 100, 92);
		await browser.action('pointer').move({ ...onSelection, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await menuItem('Copy selection')).waitForDisplayed({ timeoutMsg: 'no Copy selection in the menu' });
		assert.equal(await (await menuItem('Copy selection')).getAttribute('data-disabled'), null);
		assert.equal(await (await menuItem('Copy comment')).isExisting(), false);
		await browser.keys('Escape');

		// "Highlight with note" opens the comment; left empty, it closes and the bar shows.
		await drag(browser, await pagePoint(browser, 0, 40, 116), await pagePoint(browser, 0, 200, 116), 1);
		await (await (await quick()).$('button[aria-label="Highlight with note"]')).click();
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'Highlight with note put no cursor in the comment' });
		await browser.keys('Escape');
		await browser.waitUntil(async () => !(await panelShown(browser)), { timeoutMsg: 'the empty comment stayed open' });
		await browser.waitUntil(barShown, { timeoutMsg: 'no bar after the empty comment closed' });
		await browser.keys('Escape');

		// Left by a click elsewhere on the page, which deselects it, the highlight clicked again
		// shows its bar, not the empty comment.
		await drag(browser, await pagePoint(browser, 0, 40, 140), await pagePoint(browser, 0, 200, 140), 1);
		await (await (await quick()).$('button[aria-label="Highlight with note"]')).click();
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'Highlight with note put no cursor in the comment' });
		await click(browser, await pagePoint(browser, 0, 450, 40));
		await browser.waitUntil(async () => !(await panelShown(browser)), { timeoutMsg: 'a click away left the comment open' });
		await click(browser, await pagePoint(browser, 0, 100, 140));
		await browser.waitUntil(barShown, { timeoutMsg: 'no bar on the highlight clicked again' });
		assert.equal(await panelShown(browser), false, 'the highlight clicked again showed its empty comment');
		await click(browser, await pagePoint(browser, 0, 450, 40));

		// A double-click opens the comment too; with text typed it stays.
		const onHighlight = await pagePoint(browser, 0, 100, 116);
		await browser.action('pointer').move({ ...onHighlight, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		await browser.waitUntil(() => inPanelNote(browser), { timeoutMsg: 'a double-click put no cursor in the comment' });
		await browser.keys([...'Kept']);
		await browser.keys('Escape');
		await browser.pause(200);
		assert.equal(await panelShown(browser), true, 'a comment with text closed');
		await click(browser, await pagePoint(browser, 0, 300, 50));
		await browser.waitUntil(async () => !(await panelShown(browser)), { timeoutMsg: 'a click away left the comment open' });

		// A right-click on the highlight selects it for the menu without opening its comment,
		// or its bar while the menu is open. Copy comment and Copy selection both apply.
		await browser.action('pointer').move({ ...onHighlight, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await menuItem('Copy comment')).waitForDisplayed({ timeoutMsg: 'no Copy comment on the highlight' });
		assert.equal(await (await menuItem('Copy comment')).getAttribute('data-disabled'), null);
		assert.equal(await (await menuItem('Copy selection')).getAttribute('data-disabled'), null);
		assert.equal(await panelShown(browser), false, 'the right-click opened the comment');
		assert.equal(await barShown(), false, 'the bar showed with the menu');

		// The menu stays by the highlight as the page scrolls.
		const menuTop = () => browser.execute(() => document.querySelector('.menu-content').getBoundingClientRect().top);
		const before = await menuTop();
		await browser.execute(() => {
			document.querySelector('[role=document]').scrollTop += 40;
		});
		await browser.waitUntil(async () => Math.abs((await menuTop()) - (before - 40)) <= 1, {
			timeoutMsg: 'the menu did not follow the page'
		});
		await browser.keys('Escape');
		await browser.waitUntil(barShown, { timeoutMsg: 'no bar once the menu closed' });

		// The list's menu has both too.
		await showAnnotations(browser);
		await (await browser.$('.annotation-row')).click({ button: 'right' });
		await (await menuItem('Copy selection')).waitForDisplayed({ timeoutMsg: 'no Copy selection on the row' });
		assert.equal(await (await menuItem('Copy selection')).getAttribute('data-disabled'), null);
		await browser.keys('Escape');
	} finally {
		await stop();
	}
});

test('a click selects an annotation with any tool but the pen, and a drag from it does what the tool does', async () => {
	const path = sample('select-with-tools.pdf', 1);

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		const selectedType = () =>
			browser.execute(
				() => document.querySelector('.annotation-row[aria-selected="true"] .annotation-type-icon')?.getAttribute('aria-label') ?? null
			);
		const activeTool = () =>
			browser.execute(() => document.querySelector('[role=toolbar][aria-label="Annotation tools"] [aria-pressed=true]')?.getAttribute('aria-label'));
		const expectSelected = async (type, message) =>
			browser.waitUntil(async () => (await selectedType()) === type, { timeoutMsg: `${message}: selected ${await selectedType()}` });
		// Esc deselects the annotation first, and leaves the tool as it was.
		const deselect = async (tool) => {
			await browser.keys('Escape');
			await expectSelected(null, 'Esc');
			assert.equal(await activeTool(), tool);
		};

		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		const onHighlight = await pagePoint(browser, 0, 60, 92);
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 120, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		assert.equal(await selectedType(), null, 'new markup was selected');

		// Highlight tool: a click on the highlight selects it and marks nothing; a drag from it
		// marks text, from where it was pressed.
		await click(browser, onHighlight);
		await expectSelected('Highlight', 'a click with the highlight tool');
		await browser.pause(300);
		assert.equal((await listRows(browser)).length, 1, 'a click on the highlight marked text');
		assert.equal(await activeTool(), 'Highlight');
		await deselect('Highlight');
		await drag(browser, await pagePoint(browser, 0, 100, 92), await pagePoint(browser, 0, 250, 92), 1);
		await waitForRowCount(browser, 2, 'a drag from the highlight with the highlight tool');

		// Pen: it draws over annotations, with its crosshair there too, and selects none.
		await browser.keys('p');
		await browser.action('pointer').move({ ...onHighlight, origin: 'viewport' }).perform();
		await browser.pause(300);
		assert.deepEqual(
			await browser.execute(() => {
				const c = document.querySelector('[role=document]').classList;
				return [c.contains('cursor-crosshair'), c.contains('cursor-pointer')];
			}),
			[true, false],
			'the pen showed the hand over the highlight'
		);
		const strokeFrom = await pagePoint(browser, 0, 70, 92);
		const strokeEnd = await pagePoint(browser, 0, 200, 220);
		await drag(browser, strokeFrom, strokeEnd);
		await waitForRowCount(browser, 3, 'a stroke from the highlight');
		// A corner of the stroke that `drag` drew, away from the resize handles.
		const onStroke = {
			x: Math.round(strokeFrom.x + (strokeEnd.x - strokeFrom.x) / 2),
			y: Math.round(strokeFrom.y + (strokeEnd.y - strokeFrom.y) / 2 - 6)
		};
		await drag(browser, onStroke, await pagePoint(browser, 0, 300, 300));
		await waitForRowCount(browser, 4, 'a stroke from the drawing');
		assert.equal(await selectedType(), null, 'the pen selected an annotation');
		assert.equal(await activeTool(), 'Pen');

		// Text box tool: a click selects the drawing; dragged then, it moves, and no text box
		// is begun.
		await browser.keys('t');
		await click(browser, onStroke);
		await expectSelected('Drawing', 'a click on the drawing with the text box tool');
		const outline = () => browser.execute(() => document.querySelector('.annotation-outline')?.getBoundingClientRect().toJSON());
		const was = await outline();
		// Pressed again at once, it would be a double-click, which opens the comment.
		await browser.pause(700);
		await drag(browser, onStroke, { x: onStroke.x + 40, y: onStroke.y + 40 }, 4);
		await browser.waitUntil(async () => Math.abs((await outline()).left - was.left - 40) < 4, {
			timeoutMsg: `the drawing did not move: ${JSON.stringify(await outline())} from ${JSON.stringify(was)}`
		});
		assert.equal((await listRows(browser)).length, 4, 'a drag of the selected drawing made something');
		await expectSelected('Drawing', 'the moved drawing');
		await deselect('Text box');

		// Text box and note: a click on an annotation selects it, and makes nothing.
		await click(browser, onHighlight);
		await expectSelected('Highlight', 'a click with the text box tool');
		assert.equal(await (await browser.$('textarea[data-annotation-editor]')).isExisting(), false, 'a text box was begun');
		await deselect('Text box');
		await browser.keys('n');
		await click(browser, onHighlight);
		await expectSelected('Highlight', 'a click with the note tool');
		await browser.pause(300);
		assert.equal((await listRows(browser)).length, 4, 'a note was placed on the highlight');
	} finally {
		await stop();
	}
});

test('an area dragged with the Select tool selects annotations, which change, move and go together', async () => {
	const path = sample('select-area.pdf', 1);

	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await showAnnotations(browser);
		const picked = () =>
			browser.execute(() =>
				[...document.querySelectorAll('.annotation-row[aria-selected="true"] .annotation-type-icon')].map((i) => i.getAttribute('aria-label')).sort()
			);
		const expectPicked = async (types, message) =>
			browser.waitUntil(async () => JSON.stringify(await picked()) === JSON.stringify([...types].sort()), {
				timeoutMsg: `${message}: selected ${JSON.stringify(await picked())}`
			});
		const groupCount = () => browser.execute(() => document.querySelector('[data-group-count]')?.textContent?.trim() ?? null);
		const outlines = () =>
			browser.execute(() => [...document.querySelectorAll('.annotation-outline')].map((o) => o.getBoundingClientRect().toJSON()));
		const ctrlClick = async (at) => {
			await browser.action('key').down(Key.Ctrl).perform(true);
			await click(browser, at);
			await browser.action('key').up(Key.Ctrl).perform();
		};

		// A highlight on the marker line, a note and a drawing in the right margin (the sample's
		// text ends before 510 pt).
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		const onHighlight = await pagePoint(browser, 0, 60, 92);
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 120, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 550, 200));
		await waitForRowCount(browser, 2, 'note');
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
		await browser.keys('p');
		await drag(browser, await pagePoint(browser, 0, 540, 300), await pagePoint(browser, 0, 580, 360), 4);
		await waitForRowCount(browser, 3, 'drawing');
		await browser.keys(['Escape']);
		await expectPicked([], 'nothing selected');

		// An area dragged from the blank margin over the note and the drawing selects them,
		// and draws a rectangle while it is dragged.
		const from = await pagePoint(browser, 0, 588, 150);
		const to = await pagePoint(browser, 0, 520, 380);
		await browser
			.action('pointer')
			.move({ ...from, origin: 'viewport' })
			.down()
			.move({ x: Math.round((from.x + to.x) / 2), y: Math.round((from.y + to.y) / 2), origin: 'viewport', duration: 50 })
			.move({ ...to, origin: 'viewport', duration: 50 })
			.perform(true);
		assert.equal(await (await browser.$('.annotation-marquee')).isExisting(), true, 'no rectangle is drawn');
		// (Let go where it is: a new action starts from the corner of the view.)
		await browser.action('pointer').move({ ...to, origin: 'viewport' }).up().perform();
		assert.equal(await (await browser.$('.annotation-marquee')).isExisting(), false, 'the rectangle stayed');
		await expectPicked(['Drawing', 'Note'], 'the area');
		await browser.waitUntil(async () => (await groupCount()) === '2 selected', { timeoutMsg: `group bar: ${await groupCount()}` });

		// Ctrl+click adds the highlight, and takes it out again.
		await ctrlClick(onHighlight);
		await expectPicked(['Drawing', 'Highlight', 'Note'], 'Ctrl+click added');
		await ctrlClick(onHighlight);
		await expectPicked(['Drawing', 'Note'], 'Ctrl+click took out');

		// Dragged by one of them, both move; undone, both go back in one step.
		const was = await outlines();
		assert.equal(was.length, 2, JSON.stringify(was));
		const onNote = await pagePoint(browser, 0, 555, 205);
		await drag(browser, onNote, { x: onNote.x - 40, y: onNote.y + 40 }, 4);
		await browser.waitUntil(
			async () => {
				const now = await outlines();
				return now.length === 2 && now.every((r, i) => Math.abs(r.left - was[i].left + 40) < 4);
			},
			{ timeoutMsg: `they did not move together: ${JSON.stringify(await outlines())} from ${JSON.stringify(was)}` }
		);
		await expectPicked(['Drawing', 'Note'], 'still selected after the move');
		await browser.keys(['Control', 'z']);
		await browser.waitUntil(
			async () => {
				const now = await outlines();
				return now.length === 2 && now.every((r, i) => Math.abs(r.left - was[i].left) < 2);
			},
			{ timeoutMsg: `one undo did not take both back: ${JSON.stringify(await outlines())}` }
		);

		// The group's colour applies to them all.
		const icons = () =>
			browser.execute(() => [...document.querySelectorAll('.annotation-row .annotation-type-icon')].map((i) => getComputedStyle(i).color));
		const before = await icons();
		const bar = await browser.$('[role=toolbar][aria-label="2 annotations actions"]');
		await bar.waitForDisplayed({ timeoutMsg: 'no bar for the group' });
		const colours = await openColours(browser, bar);
		await (await colours.$('button[aria-label="Pink"]')).click();
		await browser.waitUntil(
			async () => {
				const now = await icons();
				return now.filter((c, i) => c !== before[i]).length === 2;
			},
			{ timeoutMsg: `the colour did not reach both: ${JSON.stringify(await icons())} from ${JSON.stringify(before)}` }
		);
		await browser.keys('Escape');

		// Del deletes them both in one step; undo brings them back.
		await click(browser, await pagePoint(browser, 0, 560, 600));
		await expectPicked([], 'a click on the page deselected');
		await browser.action('key').down(Key.Shift).perform(true);
		await drag(browser, from, to, 2);
		await browser.action('key').up(Key.Shift).perform();
		await expectPicked(['Drawing', 'Note'], 'the area again');
		await browser.keys('Delete');
		await waitForRowCount(browser, 1, 'Del on the group');
		await browser.keys(['Control', 'z']);
		await waitForRowCount(browser, 3, 'one undo of the delete');

		// With Shift, an area adds to what is selected; Esc while it is dragged puts the
		// selection back.
		await click(browser, onHighlight);
		await expectPicked(['Highlight'], 'a click on the highlight');
		await browser.action('key').down(Key.Shift).perform(true);
		await drag(browser, from, to, 2);
		await browser.action('key').up(Key.Shift).perform();
		await expectPicked(['Drawing', 'Highlight', 'Note'], 'Shift added the area');
		await browser.waitUntil(async () => (await groupCount()) === '3 selected', { timeoutMsg: `group bar: ${await groupCount()}` });
		await click(browser, onHighlight);
		await expectPicked(['Highlight'], 'a click picks one of them alone');
		await browser
			.action('pointer')
			.move({ ...from, origin: 'viewport' })
			.down()
			.move({ ...to, origin: 'viewport', duration: 50 })
			.perform(true);
		await expectPicked(['Drawing', 'Note'], 'the area while dragged');
		await browser.keys('Escape');
		// (Let go where it is: a new action starts from the corner of the view.)
		await browser.action('pointer').move({ ...to, origin: 'viewport' }).up().perform();
		await expectPicked(['Highlight'], 'Esc put the selection back');
		assert.equal(await (await browser.$('.annotation-marquee')).isExisting(), false, 'Esc left the rectangle');
	} finally {
		await stop();
	}
});
