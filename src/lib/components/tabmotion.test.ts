import { describe, expect, it } from 'vitest';

import { tabStyle } from './tabmotion.ts';

const box = { width: 160, paddingLeft: 10.5, paddingRight: 4, gap: 1.75 };
const value = (style: string, name: string) => style.split('; ').find((p) => p.startsWith(`${name}:`))?.slice(name.length + 2);

describe('a tab coming and going', () => {
	it('is nothing, its gap closed, before it comes in', () => {
		const start = tabStyle(0, box);
		expect(value(start, 'width')).toBe('0px');
		expect(value(start, 'padding-left')).toBe('0px');
		expect(value(start, 'margin-right')).toBe('-1.75px');
		expect(value(start, 'opacity')).toBe('0');
	});

	it('ends at its own width, with its padding and gap', () => {
		const end = tabStyle(1, box);
		expect(value(end, 'width')).toBe('160px');
		expect(value(end, 'max-width')).toBe('160px');
		expect(value(end, 'padding-left')).toBe('10.5px');
		expect(value(end, 'margin-right')).toBe('0px');
		expect(value(end, 'opacity')).toBe('1');
	});

	it('makes most of its room early, so the neighbours move first', () => {
		expect(parseFloat(value(tabStyle(0.5, box), 'width') ?? '')).toBeGreaterThan(80);
	});
});
