import { describe, expect, it } from 'vitest';

import { POP_IN_MS, POP_OUT_MS, POP_SCALE, popStyle } from './pop.ts';

describe('the comment panel and the bars come and go with a slight zoom', () => {
	it('fades in from slightly smaller, and is as it is once in', () => {
		expect(popStyle(0)).toBe(`opacity: 0; scale: ${POP_SCALE}`);
		expect(popStyle(1)).toBe('opacity: 1; scale: 1');
		expect(popStyle(0.5)).toBe(`opacity: 0.5; scale: ${(POP_SCALE + 1) / 2}`);
	});

	it('stays under 150 ms, going faster than coming', () => {
		expect(POP_IN_MS).toBeLessThan(150);
		expect(POP_OUT_MS).toBeLessThan(POP_IN_MS);
	});
});
