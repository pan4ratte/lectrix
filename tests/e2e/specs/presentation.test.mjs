// Presentation (AGENTS.md section 6.1): F5 shows the pages one at a time, each as large as
// fits the screen, on black, from the current page. Keys, clicks and the controls turn them;
// B and W blank the screen; Esc ends it on the page it reached, and the window returns as
// it was, or stays full screen when it was.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';

import { OUT, ROOT, launch, waitForDocument } from '../lib/app.mjs';
import { cli, sample } from '../lib/pdfcli.mjs';

/** The presentation as it shows: its stage, the page shown, whether that page's pixels are
 * drawn at the screen's resolution, a blank screen, and the window. */
function look(browser) {
	return browser.execute(() => {
		const overlay = document.querySelector('[data-presentation]');
		const page = overlay?.querySelector('[data-presentation-page][data-shown]');
		const canvas = page?.querySelector('canvas');
		const box = page?.getBoundingClientRect();
		const controls = overlay?.querySelector('.presentation-controls');
		const bar = controls?.getBoundingClientRect();
		return {
			stage: overlay?.getAttribute('data-presentation') ?? null,
			page: page ? Number(page.getAttribute('data-presentation-page')) : null,
			// One image pixel to a device pixel, whichever way the page is as wide as it fits.
			sharp: canvas && box ? Math.abs(canvas.width - Math.round(box.width * devicePixelRatio)) <= 1 : false,
			fits: box ? Math.round(box.width) >= innerWidth - 1 || Math.round(box.height) >= innerHeight - 1 : false,
			blank: overlay?.querySelector('[data-presentation-blank]')?.getAttribute('data-presentation-blank') ?? null,
			controls: controls ? !controls.classList.contains('presentation-controls-hidden') : false,
			barMiddle: bar ? bar.left + bar.width / 2 : null,
			position: overlay?.querySelector('[data-presentation-position]')?.textContent?.trim() ?? null,
			fillsScreen: innerWidth >= screen.width - 1 && innerHeight >= screen.height - 1,
			viewerPage: document.querySelector('[data-page-position]')?.textContent?.trim() ?? null,
			size: [innerWidth, innerHeight]
		};
	});
}

async function until(browser, what, check) {
	let seen;
	await browser
		.waitUntil(async () => check((seen = await look(browser))), { timeout: 8000 })
		.catch(() => {
			throw new Error(`${what}: ${JSON.stringify(seen)}`);
		});
	return seen;
}

const presenting = (page) => (s) => s.stage === 'on' && s.page === page && s.sharp && s.fits && s.fillsScreen;

async function press(browser, keys, page, what) {
	await browser.keys(keys);
	return until(browser, what, (s) => s.stage === 'on' && s.page === page && !s.blank);
}

