import { describe, expect, it } from 'vitest';

import { NOTICE_MOTION_MS, NOTICE_SCALE, noticeStyle } from './notices.ts';

describe('a notification coming and going', () => {
	it('opens its place from nothing to its height, with the space above it', () => {
		expect(noticeStyle(0, 80, 8)).toContain('height: 0px; margin-top: 0px; opacity: 0;');
		expect(noticeStyle(1, 80, 8)).toContain('height: 80px; margin-top: 8px; opacity: 1;');
	});

	it('makes room ahead of the notification, which is half shown when most of it is there', () => {
		expect(noticeStyle(0.5, 80, 8)).toContain('height: 70px; margin-top: 7px; opacity: 0.5;');
	});

	it('grows around the notification’s own middle, from a little smaller', () => {
		expect(NOTICE_SCALE).toBeLessThan(1);
		expect(noticeStyle(0, 80, 8)).toContain(`scale: ${NOTICE_SCALE};`);
		expect(noticeStyle(0.5, 80, 8)).toContain('scale: 0.988;');
		expect(noticeStyle(1, 80, 8)).toContain('scale: 1;');
		// Half its height above the lower edge of its place, however far that has opened.
		for (const t of [0, 0.5, 1]) expect(noticeStyle(t, 80, 8)).toMatch(/transform-origin: 50% calc\(100% - 40px\)$/);
	});

	it('takes under 150 ms', () => {
		expect(NOTICE_MOTION_MS).toBeLessThan(150);
	});
});
