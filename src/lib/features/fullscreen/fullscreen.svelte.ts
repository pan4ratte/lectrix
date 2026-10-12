// Full screen (section 6.1): the window fills the screen and shows only the pages. This holds
// whether it is on and how it comes and goes, and what brings the side panes and the toolbar
// back meanwhile: the pointer at a side of the screen, a pane a command asked for, and Alt
// held down.

import { getCurrentWindow } from '@tauri-apps/api/window';

import { PANE_MOTION_MS, motionMs, type PaneSide } from '#lib/components/panes.ts';
import { isTextInput } from '#lib/shortcuts.ts';

import { EDGE_DWELL_MS, ENTER_FLASH_MS, PANE_LINGER_MS, PEEK_AFTER_MS, SWITCH_BACK_LEAD_MS, SWITCH_LEAD_MS, SWITCH_WAIT_MS, atPane } from './reveal.ts';

const SIDES: readonly PaneSide[] = ['left', 'right'];

type Timer = ReturnType<typeof setTimeout> | undefined;
type KeyPress = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'shiftKey' | 'metaKey' | 'repeat'>;

/**
 * Where full screen is on its way (the bars are the title bar and the view bar):
 * - `off`: a window like any other.
 * - `arriving`: still that window, with its bars and panes where they were, sliding out of
 *   it; the pages have the whole window already, under them.
 * - `full`: the window fills the screen and shows the pages alone; the panes come over the
 *   pages when something brings them.
 * - `returning`: still the whole screen, with the pages as they were; the bars and panes
 *   are back, laid out as in a window, and slide in over the pages.
 *
 * In `arriving` and `returning` the window is switched as the slide ends.
 */
export type Stage = 'off' | 'arriving' | 'full' | 'returning';

const inBrowser = typeof window !== 'undefined';
const nextFrame = () => (inBrowser ? new Promise<number>((done) => requestAnimationFrame(done)) : Promise.resolve(0));
const pause = (ms: number) => (inBrowser && ms > 0 ? new Promise<void>((done) => setTimeout(done, ms)) : Promise.resolve());

/**
 * Watches the window take its new size: `resized` runs as it does, in the frame that lays
 * the page out at that size, so that what it changes is laid out with it and not a second
 * time; or after `SWITCH_WAIT_MS`, for a window that was that size already. The promise
 * resolves then too.
 */
function windowSwitched(resized: () => void): { done: Promise<void>; cancel: () => void } {
	if (!inBrowser) {
		resized();
		return { done: Promise.resolve(), cancel: () => {} };
	}
	let cancelled = false;
	let finish = () => {};
	const done = new Promise<void>((resolve) => {
		const timer = setTimeout(() => finish(), SWITCH_WAIT_MS);
		finish = () => {
			clearTimeout(timer);
			window.removeEventListener('resize', finish);
			if (!cancelled) resized();
			cancelled = true;
			resolve();
		};
		window.addEventListener('resize', finish);
	});
	return {
		done,
		cancel: () => {
			cancelled = true;
			finish();
		}
	};
}

/** A comment, a name or a number is being typed in the pane of `side`. */
function typingIn(side: PaneSide): boolean {
	if (typeof document === 'undefined') return false;
	const field = document.activeElement;
	return field instanceof HTMLElement && isTextInput(field) && field.closest(`[data-pane="${side}"]`) !== null;
}

class FullScreen {
	stage = $state<Stage>('off');
	/** The pages alone: the bars are gone, and the panes come over the pages on demand. */
	on = $derived(this.stage === 'full');
	/** Full screen is what shows, from the moment the bars start to go until they start to
	 * come back: the toolbar floats, with the page controls in it. */
	started = $derived(this.stage === 'arriving' || this.stage === 'full');
	/** While the bars and panes slide (`arriving`, `returning`): they are out of sight, or
	 * on their way there. */
	chromeAway = $state(false);
	/** The window is on its way into full screen or out of it: what appears or goes with
	 * that does so at once, not with a motion of its own. */
	switching = $state(false);
	/** Alt is held: both panes show, and a toolbar that shows only on demand. */
	peek = $state(false);
	/** Full screen has just started: a toolbar that shows only on demand shows for a moment,
	 * so it is seen where it is. */
	justEntered = $state(false);
	/** The width the pane of a side shows at, null if it holds no panel (set by the app). */
	widthOf: (side: PaneSide) => number | null = () => null;

