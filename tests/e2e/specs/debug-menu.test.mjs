// The Debug menu (AGENTS.md section 9): hidden in release builds unless LECTRIX_DEBUG=1,
// it shows notifications and notices on demand. Here it rehearses a whole update, the
// notice's What's new included, which a test can't reach otherwise without a newer release.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch } from '../lib/app.mjs';

/** Picks Debug > `submenu` > `item`. */
async function debugMenu(browser, submenu, item) {
	await (await browser.$('button=Debug')).click();
	const trigger = await browser.$(`//*[@role="menuitem"][contains(., "${submenu}")]`);
	await trigger.click();
	await (await browser.$(`//*[@role="menuitem"][normalize-space(.)="${item}"]`)).click();
}

test('release builds show no Debug menu', async () => {
	const { browser, stop } = await launch();
	try {
		await (await browser.$('button=Help')).waitForClickable({ timeout: 15000 });
		assert.equal(await (await browser.$('button=Debug')).isExisting(), false);
	} finally {
		await stop();
	}
});

test('the Debug menu rehearses an update from notice to restart', async () => {
	const { browser, stop } = await launch([], { env: { LECTRIX_DEBUG: '1' } });
	try {
		await (await browser.$('button=Debug')).waitForClickable({ timeout: 15000 });
		await debugMenu(browser, 'Update notice', 'Update available');
		const title = await browser.$('#update-title');
		await browser.waitUntil(async () => / is available$/.test(await title.getText()), { timeoutMsg: 'no update notice' });

		await (await browser.$('button=See what’s new')).click();
		await browser.waitUntil(async () => (await browser.$$('[role=dialog] li')).length > 0, {
			timeoutMsg: 'What’s new shows nothing before updating'
		});
		await browser.saveScreenshot(join(OUT, 'debug-whats-new-before-update.png'));
		await (await browser.$('//*[@role="dialog"]//button[normalize-space(.)="Update"]')).click();

		await browser.waitUntil(async () => /^Downloading/.test(await title.getText()), { timeoutMsg: 'no download' });
		await browser.waitUntil(async () => / ready to install$/.test(await title.getText()), {
			timeout: 10000,
			timeoutMsg: 'the rehearsed download did not finish'
		});
		await (await browser.$('button=Restart now')).click();
		const told = await browser.$('//*[@role="status"][contains(., "Rehearsal: Lectrix would restart now.")]');
		await told.waitForDisplayed({ timeoutMsg: 'Restart now did not say what it would do' });
		// (It takes a moment to go: it sinks out of its place.)
		await title.waitForExist({ reverse: true, timeout: 2000, timeoutMsg: 'the notice did not go away' });

		await debugMenu(browser, 'After an update', 'On the start screen');
		await (await browser.$('.start-update')).waitForDisplayed({ timeoutMsg: 'no note beside the name' });
		await (await browser.$('button=Debug')).click();
		await (await browser.$('//*[@role="menuitem"][normalize-space(.)="Clear all"]')).click();
		await browser.waitUntil(async () => !(await (await browser.$('.start-update')).isExisting()), {
			timeoutMsg: 'Clear all left the note'
		});
		await browser.waitUntil(async () => (await browser.$$('[role=status]')).length === 0, {
			timeout: 2000,
			timeoutMsg: 'Clear all left notifications'
		});
	} finally {
		await stop();
	}
});
