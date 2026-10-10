// What's new (AGENTS.md section 6.6). The first start after an update says so: beside the
// app's name on the start screen, or in a notification when documents open; both lead to
// the changelog. The next start doesn't; Help > What's new opens it at any time.
// LECTRIX_STATE_FILE gives these runs a state file of their own, recording an older
// version as the last one that started.

import assert from 'node:assert/strict';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, ROOT, launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const STATE = join(OUT, 'whats-new-state.json');
const { version } = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));

/** A state file left by an older version. */
function updatedState() {
	rmSync(STATE, { force: true });
	writeFileSync(STATE, JSON.stringify({ last_version: '0.9.0' }));
	return { LECTRIX_STATE_FILE: STATE };
}

/** The version headings and section headings the dialog shows. */
function dialogHeadings(browser) {
	return browser.execute(() => ({
		versions: [...document.querySelectorAll('[role=dialog] h3')].map((h) => h.textContent?.trim()),
		sections: [...document.querySelectorAll('[role=dialog] h4')].map((h) => h.textContent?.trim()),
		items: document.querySelectorAll('[role=dialog] li').length,
		focused: document.activeElement?.getAttribute('aria-label')
	}));
}

async function waitForChanges(browser, what) {
	await browser.waitUntil(async () => (await dialogHeadings(browser)).items > 0, { timeoutMsg: `${what} shows no changes` });
	return dialogHeadings(browser);
}

test('on the start screen, the update is told beside the name, once', async () => {
	const env = updatedState();
	let run = await launch([], { env });
	try {
		const { browser } = run;
		const block = await browser.$(`//*[@role="status"][contains(., "Updated to ${version}")]`);
		await block.waitForDisplayed({ timeout: 15000, timeoutMsg: 'no note beside the name after the update' });
		await browser.saveScreenshot(join(OUT, 'whats-new-start-screen.png'));
		const beside = await browser.execute(() => {
			const name = document.querySelector('h1')?.getBoundingClientRect();
			const note = document.querySelector('.start-update')?.getBoundingClientRect();
			return name && note ? { right: note.left > name.right, level: note.top < name.bottom && note.bottom > name.top } : null;
		});
		assert.deepEqual(beside, { right: true, level: true }, 'the note is to the right of the name');
		assert.equal(await (await browser.$('//*[@role="status"][contains(., "was updated")]')).isExisting(), false, 'no notification as well');

		await (await block.$('button=See what’s new')).click();
		const shown = await waitForChanges(browser, 'What’s new');
		await browser.saveScreenshot(join(OUT, 'whats-new.png'));
		assert.ok(shown.versions.includes(`Version ${version}`), `versions shown: ${shown.versions.join(', ')}`);
		assert.ok(shown.sections.includes('Viewer and tabs'), `sections shown: ${shown.sections.join(', ')}`);
		assert.equal(shown.focused, 'Changes by version', 'the list takes the focus, to scroll from the keyboard');
		assert.equal(await block.isExisting(), false, 'following it put the note away');
		assert.equal(JSON.parse(readFileSync(STATE, 'utf8')).last_version, version, 'the running version is recorded');
	} finally {
		await run.stop();
	}

	// The next start: nothing. Help > What's new still opens it.
	run = await launch([], { env });
	try {
		const { browser } = run;
		await (await browser.$('button*=Open')).waitForDisplayed({ timeout: 15000 });
		await browser.pause(1000);
		assert.equal(await (await browser.$('.start-update')).isExisting(), false);

		await (await browser.$('button=Help')).click();
		await (await browser.$('//*[@role="menuitem"][contains(., "What’s new")]')).click();
		await waitForChanges(browser, 'Help > What’s new');
		await browser.keys('Escape');
		await browser.waitUntil(async () => !(await (await browser.$('[role=dialog]')).isExisting()), {
			timeoutMsg: 'Esc did not close What’s new'
		});
	} finally {
		await run.stop();
	}
});

test('with a document open, a notification tells the update', async () => {
	const env = updatedState();
	const { browser, stop } = await launch([sample('whats-new.pdf', 2)], { env });
	try {
		await waitForDocument(browser);
		const notice = await browser.$(`//*[@role="status"][contains(., "Lectrix was updated to ${version}")]`);
		await notice.waitForDisplayed({ timeout: 15000, timeoutMsg: 'no notification after the update' });
		await browser.saveScreenshot(join(OUT, 'whats-new-notification.png'));
		await (await notice.$('button=What’s new')).click();
		await waitForChanges(browser, 'What’s new');
		assert.equal(await notice.isExisting(), false, 'the button dismissed the notification');
	} finally {
		await stop();
	}
});
