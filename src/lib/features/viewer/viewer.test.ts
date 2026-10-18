import { describe, expect, it } from 'vitest';

import type { PageText } from '#lib/ipc/index.ts';

import { NavHistory } from './history.ts';
import {
	CSS_PX_PER_PT,
	PAGE_GAP,
	PAGE_MARGIN,
	computeLayout,
	contentWidth,
	currentPage,
	documentMode,
	isShown,
	normalizeRotation,
	pageAt,
	pageAtY,
	pageLeft,
	pagesBeside,
	pagesInRange,
	rowCount,
	rowOf,
	rowStart,
	turnTransform,
	turnedSize
} from './layout.ts';
import { pageBoxText, pageName, pageOf, pagePosition, resolvePageInput } from './pagebox.ts';
import {
	caretBar,
	edgeAt,
	extendByUnit,
	hitTest,
	lineAt,
	moveCaret,
	ordered,
	prepareText,
	selectionHandles,
	selectionIsEmpty,
	selectionRects,
	selectionText,
	wordAt,
	type CaretStep,
	type TextGeometry
} from './selection.ts';
import {
	MAX_ZOOM,
	MIN_ZOOM,
	cssPerImagePixel,
	fitPageZoom,
	fitWidthZoom,
	isTiled,
	pixelSize,
	renderScale,
	snapToDevice,
	stepZoom,
	isWheelNotch,
	wheelZoomFactor,
	wholePageScale,
	zoomBetween
} from './zoom.ts';
import { pinchDeltas } from './pinch.ts';

const letter = { width: 612, height: 792 };

