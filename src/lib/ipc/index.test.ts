import { describe, expect, it } from 'vitest';

import { pageUrl, parseTiming } from './index';

describe('parseTiming', () => {
	it('reads the three stages', () => {
		expect(parseTiming('dl=1.50;raster=10.25;encode=3.00')).toEqual({
			displayListMs: 1.5,
			rasterMs: 10.25,
			encodeMs: 3
		});
	});

	it('rejects missing or malformed headers', () => {
		expect(parseTiming(null)).toBeNull();
		expect(parseTiming('dl=1;raster=x;encode=2')).toBeNull();
		expect(parseTiming('dl=1')).toBeNull();
	});
});

describe('pageUrl', () => {
	it('includes document, page, scale and revision', () => {
		const url = pageUrl(3, 41, 1.5, 7);
		expect(url).toMatch(/\/page\/3\/41\?scale=1\.500&rev=7$/);
	});
});
