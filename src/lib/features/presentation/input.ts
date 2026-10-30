// Presentation (section 6.1): what a key, the wheel or a swipe asks for, and where a page
// lies on the screen. Plain functions, so they are tested without the app.

import { turnedSize, CSS_PX_PER_PT, type Rotation, type Size } from '#lib/features/viewer/layout.ts';
import { clampZoom } from '#lib/features/viewer/zoom.ts';

export type Blank = 'black' | 'white';

export type Action =
	| { kind: 'next' }
	| { kind: 'previous' }
	| { kind: 'first' }
	| { kind: 'last' }
	/** A digit of a page number being typed. */
	| { kind: 'digit'; digit: string }
	/** Enter: goes to the page typed, or to the next one. */
	| { kind: 'enter' }
	/** Backspace: takes back a digit typed, or goes to the previous page. */
	| { kind: 'erase' }
	| { kind: 'blank'; blank: Blank }
	/** Esc: one step back (the digits typed, a blank screen), then the presentation ends. */
	| { kind: 'escape' };

type KeyPress = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'altKey' | 'metaKey' | 'shiftKey'>;

/** What a key asks of the presentation; null for one it doesn't answer. */
export function actionFor(event: KeyPress): Action | null {
	if (event.ctrlKey || event.altKey || event.metaKey) return null;
	const { key } = event;
	if (/^[0-9]$/.test(key)) return { kind: 'digit', digit: key };
	switch (key) {
		case 'ArrowRight':
		case 'ArrowDown':
		case 'PageDown':
		case 'n':
		case 'N':
			return { kind: 'next' };
		case ' ':
			return { kind: event.shiftKey ? 'previous' : 'next' };
		case 'ArrowLeft':
		case 'ArrowUp':
		case 'PageUp':
		case 'p':
		case 'P':
			return { kind: 'previous' };
		case 'Home':
			return { kind: 'first' };
		case 'End':
			return { kind: 'last' };
		case 'Enter':
			return { kind: 'enter' };
		case 'Backspace':
			return { kind: 'erase' };
		case 'b':
		case 'B':
		case '.':
			return { kind: 'blank', blank: 'black' };
		case 'w':
		case 'W':
		case ',':
			return { kind: 'blank', blank: 'white' };
		case 'Escape':
			return { kind: 'escape' };
		default:
			return null;
	}
}

/** Wheel events further apart than this start a new gesture. */
export const GESTURE_GAP_MS = 250;
/** How far the wheel goes to turn a page, CSS pixels: one notch of a mouse wheel. */
export const WHEEL_STEP_PX = 50;

/**
 * Turns the wheel into pages: each notch of a mouse wheel turns one, and a touchpad
 * swipe one in all, however long it and the scrolling that carries on after it last.
 */
export class WheelSteps {
	#total = 0;
	#last = -Infinity;
	#turned = false;

	/** A wheel event: -1 for the previous page, 1 for the next, 0 for neither. */
	push(deltaY: number, deltaMode: number, time: number): -1 | 0 | 1 {
		const px = deltaMode === 1 ? deltaY * 33 : deltaMode === 2 ? deltaY * 800 : deltaY;
		if (time - this.#last > GESTURE_GAP_MS) {
			this.#total = 0;
			this.#turned = false;
		}
		this.#last = time;
		const notch = Math.abs(px) >= WHEEL_STEP_PX;
		// The rest of a touchpad swipe that has turned its page.
		if (this.#turned && !notch) return 0;
		this.#total += px;
		if (Math.abs(this.#total) < WHEEL_STEP_PX) return 0;
		const step = this.#total > 0 ? 1 : -1;
		this.#total = 0;
		this.#turned = !notch;
		return step;
	}
}

/** How far a finger moves sideways to turn a page, CSS pixels. */
export const SWIPE_PX = 60;

/** A touch from `dx`, `dy` (CSS pixels, where it ended less where it began): -1 for a
 * swipe to the right (the previous page), 1 to the left (the next), 0 for no swipe. */
export function swipeStep(dx: number, dy: number): -1 | 0 | 1 {
	if (Math.abs(dx) < SWIPE_PX || Math.abs(dx) < Math.abs(dy)) return 0;
	return dx < 0 ? 1 : -1;
}

export interface Placed {
	/** Pages are shown at this zoom (1 is 100%). */
	zoom: number;
	/** Where the page, as it is turned, lies on the screen, CSS pixels. */
	left: number;
	top: number;
	width: number;
	height: number;
}

/** A page of `size` (points), turned by `rotation`, as large as fits a screen `width` by
 * `height` (CSS pixels), in its middle. */
export function placePage(size: Size, rotation: Rotation, width: number, height: number): Placed {
	const turned = turnedSize(size, rotation);
	const zoom = clampZoom(Math.min(width / (turned.width * CSS_PX_PER_PT), height / (turned.height * CSS_PX_PER_PT)));
	const w = turned.width * CSS_PX_PER_PT * zoom;
	const h = turned.height * CSS_PX_PER_PT * zoom;
	return { zoom, left: (width - w) / 2, top: (height - h) / 2, width: w, height: h };
}
