// Presentation (section 6.1): the document's pages one at a time, each as large as fits the
// screen, on black, turned by keys, clicks, the wheel and swipes, with the transitions and
// display times the document gives its pages. The window fills the screen for it, unless
// full screen has filled it already, and returns as it was. With a second screen (and
// Settings allowing), the pages show there, and this window shows the presenter view.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { motionMs } from '#lib/components/panes.ts';
import { windowSwitched } from '#lib/features/fullscreen/fullscreen.svelte.ts';
import { resolvePageInput } from '#lib/features/viewer/pagebox.ts';
import {
	closeAudienceScreen,
	getPagePresentation,
	logError,
	openAudienceScreen,
	swapPresentationScreens,
	type PagePresentation
} from '#lib/ipc/index.ts';
import { app } from '#lib/stores/app.svelte.ts';
import type { DocTab } from '#lib/stores/doc.svelte.ts';

import type { Action, Blank } from './input.ts';
import type { StageChange } from './stage.ts';

/** How long the screen takes to go black, and to come back from it. */
export const FADE_MS = 100;
/** How long resizes are let pass after the screens are swapped, while the windows move. */
const SWAP_SETTLE_MS = 600;

/**
 * - `off`: no presentation.
 * - `arriving`: the screen goes black, and the window is switched to fill the screen.
 * - `on`: the pages show.
 * - `leaving`: still black, while the window returns to what it was.
 */
export type PresentationStage = 'off' | 'arriving' | 'on' | 'leaving';

const inBrowser = typeof window !== 'undefined';
const nextFrame = () => (inBrowser ? new Promise<number>((done) => requestAnimationFrame(done)) : Promise.resolve(0));
const pause = (ms: number) => (inBrowser && ms > 0 ? new Promise<void>((done) => setTimeout(done, ms)) : Promise.resolve());

class Presentation {
	stage = $state<PresentationStage>('off');
	/** The document presented; kept after the presentation, which still shows it as it fades
	 * out, until the next one. */
	tab = $state.raw<DocTab | null>(null);
	/** The page shown, 0-based. */
	page = $state(0);
	/** The screen is all black or all white, the page hidden (B or W). */
	blank = $state<Blank | null>(null);
	/** The digits of a page number typed so far, which Enter goes to. */
	typed = $state('');
	/** The change of page that brought the page shown, with its transition. */
	change = $state.raw<StageChange>({ seq: 0, from: null, transition: null });
	/** The pages show on another screen, and this window shows the presenter view. */
	audience = $state(false);
	/** When the presenter view's timer started, less the time it was paused, and when it
	 * was paused (null while it runs); `Date.now()` milliseconds. */
	timerStart = $state(0);
	timerPaused = $state<number | null>(null);
	on = $derived(this.stage !== 'off');
	/** The pages show (and before: they are rendered meanwhile, out of sight). */
	shown = $derived(this.stage === 'on');

	/** What each page asks of the presentation (`/Trans`, `/Dur`). */
	#pages: PagePresentation[] = [];
	/** The window was switched to fill the screen for the presentation, and is switched back
	 * after it; false when it filled the screen already (full screen). */
	#switched = false;
	/** Asked to end while the window was switching: it ends once the switch is done. */
	#stopAsked = false;
	#swapping = false;
	/** Turns to the next page when the page's display time is up. */
	#timed: ReturnType<typeof setTimeout> | undefined;

