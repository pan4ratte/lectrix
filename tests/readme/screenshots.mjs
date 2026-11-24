// The README's screenshots (docs/images/), made from the release app, each in the light and
// the dark theme:
//
//     node tests/readme/screenshots.mjs [NAME...]
//
// with the names of the shots to make again, or none for all of them. A change to how a
// feature looks makes its shots again in the same commit (AGENTS.md section 10):
//
//     hero           the viewer, the Bookmarks panel, the annotation list, the title bar
//                    and toolbars (sections 6.1, 6.2, 6.5, 8)
//     fullscreen     full screen, the floating toolbar, two pages side by side (6.1)
//     annotations    annotations, the quick tools, the annotation list (6.5)
//     side-by-side   documents side by side (6.1)
//     presenter      presentations, the presenter view (6.1)
//     organize       Organize pages (6.7)
//     compare        Compare (6.13)
//     compress       Compress (6.11)
//
// The pages are those of OpenStax's Astronomy (CC BY 4.0; the README says so), downloaded
// once into target/readme/ and checked against the version the shots were made from. Its
// copies there get page labels, tidied bookmarks and annotations (made with pdf-cli, as
// Lectrix writes them), and a second version of a few pages for Compare.
//
// Needs what the end-to-end tests need (tests/e2e/lib/app.mjs: the release app, pdf-cli,
// tauri-driver, msedgedriver), and Python with PyMuPDF and Pillow for
// tests/readme/prepare.py. The window is laid out at the shot's size whatever the screen
// (Chromium's device metrics, through msedgedriver), in English, and captured at 1.5 times
// that size, so the images are the same on every machine. prepare.py then rounds their
// corners, draws a thin outline, and writes them as WebP (a quarter of a PNG's size, with
// no loss to see).

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createWriteStream, existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';

import { ROOT, launch, organizePages, waitForDocument } from '../e2e/lib/app.mjs';
import { cli } from '../e2e/lib/pdfcli.mjs';

const WORK = join(ROOT, 'target', 'readme');
const IMAGES = join(ROOT, 'docs', 'images');
const SCALE = 1.5;
const AUTHOR = 'Sam Rivera';

const BOOK = {
	url: 'https://assets.openstax.org/oscms-prodcms/media/documents/Astronomy-OP_zItt6LJ.pdf',
	sha256: '81f6d708839e307036c3f778f50d2b7b256eb361576367ac65c911f3c4f0802a',
	path: join(WORK, 'openstax-astronomy.pdf')
};

/** Where the documents the shots open are made. */
const DOC = {
	book: join(WORK, 'Astronomy.pdf'),
	milkyWay: join(WORK, 'The Milky Way Galaxy.pdf'),
	sun: join(WORK, 'The Sun.pdf'),
	older: join(WORK, 'Section 25.1.pdf'),
	newer: join(WORK, 'Section 25.1 (revised).pdf')
};

// ----- the documents -----

async function download() {
	if (existsSync(BOOK.path) && sha256(BOOK.path) === BOOK.sha256) return;
	console.log(`downloading ${BOOK.url}`);
	const res = await fetch(BOOK.url, { headers: { 'user-agent': 'Mozilla/5.0' } });
	if (!res.ok) throw new Error(`the book did not download: ${res.status} ${res.statusText}`);
	const part = `${BOOK.path}.part`;
	await pipeline(Readable.fromWeb(res.body), createWriteStream(part));
	if (sha256(part) !== BOOK.sha256) {
		throw new Error(`${BOOK.url} is not the version the shots were made from (its SHA-256 differs)`);
	}
	renameSync(part, BOOK.path);
}