test('a presentation shows the pages one at a time, as large as fits the screen', async () => {
	// A window smaller than the screen, so its return is seen.
	const state = join(OUT, 'presentation-window.json');
	mkdirSync(OUT, { recursive: true });
	rmSync(state, { force: true });
	const { version } = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
	writeFileSync(state, JSON.stringify({ last_version: version, window: { bounds: { x: 60, y: 40, width: 900, height: 700 }, maximized: false } }));
	const { browser, stop } = await launch([sample('presentation.pdf', 6)], { env: { LECTRIX_STATE_FILE: state } });
	try {
		await waitForDocument(browser);
		const before = await look(browser);
		assert.equal(before.fillsScreen, false);

		// From the current page: the second.
		await browser.keys(['Control', 'g']);
		await browser.keys(['2', 'Enter']);
		await until(browser, 'the page box did not go to page 2', (s) => s.viewerPage?.includes('2 of 6'));
		await browser.keys(['F5']);
		await until(browser, 'F5 did not present page 2 on the whole screen', presenting(1));

		// Keys turn the pages.
		await press(browser, ['ArrowRight'], 2, 'Right did not turn to page 3');
		await press(browser, [' '], 3, 'Space did not turn to page 4');
		await press(browser, ['Backspace'], 2, 'Backspace did not turn back to page 3');
		await press(browser, ['PageUp'], 1, 'Page Up did not turn back to page 2');
		await press(browser, ['End'], 5, 'End did not go to the last page');
		await press(browser, ['ArrowRight'], 5, 'Right went past the last page');
		await press(browser, ['Home'], 0, 'Home did not go to the first page');
		// A page number, typed and entered.
		await browser.keys(['5']);
		await until(browser, 'the digit typed did not show', (s) => s.controls && s.position === 'Page 5');
		await press(browser, ['Enter'], 4, 'typing 5 and Enter did not go to page 5');
		await until(browser, 'every page did not render sharp for the screen', presenting(4));

		// A click turns on, a right click back.
		const [w, h] = (await look(browser)).size;
		const at = { x: Math.round(w / 2), y: Math.round(h / 2), origin: 'viewport' };
		await browser.action('pointer').move(at).down().up().perform();
		await until(browser, 'a click did not turn to page 6', (s) => s.page === 5);
		await browser.action('pointer').move(at).down({ button: 2 }).up({ button: 2 }).perform();
		await until(browser, 'a right click did not turn back to page 5', (s) => s.page === 4);

		// The pointer moved, the controls show; their buttons turn the pages too.
		const withBar = await until(browser, 'the controls did not show with the pointer moving', (s) => s.controls && s.position === '5 of 6');
		assert.ok(Math.abs(withBar.barMiddle - withBar.size[0] / 2) <= 1, `the controls are not in the middle: ${withBar.barMiddle}`);
		await (await browser.$('[data-presentation] button[aria-label="Previous page"]')).click();
		await until(browser, 'Previous page did not turn back to page 4', (s) => s.page === 3);
		// At rest, away from them, the pointer and the controls hide.
		await browser.action('pointer').move(at).perform();
		await until(browser, 'the controls stayed with the pointer at rest', (s) => !s.controls);

		// B blanks the screen black, and a page key brings it back on the same page; W
		// blanks it white, and Esc brings it back without ending.
		await browser.keys(['b']);
		await until(browser, 'B did not blank the screen', (s) => s.blank === 'black');
		await press(browser, ['ArrowRight'], 3, 'Right did not bring the page back where it was');
		await browser.keys(['w']);
		await until(browser, 'W did not whiten the screen', (s) => s.blank === 'white');
		await press(browser, ['Escape'], 3, 'Esc did not bring the page back');

		// Esc ends it on the page reached, and the window comes back as it was.
		await browser.keys(['Escape']);
		const back = await until(browser, 'Esc did not end the presentation', (s) => s.stage === null && !s.fillsScreen);
		assert.deepEqual(back.size, before.size, 'the window has its size back');
		await until(browser, 'the document did not stay at page 4', (s) => s.viewerPage?.includes('4 of 6'));

		// Started full screen, it ends full screen.
		await browser.keys(['F11']);
		await until(browser, 'F11 did not fill the screen', (s) => s.fillsScreen);
		await browser.keys(['F5']);
		await until(browser, 'F5 did not present from full screen', presenting(3));
		await browser.keys(['F5']);
		await until(browser, 'F5 did not end the presentation', (s) => s.stage === null);
		await browser.pause(500);
		assert.equal((await look(browser)).fillsScreen, true, 'full screen ended with the presentation');
		await browser.keys(['F11']);
		await until(browser, 'F11 did not leave full screen', (s) => !s.fillsScreen);
	} finally {
		await stop();
	}
});

test('a presentation plays the transitions and display times the document gives its pages', async () => {
	// The second page fades in over 0.8 s; the third shows for 1 s, then the fourth comes.
	const path = join(OUT, 'presentation-timing.pdf');
	rmSync(path, { force: true });
	cli('gen', path, '--pages', '5', '--transition', '1:Fade:0.8', '--duration', '2:1');
	const { browser, stop } = await launch([path]);
	try {
		await waitForDocument(browser);
		await browser.keys(['F5']);
		await until(browser, 'F5 did not present the first page', presenting(0));
		const reduced = await browser.execute(() => matchMedia('(prefers-reduced-motion: reduce)').matches);

		// Each frame for a while: the opacity of the second page, and whether the first shows.
		await browser.execute(() => {
			const frames = [];
			window.__frames = frames;
			const start = performance.now();
			const sample = () => {
				const coming = document.querySelector('[data-presentation-page="1"]');
				const going = document.querySelector('[data-presentation-page="0"]');
				frames.push({
					at: performance.now() - start,
					coming: coming && !coming.classList.contains('invisible') ? Number(getComputedStyle(coming).opacity) : null,
					going: going ? !going.classList.contains('invisible') : false
				});
				if (performance.now() - start < 3000) requestAnimationFrame(sample);
			};
			requestAnimationFrame(sample);
		});
		await browser.keys(['ArrowRight']);
		await until(browser, 'Right did not turn to page 2', (s) => s.page === 1);
		await browser.pause(3100);
		const frames = await browser.execute(() => window.__frames);
		const fading = frames.filter((f) => f.coming !== null && f.coming > 0.05 && f.coming < 0.95 && f.going);
		if (reduced) assert.equal(fading.length, 0, 'with reduced motion, the page came at once');
		else {
			assert.ok(fading.length >= 10, `the page did not fade in over the one before: ${JSON.stringify(frames.slice(0, 40))}`);
			const span = fading.at(-1).at - fading[0].at;
			assert.ok(span > 500 && span < 900, `the fade took ${span} ms, not about 800`);
		}
		const done = frames.at(-1);
		assert.deepEqual([done.coming, done.going], [1, false], `the fade did not end with the new page alone: ${JSON.stringify(frames.slice(-5))}`);

		// The third page shows for a second, then the fourth comes by itself.
		await press(browser, ['ArrowRight'], 2, 'Right did not turn to page 3');
		const turnedAt = Date.now();
		await until(browser, 'the fourth page did not come when the time of the third was up', (s) => s.page === 3);
		const waited = Date.now() - turnedAt;
		assert.ok(waited >= 900, `the fourth page came after ${waited} ms, before the third page's second`);
		// The fourth has no time of its own: it stays.
		await browser.pause(1500);
		assert.equal((await look(browser)).page, 3, 'a page without a time turned by itself');
		await browser.keys(['Escape']);
		await until(browser, 'Esc did not end the presentation', (s) => s.stage === null);
	} finally {
		await stop();
	}
});

