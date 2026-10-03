// Continuous vertical layout of a document's pages, in CSS pixels.

import type { PageSize } from '#lib/ipc/index.ts';

/** CSS pixels per PDF point at 100% zoom (96 dpi screen, 72 pt per inch). */
export const CSS_PX_PER_PT = 96 / 72;
/** Space between pages and around the page column, in CSS pixels. */
export const PAGE_GAP = 12;
export const PAGE_MARGIN = 16;

export type Rotation = 0 | 90 | 180 | 270;

export interface PageBox {
	/** Distance from the top of the scroll content to the top of the page. */
	top: number;
	/** Size on screen after view rotation. */
	width: number;
	height: number;
}

export interface Layout {
	pages: PageBox[];
	/** Height of the whole scroll content, including margins. */
	totalHeight: number;
	/** Width of the widest page. */
	maxWidth: number;
	zoom: number;
	rotation: Rotation;
}

export function normalizeRotation(degrees: number): Rotation {
	const r = (((Math.round(degrees / 90) * 90) % 360) + 360) % 360;
	return r as Rotation;
}

/** Size of a page on screen at `zoom`, after view rotation. */
export function pageCssSize(size: PageSize, zoom: number, rotation: Rotation) {
	const w = size.width * CSS_PX_PER_PT * zoom;
	const h = size.height * CSS_PX_PER_PT * zoom;
	return rotation === 90 || rotation === 270 ? { width: h, height: w } : { width: w, height: h };
}

export function computeLayout(sizes: readonly PageSize[], zoom: number, rotation: Rotation): Layout {
	const pages: PageBox[] = new Array(sizes.length);
	let top = PAGE_MARGIN;
	let maxWidth = 0;
	for (let i = 0; i < sizes.length; i++) {
		const { width, height } = pageCssSize(sizes[i]!, zoom, rotation);
		pages[i] = { top, width, height };
		top += height + PAGE_GAP;
		maxWidth = Math.max(maxWidth, width);
	}
	const totalHeight = sizes.length ? top - PAGE_GAP + PAGE_MARGIN : 0;
	return { pages, totalHeight, maxWidth, zoom, rotation };
}

/** Index of the page at vertical position `y` (the page above it if `y` is in a gap). */
export function pageAtY(layout: Layout, y: number): number {
	const pages = layout.pages;
	if (pages.length === 0) return -1;
	let lo = 0;
	let hi = pages.length - 1;
	while (lo < hi) {
		const mid = (lo + hi + 1) >> 1;
		if (pages[mid]!.top <= y) lo = mid;
		else hi = mid - 1;
	}
	return lo;
}

/** First and last page index that intersect the band [top, bottom]. */
export function pagesInRange(layout: Layout, top: number, bottom: number): [number, number] {
	const n = layout.pages.length;
	if (n === 0) return [0, -1];
	let first = pageAtY(layout, top);
	const firstPage = layout.pages[first]!;
	if (firstPage.top + firstPage.height < top && first < n - 1) first++;
	const last = pageAtY(layout, bottom);
	return [first, Math.max(first, last)];
}

/** The page that counts as "current": the one under a line 30% down the viewport. */
export function currentPage(layout: Layout, scrollTop: number, viewportHeight: number): number {
	return Math.max(0, pageAtY(layout, scrollTop + viewportHeight * 0.3));
}

/** Left edge of a page inside a scroll content of `contentWidth` (pages are centered). */
export function pageLeft(layout: Layout, index: number, contentWidth: number): number {
	const page = layout.pages[index];
	if (!page) return 0;
	return Math.max(PAGE_MARGIN, (contentWidth - page.width) / 2);
}

/** Width of the scroll content for a viewport of `viewportWidth`. */
export function contentWidth(layout: Layout, viewportWidth: number): number {
	return Math.max(viewportWidth, layout.maxWidth + 2 * PAGE_MARGIN);
}