function sha256(path) {
	return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function python(...args) {
	execFileSync(process.env.PYTHON ?? 'python', [join(ROOT, 'tests', 'readme', 'prepare.py'), ...args], { stdio: 'inherit' });
}

/** Runs the pdf-cli steps in turn on a copy of `from`, which becomes `to`. Each step writes
 * the whole file (the book is 161 MB), so only the last two are kept at a time. */
function steps(from, to, list) {
	let at = from;
	list.forEach((args, i) => {
		const next = `${to}.${i % 2}.pdf`;
		cli(...args.slice(0, 2), at, next, ...args.slice(2));
		at = next;
	});
	renameSync(at, to);
	rmSync(`${to}.${list.length % 2}.pdf`, { force: true });
}

// pdf-cli's annotation commands, by the shots' author, in the app's preset colours.
const COLOR = { yellow: 'FFD100', green: '6AD928', red: 'E52237', blue: '0000FF' };
const by = (args) => [...args, '--author', AUTHOR];
const markup = (page, text, color, ...more) => by(['annot', 'markup', '--page', `${page}`, '--text', text, '--color', color, ...more]);
const textBox = (page, rect, text, color) => by(['annot', 'text', '--page', `${page}`, '--rect', rect, '--text', text, '--size', '9', '--color', color]);
/** A drawing of strokes, each a list of [x, y] points (PDF points from the page's top left). */
const ink = (page, color, strokes) =>
	by([
		'annot',
		'ink',
		'--page',
		`${page}`,
		'--color',
		color,
		'--width',
		'1.5',
		...strokes.flatMap((s) => ['--stroke', s.map((pt) => pt.map((v) => v.toFixed(1)).join(',')).join(';')])
	]);

/** An ellipse around a box, as a pen would draw it: a little more than once round. */
function ring([x0, y0, x1, y1], pad = 6) {
	const cx = (x0 + x1) / 2;
	const cy = (y0 + y1) / 2;
	const rx = (x1 - x0) / 2 + pad;
	const ry = (y1 - y0) / 2 + pad;
	return Array.from({ length: 50 }, (_, i) => {
		const a = -Math.PI / 2 + (i / 44) * 2 * Math.PI;
		return [cx + rx * Math.cos(a), cy + ry * Math.sin(a)];
	});
}

/** The book's own page numbering: the front matter in roman numerals, then 1 on page 13. */
const LABELS = ['--rule', '1:roman-lower', '--rule', '13:decimal'];

async function prepare() {
	mkdirSync(WORK, { recursive: true });
	await download();
	const outlined = join(WORK, 'outlined.pdf');
	python('outline', BOOK.path, outlined);
	const { yellow, green, red, blue } = COLOR;
	steps(outlined, DOC.book, [
		['labels', 'set', ...LABELS],
		// The opening of chapter 25 (book page 895) and the page after it.
		markup(907, 'the band of faint white light called the Milky Way', yellow, '--opacity', '0.4', '--note', 'The band is the disk of our own Galaxy, seen edge-on from inside it.'),
		markup(907, 'multitude of individual stars', green, '--opacity', '0.4'),
		ink(907, red, [ring([86.2, 420.5, 209.9, 432.8])]),
		markup(908, 'Herschel therefore concluded that the stellar system', yellow, '--opacity', '0.4'),
		markup(908, 'They found that most of the stars', blue, '--kind', 'underline'),
		textBox(908, '72,470,205,505', 'Caroline Herschel went on to discover eight comets.', red),
		// An arrow from the text box to the painting.
		ink(908, red, [
			[
				[207, 488],
				[226, 500]
			],
			[
				[219, 492],
				[226, 500],
				[216, 499]
			]
		])
	]);
	rmSync(outlined);
	cli('pages', 'extract', '--pages', '907-946', DOC.book, DOC.milkyWay);
	cli('pages', 'extract', '--pages', '539-574', DOC.book, DOC.sun);
	// Compare's two versions: the pages as the book has them, with its page labels.
	const labelled = join(WORK, 'labelled.pdf');
	cli('labels', 'set', BOOK.path, labelled, ...LABELS);
	cli('pages', 'extract', '--pages', '908-913', labelled, DOC.older);
	rmSync(labelled);
	python('edit', DOC.older, DOC.newer);
}

// ----- the app -----

/** A Chromium DevTools command for the session's window, through msedgedriver. */
async function cdp(browser, cmd, params = {}) {
	const res = await fetch(`http://127.0.0.1:4444/session/${browser.sessionId}/ms/cdp/execute`, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ cmd, params })
	});
	const body = await res.json();
	if (!res.ok) throw new Error(`${cmd}: ${JSON.stringify(body).slice(0, 300)}`);
	return body.value;
}

