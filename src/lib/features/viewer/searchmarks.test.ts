import { describe, expect, it } from 'vitest';

import { computeLayout } from './layout.ts';
import { CURRENT_MARK_PX, MARK_PX, searchMarks } from './searchmarks.ts';

// Three pages of 72 pt, 96 CSS pixels at zoom 1: tops 16, 124 and 232, 344 pixels in all.
const sizes = [0, 1, 2].map(() => ({ width: 72, height: 72, rotation: 0 }));

describe('search marks', () => {
	it('puts each match as far down the strip as it is down the document', () => {
		const layout = computeLayout(sizes, 1, 0);
		const matches = [{ page: 0 }, { page: 1 }, { page: 2 }];
		const marks = searchMarks(layout, matches, 1, () => 48, 344);
		expect(marks.tops).toEqual([16 + 48 - MARK_PX / 2, 124 + 48 - MARK_PX / 2, 232 + 48 - MARK_PX / 2]);
		expect(marks.current).toBe(124 + 48 - CURRENT_MARK_PX / 2);
		// Half as tall a strip, half as far down.
		expect(searchMarks(layout, matches, -1, () => 48, 172)).toEqual({ tops: [31, 85, 139], current: null });
	});

	it('stands for the whole document while one row is shown', () => {
		const continuous = searchMarks(computeLayout(sizes, 1, 0), [{ page: 2 }], 0, () => 10, 344);
		const single = searchMarks(computeLayout(sizes, 1, 0, { columns: 1, cover: false, row: 1 }), [{ page: 2 }], 0, () => 10, 344);
		expect(single).toEqual(continuous);
	});

	it('keeps one mark per pixel row, inside the strip', () => {
		const layout = computeLayout(sizes, 1, 0);
		const matches = [{ page: 0 }, { page: 0 }, { page: 0 }, { page: 1 }];
		const marks = searchMarks(layout, matches, 3, (i) => (i < 3 ? 0.1 * i : null), 10);
		expect(marks.tops).toEqual([0]);
		expect(marks.current).toBeNull();
		expect(searchMarks(layout, [{ page: 2 }], 0, () => 96, 50).current).toBe(50 - CURRENT_MARK_PX);
	});
});
