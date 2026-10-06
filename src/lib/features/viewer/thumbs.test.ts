import { describe, expect, it } from 'vitest';

import {
	clampThumbWidth,
	fitWidth,
	layoutThumbs,
	renderWidth,
	shownWidth,
	stepWidth,
	thumbColumns,
	THUMB_SIDE,
	THUMB_SIZES
} from './thumbs.ts';

describe('page thumbnail sizes', () => {
	it('fit the panel, never wider than it', () => {
		expect(fitWidth(240)).toBe(240 - 2 * THUMB_SIDE);
		expect(fitWidth(40)).toBe(THUMB_SIZES[0]);
		expect(shownWidth(112, false, 240)).toBe(112);
		expect(shownWidth(112, true, 240)).toBe(208);
		expect(shownWidth(296, false, 240)).toBe(208);
		// Before the list is measured: the chosen width.
		expect(shownWidth(144, true, 0)).toBe(144);
	});

	it('step through the sizes, larger stopping at the panel width', () => {
		expect(stepWidth(112, 1, 400)).toBe(144);
		expect(stepWidth(112, -1, 400)).toBe(88);
		expect(stepWidth(64, -1, 400)).toBeNull();
		// Between steps (fitting the panel), to the nearest step that way.
		expect(stepWidth(208, -1, 240)).toBe(184);
		expect(stepWidth(184, 1, 240)).toBe(208);
		expect(stepWidth(208, 1, 240)).toBeNull();
		expect(stepWidth(456, 1, 2000)).toBeNull();
	});

	it('lay out in one column, or in a grid of as many columns as fit', () => {
		const portrait = { width: 600, height: 800 };
		const landscape = { width: 800, height: 600 };
		const pages = [portrait, landscape, portrait, portrait, portrait];

		const column = layoutThumbs(pages, 112, 400, false);
		expect(column.columns).toBe(1);
		expect(column.rows).toHaveLength(5);
		// Centred: (400 - (112 + 8)) / 2.
		expect(column.items.map((i) => i.left)).toEqual([140, 140, 140, 140, 140]);
		expect(column.items[0]).toMatchObject({ top: 12, imageH: 149, height: 171 });
		expect(column.items[1]!.top).toBe(12 + 171 + 12);

		// 120 px cells and 8 px gaps in 400 - 24: three columns, the rows as tall as their
		// tallest page, centred.
		const grid = layoutThumbs(pages, 112, 400, true);
		expect(grid.columns).toBe(3);
		expect(grid.rows.map((r) => [r.first, r.last])).toEqual([
			[0, 2],
			[3, 4]
		]);
		expect(grid.items.slice(0, 3).map((i) => i.left)).toEqual([12, 140, 268]);
		expect(grid.rows[0]!.height).toBe(171);
		expect(grid.items[3]).toMatchObject({ top: 12 + 171 + 12, left: 12, row: 1 });
		expect(grid.height).toBe(12 + 171 + 12 + 171 + 12);

		// Fitting the width leaves room for one column; before measuring, one column.
		expect(thumbColumns(fitWidth(400), 400, true)).toBe(1);
		expect(thumbColumns(64, 0, true)).toBe(1);
		expect(layoutThumbs([], 112, 400, true).height).toBe(0);
	});

	it('render at rounded widths and keep remembered ones in range', () => {
		expect(renderWidth(112)).toBe(128);
		expect(renderWidth(128)).toBe(128);
		expect(renderWidth(209)).toBe(224);
		expect(clampThumbWidth(10)).toBe(64);
		expect(clampThumbWidth(9000)).toBe(456);
		expect(clampThumbWidth(150.4)).toBe(150);
		expect(clampThumbWidth(Number.NaN)).toBe(112);
	});
});
