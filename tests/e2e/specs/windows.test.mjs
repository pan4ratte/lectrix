// Document tabs and windows (AGENTS.md section 8): tabs are reordered by dragging them
// along their row or from the keyboard, and a tab moves into a window of its own, or into
// another window, with its document as it is (unsaved changes, its place). File > Exit
// closes every window, each after its own question about unsaved changes.
//
// Dragging a tab out of the window is not scripted (WebDriver's pointer stays inside the
// page); moving goes through the tab's context menu and the File menu, which call the same
// code as a drop. A tab dropped inside its window stays there.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { focused } from '../lib/a11y.mjs';
import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const TABS = '[role=tablist][aria-label="Open documents"] [data-tab]';

/** The document tabs' names, in order. */
const tabNames = (browser) => browser.execute((s) => [...document.querySelectorAll(s)].map((t) => t.textContent.trim()), TABS);

const tabOf = (browser, name) => browser.$(`.doc-tab*=${name}`);
const menuItem = (browser, name) => browser.$(`//*[@role="menuitem"][contains(., "${name}")]`);
const pageBox = (browser) => browser.execute(() => document.querySelector('#page-box')?.value ?? null);

/** The middle of a tab, and its edges. */
function box(browser, name) {
	return browser.execute(
		(s, n) => {
			const el = [...document.querySelectorAll(s)].find((t) => t.textContent.includes(n));
			if (!el) return null;
			const r = el.getBoundingClientRect();
			return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), left: r.left, right: r.right };
		},
		TABS,
		name
	);
}

/** Drags from one point to another in small steps, as a hand would. */
async function drag(browser, from, to, { atTarget } = {}) {
	let action = browser.action('pointer').move({ x: from.x, y: from.y, origin: 'viewport' }).down();
	for (let i = 1; i <= 10; i++) {
		action = action
			.move({ x: Math.round(from.x + ((to.x - from.x) * i) / 10), y: Math.round(from.y + ((to.y - from.y) * i) / 10), origin: 'viewport' })
			.pause(16);
	}
	await action.perform(true);
	if (atTarget) await atTarget();
	await browser.action('pointer').up().perform();
}

/** Switches the session to the window whose tabs include `name`, once there is one. */
async function switchToWindowWith(browser, name) {
	let found = false;
	await browser.waitUntil(
		async () => {
			for (const handle of await browser.getWindowHandles()) {
				await browser.switchToWindow(handle);
				const names = await tabNames(browser).catch(() => []);
				if (names.some((n) => n.includes(name))) {
					found = true;
					return true;
				}
			}
			return false;
		},
		{ timeout: 20_000, timeoutMsg: `no window shows ${name}` }
	);
	return found;
}

const windowCount = async (browser) => (await browser.getWindowHandles()).length;

