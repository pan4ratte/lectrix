// Unlock (section 6.8, ADR 0025), from the start screen and File > Unlock…: a file's
// password and restrictions removed, in the file itself (Save) or a copy (Save as a copy),
// which then opens in a tab. A wrong password is asked again; a file with restrictions
// alone needs none; a Save dialog given up asks again; a file without protection is said to
// have nothing to remove.

import assert from 'node:assert/strict';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, organizePages, statusText, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const activeTab = (browser) =>
	browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.trim() ?? '');
const notices = (browser) =>
	browser.execute(() => [...document.querySelectorAll('[role="status"]')].map((n) => n.textContent.trim()).join('\n'));
const encrypted = (path) => readFileSync(path).includes('/Encrypt');

async function typePassword(browser, password) {
	const field = await browser.$('input[type="password"]');
	await field.waitForDisplayed({ timeoutMsg: 'no password was asked for' });
	await field.setValue(password);
	await (await browser.$('button=Unlock')).click();
}

/** Answers the question asked once the file is open. */
async function choose(browser, label) {
	const button = await (await browser.$('[role="alertdialog"]')).$(`button=${label}`);
	await button.waitForDisplayed({ timeoutMsg: 'Save or Save as a copy was not asked' });
	await button.click();
}

async function waitForNotice(browser, text, timeoutMsg) {
	await browser.waitUntil(async () => (await notices(browser)).includes(text), { timeoutMsg });
}

test('Unlock asks for the password, asks again after a wrong one, and saves a copy', async () => {
	const doc = sample('unlock-password.pdf', 3, ['--password', 'open sesame', '--owner-password', 'owner']);
	const out = join(OUT, 'unlock-password (unlocked).pdf');
	rmSync(out, { force: true });
	const original = readFileSync(doc);
	// The file dialog: given up, then the file; then the copy's Save dialog: given up (the
	// question comes back), then the copy.
	const { browser, stop } = await launch([], { dialogs: [null, [doc], null, [out]] });
	try {
		const tile = await browser.$('[data-tool="unlock"]');
		await tile.waitForDisplayed({ timeoutMsg: 'the start screen has no Unlock' });
		assert.equal((await tile.getText()).trim(), 'Unlock…');
		await browser.saveScreenshot(join(OUT, 'unlock-start.png'));
		await tile.click();
		await browser.pause(300);
		assert.equal(await (await browser.$('input[type="password"]')).isExisting(), false, 'a file given up asked for a password');

		await tile.click();
		await typePassword(browser, 'wrong');
		await browser.waitUntil(
			async () => (await browser.execute(() => document.body.innerText)).includes('That password didn’t open unlock-password.pdf'),
			{ timeoutMsg: 'a wrong password was not asked again' }
		);
		await typePassword(browser, 'open sesame');

		await (await browser.$('[role="alertdialog"]')).waitForDisplayed({ timeoutMsg: 'Save or Save as a copy was not asked' });
		await browser.pause(200);
		await browser.saveScreenshot(join(OUT, 'unlock-question.png'));
		await choose(browser, 'Save as a copy');
		// The Save dialog was given up: nothing written, and the question again.
		await choose(browser, 'Save as a copy');

		await waitForDocument(browser);
		assert.match(await activeTab(browser), /unlock-password \(unlocked\)\.pdf/);
		assert.match(await statusText(browser), /of 3/);
		await waitForNotice(browser, 'Saved unlock-password (unlocked).pdf without a password or restrictions.', 'nothing said the copy was saved');
		assert.ok(existsSync(out), 'no copy was written');
		assert.equal(encrypted(out), false, 'the copy is still encrypted');
		assert.deepEqual(readFileSync(doc), original, 'the original was changed');
	} finally {
		await stop();
	}
});

