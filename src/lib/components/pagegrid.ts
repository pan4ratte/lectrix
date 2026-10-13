// The page grid's geometry and its contract with what it shows (the Combine view's pages,
// a document's pages being organized). Pure functions, so the component and its tests
// share them.

/** What the grid needs from the state behind it: the selection, and the edits it starts. */
export interface PageGridModel {
	/** Keys of the selected pages. Setting it to an empty list clears the selection. */
	selected: number[];
	readonly selectedSet: ReadonlySet<number>;
	/** The page with keyboard focus, also the anchor for Shift+click and Shift+arrows. */
	focusKey: number | null;
	/** Click: `single` selects one page, `toggle` (Ctrl) adds or removes it, `range` (Shift) selects from the anchor. */
	select(key: number, mode: 'single' | 'toggle' | 'range'): void;
	selectAll(): void;
	/** Replaces the selection (a rectangle drawn over the grid); `focus`, if any, gets the
	 * keyboard focus and anchors the next Shift+click. */
	setSelection(keys: number[], focus: number | null): void;
	/** Moves the pages into gap `gap` of the list: 0 is before the first page. */
	move(keys: ReadonlySet<number>, gap: number): void;
	/** Moves the selected pages one place earlier (`-1`) or later (`1`). */
	nudge(direction: 1 | -1): void;
	rotate(keys: ReadonlySet<number>, degrees: number): void;
	remove(keys: ReadonlySet<number>): void;
}

/** How the grid draws one page. */
export interface GridPicture {
	/** The page's size in points, as its image is rendered. */
	width: number;
	height: number;
	/** A turn shown on top of the rendered image, clockwise, in degrees (0, 90, 180, 270). */
	turn: number;
	/** The image at `scale` (CSS pixels per point times the display's pixel ratio). */
	url(scale: number): string;
	/** What identifies that image: a new key means new pixels are needed. */
	imageKey(scale: number): string;
	/** The page's `/Rotate` the image is rendered for; when it changes, the old pixels turn
	 * with the page until the new ones arrive. */
	rotation?: number;
}

/** The grid's cell geometry, in CSS pixels. */
export interface Grid {
	columns: number;
	cellWidth: number;
	cellHeight: number;
	/** Distance between cell origins, horizontally and vertically. */
	pitchX: number;
	pitchY: number;
	/** Padding around the cells: above the first row and below the last, and the least at
	 * the sides. */
	pad: number;
	/** Where the first column starts: the columns are centered in the width. */
	left: number;
}

/**
 * The grid for `count` pages in `width`: as many columns as fit, centered. Fewer pages than
 * that are centered as the one row they are.
 */
export function gridFor(
	width: number,
	cellWidth: number,
	cellHeight: number,
	gap: number,
	pad: number,
	count = Infinity
): Grid {
	const columns = Math.max(1, Math.floor((width - 2 * pad + gap) / (cellWidth + gap)));
	const used = Math.max(1, Math.min(columns, count));
	const left = Math.max(pad, Math.floor((width - (used * (cellWidth + gap) - gap)) / 2));
	return { columns, cellWidth, cellHeight, pitchX: cellWidth + gap, pitchY: cellHeight + gap, pad, left };
}

/** Where cell `index` starts in the grid's content. */
export function cellOrigin(grid: Grid, index: number) {
	return {
		x: grid.left + (index % grid.columns) * grid.pitchX,
		y: grid.pad + Math.floor(index / grid.columns) * grid.pitchY
	};
}

/** How much wider a cell is than the square its picture is fitted into. */
export const CELL_SIDE = 16;

/**
 * The largest square a picture is fitted into in a view of `width` x `height`: the size at
 * which one page, with the `captionHeight` under it, fills the view.
 */
export function largestThumb(width: number, height: number, pad: number, captionHeight: number): number {
	return Math.floor(Math.min(width - 2 * pad - CELL_SIDE, height - 2 * pad - captionHeight));
}

/** The squares the buttons fit a page's picture into, in CSS pixels: each about a third
 * larger than the one before, up to the largest the view has room for, at which one page
 * fills it. The wheel and a pinch give any size from the first to that one. */
export const THUMB_SIZES = [96, 128, 160, 208, 264, 336, 432, 560, 720, 928, 1200, 1552, 2000] as const;
export const DEFAULT_THUMB = 160;
export const MIN_THUMB = THUMB_SIZES[0];

/** The sizes the buttons step through in a view whose largest size is `largest`: those
 * clearly under it, then that size itself. */
