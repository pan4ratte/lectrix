// The search's counter following the view (section 6.1, Settings: "Counter follows the
// view"): as the pages scroll, it counts the matches on screen, so that next and previous go
// on from there rather than from the match last gone to.

/** The match the counter counts: shown, or passed (`between`, none on screen), or -1. */
export interface Followed {
	index: number;
	between: boolean;
}

/**
 * The match the counter counts with the view from `top` to `bottom` (pixels down the
 * content): the match shown (`current`, unless passed) while it is on screen; otherwise the
 * last match on screen; with none, the last one above it, passed; with none above, -1.
 * `hits` are in document order, `pages` the first and last page in view, and `y` gives a
 * match's middle in the same pixels as `top` (null for one without a place).
 */
export function followedHit(
	hits: readonly { page: number }[],
	pages: readonly [number, number],
	y: (index: number) => number | null,
	top: number,
	bottom: number,
	current: Followed
): Followed {
	const [first, last] = pages;
	const from = firstAtPage(hits, first);
	const to = firstAtPage(hits, last + 1);
	const onScreen = (i: number) => {
		const at = y(i);
		return at !== null && at >= top && at <= bottom;
	};
	if (!current.between && current.index >= from && current.index < to && onScreen(current.index)) return current;
	let lastSeen = -1;
	let lastAbove = from - 1;
	for (let i = from; i < to; i++) {
		const at = y(i);
		if (at === null) continue;
		if (at > bottom) continue;
		if (at >= top) lastSeen = i;
		else lastAbove = Math.max(lastAbove, i);
	}
	if (lastSeen >= 0) return { index: lastSeen, between: false };
	return lastAbove >= 0 ? { index: lastAbove, between: true } : { index: -1, between: false };
}

/** The first of `hits` (in page order) on page `page` or after it. */
function firstAtPage(hits: readonly { page: number }[], page: number): number {
	let lo = 0;
	let hi = hits.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (hits[mid]!.page < page) lo = mid + 1;
		else hi = mid;
	}
	return lo;
}