describe('layout', () => {
	it('stacks pages with gaps and margins', () => {
		const layout = computeLayout([letter, letter, { width: 792, height: 612 }], 1, 0);
		const h = 792 * CSS_PX_PER_PT;
		expect(layout.pages[0]!.top).toBe(PAGE_MARGIN);
		expect(layout.pages[1]!.top).toBeCloseTo(PAGE_MARGIN + h + PAGE_GAP);
		expect(layout.maxWidth).toBeCloseTo(792 * CSS_PX_PER_PT);
		expect(layout.totalHeight).toBeCloseTo(PAGE_MARGIN * 2 + 2 * h + 612 * CSS_PX_PER_PT + 2 * PAGE_GAP);
	});

	it('swaps width and height when the view is rotated', () => {
		const layout = computeLayout([letter], 2, 90);
		expect(layout.pages[0]!.width).toBeCloseTo(792 * CSS_PX_PER_PT * 2);
		expect(layout.pages[0]!.height).toBeCloseTo(612 * CSS_PX_PER_PT * 2);
	});

	it('lays each page out as it is turned, when pages are turned one by one', () => {
		const w = 612 * CSS_PX_PER_PT;
		const h = 792 * CSS_PX_PER_PT;
		// The second page on its side, the third upside down.
		const turns = [0, 90, 180] as const;
		const layout = computeLayout([letter, letter, letter], 1, (page) => turns[page]!);
		expect(layout.pages.map((p) => [p.width, p.height])).toEqual([
			[w, h],
			[h, w],
			[w, h]
		]);
		// The pages after it follow its new height, and the widest page sets the content's width.
		expect(layout.pages[2]!.top).toBeCloseTo(PAGE_MARGIN + h + PAGE_GAP + w + PAGE_GAP);
		expect(layout.maxWidth).toBeCloseTo(h);
		// Beside an upright page, the one on its side is centred on it.
		const row = computeLayout([letter, letter], 1, (page) => turns[page]!, { columns: 2, cover: false, row: null });
		expect(row.rows[0]!.height).toBeCloseTo(h);
		expect(row.pages[1]!.top).toBeCloseTo(PAGE_MARGIN + (h - w) / 2);
		expect(row.pages[1]!.x).toBe(PAGE_GAP / 2);
	});

	it('turns a size and the layer that shows it', () => {
		expect(turnedSize(letter, 0)).toEqual(letter);
		expect(turnedSize(letter, 180)).toEqual(letter);
		expect(turnedSize(letter, 90)).toEqual({ width: 792, height: 612 });
		expect(turnedSize(letter, 270)).toEqual({ width: 792, height: 612 });
		// A layer 60 by 80, turned about its top-left corner and moved back into its box.
		expect(turnTransform(0, 60, 80)).toBe('none');
		expect(turnTransform(90, 60, 80)).toBe('translate(80px, 0) rotate(90deg)');
		expect(turnTransform(180, 60, 80)).toBe('translate(60px, 80px) rotate(180deg)');
		expect(turnTransform(270, 60, 80)).toBe('translate(0, 60px) rotate(270deg)');
	});

	it('finds pages by position', () => {
		const layout = computeLayout(Array(1000).fill(letter), 1, 0);
		const p500 = layout.pages[500]!;
		expect(pageAtY(layout, p500.top + 1)).toBe(500);
		expect(pageAtY(layout, p500.top - 1)).toBe(499); // in the gap above page 500
		expect(pageAtY(layout, 0)).toBe(0);
		expect(pageAtY(layout, 1e9)).toBe(999);
		expect(pagesInRange(layout, p500.top + 10, p500.top + p500.height + 50)).toEqual([500, 501]);
		expect(currentPage(layout, p500.top, 1000)).toBe(500);
	});

	it('numbers rows of one or two pages, with or without a cover', () => {
		// 1 | 2 3 | 4 5 | 6 with a cover; 1 2 | 3 4 | 5 6 without.
		expect([0, 1, 2, 3, 4, 5].map((p) => rowOf(p, 2, true))).toEqual([0, 1, 1, 2, 2, 3]);
		expect([0, 1, 2, 3].map((r) => rowStart(r, 2, true))).toEqual([0, 1, 3, 5]);
		expect([0, 1, 2, 3, 4, 5].map((p) => rowOf(p, 2, false))).toEqual([0, 0, 1, 1, 2, 2]);
		expect([0, 1, 2].map((r) => rowStart(r, 2, false))).toEqual([0, 2, 4]);
		expect(rowOf(7, 1, true)).toBe(7);
		expect(rowCount(6, 2, true)).toBe(4);
		expect(rowCount(6, 2, false)).toBe(3);
		expect(rowCount(5, 2, false)).toBe(3);
		expect(rowCount(0, 2, false)).toBe(0);
	});

	it('puts two pages side by side, meeting at the middle', () => {
		const w = 612 * CSS_PX_PER_PT;
		const layout = computeLayout(Array(5).fill(letter), 1, 0, { columns: 2, cover: true, row: null });
		expect(layout.rows.map((r) => [r.first, r.last])).toEqual([
			[0, 0],
			[1, 2],
			[3, 4]
		]);
		// The cover alone on the right, then left and right pages.
		expect(layout.pages[0]!.x).toBe(PAGE_GAP / 2);
		expect(layout.pages[1]!.x).toBeCloseTo(-PAGE_GAP / 2 - w);
		expect(layout.pages[2]!.x).toBe(PAGE_GAP / 2);
		expect(layout.pages[1]!.top).toBe(layout.pages[2]!.top);
		expect(layout.maxWidth).toBeCloseTo(2 * w + PAGE_GAP);
		const cw = contentWidth(layout, 0);
		expect(pageLeft(layout, 1, cw)).toBeCloseTo(PAGE_MARGIN);
		// Without a cover, a last page alone stays on the left.
		const plain = computeLayout(Array(3).fill(letter), 1, 0, { columns: 2, cover: false, row: null });
		expect(plain.rows.map((r) => [r.first, r.last])).toEqual([
			[0, 1],
			[2, 2]
		]);
		expect(plain.pages[2]!.x).toBeCloseTo(-PAGE_GAP / 2 - w);
	});

	it('centres pages of different heights in their row', () => {
		const small = { width: 612, height: 396 };
		const layout = computeLayout([letter, small], 1, 0, { columns: 2, cover: false, row: null });
		const row = layout.rows[0]!;
		expect(row.height).toBeCloseTo(792 * CSS_PX_PER_PT);
		expect(layout.pages[1]!.top).toBeCloseTo(row.top + (792 - 396) * CSS_PX_PER_PT * 0.5);
	});

	it('finds the page at a point, on either side of the middle', () => {
		const layout = computeLayout(Array(4).fill(letter), 1, 0, { columns: 2, cover: false, row: null });
		const cw = contentWidth(layout, 2000);
		const y = layout.rows[1]!.top + 10;
		expect(pageAt(layout, pageLeft(layout, 2, cw) + 5, y, cw)).toBe(2);
		expect(pageAt(layout, pageLeft(layout, 3, cw) + 5, y, cw)).toBe(3);
		// Beside the pages, the nearer one.
		expect(pageAt(layout, 0, y, cw)).toBe(2);
		expect(pageAt(layout, cw, y, cw)).toBe(3);
		expect(pageAtY(layout, y)).toBe(2);
		expect(currentPage(layout, layout.rows[1]!.top, 100)).toBe(2);
	});

	it('shows one row at a time in the single page and two-page modes', () => {
		const h = 792 * CSS_PX_PER_PT;
		const layout = computeLayout(Array(6).fill(letter), 1, 0, { columns: 2, cover: false, row: 1 });
		expect(layout.shown).toEqual([1, 1]);
		expect(layout.rows[1]!.top).toBe(PAGE_MARGIN);
		expect(layout.totalHeight).toBeCloseTo(h + 2 * PAGE_MARGIN);
		expect(pagesInRange(layout, -1e6, 1e6)).toEqual([2, 3]);
		expect(pageAtY(layout, -1e6)).toBe(2);
		expect(pageAtY(layout, 1e6)).toBe(2);
		expect([1, 2, 3, 4].map((p) => isShown(layout, p))).toEqual([false, true, true, false]);
		// A row past the end shows the last one.
		const single = computeLayout(Array(3).fill(letter), 1, 0, { columns: 1, cover: false, row: 9 });
		expect(single.shown).toEqual([2, 2]);
		expect(pagesInRange(single, 0, 1e6)).toEqual([2, 2]);
	});

	it('names the rows next to the mounted pages, the one after them first', () => {
		const pages = Array(7).fill(letter);
		const column = computeLayout(pages, 1, 0);
		expect(pagesBeside(column, 2, 3)).toEqual([4, 1]);
		expect(pagesBeside(column, 0, 0)).toEqual([1]);
		expect(pagesBeside(column, 5, 6)).toEqual([4]);
		// Two pages to a row, the first one alone: rows 0 | 1 2 | 3 4 | 5 6.
		const book = computeLayout(pages, 1, 0, { columns: 2, cover: true, row: null });
		expect(pagesBeside(book, 1, 2)).toEqual([3, 4, 0]);
		expect(pagesBeside(book, 3, 4)).toEqual([5, 6, 1, 2]);
		// One row at a time: the rows a turn shows.
		const single = computeLayout(pages, 1, 0, { columns: 1, cover: false, row: 3 });
		expect(pagesBeside(single, ...pagesInRange(single, 0, 1e6))).toEqual([4, 2]);
		// Nothing mounted, or no pages.
		expect(pagesBeside(column, 0, -1)).toEqual([]);
		expect(pagesBeside(computeLayout([], 1, 0), 0, 0)).toEqual([]);
	});

	it("reads the document's own page layout", () => {
		expect(documentMode('SinglePage')).toEqual({ mode: 'singlePage', cover: false });
		expect(documentMode('OneColumn')).toEqual({ mode: 'singlePageContinuous', cover: false });
		expect(documentMode('TwoPageLeft')).toEqual({ mode: 'twoPage', cover: false });
		expect(documentMode('TwoPageRight')).toEqual({ mode: 'twoPage', cover: true });
		expect(documentMode('TwoColumnLeft')).toEqual({ mode: 'twoPageContinuous', cover: false });
		expect(documentMode('TwoColumnRight')).toEqual({ mode: 'twoPageContinuous', cover: true });
		expect(documentMode(null)).toBeNull();
	});

	it('normalizes rotations', () => {
		expect(normalizeRotation(-90)).toBe(270);
		expect(normalizeRotation(450)).toBe(90);
		expect(normalizeRotation(360)).toBe(0);
	});
});