test('a file that forbids changes opens with them turned off, and what it allows on', async () => {
	// No open password; annotating allowed, nothing else (`/P` with its reserved bits set,
	// as in every real file: they once made Lectrix allow everything).
	const restricted = sample('unlock-forbidden.pdf', 2, ['--owner-password', 'owner']);
	const { browser, stop } = await launch([restricted]);
	try {
		await waitForDocument(browser);
		const highlight = await browser.$('button.tool-button[aria-label="Highlight"]');
		await highlight.waitForExist({ timeoutMsg: 'the toolbar has no Highlight tool' });
		assert.equal(await highlight.isEnabled(), true, 'annotating, which the file allows, is off');
		await (await browser.$('.viewer-scroll')).click();
		await browser.keys(['Control', 'b']);
		await browser.waitUntil(
			async () =>
				(await browser.execute(() => document.body.innerText)).includes('security settings don’t allow changing bookmarks'),
			{ timeoutMsg: 'adding a bookmark was not refused' }
		);
		assert.equal(await browser.execute(() => document.querySelectorAll('[role=treeitem]').length), 0);

		const tabs = () => browser.execute(() => document.querySelectorAll('[role="tab"]').length);
		const before = await tabs();
		// The refusal's Unlock… unlocks this document: Save, and the tab reads it again.
		await (await (await browser.$('[role="alert"]')).$('button=Unlock…')).click();
		await choose(browser, 'Save');
		await waitForNotice(browser, 'unlock-forbidden.pdf no longer has a password or restrictions.', 'nothing said the file was saved');
		assert.equal(encrypted(restricted), false, 'the file is still encrypted');
		assert.equal(await tabs(), before, 'the file opened in a second tab');
		// The watcher looks every 1.5 s.
		await browser.pause(2000);
		assert.equal(
			await browser.execute(() => document.body.innerText.includes('Another program changed this file')),
			false,
			'its own save was reported as a change by another program'
		);
		await (await browser.$('.viewer-scroll')).click();
		await browser.keys(['Control', 'b']);
		await browser.waitUntil(async () => (await browser.execute(() => document.querySelectorAll('[role=treeitem]').length)) === 1, {
			timeoutMsg: 'a bookmark can’t be added after unlocking'
		});
	} finally {
		await stop();
	}
});

test('a protected document shows a lock in its tab; a locked tool says why; File > Unlock… unlocks the document shown', async () => {
	const restricted = sample('unlock-tools.pdf', 2, ['--owner-password', 'owner', '--allow', 'print']);
	const { browser, stop } = await launch([restricted]);
	const refusal = 'security settings don’t allow adding or changing annotations';
	const refusals = () =>
		browser.execute(
			(text) => [...document.querySelectorAll('[role="alert"]')].filter((n) => n.textContent.includes(text)).length,
			refusal
		);
	try {
		await waitForDocument(browser);
		const lock = await browser.$('[role="tab"] [data-protected]');
		await lock.waitForExist({ timeoutMsg: 'the tab shows no lock' });
		await browser.saveScreenshot(join(OUT, 'unlock-tab.png'));

		// The pointer on a locked tool: it looks unavailable, and says why.
		const highlight = await browser.$('button.tool-button[aria-label="Highlight"]');
		assert.equal(await highlight.getAttribute('aria-disabled'), 'true');
		await highlight.click();
		await browser.waitUntil(async () => (await refusals()) > 0, { timeoutMsg: 'clicking a locked tool said nothing' });
		assert.equal(await highlight.getAttribute('aria-pressed'), 'false', 'the locked tool was picked');
		await browser.execute(() => document.querySelectorAll('[role="alert"] button[aria-label^="Dismiss"], [role="alert"] [data-dismiss]').forEach((b) => b.click()));

		// File > Unlock… with the protected document shown: no Open dialog, the question.
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await browser.$('[role="menuitem"]*=Unlock')).click();
		await choose(browser, 'Save');
		await waitForNotice(browser, 'unlock-tools.pdf no longer has a password or restrictions.', 'nothing said the file was saved');
		await browser.waitUntil(async () => !(await lock.isExisting()), { timeoutMsg: 'the tab still shows a lock' });
		await browser.waitUntil(async () => (await highlight.getAttribute('aria-disabled')) !== 'true', {
			timeoutMsg: 'the tools are still locked after unlocking'
		});
		assert.equal(encrypted(restricted), false, 'the file is still encrypted');
	} finally {
		await stop();
	}
});