test('with a second screen, the pages show there and this window shows the presenter view', async () => {
	// A note on the second page, for the presenter's notes. LECTRIX_AUDIENCE_ANY_SCREEN puts
	// the pages' window on this one screen, over the presenter view.
	const plain = sample('presenter-plain.pdf', 4);
	const path = join(OUT, 'presenter.pdf');
	rmSync(path, { force: true });
	cli('annot', 'note', plain, path, '--page', '2', '--at', '100,100', '--text', 'Say hello to the room');
	const { browser, stop } = await launch([path], { env: { LECTRIX_AUDIENCE_ANY_SCREEN: '1' } });
	try {
		await waitForDocument(browser);
		const main = await browser.getWindowHandle();
		const inMain = (script) => browser.switchToWindow(main).then(() => browser.execute(script));
		await browser.keys(['F5']);
		await browser.waitUntil(
			() => browser.execute(() => document.querySelector('[data-presentation="on"][data-presenter-view]') !== null),
			{ timeoutMsg: 'F5 did not show the presenter view' }
		);

		// The audience screen: a window of its own, with the first page.
		let audience = null;
		await browser.waitUntil(
			async () => {
				for (const handle of await browser.getWindowHandles()) {
					if (handle === main) continue;
					await browser.switchToWindow(handle);
					if (await browser.execute(() => document.querySelector('[data-audience]') !== null)) {
						audience = handle;
						return true;
					}
				}
				await browser.switchToWindow(main);
				return false;
			},
			{ timeout: 15_000, timeoutMsg: 'no audience screen' }
		);
		const onAudience = async (what, check) => {
			await browser.switchToWindow(audience);
			await browser.waitUntil(() => browser.execute(check), { timeoutMsg: what });
		};
		await onAudience('the audience screen did not show the first page', () =>
			document.querySelector('[data-audience] [data-shown]')?.getAttribute('data-presentation-page') === '0'
		);

		// A key in the presenter view turns the audience's page; the presenter view shows the
		// page's note and the next page.
		await browser.switchToWindow(main);
		await browser.keys(['ArrowRight']);
		await onAudience('Right in the presenter view did not turn the audience screen', () =>
			document.querySelector('[data-audience] [data-shown]')?.getAttribute('data-presentation-page') === '1'
		);
		await browser.switchToWindow(main);
		await browser.waitUntil(
			() =>
				browser.execute(
					() =>
						document.querySelector('[data-presenter-notes]')?.textContent?.includes('Say hello to the room') &&
						document.querySelector('[data-presenter-next] [data-shown]')?.getAttribute('data-presentation-page') === '2' &&
						document.querySelector('[data-presenter-current] [data-shown]')?.getAttribute('data-presentation-page') === '1'
				),
			{ timeoutMsg: 'the presenter view did not show the page, its note and the next page' }
		);
		const timer = await inMain(() => document.querySelector('[data-presenter-timer]')?.textContent?.trim());
		assert.match(timer, /^\d\d:\d\d$/);

		// A click on the audience screen turns on; B there blanks it, and the presenter view
		// says so.
		await browser.switchToWindow(audience);
		const [w, h] = await browser.execute(() => [innerWidth, innerHeight]);
		await browser.action('pointer').move({ x: Math.round(w / 2), y: Math.round(h / 2), origin: 'viewport' }).down().up().perform();
		await onAudience('a click on the audience screen did not turn the page', () =>
			document.querySelector('[data-audience] [data-shown]')?.getAttribute('data-presentation-page') === '2'
		);
		await browser.keys(['b']);
		await onAudience('B on the audience screen did not blank it', () => document.querySelector('[data-audience] [data-presentation-blank="black"]') !== null);
		await browser.switchToWindow(main);
		await browser.waitUntil(() => browser.execute(() => document.querySelector('[data-presenter-blank]') !== null), {
			timeoutMsg: 'the presenter view did not say the screen is black'
		});
		await browser.keys(['b']);
		await onAudience('B again did not bring the page back', () => document.querySelector('[data-audience] [data-presentation-blank]') === null);

		// Ended in the presenter view: the audience screen goes, at the page reached.
		await browser.switchToWindow(main);
		await (await browser.$('[data-presenter-view] button[aria-label="End presentation"]')).click();
		await browser.waitUntil(async () => (await browser.getWindowHandles()).length === 1, { timeoutMsg: 'the audience screen stayed' });
		await browser.switchToWindow(main);
		await until(browser, 'the presentation did not end at page 3', (s) => s.stage === null && s.viewerPage?.includes('3 of 4'));
	} finally {
		await stop();
	}
});