describe('zoom', () => {
	it('steps through presets and clamps', () => {
		expect(stepZoom(1, 1)).toBe(1.25);
		expect(stepZoom(1, -1)).toBe(0.75);
		expect(stepZoom(1.1, -1)).toBe(1);
		expect(stepZoom(16, 1)).toBe(MAX_ZOOM);
		expect(stepZoom(0.25, -1)).toBe(MIN_ZOOM);
	});

	it('glides between zoom levels evenly in ratio, easing out', () => {
		expect(zoomBetween(1, 2, 0)).toBe(1);
		expect(zoomBetween(1, 2, 1)).toBe(2);
		expect(zoomBetween(1, 2, 1.5)).toBe(2);
		// Eased out: more than halfway (in ratio) at half the time.
		expect(zoomBetween(1, 2, 0.5)).toBeGreaterThan(Math.SQRT2);
		expect(zoomBetween(2, 1, 0.5)).toBeLessThan(Math.SQRT1_2 * 2);
		let last = 1;
		for (let t = 0.1; t <= 1; t += 0.1) {
			const z = zoomBetween(1, 4, t);
			expect(z).toBeGreaterThan(last);
			last = z;
		}
	});

	it('fits width and page, for a page as it is turned on screen', () => {
		const viewport = 612 * CSS_PX_PER_PT + 2 * PAGE_MARGIN + 1;
		expect(fitWidthZoom(letter, viewport)).toBeCloseTo(1);
		expect(fitWidthZoom(turnedSize(letter, 90), viewport)).toBeCloseTo(612 / 792, 2);
		expect(fitPageZoom(letter, 10000, 792 * CSS_PX_PER_PT + 2 * PAGE_MARGIN + 1)).toBeCloseTo(1);
		expect(fitPageZoom(turnedSize(letter, 270), 10000, 612 * CSS_PX_PER_PT + 2 * PAGE_MARGIN + 1)).toBeCloseTo(1);
		// The fitted page never needs a horizontal scrollbar.
		expect(fitWidthZoom(letter, 1000) * 612 * CSS_PX_PER_PT + 2 * PAGE_MARGIN).toBeLessThan(1000);
	});

	it('fits two pages side by side, and the row they make fits the view', () => {
		const viewport = 2 * 612 * CSS_PX_PER_PT + PAGE_GAP + 2 * PAGE_MARGIN + 1;
		expect(fitWidthZoom(letter, viewport, 2)).toBeCloseTo(1);
		expect(fitPageZoom(letter, viewport, 10000, 2)).toBeCloseTo(1);
		const zoom = fitWidthZoom(letter, 1000, 2);
		const layout = computeLayout([letter, letter], zoom, 0, { columns: 2, cover: false, row: null });
		expect(contentWidth(layout, 1000)).toBe(1000);
	});

	it('renders at the scale the page is shown at, to the thousandth', () => {
		expect(renderScale(1, 1)).toBe(1.333);
		expect(renderScale(1, 1.25)).toBe(1.667);
		expect(renderScale(1, 1.5)).toBe(2);
		expect(renderScale(1, 2)).toBe(renderScale(2, 1));
		expect(renderScale(0.1, 0.0001)).toBe(0.001);
	});

	it('shows an image of the shown scale one to one, and others stretched', () => {
		// At the page's own scale, an image pixel is a device pixel.
		expect(cssPerImagePixel(renderScale(1.1, 1.25), 1.1, 1.25)).toBe(0.8);
		expect(cssPerImagePixel(renderScale(1, 1), 1, 1)).toBe(1);
		// An image of another zoom, or an underlay, spans the page.
		expect(cssPerImagePixel(renderScale(1, 1), 1.2, 1)).toBeCloseTo((1.2 * CSS_PX_PER_PT) / 1.333);
		expect(cssPerImagePixel(1, 2, 1)).toBeCloseTo(2 * CSS_PX_PER_PT);
	});

	it('snaps to whole device pixels', () => {
		expect(snapToDevice(10.3, 1)).toBe(10);
		expect(snapToDevice(10.3, 1.25)).toBe(10.4);
		expect(snapToDevice(10.5, 2)).toBe(10.5);
		expect(snapToDevice(10.3, 0)).toBe(10);
	});

	it('follows a touchpad pinch exactly and steps a wheel notch', () => {
		// Chromium sends a pinch to scale s as deltaY = -100 ln(s).
		expect(wheelZoomFactor(-100 * Math.log(1.1), 0)).toBeCloseTo(1.1);
		expect(wheelZoomFactor(-100 * Math.log(0.95), 0)).toBeCloseTo(0.95);
		expect(wheelZoomFactor(-120, 0)).toBeCloseTo(1.24, 2);
		expect(wheelZoomFactor(100, 0)).toBeLessThan(1);
		// Notches glide; pinches follow the fingers.
		expect(isWheelNotch(-120, 0)).toBe(true);
		expect(isWheelNotch(3, 1)).toBe(true);
		expect(isWheelNotch(-100 * Math.log(1.1), 0)).toBe(false);
		expect(wheelZoomFactor(-3, 1)).toBeCloseTo(wheelZoomFactor(-99, 0));
	});

	it('turns a pinch passed on from Rust into the Ctrl+wheel steps of a pinch', () => {
		const zoom = (deltas: number[]) => deltas.reduce((z, d) => z * wheelZoomFactor(d, 0), 1);
		expect(zoom(pinchDeltas(1, 1.1))).toBeCloseTo(1.1);
		expect(zoom(pinchDeltas(1.2, 0.9))).toBeCloseTo(0.75);
		// A large jump between two events is split, so it still follows the fingers.
		const deltas = pinchDeltas(1, 3);
		expect(deltas.length).toBeGreaterThan(1);
		expect(deltas.every((d) => !isWheelNotch(d, 0))).toBe(true);
		expect(zoom(deltas)).toBeCloseTo(3);
		expect(pinchDeltas(1, 1)).toEqual([]);
		expect(pinchDeltas(0, 1)).toEqual([]);
	});

	it('rounds pixel sizes like MuPDF', () => {
		expect(pixelSize(letter, 1)).toEqual({ width: 612, height: 792 });
		expect(pixelSize({ width: 100.4, height: 10 }, 1)).toEqual({ width: 101, height: 10 });
	});

	it('draws a large page from tiles over a smaller image of the whole page', () => {
		// 612 x 792 pt: 1.9 megapixels at 2 px per point, 31 at 8.
		expect(isTiled(letter, 2)).toBe(false);
		expect(wholePageScale(letter, 2)).toBe(2);
		expect(isTiled(letter, 8)).toBe(true);
		const underlay = wholePageScale(letter, 8);
		expect(underlay).toBe(1.759);
		const { width, height } = pixelSize(letter, underlay);
		expect(width * height).toBeLessThan(1_510_000);
	});
});