function sizesUpTo(largest: number): number[] {
	if (!Number.isFinite(largest)) return [...THUMB_SIZES];
	const top = Math.max(MIN_THUMB, largest);
	// (A step of a few pixels to the largest size would not be one.)
	return [...THUMB_SIZES.filter((s) => s <= top * 0.9), top];
}

/** `size` kept between the smallest size and `largest`, the size at which one page fills
 * the view (unknown until the grid has measured it: no limit then). */
export function clampThumb(size: number, largest = Infinity): number {
	const top = Math.max(MIN_THUMB, largest);
	if (!Number.isFinite(size)) return Math.min(DEFAULT_THUMB, top);
	return Math.max(MIN_THUMB, Math.min(top, size));
}

/** The next smaller (`-1`) or larger (`1`) of the buttons' sizes, or null at the end. */
export function stepThumb(size: number, direction: 1 | -1, largest = Infinity): number | null {
	const sizes = sizesUpTo(largest);
	// (A size from the wheel may be a hair off one of them.)
	const next = direction > 0 ? sizes.find((s) => s > size + 0.5) : sizes.findLast((s) => s < size - 0.5);
	return next ?? null;
}

/** A place between pages to drop dragged pages: its index (0 is before the first page)
 * and where to draw it in the grid's content. */
export interface Gap {
	gap: number;
	x: number;
	y: number;
}

/** The gap nearest a point in the grid's content, in the row under the pointer. */
export function gapAt(grid: Grid, count: number, x: number, y: number): Gap {
	if (count === 0) return { gap: 0, x: grid.left, y: grid.pad };
	const rows = Math.ceil(count / grid.columns);
	const row = Math.max(0, Math.min(rows - 1, Math.floor((y - grid.pad) / grid.pitchY)));
	const cellsInRow = Math.min(grid.columns, count - row * grid.columns);
	const col = Math.max(0, Math.min(cellsInRow, Math.round((x - grid.left) / grid.pitchX)));
	return { gap: row * grid.columns + col, x: grid.left + col * grid.pitchX, y: grid.pad + row * grid.pitchY };
}

/** A rectangle in the grid's content. */
export interface Box {
	x0: number;
	y0: number;
	x1: number;
	y1: number;
}

/** The rectangle with corners `a` and `b`, whichever way it was drawn. */
export function boxBetween(a: { x: number; y: number }, b: { x: number; y: number }): Box {
	return { x0: Math.min(a.x, b.x), y0: Math.min(a.y, b.y), x1: Math.max(a.x, b.x), y1: Math.max(a.y, b.y) };
}

export function boxesTouch(a: Box, b: Box): boolean {
	return a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1;
}

/**
 * The cells of the rows that `box` reaches, as a first and last index, or null if it
 * reaches none. (Which of them it touches is for the caller: a cell's picture is smaller
 * than the cell.)
 */
export function cellsInRowsOf(grid: Grid, count: number, box: Box): { first: number; last: number } | null {
	if (count === 0) return null;
	const rows = Math.ceil(count / grid.columns);
	const firstRow = Math.max(0, Math.floor((box.y0 - grid.pad) / grid.pitchY));
	const lastRow = Math.min(rows - 1, Math.floor((box.y1 - grid.pad) / grid.pitchY));
	if (firstRow > lastRow) return null;
	return { first: firstRow * grid.columns, last: Math.min(count, (lastRow + 1) * grid.columns) - 1 };
}

/**
 * A page of `width` x `height` points fitted into a square of `side` pixels after a turn
 * of `turn` degrees: its size as shown (turned), as rendered (the page's own
 * orientation), and the pixels per point.
 */
export function fitPicture(width: number, height: number, turn: number, side: number) {
	const sideways = turn === 90 || turn === 270;
	const w = sideways ? height : width;
	const h = sideways ? width : height;
	const fit = Math.min(side / w, side / h);
	return { shownW: w * fit, shownH: h * fit, imageW: width * fit, imageH: height * fit, fit };
}

/** The keys from `anchor` to `to` inclusive, in the grid's order (Shift+click). */
export function rangeOfKeys(keys: readonly number[], anchor: number, to: number): number[] {
	const a = keys.indexOf(anchor);
	const b = keys.indexOf(to);
	if (a < 0 || b < 0) return b >= 0 ? [to] : [];
	const [from, end] = a <= b ? [a, b] : [b, a];
	return keys.slice(from, end + 1);
}

/** `base` followed by the keys of `added` it does not hold yet. */
export function withKeys(base: readonly number[], added: readonly number[]): number[] {
	const held = new Set(base);
	return [...base, ...added.filter((k) => !held.has(k))];
}