test('tabs are reordered by dragging and from the keyboard', async () => {
	const files = ['order-a.pdf', 'order-b.pdf', 'order-c.pdf'].map((name) => sample(name, 2));
	const { browser, stop } = await launch(files);
	try {
		await waitForDocument(browser);
		assert.deepEqual(await tabNames(browser), ['order-a.pdf', 'order-b.pdf', 'order-c.pdf']);

		// Dragged past the others, the first tab follows the pointer and they make way.
		const a = await box(browser, 'order-a.pdf');
		const c = await box(browser, 'order-c.pdf');
		await drag(browser, a, { x: c.right - 4, y: c.y }, {
			atTarget: async () => {
				const held = await browser.execute(() => {
					const el = document.querySelector('.doc-tab-held');
					return el ? { name: el.textContent.trim(), transform: getComputedStyle(el).transform } : null;
				});
				assert.equal(held?.name, 'order-a.pdf', 'the dragged tab is not held');
				assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-c.pdf', 'order-a.pdf'], 'the tabs did not make way during the drag');
			}
		});
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-c.pdf', 'order-a.pdf']);
		assert.equal(await browser.execute(() => document.querySelector('.doc-tab-held')), null, 'the tab is still held after the drop');
		assert.equal(await browser.execute((s) => [...document.querySelectorAll(s)].some((t) => t.style.transform), TABS), false);
		// The dragged tab is the one shown.
		assert.ok((await browser.$('[role=tab][aria-selected="true"]').getText()).includes('order-a.pdf'));

		// And back to the left, one place.
		const last = await box(browser, 'order-a.pdf');
		const middle = await box(browser, 'order-c.pdf');
		await drag(browser, last, { x: middle.left + 10, y: middle.y });
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-a.pdf', 'order-c.pdf']);

		// A short drag down stays a reorder: the tab is not pulled off its row.
		assert.equal(await windowCount(browser), 1);

		// From the keyboard, with the focus staying on the tab.
		await (await tabOf(browser, 'order-a.pdf')).click();
		await browser.keys(['Control', 'Shift', 'ArrowLeft']);
		assert.deepEqual(await tabNames(browser), ['order-a.pdf', 'order-b.pdf', 'order-c.pdf']);
		assert.ok((await focused(browser)).name.includes('order-a.pdf'), 'the moved tab lost the focus');
		await browser.keys(['Control', 'Shift', 'ArrowRight']);
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-a.pdf', 'order-c.pdf']);

		// And from the tab's menu.
		await browser.keys(['Shift', 'F10']);
		await (await menuItem(browser, 'Move right')).click();
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-c.pdf', 'order-a.pdf']);
		await (await tabOf(browser, 'order-a.pdf')).click({ button: 'right' });
		assert.equal(await (await menuItem(browser, 'Move right')).getAttribute('aria-disabled'), 'true', 'the last tab can move right');
		await browser.keys(['Escape']);

		// Pulled down off the row, the tab is on its way out: a stand-in follows the pointer
		// and the tab waits in its place. Esc leaves it where it was.
		const main = await browser.getWindowHandle();
		const pulled = await box(browser, 'order-c.pdf');
		const below = { x: pulled.x + 40, y: pulled.y + 140 };
		const onItsWay = async () => {
			assert.ok((await (await browser.$('.panel-drag-ghost')).getText()).includes('order-c.pdf'), 'nothing follows the pointer off the row');
			assert.equal(await browser.execute(() => document.querySelector('.doc-tab-held')), null, 'the tab still follows the pointer along the row');
		};
		await drag(browser, pulled, below, {
			atTarget: async () => {
				await onItsWay();
				await browser.keys(['Escape']);
				assert.equal(await (await browser.$('.panel-drag-ghost')).isExisting(), false, 'Esc did not end the drag');
			}
		});
		await browser.pause(300);
		assert.equal(await windowCount(browser), 1, 'a drag ended with Esc moved the tab');
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-c.pdf', 'order-a.pdf']);

		// Dropped there, inside its window, it stays, and is the tab shown.
		await drag(browser, pulled, below, { atTarget: onItsWay });
		await browser.pause(500);
		assert.equal(await windowCount(browser), 1, 'a tab dropped inside its window got a window of its own');
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-c.pdf', 'order-a.pdf']);
		assert.ok((await browser.$('[role=tab][aria-selected="true"]').getText()).includes('order-c.pdf'), 'the dropped tab is not shown');

		// Its menu gives it a window of its own (as a drop outside the window does, which
		// WebDriver's pointer can't reach).
		await (await tabOf(browser, 'order-c.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Move to new window')).click();
		await browser.waitUntil(async () => (await windowCount(browser)) === 2, { timeout: 20_000, timeoutMsg: 'the tab got no window' });
		await browser.waitUntil(async () => (await tabNames(browser)).length === 2, { timeoutMsg: 'the dropped tab stayed in its window' });
		assert.deepEqual(await tabNames(browser), ['order-b.pdf', 'order-a.pdf']);
		assert.ok(await switchToWindowWith(browser, 'order-c.pdf'));
		assert.notEqual(await browser.getWindowHandle(), main);
		await waitForDocument(browser);
		assert.deepEqual(await tabNames(browser), ['order-c.pdf']);
		// Its window is named after it, for the taskbar; the first one after what it shows.
		await browser.waitUntil(
			() => browser.execute(async () => (await window.__TAURI_INTERNALS__.invoke('list_windows')).some((w) => w.title === 'order-a.pdf - Lectrix')),
			{ timeoutMsg: 'the first window is not titled after its document' }
		);
	} finally {
		await stop();
	}
});

