// Settings (AGENTS.md section 6.6): the theme as a three-way toggle, sliders with their own
// look, restoring the defaults after asking, and the keyboard shortcuts: listed, searched,
// changed, and in effect at once.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

async function openSettings(browser, group) {
	await browser.keys(['Control', ',']);
	await (await browser.$(`button[role="tab"]*=${group}`)).click();
	// The settings arrive from Rust a moment after the dialog opens.
	await (await browser.$('[role="tabpanel"]:not([hidden]) .settings-section')).waitForExist({ timeoutMsg: `${group} shows no settings` });
}

async function closeSettings(browser) {
	await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
	await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
		timeoutMsg: 'Settings did not close'
	});
}

const theme = (browser) => browser.execute(() => document.documentElement.dataset.theme ?? '');

/** The theme toggle's segments, and which one is chosen. */
const themeChoice = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('#setting-appearance [role="radio"]')].map((r) => `${r.textContent.trim()}${r.getAttribute('aria-checked') === 'true' ? '*' : ''}`)
	);

/** Answers the question on screen with the button named `label`. */
async function answer(browser, label) {
	const dialog = await browser.$('[role="alertdialog"]');
	await dialog.waitForDisplayed({ timeoutMsg: 'no question was asked' });
	await (await dialog.$(`button=${label}`)).click();
	await dialog.waitForExist({ reverse: true, timeoutMsg: 'the question stayed' });
}

test('the theme toggle, the sliders, and restoring the defaults', async () => {
	const { browser, stop } = await launch([]);
	try {
		// The theme: three segments in one group, the one in use chosen.
		await openSettings(browser, 'Appearance');
		assert.deepEqual(await themeChoice(browser), ['System*', 'Light', 'Dark']);
		await (await (await browser.$('#setting-appearance')).$('[role="radio"]*=Dark')).click();
		await browser.waitUntil(async () => (await theme(browser)) === 'dark', { timeoutMsg: 'Dark did not apply' });
		// Arrow keys move to the next one and choose it, as radio buttons do.
		await browser.keys('ArrowLeft');
		await browser.waitUntil(async () => (await theme(browser)) === 'light', { timeoutMsg: 'Left did not choose Light' });
		assert.deepEqual(await themeChoice(browser), ['System', 'Light*', 'Dark']);
		// One raised mark glides to the segment chosen, and one fill to the group shown.
		const markOn = (mark, chosen) =>
			browser.execute(
				(m, c) => {
					const a = document.querySelector(m)?.getBoundingClientRect();
					const b = document.querySelector(c)?.getBoundingClientRect();
					return !!a && !!b && Math.abs(a.left - b.left) < 1 && Math.abs(a.top - b.top) < 1 && Math.abs(a.width - b.width) < 1;
				},
				mark,
				chosen
			);
		await browser.waitUntil(() => markOn('#setting-appearance .tool-mark', '#setting-appearance [aria-checked="true"]'), {
			timeoutMsg: 'the theme toggle has no mark on the segment chosen'
		});
		await browser.waitUntil(() => markOn('.settings-nav .tool-mark', '.settings-nav [role="tab"][aria-selected="true"]'), {
			timeoutMsg: 'the settings groups have no mark on the one shown'
		});
		// Restore defaults is in the colour of things that can't be undone.
		await (await browser.$('button[role="tab"]*=General')).click();
		const danger = await browser.execute(() => {
			const button = [...document.querySelectorAll('[role="tabpanel"]:not([hidden]) button')].find((b) => b.textContent.includes('Restore defaults'));
			return { on: button.classList.contains('button-danger'), colour: getComputedStyle(button).backgroundColor };
		});
		assert.ok(danger.on && danger.colour !== 'rgba(0, 0, 0, 0)', JSON.stringify(danger));
		await (await browser.$('button[role="tab"]*=Appearance')).click();

		// A slider: no native look, filled as far as its handle, the fill and the handle in
		// colours of their own.
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		const slider = () =>
			browser.execute(() => {
				const el = document.querySelector('#setting-tooltipDelayMs');
				const style = getComputedStyle(el);
				return {
					appearance: style.appearance,
					ratio: Number(style.getPropertyValue('--ratio')),
					fill: style.getPropertyValue('--lectrix-slider-fill').trim(),
					handle: style.getPropertyValue('--lectrix-slider-handle').trim(),
					text: el.getAttribute('aria-valuetext')
				};
			});
		const before = await slider();
		assert.equal(before.appearance, 'none');
		assert.equal(before.text, '0.3 s');
		assert.ok(Math.abs(before.ratio - 0.15) < 1e-6, `filled to ${before.ratio}`);
		assert.notEqual(before.fill, before.handle, 'the fill and the handle share a colour');
		// The fill is the accent of a switch that is on, the handle its thumb's colour with a
		// thin edge and a shadow.
		const look = await browser.execute(() => {
			const page = document.querySelector('[role="tabpanel"]:not([hidden])');
			const slider = page.querySelector('#setting-tooltipDelayMs').parentElement;
			const handle = getComputedStyle(slider.querySelector('.slider-handle'));
			const on = page.querySelector('[role="switch"][data-state="checked"]');
			return {
				fill: getComputedStyle(slider.querySelector('.slider-fill')).backgroundColor,
				switchOn: getComputedStyle(on).backgroundColor,
				handle: handle.backgroundColor,
				thumb: getComputedStyle(on.querySelector('.switch-thumb')).backgroundColor,
				edge: handle.borderTopWidth,
				shadow: handle.boxShadow,
				handleSize: slider.querySelector('.slider-handle').getBoundingClientRect().width,
				thumbSize: on.querySelector('.switch-thumb').getBoundingClientRect().width
			};
		});
		assert.equal(look.fill, look.switchOn);
		assert.equal(look.handle, look.thumb);
		// The two are one size, too.
		assert.ok(Math.abs(look.handleSize - look.thumbSize) < 0.5, `the handle is ${look.handleSize} wide, a switch's thumb ${look.thumbSize}`);
		// (One CSS pixel, which a scaled display snaps to its own pixels.)
		assert.ok(parseFloat(look.edge) > 0 && parseFloat(look.edge) <= 1, `the handle's edge is ${look.edge}`);
		assert.notEqual(look.shadow, 'none');
		await (await browser.$('#setting-tooltipDelayMs')).click();
		await browser.keys('End');
		await browser.waitUntil(async () => (await slider()).ratio === 1, { timeoutMsg: 'the fill did not follow the handle' });
		assert.equal((await slider()).text, '2.0 s');

		// Restore defaults asks first; Cancel changes nothing.
		await (await browser.$('button[role="tab"]*=General')).click();
		await (await browser.$('#setting-author')).setValue('E2E Tester');
		await (await browser.$('button=Restore defaults…')).click();
		await answer(browser, 'Cancel');
		assert.equal(await (await browser.$('#setting-author')).getValue(), 'E2E Tester');
		assert.equal(await theme(browser), 'light');

		// Confirmed, every setting is as Lectrix came, at once and while Settings stays open.
		await (await browser.$('button=Restore defaults…')).click();
		await answer(browser, 'Restore defaults');
		await browser.waitUntil(async () => (await (await browser.$('#setting-author')).getValue()) === '', {
			timeoutMsg: 'the author was not reset'
		});
		await (await browser.$('button[role="tab"]*=Appearance')).click();
		assert.deepEqual(await themeChoice(browser), ['System*', 'Light', 'Dark']);
		assert.notEqual(await theme(browser), 'light', 'the theme stayed forced to light');
		await (await browser.$('button[role="tab"]*=Annotations')).click();
		assert.equal((await slider()).text, '0.3 s');
		await closeSettings(browser);
	} finally {
		await stop();
	}
});

