import { describe, expect, it } from 'vitest';

import { isAppError, pageUrl } from './index';

describe('pageUrl', () => {
	it('includes document, page, scale and revision', () => {
		const url = pageUrl(3, 41, 1.5, 7);
		expect(url).toMatch(/\/page\/3\/41\?scale=1\.500&rev=7$/);
	});

	it('adds tiles and the PNG format when asked', () => {
		const url = pageUrl(1, 0, 4, 2, { x: 512, y: 0, width: 512, height: 100 }, 'png');
		expect(url).toMatch(/\?scale=4\.000&rev=2&tile=512,0,512,100&fmt=png$/);
	});
});

describe('isAppError', () => {
	it('recognizes errors from commands', () => {
		expect(isAppError({ message: 'x', suggestion: null, code: 'general' })).toBe(true);
		expect(isAppError(new Error('x'))).toBe(false);
		expect(isAppError(null)).toBe(false);
	});
});
