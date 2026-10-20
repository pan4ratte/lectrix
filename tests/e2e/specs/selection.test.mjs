// Selected text (AGENTS.md section 6.1): a side of the selection, taken with the pointer,
// extends or reduces it, and the other side stays where it is; Shift and the arrow keys do
// the same from the keyboard; Settings can put a handle on each side; and the system reads
// the selected text as the page's own selection.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

/** Converts page points of page `index` (view space at zoom 1) to viewport pixels. */
async function pagePoint(browser, index, x, y) {
	const r = await browser.execute(
		(i) => document.querySelector(`.page[data-page="${i}"]`).getBoundingClientRect().toJSON(),
		index
	);
	const k = r.width / 612;
	return { x: Math.round(r.left + x * k), y: Math.round(r.top + y * k) };
}

/** The selection's rectangles, one per line, in viewport pixels. */
async function selected(browser) {
	return browser.execute(() =>
		[...document.querySelectorAll('.page rect.fill-selection')].map((el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		})
	);
}

async function drag(browser, from, to, steps = 6) {
	let action = browser.action('pointer').move({ ...from, origin: 'viewport' }).down();
	for (let i = 1; i <= steps; i++) {
		action = action.move({
			x: Math.round(from.x + ((to.x - from.x) * i) / steps),
			y: Math.round(from.y + ((to.y - from.y) * i) / steps),
			origin: 'viewport',
			duration: 20
		});
	}
	await action.up().perform();
}

const cursor = (browser) => browser.execute(() => getComputedStyle(document.querySelector('.viewer-scroll')).cursor);

/** A point on a side of a rectangle, halfway up it. */
const side = (r, which) => ({ x: Math.round(r[which]), y: Math.round((r.top + r.bottom) / 2) });

const near = (a, b) => Math.abs(a - b) <= 1;

