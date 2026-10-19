// Print (section 6.12, ADR 0028): File > Print… and Ctrl+P open the Print dialog for the
// document shown, with the printers, the pages (by number or label), the layout and a
// preview of each sheet; Print sends the job with progress, and a notification says so.
// The job goes to Microsoft Print to PDF, which LECTRIX_PRINT_OUTPUT has write a file
// without asking for its name; where that printer is missing, the dialog is checked but
// nothing is printed. A document whose security settings forbid printing says so instead.

import assert from 'node:assert/strict';
import { existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { tabWalk, unnamedControls } from '../lib/a11y.mjs';
import { OUT, keys, launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

const PDF_PRINTER = 'Microsoft Print to PDF';

const step = (browser) => browser.execute(() => document.querySelector('[data-print-dialog]')?.getAttribute('data-step') ?? null);
const notices = (browser) =>
	browser.execute(() =>
		[...document.querySelectorAll('[role="status"], [role="alert"]')].map((n) => n.textContent.trim()).join('\n')
	);
const sheetText = (browser) => browser.execute(() => document.querySelector('[data-print-sheet]')?.textContent.trim() ?? '');

test('Ctrl+P prints the pages asked for, with a preview of each sheet', async () => {
	const doc = sample('print.pdf', 5);
	const out = join(OUT, 'print-output.pdf');
	rmSync(out, { force: true });
	const { browser, stop } = await launch([doc], { env: { LECTRIX_PRINT_OUTPUT: out } });
	try {
		await waitForDocument(browser);
		await keys(browser, ['Control', 'p']);
		await browser.waitUntil(async () => (await step(browser)) === 'settings', { timeoutMsg: 'Ctrl+P did not open the Print dialog' });
		const dialog = await browser.$('[data-print-dialog]');
		assert.match(await dialog.getText(), /Print print\.pdf/);

		// The printers are listed; pick the one that writes a file, if there is one.
		await browser.waitUntil(
			() => browser.execute(() => !document.querySelector('[data-print-settings]')?.textContent.includes('Looking for printers')),
			{ timeout: 30_000, timeoutMsg: 'the printers were never listed' }
		);
		const printer = await browser.$('[data-print-settings] button[aria-label="Printer"]');
		const hasPrinters = await printer.isExisting();
		let canPrint = false;
		if (hasPrinters) {
			await printer.click();
			const option = await browser.$(`[role="option"]*=${PDF_PRINTER}`);
			canPrint = await option.isExisting();
			if (canPrint) await option.click();
			else await keys(browser, ['Escape']);
		}
		if (!canPrint) {
			// eslint-disable-next-line no-console
			console.log(`no ${PDF_PRINTER} here: the dialog is checked, nothing is printed`);
		}

		// Pages 2 to 3 of 5: two sheets in the preview, the page printed drawn on each.
		await (await browser.$('[data-print-range]')).setValue('2-3');
		if (canPrint) {
			await browser.waitUntil(async () => (await sheetText(browser)) === '1 of 2', {
				timeout: 30_000,
				timeoutMsg: `the preview does not show two sheets: ${await sheetText(browser)}`
			});
			await browser.waitUntil(
				() =>
					browser.execute(() => {
						const img = document.querySelector('[data-print-preview] img');
						return img instanceof HTMLImageElement && img.complete && img.naturalWidth > 0 && img.dataset.printPage === '1';
					}),
				{ timeoutMsg: 'the preview does not show page 2' }
			);
			await browser.saveScreenshot(join(OUT, 'print-dialog.png'));
		}

		// A range that names no page says how to type one, and Print waits.
		await (await browser.$('[data-print-range]')).setValue('9');
		await browser.waitUntil(async () => (await dialog.getText()).includes('Type pages as'), {
			timeoutMsg: 'a page that is not there was not pointed out'
		});
		assert.equal(await (await browser.$('[data-print-run]')).isEnabled(), false, 'Print is offered for no pages');
		await (await browser.$('[data-print-range]')).setValue('2-3');

		// Every Tab stop is named and shows focus (AGENTS.md section 8).
		const stops = (await tabWalk(browser, { max: 40 })).filter((s) => s.tag !== 'none');
		for (const s of stops) {
			assert.ok(s.name, `an unnamed Tab stop (${s.tag} .${s.cls})`);
			assert.ok(s.visibleFocus, `focus not visible on "${s.name}"`);
		}
		assert.deepEqual(await unnamedControls(browser), [], 'controls without an accessible name');

		if (!canPrint) {
			await keys(browser, ['Escape']);
			await browser.waitUntil(async () => (await step(browser)) === null, { timeoutMsg: 'Esc did not close the dialog' });
			return;
		}
		await browser.waitUntil(() => (browser.$('[data-print-run]')).then((b) => b.isEnabled()), {
			timeout: 30_000,
			timeoutMsg: 'Print is not offered'
		});
		await (await browser.$('[data-print-run]')).click();
		await browser.waitUntil(async () => (await step(browser)) === null, { timeout: 60_000, timeoutMsg: 'printing did not finish' });
		await browser.waitUntil(async () => (await notices(browser)).includes(`Sent 2 pages of print.pdf to ${PDF_PRINTER}.`), {
			timeoutMsg: `nothing said the pages were sent: ${await notices(browser)}`
		});
		await browser.waitUntil(() => existsSync(out), { timeout: 30_000, timeoutMsg: 'the printer wrote no file' });
	} finally {
		await stop();
	}
	if (existsSync(out)) assert.match(cli('info', out), /pages: 2/);
});

test('a document that forbids printing says so instead of printing', async () => {
	const doc = sample('print-forbidden.pdf', 2, ['--owner-password', 'owner', '--allow', 'copy']);
	const { browser, stop } = await launch([doc]);
	try {
		await waitForDocument(browser);
		await keys(browser, ['Control', 'p']);
		await browser.waitUntil(async () => (await notices(browser)).includes('print-forbidden.pdf doesn’t allow printing.'), {
			timeoutMsg: 'nothing said printing is not allowed'
		});
		assert.equal(await step(browser), null, 'the Print dialog opened');
	} finally {
		await stop();
	}
});
