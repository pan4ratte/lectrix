// Full screen (AGENTS.md section 6.1): F11 fills the screen with the pages alone. The title
// bar, the view bar and the side panes go; the toolbar floats, with the page and zoom
// controls in it; the pointer at a side of the screen brings that side's pane over the
// pages, and Alt held brings both, with a toolbar that shows only on demand.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { Key } from 'webdriverio';

import { OUT, ROOT, launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const FLOATING = '[role=toolbar][aria-label="Page, zoom and annotation tools"]';
const TOOLS = '[role=toolbar][aria-label="Annotation tools"]';

/** What of the window's own parts is on screen, and how large the page area is. */
function look(browser) {
	return browser.execute((floating) => {
		const has = (s) => document.querySelector(s) !== null;
		const scroller = document.querySelector('.viewer-scroll');
		return {
			titleBar: has('header'),
			viewBar: has('[data-view-bar]'),
			left: has('aside[aria-label="Left pane"]'),
			right: has('aside[aria-label="Right pane"]'),
			floating: has(floating),
			fillsScreen: innerWidth >= screen.width - 1 && innerHeight >= screen.height - 1,
			pages: scroller ? [scroller.offsetWidth, scroller.offsetHeight] : null,
			size: [innerWidth, innerHeight]
		};
	}, FLOATING);
}

async function until(browser, what, check) {
	let seen;
	await browser
		.waitUntil(async () => check((seen = await look(browser))), { timeout: 8000 })
		.catch(() => {
			throw new Error(`${what}: ${JSON.stringify(seen)}`);
		});
	// Whatever slid has come to rest.
	await browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')), {
		timeoutMsg: `${what}: something kept moving`
	});
	return look(browser);
}

const full = (s) => s.fillsScreen && !s.titleBar && !s.viewBar && s.floating;
const windowed = (s) => s.titleBar && s.viewBar && !s.floating;

async function pointerTo(browser, x, y) {
	await browser.action('pointer').move({ x: Math.round(x), y: Math.round(y), origin: 'viewport' }).perform();
}

async function closeSettings(browser) {
	await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
	await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
		timeoutMsg: 'Settings did not close'
	});
}