test('dragging a side of the selected text extends and reduces the selection', async () => {
	const path = sample('selection-sides.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);

		// Part of the marker line (96 pt from the top, from 36 pt in).
		await drag(browser, await pagePoint(browser, 0, 100, 92), await pagePoint(browser, 0, 200, 92));
		await browser.waitUntil(async () => (await selected(browser)).length === 1, { timeoutMsg: 'no text was selected' });
		const [first] = await selected(browser);

		// Inside the selection the pointer is the text cursor; on a side, it shows that the
		// side can be dragged. (The pointer was let go on the end side: it comes back to it.)
		const middle = { x: Math.round((first.left + first.right) / 2), y: side(first, 'left').y };
		await browser.action('pointer').move({ ...middle, origin: 'viewport' }).perform();
		await browser.waitUntil(async () => (await cursor(browser)) === 'text', { timeoutMsg: 'no text cursor inside the selection' });
		for (const which of ['right', 'left']) {
			await browser.action('pointer').move({ ...side(first, which), origin: 'viewport' }).perform();
			await browser.waitUntil(async () => (await cursor(browser)) === 'ew-resize', {
				timeoutMsg: `no resize cursor on the ${which} side of the selection`
			});
		}

		// The end side, dragged on: the selection ends further on and starts where it did.
		await drag(browser, side(first, 'right'), { x: side(first, 'right').x + 80, y: side(first, 'right').y });
		const [extended] = await selected(browser);
		assert.ok(extended.right > first.right + 60, `the end went from ${first.right} to ${extended.right}`);
		assert.ok(near(extended.left, first.left), `the start moved from ${first.left} to ${extended.left}`);

		// The start side, dragged into the selection: it starts later and ends where it did.
		await drag(browser, side(extended, 'left'), { x: side(extended, 'left').x + 40, y: side(extended, 'left').y });
		const [reduced] = await selected(browser);
		assert.ok(reduced.left > extended.left + 25, `the start went from ${extended.left} to ${reduced.left}`);
		assert.ok(near(reduced.right, extended.right), `the end moved from ${extended.right} to ${reduced.right}`);

		// A side pressed and let go where it is changes nothing. (The start side: a press on
		// the end side right after a click there would be a double-click.)
		await browser.action('pointer').move({ ...side(reduced, 'left'), origin: 'viewport' }).down().up().perform();
		assert.deepEqual(await selected(browser), [reduced]);

		// The end side, dragged down a line: the selection runs on into that line.
		const below = await pagePoint(browser, 0, 300, 116);
		await drag(browser, side(reduced, 'right'), below);
		const lines = await selected(browser);
		assert.equal(lines.length, 2, JSON.stringify(lines));
		assert.ok(near(lines[0].left, reduced.left), `the start moved from ${reduced.left} to ${lines[0].left}`);
		assert.ok(lines[1].top > lines[0].top, JSON.stringify(lines));

		// The end side, dragged back past the start: the selection turns over there.
		await drag(browser, side(lines[1], 'right'), { x: Math.round(reduced.left) - 50, y: side(reduced, 'left').y });
		const [turned, ...rest] = await selected(browser);
		assert.equal(rest.length, 0, 'the selection still runs into the next line');
		assert.ok(turned.left < reduced.left - 30, `the selection starts at ${turned.left}, was ${reduced.left}`);
		assert.ok(Math.abs(turned.right - reduced.left) <= 1, `the selection ends at ${turned.right}, not at ${reduced.left}`);

		// A double-click selects the whole word, wherever in it the pointer is: "quick" on the
		// marker line runs from 60 to 88 pt, and the click is past its middle.
		await browser.keys('Escape');
		const inWord = await pagePoint(browser, 0, 80, 92);
		await browser.action('pointer').move({ ...inWord, origin: 'viewport' }).down().up().pause(60).down().up().perform();
		await browser.waitUntil(async () => (await selected(browser)).length === 1, { timeoutMsg: 'a double-click selected no word' });
		const [quick] = await selected(browser);
		const wordStart = (await pagePoint(browser, 0, 60, 92)).x;
		const wordEnd = (await pagePoint(browser, 0, 88, 92)).x;
		assert.ok(
			Math.abs(quick.left - wordStart) <= 2 && Math.abs(quick.right - wordEnd) <= 2,
			`"quick" is ${wordStart} to ${wordEnd}, selected ${JSON.stringify(quick)}`
		);

		// Presses in a row on a side still count as a double- and a triple-click: the third
		// selects the whole line (which starts 36 pt in).
		const press = browser.action('pointer').move({ ...side(quick, 'left'), origin: 'viewport' });
		await press.down().up().pause(60).down().up().pause(60).down().up().perform();
		const lineEnd = (await pagePoint(browser, 0, 300, 92)).x;
		await browser.waitUntil(
			async () => {
				const now = await selected(browser);
				return now.length === 1 && now[0].left < quick.left - 20 && now[0].right > lineEnd;
			},
			{ timeoutMsg: 'a triple-click on a side of the selection did not select the line' }
		);
	} finally {
		await stop();
	}
});

/** Double-clicks "quick" on the marker line of the first page: 60 to 88 pt. */
async function selectQuick(browser) {
	const inWord = await pagePoint(browser, 0, 80, 92);
	await browser.action('pointer').move({ ...inWord, origin: 'viewport' }).down().up().pause(60).down().up().perform();
	await browser.waitUntil(async () => (await selected(browser)).length === 1, { timeoutMsg: 'a double-click selected no word' });
}

