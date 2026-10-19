// Protect (section 6.9, ADR 0026) and a document's properties (section 6.10). From the
// start screen, Protect gives a file a password and restrictions and saves a protected
// copy, which opens in a tab without asking for the password; the dialog says why its main
// button is off. For a document in a tab, File > Protect… sets the protection its next save
// writes; its properties show it, from the tab's lock, and remove it. The Description tab
// changes the title, and the Fonts tab lists the fonts.

import assert from 'node:assert/strict';
import { readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, statusText, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

const activeTab = (browser) =>
	browser.execute(() => document.querySelector('[role="tab"][aria-selected="true"]')?.textContent.trim() ?? '');
const notices = (browser) =>
	browser.execute(() => [...document.querySelectorAll('[role="status"]')].map((n) => n.textContent.trim()).join('\n'));
const encrypted = (path) => readFileSync(path).includes('/Encrypt');

/** Whether `pdf-cli` opens `path` with `password` (none: without one). */
function opensWith(path, password) {
	const out = join(OUT, 'protect-check.pdf');
	try {
		cli('unlock', path, out, ...(password === null ? [] : ['--password', password]));
		return true;
	} catch {
		return false;
	} finally {
		rmSync(out, { force: true });
	}
}

async function waitForNotice(browser, text, timeoutMsg) {
	await browser.waitUntil(async () => (await notices(browser)).includes(text), { timeoutMsg });
}

async function protectDialog(browser) {
	const dialog = await browser.$('[data-protect-dialog]');
	await dialog.waitForDisplayed({ timeoutMsg: 'the Protect dialog did not open' });
	return dialog;
}

async function type(dialog, field, value) {
	await (await dialog.$(`[data-password="${field}"]`)).setValue(value);
}

async function choose(browser, label) {
	const button = await (await browser.$('[role="alertdialog"]')).$(`button=${label}`);
	await button.waitForDisplayed({ timeoutMsg: 'Save or Save as a copy was not asked' });
	await button.click();
}

async function fromMenu(browser, menu, item) {
	await (await browser.$(`button[role="menuitem"]=${menu}`)).click();
	await (await browser.$(`[role="menuitem"]*=${item}`)).click();
}

test('Protect, from the start screen, saves a protected copy that opens with its password', async () => {
	const doc = sample('protect-tool.pdf', 3);
	const out = join(OUT, 'protect-tool (protected).pdf');
	rmSync(out, { force: true });
	const original = readFileSync(doc);
	const { browser, stop } = await launch([], { dialogs: [[doc], [out]] });
	try {
		const tile = await browser.$('[data-tool="protect"]');
		await tile.waitForDisplayed({ timeoutMsg: 'the start screen has no Protect' });
		assert.equal((await tile.getText()).trim(), 'Protect…');
		await tile.click();

		const dialog = await protectDialog(browser);
		const protect = await dialog.$('button=Protect');
		assert.equal(await protect.isEnabled(), false, 'Protect is on with nothing ticked');
		await (await dialog.$('[data-require-open]')).click();
		await type(dialog, 'open', 'user');
		await type(dialog, 'open-confirm', 'usr');
		await browser.waitUntil(async () => (await dialog.getText()).includes('The passwords don’t match.'), {
			timeoutMsg: 'passwords that differ were not pointed out'
		});
		assert.equal(await protect.isEnabled(), false, 'Protect is on with passwords that differ');
		await type(dialog, 'open-confirm', 'user');
		await (await dialog.$('[data-restrict]')).click();
		await type(dialog, 'permissions', 'user');
		await browser.waitUntil(
			async () => (await dialog.getText()).includes('Use a different password from the one to open the file.'),
			{ timeoutMsg: 'the same password twice was not pointed out' }
		);
		await type(dialog, 'permissions', 'owner');
		await type(dialog, 'permissions-confirm', 'owner');
		await browser.waitUntil(() => protect.isEnabled(), { timeoutMsg: 'Protect stayed off' });
		await browser.saveScreenshot(join(OUT, 'protect-dialog.png'));
		await protect.click();

		await choose(browser, 'Save as a copy');
		await waitForDocument(browser);
		assert.match(await activeTab(browser), /protect-tool \(protected\)\.pdf/);
		assert.match(await statusText(browser), /of 3/);
		await waitForNotice(browser, 'Saved protect-tool (protected).pdf with its protection.', 'nothing said the copy was saved');
		await (await browser.$('[role="tab"] [data-protected]')).waitForExist({ timeoutMsg: 'the copy’s tab shows no lock' });
		assert.deepEqual(readFileSync(doc), original, 'the original was changed');
		assert.ok(encrypted(out), 'the copy is not encrypted');
		assert.equal(opensWith(out, null), false, 'the copy opens without its password');
		assert.equal(opensWith(out, 'user'), true, 'the copy does not open with its password');
	} finally {
		await stop();
	}
});

test('a document’s protection is written by its next save; its properties show it and remove it', async () => {
	const doc = sample('protect-document.pdf', 2);
	const { browser, stop } = await launch([doc]);
	try {
		await waitForDocument(browser);
		await fromMenu(browser, 'File', 'Protect');
		const dialog = await protectDialog(browser);
		await (await dialog.$('[data-require-open]')).click();
		await type(dialog, 'open', 'secret');
		await type(dialog, 'open-confirm', 'secret');
		await (await dialog.$('button=Protect')).click();

		const lock = await browser.$('[role="tab"] [data-protected]');
		await lock.waitForExist({ timeoutMsg: 'the tab shows no lock for the protection to come' });
		assert.match(await statusText(browser), /Unsaved changes/);
		assert.equal(encrypted(doc), false, 'the protection was written before the save');
		await browser.keys(['Control', 's']);
		await browser.waitUntil(() => encrypted(doc), { timeoutMsg: 'the save did not write the protection' });
		await browser.waitUntil(async () => /All changes saved/.test(await statusText(browser)), {
			timeoutMsg: 'the document is still unsaved after the save'
		});
		assert.equal(opensWith(doc, null), false, 'the file opens without its password');
		assert.equal(opensWith(doc, 'secret'), true, 'the file does not open with its password');

		// The lock opens the document's properties at Security.
		await lock.click();
		const properties = await browser.$('[data-properties-dialog]');
		await properties.waitForDisplayed({ timeoutMsg: 'the lock did not open the properties' });
		const kind = await properties.$('[data-protection-kind]');
		await browser.waitUntil(async () => (await kind.getText()) === 'Password to open', {
			timeoutMsg: 'Security does not say the file has a password to open'
		});
		assert.match(await properties.getText(), /AES 256-bit/);
		await browser.saveScreenshot(join(OUT, 'properties-security.png'));

		// Change protection…: nothing ticked removes it at the next save.
		await (await properties.$('button=Change protection…')).click();
		const change = await protectDialog(browser);
		const open = await change.$('[data-require-open]');
		assert.equal(await open.isSelected(), true, 'the dialog does not start from the protection the file has');
		await open.click();
		await (await change.$('button=Remove protection')).click();
		const pending = await properties.$('[data-protection-pending]');
		await browser.waitUntil(async () => (await pending.isExisting()) && (await pending.getText()) === 'None', {
			timeoutMsg: 'Security does not say what the save will write'
		});
		await (await properties.$('button=OK')).click();
		await browser.waitUntil(async () => !(await lock.isExisting()), { timeoutMsg: 'the lock stayed for protection removed' });
		await browser.keys(['Control', 's']);
		await browser.waitUntil(() => !encrypted(doc), { timeoutMsg: 'the save did not remove the protection' });
		assert.equal(opensWith(doc, null), true);
	} finally {
		await stop();
	}
});

test('Properties lists the fonts, and changes the title as one undo step', async () => {
	const doc = sample('properties.pdf', 3);
	const { browser, stop } = await launch([doc]);
	try {
		await waitForDocument(browser);
		await (await browser.$('.viewer-scroll')).click();
		await browser.keys(['Control', 'd']);
		const properties = await browser.$('[data-properties-dialog]');
		await properties.waitForDisplayed({ timeoutMsg: 'Ctrl+D did not open the properties' });
		const title = await properties.$('#properties-title');
		await browser.waitUntil(async () => (await title.getValue()) !== '', { timeoutMsg: 'the title was not read' });
		const text = await properties.getText();
		assert.match(text, /3 pages/);
		assert.match(text, /\(Letter\)/);
		await browser.saveScreenshot(join(OUT, 'properties-description.png'));

		await (await properties.$('[data-properties-tab="fonts"]')).click();
		const font = await properties.$('[data-font="Helvetica"]');
		await font.waitForExist({ timeoutMsg: 'the Fonts tab does not list Helvetica' });
		assert.match(await font.getText(), /Not embedded \(standard font\)/);
		await browser.saveScreenshot(join(OUT, 'properties-fonts.png'));

		await (await properties.$('[data-properties-tab="description"]')).click();
		await title.setValue('Annual report');
		await (await properties.$('button=OK')).click();
		await properties.waitForDisplayed({ reverse: true, timeoutMsg: 'OK did not close the properties' });
		await (await browser.$('button[role="menuitem"]=Edit')).click();
		await (await browser.$('[role="menuitem"]*=Undo change properties')).waitForExist({
			timeoutMsg: 'the change is not the step Undo names'
		});
		await browser.keys('Escape');
		await browser.keys(['Control', 's']);
		await browser.waitUntil(() => readFileSync(doc).includes('Annual report'), {
			timeoutMsg: 'the save did not write the title'
		});
	} finally {
		await stop();
	}
});
