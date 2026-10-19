// Links on a page (AGENTS.md section 6.1) and bookmark titles as other producers write
// them (section 6.2). The file is written by hand, as the book this came from was made: a
// contents page whose lines are links to the chapters, and bookmark titles padded with NUL
// characters or ended with a line break, which Acrobat does not show.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';

/** Objects 1..n as a PDF with a classic cross-reference table. */
function assemble(objects) {
	let body = '%PDF-1.4\n';
	const offsets = objects.map((object, i) => {
		const at = body.length;
		body += `${i + 1} 0 obj\n${object}\nendobj\n`;
		return at;
	});
	const xref = body.length;
	body += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
	for (const at of offsets) body += `${String(at).padStart(10, '0')} 00000 n \n`;
	body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
	return Buffer.from(body, 'latin1');
}

/** A text string in UTF-16BE, followed by `padding` NUL characters. */
function utf16(text, padding = 0) {
	const units = [...text].map((c) => c.charCodeAt(0).toString(16).padStart(4, '0')).join('');
	return `<FEFF${units}${'0000'.repeat(padding)}>`;
}

function stream(content) {
	return `<< /Length ${content.length} >>\nstream\n${content}\nendstream`;
}

/** Lines of text 40 pt apart from y = 700 (user space), in 14 pt Helvetica. */
const lines = (texts) => stream(`BT /F1 14 Tf 72 700 Td ${texts.map((t) => `(${t}) Tj 0 -40 Td`).join(' ')} ET`);

function fixture() {
	const page = (contents, annots = '') => `<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 3 0 R >> >> /Contents ${contents} 0 R ${annots} >>`;
	// A link over line `n` of the contents page.
	const over = (n, target) => `<< /Type /Annot /Subtype /Link /Border [0 0 0] /Rect [72 ${692 - n * 40} 330 ${716 - n * 40}] ${target} >>`;
	return assemble([
		'<< /Type /Catalog /Pages 2 0 R /Outlines 12 0 R >>',
		'<< /Type /Pages /Kids [4 0 R 5 0 R 6 0 R 7 0 R] /Count 4 /MediaBox [0 0 612 792] >>',
		'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
		// 4: the contents page, with a link over each line
		page(8, '/Annots [16 0 R 17 0 R 18 0 R]'),
		page(9),
		page(10),
		page(11),
		lines(['Chapter 1 First things', 'Chapter 2 Second thoughts', 'The publisher on the web']),
		lines(['Chapter 1']),
		lines(['Chapter 2']),
		lines(['Index']),
		// 12: the outline
		'<< /Type /Outlines /First 13 0 R /Last 19 0 R /Count 4 >>',
		'<< /Title (Cover\\n) /Parent 12 0 R /Next 14 0 R /Dest [4 0 R /Fit] >>',
		`<< /Title ${utf16('Chapter 1 First things', 30)} /Parent 12 0 R /Prev 13 0 R /Next 15 0 R /Dest [5 0 R /Fit] >>`,
		`<< /Title ${utf16('Index', 12)} /Parent 12 0 R /Prev 14 0 R /Next 19 0 R /Dest [7 0 R /Fit] >>`,
		// 16 to 18: the links
		over(0, '/Dest [5 0 R /XYZ 0 792 null]'),
		over(1, '/A << /S /GoTo /D [6 0 R /Fit] >>'),
		over(2, '/A << /S /URI /URI (https://example.org/publisher) >>'),
		// 19: a bookmark that is a web link
		'<< /Title (Publisher) /Parent 12 0 R /Prev 15 0 R /A << /S /URI /URI (https://example.org/books) >> >>'
	]);
}

const pageBox = (browser) => browser.execute(() => document.querySelector('#page-box')?.value ?? null);