test('Shift and the arrow keys extend and reduce the selection', async () => {
	const path = sample('selection-keys.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await selectQuick(browser);
		/** Where a place on the marker line is on screen (its words: "The" from 36 pt, "quick"
		 * from 60, "brown" from 91.3 to 124). */
		const x = async (pt) => (await pagePoint(browser, 0, pt, 92)).x;
		/** Waits for the selection to be one line from `left` to `right` (points), and returns it. */
		const line = async (left, right, message) => {
			const [l, r] = [await x(left), await x(right)];
			let now = [];
			await browser.waitUntil(
				async () => {
					now = await selected(browser);
					return now.length === 1 && Math.abs(now[0].left - l) <= 2 && Math.abs(now[0].right - r) <= 2;
				},
				{ timeoutMsg: `${message}: expected ${l} to ${r}` }
			).catch(async (e) => {
				throw new Error(`${e.message}, selected ${JSON.stringify(await selected(browser))}`);
			});
			return now[0];
		};

		// Shift+Right adds a character (the space after the word), Ctrl+Shift+Right the next
		// word, and Shift+Left takes its last letter away again. The start stays.
		await browser.keys(['Shift', 'ArrowRight']);
		await line(60, 91.33, 'Shift+Right');
		await browser.keys(['Control', 'Shift', 'ArrowRight']);
		await line(60, 124, 'Ctrl+Shift+Right');
		await browser.keys(['Shift', 'ArrowLeft']);
		const before = await line(60, 117.33, 'Shift+Left');

		// Shift+Down runs the selection on into the next line; Shift+Up brings it back.
		await browser.keys(['Shift', 'ArrowDown']);
		await browser.waitUntil(async () => (await selected(browser)).length === 2, { timeoutMsg: 'Shift+Down did not reach the next line' });
		await browser.keys(['Shift', 'ArrowUp']);
		await browser.waitUntil(async () => (await selected(browser)).length === 1, { timeoutMsg: 'Shift+Up did not come back' });
		assert.deepEqual(await selected(browser), [before]);

		// Ctrl+Shift+Left goes back a word at a time; past the start, the selection turns over
		// rather than becoming nothing.
		await browser.keys(['Control', 'Shift', 'ArrowLeft']);
		await line(60, 91.33, 'Ctrl+Shift+Left');
		await browser.keys(['Control', 'Shift', 'ArrowLeft']);
		await line(36, 60, 'Ctrl+Shift+Left past the start');

		// Ctrl+Shift+Down takes in the rest of the paragraph, at least to the end of the line.
		await browser.keys(['Control', 'Shift', 'ArrowDown']);
		const lineEnd = await x(300);
		await browser.waitUntil(async () => (await selected(browser)).some((r) => r.right > lineEnd), {
			timeoutMsg: 'Ctrl+Shift+Down did not reach the end of the paragraph'
		});

		// Without a selection the keys are the viewer's own: nothing gets selected.
		await browser.keys('Escape');
		await browser.keys(['Shift', 'ArrowRight']);
		await browser.pause(150);
		assert.deepEqual(await selected(browser), []);
	} finally {
		await stop();
	}
});

