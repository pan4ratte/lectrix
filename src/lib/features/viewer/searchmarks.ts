// The marks of a search's matches beside the pages' scrollbar (section 6.1): one for each
// match, where the scrollbar's thumb is when that match is in the middle of the view.

import type { Platform } from '#lib/platform.ts';

import { PAGE_MARGIN, type Layout } from './layout.ts';

/** How tall a match's mark is, and the mark of the match shown, in CSS pixels. */
export const MARK_PX = 2;
export const CURRENT_MARK_PX = 4;

/** WebView2's thin scrollbar, for each pixel of its width: how long each arrow button is,
 * and the shortest its thumb gets (12 and 11 CSS pixels for its 10, measured at 100% and
 * 150% scaling). */
const ARROW_PER_WIDTH = 1.2;
const MIN_THUMB_PER_WIDTH = 1.1;

export interface SearchMarks {
	/** The top of each mark, from the top of the strip, in CSS pixels: one per pixel row. */
	tops: number[];
	/** The top of the shown match's mark, or null. */
	current: number | null;
}

/** The pages' scrollbar, in CSS pixels: what the marks are placed along. */
export interface Track {
	/** The scrollbar's height, which is the view's. */
	height: number;
	/** How far from either end the thumb stops: the arrow buttons' length. */
	start: number;
	/** The thumb's length. */
	thumb: number;
}

/**
 * The scrollbar of a view `height` pixels tall over content `content` pixels tall, with a
 * scrollbar `width` pixels wide (0 for none, or one laid over the pages). Only WebView2's
 * has arrow buttons and a shortest thumb; elsewhere the thumb is as long as the share of
 * the content it shows.
 */
export function scrollbarTrack(platform: Platform, width: number, height: number, content: number): Track {
	const windows = platform === 'windows' && width > 0;
	const start = windows ? width * ARROW_PER_WIDTH : 0;
	const length = Math.max(0, height - 2 * start);
	const share = content > height ? height / content : 1;
	return { height, start, thumb: Math.max(windows ? width * MIN_THUMB_PER_WIDTH : 0, length * share) };
}

/**
 * Where each match's mark goes beside the scrollbar `track`: where the middle of its thumb
 * is when the view has the match in its middle (Chrome and VS Code place theirs as far down
 * as the match is down the document, which beside a long document's short thumb puts a
 * mark pages away from where the thumb shows its match). In single page and two pages,
 * where the scrollbar spans the row shown, the marks stand for the whole document, as far
 * down the scrollbar's run as the match is down it. `place` gives a match's distance from
 * the top of its page box, in CSS pixels, and null for one without a place.
 */
export function searchMarks(
	layout: Layout,
	matches: readonly { page: number }[],
	current: number,
	place: (index: number) => number | null,
	track: Track
): SearchMarks {
	const rows = layout.rows;
	const last = rows[rows.length - 1];
	const run = track.height - 2 * track.start;
	if (!last || run <= 0) return { tops: [], current: null };
	const scrolls = layout.shown[0] === 0 && layout.shown[1] === rows.length - 1 && layout.totalHeight > track.height;
	// In the modes that show one row, the pages lie above and below the content; measure
	// from the first row's place.
	const origin = rows[0]!.top - PAGE_MARGIN;
	const length = last.top + last.height + PAGE_MARGIN - origin;
	/** How far along the scrollbar's run the middle of the thumb (or the mark) is, 0 to 1,
	 * for a point `y` down the content. */
	const along = scrolls
		? (y: number) => Math.min(1, Math.max(0, (y - track.height / 2) / (layout.totalHeight - track.height)))
		: (y: number) => (y - origin) / length;
	const thumb = scrolls ? Math.min(track.thumb, run) : 0;
	const top = (index: number, height: number): number | null => {
		const box = layout.pages[matches[index]!.page];
		const y = place(index);
		if (!box || y === null) return null;
		const at = track.start + thumb / 2 + along(box.top + y) * (run - thumb) - height / 2;
		return Math.round(Math.min(track.height - height, Math.max(0, at)));
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
