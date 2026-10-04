// Crash recovery (AGENTS.md section 7): unsaved changes survive a crash and are offered at
// the next start; saving, discarding or quitting normally deletes the recovery copies.

import assert from 'node:assert/strict';
import { existsSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

const DIR = join(OUT, 'recovery');
// Copies every half second instead of every two minutes.
const env = { FOLIO_RECOVERY_DIR: DIR, FOLIO_RECOVERY_INTERVAL_MS: '500' };

const recoveryFiles = () => (existsSync(DIR) ? readdirSync(DIR) : []);

async function statusText(browser) {
	return browser.execute(() => document.querySelector('footer')?.textContent ?? '');
}

async function dialogText(browser) {
	return browser.execute(() => document.querySelector('[role="alertdialog"]')?.textContent ?? '');
}

/** Rotates page 1 through its context menu and waits for a recovery copy of it. */
async function changeAndWaitForCopy(browser) {
	await waitForDocument(browser);
	await (await browser.$('.page')).click({ button: 'right' });
	await (await browser.$('div=Rotate page clockwise')).click();
	await browser.waitUntil(async () => (await statusText(browser)).includes('Unsaved changes'), {
		timeoutMsg: 'the rotation did not mark the document as changed'
	});
	await browser.waitUntil(async () => recoveryFiles().some((f) => f.endsWith('.json')), {
		timeout: 10_000,
		timeoutMsg: 'no recovery copy was written'
	});
}

async function answer(browser, label) {
	const button = await browser.waitUntil(
		() =>
			browser.execute(
				(text) =>
					[...document.querySelectorAll('[role="alertdialog"] button')].find((b) => b.textContent?.trim() === text) ??
					false,
				label
			),
		{ timeoutMsg: `no "${label}" button in the dialog` }
	);
	await (await browser.$(button)).click();
}

async function waitForQuestion(browser) {
	await browser.waitUntil(async () => (await dialogText(browser)).includes('Restore unsaved changes?'), {
		timeout: 15_000,
		timeoutMsg: 'the restore question did not appear'
	});
}

test('a crash keeps unsaved changes, and the next start restores them', async () => {
	rmSync(DIR, { recursive: true, force: true });
	const path = sample('recovered.pdf', 3);

	let { browser, crash, stop } = await launch([path], { env });
	try {
		await changeAndWaitForCopy(browser);
	} finally {
		await crash();
	}
	assert.doesNotMatch(cli('info', path), /rotated pages/, 'the file itself is unchanged');

	// "Not now" (Escape) keeps the changes for the next start.
	({ browser, stop } = await launch([], { env }));
	try {
		await waitForQuestion(browser);
		assert.match(await dialogText(browser), /your changes to recovered\.pdf/);
		await browser.keys(['Escape']);
		await browser.pause(300);
		assert.equal(await browser.execute(() => document.querySelectorAll('.page').length), 0);
	} finally {
		await stop();
	}
	assert.ok(recoveryFiles().length > 0, 'not now: the copy stays');

	({ browser, stop } = await launch([], { env }));
	try {
		await waitForQuestion(browser);
		await answer(browser, 'Restore');
		await waitForDocument(browser);
		const shape = await browser.execute(() => {
			const r = document.querySelector('.page')?.getBoundingClientRect();
			return r ? r.width > r.height : null;
		});
		assert.equal(shape, true, 'page 1 comes back turned');
		assert.match(await statusText(browser), /Unsaved changes/);
		await browser.keys(['Control', 's']);
		await browser.waitUntil(async () => (await statusText(browser)).includes('All changes saved'), {
			timeout: 10_000,
			timeoutMsg: 'saving did not finish'
		});
		await browser.waitUntil(async () => recoveryFiles().length === 0, {
			timeout: 5000,
			timeoutMsg: `saving left recovery files: ${recoveryFiles().join(', ')}`
		});
	} finally {
		await stop();
	}
	assert.match(cli('info', path), /rotated pages \(page:degrees\): 1:90\r?\n/);
});

test('discarding recovered changes deletes them; quitting without saving leaves none', async () => {
	rmSync(DIR, { recursive: true, force: true });
	const path = sample('discarded.pdf', 3);

	let { browser, crash, stop, exitedByItself } = await launch([path], { env });
	try {
		await changeAndWaitForCopy(browser);
	} finally {
		await crash();
	}

	({ browser, stop } = await launch([], { env }));
	try {
		await waitForQuestion(browser);
		await answer(browser, 'Discard…');
		await browser.waitUntil(async () => (await dialogText(browser)).includes('Discard the recovered changes?'));
		await answer(browser, 'Discard');
		await browser.waitUntil(async () => recoveryFiles().length === 0, {
			timeout: 5000,
			timeoutMsg: 'discarding left recovery files'
		});
	} finally {
		await stop();
	}

	// Quitting normally with "Don't save" deletes this run's copy too.
	({ browser, exitedByItself } = await launch([path], { env }));
	try {
		await changeAndWaitForCopy(browser);
		await (await browser.$('button[aria-label="Close"]')).click();
		await answer(browser, 'Don’t save');
		await new Promise((r) => setTimeout(r, 1500));
	} finally {
		await exitedByItself();
	}
	assert.deepEqual(recoveryFiles(), [], 'a normal quit leaves no recovery copies');
	assert.doesNotMatch(cli('info', path), /rotated pages/);
});
