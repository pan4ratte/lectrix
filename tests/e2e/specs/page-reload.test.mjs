// A webview page that reloads while documents are open (a renderer crash, or a test driver
// navigating it) gets them back from Rust instead of showing an empty window.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

test('open documents come back after the page reloads', async () => {
	const path = sample('reload.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await browser.url('http://tauri.localhost/');
		await waitForDocument(browser);
		const tabs = await browser.execute(() =>
			[...document.querySelectorAll('[role=tab]')].map((t) => t.textContent?.trim() ?? '')
		);
		assert.ok(
			tabs.some((t) => t.includes('reload.pdf')),
			`the tab is back: ${JSON.stringify(tabs)}`
		);
	} finally {
		await stop();
	}
});
