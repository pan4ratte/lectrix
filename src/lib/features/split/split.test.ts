import { describe, expect, it } from 'vitest';

import { equalWidths, followerPlace, joinIndex, paneNames, resizeWidths } from './split.ts';

describe('the panes of a joined tab', () => {
	it('start with the same width', () => {
		expect(equalWidths(3)).toEqual([1 / 3, 1 / 3, 1 / 3]);
	});

	it('move only the two panes beside a divider, never below the narrowest', () => {
		// 2000 px: 500, 500 and 1000 px.
		const out = resizeWidths([0.25, 0.25, 0.5], 0, 100, 2000);
		expect(out[0]).toBeCloseTo(0.3);
		expect(out[1]).toBeCloseTo(0.2);
		expect(out[2]).toBe(0.5);
		// 1000 px for the pair, 240 the least for each.
		const far = resizeWidths([0.25, 0.25, 0.5], 0, 900, 2000);
		expect(far[0]! * 2000).toBeCloseTo(760);
		expect(far[1]! * 2000).toBeCloseTo(240);
		expect(resizeWidths([0.5, 0.5], 3, 10, 1000)).toEqual([0.5, 0.5]);
	});

	it('name a document shown twice by its place', () => {
		const names = paneNames([
			{ id: 1, name: 'report.pdf' },
			{ id: 2, name: 'draft.pdf' },
			{ id: 1, name: 'report.pdf' }
		]);
		expect(names).toEqual(['report.pdf', 'draft.pdf', 'report.pdf (2)']);
	});

	it('keep their distance when locked, within each document', () => {
		// Page 3 beside page 10; the first pane scrolls on by a page and a half.
		expect(followerPlace(10, 3, 4.5, 50)).toBeCloseTo(11.5);
		expect(followerPlace(10, 3, 0, 50)).toBe(7);
		// The follower can't go past its own ends.
		expect(followerPlace(1, 3, 0, 50)).toBe(0);
		expect(followerPlace(48, 3, 9, 50)).toBe(50);
	});

	it('take a tab dropped near an edge or a divider, not over the middle of a pane', () => {
		// 1000 px of pages: bands of 200 px at the edges.
		expect(joinIndex(150, 1000, [1])).toBe(0);
		expect(joinIndex(900, 1000, [1])).toBe(1);
		expect(joinIndex(500, 1000, [1])).toBeNull();
		// Two panes: around the divider at 500 px too, 100 px each way.
		expect(joinIndex(560, 1000, [0.5, 0.5])).toBe(1);
		expect(joinIndex(350, 1000, [0.5, 0.5])).toBeNull();
		expect(joinIndex(1200, 1000, [1])).toBeNull();
		// Narrow pages keep a band of 64 px.
		expect(joinIndex(60, 300, [1])).toBe(0);
	});
});
