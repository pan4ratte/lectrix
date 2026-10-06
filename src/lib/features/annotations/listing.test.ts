import { describe, expect, it } from 'vitest';

import type { Annotation } from '#lib/ipc/index.ts';
import type { Line } from '#lib/features/viewer/selection.ts';

import {
	colorKey,
	emptyFilter,
	filterCount,
	groupAnnotations,
	markedText,
	marksText,
	matchesFilter,
	matchesSearch,
	orderLabels,
	sortAnnotations
} from './listing.ts';

function annotation(patch: Partial<Annotation>): Annotation {
	return {
		id: 1,
		page: 0,
		subtype: 'Highlight',
		kind: 'highlight',
		rect: [0, 0, 100, 100],
		bounds: [0, 0, 100, 100],
		quads: [],
		ink: [],
		color: '#ffd100',
		opacity: 1,
		contents: '',
		author: 'Ada',
		created: null,
		modified: null,
		replyTo: null,
		hidden: false,
		width: null,
		fontSize: null,
		problems: [],
		...patch
	};
}

/** A line of 10-point-wide characters from x = 0, between y = 0 and 10 (+ `dy`). */
function line(text: string, dy = 0): Line {
	const chars = Array.from(text);
	const boxes = new Float32Array(chars.length * 4);
	chars.forEach((_, i) => boxes.set([i * 10, dy, i * 10 + 10, dy + 10], i * 4));
	return { chars, boxes, bbox: [0, dy, chars.length * 10, dy + 10], block: 0, dir: [1, 0] };
}

/** A quad from x0 to x1 over a line at `dy`, in Acrobat's order (ul, ur, ll, lr). */
function quad(x0: number, x1: number, dy = 0): number[] {
	return [x0, dy, x1, dy, x0, dy + 10, x1, dy + 10];
}

describe('the annotation list', () => {
	it('filters by type, colour and author: any picked value in each, all filters together', () => {
		const yellow = annotation({ id: 1 });
		const red = annotation({ id: 2, subtype: 'Underline', color: '#E52237', author: 'Bo' });
		const plain = annotation({ id: 3, subtype: 'Stamp', color: null });
		const f = emptyFilter();
		expect(filterCount(f)).toBe(0);
		expect([yellow, red, plain].every((a) => matchesFilter(a, f))).toBe(true);

		const byType = { ...f, types: new Set(['Highlight', 'Stamp']) };
		expect([yellow, red, plain].map((a) => matchesFilter(a, byType))).toEqual([true, false, true]);
		// Colours compare in lower case; an annotation without one never matches a colour.
		expect(colorKey(red)).toBe('#e52237');
		const byColour = { ...f, colors: new Set(['#e52237']) };
		expect([yellow, red, plain].map((a) => matchesFilter(a, byColour))).toEqual([false, true, false]);
		const both = { ...f, types: new Set(['Underline']), authors: new Set(['Ada']) };
		expect(filterCount(both)).toBe(2);
		expect(matchesFilter(red, both)).toBe(false);
	});

	it('searches ignoring case and spacing', () => {
		expect(matchesSearch('Check the\nfigures', 'the FIGURES')).toBe(true);
		expect(matchesSearch('Check', 'figures')).toBe(false);
		expect(matchesSearch('anything', '  ')).toBe(true);
		expect(marksText(annotation({ subtype: 'Squiggly' }))).toBe(true);
		expect(marksText(annotation({ subtype: 'Text' }))).toBe(false);
	});

	it('finds the text a markup annotation marks', () => {
		const lines = [line('The quick brown'), line('fox jumps', 20)];
		// "quick" (characters 4 to 8), then "fox" on the next line.
		expect(markedText(lines, [...quad(40, 90), ...quad(0, 30, 20)])).toBe('quick fox');
		// A turned page: the same quad written from another corner still holds its characters.
		expect(markedText(lines, [90, 10, 40, 10, 90, 0, 40, 0])).toBe('quick');
		expect(markedText(lines, [])).toBe('');
	});

	it('sorts by page, author or date, A to Z or Z to A, ties in page order', () => {
		const a = annotation({ id: 1, page: 0, author: 'Cy', created: 100, modified: 500 });
		const b = annotation({ id: 2, page: 1, author: 'Ada', created: 300, modified: null });
		const c = annotation({ id: 3, page: 2, author: '', created: 300, modified: 400 });
		const d = annotation({ id: 4, page: 2, author: 'Cy', created: 200, modified: 400 });
		const rows = [a, b, c, d];
		const ids = (list: Annotation[]) => list.map((x) => x.id);
		expect(ids(sortAnnotations(rows, 'page'))).toEqual([1, 2, 3, 4]);
		// Z to A by page: the last page first, each page's rows in their order.
		expect(ids(sortAnnotations(rows, 'page', 'desc'))).toEqual([3, 4, 2, 1]);
		// Without an author, last either way.
		expect(ids(sortAnnotations(rows, 'author'))).toEqual([2, 1, 4, 3]);
		expect(ids(sortAnnotations(rows, 'author', 'desc'))).toEqual([1, 4, 2, 3]);
		expect(ids(sortAnnotations(rows, 'created'))).toEqual([1, 4, 2, 3]);
		expect(ids(sortAnnotations(rows, 'created', 'desc'))).toEqual([2, 3, 4, 1]);
		// Without the date, last either way.
		expect(ids(sortAnnotations(rows, 'modified'))).toEqual([3, 4, 1, 2]);
		expect(ids(sortAnnotations(rows, 'modified', 'desc'))).toEqual([1, 3, 4, 2]);
		expect(orderLabels('author')).toEqual({ asc: 'A–Z', desc: 'Z–A' });
		expect(orderLabels('modified').desc).toBe('Z–A (newest first)');
	});

	it('groups rows under their page, author or day', () => {
		const a = annotation({ id: 1, page: 0, author: 'Ada', modified: 1 });
		const b = annotation({ id: 2, page: 0, author: 'Bo', modified: 1 });
		const c = annotation({ id: 3, page: 4, author: 'Ada', modified: null });
		const page = (p: number) => `Page ${p + 1}`;
		const day = () => '5 Oct 2026';
		const titles = (sort: Parameters<typeof groupAnnotations>[1]) =>
			groupAnnotations(sortAnnotations([a, b, c], sort), sort, page, day).map((g) => [g.title, g.items.map((x) => x.id)]);
		expect(titles('page')).toEqual([
			['Page 1', [1, 2]],
			['Page 5', [3]]
		]);
		expect(titles('author')).toEqual([
			['Ada', [1, 3]],
			['Bo', [2]]
		]);
		expect(titles('modified')).toEqual([
			['5 Oct 2026', [1, 2]],
			['No date', [3]]
		]);
	});
});