describe('page box', () => {
	const labels = ['i', 'ii', 'iii', '1', '2', 'A-1'];

	it('prefers labels, then page numbers', () => {
		expect(resolvePageInput('iii', labels, 6)).toBe(2);
		expect(resolvePageInput('III', labels, 6)).toBe(2);
		expect(resolvePageInput('a-1', labels, 6)).toBe(5);
		expect(resolvePageInput('2', labels, 6)).toBe(4); // the label "2", not page 2
		expect(resolvePageInput('6', labels, 6)).toBe(5); // no label "6": physical page 6
		expect(resolvePageInput('7', labels, 6)).toBeNull();
		expect(resolvePageInput('x', labels, 6)).toBeNull();
		expect(resolvePageInput(' 3 ', null, 6)).toBe(2);
		expect(resolvePageInput('', null, 6)).toBeNull();
	});

	it('shows labels with the physical position', () => {
		expect(pagePosition(3, 312, ['i', 'ii', 'iii', 'iv'])).toBe('iv (4 of 312)');
		expect(pagePosition(3, 312, null)).toBe('4 of 312');
		expect(pagePosition(3, 312, ['1', '2', '3', '4'])).toBe('4 of 312');
		expect(pageOf(3, 312, ['i', 'ii', 'iii', 'iv'])).toBe('(4 of 312)');
		expect(pageOf(3, 312, null)).toBe('of 312');
		expect(pageOf(3, 312, ['1', '2', '3', '4'])).toBe('of 312');
		expect(pageBoxText(0, labels)).toBe('i');
		expect(pageBoxText(0, null)).toBe('1');
	});

	it('is empty on a page without a label, and still says where the page is', () => {
		// Pages 3 and 4 are in a range of style "None" without a prefix.
		const some = ['i', 'ii', '', '', '1'];
		expect(pageBoxText(2, some)).toBe('');
		expect(pageOf(2, 5, some)).toBe('(3 of 5)');
		expect(pagePosition(2, 5, some)).toBe('3 of 5');
		// A sentence names such a page by its number.
		expect(pageName(2, some)).toBe('3');
		expect(pageName(1, some)).toBe('ii');
		expect(pageName(2, null)).toBe('3');
		// Its number still reaches it from the page box.
		expect(resolvePageInput('3', some, 5)).toBe(2);
	});
});

