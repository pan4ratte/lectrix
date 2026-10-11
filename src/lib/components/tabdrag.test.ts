import { describe, expect, it } from 'vitest';

import { TEAR_OFF, dragShift, dropIndex, reordered, tornOff, type Span } from './tabdrag.ts';

/** Three tabs 2 px apart: 100, 180 and 120 px wide, the first at 300. */
const TABS: Span[] = [
	{ left: 300, right: 400 },
	{ left: 402, right: 582 },
	{ left: 584, right: 704 }
];
const ROW: Span = { left: 300, right: 704 };

describe('a dragged tab follows the pointer', () => {
	it('by where it was grabbed', () => {
		// Grabbed 30 px into the first tab, the pointer now at 380: 50 px to the right.
		expect(dragShift(380, 30, TABS[0]!, ROW)).toBe(50);
		// The second tab, 20 px to the left.
		expect(dragShift(412, 30, TABS[1]!, ROW)).toBe(-20);
	});

	it('and stays within the row', () => {
		expect(dragShift(320, 30, TABS[0]!, ROW)).toBe(0);
		expect(dragShift(100, 30, TABS[0]!, ROW)).toBe(0);
		expect(dragShift(2000, 30, TABS[0]!, ROW)).toBe(304);
		expect(dragShift(100, 30, TABS[2]!, ROW)).toBe(-284);
		expect(dragShift(2000, 30, TABS[2]!, ROW)).toBe(0);
	});
});

describe('the place a dragged tab takes', () => {
	/** The tab at `index`, held `by` pixels from its place. */
	const held = (index: number, by: number): Span => ({ left: TABS[index]!.left + by, right: TABS[index]!.right + by });

	it('is its own until its leading edge passes a neighbour’s centre', () => {
		// The second tab's centre is at 492, 92 px from the first one's right edge.
		expect(dropIndex(TABS, 0, held(0, 0))).toBe(0);
		expect(dropIndex(TABS, 0, held(0, 91))).toBe(0);
		expect(dropIndex(TABS, 0, held(0, 93))).toBe(1);
		// The last tab's centre is at 644.
		expect(dropIndex(TABS, 0, held(0, 243))).toBe(1);
		expect(dropIndex(TABS, 0, held(0, 245))).toBe(2);
	});

	it('going left as well', () => {
		// The third tab's left edge (584) passes the second's centre (492), then the first's (350).
		expect(dropIndex(TABS, 2, held(2, -91))).toBe(2);
		expect(dropIndex(TABS, 2, held(2, -93))).toBe(1);
		expect(dropIndex(TABS, 2, held(2, -233))).toBe(1);
		expect(dropIndex(TABS, 2, held(2, -235))).toBe(0);
		expect(dropIndex(TABS, 1, held(1, -53))).toBe(0);
		expect(dropIndex(TABS, 1, held(1, -51))).toBe(1);
	});

	it('reaches the ends of the row, whatever the widths', () => {
		// The wide second tab against the row's right end passes the narrower last one.
		expect(dropIndex(TABS, 1, { left: ROW.right - 180, right: ROW.right })).toBe(2);
		// And against its left end, the first.
		expect(dropIndex(TABS, 1, { left: ROW.left, right: ROW.left + 180 })).toBe(0);
	});

	it('does not flip back once tabs of different widths have changed places', () => {
		// The narrow first tab passes the wide second one...
		const at = held(0, 93);
		const to = dropIndex(TABS, 0, at);
		expect(to).toBe(1);
		// ...which now sits where the narrow one was; held where it is, the order stays.
		expect(dropIndex(reordered(TABS, 0, to), to, at)).toBe(to);
		// The wide one past the narrow one, the other way.
		const wide = held(1, -53);
		expect(dropIndex(reordered(TABS, 1, 0), 0, wide)).toBe(0);
	});
});

describe('the row after a tab moved', () => {
	it('keeps widths and gaps', () => {
		expect(reordered(TABS, 0, 2)).toEqual([
			{ left: 300, right: 480 },
			{ left: 482, right: 602 },
			{ left: 604, right: 704 }
		]);
		expect(reordered(TABS, 2, 0)).toEqual([
			{ left: 300, right: 420 },
			{ left: 422, right: 522 },
			{ left: 524, right: 704 }
		]);
		expect(reordered(TABS, 1, 1)).toEqual(TABS);
		expect(reordered([], 0, 0)).toEqual([]);
	});
});

describe('a tab comes off its row', () => {
	const row = { top: 3, bottom: 31 };

	it('once the pointer is well above or below it', () => {
		expect(tornOff({ x: 400, y: 20 }, row, 1200)).toBe(false);
		expect(tornOff({ x: 400, y: 31 + TEAR_OFF }, row, 1200)).toBe(false);
		expect(tornOff({ x: 400, y: 32 + TEAR_OFF }, row, 1200)).toBe(true);
		// Above the window's top edge.
		expect(tornOff({ x: 400, y: -30 }, row, 1200)).toBe(true);
	});

	it('or out of the window at a side', () => {
		expect(tornOff({ x: -1, y: 20 }, row, 1200)).toBe(true);
		expect(tornOff({ x: 1201, y: 20 }, row, 1200)).toBe(true);
		expect(tornOff({ x: 1200, y: 20 }, row, 1200)).toBe(false);
	});
});
