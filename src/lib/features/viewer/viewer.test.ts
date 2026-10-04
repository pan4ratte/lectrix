import { describe, expect, it } from 'vitest';

import type { PageText } from '#lib/ipc/index.ts';

import { NavHistory } from './history.ts';
import {
	CSS_PX_PER_PT,
	PAGE_GAP,
	PAGE_MARGIN,
	computeLayout,
	currentPage,
	normalizeRotation,
	pageAtY,
	pagesInRange
} from './layout.ts';
import { pageBoxText, pagePosition, resolvePageInput } from './pagebox.ts';
import {
	hitTest,
	lineAt,
	ordered,
	prepareText,
	selectionRects,
	selectionText,
	wordAt,
	type TextGeometry
} from './selection.ts';
import {
	MAX_ZOOM,
	MIN_ZOOM,
	fitPageZoom,
	fitWidthZoom,
	pixelSize,
	renderScale,
	stepZoom,
	wheelZoomFactor
} from './zoom.ts';

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

	it('fits width and page, honoring view rotation', () => {
		const viewport = 612 * CSS_PX_PER_PT + 2 * PAGE_MARGIN + 1;
		expect(fitWidthZoom(letter, viewport, 0)).toBeCloseTo(1);
		expect(fitWidthZoom(letter, viewport, 90)).toBeCloseTo(612 / 792, 2);
		expect(fitPageZoom(letter, 10000, 792 * CSS_PX_PER_PT + 2 * PAGE_MARGIN + 1, 0)).toBeCloseTo(1);
		// The fitted page never needs a horizontal scrollbar.
		expect(fitWidthZoom(letter, 1000, 0) * 612 * CSS_PX_PER_PT + 2 * PAGE_MARGIN).toBeLessThan(1000);
	});

	it('rounds render scales up to a shared ladder', () => {
		const s = renderScale(1, 1);
		expect(s).toBeGreaterThanOrEqual(CSS_PX_PER_PT);
		expect(s / CSS_PX_PER_PT).toBeLessThan(1.05);
		expect(renderScale(1.005, 1)).toBe(renderScale(1.01, 1));
		expect(renderScale(2, 1)).toBeCloseTo(2 * renderScale(1, 1), 1);
		expect(renderScale(1, 2)).toBe(renderScale(2, 1));
	});

	it('follows a touchpad pinch exactly and steps a wheel notch', () => {
		// Chromium sends a pinch to scale s as deltaY = -100 ln(s).
		expect(wheelZoomFactor(-100 * Math.log(1.1), 0)).toBeCloseTo(1.1);
		expect(wheelZoomFactor(-100 * Math.log(0.95), 0)).toBeCloseTo(0.95);
		expect(wheelZoomFactor(-120, 0)).toBeCloseTo(1.24, 2);
		expect(wheelZoomFactor(100, 0)).toBeLessThan(1);
		expect(wheelZoomFactor(-3, 1)).toBeCloseTo(wheelZoomFactor(-99, 0));
	});

	it('rounds pixel sizes like MuPDF', () => {
		expect(pixelSize(letter, 1)).toEqual({ width: 612, height: 792 });
		expect(pixelSize({ width: 100.4, height: 10 }, 1)).toEqual({ width: 101, height: 10 });
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
		expect(pageBoxText(0, labels)).toBe('i');
		expect(pageBoxText(0, null)).toBe('1');
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
	const text: PageText = { page: index, revision: 0, lines };
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
			lines: [{ text: 'abc', boxes, bbox: [0, 0, 12, 30], block: 0, vertical: false }]
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

	it('selects words and lines', () => {
		const [ws, we] = wordAt(p0, { page: 0, line: 0, offset: 8 });
		expect(selectionText(new Map([[0, p0]]), ws, we)).toBe('world');
		const [ls, le] = lineAt(p0, { page: 0, line: 1, offset: 3 });
		expect(selectionText(new Map([[0, p0]]), ls, le)).toBe('second line');
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
