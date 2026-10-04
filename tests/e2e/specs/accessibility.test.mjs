// Accessibility pass (AGENTS.md section 8): every control is reachable by
// keyboard, shows where focus is, and has an accessible name; a whole session works with
// the keyboard alone.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { focused, tabWalk, unnamedControls } from '../lib/a11y.mjs';
import { launch, waitForDocument } from '../lib/app.mjs';
import { cli, outlineLines, sample } from '../lib/pdfcli.mjs';

/** A 4-page sample with a sticky note on page 1. */
function notedSample(name) {
	const plain = sample(`${name}-plain.pdf`, 4);
	const path = plain.replace('-plain.pdf', '.pdf');
	cli('annot', 'note', plain, path, '--at', '100,100', '--text', 'A note');
	return path;
}

function describe(stop) {
	return `${stop.tag}[${stop.computedRole}] "${stop.name}" .${stop.cls}`;
}

/** Every Tab stop is named and shows focus (the stop where focus leaves the page aside). */
function assertStops(stops, where, atLeast = 4) {
	const real = stops.filter((s) => s.tag !== 'none');
	assert.ok(real.length >= atLeast, `${where}: Tab reaches controls`);
	for (const s of real) {
		assert.ok(s.name, `${where}: unnamed Tab stop ${describe(s)}`);
		assert.ok(s.visibleFocus, `${where}: focus not visible on ${describe(s)}`);
	}
	return real.map((s) => s.name);
}

async function assertAllNamed(browser, where) {
	assert.deepEqual(await unnamedControls(browser), [], `${where}: controls without an accessible name`);
}

/** Presses Tab until `match(stop)` or gives up after `max` presses. */
async function tabTo(browser, match, what, max = 60) {
	for (let i = 0; i < max; i++) {
		await browser.keys(['Tab']);
		const stop = await focused(browser);
		if (stop && match(stop)) return stop;
	}
	throw new Error(`Tab never reached ${what}`);
}

async function statusText(browser) {
	return browser.execute(() => document.querySelector('footer')?.textContent ?? '');
}

test('every Tab stop is named and shows focus, in every panel and dialog', async () => {
	let { browser, stop } = await launch([]);
	try {
		await browser.waitUntil(() => browser.execute(() => document.body.innerText.includes('Combine files')));
		const names = assertStops(await tabWalk(browser), 'start screen');
		for (const name of ['File', 'Open… Ctrl+O', 'Combine files…', 'Close']) assert.ok(names.includes(name), `start screen reaches ${name}`);
		await assertAllNamed(browser, 'start screen');
	} finally {
		await stop();
	}

	({ browser, stop } = await launch([notedSample('a11y-stops')]));
	try {
		await waitForDocument(browser);
		const names = assertStops(await tabWalk(browser), 'document');
		for (const name of ['File', 'Pages', 'Page 1', 'Highlight', 'Text box', 'Go to page', 'Zoom in (Ctrl+=)']) {
			assert.ok(names.includes(name), `document view reaches ${name}`);
		}
		await assertAllNamed(browser, 'document');

		for (const panel of ['Bookmarks', 'Page labels']) {
			await (await browser.$(`button[role="tab"][aria-label="${panel}"]`)).click();
			await browser.pause(200);
			assertStops(await tabWalk(browser, { max: 80 }), `${panel} panel`);
			await assertAllNamed(browser, `${panel} panel`);
		}

		// The annotation pane on the right, and the edges that resize both panes.
		await (await browser.$('button[aria-label="Show annotations"]')).click();
		await browser.pause(300);
		const withPane = assertStops(await tabWalk(browser, { max: 100 }), 'annotation pane');
		for (const name of ['Hide annotations', 'Show type', 'Resize sidebar', 'Resize annotations']) {
			assert.ok(withPane.includes(name), `annotation pane reaches ${name}`);
		}
		await assertAllNamed(browser, 'annotation pane');

		// A selected annotation: its row, the inspector and the toolbar's colours.
		await (await browser.$('.annotation-row')).click();
		await browser.pause(300);
		const withInspector = assertStops(await tabWalk(browser, { max: 100 }), 'annotation inspector');
		for (const name of ['Yellow', 'Custom colour', 'Opacity', 'Author', 'Delete']) {
			assert.ok(withInspector.includes(name), `inspector reaches ${name}`);
		}
		await assertAllNamed(browser, 'annotation inspector');

		await browser.keys(['Control', 'f']);
		await browser.waitUntil(async () => (await focused(browser))?.name === 'Find in document', {
			timeoutMsg: 'Ctrl+F did not focus the search field'
		});
		assertStops(await tabWalk(browser, { max: 100 }), 'search');
		await browser.keys(['Escape']);
	} finally {
		await stop();
	}
});

