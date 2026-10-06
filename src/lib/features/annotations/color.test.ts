import { describe, expect, it } from 'vitest';

import { hexToHsv, hsvToHex, parseHex } from './color';
import { PRESET_COLORS } from './tools';

describe('parseHex', () => {
	it('takes six or three digits, with or without #, in either case', () => {
		expect(parseHex('#E52237')).toBe('#e52237');
		expect(parseHex('e52237')).toBe('#e52237');
		expect(parseHex(' #fd0 ')).toBe('#ffdd00');
		expect(parseHex('FD0')).toBe('#ffdd00');
	});

	it('refuses anything else', () => {
		for (const text of ['', '#', '#12', '#1234', '#12345', '#1234567', 'red', '#ggg000', 'rgb(1,2,3)', '##123456']) {
			expect(parseHex(text), text).toBeNull();
		}
	});
});

describe('hex and HSV', () => {
	it('knows the primaries and greys', () => {
		expect(hexToHsv('#ff0000')).toEqual({ h: 0, s: 1, v: 1 });
		expect(hexToHsv('#00ff00')).toEqual({ h: 120, s: 1, v: 1 });
		expect(hexToHsv('#0000ff')).toEqual({ h: 240, s: 1, v: 1 });
		expect(hexToHsv('#000000')).toEqual({ h: 0, s: 0, v: 0 });
		expect(hexToHsv('#ffffff')).toEqual({ h: 0, s: 0, v: 1 });
		expect(hsvToHex({ h: 300, s: 1, v: 1 })).toBe('#ff00ff');
		expect(hsvToHex({ h: 0, s: 0, v: 0.5 })).toBe('#808080');
	});

	it('round-trips every preset and a spread of colours exactly', () => {
		const colours = [...PRESET_COLORS.map((c) => c.value)];
		for (let i = 0; i < 4096; i += 37) colours.push(`#${(i * 4099).toString(16).padStart(6, '0').slice(-6)}`);
		for (const hex of colours) expect(hsvToHex(hexToHsv(hex)), hex).toBe(hex);
	});
});
