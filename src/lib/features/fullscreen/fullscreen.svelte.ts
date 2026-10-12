// Full screen (section 6.1): the window fills the screen and shows only the pages. This holds
// whether it is on, and what brings the side panes and the toolbar back meanwhile: the
// pointer at a side of the screen, a pane a command asked for, and Alt held down.

import { getCurrentWindow } from '@tauri-apps/api/window';

import type { PaneSide } from '#lib/components/panes.ts';
import { isTextInput } from '#lib/shortcuts.ts';

import { EDGE_DWELL_MS, ENTER_FLASH_MS, PANE_LINGER_MS, PEEK_AFTER_MS, atPane } from './reveal.ts';

const SIDES: readonly PaneSide[] = ['left', 'right'];

type Timer = ReturnType<typeof setTimeout> | undefined;
type KeyPress = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'shiftKey' | 'metaKey' | 'repeat'>;

/** A comment, a name or a number is being typed in the pane of `side`. */
function typingIn(side: PaneSide): boolean {
	if (typeof document === 'undefined') return false;
	const field = document.activeElement;
	return field instanceof HTMLElement && isTextInput(field) && field.closest(`[data-pane="${side}"]`) !== null;
}

class FullScreen {
	on = $state(false);
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

	async enter() {
		if (this.on) return;
		this.#reset();
		this.on = true;
		this.justEntered = true;
		this.#enterTimer = setTimeout(() => (this.justEntered = false), ENTER_FLASH_MS);
		try {
			await getCurrentWindow().setFullscreen(true);
		} catch (e) {
			this.on = false;
			this.#reset();
			throw e;
		}
	}

	async leave() {
		if (!this.on) return;
		this.on = false;
		this.#reset();
		await getCurrentWindow().setFullscreen(false);
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
