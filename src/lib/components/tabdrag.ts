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
