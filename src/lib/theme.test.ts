import { describe, expect, it } from 'vitest';

import { contrast, luminance, mix } from './theme';

describe('contrast arithmetic', () => {
	it('computes WCAG luminance and contrast', () => {
		expect(luminance('#ffffff')).toBeCloseTo(1);
		expect(luminance('nonsense')).toBe(0);
		expect(contrast('#000000', '#ffffff')).toBeCloseTo(21);
		expect(contrast('#2f5daa', '#ffffff')).toBeGreaterThan(4.5); // the light accent
	});

	it('mixes in sRGB like CSS color-mix', () => {
		expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080');
		expect(mix('#2f5daa', '#2f5daa', 0.3)).toBe('#2f5daa');
	});
});
