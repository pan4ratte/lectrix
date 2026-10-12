import { describe, expect, it } from 'vitest';

import { ADJUST_MAX, ADJUST_MIN, NO_ADJUSTMENTS, isAdjusted, normalizeAdjustments, pageFilter } from './adjustments.ts';

describe('visual adjustments', () => {
	it('leave the pages as they are until something is adjusted', () => {
		expect(isAdjusted(NO_ADJUSTMENTS)).toBe(false);
		expect(pageFilter(NO_ADJUSTMENTS)).toBeNull();
		for (const change of [{ brightness: 95 }, { contrast: 105 }, { invert: true }, { grayscale: true }]) {
			const a = { ...NO_ADJUSTMENTS, ...change };
			expect(isAdjusted(a), JSON.stringify(change)).toBe(true);
			expect(pageFilter(a), JSON.stringify(change)).not.toBeNull();
		}
	});

	it('are one CSS filter, with only what is adjusted in it', () => {
		expect(pageFilter({ ...NO_ADJUSTMENTS, brightness: 80 })).toBe('brightness(0.8)');
		expect(pageFilter({ ...NO_ADJUSTMENTS, contrast: 125 })).toBe('contrast(1.25)');
		expect(pageFilter({ ...NO_ADJUSTMENTS, grayscale: true })).toBe('grayscale(1)');
		// Inverting turns the hues back, so colours keep theirs.
		expect(pageFilter({ ...NO_ADJUSTMENTS, invert: true })).toBe('invert(1) hue-rotate(180deg)');
	});

	it('put brightness and contrast after the inversion, so they act on what is seen', () => {
		expect(pageFilter({ brightness: 70, contrast: 110, invert: true, grayscale: true })).toBe(
			'grayscale(1) invert(1) hue-rotate(180deg) brightness(0.7) contrast(1.1)'
		);
	});

	it('are made whole when read back', () => {
		// A view stored before there were adjustments.
		expect(normalizeAdjustments(undefined)).toEqual(NO_ADJUSTMENTS);
		expect(normalizeAdjustments(null)).toEqual(NO_ADJUSTMENTS);
		expect(normalizeAdjustments({ invert: true })).toEqual({ ...NO_ADJUSTMENTS, invert: true });
		// Values out of range, or not numbers at all.
		expect(normalizeAdjustments({ brightness: 5, contrast: 400, invert: false, grayscale: true })).toEqual({
			brightness: ADJUST_MIN,
			contrast: ADJUST_MAX,
			invert: false,
			grayscale: true
		});
		expect(normalizeAdjustments({ brightness: Number.NaN, contrast: 112.4 })).toEqual({ ...NO_ADJUSTMENTS, contrast: 112 });
		// A copy: changing it leaves the defaults alone.
		expect(normalizeAdjustments(NO_ADJUSTMENTS)).not.toBe(NO_ADJUSTMENTS);
	});
});