test('a drag from a double-click selects whole words; a click puts a text cursor in the text', async () => {
	const path = sample('selection-caret.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		const x = async (pt) => (await pagePoint(browser, 0, pt, 92)).x;
		const caretX = () =>
			browser.execute(() => {
				const el = document.querySelector('.page [data-text-caret]');
				if (!el) return null;
				const r = el.getBoundingClientRect();
				return (r.left + r.right) / 2;
			});
		const outlined = () => browser.execute(() => document.querySelector('.page rect.annotation-outline') !== null);

		// "quick" double-clicked (60 to 88 pt) and held, then dragged into the middle of "brown"
		// (91.3 to 124): the selection takes in the whole of "brown".
		const inQuick = await pagePoint(browser, 0, 80, 92);
		const inBrown = await pagePoint(browser, 0, 105, 92);
		await browser
			.action('pointer')
			.move({ ...inQuick, origin: 'viewport' })
			.down()
			.up()
			.pause(60)
			.down()
			.move({ ...inBrown, origin: 'viewport', duration: 60 })
			.up()
			.perform();
		await browser.waitUntil(
			async () => {
				const now = await selected(browser);
				return now.length === 1 && Math.abs(now[0].left - (await x(60))) <= 2 && Math.abs(now[0].right - (await x(124))) <= 2;
			},
			{ timeoutMsg: `the drag did not select whole words: ${JSON.stringify(await selected(browser))}` }
		);
		await browser.keys('Escape');

		// The Highlight tool: a click in the text puts the text cursor there, before "quick".
		await browser.keys('h');
		await browser.pause(600);
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 61, 92)), origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(async () => (await caretX()) !== null, { timeoutMsg: 'a click in the text put no text cursor there' });
		assert.ok(Math.abs((await caretX()) - (await x(60))) <= 2, `the text cursor is at ${await caretX()}`);
		assert.deepEqual(await selected(browser), [], 'a click selected text');

		// Ctrl+Shift+Right selects "quick", and letting Shift go highlights it; the text cursor
		// is then after it, and the arrow keys move it on.
		await browser.keys(['Control', 'Shift', 'ArrowRight']);
		await browser.waitUntil(async () => (await caretX()) !== null && Math.abs((await caretX()) - (await x(88))) <= 2, {
			timeoutMsg: `the text cursor did not come back after the word: ${await caretX()}`
		});
		assert.deepEqual(await selected(browser), [], 'the selection was not marked');
		const after = await caretX();
		await browser.keys('ArrowRight');
		await browser.waitUntil(async () => (await caretX()) > after + 1, { timeoutMsg: 'Right did not move the text cursor' });

		// Another tool takes the text cursor away.
		await browser.keys('Escape');
		await browser.keys('Escape');
		await browser.waitUntil(async () => (await caretX()) === null, { timeoutMsg: 'the text cursor stayed' });

		// With the Select tool, a click on the highlight selects it; a drag from it selects the
		// text under it instead.
		await browser.pause(600);
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 75, 92)), origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(outlined, { timeoutMsg: 'a click on the highlight did not select it' });
		await browser.keys('Escape');
		await browser.waitUntil(async () => !(await outlined()), { timeoutMsg: 'Esc left the highlight selected' });
		await browser.pause(600);
		await drag(browser, await pagePoint(browser, 0, 70, 92), await pagePoint(browser, 0, 115, 92));
		await browser.waitUntil(async () => (await selected(browser)).length === 1, { timeoutMsg: 'a drag from the highlight selected no text' });
		assert.equal(await outlined(), false, 'the drag selected the highlight');

		// The Select tool has the text cursor too: a click after "quick" (away from the
		// highlight), then Ctrl+Shift+Right selects "brown", with the quick tools over it.
		await browser.keys('Escape');
		await browser.pause(600);
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 90.5, 92)), origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(async () => (await caretX()) !== null, { timeoutMsg: 'a click with the Select tool put no text cursor' });
		await browser.keys(['Control', 'Shift', 'ArrowRight']);
		await browser.waitUntil(
			async () => {
				const now = await selected(browser);
				return now.length === 1 && Math.abs(now[0].right - (await x(124))) <= 2;
			},
			{ timeoutMsg: `Ctrl+Shift+Right from the text cursor did not select "brown": ${JSON.stringify(await selected(browser))}` }
		);
		await (await browser.$('[role=toolbar][aria-label="Quick tools"]')).waitForDisplayed({ timeoutMsg: 'no quick tools over the selection' });
		assert.equal(await caretX(), null, 'the text cursor shows beside the selection');
	} finally {
		await stop();
	}
});

