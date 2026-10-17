// Recent files and the remembered view of each file (AGENTS.md section 6.1).
// E2E runs keep app state in memory (LECTRIX_EPHEMERAL), so this happens in one session:
// close a document, open it again from the start screen's recent files (as a grid).
// Also the start screen shown over open documents, from the title bar's icon and name.

import assert from 'node:assert/strict';
import { test } from 'node:test';

import { launch, waitForDocument } from '../lib/app.mjs';
import { sample } from '../lib/pdfcli.mjs';

const pageBox = (browser) => browser.execute(() => document.querySelector('#page-box')?.value ?? null);
const zoomText = (browser) =>
	browser.execute(() => document.querySelector('[aria-label^="Zoom level"]')?.getAttribute('aria-label') ?? null);

const startButton = (browser) => browser.$('button[aria-label="Lectrix: start screen"]');
const selectedTabs = (browser) =>
	browser.execute(() => [...document.querySelectorAll('[role=tablist][aria-label="Open documents"] [role=tab][aria-selected="true"]')].length);

/** A tool tile's look: how far it has taken the brand gradient (0 at rest, 1 under the
 * pointer), and its icon's stroke beside the text's colour, as r, g, b. */
const toolLook = (browser) =>
	browser.execute(() => {
		const rgb = (css) => {
			const n = (css.match(/[\d.]+/g) ?? []).map(Number).slice(0, 3);
			return css.startsWith('color(') ? n.map((v) => Math.round(v * 255)) : n;
		};
		const probe = document.createElement('span');
		probe.style.color = 'var(--lectrix-fg)';
		document.body.append(probe);
		const fg = rgb(getComputedStyle(probe).color);
		probe.remove();
		const tile = document.querySelector('[data-tool="open"]');
		return {
			accent: Number(getComputedStyle(tile).getPropertyValue('--tool-accent')),
			icon: rgb(getComputedStyle(tile.querySelector('stop')).stopColor),
			fg
		};
	});