test('shortcuts are listed, searched and changed, and work at once', async () => {
	const path = sample('settings-shortcuts.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await openSettings(browser, 'Shortcuts');

		/** The commands listed, by name. */
		const listed = () =>
			browser.execute(() => [...document.querySelectorAll('.shortcut-row[data-command]')].map((r) => r.dataset.command));
		/** The shortcuts a command shows. */
		const keysOf = (command) =>
			browser.execute(
				(c) => [...document.querySelectorAll(`.shortcut-row[data-command="${c}"] [data-slot]`)].map((b) => b.textContent.trim()),
				command
			);
		const slot = (command, i) => browser.$(`.shortcut-row[data-command="${command}"] [data-slot="${i}"]`);
		const search = await browser.$('#setting-shortcuts');

		// Every command is there, in groups, with the keys that can't be changed after them.
		const all = await listed();
		assert.ok(all.length > 40 && all.includes('save') && all.includes('toolHighlight') && all.includes('fitPage'), all.join(' '));
		const groups = await browser.execute(() => [...document.querySelectorAll('[role="dialog"] h3')].filter((h) => h.offsetParent !== null).map((h) => h.textContent.trim()));
		assert.deepEqual(groups, ['File', 'Edit', 'View', 'Go to', 'Document', 'Annotation tools', 'Help', 'Fixed keys']);
		assert.deepEqual(await keysOf('redo'), ['Ctrl+Y', 'Ctrl+Shift+Z']);
		// A command without one shows only the plus that adds one; every row ends with it.
		assert.deepEqual(await keysOf('fitPage'), []);
		const lastButtons = await browser.execute(() =>
			['fitPage', 'redo'].map((c) => [...document.querySelectorAll(`.shortcut-row[data-command="${c}"] button`)].at(-1)?.getAttribute('aria-label'))
		);
		assert.deepEqual(lastButtons, ['Add a shortcut for Fit page', 'Add a shortcut for Redo']);
		// The shortcuts a command came with have no button to remove them.
		assert.equal((await browser.$$('.shortcut-remove')).length, 0);

		// The search filters by name, by group and by key.
		await search.setValue('zoom');
		assert.deepEqual(await listed(), ['zoomIn', 'zoomOut']);
		await search.setValue('ctrl+shift+s');
		assert.deepEqual(await listed(), ['saveAs']);
		// (The eight tools, and the nine preset colours the number keys pick.)
		await search.setValue('annotation tools');
		assert.equal((await listed()).length, 17);
		await search.setValue('preset colour');
		assert.equal((await listed()).length, 9);
		await search.setValue('no such thing');
		assert.deepEqual(await listed(), []);
		assert.ok(await (await browser.$('p*=No shortcut matches')).isDisplayed());
		await search.setValue('find');
		assert.deepEqual(await listed(), ['find', 'findNext', 'findPrevious']);

		// Change one: press it, then the new keys. A shortcut another command has is refused,
		// and says whose it is.
		await (await slot('find', 0)).click();
		assert.equal(await (await slot('find', 0)).getText(), 'Press keys…');
		await browser.keys(['Control', 'o']);
		const problem = await browser.$('.shortcut-problem');
		await problem.waitForDisplayed({ timeoutMsg: 'a shortcut in use was not refused' });
		assert.match(await problem.getText(), /Ctrl\+O is already the shortcut for .Open./);
		// A key with a fixed use is refused too.
		await browser.keys('F2');
		await browser.waitUntil(async () => /fixed use/.test(await problem.getText()), { timeoutMsg: 'F2 was not refused' });
		await browser.keys('F6');
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'F6', { timeoutMsg: 'F6 was not taken' });

		// Esc while pressing leaves the shortcut and Settings as they are.
		await (await slot('find', 0)).click();
		await browser.keys('Escape');
		assert.deepEqual(await keysOf('find'), ['F6']);
		assert.ok(await (await browser.$('[role="dialog"]')).isExisting(), 'Esc closed Settings while a shortcut was being pressed');

		// A second shortcut, then the first one taken away with Backspace.
		await (await browser.$('button[aria-label="Add a shortcut for Find"]')).click();
		await browser.keys(['Control', 'Alt', 'f']);
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'F6,Ctrl+Alt+F', { timeoutMsg: 'no second shortcut' });
		await (await slot('find', 0)).click();
		await browser.keys('Backspace');
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'Ctrl+Alt+F', { timeoutMsg: 'Backspace removed nothing' });

		// A shortcut that was added has an x on its chip, which removes that one.
		await (await browser.$('button[aria-label="Remove Ctrl+Alt+F from Find"]')).click();
		await browser.waitUntil(async () => (await keysOf('find')).length === 0, { timeoutMsg: 'the x removed nothing' });
		// The plus gives it one again; the one it came with is as it was, with no x.
		await (await browser.$('button[aria-label="Add a shortcut for Find"]')).click();
		await browser.keys(['Control', 'f']);
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'Ctrl+F', { timeoutMsg: 'the plus added nothing' });
		assert.equal((await browser.$$('.shortcut-remove')).length, 0, 'a shortcut the command came with can be removed with an x');
		assert.equal(await (await browser.$('button=Reset all…')).isEnabled(), false, 'the defaults count as changed');

		// In effect at once: with F6 for Find, Ctrl+F does nothing and F6 opens the search bar;
		// the menu shows the new key.
		await (await slot('find', 0)).click();
		await browser.keys('F6');
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'F6', { timeoutMsg: 'F6 was not taken again' });
		await closeSettings(browser);
		await (await browser.$('[role=document]')).click();
		await browser.keys(['Control', 'f']);
		await browser.pause(300);
		assert.equal(await (await browser.$('[data-search-input]')).isExisting(), false, 'Ctrl+F still opens the search');
		await browser.keys('F6');
		await (await browser.$('[data-search-input]')).waitForDisplayed({ timeoutMsg: 'F6 did not open the search' });
		await (await browser.$('button[role="menuitem"]=Edit')).click();
		const find = await browser.$('[role="menuitem"]*=Find');
		await find.waitForDisplayed();
		assert.match(await find.getText(), /F6/);
		await browser.keys('Escape');

		// Reset all asks first, then every command has its own keys again.
		await openSettings(browser, 'Shortcuts');
		const resetAll = await browser.$('button=Reset all…');
		await resetAll.click();
		await answer(browser, 'Reset shortcuts');
		await browser.waitUntil(async () => (await keysOf('find')).join() === 'Ctrl+F', { timeoutMsg: 'the shortcuts were not reset' });
		assert.equal(await resetAll.isEnabled(), false, 'Reset all stays on with nothing changed');
		await closeSettings(browser);
	} finally {
		await stop();
	}
});
