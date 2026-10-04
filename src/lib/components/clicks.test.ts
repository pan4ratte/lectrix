import { describe, expect, it } from 'vitest';

import { ClickCounter, MULTI_CLICK_MS, keyPressed } from './clicks.ts';

const press = (timeStamp: number, clientX = 100, clientY = 100) => ({ timeStamp, clientX, clientY });

describe('click counting', () => {
	it('counts quick presses at one spot', () => {
		const clicks = new ClickCounter();
		expect(clicks.count(press(1000))).toBe(1);
		expect(clicks.count(press(1200))).toBe(2);
		expect(clicks.count(press(1400, 102, 101))).toBe(3);
	});

	it('starts again after a pause or a move', () => {
		const clicks = new ClickCounter();
		clicks.count(press(1000));
		expect(clicks.count(press(1000 + MULTI_CLICK_MS + 1))).toBe(1);
		expect(clicks.count(press(1700, 120, 100))).toBe(1);
		expect(clicks.count(press(1800, 120, 100))).toBe(2);
	});

	it('starts again after a key press', () => {
		const clicks = new ClickCounter();
		clicks.count(press(5000));
		keyPressed(5100);
		expect(clicks.count(press(5200))).toBe(1);
		expect(clicks.count(press(5300))).toBe(2);
	});
});
