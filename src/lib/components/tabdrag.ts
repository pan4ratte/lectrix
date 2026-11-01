// Dragging a document tab (section 8): along its row it follows the pointer and the other
// tabs make way; pulled off the row, it is on its way to another window. The geometry is
// here, apart from the component, so it can be tested.

/** Pointer travel before a press on a tab becomes a drag. */
export const DRAG_THRESHOLD = 6;

/** How far above or below the tabs' row the pointer goes before the tab comes off it. */
export const TEAR_OFF = 24;

/** A tab's or the row's extent along the row, in CSS pixels of the window. */
export interface Span {
	left: number;
	right: number;
}

/**
 * How far the dragged tab is shifted from its place in the row to stay under the pointer,
 * which holds it `grab` pixels from its left edge. It stays within the row.
 */
export function dragShift(pointerX: number, grab: number, tab: Span, row: Span): number {
	const wanted = pointerX - grab - tab.left;
	return Math.min(Math.max(wanted, row.left - tab.left), row.right - tab.right);
}

/**
 * The place the dragged tab (`from`) takes among `tabs`, each at its place in the row,
 * while it is held at `held`: it passes a neighbour once its leading edge has passed that
 * neighbour's centre. (By its edge, not its centre: held against an end of the row, a
 * wide tab's centre would never pass a narrower last one's.)
 */
export function dropIndex(tabs: readonly Span[], from: number, held: Span): number {
	const centre = (t: Span) => (t.left + t.right) / 2;
	let to = from;
	while (to + 1 < tabs.length && held.right > centre(tabs[to + 1]!)) to++;
	if (to !== from) return to;
	while (to > 0 && held.left < centre(tabs[to - 1]!)) to--;
	return to;
}

/** The tabs' places once the one at `from` has moved to `to`; they keep their widths and
 * the gaps between them. */
export function reordered(tabs: readonly Span[], from: number, to: number): Span[] {
	const first = tabs[0];
	if (!first || from === to) return [...tabs];
	const gap = tabs.length > 1 ? tabs[1]!.left - first.right : 0;
	const order = [...tabs];
	const [moved] = order.splice(from, 1);
	if (moved) order.splice(to, 0, moved);
	let left = first.left;
	return order.map((t) => {
		const span = { left, right: left + (t.right - t.left) };
		left = span.right + gap;
		return span;
	});
}

/**
 * Whether the pointer has pulled the tab off its row: well above or below it, or out of
 * the window at a side.
 */
export function tornOff(
	point: { x: number; y: number },
	row: { top: number; bottom: number },
	windowWidth: number
): boolean {
	return point.y < row.top - TEAR_OFF || point.y > row.bottom + TEAR_OFF || point.x < 0 || point.x > windowWidth;
}

// ----- a row too long for its room -----
// The row scrolls sideways once its tabs, each at its narrowest, need more room than the
// title bar has (as Edge's and VS Code's do): with the wheel, to the tab shown, and by
// itself while a tab is dragged against either end.

/** How near an end of the row a dragged tab's pointer starts to scroll it, CSS pixels. */
export const EDGE_SCROLL = 32;

/** The fastest a dragged tab scrolls the row, CSS pixels a frame (the pointer at the end of
 * the row or past it). */
export const EDGE_SCROLL_MAX = 14;

/** The room kept beyond a tab scrolled into view, so the fade at that end is past it. */
export const REVEAL_MARGIN = 24;

/**
 * The scroll position that brings `tab` into the row's visible part `view` (both where they
 * are on screen now, the row scrolled to `scrollLeft`): the least scroll that does, with
 * `REVEAL_MARGIN` beyond it where there is room. A tab wider than the view shows its start.
 */
export function revealScroll(scrollLeft: number, view: Span, tab: Span): number {
	const room = view.right - view.left;
	const width = tab.right - tab.left;
	const margin = Math.max(0, Math.min(REVEAL_MARGIN, (room - width) / 2));
	if (tab.left - margin < view.left || width > room) return scrollLeft + tab.left - margin - view.left;
	if (tab.right + margin > view.right) return scrollLeft + tab.right + margin - view.right;
	return scrollLeft;
}

/**
 * How fast a tab dragged with the pointer at `x` scrolls the row whose visible part is
 * `view`, CSS pixels a frame: negative to the start, positive to the end, 0 away from both.
 * It grows as the pointer nears an end, and is at its fastest from the end on.
 */
export function edgeScrollSpeed(x: number, view: Span): number {
	const edge = Math.min(EDGE_SCROLL, (view.right - view.left) / 3);
	if (edge <= 0) return 0;
	const into = (depth: number) => Math.ceil(EDGE_SCROLL_MAX * Math.min(1, depth / edge));
	if (x < view.left + edge) return -into(view.left + edge - x);
	if (x > view.right - edge) return into(x - (view.right - edge));
	return 0;
}
