import { describe, expect, it } from 'vitest';

import { isAppError, MAX_HIDDEN, pageUrl } from './index';

describe('pageUrl', () => {
	it('includes document, page, scale and revision, as PNG by default (ADR 0004)', () => {
		const url = pageUrl(3, 41, 1.5, 7);
		expect(url).toMatch(/\/page\/3\/41\?scale=1\.500&rev=7&fmt=png$/);
		expect(pageUrl(3, 41, 1.5, 7, undefined, 'rgba')).toMatch(/&rev=7$/);
	});

	it('adds tiles and the PNG format when asked', () => {
		const url = pageUrl(1, 0, 4, 2, { x: 512, y: 0, width: 512, height: 100 }, 'png');
		expect(url).toMatch(/\?scale=4\.000&rev=2&tile=512,0,512,100&fmt=png$/);
	});

	it('names the annotations to leave out, unless they are too many', () => {
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', [7, 41])).toMatch(/\?scale=1\.000&rev=2&hide=7,41&fmt=png$/);
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', [])).toMatch(/&rev=2&fmt=png$/);
		const many = Array.from({ length: MAX_HIDDEN + 1 }, (_, i) => i);
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', many)).toMatch(/&rev=2&fmt=png$/);
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', many.slice(1))).toContain('&hide=1,2,');
	});

	it('asks for the content or the annotations alone; the content hides nothing', () => {
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', [7], 'content')).toMatch(/&rev=2&layer=content&fmt=png$/);
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', [7], 'annots')).toMatch(/&hide=7&layer=annots&fmt=png$/);
		expect(pageUrl(1, 0, 1, 2, undefined, 'png', [], 'annots-dark')).toMatch(/&rev=2&layer=annots-dark&fmt=png$/);
	});
});

describe('isAppError', () => {
	it('recognizes errors from commands', () => {
		expect(isAppError({ message: 'x', suggestion: null, code: 'general' })).toBe(true);
		expect(isAppError(new Error('x'))).toBe(false);
		expect(isAppError(null)).toBe(false);
	});
});
