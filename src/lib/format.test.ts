import { describe, expect, it } from 'vitest';

import { fileSize, openedDate } from './format.ts';

describe('openedDate', () => {
	const now = new Date(2026, 9, 6, 20, 0).getTime();

	it('says today and yesterday with the time', () => {
		expect(openedDate(new Date(2026, 9, 6, 19, 7).getTime(), now, 'ru-RU')).toBe('Today, 19:07');
		expect(openedDate(new Date(2026, 9, 6, 0, 52).getTime(), now, 'ru-RU')).toBe('Today, 0:52');
		expect(openedDate(new Date(2026, 9, 5, 22, 16).getTime(), now, 'ru-RU')).toBe('Yesterday, 22:16');
		expect(openedDate(new Date(2026, 9, 5, 22, 16).getTime(), now, 'en-US')).toBe('Yesterday, 10:16 PM');
	});

	it('gives the day and month this year, and the year before that', () => {
		expect(openedDate(new Date(2026, 9, 4, 23, 59).getTime(), now, 'en-US')).toBe('Oct 4');
		expect(openedDate(new Date(2026, 0, 1).getTime(), now, 'en-GB')).toBe('1 Jan');
		expect(openedDate(new Date(2025, 11, 31).getTime(), now, 'en-US')).toBe('Dec 31, 2025');
	});

	it('finds yesterday across a month and a year', () => {
		const newYear = new Date(2027, 0, 1, 9, 0).getTime();
		expect(openedDate(new Date(2026, 11, 31, 23, 0).getTime(), newYear, 'en-GB')).toBe('Yesterday, 23:00');
	});
});

describe('fileSize', () => {
	it('counts as File Explorer does', () => {
		expect(fileSize(0, 'en-US')).toBe('0 bytes');
		expect(fileSize(1, 'en-US')).toBe('1 byte');
		expect(fileSize(812, 'en-US')).toBe('812 bytes');
		expect(fileSize(1024, 'en-US')).toBe('1 KB');
		expect(fileSize(1536, 'en-US')).toBe('1.5 KB');
		expect(fileSize(48 * 1024 + 300, 'en-US')).toBe('48 KB');
		expect(fileSize(3.4 * 1024 * 1024, 'en-US')).toBe('3.4 MB');
		expect(fileSize(250 * 1024 * 1024, 'en-US')).toBe('250 MB');
		expect(fileSize(2 * 1024 ** 3, 'en-US')).toBe('2 GB');
		expect(fileSize(1536, 'ru-RU')).toBe('1,5 KB');
	});
});
