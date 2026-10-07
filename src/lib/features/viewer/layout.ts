// The layout of a document's pages, in CSS pixels: rows of one or two pages (section 6.1,
// scroll modes), all of them scrolling continuously, or one row at a time.

import type { PageLayout, ScrollMode } from '#lib/ipc/index.ts';

/** CSS pixels per PDF point at 100% zoom (96 dpi screen, 72 pt per inch). */
export const CSS_PX_PER_PT = 96 / 72;
/** Space between pages and around the page column, in CSS pixels. */
export const PAGE_GAP = 12;
export const PAGE_MARGIN = 16;

export type Rotation = 0 | 90 | 180 | 270;

/** A page's size in points, as laid out (after its `/Rotate`). */
export interface Size {
	width: number;
	height: number;
}

/** How pages are laid out in a tab: the scroll modes the view offers. */
export type ViewMode = Exclude<ScrollMode, 'document'>;

export interface Arrangement {
	/** Pages side by side in a row. */
	columns: 1 | 2;
	/** With two columns, the first page has a row of its own, on the right (a book's cover). */
	cover: boolean;
	/** The one row shown (single page, two pages), or null: every row, continuously. */
	row: number | null;
}

export const CONTINUOUS: Arrangement = { columns: 1, cover: false, row: null };

/** The scroll modes, in the order the menus list them. */
export const VIEW_MODES: readonly { id: ViewMode; label: string }[] = [
	{ id: 'singlePage', label: 'Single page' },
	{ id: 'singlePageContinuous', label: 'Single page, continuous' },
	{ id: 'twoPage', label: 'Two pages' },
	{ id: 'twoPageContinuous', label: 'Two pages, continuous' }
];

export function isContinuous(mode: ViewMode): boolean {
	return mode === 'singlePageContinuous' || mode === 'twoPageContinuous';
}

export function columnsOf(mode: ViewMode): 1 | 2 {
	return mode === 'twoPage' || mode === 'twoPageContinuous' ? 2 : 1;
}

export interface PageBox {
	/** Distance from the top of the scroll content to the top of the page. */
	top: number;
	/** Left edge relative to the content's middle (the spine between two pages). */
	x: number;
	/** Size on screen after view rotation. */
	width: number;
	height: number;
}

export interface Row {
	top: number;
	height: number;
	/** First and last page index in the row. */
	first: number;
	last: number;
}

export interface Layout {
	/** Every page; those of rows not shown lie outside the content. */
	pages: PageBox[];
	rows: Row[];
	/** First and last row shown. */
	shown: [number, number];
	/** Height of the whole scroll content, including margins. */
	totalHeight: number;
	/** Width of the widest row shown, even about the middle. */
	maxWidth: number;
	zoom: number;
	rotation: Rotation;
}

export function normalizeRotation(degrees: number): Rotation {
	const r = (((Math.round(degrees / 90) * 90) % 360) + 360) % 360;
	return r as Rotation;
}

/** Size of a page on screen at `zoom`, after view rotation. */
export function pageCssSize(size: Size, zoom: number, rotation: Rotation) {
	const w = size.width * CSS_PX_PER_PT * zoom;
	const h = size.height * CSS_PX_PER_PT * zoom;
	return rotation === 90 || rotation === 270 ? { width: h, height: w } : { width: w, height: h };
}

/** The row holding page `page`. */
export function rowOf(page: number, columns: 1 | 2, cover: boolean): number {
	if (columns === 1) return page;
	return cover ? (page === 0 ? 0 : Math.floor((page + 1) / 2)) : Math.floor(page / 2);
}

/** The first page of row `row`. */
export function rowStart(row: number, columns: 1 | 2, cover: boolean): number {
	if (columns === 1) return row;
	return cover ? (row === 0 ? 0 : 2 * row - 1) : 2 * row;
}

/** The number of rows `count` pages make. */
export function rowCount(count: number, columns: 1 | 2, cover: boolean): number {
	return count === 0 ? 0 : rowOf(count - 1, columns, cover) + 1;
}

