// The marks of a search's matches beside the pages' scrollbar (section 6.1): one for each
// match, where in the document it is, as Chrome and VS Code mark theirs.

import { PAGE_MARGIN, type Layout } from './layout.ts';

/** How tall a match's mark is, and the mark of the match shown, in CSS pixels. */
export const MARK_PX = 2;
export const CURRENT_MARK_PX = 4;

export interface SearchMarks {
	/** The top of each mark, from the top of the strip, in CSS pixels: one per pixel row. */
	tops: number[];
	/** The top of the shown match's mark, or null. */
	current: number | null;
}

/**
 * Where each match's mark goes on a strip `track` pixels tall: as far down it as the match
 * is down the document laid out end to end, which is where the scrollbar shows it when
 * all the pages scroll (in single page and two pages, the strip still stands for the whole
 * document). `place` gives a match's distance from the top of its page box, in CSS
 * pixels, and null for one without a place.
 */
export function searchMarks(
	layout: Layout,
	matches: readonly { page: number }[],
	current: number,
	place: (index: number) => number | null,
	track: number
): SearchMarks {
	const rows = layout.rows;
	const last = rows[rows.length - 1];
	if (!last || track <= 0) return { tops: [], current: null };
	// In the modes that show one row, the pages lie above and below the content; measure
	// from the first row's place.
	const origin = rows[0]!.top - PAGE_MARGIN;
	const length = last.top + last.height + PAGE_MARGIN - origin;
	const top = (index: number, height: number): number | null => {
		const box = layout.pages[matches[index]!.page];
		const y = place(index);
		if (!box || y === null) return null;
		const at = ((box.top - origin + y) / length) * track - height / 2;
		return Math.round(Math.min(track - height, Math.max(0, at)));
	};
	const tops = new Set<number>();
	for (let i = 0; i < matches.length; i++) {
		const t = top(i, MARK_PX);
		if (t !== null) tops.add(t);
	}
	return {
		tops: [...tops].sort((a, b) => a - b),
		current: current >= 0 && current < matches.length ? top(current, CURRENT_MARK_PX) : null
	};
}