test('a tab moves to a window of its own and into another window, as it is', async () => {
	const first = sample('window-a.pdf', 3);
	const second = sample('window-b.pdf', 6);
	// Ctrl+O, later, picks the file that is open in the other window.
	const run = await launch([first, second], { dialogs: [second] });
	const { browser } = run;
	let quit = false;
	try {
		await waitForDocument(browser);
		const main = await browser.getWindowHandle();

		// window-b.pdf: on page 3, with a change that is not saved.
		await browser.keys(['Control', 'g']);
		await browser.keys(['Control', 'a']);
		await browser.keys(['3', 'Enter']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '3', { timeoutMsg: 'did not go to page 3' });
		await browser.keys(['Escape']);
		await browser.keys(['Control', 'b']);
		await browser.keys(['Enter']);
		await (await browser.$('.doc-tab*=window-b.pdf')).$('[data-unsaved]').waitForExist({ timeoutMsg: 'the bookmark did not mark the tab unsaved' });

		// Into a window of its own, from its menu.
		await (await tabOf(browser, 'window-b.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Move to new window')).click();
		await browser.waitUntil(async () => (await windowCount(browser)) === 2, { timeout: 20_000, timeoutMsg: 'no second window opened' });
		await browser.switchToWindow(main);
		await browser.waitUntil(async () => (await tabNames(browser)).length === 1, { timeoutMsg: 'the tab stayed in its first window' });
		assert.deepEqual(await tabNames(browser), ['window-a.pdf']);
		await waitForDocument(browser);

		// The new window shows the document where it was, still unsaved, with its history.
		assert.ok(await switchToWindowWith(browser, 'window-b.pdf'));
		const other = await browser.getWindowHandle();
		assert.notEqual(other, main);
		await waitForDocument(browser);
		assert.deepEqual(await tabNames(browser), ['window-b.pdf']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '3', { timeoutMsg: 'the moved tab did not open on page 3' });
		assert.ok(await (await browser.$('.doc-tab*=window-b.pdf')).$('[data-unsaved]').isExisting(), 'the moved tab is not marked unsaved');
		await (await browser.$('button[role="menuitem"]=Edit')).click();
		await (await menuItem(browser, 'Undo add bookmark')).waitForExist({ timeoutMsg: 'the moved document lost its undo history' });
		await browser.keys(['Escape']);

		// In the first window, its only tab can't go to a new window; the other window is
		// offered, by what it shows.
		await browser.switchToWindow(main);
		await (await tabOf(browser, 'window-a.pdf')).click({ button: 'right' });
		assert.equal(await (await menuItem(browser, 'Move to new window')).getAttribute('aria-disabled'), 'true', 'a window’s only tab can move to a new window');
		await (await menuItem(browser, 'Move to window: window-b.pdf')).waitForExist({ timeoutMsg: 'the other window is not in the tab’s menu' });
		await browser.keys(['Escape']);

		// Opened again from the first window, the file is not opened twice.
		await browser.keys(['Control', 'o']);
		await browser.pause(500);
		assert.deepEqual(await tabNames(browser), ['window-a.pdf']);
		assert.equal(await (await browser.$('[role=alert]')).isExisting(), false, 'opening a file open in another window showed an error');
		assert.equal(await windowCount(browser), 2);

		// Back into the first window: the window left with nothing closes.
		await browser.switchToWindow(other);
		await (await tabOf(browser, 'window-b.pdf')).click({ button: 'right' });
		await (await menuItem(browser, 'Move to window: window-a.pdf')).click();
		await browser.waitUntil(async () => (await windowCount(browser)) === 1, { timeout: 20_000, timeoutMsg: 'the emptied window stayed open' });
		await browser.switchToWindow(main);
		await browser.waitUntil(async () => (await tabNames(browser)).length === 2, { timeoutMsg: 'the tab did not come back' });
		assert.deepEqual(await tabNames(browser), ['window-a.pdf', 'window-b.pdf']);
		await waitForDocument(browser);
		assert.ok((await browser.$('[role=tab][aria-selected="true"]').getText()).includes('window-b.pdf'), 'the tab moved in is not shown');
		await browser.waitUntil(async () => (await pageBox(browser)) === '3', { timeoutMsg: 'the tab came back on another page' });
		assert.ok(await (await browser.$('.doc-tab*=window-b.pdf')).$('[data-unsaved]').isExisting(), 'the tab came back saved');

		// Out again from the File menu. Exit then asks in that window about its changes:
		// Cancel there keeps both windows.
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await menuItem(browser, 'Move tab to new window')).click();
		await browser.waitUntil(async () => (await windowCount(browser)) === 2, { timeout: 20_000, timeoutMsg: 'File > Move tab to new window opened no window' });
		await browser.switchToWindow(main);
		await browser.waitUntil(async () => (await tabNames(browser)).length === 1);
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await menuItem(browser, 'Exit')).click();
		assert.ok(await switchToWindowWith(browser, 'window-b.pdf'));
		const question = await browser.$('[role=alertdialog]');
		await question.waitForDisplayed({ timeoutMsg: 'the other window did not ask about its unsaved changes' });
		assert.ok((await question.getText()).includes('window-b.pdf'));
		await (await question.$('button=Cancel')).click();
		await browser.pause(600);
		assert.equal(await windowCount(browser), 2, 'a window closed though its question was cancelled');
		await browser.switchToWindow(main);
		assert.deepEqual(await tabNames(browser), ['window-a.pdf']);

		// Exit again, and "Don't save" there: every window closes and Lectrix quits.
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await menuItem(browser, 'Exit')).click();
		assert.ok(await switchToWindowWith(browser, 'window-b.pdf'));
		const again = await browser.$('[role=alertdialog]');
		await again.waitForDisplayed({ timeoutMsg: 'the other window did not ask again' });
		await (await again.$('button*=save')).waitForExist();
		await (await again.$('//button[contains(., "Don")]')).click();
		quit = true;
		assert.ok(await run.exitedByItself(), 'Lectrix did not quit after Exit');
	} finally {
		if (!quit) await run.stop();
	}
});