test('menus, tabs and lists move with arrow keys; dialogs keep focus inside', async () => {
	const { browser, stop } = await launch([notedSample('a11y-arrows')]);
	try {
		await waitForDocument(browser);

		// Sidebar tabs: one Tab stop, arrows move between them.
		await (await browser.$('button[role="tab"][aria-label="Pages"]')).click();
		await browser.keys(['ArrowRight']);
		assert.equal((await focused(browser)).name, 'Bookmarks');
		await browser.keys(['ArrowLeft']);
		assert.equal((await focused(browser)).name, 'Pages');

		// Thumbnails: Down moves to the next page.
		const thumbs = await tabTo(browser, (s) => s.computedRole === 'option' && s.name.startsWith('Page'), 'the thumbnails');
		await browser.keys(['ArrowDown']);
		const next = await focused(browser);
		assert.notEqual(next.name, thumbs.name, 'Down moves to another thumbnail');
		assert.ok(next.visibleFocus, 'the thumbnail shows focus');

		// Context menus open from the keyboard: a thumbnail's, then a bookmark's.
		const menuText = () => browser.execute(() => document.querySelector('[role="menu"]')?.textContent ?? null);
		await browser.keys(['Shift', 'F10']);
		await browser.waitUntil(async () => (await menuText())?.includes('Rotate'), {
			timeoutMsg: 'Shift+F10 did not open the thumbnail menu'
		});
		await browser.keys(['Escape']);
		await browser.keys(['Control', 'b']);
		await browser.waitUntil(async () => (await focused(browser))?.tag === 'input', { timeoutMsg: 'no rename field after Ctrl+B' });
		await browser.keys(['Enter']);
		await browser.waitUntil(async () => (await focused(browser))?.computedRole === 'tree');
		// (WebDriver has no code for the Menu key; contextmenu.test.ts covers it.)
		await browser.keys(['Shift', 'F10']);
		await browser.waitUntil(async () => (await menuText())?.includes('Properties'), {
			timeoutMsg: 'Shift+F10 did not open the bookmark menu'
		});
		await browser.keys(['Escape']);

		// The sidebar's edge, after its content: arrows resize it, Home goes to the narrowest.
		await tabTo(browser, (s) => s.name === 'Resize sidebar', 'the sidebar edge');
		const sidebarWidth = () =>
			browser.execute(() => Number(document.querySelector('[role="separator"][aria-label="Resize sidebar"]').getAttribute('aria-valuenow')));
		const before = await sidebarWidth();
		await browser.keys(['ArrowRight']);
		assert.equal(await sidebarWidth(), before + 16, 'Right widens the sidebar');
		await browser.keys(['Home']);
		assert.equal(await sidebarWidth(), 180, 'Home makes it narrowest');

		// Menu bar: Enter opens a menu, arrows move through items with visible focus.
		const file = await browser.$('button[role="menuitem"]');
		await file.click();
		await browser.keys(['ArrowDown']);
		const item = await focused(browser);
		assert.equal(item.computedRole, 'menuitem');
		assert.ok(item.visibleFocus, `focus is visible on menu item "${item.name}"`);
		await browser.keys(['ArrowRight']);
		assert.equal((await focused(browser)).computedRole, 'menuitem', 'Right opens the next menu');
		await browser.keys(['Escape']);

		// Settings: Tab stays inside the dialog; Escape closes it.
		await browser.keys(['Control', ',']);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') !== null));
		const stops = await tabWalk(browser, { max: 30 });
		assertStops(stops, 'Settings');
		const outside = await browser.execute(() => !document.activeElement?.closest('[role="dialog"]'));
		assert.equal(outside, false, 'focus stayed in the Settings dialog');
		await browser.keys(['Escape']);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Escape did not close Settings'
		});

		// Help > About: the version and where the source code is (AGPL section 6).
		await (await browser.$('button[role="menuitem"]=Help')).click();
		await (await browser.$('[role="menuitem"]*=About')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') !== null));
		const about = await browser.execute(() => ({
			text: document.querySelector('[role="dialog"]')?.textContent ?? '',
			source: document.querySelector('[role="dialog"] input')?.value ?? ''
		}));
		assert.match(about.text, /Version \d+\.\d+\.\d+/);
		assert.match(about.text, /GNU Affero\s+General Public License/);
		assert.match(about.source, /^https:\/\//);
		const aboutStops = assertStops(await tabWalk(browser, { max: 20 }), 'About', 3);
		assert.ok(aboutStops.includes('Source code') && aboutStops.includes('Copy'), `About reaches ${aboutStops.join(', ')}`);
		await browser.keys(['Escape']);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Escape did not close About'
		});
	} finally {
		await stop();
	}
});

test('a keyboard-only session: go to a page, bookmark it, edit a note, save', async () => {
	const path = notedSample('a11y-keyboard');
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);

		await browser.keys(['Control', 'g']);
		assert.equal((await focused(browser)).name, 'Go to page');
		await browser.keys(['Control', 'a']);
		await browser.keys(['3', 'Enter']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('3 of 4') || (await browser.execute(() => document.querySelector('#page-box')?.value)) === '3', {
			timeoutMsg: 'Ctrl+G and Enter did not go to page 3'
		});

		// Ctrl+B adds a bookmark here and opens it for renaming.
		await browser.keys(['Escape']);
		await browser.keys(['Control', 'b']);
		await browser.waitUntil(async () => (await focused(browser))?.tag === 'input', { timeoutMsg: 'no rename field after Ctrl+B' });
		await browser.keys(['Control', 'a']);
		await browser.keys([...'Chapter three', 'Enter']);

		// Open the annotation pane and reach its list, by Tab and Enter only.
		await tabTo(browser, (s) => s.name === 'Show annotations', 'the annotation pane button');
		await browser.keys(['Enter']);
		await tabTo(browser, (s) => s.computedRole === 'listbox' && s.name === 'Annotations', 'the annotation list');
		await browser.keys(['ArrowDown']);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('.annotation-row[aria-selected="true"]') !== null), {
			timeoutMsg: 'Down did not select the note'
		});
		// The selected row's note text is the next stop.
		const note = await tabTo(browser, (s) => s.tag === 'textarea', 'the note text', 5);
		assert.equal(note.name, 'Note');
		await browser.keys(['Control', 'a']);
		await browser.keys([...'Edited by keyboard']);

		// Saving straight from the note field commits the text first.
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), {
			timeout: 10_000,
			timeoutMsg: 'saving did not finish'
		});
	} finally {
		await stop();
	}

	assert.ok(
		outlineLines(path).some((l) => l.includes('Chapter three') && l.includes('page 3')),
		`bookmark on page 3: ${outlineLines(path).join(' | ')}`
	);
	assert.match(cli('annot', 'list', path), /Edited by keyboard/);
});
