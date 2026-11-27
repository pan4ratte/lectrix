// Looking selected text up on the web (section 6.1, ADR 0037): the right-click menu of
// selected text offers websites, each marked as leading out of Lectrix, Settings switches
// each on or off and sets the language translations go into, and a text too long for an
// address is said to be shortened, until the notice is told not to show again. The
// addresses are written to a file instead of reaching the browser.

import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';

/** The long block's lines: 50 of 112 characters, about 9,000 once in an address. */
const LONG_LINE = 'lorem? ipsum? dolor? sit? amet? '.repeat(4).slice(0, 112);
const LONG_LINES = 50;

/**
 * One page: a line of 14 pt Helvetica, its baseline 92 pt from the top, then a block of
 * 6 pt lines 7 pt apart, their baselines from 122 pt from the top down.
 */
function fixture(text) {
	const block = Array.from({ length: LONG_LINES }, () => `(${LONG_LINE}) Tj T*`).join(' ');
	const content = `BT /F1 14 Tf 72 700 Td (${text}) Tj ET BT /F1 6 Tf 7 TL 72 670 Td ${block} ET`;
	const objects = [
		'<< /Type /Catalog /Pages 2 0 R >>',
		'<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>',
		'<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
		'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
		`<< /Length ${content.length} >>\nstream\n${content}\nendstream`
	];
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

/** A point of the page, given in page points from its top-left, in the window. */
async function pagePoint(browser, x, y) {
	const r = await browser.execute(() => document.querySelector('.page[data-page="0"]').getBoundingClientRect().toJSON());
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

const QUERY = 'Look%20up%20and%2For%20translate';
/** DeepL's: a slash keeps a backslash before it. */
const DEEPL_QUERY = 'Look%20up%20and%5C%2For%20translate';

test('selected text is looked up on the websites Settings has on, in the language it sets', async () => {
	mkdirSync(OUT, { recursive: true });
	const path = join(OUT, 'lookups.pdf');
	writeFileSync(path, fixture('Look up and/or translate'));
	const opened = join(OUT, 'lookups-opened.txt');
	rmSync(opened, { force: true });
	const addresses = () => (existsSync(opened) ? readFileSync(opened, 'utf8').split('\n').filter(Boolean) : []);
	const { browser, stop } = await launch([path], { env: { LECTRIX_BROWSER_LOG: opened } });
	try {
		await waitForDocument(browser);
		const onText = await pagePoint(browser, 120, 88);
		const lookups = () =>
			browser.execute(() =>
				[...document.querySelectorAll('[role=menuitem][data-lookup]')].map((item) => ({
					id: item.getAttribute('data-lookup'),
					name: item.getAttribute('aria-label'),
					icon: item.querySelector('svg') !== null
				}))
			);
		const openMenu = async (at = onText) => {
			await browser.action('pointer').move({ ...at, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
			await (await browser.$('//*[@role="menuitem"][contains(., "Copy selection")]')).waitForDisplayed({ timeoutMsg: 'no menu on the selection' });
		};
		const pick = async (id) => {
			const before = addresses().length;
			await (await browser.$(`[role=menuitem][data-lookup="${id}"]`)).click();
			await browser.waitUntil(() => addresses().length > before, { timeoutMsg: `${id} opened nothing` });
			return addresses().at(-1);
		};
		const selected = () => browser.execute(() => document.querySelectorAll('rect.fill-selection').length > 0);
		const notice = () =>
			browser.execute(() => [...document.querySelectorAll('[role=status], [role=alert]')].some((t) => t.textContent.includes('Only the start of the text')));
		const settings = async (change) => {
			await browser.keys(['Control', ',']);
			await (await browser.$('button[role="tab"]*=Viewing')).click();
			await change();
			await browser.keys('Escape');
			await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), { timeoutMsg: 'Settings stayed open' });
		};

		// Select the line with a triple-click, once its text is read: the I-beam shows over it.
		await browser.action('pointer').move({ ...onText, origin: 'viewport' }).perform();
		await browser.waitUntil(
			() => browser.execute((x, y) => getComputedStyle(document.elementFromPoint(x, y)).cursor, onText.x, onText.y).then((c) => c === 'text'),
			{ timeoutMsg: 'the page’s text was not read' }
		);
		const selectLine = async () => {
			await browser.action('pointer').move({ ...onText, origin: 'viewport' }).down().up().pause(60).down().up().pause(60).down().up().perform();
			await browser.waitUntil(selected, { timeoutMsg: 'the line was not selected' });
		};
		await selectLine();

		// Three to begin with, each marked as opening the browser.
		await openMenu();
		const offered = await lookups();
		assert.deepEqual(
			offered.map((l) => l.id),
			['deepl', 'google', 'googleScholar']
		);
		for (const l of offered) {
			assert.ok(l.icon, `${l.id} has no external link icon`);
			assert.match(l.name ?? '', /, opens in your browser$/, `${l.id}'s name does not say it opens the browser`);
		}
		assert.equal(await pick('google'), `https://www.google.com/search?q=${QUERY}`);
		await openMenu();
		assert.match(await pick('deepl'), new RegExp(`^https://www\\.deepl\\.com/translator#(en|de)/[a-z-]{2,7}/${DEEPL_QUERY}$`));

		// Settings: German, Google Translate and Wikipedia on, Google off.
		await settings(async () => {
			await (await browser.$('#setting-lookupLanguage')).click();
			await (await browser.$('[role="option"]*=German')).click();
			for (const id of ['googleTranslate', 'wikipedia', 'google']) await (await browser.$(`#setting-webLookups-${id}`)).click();
		});
		await selectLine();
		await openMenu();
		assert.deepEqual(
			(await lookups()).map((l) => l.id),
			['googleTranslate', 'deepl', 'wikipedia', 'googleScholar']
		);
		// One line before each group, and one before the page's items.
		assert.equal(await browser.execute(() => document.querySelectorAll('.menu-content [role=separator]').length), 4);
		assert.equal(await pick('googleTranslate'), `https://translate.google.com/?sl=auto&tl=de&op=translate&text=${QUERY}`);
		await openMenu();
		assert.equal(await pick('deepl'), `https://www.deepl.com/translator#en/de/${DEEPL_QUERY}`);
		await openMenu();
		assert.equal(await pick('wikipedia'), `https://de.wikipedia.org/w/index.php?search=${QUERY}`);
		assert.equal(await notice(), false, 'a short text was said to be shortened');

		// A text too long for an address: only its start goes, and a notice says so, until
		// Don't show again.
		const blockStart = await pagePoint(browser, 74, 120);
		const blockEnd = await pagePoint(browser, 400, 122 + (LONG_LINES - 1) * 7 - 2);
		let drag = browser.action('pointer').move({ ...blockStart, origin: 'viewport' }).down();
		for (let i = 1; i <= 10; i++) {
			drag = drag.move({
				x: Math.round(blockStart.x + ((blockEnd.x - blockStart.x) * i) / 10),
				y: Math.round(blockStart.y + ((blockEnd.y - blockStart.y) * i) / 10),
				origin: 'viewport',
				duration: 20
			});
		}
		await drag.up().perform();
		await browser.waitUntil(selected, { timeoutMsg: 'the block was not selected' });
		const onBlock = await pagePoint(browser, 150, 200);
		await openMenu(onBlock);
		const shortened = await pick('googleScholar');
		assert.ok(shortened.length <= 8192 && shortened.length > 7000, `the address is ${shortened.length} long`);
		// From the word the drag started on (it may begin inside it).
		assert.match(shortened, /^https:\/\/scholar\.google\.com\/scholar\?q=[a-z]*%3F%20ipsum%3F%20/);
		assert.ok(!shortened.endsWith('%20'), 'the text was not cut at a space');
		await browser.waitUntil(notice, { timeoutMsg: 'no notice that the text was shortened' });
		await (await browser.$('button=Don’t show again')).click();
		await browser.waitUntil(async () => !(await notice()), { timeoutMsg: 'the notice stayed' });
		await openMenu(onBlock);
		await pick('googleScholar');
		await browser.pause(500);
		assert.equal(await notice(), false, 'the notice showed again');
		await settings(async () => {
			assert.equal(await (await browser.$('#setting-lookupShortenedNotice')).getAttribute('aria-checked'), 'false');
		});

		// Without a selection, the menu has no lookups.
		await browser.action('pointer').move({ ...(await pagePoint(browser, 500, 60)), origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(async () => !(await selected()), { timeoutMsg: 'a click away left the text selected' });
		await browser.action('pointer').move({ ...onText, origin: 'viewport' }).down({ button: 2 }).up({ button: 2 }).perform();
		await (await browser.$('//*[@role="menuitem"][contains(., "Copy page as image")]')).waitForDisplayed({ timeoutMsg: 'no page menu' });
		assert.deepEqual(await lookups(), [], 'lookups offered with nothing selected');
		await browser.keys('Escape');
	} finally {
		await stop();
	}
});
