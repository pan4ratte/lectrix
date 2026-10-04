import { describe, expect, it } from 'vitest';

import { ANNOTATIONS_LIMITS, RESIZE_STEP, SIDEBAR_LIMITS, clampWidth, keyResize, shownWidth } from './panes.ts';

describe('side panes', () => {
	it('keeps widths within the limits', () => {
		expect(clampWidth(100, SIDEBAR_LIMITS)).toBe(SIDEBAR_LIMITS.min);
		expect(clampWidth(9999, SIDEBAR_LIMITS)).toBe(SIDEBAR_LIMITS.max);
		expect(clampWidth(300.4, SIDEBAR_LIMITS)).toBe(300);
		expect(clampWidth(Number.NaN, ANNOTATIONS_LIMITS)).toBe(ANNOTATIONS_LIMITS.initial);
	});

	it('leaves the page room in a narrow window, but never goes below the minimum', () => {
		expect(shownWidth(480, SIDEBAR_LIMITS, 2000)).toBe(480);
		expect(shownWidth(480, SIDEBAR_LIMITS, 1000)).toBe(400);
		expect(shownWidth(480, SIDEBAR_LIMITS, 300)).toBe(SIDEBAR_LIMITS.min);
		expect(shownWidth(300, ANNOTATIONS_LIMITS, 0)).toBe(300);
	});

	it('resizes from the keyboard, the outward arrow widening', () => {
		expect(keyResize(240, 'ArrowRight', 'left', SIDEBAR_LIMITS)).toBe(240 + RESIZE_STEP);
		expect(keyResize(240, 'ArrowLeft', 'left', SIDEBAR_LIMITS)).toBe(240 - RESIZE_STEP);
		expect(keyResize(300, 'ArrowLeft', 'right', ANNOTATIONS_LIMITS)).toBe(300 + RESIZE_STEP);
		expect(keyResize(300, 'ArrowRight', 'right', ANNOTATIONS_LIMITS)).toBe(300 - RESIZE_STEP);
		expect(keyResize(SIDEBAR_LIMITS.max, 'ArrowRight', 'left', SIDEBAR_LIMITS)).toBe(SIDEBAR_LIMITS.max);
		expect(keyResize(300, 'Home', 'right', ANNOTATIONS_LIMITS)).toBe(ANNOTATIONS_LIMITS.min);
		expect(keyResize(300, 'End', 'right', ANNOTATIONS_LIMITS)).toBe(ANNOTATIONS_LIMITS.max);
		expect(keyResize(300, 'Enter', 'right', ANNOTATIONS_LIMITS)).toBeNull();
	});
});
