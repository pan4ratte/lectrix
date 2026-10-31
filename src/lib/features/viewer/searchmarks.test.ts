import { describe, expect, it } from 'vitest';

import { computeLayout } from './layout.ts';
import { CURRENT_MARK_PX, MARK_PX, scrollbarTrack, searchMarks } from './searchmarks.ts';

// Three pages of 72 pt, 96 CSS pixels at zoom 1: tops 16, 124 and 232, 344 pixels in all.
const sizes = [0, 1, 2].map(() => ({ width: 72, height: 72, rotation: 0 }));
const plain = (height: number) => ({ height, start: 0, thumb: 0 });

describe('search marks', () => {
	it('puts each match where the thumb is with the match in the middle of the view', () => {
		const layout = computeLayout(sizes, 1, 0);
		const matches = [{ page: 0 }, { page: 1 }, { page: 2 }];
		// A view 100 px tall scrolls 244 px: the middle of page 2 (172 px down) is in the
		// middle of the view half way down.
		const marks = searchMarks(layout, matches, 1, () => 48, plain(100));
		expect(marks.tops).toEqual([5, 50 - MARK_PX / 2, 93]);
		expect(marks.current).toBe(50 - CURRENT_MARK_PX / 2);
		// With arrow buttons and a thumb, along the run between the thumb's middle at either
		// end; matches in the first half view are where the thumb is at the top.
		const track = { height: 100, start: 12, thumb: 11 };
		expect(searchMarks(layout, matches, -1, () => 48, track).tops).toEqual([20, 49, 78]);
		expect(searchMarks(layout, [{ page: 0 }], 0, () => 0, track).current).toBe(Math.round(17.5 - CURRENT_MARK_PX / 2));
	});

	it('stands for the whole document while one row is shown, or nothing scrolls', () => {
		const continuous = searchMarks(computeLayout(sizes, 1, 0), [{ page: 2 }], 0, () => 10, plain(344));
		const single = searchMarks(computeLayout(sizes, 1, 0, { columns: 1, cover: false, row: 1 }), [{ page: 2 }], 0, () => 10, plain(344));
		expect(single).toEqual(continuous);
		expect(continuous.current).toBe(242 - CURRENT_MARK_PX / 2);
		// Between the arrow buttons.
		const arrows = searchMarks(computeLayout(sizes, 1, 0, { columns: 1, cover: false, row: 1 }), [{ page: 2 }], 0, () => 10, { height: 344, start: 12, thumb: 11 });
		expect(arrows.current).toBe(Math.round(12 + (242 / 344) * 320 - CURRENT_MARK_PX / 2));
	});

	it('keeps one mark per pixel row, inside the strip', () => {
		const layout = computeLayout(sizes, 1, 0);
		const matches = [{ page: 0 }, { page: 0 }, { page: 0 }, { page: 1 }];
		const marks = searchMarks(layout, matches, 3, (i) => (i < 3 ? 0.1 * i : null), plain(10));
		expect(marks.tops).toEqual([0]);
		expect(marks.current).toBeNull();
		expect(searchMarks(layout, [{ page: 2 }], 0, () => 96, plain(50)).current).toBe(50 - CURRENT_MARK_PX);
	});
});

describe('scrollbar track', () => {
	it("has WebView2's arrow buttons and shortest thumb", () => {
		// As measured: a 3-page document at 100% (thumb 270 px), a 441-page one (thumb 11 px).
		const short = scrollbarTrack('windows', 10, 1523, 8442);
		expect(short.start).toBe(12);
		expect(short.thumb).toBeCloseTo(270.4, 1);
		expect(scrollbarTrack('windows', 10, 990, 612_320)).toEqual({ height: 990, start: 12, thumb: 11 });
	});

	it('has neither elsewhere, nor without a scrollbar', () => {
		expect(scrollbarTrack('macos', 0, 1000, 4000)).toEqual({ height: 1000, start: 0, thumb: 250 });
		expect(scrollbarTrack('linux', 12, 1000, 4000)).toEqual({ height: 1000, start: 0, thumb: 250 });
		expect(scrollbarTrack('windows', 0, 1000, 800)).toEqual({ height: 1000, start: 0, thumb: 1000 });
	});
});