test('Settings puts handles on the sides of the selection, and can put the quick tools away', async () => {
	const path = sample('selection-handles.pdf', 2);
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		/** The handles' bars: which side each is on, and where it is on screen. */
		const handles = () =>
			browser.execute(() =>
				[...document.querySelectorAll('.page [data-selection-handle]')].map((el) => {
					const r = el.getBoundingClientRect();
					return { side: el.dataset.selectionHandle, x: (r.left + r.right) / 2, top: r.top, bottom: r.bottom };
				})
			);

		// Off until asked for.
		await selectQuick(browser);
		assert.deepEqual(await handles(), []);

		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Viewing')).click();
		await (await browser.$('label*=Show handles on selected text')).click();
		assert.equal(await (await browser.$('#setting-selectionHandles')).getAttribute('aria-checked'), 'true');
		await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Settings did not close'
		});

		await browser.keys('Escape');
		await selectQuick(browser);
		const [word] = await selected(browser);
		await browser.waitUntil(async () => (await handles()).length === 2, { timeoutMsg: 'the selection has no handles' });
		const [start, end] = await handles();
		assert.deepEqual([start.side, end.side], ['start', 'end']);
		assert.ok(near(start.x, word.left) && near(end.x, word.right), `handles at ${start.x} and ${end.x}, the word is ${JSON.stringify(word)}`);

		// The end handle's knob is at the bottom of the line: dragged from there, the selection
		// goes on, and the handle goes with it.
		const knob = { x: Math.round(end.x), y: Math.round(end.bottom) };
		await drag(browser, knob, { x: knob.x + 60, y: Math.round((word.top + word.bottom) / 2) });
		const [longer] = await selected(browser);
		assert.ok(longer.right > word.right + 40 && near(longer.left, word.left), `dragging the handle left ${JSON.stringify(longer)}`);
		const moved = (await handles()).find((h) => h.side === 'end');
		assert.ok(near(moved.x, longer.right), `the end handle is at ${moved.x}, the selection ends at ${longer.right}`);

		// Settings > Toolbars puts the quick tools away with one switch, keeping which ones
		// are chosen; the selection keeps its handles.
		const quick = () => browser.$('[role=toolbar][aria-label="Quick tools"]');
		assert.equal(await (await quick()).isDisplayed(), true, 'no quick tools over the selection');
		await browser.keys(['Control', ',']);
		await (await browser.$('button[role="tab"]*=Toolbars')).click();
		await (await browser.$('label*=Show quick tools over selected text')).click();
		assert.equal(await (await browser.$('#setting-quickBar')).getAttribute('aria-checked'), 'false');
		assert.equal(await (await browser.$('#setting-quickTools-highlight')).isEnabled(), false, 'a quick tool can be chosen with the bar off');
		assert.equal(await (await browser.$('#setting-quickTools-highlight')).getAttribute('aria-checked'), 'true', 'the chosen tools were forgotten');
		await (await browser.$('[role="dialog"] button[aria-label="Close"]')).click();
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[role="dialog"]') === null), {
			timeoutMsg: 'Settings did not close'
		});
		await browser.keys('Escape');
		await selectQuick(browser);
		await browser.waitUntil(async () => (await handles()).length === 2, { timeoutMsg: 'the selection lost its handles' });
		assert.equal(await (await quick()).isExisting(), false, 'the quick tools showed with their bar switched off');
	} finally {
		await stop();
	}
});

test('the selected text is the page’s own selection, for the system to read, unless copying is forbidden', async () => {
	const path = sample('selection-system.pdf', 1);
	const locked = join(OUT, 'selection-system-no-copy.pdf');
	cli('protect', path, locked, '--permissions-password', 'owner');
	const domSelection = (browser) => browser.execute(() => document.getSelection()?.toString() ?? '');

	let { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 116));
		await browser.waitUntil(async () => (await domSelection(browser)).startsWith('he quick brown fox'), {
			timeoutMsg: `the page's selection holds ${JSON.stringify(await domSelection(browser))}`
		});
		assert.match(await domSelection(browser), /page 1\.\n+Lorem ipsum/, 'the selection lost its line break');
		assert.equal(await browser.execute(() => document.activeElement?.classList.contains('viewer-scroll')), true);

		// A click away clears it, as it clears the drawn selection.
		await browser.action('pointer').move({ ...(await pagePoint(browser, 0, 450, 40)), origin: 'viewport' }).down().up().perform();
		await browser.waitUntil(async () => (await domSelection(browser)) === '', { timeoutMsg: 'a click away left the selection' });
	} finally {
		await stop();
	}

	({ browser, stop } = await launch([locked]));
	try {
		await waitForDocument(browser);
		await drag(browser, await pagePoint(browser, 0, 40, 92), await pagePoint(browser, 0, 200, 116));
		await browser.waitUntil(async () => (await selected(browser)).length > 0, { timeoutMsg: 'no text selected' });
		await browser.pause(300);
		assert.equal(await domSelection(browser), '', 'a document that forbids copying gave the system its text');
	} finally {
		await stop();
	}
});