	/** The pointer is at the pane, or was a moment ago. */
	#hover = $state({ left: false, right: false });
	/** A command asked for the pane: it stays until hidden again or the page is pressed. */
	#pinned = $state({ left: false, right: false });
	/** Hidden under the pointer: the pointer must leave that side before it brings the pane again. */
	#dismissed = { left: false, right: false };
	#at = { left: false, right: false };
	#x = -1;
	#width = 0;
	#dwell: Record<PaneSide, Timer> = { left: undefined, right: undefined };
	#linger: Record<PaneSide, Timer> = { left: undefined, right: undefined };
	#altDown = false;
	#peekTimer: Timer;
	#enterTimer: Timer;

	/** Whether the pane of `side` lies over the pages. */
	shows(side: PaneSide): boolean {
		return this.on && (this.peek || this.#hover[side] || this.#pinned[side]);
	}

	async toggle() {
		await (this.on ? this.leave() : this.enter());
	}

	/** How long before the window is asked to switch: the slide, less the time the window
	 * takes to answer (`lead`). */
	#slide(lead: number): Promise<void> {
		return pause((inBrowser ? motionMs(PANE_MOTION_MS) : 0) - lead);
	}

	/**
	 * Into full screen. The pages take the whole window at once, under the bars and panes,
	 * which slide out of it by transforms alone (the compositor moves them without laying
	 * anything out); the window is switched as they go, and in the frame that brings its new
	 * size everything is laid out for full screen.
	 *
	 * In that order because switching a window to full screen makes the webview lay out and
	 * draw everything again at the new size, which holds its frames up for 40 to 100 ms
	 * (longest the first time). A slide started with the switch was lost in that; one that
	 * waits for it to pass starts 150 ms after the key. Before the switch, nothing is in its
	 * way, and it answers the key at once.
	 */
	async enter() {
		if (this.stage !== 'off' || this.switching) return;
		this.switching = true;
		this.#reset();
		this.chromeAway = true;
		this.stage = 'arriving';
		this.justEntered = true;
		this.#enterTimer = setTimeout(() => (this.justEntered = false), ENTER_FLASH_MS);
		await this.#slide(SWITCH_LEAD_MS);
		const switched = windowSwitched(() => (this.stage = 'full'));
		try {
			await getCurrentWindow().setFullscreen(true);
			await switched.done;
		} catch (e) {
			switched.cancel();
			this.stage = 'off';
			this.chromeAway = false;
			this.#reset();
			throw e;
		} finally {
			this.switching = false;
		}
	}

	/**
	 * Out of full screen, the same way round: the bars and panes come back, laid out as in a
	 * window but over the pages, which stay as they are, and slide in; the window is switched
	 * as they arrive, and in the frame that brings its size the pages get their place in it.
	 */
	async leave() {
		if (this.stage !== 'full' || this.switching) return;
		this.switching = true;
		this.#reset();
		this.chromeAway = true;
		this.stage = 'returning';
		// Drawn once out of sight: from there they slide.
		await nextFrame();
		await nextFrame();
		this.chromeAway = false;
		await this.#slide(SWITCH_BACK_LEAD_MS);
		const back = () => {
			this.stage = 'off';
			this.chromeAway = false;
		};
		const switched = windowSwitched(back);
		try {
			await getCurrentWindow().setFullscreen(false);
			await switched.done;
		} finally {
			// Whatever the window did, Lectrix is laid out as a window again.
			switched.cancel();
			if ((this.stage as Stage) !== 'off') back();
			this.switching = false;
		}
	}

