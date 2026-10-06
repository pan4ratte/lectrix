// Help > Check for updates (AGENTS.md section 6.6, ADR 0011). Test runs never go online
// (LECTRIX_EPHEMERAL), so the check says that this copy doesn't check for updates: the
// menu item reaches Rust, and the answer shows as a notification, not a notice left
// checking. The answers from GitHub are covered by Vitest and the manual checklist.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch } from '../lib/app.mjs';

test('Help > Check for updates says what it found', async () => {
	const { browser, stop } = await launch();
	try {
		const help = await browser.$('button=Help');
		await help.waitForClickable({ timeout: 15000 });
		await help.click();
		const item = await browser.$('//*[@role="menuitem"][contains(., "Check for updates")]');
		await item.click();
		const alert = await browser.$('[role=alert]');
		await alert.waitForDisplayed({ timeoutMsg: 'no answer from Check for updates' });
		assert.match(await alert.getText(), /doesn’t check for updates/);
		const checking = await browser.execute(() => document.querySelector('#update-title')?.textContent ?? null);
		assert.equal(checking, null, 'the checking notice went away');
	} finally {
		await stop();
	}
});
