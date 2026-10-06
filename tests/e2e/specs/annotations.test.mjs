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
		await (await browser.$('p*=No annotations yet')).waitForDisplayed();

		// Highlight: H, then select text on the marker line (sample pages put it 96 pt from
		// the top, from 36 pt in).
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 1, 'highlight');
		assert.match(await statusText(browser), /Unsaved changes/);

		// Note: N, click, type its text in the inspector. The new highlight is not selected
		// (section 6.5); Escape would deselect it, since its inspector would cover the spot on
		// the page (the annotation pane leaves the page narrower).
		await browser.keys(['Escape']);
		await browser.keys(['Escape']);
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 400, 150));
		await waitForRowCount(browser, 2, 'note');
		const note = await browser.$('aside[aria-label="Annotation comment"] textarea');
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
		// Selected: no Delete button in the row (the menu and the Delete key delete); its
		// note field has the same text size as the comments, and no scrollbar.
		const row = await browser.execute(() => {
			const selected = document.querySelector('.annotation-row[aria-selected="true"]');
			const field = selected.querySelector('.comment-field');
			return {
				buttons: selected.querySelectorAll('button').length,
				field: getComputedStyle(field).fontSize,
				text: getComputedStyle(document.querySelector('.annotation-row p')).fontSize,
				scrolls: field.scrollHeight > field.clientHeight + 1
			};
		});
		assert.deepEqual(row, { buttons: 0, field: row.text, text: row.text, scrolls: false });
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
		await (await browser.$('button[role="tab"]*=General')).click();
		await (await browser.$('#setting-author')).setValue('E2E Tester');
		await (await browser.$('button[role="tab"]*=Appearance')).click();
		await (await browser.$('#setting-appearance')).click();
		await (await browser.$('[role="option"]*=Dark')).click();
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

		// The new highlight is not selected: no bar, no inspector. A click selects it, and its
		// bar turns it into an underline, then makes it pink.
		const highlightBar = await browser.$('[role=toolbar][aria-label="Highlight actions"]');
		assert.equal(await highlightBar.isExisting(), false, 'the new highlight was selected');
		assert.equal(await (await browser.$('aside[aria-label="Annotation comment"]')).isExisting(), false);
		// (Away from where the double-click below lands, which would count this press as its first.)
		await click(browser, await pagePoint(browser, 0, 60, 92));
		await highlightBar.waitForDisplayed({ timeoutMsg: 'no bar for the clicked highlight' });
		await (await highlightBar.$('button[aria-label="Underline"]')).click();
		const underlineBar = await browser.$('[role=toolbar][aria-label="Underline actions"]');
		await underlineBar.waitForDisplayed({ timeoutMsg: 'the highlight did not become an underline' });
		// The bar shows only the colour in use; its button opens the colours, on the one in use.
		assert.equal((await underlineBar.$$('.swatch')).length, 0, 'the bar shows every colour');
		const colours = await openColours(browser, underlineBar);
		await browser.waitUntil(async () => (await focusedLabel(browser)) === 'Yellow', { timeoutMsg: 'the colours did not open on the one in use' });
		await (await colours.$('button[aria-label="Pink"]')).click();
		await browser.waitUntil(async () => (await (await colours.$('button[aria-label="Pink"]')).getAttribute('aria-checked')) === 'true', {
			timeoutMsg: 'the underline did not turn pink'
		});
		await browser.keys(['Escape']);
		await colours.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		assert.match(await focusedLabel(browser), /^Colour: Pink/, 'the focus did not return to the colour button');
		assert.match((await listRows(browser))[0], /^Underline/);

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
		await browser.waitUntil(
			() => browser.execute(() => document.activeElement?.matches('.annotation-row[aria-selected="true"] textarea.comment-field') === true),
			{ timeoutMsg: 'the double-click did not focus the comment in the list' }
		);
		assert.equal(await (await browser.$('aside[aria-label="Annotation comment"]')).isExisting(), false, 'the double-click opened the comment panel');
		await browser.keys([...'Quick note']);
		await browser.keys(['Control', 'Enter']);

		// The bar's Note button opens the comment panel with the cursor in the note, which
		// shows the comment typed in the list.
		await (await underlineBar.$('button[aria-label="Note"]')).click();
		await browser.waitUntil(
			() =>
				browser.execute(
					() => document.activeElement?.closest('aside[aria-label="Annotation comment"]') !== null && document.activeElement?.tagName === 'TEXTAREA'
				),
			{ timeoutMsg: 'the Note button did not focus the note' }
		);
		assert.equal(await browser.execute(() => document.activeElement.value), 'Quick note', 'the comment typed in the list was not kept');
		await browser.keys(['Control', 'Enter']);

		// The comment panel takes the bar's place, beside the underline.
		assert.equal(await underlineBar.isExisting(), false, 'the bar stayed beside the comment panel');
		const commentPanel = await browser.$('aside[aria-label="Annotation comment"]');
		const panelBox = () =>
			browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"]').getBoundingClientRect().toJSON());
		const placed = await panelBox();
		assert.ok(placed.left > on.x && Math.abs(placed.top - on.y) < 60, `panel at ${JSON.stringify(placed)}, underline at ${JSON.stringify(on)}`);

		// The colour panel has the opacity slider too; it applies when let go.
		let colourPanel = await openColours(browser, commentPanel);
		const slider = await colourPanel.$('input[type=range][aria-label="Opacity"]');
		// (It was a highlight, made at the highlight's 40%.)
		assert.equal(await slider.getValue(), '0.4');
		await browser.execute(() => document.querySelector('.color-panel input[type=range]').focus());
		await browser.keys(['ArrowLeft']);
		await browser.keys(['Escape']);
		await colourPanel.waitForExist({ reverse: true, timeoutMsg: 'Esc left the colours open' });
		colourPanel = await openColours(browser, commentPanel);
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

		// A tail on its border points at the underline, from the side facing it.
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

		// Closed and opened again, it comes back where it was put: with a comment now, a click
		// shows the underline's comment panel rather than its bar.
		const away = await pagePoint(browser, 0, 450, 200);
		assert.ok(away.x > grown.right && away.y < viewer.bottom, `the blank spot ${JSON.stringify(away)} is under the panel or off screen`);
		await click(browser, away);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"]') === null), {
			timeoutMsg: 'a click on the page left the comment panel open'
		});
		await click(browser, on);
		await (await browser.$('aside[aria-label="Annotation comment"]')).waitForDisplayed({ timeoutMsg: 'a click on the commented underline showed no comment panel' });
		assert.equal(await underlineBar.isExisting(), false, 'a click on the commented underline showed its bar');
		const back = await panelBox();
		assert.ok(Math.abs(back.left - grown.left) <= 1 && Math.abs(back.top - grown.top) <= 1, `back at ${JSON.stringify(back)}, put at ${JSON.stringify(grown)}`);
		assert.ok(Math.abs(back.width - grown.width) <= 1 && Math.abs(back.height - grown.height) <= 1, `back as ${JSON.stringify(back)}, sized ${JSON.stringify(grown)}`);
		// Its header has the bar's controls: the type buttons and Delete.
		assert.equal(
			await browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"] button[aria-label="Underline"]')?.getAttribute('aria-pressed')),
			'true',
			'no type buttons in the comment panel'
		);
		// Esc from its controls closes it, and the bar takes its place.
		await browser.execute(() => document.querySelector('aside[aria-label="Annotation comment"] button[aria-label="Delete"]').focus());
		await browser.keys(['Escape']);
		await (await browser.$('[role=toolbar][aria-label="Underline actions"]')).waitForDisplayed({ timeoutMsg: 'Esc on the comment panel did not bring the bar back' });
		assert.equal(await (await browser.$('aside[aria-label="Annotation comment"]')).isExisting(), false, 'Esc left the comment panel open');

		// Another annotation's panel takes the width given above, but no more height than its
		// comment needs: a new note, with no comment yet, gets a shorter one.
		await browser.keys('n');
		await click(browser, await pagePoint(browser, 0, 450, 260));
		await (await browser.$('aside[aria-label="Annotation comment"]')).waitForDisplayed({ timeoutMsg: 'the new note opened no comment panel' });
		const trimmed = await panelBox();
		assert.ok(Math.abs(trimmed.width - grown.width) <= 1, `the note's panel is ${trimmed.width} wide, the last size ${grown.width}`);
		assert.ok(trimmed.height < grown.height - 40, `the note's panel is ${trimmed.height} tall, the last size ${grown.height}`);
		await (await browser.$('aside[aria-label="Annotation comment"] button[aria-label="Delete"]')).click();
		await (await browser.$('aside[aria-label="Annotation comment"]')).waitForExist({ reverse: true, timeoutMsg: 'Delete in the panel left the note' });

		// The colour given to the underline (pink, above) is what new underlines get.
		await browser.keys(['Escape']);
		await browser.keys('u');
		const tools = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		await (await tools.$('[data-color-button]')).waitForDisplayed({ timeoutMsg: 'no colour for the Underline tool' });
		assert.match(await (await tools.$('[data-color-button]')).getAttribute('aria-label'), /^Colour: Pink/, 'new underlines are not pink');
		await browser.keys(['Escape']);

		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });

		// Settings: the annotation toolbar moves to the top, shown only near it.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
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

		// Settings: docked in the bar above the pages instead, beside the page and zoom
		// controls, always shown; the floating options don't apply to it.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('#setting-toolbarStyle')).click();
		await (await browser.$('[role="option"]*=In the bar above the pages')).click();
		assert.equal(await (await browser.$('#setting-toolbarVisibility')).getAttribute('data-disabled'), '');
		assert.equal(await (await browser.$('#setting-toolbarPosition')).getAttribute('data-disabled'), '');
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

		// Settings: new markup opens its comment. A highlight then has the inspector open with
		// the cursor in the note.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('label*=Add a comment after marking text')).click();
		await closeSettings(browser);
		await (await browser.$('[role=document]')).click();
		await browser.keys('h');
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 92), 1);
		await waitForRowCount(browser, 2, 'highlight with its comment open');
		await browser.waitUntil(
			() =>
				browser.execute(
					() => document.activeElement?.closest('aside[aria-label="Annotation comment"]') !== null && document.activeElement?.tagName === 'TEXTAREA'
				),
			{ timeoutMsg: 'the new highlight’s note is not focused' }
		);
		await browser.keys([...'Comment at once']);
		await browser.keys(['Control', 'Enter']);
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), { timeoutMsg: 'not saved' });

		// Settings: without the last colour, new annotations start from the defaults again
		// (the underline's red, not the pink given to one above).
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		await (await browser.$('label*=Use the last colour and opacity')).click();
		await closeSettings(browser);
		await (await browser.$('[role=toolbar][aria-label="Annotation tools"] button[aria-label="Underline"]')).click();
		const docked = await browser.$('[role=toolbar][aria-label="Annotation tools"]');
		const toolColour = await openColours(browser, docked);
		assert.equal(await (await toolColour.$('button[aria-label="Red"]')).getAttribute('aria-checked'), 'true', 'new underlines kept the last colour');
		await browser.keys(['Escape']);
		await toolColour.waitForExist({ reverse: true });
		await browser.keys(['Escape']);

		// With the annotation list hidden, a double-click opens the comment panel instead.
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
		await browser.waitUntil(
			() =>
				browser.execute(
					() => document.activeElement?.closest('aside[aria-label="Annotation comment"]') !== null && document.activeElement?.tagName === 'TEXTAREA'
				),
			{ timeoutMsg: 'without the list, the double-click did not open the comment panel' }
		);
	} finally {
		await stop();
	}
	const saved = onDisk(path);
	assert.equal(saved.length, 2, saved.join('\n'));
	assert.ok(saved.some((l) => /Underline .*text "Quick note"/.test(l)), saved.join('\n'));
	assert.ok(saved.some((l) => /Highlight .*text "Comment at once"/.test(l)), saved.join('\n'));
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
		const types = await browser.$('[role=group][aria-labelledby^="filter-types-"]');
		await (await types.$('button[aria-label="Note"]')).click();
		await waitForRowCount(browser, 1, 'the Note filter');
		assert.match((await listRows(browser))[0], /^Note/);
		assert.ok(await (await browser.$('[role=group][aria-labelledby^="filter-colours-"] button[aria-label="Yellow"]')).isExisting(), 'no yellow in the colours');
		assert.equal((await browser.$$('[role=group][aria-labelledby^="filter-authors-"] .filter-pill')).length, 1);
		const subheadings = await browser.execute(() => [...document.querySelectorAll('.filter-heading')].map((h) => h.textContent.trim()));
		assert.deepEqual(subheadings, ['Type', 'Colour', 'Author']);
		await (await browser.$('button=Clear filters')).click();
		await waitForRowCount(browser, 2, 'Clear filters');

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
