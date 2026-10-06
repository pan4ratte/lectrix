// Page thumbnail sizes in the Pages panel (section 6.1): steps for Smaller and Larger, and
// the width that fills the panel.

/** Widths the Smaller and Larger buttons step through, CSS pixels. */
export const THUMB_SIZES: readonly number[] = [64, 88, 112, 144, 184, 232, 296, 376, 456];
export const DEFAULT_THUMB_WIDTH = 112;
/** Space each side of a thumbnail: the list's padding and the thumbnail's own. */
export const THUMB_SIDE = 16;
/** Thumbnails render at widths rounded up to this, so resizing the panel while they fit
 * its width doesn't render them again at every pixel. */
const RENDER_STEP = 32;

/** The widest thumbnail a list `listWidth` wide shows whole. */
export function fitWidth(listWidth: number): number {
	return Math.max(THUMB_SIZES[0]!, Math.floor(listWidth - 2 * THUMB_SIDE));
}

/** The width thumbnails show at: the panel's when fitting it, else the chosen one, never
 * wider than the panel (before the list is measured, the chosen one). */
export function shownWidth(chosen: number, fit: boolean, listWidth: number): number {
	if (listWidth <= 0) return chosen;
	const max = fitWidth(listWidth);
	return fit ? max : Math.min(chosen, max);
}

/** The next width smaller (-1) or larger (1) than `shown`, or null at either end; larger
 * stops at the panel's width. */
export function stepWidth(shown: number, direction: 1 | -1, listWidth: number): number | null {
	if (direction < 0) return [...THUMB_SIZES].reverse().find((s) => s < shown) ?? null;
	const max = listWidth > 0 ? fitWidth(listWidth) : Infinity;
	if (shown >= max) return null;
	const next = THUMB_SIZES.find((s) => s > shown);
	if (next === undefined) return null;
	return Math.min(next, max);
}

/** The width a thumbnail shown `shown` wide is rendered at. */
export function renderWidth(shown: number): number {
	return Math.ceil(shown / RENDER_STEP) * RENDER_STEP;
}

/** A remembered width, kept within the steps. */
export function clampThumbWidth(width: number): number {
	const min = THUMB_SIZES[0]!;
	const max = THUMB_SIZES[THUMB_SIZES.length - 1]!;
	return Number.isFinite(width) ? Math.min(max, Math.max(min, Math.round(width))) : DEFAULT_THUMB_WIDTH;
}

/** The list's padding. */
export const LIST_PAD = 12;
/** A thumbnail's padding inside its cell (around its focus ring). */
const CELL_PAD = THUMB_SIDE - LIST_PAD;
/** Room under each thumbnail for its label. */
const LABEL_HEIGHT = 22;
/** Space between rows, and between columns in a grid. */
const ROW_GAP = 12;
const COLUMN_GAP = 8;

export interface ThumbItem {
	/** The thumbnail's cell (its padding included), in the list. */
	top: number;
	left: number;
	imageH: number;
	height: number;
	row: number;
}

export interface ThumbRow {
	top: number;
	height: number;
	/** Its first and last page. */
	first: number;
	last: number;
}

export interface ThumbLayout {
	items: ThumbItem[];
	rows: ThumbRow[];
	columns: number;
	height: number;
}

/** How many thumbnails `width` wide a row holds: one, or in a grid as many as fit. */
export function thumbColumns(width: number, listWidth: number, grid: boolean): number {
	if (!grid || listWidth <= 0) return 1;
	const cell = width + 2 * CELL_PAD;
	return Math.max(1, Math.floor((listWidth - 2 * LIST_PAD + COLUMN_GAP) / (cell + COLUMN_GAP)));
}

/**
 * Lays out thumbnails `width` wide for pages of the given sizes (rotation applied): in one
 * column, or in a grid of as many columns as fit, centred in the list. A row is as tall as
 * its tallest page.
 */
export function layoutThumbs(pages: readonly { width: number; height: number }[], width: number, listWidth: number, grid: boolean): ThumbLayout {
	const columns = thumbColumns(width, listWidth, grid);
	const cell = width + 2 * CELL_PAD;
	const rowWidth = columns * cell + (columns - 1) * COLUMN_GAP;
	const left0 = Math.max(0, (listWidth - rowWidth) / 2);
	const items: ThumbItem[] = [];
	const rows: ThumbRow[] = [];
	let top = LIST_PAD;
	for (let first = 0; first < pages.length; first += columns) {
		const last = Math.min(pages.length, first + columns) - 1;
		let height = 0;
		for (let i = first; i <= last; i++) {
			const p = pages[i]!;
			const imageH = Math.round((width * p.height) / p.width);
			const item = { top, left: left0 + (i - first) * (cell + COLUMN_GAP), imageH, height: imageH + LABEL_HEIGHT, row: rows.length };
			items.push(item);
			height = Math.max(height, item.height);
		}
		rows.push({ top, height, first, last });
		top += height + ROW_GAP;
	}
	return { items, rows, columns, height: rows.length ? top - ROW_GAP + LIST_PAD : 0 };
}
