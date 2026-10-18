import { describe, expect, it } from 'vitest';

import {
	THUMB_SIZES,
	boxBetween,
	boxesTouch,
	cellOrigin,
	cellsInRowsOf,
	clampThumb,
	fitPicture,
	gapAt,
	gridFor,
	largestThumb,
	rangeOfKeys,
	stepThumb,
	withKeys
} from './pagegrid.ts';

describe('grid', () => {
	// Five columns of 140 with 8 between them are 732 wide, from the padding at the left.
	const grid = gridFor(800, 140, 180, 8, 16);
	const LEFT = 16;

	it('fits whole cells into the width', () => {
		// 800 - 32 + 8 = 776; 776 / 148 = 5.2
		expect(grid.columns).toBe(5);
		expect(gridFor(50, 140, 180, 8, 16).columns).toBe(1);
	});

	it('starts the columns at the left, however few pages fill them', () => {
		expect(grid.left).toBe(LEFT);
		expect(gridFor(50, 140, 180, 8, 16).left).toBe(LEFT);
	});

	it('places cells row by row', () => {
		expect(cellOrigin(grid, 0)).toEqual({ x: LEFT, y: 16 });
		expect(cellOrigin(grid, 4)).toEqual({ x: LEFT + 4 * 148, y: 16 });
		expect(cellOrigin(grid, 5)).toEqual({ x: LEFT, y: 16 + 188 });
	});

	it('finds the nearest gap in the row under the pointer', () => {
		const at = (count: number, x: number, y: number) => gapAt(grid, count, x, y);
		expect(at(12, LEFT, 20)).toEqual({ gap: 0, x: LEFT, y: 16 });
		// Left of the first column is still before the first page.
		expect(at(12, 2, 20).gap).toBe(0);
		expect(at(12, LEFT + 148 * 0.6, 20).gap).toBe(1);
		// The end of a full row is drawn after its last cell.
		expect(at(12, LEFT + 148 * 5, 30)).toEqual({ gap: 5, x: LEFT + 5 * 148, y: 16 });
		expect(at(12, LEFT + 148 * 4, 16 + 188 + 10).gap).toBe(9);
		// Past the end: after the last page, in the last row.
		expect(at(12, 9999, 9999)).toEqual({ gap: 12, x: LEFT + 2 * 148, y: 16 + 2 * 188 });
		expect(at(0, 40, 40).gap).toBe(0);
	});

	it('knows the size at which one page fills the view', () => {
		// 1000 wide leaves 1000 - 32 - 16 for the square; 700 tall leaves 700 - 32 - 44.
		expect(largestThumb(1000, 700, 16, 44)).toBe(624);
		expect(largestThumb(500, 900, 16, 44)).toBe(452);
		// At that size the grid has one column.
		expect(gridFor(1000, 624 + 16, 624 + 44, 8, 16).columns).toBe(1);
	});
});

describe('selection rectangle', () => {
	const grid = gridFor(800, 140, 180, 8, 16);

	it('is the same rectangle whichever way it is drawn', () => {
		const box = { x0: 10, y0: 20, x1: 110, y1: 220 };
		expect(boxBetween({ x: 10, y: 20 }, { x: 110, y: 220 })).toEqual(box);
		expect(boxBetween({ x: 110, y: 220 }, { x: 10, y: 20 })).toEqual(box);
		expect(boxBetween({ x: 110, y: 20 }, { x: 10, y: 220 })).toEqual(box);
	});

	it('touches what it overlaps or meets, and nothing else', () => {
		const picture = { x0: 100, y0: 100, x1: 200, y1: 240 };
		expect(boxesTouch({ x0: 0, y0: 0, x1: 150, y1: 150 }, picture)).toBe(true);
		expect(boxesTouch({ x0: 120, y0: 120, x1: 130, y1: 130 }, picture)).toBe(true);
		expect(boxesTouch({ x0: 0, y0: 0, x1: 100, y1: 100 }, picture)).toBe(true);
		expect(boxesTouch({ x0: 0, y0: 0, x1: 99, y1: 300 }, picture)).toBe(false);
		expect(boxesTouch({ x0: 0, y0: 241, x1: 300, y1: 300 }, picture)).toBe(false);
	});

	it('looks only at the rows it reaches', () => {
		const rows = (y0: number, y1: number, count = 12) => cellsInRowsOf(grid, count, { x0: 0, y0, x1: 10, y1 });
		// Rows start at 16, 204 and 392; twelve pages are two full rows and two more.
		expect(rows(20, 30)).toEqual({ first: 0, last: 4 });
		expect(rows(20, 210)).toEqual({ first: 0, last: 9 });
		expect(rows(210, 9999)).toEqual({ first: 5, last: 11 });
		// In the padding above the first row, or below the last one, it reaches nothing.
		expect(rows(-50, 5)).toBeNull();
		expect(rows(600, 700)).toBeNull();
		expect(rows(20, 30, 0)).toBeNull();
	});

	it('adds pages to a selection without repeating any', () => {
		expect(withKeys([3, 1], [1, 2, 4])).toEqual([3, 1, 2, 4]);
		expect(withKeys([], [2, 1])).toEqual([2, 1]);
		expect(withKeys([5], [])).toEqual([5]);
	});
});