test('the title bar shows the start screen over open documents, which stay open', async () => {
	const first = sample('start-a.pdf', 8);
	const second = sample('start-b.pdf', 2);
	const { browser, stop } = await launch([first, second]);
	try {
		await waitForDocument(browser);
		await browser.keys(['Control', 'g']);
		await browser.keys(['Control', 'a']);
		await browser.keys(['2', 'Enter']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '2', { timeoutMsg: 'did not go to page 2' });
		await browser.keys(['Escape']);

		// Another tab and back: the document is where it was.
		await browser.keys(['Control', 'Tab']);
		await browser.waitUntil(async () => (await browser.$('[role=tab][aria-selected="true"]').getText()).includes('start-a.pdf'), {
			timeoutMsg: 'Ctrl+Tab did not show the other document'
		});
		await browser.keys(['Control', 'Tab']);
		await browser.waitUntil(async () => (await browser.$('[role=tab][aria-selected="true"]').getText()).includes('start-b.pdf'));
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await pageBox(browser)) === '2', { timeoutMsg: 'the document went back to its first page behind another tab' });

		// The icon and name: the start screen, with both tabs still there and neither shown.
		const start = await startButton(browser);
		assert.equal(await start.getAttribute('aria-pressed'), 'false');
		await start.click();
		await (await browser.$('h2=Recent files')).waitForDisplayed({ timeoutMsg: 'the title bar did not show the start screen' });
		assert.equal(await start.getAttribute('aria-pressed'), 'true');
		// On the start screen the button is plain, though the pointer is still on it.
		assert.equal(
			await browser.execute((el) => getComputedStyle(el).backgroundColor, start),
			'rgba(0, 0, 0, 0)',
			'the title bar button is filled while the start screen shows'
		);
		assert.equal((await browser.$$('[role=tablist][aria-label="Open documents"] [role=tab]')).length, 2, 'a tab closed');
		assert.equal(await selectedTabs(browser), 0, 'a tab is shown as on screen behind the start screen');
		assert.equal(await pageBox(browser), null, 'the document is still on screen');
		// The recent files show without the animation of a change of view.
		assert.equal(await browser.execute(() => document.querySelector('.recent-enter')), null, 'the recent files were animated as the start screen showed');

		// The tools are plain at rest, and take the brand gradient under the pointer.
		const rest = await toolLook(browser);
		assert.equal(rest.accent, 0, 'a tool has the accent at rest');
		assert.ok(rest.icon.every((v, i) => Math.abs(v - rest.fg[i]) <= 1), `a tool's icon is ${rest.icon} at rest, the text ${rest.fg}`);
		const tile = await (await browser.$('[data-tool="open"]')).getLocation();
		await browser.action('pointer').move({ x: Math.round(tile.x + 40), y: Math.round(tile.y + 30), origin: 'viewport' }).perform();
		await browser.waitUntil(async () => (await toolLook(browser)).accent === 1, { timeoutMsg: 'the tool under the pointer did not take the accent' });
		const hovered = await toolLook(browser);
		assert.ok(hovered.icon.some((v, i) => Math.abs(v - hovered.fg[i]) > 8), `the icon under the pointer is ${hovered.icon}, the text ${hovered.fg}`);
		await browser.action('pointer').move({ x: Math.round(tile.x + 40), y: Math.round(tile.y - 40), origin: 'viewport' }).perform();
		await browser.waitUntil(async () => (await toolLook(browser)).accent === 0, { timeoutMsg: 'the tool kept the accent after the pointer left' });

		// The same click goes back to the document, where it was.
		await start.click();
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await pageBox(browser)) === '2', { timeoutMsg: 'the document did not come back on page 2' });
		assert.equal(await start.getAttribute('aria-pressed'), 'false');
		assert.equal(await selectedTabs(browser), 1);

		// From the start screen, a tab goes to its document; Ctrl+Tab, to the one it was shown over.
		await start.click();
		await (await browser.$('h2=Recent files')).waitForDisplayed();
		await (await browser.$('.doc-tab*=start-a.pdf')).click();
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await browser.$('[role=tab][aria-selected="true"]').getText()).includes('start-a.pdf'), {
			timeoutMsg: 'the tab clicked on the start screen did not show'
		});
		await start.click();
		await (await browser.$('h2=Recent files')).waitForDisplayed();
		await browser.keys(['Control', 'Tab']);
		await waitForDocument(browser);
		assert.match(await browser.$('[role=tab][aria-selected="true"]').getText(), /start-a\.pdf/, 'Ctrl+Tab did not go back to the document');

		// A tab closed from the start screen leaves the start screen; a recent file that is
		// open goes to its tab.
		await start.click();
		await (await browser.$('h2=Recent files')).waitForDisplayed();
		// A row opens its file wherever it is clicked, and its button reaches no further: the
		// tools above stay theirs (in WebKit, an overlay on the row covered the whole screen).
		const hits = await browser.execute(() => {
			const hit = (/** @type {Element | null} */ el) => {
				const box = el?.getBoundingClientRect();
				return box ? document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2) : null;
			};
			const row = document.querySelector('.recent-row');
			return {
				size: hit(row?.querySelector('td:nth-child(3)') ?? null) === row?.querySelector('.recent-open'),
				tool: hit(document.querySelector('[data-tool="open"]'))?.closest('[data-tool]')?.getAttribute('data-tool') ?? null
			};
		});
		assert.equal(hits.size, true, 'a click on a recent file’s size does not open it');
		assert.equal(hits.tool, 'open', 'something covers the Open tile');
		await (await browser.$('button[aria-label="Close start-b.pdf"]')).click();
		await browser.waitUntil(async () => (await browser.$$('[role=tablist][aria-label="Open documents"] [role=tab]')).length === 1, {
			timeoutMsg: 'the tab did not close'
		});
		assert.equal(await (await browser.$('h2=Recent files')).isDisplayed(), true, 'closing a tab left the start screen');
		await (await browser.$('button*=start-a.pdf')).click();
		await waitForDocument(browser);
		assert.equal((await browser.$$('[role=tablist][aria-label="Open documents"] [role=tab]')).length, 1, 'the open file opened again');
	} finally {
		await stop();
	}
});