	/** Starts presenting `tab` from its current page. */
	async start(tab: DocTab) {
		if (this.stage !== 'off' || tab.pageCount === 0) return;
		this.tab = tab;
		this.page = Math.min(tab.currentPage, tab.pageCount - 1);
		this.blank = null;
		this.typed = '';
		this.change = { seq: this.change.seq + 1, from: null, transition: null };
		this.#stopAsked = false;
		this.stage = 'arriving';
		const window = getCurrentWindow();
		const [full, pages] = await Promise.all([
			window.isFullscreen().catch(() => false),
			getPagePresentation(tab.id).catch((e: unknown) => {
				void logError(`presentation of document ${tab.id}: ${String(e)}`);
				return [];
			})
		]);
		this.#pages = pages;
		this.#switched = !full;
		// Before this window fills its screen, so the other screen is told apart from it.
		if (app.settings?.presenterView !== false) {
			this.audience = await openAudienceScreen().catch((e: unknown) => {
				void logError(`audience screen: ${String(e)}`);
				return false;
			});
		}
		// Black first: the window lays everything out again as it switches, unseen.
		await pause(motionMs(FADE_MS));
		if (!full) {
			const switched = windowSwitched(() => {});
			try {
				await window.setFullscreen(true);
				await switched.done;
			} catch (e) {
				switched.cancel();
				if (this.audience) await closeAudienceScreen(false).catch(() => {});
				this.#reset();
				throw e;
			}
		}
		this.stage = 'on';
		this.resetTimer();
		this.#schedule();
		if (this.#stopAsked) await this.stop();
	}

	/** Ends the presentation; the document is left at the page it ended on. */
	async stop() {
		if (this.stage === 'arriving') {
			this.#stopAsked = true;
			return;
		}
		if (this.stage !== 'on') return;
		const tab = this.tab;
		const page = this.page;
		this.stage = 'leaving';
		clearTimeout(this.#timed);
		// Under the black screen, before the window changes size: the viewer keeps its place
		// through that.
		if (tab && page !== tab.currentPage && page < tab.pageCount) tab.viewer?.goTo({ page, offset: 0 }, { recordHistory: true });
		if (this.audience) {
			// Back on its own screen, if the screens were swapped.
			await closeAudienceScreen(!this.#switched).catch(() => {});
			this.audience = false;
		}
		if (this.#switched) {
			const switched = windowSwitched(() => {});
			try {
				await getCurrentWindow().setFullscreen(false);
				await switched.done;
			} catch {
				switched.cancel();
			}
		}
		// Again once the window is back and laid out: the zoom may fit it anew, keeping the
		// middle of the view in place, which can bring the page before to the top.
		if (tab && page < tab.pageCount) {
			await nextFrame();
			await nextFrame();
			if (tab.currentPage !== page) tab.viewer?.goTo({ page, offset: 0 }, { recordHistory: false });
		}
		this.#reset();
		tab?.viewer?.focus();
	}

	async toggle(tab: DocTab | null) {
		if (this.on) await this.stop();
		else if (tab) await this.start(tab);
	}

	/**
	 * The window changed size. If it no longer fills the screen (left full screen by itself:
	 * on macOS, by the button in the bar the system shows at the top of the screen), the
	 * presentation ends.
	 */
	async windowResized() {
		if (this.stage !== 'on' || this.#swapping) return;
		const full = await getCurrentWindow()
			.isFullscreen()
			.catch(() => true);
		if (full || this.stage !== 'on' || this.#swapping) return;
		this.#switched = false;
		await this.stop();
	}

	/** Swaps the screens of the pages and of the presenter view. */
	async swapScreens() {
		if (!this.audience || this.stage !== 'on' || this.#swapping) return;
		this.#swapping = true;
		try {
			await swapPresentationScreens();
			await pause(SWAP_SETTLE_MS);
		} finally {
			this.#swapping = false;
		}
	}

	/** What page `index` asks of the presentation. */
	pageInfo(index: number): PagePresentation | null {
		return this.#pages[index] ?? null;
	}

	/** Shows page `page` (0-based), within the document; the screen comes back from black
	 * or white. */
	goTo(page: number) {
		const count = this.tab?.pageCount ?? 0;
		if (count === 0) return;
		const to = Math.min(count - 1, Math.max(0, page));
		this.blank = null;
		this.typed = '';
		if (to !== this.page) {
			const transition = app.settings?.presentationTransitions === false ? null : (this.#pages[to]?.transition ?? null);
			this.change = { seq: this.change.seq + 1, from: this.page, transition };
			this.page = to;
		}
		this.#schedule();
	}

	/** One page on (1) or back (-1). A blank screen comes back first, on the same page. */
	step(direction: -1 | 1) {
		this.typed = '';
		if (this.blank) {
			this.blank = null;
			this.#schedule();
			return;
		}
		this.goTo(this.page + direction);
	}

	/** Blanks the screen black or white, or brings the page back. */
	setBlank(blank: Blank | null) {
		this.typed = '';
		this.blank = blank;
		this.#schedule();
	}

	/** Does what a key asks (`actionFor`). */
	act(action: Action) {
		const tab = this.tab;
		if (!tab) return;
		switch (action.kind) {
			case 'next':
				return this.step(1);
			case 'previous':
				return this.step(-1);
			case 'first':
				return this.goTo(0);
			case 'last':
				return this.goTo(tab.pageCount - 1);
			case 'digit':
				// No page number is that long; what more is typed is a mistake.
				if (this.typed.length < 6) this.typed += action.digit;
				return;
			case 'enter': {
				if (!this.typed) return this.step(1);
				const page = resolvePageInput(this.typed, tab.displayLabels, tab.pageCount);
				this.typed = '';
				if (page !== null) this.goTo(page);
				return;
			}
			case 'erase':
				if (this.typed) this.typed = this.typed.slice(0, -1);
				else this.step(-1);
				return;
			case 'blank':
				return this.setBlank(this.blank === action.blank ? null : action.blank);
			case 'escape':
				if (this.typed) this.typed = '';
				else if (this.blank) this.setBlank(null);
				else void this.stop();
				return;
		}
	}

	// ----- the presenter view's timer -----

	/** How long the presentation has run, in milliseconds, at `now`. */
	elapsed(now: number): number {
		return Math.max(0, (this.timerPaused ?? now) - this.timerStart);
	}

	toggleTimer() {
		const now = Date.now();
		if (this.timerPaused === null) this.timerPaused = now;
		else {
			this.timerStart += now - this.timerPaused;
			this.timerPaused = null;
		}
	}

	resetTimer() {
		this.timerStart = Date.now();
		this.timerPaused = null;
	}

	/**
	 * The next page comes by itself when the page's display time (`/Dur`) is up, counted
	 * once its transition has played; not while the screen is blank, nor from the last page.
	 */
	#schedule() {
		clearTimeout(this.#timed);
		this.#timed = undefined;
		const tab = this.tab;
		if (this.stage !== 'on' || !tab || this.blank || app.settings?.presentationTiming === false) return;
		const seconds = this.#pages[this.page]?.seconds;
		if (!seconds || this.page >= tab.pageCount - 1) return;
		const transition = this.change.transition;
		const lead = transition ? motionMs(transition.seconds * 1000) : 0;
		this.#timed = setTimeout(() => {
			this.#timed = undefined;
			if (this.stage === 'on') this.goTo(this.page + 1);
		}, lead + seconds * 1000);
	}

	#reset() {
		clearTimeout(this.#timed);
		this.#timed = undefined;
		this.stage = 'off';
		this.blank = null;
		this.typed = '';
		this.audience = false;
		this.#pages = [];
		this.#switched = false;
		this.#stopAsked = false;
	}
}

export const presentation = new Presentation();