test('a locked tool’s shortcut says why too', async () => {
	const restricted = sample('unlock-shortcut.pdf', 2, ['--owner-password', 'owner', '--allow', 'print']);
	const { browser, stop } = await launch([restricted]);
	try {
		await waitForDocument(browser);
		await (await browser.$('.viewer-scroll')).click();
		await browser.keys('h');
		await browser.waitUntil(
			async () =>
				(await browser.execute(() => document.body.innerText)).includes('security settings don’t allow adding or changing annotations'),
			{ timeoutMsg: 'the Highlight shortcut said nothing' }
		);
		assert.equal(await (await browser.$('button.tool-button[aria-label="Highlight"]')).getAttribute('aria-pressed'), 'false');
	} finally {
		await stop();
	}
});

test('the grid of pages of a file that forbids changing them offers Unlock… in its bar', async () => {
	const restricted = sample('unlock-organize.pdf', 3, ['--owner-password', 'owner']);
	const { browser, stop } = await launch([restricted]);
	try {
		await waitForDocument(browser);
		await organizePages(browser);
		const bar = await browser.$('[data-organize-bar]');
		const note = 'Its security settings don’t allow changes.';
		await browser.waitUntil(async () => (await bar.getText()).includes(note), { timeoutMsg: 'the bar does not say why' });
		await browser.saveScreenshot(join(OUT, 'unlock-organize-bar.png'));
		await (await bar.$('button=Unlock…')).click();
		await choose(browser, 'Save');
		await browser.waitUntil(async () => !(await bar.getText()).includes(note), {
			timeoutMsg: 'the pages still can’t be changed after unlocking'
		});
		assert.equal(await (await bar.$('button=Unlock…')).isExisting(), false, 'Unlock… is still offered');
		assert.equal(encrypted(restricted), false, 'the file is still encrypted');
	} finally {
		await stop();
	}
});

test('File > Unlock… saves the file itself, with no password for restrictions alone; Cancel and a file without protection change nothing', async () => {
	const restricted = sample('unlock-restricted.pdf', 2, ['--owner-password', 'owner']);
	const plain = sample('unlock-plain.pdf', 2);
	const before = readFileSync(restricted);
	const { browser, stop } = await launch([], { dialogs: [[restricted], [restricted], [plain]] });
	const unlockFromMenu = async () => {
		await (await browser.$('button[role="menuitem"]=File')).click();
		await (await browser.$('[role="menuitem"]*=Unlock')).click();
	};
	try {
		await (await browser.$('[data-tool="unlock"]')).waitForDisplayed({ timeoutMsg: 'the start screen did not show' });

		// Cancel: nothing written.
		await unlockFromMenu();
		await choose(browser, 'Cancel');
		await browser.pause(300);
		assert.deepEqual(readFileSync(restricted), before, 'Cancel changed the file');

		// Save: the file itself, which opens in a tab.
		await unlockFromMenu();
		await choose(browser, 'Save');
		await waitForDocument(browser);
		assert.match(await activeTab(browser), /^unlock-restricted\.pdf/);
		assert.equal(encrypted(restricted), false, 'the file is still encrypted');
		await waitForNotice(browser, 'unlock-restricted.pdf no longer has a password or restrictions.', 'nothing said the file was saved');

		// From the menu with a document open, a file without protection.
		await unlockFromMenu();
		await waitForNotice(browser, 'unlock-plain.pdf has no password or restrictions.', 'nothing said the file has nothing to remove');
	} finally {
		await stop();
	}
});