describe('selection', () => {
	it('selects ranges in grid order, in either direction', () => {
		const keys = [5, 1, 2, 3, 4];
		expect(rangeOfKeys(keys, 1, 3)).toEqual([1, 2, 3]);
		expect(rangeOfKeys(keys, 2, 5)).toEqual([5, 1, 2]);
		// An anchor that is gone: the page clicked alone.
		expect(rangeOfKeys(keys, 99, 2)).toEqual([2]);
		expect(rangeOfKeys(keys, 2, 99)).toEqual([]);
	});
});

describe('pictures', () => {
	it('fits a page into the square, upright or turned', () => {
		// A 300 x 400 page in a 120 px square: 90 x 120.
		expect(fitPicture(300, 400, 0, 120)).toEqual({ shownW: 90, shownH: 120, imageW: 90, imageH: 120, fit: 0.3 });
		// Turned a quarter it is shown 120 x 90; its image is still rendered upright.
		expect(fitPicture(300, 400, 90, 120)).toEqual({ shownW: 120, shownH: 90, imageW: 90, imageH: 120, fit: 0.3 });
		expect(fitPicture(300, 400, 180, 120).shownH).toBe(120);
		// A wide page is limited by its width.
		expect(fitPicture(800, 400, 0, 120)).toMatchObject({ shownW: 120, shownH: 60 });
	});
});

describe('picture sizes', () => {
	it('step through the sizes and stop at the ends', () => {
		expect(stepThumb(160, 1)).toBe(208);
		expect(stepThumb(160, -1)).toBe(128);
		expect(stepThumb(THUMB_SIZES[0], -1)).toBeNull();
		expect(stepThumb(THUMB_SIZES[THUMB_SIZES.length - 1]!, 1)).toBeNull();
	});

	it('go up to the size at which one page fills the view, and no further', () => {
		// In a view whose largest size is 700: ..., 432, 560, then 700 itself.
		expect(stepThumb(432, 1, 700)).toBe(560);
		expect(stepThumb(560, 1, 700)).toBe(700);
		expect(stepThumb(700, 1, 700)).toBeNull();
		expect(stepThumb(700, -1, 700)).toBe(560);
		// A size a few pixels under the largest is not a step of its own.
		expect(stepThumb(432, 1, 570)).toBe(570);
		// A view too small for the smallest size still has that one.
		expect(stepThumb(96, 1, 40)).toBeNull();
		expect(clampThumb(500, 40)).toBe(96);
	});

	it('step from a size between two of them, as the wheel and a pinch leave', () => {
		expect(stepThumb(140, 1)).toBe(160);
		expect(stepThumb(140, -1)).toBe(128);
		// A hair off a size is that size.
		expect(stepThumb(159.8, 1)).toBe(208);
		expect(stepThumb(160.3, -1)).toBe(128);
	});

	it('stay between the smallest and the largest', () => {
		expect(clampThumb(20)).toBe(THUMB_SIZES[0]);
		expect(clampThumb(1000, 624)).toBe(624);
		expect(clampThumb(141.5, 624)).toBe(141.5);
		expect(clampThumb(Number.NaN)).toBe(160);
		// Until the grid has measured its view, there is no largest size.
		expect(clampThumb(1000)).toBe(1000);
	});
});