test('a closed document reopens from the recent list where it was left', async () => {
	const first = sample('recent-a.pdf', 8);
	const second = sample('recent-b.pdf', 2);
	const { browser, stop } = await launch([first, second]);
	try {
		await waitForDocument(browser);
		// The second file is active; go back to the first, then to page 6 and zoom in twice.
		await browser.keys(['Control', 'Tab']);
		await browser.waitUntil(async () => (await browser.getTitle()).includes('recent-a') || (await pageBox(browser)) !== null);
		await browser.keys(['Control', 'g']);
		await browser.keys(['Control', 'a']);
		await browser.keys(['6', 'Enter']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '6', { timeoutMsg: 'did not go to page 6' });
		await browser.keys(['Escape']);
		await browser.keys(['Control', '=']);
		await browser.keys(['Control', '=']);
		await browser.pause(300);
		const zoom = await zoomText(browser);
		// The second step, pressed while the first still glides, goes on from where that was
		// heading, and both land on presets.
		assert.match(zoom ?? '', /^Zoom level (50|75|100|125|150|200|300|400)%$/, `zoom after two steps: ${zoom}`);

		// Close both (the view is remembered on closing), and reopen the first from the list.
		await browser.keys(['Control', 'w']);
		await browser.keys(['Control', 'w']);
		await (await browser.$('button*=recent-a.pdf')).waitForDisplayed({ timeoutMsg: 'recent-a.pdf is not in the recent list' });
		// A table: name, when it was opened, size; newest first.
		const rows = await browser.execute(() =>
			[...document.querySelectorAll('table tbody tr')].map((tr) => [...tr.cells].slice(0, 3).map((td) => td.textContent?.trim() ?? ''))
		);
		const order = rows.map((r) => r[0]);
		assert.ok(order[0] === 'recent-b.pdf' && order[1] === 'recent-a.pdf', `newest first: ${order.join(' | ')}`);
		assert.match(rows[1][1], /^Today, \d{1,2}:\d{2}/, `opened: ${rows[1][1]}`);
		assert.match(rows[1][2], /^[\d.,]+ (bytes|KB)$/, `size: ${rows[1][2]}`);

		// The grid: each file's first page, then its name, then when it was opened and its size.
		// Changing the view brings the files in animated (the table, shown first, was not), and
		// the mark of the view shown goes to the button picked.
		assert.equal(await browser.execute(() => document.querySelector('.recent-enter')), null, 'the recent files were animated at first');
		await (await browser.$('button[aria-label="Grid"]')).click();
		assert.equal(await browser.execute(() => document.querySelector('.recent-grid.recent-enter') !== null), true, 'the grid came in without its animation');
		await browser.waitUntil(
			() =>
				browser.execute(() => {
					const mark = document.querySelector('[aria-label="Show recent files as"] .tool-mark').getBoundingClientRect();
					const grid = document.querySelector('button[aria-label="Grid"]').getBoundingClientRect();
					return Math.abs(mark.left - grid.left) < 1 && Math.abs(mark.width - grid.width) < 1;
				}),
			{ timeoutMsg: 'the mark did not go to the Grid button' }
		);
		await browser.waitUntil(
			() =>
				browser.execute(() => {
					const pages = [...document.querySelectorAll('.recent-card img.recent-page')];
					return pages.length === 2 && pages.every((img) => img.complete && img.naturalWidth > 0);
				}),
			{ timeoutMsg: 'the first pages did not show in the grid' }
		);
		const cards = await browser.execute(() =>
			[...document.querySelectorAll('.recent-card')].map((card) => ({
				name: card.querySelector('.recent-name')?.textContent?.trim() ?? '',
				details: card.querySelector('.recent-details')?.textContent?.replace(/\s+/g, ' ').trim() ?? '',
				ratio: (() => {
					const img = card.querySelector('img.recent-page');
					return img ? img.naturalHeight / img.naturalWidth : 0;
				})()
			}))
		);
		assert.deepEqual(
			cards.map((c) => c.name),
			['recent-b.pdf', 'recent-a.pdf'],
			'the grid is newest first'
		);
		assert.match(cards[1].details, /^Today, \d{1,2}:\d{2}.* · [\d.,]+ (bytes|KB)$/, `details: ${cards[1].details}`);
		// The sample pages are US Letter, 612 x 792 pt.
		assert.ok(Math.abs(cards[1].ratio - 792 / 612) < 0.02, `the preview has the page's shape: ${cards[1].ratio}`);

		const card = await (await browser.$('.recent-card*=recent-a.pdf')).$('button.recent-open');
		await card.click();
		await waitForDocument(browser);
		await browser.waitUntil(async () => (await pageBox(browser)) === '6', {
			timeoutMsg: 'the document did not reopen on page 6'
		});
		assert.equal(await zoomText(browser), zoom, 'the zoom is remembered too');
	} finally {
		await stop();
	}
});
