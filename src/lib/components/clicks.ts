// Counts presses in a row (double- and triple-clicks) from pointerdown events, which start
// drags and so can't wait for click or dblclick. PointerEvent.detail is always 0 on
// pointerdown (the Pointer Events spec, which WebView2 follows); only click and dblclick
// carry the count.

/** Windows' default double-click time. */
export const MULTI_CLICK_MS = 500;
/** How far the pointer may move between the presses of one double-click, CSS pixels. */
const SLOP_PX = 4;

/** When a key was last pressed: a key between two presses makes them separate clicks
 * (click a bookmark, press F2, type, then drag it: not a double-click). */
let lastKey = -Infinity;

export function keyPressed(timeStamp: number) {
	lastKey = timeStamp;
}

if (typeof window !== 'undefined') {
	window.addEventListener('keydown', (event) => keyPressed(event.timeStamp), true);
}

export class ClickCounter {
	private last = { time: -Infinity, x: 0, y: 0, count: 0 };

	/** 1 for a single press, 2 for the second press of a double-click, and so on. */
	count(event: { timeStamp: number; clientX: number; clientY: number }): number {
		const soon = event.timeStamp - this.last.time <= MULTI_CLICK_MS;
		const near = Math.hypot(event.clientX - this.last.x, event.clientY - this.last.y) <= SLOP_PX;
		const count = soon && near && lastKey < this.last.time ? this.last.count + 1 : 1;
		this.last = { time: event.timeStamp, x: event.clientX, y: event.clientY, count };
		return count;
	}
}