export function computeLayout(
	sizes: readonly Size[],
	zoom: number,
	rotation: Rotation,
	arrangement: Arrangement = CONTINUOUS
): Layout {
	const { columns, cover } = arrangement;
	const n = sizes.length;
	const pages: PageBox[] = new Array(n);
	const rows: Row[] = [];
	let top = PAGE_MARGIN;
	for (let first = 0; first < n; ) {
		const row = rows.length;
		const last = Math.min(n, rowStart(row + 1, columns, cover)) - 1;
		let height = 0;
		for (let i = first; i <= last; i++) {
			const { width, height: h } = pageCssSize(sizes[i]!, zoom, rotation);
			pages[i] = { top, x: 0, width, height: h };
			height = Math.max(height, h);
		}
		for (let i = first; i <= last; i++) {
			const p = pages[i]!;
			// Pages of different heights in a row are centred on each other.
			p.top = top + (height - p.height) / 2;
			if (columns === 1) p.x = -p.width / 2;
			// Two columns meet at the middle; a page alone keeps its side: a cover on the right,
			// a last page on the left.
			else if (i === last && (i > first || (cover && i === 0))) p.x = PAGE_GAP / 2;
			else p.x = -PAGE_GAP / 2 - p.width;
		}
		rows.push({ top, height, first, last });
		top += height + PAGE_GAP;
		first = last + 1;
	}
	if (rows.length === 0) return { pages, rows, shown: [0, -1], totalHeight: 0, maxWidth: 0, zoom, rotation };

	let shown: [number, number] = [0, rows.length - 1];
	let totalHeight = top - PAGE_GAP + PAGE_MARGIN;
	if (arrangement.row !== null) {
		// One row: it alone makes the content, and the others lie outside it.
		const row = Math.min(Math.max(0, arrangement.row), rows.length - 1);
		const shift = rows[row]!.top - PAGE_MARGIN;
		for (const p of pages) p.top -= shift;
		for (const r of rows) r.top -= shift;
		shown = [row, row];
		totalHeight = rows[row]!.height + 2 * PAGE_MARGIN;
	}
	let half = 0;
	for (let r = shown[0]; r <= shown[1]; r++) {
		const { first, last } = rows[r]!;
		for (let i = first; i <= last; i++) {
			const p = pages[i]!;
			half = Math.max(half, -p.x, p.x + p.width);
		}
	}
	return { pages, rows, shown, totalHeight, maxWidth: 2 * half, zoom, rotation };
}

/** Whether page `index` is in a row shown. */
export function isShown(layout: Layout, index: number): boolean {
	const [from, to] = layout.shown;
	return to >= from && index >= layout.rows[from]!.first && index <= layout.rows[to]!.last;
}

/** Index of the row shown at vertical position `y` (the row above it if `y` is in a gap). */
export function rowAtY(layout: Layout, y: number): number {
	const [from, to] = layout.shown;
	if (to < from) return -1;
	let lo = from;
	let hi = to;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (layout.rows[mid]!.top <= y) lo = mid;
		else hi = mid - 1;
	}
	return lo;
}

/** The first page of the row at vertical position `y` (see `rowAtY`). */
export function pageAtY(layout: Layout, y: number): number {
	const row = rowAtY(layout, y);
	return row < 0 ? -1 : layout.rows[row]!.first;
}

/** The page at a point of the content: in the row at `y`, the one nearest `x`. */
export function pageAt(layout: Layout, x: number, y: number, contentWidth: number): number {
	const row = rowAtY(layout, y);
	if (row < 0) return -1;
	const { first, last } = layout.rows[row]!;
	let best = first;
	let bestDistance = Infinity;
	for (let i = first; i <= last; i++) {
		const left = pageLeft(layout, i, contentWidth);
		const distance = Math.max(0, left - x, x - (left + layout.pages[i]!.width));
		if (distance < bestDistance) {
			best = i;
			bestDistance = distance;
		}
	}
	return best;
}

/** First and last page index of the rows that intersect the band [top, bottom]. */
export function pagesInRange(layout: Layout, top: number, bottom: number): [number, number] {
	const [from, to] = layout.shown;
	if (to < from) return [0, -1];
	let first = rowAtY(layout, top);
	const firstRow = layout.rows[first]!;
	if (firstRow.top + firstRow.height < top && first < to) first++;
	const last = Math.max(first, rowAtY(layout, bottom));
	return [layout.rows[first]!.first, layout.rows[last]!.last];
}

/** The page that counts as "current": the first in the row under a line 30% down the viewport. */
export function currentPage(layout: Layout, scrollTop: number, viewportHeight: number): number {
	return Math.max(0, pageAtY(layout, scrollTop + viewportHeight * 0.3));
}

/** Left edge of a page inside a scroll content of `contentWidth` (rows are centred). */
export function pageLeft(layout: Layout, index: number, contentWidth: number): number {
	const page = layout.pages[index];
	if (!page) return 0;
	return contentWidth / 2 + page.x;
}

/** Width of the scroll content for a viewport of `viewportWidth`. */
export function contentWidth(layout: Layout, viewportWidth: number): number {
	return Math.max(viewportWidth, layout.maxWidth + 2 * PAGE_MARGIN);
}

/** The scroll mode a document asks for in its catalog's `/PageLayout`, and whether its
 * first page stands alone (the `…Right` layouts); null when it asks for none. */
export function documentMode(pageLayout: PageLayout | null): { mode: ViewMode; cover: boolean } | null {
	switch (pageLayout) {
		case 'SinglePage':
			return { mode: 'singlePage', cover: false };
		case 'OneColumn':
			return { mode: 'singlePageContinuous', cover: false };
		case 'TwoPageLeft':
			return { mode: 'twoPage', cover: false };
		case 'TwoPageRight':
			return { mode: 'twoPage', cover: true };
		case 'TwoColumnLeft':
			return { mode: 'twoPageContinuous', cover: false };
		case 'TwoColumnRight':
			return { mode: 'twoPageContinuous', cover: true };
		default:
			return null;
	}
}
