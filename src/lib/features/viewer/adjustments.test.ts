import { describe, expect, it } from 'vitest';

import {
	ADJUST_MAX,
	ADJUST_MIN,
	NO_ADJUSTMENTS,
	SEPIA_MAX,
	isAdjusted,
	normalizeAdjustments,
	pageFilter,
	pageTone,
	sameAdjustments
} from './adjustments.ts';

describe('visual adjustments', () => {
	it('leave the pages as they are until something is adjusted', () => {
		expect(isAdjusted(NO_ADJUSTMENTS)).toBe(false);
		expect(pageFilter(NO_ADJUSTMENTS)).toBeNull();
		expect(pageTone(NO_ADJUSTMENTS)).toBeNull();
		for (const change of [{ brightness: 95 }, { contrast: 105 }, { invert: true }, { grayscale: true }, { sepia: 5 }]) {
			const a = { ...NO_ADJUSTMENTS, ...change };
			expect(isAdjusted(a), JSON.stringify(change)).toBe(true);
			// Each shows in the filter, or (sepia) in the paper's tone over the page.
			expect(pageFilter(a) ?? pageTone(a), JSON.stringify(change)).not.toBeNull();
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
		expect(pageFilter({ brightness: 70, contrast: 110, invert: true, grayscale: true, sepia: 0 })).toBe(
			'grayscale(1) invert(1) hue-rotate(180deg) brightness(0.7) contrast(1.1)'
		);
	});

	it('tone the page sepia by a layer over it, not by the filter', () => {
		// CSS's sepia() leaves white paper nearly white: the paper's tone is laid over the
		// page instead, as much of it as the slider says.
		expect(pageFilter({ ...NO_ADJUSTMENTS, sepia: 60 })).toBeNull();
		expect(pageTone({ ...NO_ADJUSTMENTS, sepia: 60 })).toBe(0.6);
		expect(pageTone({ ...NO_ADJUSTMENTS, sepia: SEPIA_MAX })).toBe(1);
		expect(pageTone({ ...NO_ADJUSTMENTS, sepia: 5 })).toBe(0.05);
		// Over whatever the filter shows: the filter has nothing of it.
		expect(pageFilter({ brightness: 70, contrast: 110, invert: true, grayscale: true, sepia: 60 })).toBe(
			'grayscale(1) invert(1) hue-rotate(180deg) brightness(0.7) contrast(1.1)'
		);
	});

	it('are the same when every one of them is', () => {
		const a = { brightness: 90, contrast: 110, invert: true, grayscale: false, sepia: 30 };
		expect(sameAdjustments(a, { ...a })).toBe(true);
		for (const change of [{ brightness: 95 }, { contrast: 100 }, { invert: false }, { grayscale: true }, { sepia: 35 }]) {
			expect(sameAdjustments(a, { ...a, ...change }), JSON.stringify(change)).toBe(false);
		}
	});

	it('are made whole when read back', () => {
		// A view stored before there were adjustments.
		expect(normalizeAdjustments(undefined)).toEqual(NO_ADJUSTMENTS);
		expect(normalizeAdjustments(null)).toEqual(NO_ADJUSTMENTS);
		expect(normalizeAdjustments({ invert: true })).toEqual({ ...NO_ADJUSTMENTS, invert: true });
		// Values out of range, or not numbers at all.
		expect(normalizeAdjustments({ brightness: 5, contrast: 400, invert: false, grayscale: true, sepia: 400 })).toEqual({
			brightness: ADJUST_MIN,
			contrast: ADJUST_MAX,
			invert: false,
			grayscale: true,
			sepia: SEPIA_MAX
		});
		expect(normalizeAdjustments({ brightness: Number.NaN, contrast: 112.4 })).toEqual({ ...NO_ADJUSTMENTS, contrast: 112 });
		// A view stored before there was sepia has none; a wild value is none or the most.
		expect(normalizeAdjustments({ brightness: 80, contrast: 100, invert: true, grayscale: false })).toEqual({
			...NO_ADJUSTMENTS,
			brightness: 80,
			invert: true
		});
		expect(normalizeAdjustments({ sepia: -20 }).sepia).toBe(0);
		expect(normalizeAdjustments({ sepia: Number.NaN }).sepia).toBe(0);
		expect(normalizeAdjustments({ sepia: 37.6 }).sepia).toBe(38);
		// A copy: changing it leaves the defaults alone.
		expect(normalizeAdjustments(NO_ADJUSTMENTS)).not.toBe(NO_ADJUSTMENTS);
	});
});