test('full screen shows the pages alone; the sides of the screen and Alt bring the rest', async () => {
	// A window smaller than the screen, CI's 1024 x 768 one too: leaving full screen is told
	// by the window no longer filling it. (With the version that last started, so nothing
	// says Lectrix was updated.)
	const state = join(OUT, 'fullscreen-window.json');
	mkdirSync(OUT, { recursive: true });
	rmSync(state, { force: true });
	const { version } = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
	writeFileSync(state, JSON.stringify({ last_version: version, window: { bounds: { x: 60, y: 40, width: 900, height: 700 }, maximized: false } }));
	const { browser, stop } = await launch([sample('fullscreen.pdf', 6)], { env: { LECTRIX_STATE_FILE: state } });
	try {
		await waitForDocument(browser);
		const before = await until(browser, 'the window as it starts', (s) => windowed(s) && s.left && !s.fillsScreen);
		assert.equal(before.right, false, 'the right pane starts closed');

		// F11: the window's own parts go, and the pages have the whole screen.
		await browser.keys(['F11']);
		const inFull = await until(browser, 'F11 did not show the pages alone', (s) => full(s) && !s.left && !s.right);
		assert.deepEqual(inFull.pages, inFull.size, 'the pages fill the screen');
		await browser.waitUntil(
			() => browser.execute(() => [...document.querySelectorAll('[role=status]')].some((n) => n.textContent.includes('to leave full screen'))),
			{ timeoutMsg: 'nothing said how to leave' }
		);

		// The toolbar floats over the pages with the page and zoom controls in it, and a
		// button that leaves.
		const bar = await browser.execute((sel) => {
			const el = document.querySelector(sel);
			return {
				overPages: el.parentElement.querySelector('.viewer-scroll') !== null,
				pageBox: el.querySelector('[role=group][aria-label="Page and zoom"] [data-page-box]') !== null,
				tools: el.querySelector('button[aria-label="Highlight"]') !== null,
				leave: el.querySelector('button[aria-label="Leave full screen"]') !== null
			};
		}, FLOATING);
		assert.deepEqual(bar, { overPages: true, pageBox: true, tools: true, leave: true });
		const pageBox = await browser.$(`${FLOATING} [data-page-box]`);
		await pageBox.click();
		await browser.keys(['3', 'Enter']);
		await browser.waitUntil(
			() => browser.execute(() => document.querySelector('[data-page-position]')?.textContent?.includes('3 of 6')),
			{ timeoutMsg: 'the page box in the toolbar did not go to page 3' }
		);

		// The pointer at the left side of the screen brings the left pane over the pages,
		// which keep their size; away from it, the pane goes.
		const mid = inFull.size[1] / 2;
		await pointerTo(browser, inFull.size[0] / 2, mid);
		await pointerTo(browser, 0, mid);
		const withLeft = await until(browser, 'the left side did not bring the left pane', (s) => s.left);
		assert.equal(withLeft.right, false);
		assert.deepEqual(withLeft.pages, inFull.pages, 'the pane lies over the pages');
		// Over the pane, it stays.
		await pointerTo(browser, 120, mid);
		await browser.pause(600);
		assert.equal((await look(browser)).left, true, 'the pane went from under the pointer');
		await pointerTo(browser, inFull.size[0] / 2, mid);
		await until(browser, 'the left pane stayed', (s) => !s.left);

		// The same at the right side.
		await pointerTo(browser, inFull.size[0] - 1, mid);
		await until(browser, 'the right side did not bring the right pane', (s) => s.right && !s.left);
		await pointerTo(browser, inFull.size[0] / 2, mid);
		await until(browser, 'the right pane stayed', (s) => !s.right);

		// A toolbar that shows only when the pointer is near (Settings) hides here too.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('label*=Show only when the pointer is near')).click();
		await closeSettings(browser);
		await pointerTo(browser, inFull.size[0] / 2, mid);
		const floating = await browser.$(FLOATING);
		await browser.waitUntil(async () => (await floating.getCSSProperty('opacity')).value === 0, {
			timeoutMsg: 'the toolbar did not hide away from its edge'
		});

		// Alt held: both panes and the toolbar. Let go: gone again.
		await browser.action('key').down(Key.Alt).perform(true);
		await until(browser, 'Alt held did not bring both panes', (s) => s.left && s.right);
		await browser.waitUntil(async () => (await floating.getCSSProperty('opacity')).value === 1, {
			timeoutMsg: 'Alt held did not show the toolbar'
		});
		await browser.action('key').up(Key.Alt).perform();
		await until(browser, 'the panes stayed after Alt', (s) => !s.left && !s.right);
		await browser.waitUntil(async () => (await floating.getCSSProperty('opacity')).value === 0, {
			timeoutMsg: 'the toolbar stayed after Alt'
		});
		// Alt as part of a shortcut brings nothing.
		await browser.keys(['Alt', 'ArrowLeft']);
		await browser.pause(400);
		assert.deepEqual([(await look(browser)).left, (await look(browser)).right], [false, false]);

		// A panel asked for from the keyboard (Ctrl+B adds a bookmark, and names it in the
		// Bookmarks panel) shows until the page is pressed.
		await browser.keys(['Control', 'b']);
		await until(browser, 'Ctrl+B did not show the Bookmarks panel', (s) => s.left);
		await browser.keys(['Escape']);
		await browser.pause(600);
		assert.equal((await look(browser)).left, true, 'the panel a command asked for went by itself');
		await browser.action('pointer').move({ x: Math.round(inFull.size[0] / 2), y: Math.round(mid), origin: 'viewport' }).down().up().perform();
		await until(browser, 'pressing the page did not let the pane go', (s) => !s.left);

		// Esc steps back from what the press on the page left (a selection), then, with
		// nothing else to step back from, leaves: the window is as it was.
		for (let i = 0; i < 3 && !windowed(await look(browser)); i++) {
			await browser.keys(['Escape']);
			await browser.pause(300);
		}
		const back = await until(browser, 'Esc did not leave full screen', (s) => windowed(s) && s.left && !s.fillsScreen);
		assert.deepEqual(back.size, before.size, 'the window has its size back');

		// Docked in the view bar (as it starts), the toolbar floated all the same in full
		// screen, where Settings' edge for the floating toolbar is used. At the left, its
		// controls are stacked, the page and zoom controls first; their menus open beside it,
		// over the pages; and its own button leaves.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('label*=Show only when the pointer is near')).click();
		assert.equal(await (await browser.$('#setting-toolbarPosition')).getAttribute('data-disabled'), null, 'the floating options stay, for full screen');
		await (await browser.$('#setting-toolbarPosition')).click();
		await (await browser.$('[role="option"]*=Left')).click();
		await closeSettings(browser);
		await (await browser.$(`[data-view-bar] ${TOOLS}`)).waitForExist({ timeoutMsg: 'the toolbar is not docked in the view bar' });
		await browser.keys(['F11']);
		await until(browser, 'F11 did not float the docked toolbar', full);
		const stacked = await browser.execute((sel) => {
			const el = document.querySelector(sel);
			const r = el.getBoundingClientRect();
			const box = el.querySelector('[data-page-box]').getBoundingClientRect();
			const tool = el.querySelector('button[aria-label="Highlight"]').getBoundingClientRect();
			const leave = el.querySelector('button[aria-label="Leave full screen"]').getBoundingClientRect();
			return {
				orientation: el.getAttribute('aria-orientation'),
				atLeft: r.left < 40,
				tall: r.height > r.width * 3,
				inScreen: r.top >= 0 && r.bottom <= innerHeight,
				centred: Math.abs(r.top + r.height / 2 - innerHeight / 2) < 2,
				inOrder: box.bottom <= tool.top && tool.bottom <= leave.top,
				right: r.right
			};
		}, FLOATING);
		assert.deepEqual(
			{ ...stacked, right: 0 },
			{ orientation: 'vertical', atLeft: true, tall: true, inScreen: true, centred: true, inOrder: true, right: 0 },
			`the toolbar at the left of the screen: ${JSON.stringify(stacked)}`
		);
		await (await browser.$(`${FLOATING} button[aria-label="Zoom options"]`)).click();
		const zoomMenu = await browser.$('[role=menu]');
		await zoomMenu.waitForDisplayed({ timeoutMsg: 'the zoom level opened no menu' });
		await browser.waitUntil(() => browser.execute(() => document.getAnimations().every((a) => a.playState !== 'running')));
		const menuLeft = await browser.execute(() => document.querySelector('[role=menu]').getBoundingClientRect().left);
		assert.ok(menuLeft >= stacked.right, `the zoom menu opened over the toolbar: ${menuLeft} < ${stacked.right}`);
		await browser.keys(['Escape']);
		await zoomMenu.waitForExist({ reverse: true });
		await (await browser.$(`${FLOATING} button[aria-label="Leave full screen"]`)).click();
		await until(browser, 'the button did not leave full screen', (s) => windowed(s) && !s.fillsScreen);
		await (await browser.$(`[data-view-bar] ${TOOLS}`)).waitForExist({ timeoutMsg: 'the toolbar did not dock again' });

		// Docked at the right of the pages (Settings), it has a bar of its own there, which
		// goes in full screen and comes back after it.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('#setting-toolbarDock')).click();
		await (await browser.$('[role="option"]*=Right')).click();
		await closeSettings(browser);
		const dockedRight = () =>
			browser.execute((sel) => {
				const bar = document.querySelector(sel);
				const pages = document.querySelector('.viewer-scroll')?.getBoundingClientRect();
				return bar !== null && pages !== undefined && bar.closest('[data-view-bar]') === null && bar.getBoundingClientRect().left >= pages.right - 1;
			}, TOOLS);
		await browser.waitUntil(dockedRight, { timeoutMsg: 'the toolbar did not dock at the right of the pages' });
		await browser.keys(['F11']);
		const again = await until(browser, 'F11 did not float the toolbar docked at the right', full);
		assert.deepEqual(again.pages, again.size, 'the pages fill the screen, with no bar beside them');
		await browser.keys(['F11']);
		await until(browser, 'F11 did not leave full screen', (s) => windowed(s) && !s.fillsScreen);
		await browser.waitUntil(dockedRight, { timeoutMsg: 'the toolbar did not dock at the right again' });
	} finally {
		await stop();
	}
});

test('a window closed in full screen comes back as the window it was', async () => {
	const state = join(OUT, 'fullscreen-state.json');
	// As in window-placement.test.mjs: within a 1024 x 768 screen at 100% to 150%.
	const normal = { x: 100, y: 60, width: 990, height: 750 };
	rmSync(state, { force: true });
	writeFileSync(state, JSON.stringify({ window: { bounds: normal, maximized: false } }));
	const { browser, stop, exitedByItself } = await launch([sample('fullscreen-close.pdf', 2)], { env: { LECTRIX_STATE_FILE: state } });
	try {
		await waitForDocument(browser);
		await browser.keys(['F11']);
		await until(browser, 'F11 did not fill the screen', full);
		// The window asked to close, as Alt+F4 asks it.
		await browser.execute(() => void window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'main' }));
		assert.ok(await exitedByItself(), 'Lectrix did not quit');
	} catch (e) {
		await stop();
		throw e;
	}
	const saved = JSON.parse(readFileSync(state, 'utf8')).window;
	assert.deepEqual(saved, { bounds: normal, maximized: false }, 'full screen was remembered as the window’s place');
});