	#reset() {
		for (const side of SIDES) {
			clearTimeout(this.#dwell[side]);
			clearTimeout(this.#linger[side]);
			this.#dwell[side] = this.#linger[side] = undefined;
			this.#hover[side] = this.#pinned[side] = false;
			this.#dismissed[side] = this.#at[side] = false;
		}
		clearTimeout(this.#peekTimer);
		clearTimeout(this.#enterTimer);
		this.#peekTimer = this.#enterTimer = undefined;
		this.#altDown = false;
		// Where the pointer is isn't known until it moves.
		this.#x = -1;
		this.peek = false;
		this.justEntered = false;
	}

	// ----- the side panes -----

	/** Shows a pane until it is hidden again or the page is pressed, or hides it. */
	pin(side: PaneSide, shown: boolean) {
		this.#pinned[side] = shown;
		if (shown) return;
		clearTimeout(this.#dwell[side]);
		clearTimeout(this.#linger[side]);
		this.#dwell[side] = this.#linger[side] = undefined;
		this.#hover[side] = false;
		this.#dismissed[side] = this.#at[side];
	}

	/** The pointer moved to `x` in a window `width` wide, with `buttons` down. */
	pointerMoved(x: number, buttons: number, width: number) {
		if (!this.on) return;
		this.#x = x;
		this.#width = width;
		// A drag (selecting text, drawing, the scrollbar, a pane's edge) neither brings a pane
		// nor lets one go.
		if (buttons !== 0) {
			for (const side of SIDES) this.#stopDwell(side);
			return;
		}
		for (const side of SIDES) {
			const shown = this.shows(side);
			const at = atPane(side, x, width, shown ? this.widthOf(side) : null);
			this.#at[side] = at;
			if (!at) {
				this.#dismissed[side] = false;
				this.#stopDwell(side);
				if (this.#hover[side]) this.#letGo(side);
			} else if (!this.#dismissed[side]) {
				clearTimeout(this.#linger[side]);
				this.#linger[side] = undefined;
				// Over a pane that shows, whatever brought it: it stays while the pointer does.
				if (shown) this.#hover[side] = true;
				else if (this.#dwell[side] === undefined) {
					this.#dwell[side] = setTimeout(() => {
						this.#dwell[side] = undefined;
						this.#hover[side] = true;
					}, EDGE_DWELL_MS);
				}
			}
		}
	}

	/** The pointer left the window (to another monitor). */
	pointerLeft() {
		if (!this.on) return;
		this.#x = -1;
		for (const side of SIDES) {
			this.#at[side] = false;
			this.#dismissed[side] = false;
			this.#stopDwell(side);
			if (this.#hover[side]) this.#letGo(side);
		}
	}

	/** The page was pressed: the panes a command brought go, and so does what Alt shows (the
	 * press may start an area highlight, which is drawn with Alt held). */
	pagePressed() {
		if (!this.on) return;
		this.#endPeek();
		for (const side of SIDES) {
			if (!this.#pinned[side]) continue;
			this.#keepWhileTyping(side);
			this.#pinned[side] = false;
		}
	}

	#stopDwell(side: PaneSide) {
		clearTimeout(this.#dwell[side]);
		this.#dwell[side] = undefined;
	}

	/** Hides the pane once the pointer has been away for a moment; not from under a field
	 * being typed in, which commits what it holds when it is left. */
	#letGo(side: PaneSide) {
		if (this.#linger[side] !== undefined) return;
		this.#linger[side] = setTimeout(() => {
			this.#linger[side] = undefined;
			if (typingIn(side)) this.#letGo(side);
			else this.#hover[side] = false;
		}, PANE_LINGER_MS);
	}

	/** What showed the pane is over: it stays for the pointer on it, or the field typed in. */
	#keepWhileTyping(side: PaneSide) {
		if (this.#hover[side]) return;
		const under = this.#x >= 0 && !this.#dismissed[side] && atPane(side, this.#x, this.#width, this.widthOf(side));
		if (under) this.#hover[side] = true;
		else if (typingIn(side)) {
			this.#hover[side] = true;
			this.#letGo(side);
		}
	}

	// ----- Alt held -----

	/** A key went down. True for Alt by itself, which the caller keeps from the system (it
	 * would open the window's menu). */
	keyDown(event: KeyPress): boolean {
		if (!this.on) return false;
		if (event.key !== 'Alt') {
			// Alt with another key is a shortcut (Alt+Left), not a hold.
			if (this.#altDown) this.#endPeek();
			return false;
		}
		if (!this.#altDown && !event.repeat && !event.ctrlKey && !event.shiftKey && !event.metaKey) {
			this.#altDown = true;
			this.#peekTimer = setTimeout(() => {
				this.#peekTimer = undefined;
				this.peek = true;
			}, PEEK_AFTER_MS);
		}
		return true;
	}

	/** A key went up. True for Alt, as `keyDown`. */
	keyUp(event: Pick<KeyboardEvent, 'key'>): boolean {
		if (!this.on || event.key !== 'Alt') return false;
		this.#altDown = false;
		this.#endPeek();
		return true;
	}

	/** The window lost the focus (Alt+Tab): Alt's release will not be seen. */
	blurred() {
		this.#altDown = false;
		this.#endPeek();
	}

	/** Ends what Alt shows; holding it on shows nothing more until it is pressed again. */
	#endPeek() {
		clearTimeout(this.#peekTimer);
		this.#peekTimer = undefined;
		if (!this.peek) return;
		for (const side of SIDES) if (!this.#pinned[side]) this.#keepWhileTyping(side);
		this.peek = false;
	}
}

export const fullscreen = new FullScreen();