async function layOut(browser, [width, height]) {
	await cdp(browser, 'Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: SCALE, mobile: false });
	await browser.waitUntil(() => browser.execute((w) => innerWidth === w, width), { timeoutMsg: 'the window did not take its size' });
	await settle(browser);
}

/** Waits for the layout to settle and every page picture on screen to be drawn. */
async function settle(browser, ms = 600) {
	await browser.pause(ms);
	await browser.waitUntil(
		() =>
			browser.execute(() =>
				[...document.querySelectorAll('img')].every((i) => !i.getBoundingClientRect().width || (i.complete && i.naturalWidth > 0))
			),
		{ timeout: 30_000, timeoutMsg: 'the pictures on screen were not drawn' }
	);
	await browser.pause(300);
}

/** The state file a shot starts from: no What's new, the theme, and what else it asks. */
function stateFile(theme, { settings = {}, panes = {} } = {}) {
	const { version } = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
	const path = join(WORK, `state-${theme}.json`);
	writeFileSync(
		path,
		JSON.stringify({
			last_version: version,
			settings: { appearance: theme, author: AUTHOR, ...settings },
			panes: { leftOpen: false, rightOpen: false, leftPanels: ['pages', 'bookmarks'], rightPanels: ['annotations'], ...panes }
		})
	);
	return path;
}

/** Points on a page in the window, from PDF points (top-left origin) on page `index`. */
async function onPage(browser, index, points) {
	return browser.execute(
		(i, pts) => {
			const page = document.querySelector(`.page[data-page="${i}"]`);
			const r = page.getBoundingClientRect();
			const k = r.width / 612;
			return pts.map(([x, y]) => [Math.round(r.left + x * k), Math.round(r.top + y * k)]);
		},
		index,
		points
	);
}

/** Scrolls the viewer until PDF point `y` (from the top) of page `index` is near its top. */
async function scrollTo(browser, index, y) {
	await browser.execute(
		(i, at) => {
			const scroller = document.querySelector('.viewer-scroll');
			const r = document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect();
			scroller.scrollTop += r.top + at * (r.width / 612) - scroller.getBoundingClientRect().top - 16;
		},
		index,
		y
	);
	await settle(browser);
}

async function dragOver(browser, [x0, y0], [x1, y1]) {
	await browser
		.action('pointer', { parameters: { pointerType: 'mouse' } })
		.move({ x: x0, y: y0 })
		.down()
		.move({ x: Math.round((x0 + x1) / 2), y: y1, duration: 150 })
		.move({ x: x1, y: y1, duration: 150 })
		.up()
		.perform();
}

/** A click with Ctrl held, as one set of actions: the key goes down before the click and up
 * after it. */
async function ctrlClick(browser, element) {
	const CONTROL = '\uE009';
	await browser.actions([
		browser.action('key').down(CONTROL).pause(20).pause(20).pause(20).up(CONTROL),
		browser.action('pointer', { parameters: { pointerType: 'mouse' } }).pause(20).move({ origin: element }).down().up()
	]);
}

async function goToPage(browser, label) {
	const box = await browser.$('[data-page-box]');
	await box.click();
	await browser.keys(['Control', 'a']);
	await browser.keys([...label, 'Enter']);
	await settle(browser);
}

const menuItem = (browser, name) => browser.$(`//*[@role="menuitem" or @role="menuitemradio"][contains(., "${name}")]`);

// ----- the shots -----

const SHOTS = {
	/** The top of the README: the opening of a chapter, its bookmark open on the left and its
	 * annotations listed on the right. */
	hero: {
		size: [1280, 800],
		files: () => [DOC.book],
		state: {
			settings: { scroll_mode: 'singlePage' },
			panes: { leftOpen: true, leftWidth: 270, leftActive: 'bookmarks', rightOpen: true, rightWidth: 300 }
		},
		async setUp(browser) {
			// The tree draws only the rows in view: scroll it until the chapter's is there.
			const selector = '//*[@role="treeitem"][@aria-level="1"][contains(., "The Milky Way Galaxy")]';
			await browser.waitUntil(
				async () => {
					if (await (await browser.$(selector)).isExisting()) return true;
					await browser.execute(() => {
						const tree = document.querySelector('[role="tree"]');
						let el = tree;
						while (el && el.scrollHeight <= el.clientHeight) el = el.parentElement;
						if (el) el.scrollTop += 300;
					});
					return false;
				},
				{ timeoutMsg: 'the chapter is not in the Bookmarks panel' }
			);
			const row = await browser.$(selector);
			await browser.execute((el) => el.scrollIntoView({ block: 'center' }), row);
			await row.click();
			await (await row.$('[data-toggle]')).click();
			await (await browser.$('button[aria-label="Zoom options"]')).click();
			await (await menuItem(browser, 'Fit page')).click();
			// The row stays selected, without the focus ring.
			await browser.execute(() => document.querySelector('.viewer-scroll')?.focus());
			await settle(browser, 1200);
		}
	},

	/** Full screen: a spread with the pages alone, the toolbar floating over them at the bottom. */
	fullscreen: {
		size: [1280, 800],
		files: () => [DOC.book],
		state: { settings: { scroll_mode: 'twoPage' } },
		async setUp(browser) {
			await goToPage(browser, '895');
			await browser.keys(['F11']);
			await (await browser.$('[role=toolbar][aria-label="Page, zoom and annotation tools"]')).waitForDisplayed({
				timeoutMsg: 'F11 did not float the toolbar'
			});
			await (await browser.$('button[aria-label="Zoom options"]')).click();
			await (await menuItem(browser, 'Fit page')).click();
			// The first full screen says how to leave it: not in the picture.
			const dismiss = await browser.$('button[aria-label="Dismiss"]');
			await dismiss.waitForClickable({ timeoutMsg: 'no notification about leaving full screen' });
			await dismiss.click();
			await browser.waitUntil(async () => !(await dismiss.isExisting()), { timeoutMsg: 'the notification stayed' });
			await browser.execute(() => document.querySelector('.viewer-scroll')?.focus());
			await settle(browser, 1500);
		}
	},

	/** Annotations on a page, the list beside them, and the quick tools over selected text. */
	annotations: {
		size: [1180, 760],
		files: () => [DOC.book],
		state: { panes: { rightOpen: true, rightWidth: 320 } },
		async setUp(browser) {
			await goToPage(browser, '896');
			await scrollTo(browser, 907, 200);
			const [from, to] = await onPage(browser, 907, [
				[154, 350],
				[344, 350]
			]);
			await dragOver(browser, from, to);
			await settle(browser, 800);
		}
	},

	/** Two chapters side by side. */
	'side-by-side': {
		size: [1280, 800],
		files: () => [DOC.milkyWay, DOC.sun],
		async setUp(browser) {
			await (await browser.$('.doc-tab*=The Milky Way Galaxy.pdf')).click({ button: 'right' });
			await (await menuItem(browser, 'Show beside the current tab')).click();
			await browser.waitUntil(async () => (await browser.$$('[data-split-pane]')).length === 2, { timeoutMsg: 'no panes side by side' });
			await settle(browser, 1200);
		}
	},

	/** The presenter view, the pages on another screen. */
	presenter: {
		size: [1280, 800],
		files: () => [DOC.milkyWay],
		env: { LECTRIX_AUDIENCE_ANY_SCREEN: '1' },
		async setUp(browser) {
			const main = await browser.getWindowHandle();
			await browser.keys(['F5']);
			await browser.waitUntil(
				() => browser.execute(() => document.querySelector('[data-presentation="on"][data-presenter-view]') !== null),
				{ timeoutMsg: 'F5 did not show the presenter view' }
			);
			await browser.switchToWindow(main);
			await settle(browser, 1500);
		}
	},

	/** Organize pages: the grid of a chapter, two pages selected. */
	organize: {
		size: [1280, 800],
		files: () => [DOC.book],
		async setUp(browser) {
			await goToPage(browser, '895');
			await organizePages(browser);
			await settle(browser, 800);
			await (await browser.$('button[aria-label="Larger pages"]')).click();
			await settle(browser, 800);
			// The chapter's opening at the top, two pages after it selected.
			const cell = (n) => `[data-organize-view] [role=option][aria-posinset="${n}"]`;
			await browser.execute((sel) => {
				const el = [...document.querySelectorAll('[data-organize-view] *')].find(
					(e) => e.scrollHeight > e.clientHeight + 10 && /auto|scroll/.test(getComputedStyle(e).overflowY)
				);
				const top = document.querySelector(sel).getBoundingClientRect().top;
				el.scrollTop += top - el.getBoundingClientRect().top - 24;
			}, cell(907));
			await settle(browser, 800);
			await (await browser.$(`${cell(909)} [data-picture]`)).click();
			await ctrlClick(browser, await browser.$(`${cell(910)} [data-picture]`));
			await (await browser.$(`${cell(912)} [data-picture]`)).moveTo();
			await settle(browser, 1000);
		}
	},

	/** Compare: two versions of a section, the differences listed and marked. */
	compare: {
		size: [1280, 800],
		files: () => [],
		dialogs: () => [[DOC.older], [DOC.newer]],
		async setUp(browser) {
			const tile = await browser.$('[data-tool="compare"]');
			await tile.waitForDisplayed();
			await tile.click();
			for (const slot of [0, 1]) {
				const open = await browser.$(`[data-compare-slot="${slot}"] .button-primary`);
				await open.waitForClickable();
				await open.click();
			}
			await (await browser.$('[data-compare-run]')).waitForClickable({ timeout: 20_000 });
			await (await browser.$('[data-compare-run]')).click();
			await browser.waitUntil(() => browser.execute(() => document.querySelectorAll('[data-compare-row]').length > 0), {
				timeout: 30_000,
				timeoutMsg: 'comparing listed no differences'
			});
			await settle(browser, 1500);
		}
	},

	/** Compress: the result, a page as it was and as it will be. */
	compress: {
		size: [1180, 760],
		files: () => [],
		dialogs: () => [[DOC.milkyWay]],
		async setUp(browser) {
			const tile = await browser.$('[data-tool="compress"]');
			await tile.waitForDisplayed();
			await tile.click();
			await (await browser.$('[data-compress-run]')).waitForClickable({ timeout: 20_000 });
			await (await browser.$('[data-compress-run]')).click();
			await browser.waitUntil(
				() => browser.execute(() => document.querySelector('[data-compress-dialog]')?.getAttribute('data-step') === 'result'),
				{ timeout: 120_000, timeoutMsg: 'compressing did not finish' }
			);
			await settle(browser, 1500);
		}
	}
};

async function shoot(name, theme) {
	const shot = SHOTS[name];
	const { browser, stop } = await launch(shot.files(), {
		dialogs: shot.dialogs?.(),
		// The webview in English: the app formats numbers in its language (7.1 MB, 1,206
		// pages), whatever this machine's is.
		env: { LECTRIX_STATE_FILE: stateFile(theme, shot.state), LECTRIX_BROWSER_ARGS: '--lang=en-US', ...shot.env }
	});
	try {
		if (shot.files().length > 0) await waitForDocument(browser);
		await layOut(browser, shot.size);
		try {
			await shot.setUp(browser);
		} catch (e) {
			const { data } = await cdp(browser, 'Page.captureScreenshot', { format: 'png' });
			const failure = join(WORK, `failure-${name}-${theme}.png`);
			writeFileSync(failure, Buffer.from(data, 'base64'));
			throw new Error(`${name} (${theme}) failed; the window as it was: ${failure}`, { cause: e });
		}
		const { data } = await cdp(browser, 'Page.captureScreenshot', { format: 'png' });
		const captured = join(WORK, `${name}-${theme}.png`);
		writeFileSync(captured, Buffer.from(data, 'base64'));
		// Rounded corners and an outline, drawn into the image: the README can't style it.
		const out = join(IMAGES, `${name}-${theme}.webp`);
		python('frame', captured, out, theme);
		console.log(`wrote ${out}`);
	} finally {
		await stop();
	}
}

const wanted = process.argv.slice(2);
for (const name of wanted) if (!SHOTS[name]) throw new Error(`no shot named ${name}: ${Object.keys(SHOTS).join(', ')}`);
await prepare();
mkdirSync(IMAGES, { recursive: true });
for (const name of wanted.length > 0 ? wanted : Object.keys(SHOTS)) {
	for (const theme of ['light', 'dark']) await shoot(name, theme);
}
