import { describe, expect, it } from 'vitest';

import { luminance, textOn } from './theme';

describe('accent contrast', () => {
	it('picks readable text on accent colors', () => {
		expect(textOn('#005fb8')).toBe('#ffffff'); // Windows default blue
		expect(textOn('#ffb900')).toBe('#000000'); // gold
		expect(textOn('#000000')).toBe('#ffffff');
		expect(luminance('#ffffff')).toBeCloseTo(1);
		expect(luminance('nonsense')).toBe(0);
	});
});