describe('navigation history', () => {
	it('goes back and forward, and a new jump clears forward', () => {
		const h = new NavHistory();
		h.push({ page: 0, offset: 0 });
		h.push({ page: 10, offset: 0.5 });
		expect(h.goBack({ page: 20, offset: 0 })).toEqual({ page: 10, offset: 0.5 });
		expect(h.goForward({ page: 10, offset: 0.5 })).toEqual({ page: 20, offset: 0 });
		h.goBack({ page: 20, offset: 0 });
		h.push({ page: 10, offset: 0.5 });
		expect(h.canGoForward).toBe(false);
	});
});

// Two lines in block 0 and one in block 1; 10-pt wide characters.
function line(text: string, y: number, block: number, x = 0) {
	const boxes: number[] = [];
	Array.from(text).forEach((_, i) => boxes.push(x + i * 10, y, x + i * 10 + 10, y + 12));
	return {
		text,
		boxes,
		bbox: [x, y, x + Array.from(text).length * 10, y + 12] as [number, number, number, number],
		block,
		vertical: false
	};
}

function page(index: number, lines: ReturnType<typeof line>[]): TextGeometry {
	const text: PageText = { page: index, revision: 0, lines, links: [] };
	return prepareText(text);
}

describe('selection', () => {
	const p0 = page(0, [line('Hello world', 0, 0), line('second line', 20, 0), line('Next para', 50, 1)]);
	const p1 = page(1, [line('Page two', 0, 0)]);

	it('hit-tests carets between characters', () => {
		expect(hitTest(p0, 26, 5, false)).toEqual({ page: 0, line: 0, offset: 3 });
		expect(hitTest(p0, 1000, 5, false)).toBeNull();
		expect(hitTest(p0, 1000, 5, true)).toEqual({ page: 0, line: 0, offset: 11 });
		expect(hitTest(p0, 5, 22, false)).toEqual({ page: 0, line: 1, offset: 0 });
	});

	it('works on text running in other directions', () => {
		// A line running top to bottom (text on a page turned 90 degrees).
		const boxes = [0, 0, 12, 10, 0, 10, 12, 20, 0, 20, 12, 30];
		const vertical = prepareText({
			page: 0,
			revision: 0,
			lines: [{ text: 'abc', boxes, bbox: [0, 0, 12, 30], block: 0, vertical: false }],
			links: []
		});
		expect(hitTest(vertical, 6, 16, false)?.offset).toBe(2);
	});

	it('builds rectangles and text across lines, blocks and pages', () => {
		const [start, end] = ordered({ page: 1, line: 0, offset: 4 }, { page: 0, line: 0, offset: 6 });
		expect(selectionRects(p0, start, end)).toEqual([
			[60, 0, 110, 12],
			[0, 20, 110, 32],
			[0, 50, 90, 62]
		]);
		const texts = new Map([
			[0, p0],
			[1, p1]
		]);
		expect(selectionText(texts, start, end)).toBe('world\nsecond line\n\nNext para\nPage');
	});

	it('finds the side of the selection under the pointer', () => {
		// "world" on the first line to "second" on the next: it starts at x 60 and ends at x 60.
		const start = { page: 0, line: 0, offset: 6 };
		const end = { page: 0, line: 1, offset: 6 };
		expect(edgeAt(p0, start, end, 58, 6, 3)).toEqual({ side: 'start', vertical: false });
		expect(edgeAt(p0, start, end, 62, 26, 3)).toEqual({ side: 'end', vertical: false });
		// Not the other ends of those lines, nor further away than the reach.
		expect(edgeAt(p0, start, end, 110, 6, 3)).toBeNull();
		expect(edgeAt(p0, start, end, 0, 26, 3)).toBeNull();
		expect(edgeAt(p0, start, end, 54, 6, 3)).toBeNull();
		expect(edgeAt(p0, start, end, 60, 40, 3)).toBeNull();
		// Of two sides in reach, the closer one.
		const a = { page: 0, line: 0, offset: 2 };
		const b = { page: 0, line: 0, offset: 3 };
		expect(edgeAt(p0, a, b, 23, 6, 10)?.side).toBe('start');
		expect(edgeAt(p0, a, b, 27, 6, 10)?.side).toBe('end');
	});

	it('gives a page only the sides of the selection that are on it', () => {
		const start = { page: 0, line: 2, offset: 5 };
		const end = { page: 1, line: 0, offset: 4 };
		expect(edgeAt(p0, start, end, 50, 56, 3)?.side).toBe('start');
		// Where the selection runs off the first page and onto the second, there is no side.
		expect(edgeAt(p0, start, end, 90, 56, 3)).toBeNull();
		expect(edgeAt(p1, start, end, 0, 6, 3)).toBeNull();
		expect(edgeAt(p1, start, end, 40, 6, 3)?.side).toBe('end');
	});

	it('finds the sides of text running in other directions', () => {
		// Top to bottom, then right to left: the selection starts where its text does.
		const down = prepareText({
			page: 0,
			revision: 0,
			lines: [{ text: 'abc', boxes: [0, 0, 12, 10, 0, 10, 12, 20, 0, 20, 12, 30], bbox: [0, 0, 12, 30], block: 0, vertical: false }],
			links: []
		});
		const from = { page: 0, line: 0, offset: 1 };
		const to = { page: 0, line: 0, offset: 3 };
		expect(edgeAt(down, from, to, 6, 9, 2)).toEqual({ side: 'start', vertical: true });
		expect(edgeAt(down, from, to, 6, 31, 2)).toEqual({ side: 'end', vertical: true });
		const leftward = prepareText({
			page: 0,
			revision: 0,
			lines: [{ text: 'abc', boxes: [20, 0, 30, 12, 10, 0, 20, 12, 0, 0, 10, 12], bbox: [0, 0, 30, 12], block: 0, vertical: false }],
			links: []
		});
		expect(edgeAt(leftward, from, to, 21, 6, 2)?.side).toBe('start');
		expect(edgeAt(leftward, from, to, 1, 6, 2)?.side).toBe('end');
	});

	it('places a handle on each side, the start one at the top and the end one at the bottom', () => {
		const handles = selectionHandles(p0, { page: 0, line: 0, offset: 6 }, { page: 0, line: 1, offset: 6 });
		expect(handles).toEqual([
			{ side: 'start', vertical: false, box: [60, 0, 60, 12], knob: [60, 0] },
			{ side: 'end', vertical: false, box: [60, 20, 60, 32], knob: [60, 32] }
		]);
		// A selection that runs on to the next page has no end here.
		expect(selectionHandles(p0, { page: 0, line: 2, offset: 5 }, { page: 1, line: 0, offset: 4 }).map((h) => h.side)).toEqual([
			'start'
		]);
	});

	it('knows a selection with nothing in it', () => {
		const both = new Map([
			[0, p0],
			[1, p1]
		]);
		const at = (page: number, line: number, offset: number) => ({ page, line, offset });
		expect(selectionIsEmpty(both, at(0, 0, 4), at(0, 0, 4))).toBe(true);
		// The end of a line and the start of the next are one place, across pages too.
		expect(selectionIsEmpty(both, at(0, 0, 11), at(0, 1, 0))).toBe(true);
		expect(selectionIsEmpty(both, at(0, 2, 9), at(1, 0, 0))).toBe(true);
		expect(selectionIsEmpty(both, at(0, 0, 10), at(0, 1, 0))).toBe(false);
		// A page whose text is not known may have some.
		expect(selectionIsEmpty(new Map([[0, p0]]), at(0, 2, 9), at(1, 0, 0))).toBe(false);
	});

	describe('moving the end of the selection from the keyboard', () => {
		const texts = new Map([
			[0, p0],
			[1, p1]
		]);
		const at = (page: number, line: number, offset: number) => ({ page, line, offset });
		const move = (from: ReturnType<typeof at>, direction: 1 | -1, step: CaretStep, goal?: [number, number]) =>
			moveCaret((page) => texts.get(page) ?? null, 2, from, direction, step, goal);

		it('goes a character at a time, across lines and pages', async () => {
			expect(await move(at(0, 0, 3), 1, 'character')).toEqual(at(0, 0, 4));
			expect(await move(at(0, 0, 3), -1, 'character')).toEqual(at(0, 0, 2));
			// From the end of a line, past the first character of the next, and back.
			expect(await move(at(0, 0, 11), 1, 'character')).toEqual(at(0, 1, 1));
			expect(await move(at(0, 1, 0), -1, 'character')).toEqual(at(0, 0, 10));
			expect(await move(at(0, 2, 9), 1, 'character')).toEqual(at(1, 0, 1));
			expect(await move(at(1, 0, 0), -1, 'character')).toEqual(at(0, 2, 8));
			// Nowhere to go at the ends of the document.
			expect(await move(at(1, 0, 8), 1, 'character')).toEqual(at(1, 0, 8));
			expect(await move(at(0, 0, 0), -1, 'character')).toEqual(at(0, 0, 0));
		});

		it('goes to the end of a word forward, and to the start of one back', async () => {
			expect(await move(at(0, 0, 0), 1, 'word')).toEqual(at(0, 0, 5));
			expect(await move(at(0, 0, 5), 1, 'word')).toEqual(at(0, 0, 11));
			expect(await move(at(0, 0, 11), 1, 'word')).toEqual(at(0, 1, 6));
			expect(await move(at(0, 1, 9), -1, 'word')).toEqual(at(0, 1, 7));
			expect(await move(at(0, 1, 7), -1, 'word')).toEqual(at(0, 1, 0));
			expect(await move(at(0, 1, 0), -1, 'word')).toEqual(at(0, 0, 6));
		});

		it('goes a line down or up, keeping its place across the line', async () => {
			expect(await move(at(0, 0, 8), 1, 'line')).toEqual(at(0, 1, 8));
			expect(await move(at(0, 1, 8), -1, 'line')).toEqual(at(0, 0, 8));
			// A shorter line ends sooner; the point the keys set out from brings the place back.
			expect(await move(at(0, 1, 11), 1, 'line')).toEqual(at(0, 2, 9));
			expect(await move(at(1, 0, 8), -1, 'line', [110, 26])).toEqual(at(0, 2, 9));
			expect(await move(at(0, 2, 9), -1, 'line', [110, 26])).toEqual(at(0, 1, 11));
			expect(await move(at(0, 2, 3), 1, 'line')).toEqual(at(1, 0, 3));
			// On the first line, to its start; on the last, to its end.
			expect(await move(at(0, 0, 4), -1, 'line')).toEqual(at(0, 0, 0));
			expect(await move(at(1, 0, 3), 1, 'line')).toEqual(at(1, 0, 8));
		});

		it('goes to the end of the paragraph, then of the next one', async () => {
			expect(await move(at(0, 0, 3), 1, 'paragraph')).toEqual(at(0, 1, 11));
			expect(await move(at(0, 1, 11), 1, 'paragraph')).toEqual(at(0, 2, 9));
			expect(await move(at(0, 2, 9), 1, 'paragraph')).toEqual(at(1, 0, 8));
			expect(await move(at(0, 2, 4), -1, 'paragraph')).toEqual(at(0, 2, 0));
			expect(await move(at(0, 2, 0), -1, 'paragraph')).toEqual(at(0, 0, 0));
		});

		it('passes pages without text', async () => {
			const far = page(2, [line('Far', 0, 0)]);
			const source = (index: number) => (index === 0 ? p0 : index === 2 ? far : null);
			expect(await moveCaret(source, 3, at(0, 2, 9), 1, 'character')).toEqual(at(2, 0, 1));
			expect(await moveCaret(source, 3, at(2, 0, 0), -1, 'word')).toEqual(at(0, 2, 5));
		});
	});

	it('selects words and lines', () => {
		const [ws, we] = wordAt(p0, { page: 0, line: 0, offset: 8 });
		expect(selectionText(new Map([[0, p0]]), ws, we)).toBe('world');
		const [ls, le] = lineAt(p0, { page: 0, line: 1, offset: 3 });
		expect(selectionText(new Map([[0, p0]]), ls, le)).toBe('second line');
	});

	it('goes on by whole words or lines from a double- or triple-click', () => {
		const at = (line: number, offset: number) => ({ page: 0, line, offset });
		const texts = new Map([[0, p0]]);
		const shown = (s: { anchor: ReturnType<typeof at>; focus: ReturnType<typeof at> }) =>
			selectionText(texts, ...ordered(s.anchor, s.focus));
		// "second" double-clicked, then dragged into the middle of "line" and of "world".
		const word = wordAt(p0, at(1, 2));
		expect(shown(extendByUnit(p0, word, at(1, 9), 'word'))).toBe('second line');
		expect(shown(extendByUnit(p0, word, at(0, 8), 'word'))).toBe('world\nsecond');
		// Back within the word, the word alone; onto the space after a word, that word.
		expect(shown(extendByUnit(p0, word, at(1, 3), 'word'))).toBe('second');
		expect(extendByUnit(p0, wordAt(p0, at(0, 1)), at(0, 5), 'word').focus).toEqual(at(0, 5));
		// A triple-clicked line goes on by lines.
		const whole = lineAt(p0, at(1, 4));
		expect(shown(extendByUnit(p0, whole, at(2, 2), 'line'))).toBe('second line\n\nNext para');
		expect(shown(extendByUnit(p0, whole, at(0, 7), 'line'))).toBe('Hello world\nsecond line');
	});

	it('draws the text cursor between two characters, as high as they are', () => {
		expect(caretBar(p0, { page: 0, line: 0, offset: 3 })).toEqual([30, 0, 30, 12]);
		// At the end of a line, after its last character.
		expect(caretBar(p0, { page: 0, line: 1, offset: 11 })).toEqual([110, 20, 110, 32]);
		expect(caretBar(p0, { page: 0, line: 7, offset: 0 })).toBeNull();
	});
});

describe('page ranges', () => {
	it('parses lists and ranges into sorted unique indexes', async () => {
		const { parsePageRanges } = await import('./ranges.ts');
		expect(parsePageRanges('1-3, 7', 10)).toEqual([0, 1, 2, 6]);
		expect(parsePageRanges('3,1,3', 10)).toEqual([0, 2]);
		expect(parsePageRanges('5–6', 10)).toEqual([4, 5]);
		expect(parsePageRanges('0', 10)).toBeNull();
		expect(parsePageRanges('4-2', 10)).toBeNull();
		expect(parsePageRanges('11', 10)).toBeNull();
		expect(parsePageRanges('a', 10)).toBeNull();
		expect(parsePageRanges('', 10)).toBeNull();
	});
});
