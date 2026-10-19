import { describe, expect, it } from 'vitest';

import {
	initialPrinter,
	pagesToPrint,
	parseCopies,
	parsePrintRange,
	parseScale,
	printerLabel,
	sentMessage,
	type PagesAsked
} from './print.ts';

// Roman front matter, an arabic body, and an appendix whose labels have hyphens.
const labels = ['i', 'ii', 'iii', '1', '2', '3', 'A-1', 'A-2'];

describe('print ranges', () => {
	it('takes page numbers in the order typed', () => {
		expect(parsePrintRange('1-3, 7', null, 10)).toEqual([0, 1, 2, 6]);
		expect(parsePrintRange('7, 1', null, 10)).toEqual([6, 0]);
		expect(parsePrintRange('5–3', null, 10)).toEqual([4, 3, 2]);
		expect(parsePrintRange('2,2', null, 10)).toEqual([1, 1]);
	});

	it('runs an open end to the first or last page', () => {
		expect(parsePrintRange('8-', null, 10)).toEqual([7, 8, 9]);
		expect(parsePrintRange('-2', null, 10)).toEqual([0, 1]);
		expect(parsePrintRange('-', null, 10)).toBeNull();
	});

	it('takes labels as the page box does, hyphens and all', () => {
		expect(parsePrintRange('ii-1', labels, 8)).toEqual([1, 2, 3]);
		expect(parsePrintRange('A-1', labels, 8)).toEqual([6]);
		expect(parsePrintRange('A-1-A-2', labels, 8)).toEqual([6, 7]);
		expect(parsePrintRange('III', labels, 8)).toEqual([2]);
		// The label "2", not the second page.
		expect(parsePrintRange('2', labels, 8)).toEqual([4]);
	});

	it('refuses what names no page', () => {
		expect(parsePrintRange('', null, 10)).toBeNull();
		expect(parsePrintRange('0', null, 10)).toBeNull();
		expect(parsePrintRange('11', null, 10)).toBeNull();
		expect(parsePrintRange('1-3, x', null, 10)).toBeNull();
	});
});

describe('pages to print', () => {
	const asked = (more: Partial<PagesAsked>): PagesAsked => ({
		choice: 'all',
		rangeText: '',
		current: 0,
		subset: 'all',
		reverse: false,
		...more
	});

	it('all, the current page, or a range', () => {
		expect(pagesToPrint(asked({}), null, 3)).toEqual([0, 1, 2]);
		expect(pagesToPrint(asked({ choice: 'current', current: 4 }), null, 6)).toEqual([4]);
		expect(pagesToPrint(asked({ choice: 'range', rangeText: '2-3' }), null, 6)).toEqual([1, 2]);
		expect(pagesToPrint(asked({ choice: 'range', rangeText: 'nope' }), null, 6)).toBeNull();
	});

	it('odd and even by place among the pages chosen, and backwards', () => {
		const range = { choice: 'range' as const, rangeText: '2-7' };
		expect(pagesToPrint(asked({ ...range, subset: 'odd' }), null, 10)).toEqual([1, 3, 5]);
		expect(pagesToPrint(asked({ ...range, subset: 'even' }), null, 10)).toEqual([2, 4, 6]);
		expect(pagesToPrint(asked({ ...range, subset: 'even', reverse: true }), null, 10)).toEqual([6, 4, 2]);
		// Nothing left: one page has no even one.
		expect(pagesToPrint(asked({ choice: 'current', subset: 'even' }), null, 3)).toBeNull();
	});
});

describe('fields and texts', () => {
	it('copies and scale', () => {
		expect(parseCopies(' 3 ')).toBe(3);
		expect(parseCopies('0')).toBeNull();
		expect(parseCopies('1000')).toBeNull();
		expect(parseCopies('2.5')).toBeNull();
		expect(parseScale('150%')).toBe(150);
		expect(parseScale('5')).toBeNull();
		expect(parseScale('1001')).toBeNull();
	});

	it('names printers as people know them', () => {
		expect(printerLabel('Office Laser', true)).toBe('Office Laser (default)');
		expect(printerLabel('\\\\print-server\\Floor 3', false)).toBe('Floor 3 on print-server');
	});

	it('starts with the printer used last, else the default', () => {
		const printers = [
			{ name: 'A', default: false },
			{ name: 'B', default: true }
		];
		expect(initialPrinter(printers, 'A')).toBe('A');
		expect(initialPrinter(printers, 'Gone')).toBe('B');
		expect(initialPrinter([{ name: 'A', default: false }], null)).toBe('A');
		expect(initialPrinter([], 'A')).toBeNull();
	});

	it('says what was sent', () => {
		expect(sentMessage('report.pdf', 'Office Laser', 1)).toBe('Sent 1 page of report.pdf to Office Laser.');
		expect(sentMessage('report.pdf', 'Office Laser', 12)).toBe('Sent 12 pages of report.pdf to Office Laser.');
	});
});