/** A point of page `index`, given in page points from its top-left, in the window. */
async function pagePoint(browser, index, x, y) {
	const r = await browser.execute((i) => document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect().toJSON(), index);
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

const move = (browser, at) => browser.action('pointer').move({ ...at, origin: 'viewport' }).perform();
const click = (browser, at) => browser.action('pointer').move({ ...at, origin: 'viewport' }).down().up().perform();
const cursor = (browser, at) => browser.execute((x, y) => getComputedStyle(document.elementFromPoint(x, y)).cursor, at.x, at.y);

test('links in the text are followed, and bookmark titles show as in Acrobat', async () => {
	mkdirSync(OUT, { recursive: true });
	const path = join(OUT, 'links.pdf');
	writeFileSync(path, fixture());
	// Web addresses are written here instead of being handed to the browser.
	const opened = join(OUT, 'links-opened.txt');
	rmSync(opened, { force: true });
	const { browser, stop } = await launch([path], { env: { LECTRIX_BROWSER_LOG: opened } });
	try {
		await waitForDocument(browser);

		// Titles end at their first NUL, and a line break at the end is not shown.
		await (await browser.$('button[role="tab"][aria-label="Bookmarks"]')).click();
		await browser.waitUntil(async () => (await browser.$$('[role=treeitem]')).length === 4, { timeoutMsg: 'the bookmarks did not show' });
		const titles = await browser.execute(() =>
			[...document.querySelectorAll('[role=treeitem]')].sort((a, b) => a.offsetTop - b.offsetTop).map((r) => r.textContent)
		);
		assert.deepEqual(
			titles.map((t) => t.trim()),
			['Cover', 'Chapter 1 First things', 'Index', 'Publisher']
		);
		assert.ok(titles.every((t) => !/[\u0000-\u001f�]/.test(t)), `a title shows padding: ${JSON.stringify(titles)}`);

		// The first line of the contents page is a link (its text runs from 72 to about
		// 220 pt, 76 to 100 pt from the top): the hand shows over it, the I-beam over text
		// that is no link.
		const first = await pagePoint(browser, 0, 120, 88);
		await move(browser, first);
		await browser.waitUntil(async () => (await cursor(browser, first)) === 'pointer', { timeoutMsg: 'no hand over a link' });
		// Its area is tinted meanwhile: the link's /Rect, 72 to 330 pt across and 76 to
		// 100 pt down the page.
		const tint = () =>
			browser.execute(() => {
				const r = document.querySelector('[data-link-hover]');
				return r ? ['x', 'y', 'width', 'height'].map((a) => Math.round(Number(r.getAttribute(a)))) : null;
			});
		assert.deepEqual(await tint(), [72, 76, 258, 24]);
		await move(browser, await pagePoint(browser, 0, 450, 88));
		await browser.waitUntil(async () => (await tint()) === null, { timeoutMsg: 'the tint stayed after the pointer left the link' });
		await move(browser, first);
		await browser.waitUntil(async () => (await tint()) !== null);
		// A tooltip says where it leads, as the page box will name the page.
		const leads = () => browser.execute(() => document.querySelector('[role=tooltip]')?.innerText.trim() ?? null);
		await browser.waitUntil(async () => (await leads()) === 'Page 2', { timeoutMsg: 'no tooltip says where the link leads' });
		// On the web link: its address.
		await move(browser, await pagePoint(browser, 0, 120, 168));
		await browser.waitUntil(async () => (await leads()) === 'https://example.org/publisher', {
			timeoutMsg: 'the tooltip does not show a web link’s address'
		});
		await move(browser, await pagePoint(browser, 0, 450, 300));
		await browser.waitUntil(async () => (await leads()) === null, { timeoutMsg: 'the tooltip stayed after the pointer left the link' });
		assert.equal(await pageBox(browser), '1');

		// A click follows it, gliding there as to a bookmark's place (the positions seen frame
		// by frame pass between the two pages); Alt+Left goes back to the contents page.
		await browser.execute(() => {
			const scroller = document.querySelector('.viewer-scroll');
			const seen = [scroller.scrollTop];
			window.__glide = { seen, done: false };
			const start = performance.now();
			const step = () => {
				seen.push(scroller.scrollTop);
				if (performance.now() - start < 1500) requestAnimationFrame(step);
				else window.__glide.done = true;
			};
			requestAnimationFrame(step);
		});
		await click(browser, first);
		await browser.waitUntil(async () => (await pageBox(browser)) === '2', { timeoutMsg: 'the link did not lead to page 2' });
		await browser.waitUntil(() => browser.execute(() => window.__glide.done), { timeoutMsg: 'the view was not watched to the end' });
		const glide = await browser.execute(() => ({
			seen: window.__glide.seen,
			reduced: matchMedia('(prefers-reduced-motion: reduce)').matches
		}));
		const passed = glide.seen.filter((top) => top !== glide.seen[0] && top !== glide.seen.at(-1));
		if (!glide.reduced) assert.ok(passed.length > 0, 'the link was jumped to instead of glided to');
		assert.equal(await browser.execute(() => getSelection()?.toString() ?? ''), '', 'following a link selected text');
		await browser.keys(['Alt', 'ArrowLeft']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '1', { timeoutMsg: 'Back did not return to the contents page' });

		// A GoTo action, on the second line.
		await click(browser, await pagePoint(browser, 0, 120, 128));
		await browser.waitUntil(async () => (await pageBox(browser)) === '3', { timeoutMsg: 'the second link did not lead to page 3' });
		await browser.keys(['Alt', 'ArrowLeft']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '1');

		// A drag that starts on a link selects its text and follows nothing.
		const from = await pagePoint(browser, 0, 76, 88);
		const to = await pagePoint(browser, 0, 200, 88);
		await browser
			.action('pointer')
			.move({ ...from, origin: 'viewport' })
			.down()
			.move({ x: Math.round((from.x + to.x) / 2), y: to.y, origin: 'viewport' })
			.pause(16)
			.move({ ...to, origin: 'viewport' })
			.pause(16)
			.up()
			.perform();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-floating-bar]') !== null), {
			timeoutMsg: 'dragging over a link did not select its text'
		});
		assert.equal(await pageBox(browser), '1', 'a drag over a link followed it');
		await browser.keys(['Escape']);

		// A web address goes to the system's browser (here, to the log), and Lectrix says
		// which address that was.
		await click(browser, await pagePoint(browser, 0, 120, 168));
		const told = await browser.$('//*[@role="status"][contains(., "Opened in your browser")]');
		await told.waitForDisplayed({ timeoutMsg: 'opening a web link was not told' });
		assert.ok((await told.getText()).includes('https://example.org/publisher'));
		assert.equal(readFileSync(opened, 'utf8').trim(), 'https://example.org/publisher');
		assert.equal(await pageBox(browser), '1');
		// Rust opens only an address a link on that page carries.
		const refused = await browser.execute(async () => {
			const open = (uri) =>
				window.__TAURI_INTERNALS__.invoke('open_link', { id: 1, from: { kind: 'page', page: 0 }, uri }).then(
					() => 'opened',
					(e) => e.message
				);
			return [await open('https://example.org/elsewhere'), await open('file:///C:/Windows/notepad.exe')];
		});
		assert.deepEqual(refused, ['Lectrix doesn’t open this link.', 'Lectrix doesn’t open this link.']);
		assert.equal(readFileSync(opened, 'utf8').trim(), 'https://example.org/publisher', 'an address no link carries was opened');

		// A bookmark that is a web link opens the same way, and only its own address.
		await (await browser.$('.tree-row*=Publisher')).click();
		await browser.waitUntil(() => readFileSync(opened, 'utf8').includes('https://example.org/books'), {
			timeoutMsg: 'the bookmark’s web link did not open'
		});
		const fromBookmark = await browser.execute(async () => {
			const ids = [...document.querySelectorAll('[role=treeitem]')].length;
			const open = (bookmark, uri) =>
				window.__TAURI_INTERNALS__.invoke('open_link', { id: 1, from: { kind: 'bookmark', bookmark }, uri }).then(
					() => 'opened',
					(e) => e.message
				);
			// The bookmark is object 19; object 14 is a bookmark too, but not this address's.
			return [ids, await open(14, 'https://example.org/books'), await open(19, 'https://example.org/other')];
		});
		assert.deepEqual(fromBookmark, [4, 'Lectrix doesn’t open this link.', 'Lectrix doesn’t open this link.']);
		assert.deepEqual(readFileSync(opened, 'utf8').trim().split('\n'), ['https://example.org/publisher', 'https://example.org/books']);

		// Settings turn the tint and the tooltip off; links work as before.
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Viewing')).click();
		await (await browser.$('label*=Highlight the link under the pointer')).click();
		assert.equal(await (await browser.$('#setting-highlightLinks')).getAttribute('aria-checked'), 'false');
		await (await browser.$('label*=Show where a link leads')).click();
		assert.equal(await (await browser.$('#setting-linkPreviews')).getAttribute('aria-checked'), 'false');
		await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Settings did not close'
		});
		await move(browser, await pagePoint(browser, 0, 450, 300));
		await move(browser, first);
		await browser.waitUntil(async () => (await cursor(browser, first)) === 'pointer', { timeoutMsg: 'no hand over a link with the tint off' });
		assert.equal(await tint(), null, 'the link is tinted though Settings turned that off');
		await browser.pause(700);
		assert.equal(await leads(), null, 'a tooltip says where the link leads though Settings turned that off');
		await click(browser, first);
		await browser.waitUntil(async () => (await pageBox(browser)) === '2', { timeoutMsg: 'the link did not work with the tint off' });
		await browser.keys(['Alt', 'ArrowLeft']);
		await browser.waitUntil(async () => (await pageBox(browser)) === '1');

		// With a markup tool, a click on a link is the tool's, not the link's.
		await browser.keys('h');
		await click(browser, first);
		await browser.pause(300);
		assert.equal(await pageBox(browser), '1', 'the Highlight tool followed a link');
	} finally {
		await stop();
	}
});
