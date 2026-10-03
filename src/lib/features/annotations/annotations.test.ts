import { describe, expect, it } from 'vitest';

import type { Annotation } from '#lib/ipc/index.ts';
import { prepareText } from '#lib/features/viewer/selection.ts';

import {
	annotationAt,
	boxQuad,
	clampBox,
	handleAt,
	hits,
	moveBox,
	normalizeBox,
	resizeBox,
	selectionRanges
} from './geometry.ts';
import { capabilities, isMarkupTool, typeName } from './tools.ts';

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
		color: '#ffd400',
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

describe('hit-testing', () => {
	it('finds text markup by its quads, not its whole rect', () => {
		// Two lines marked; the gap between them is not part of the highlight.
		const a = annotation({ quads: [10, 10, 90, 10, 10, 20, 90, 20, 10, 40, 50, 40, 10, 50, 50, 50] });
		expect(hits(a, 50, 15, 0)).toBe(true);
		expect(hits(a, 30, 45, 0)).toBe(true);
		expect(hits(a, 50, 30, 0)).toBe(false);
		expect(hits(a, 70, 45, 0)).toBe(false);
	});

	it('finds drawings near their strokes, within tolerance and half the width', () => {
		const a = annotation({ subtype: 'Ink', kind: 'ink', ink: [[0, 0, 100, 0]], width: 4 });
		expect(hits(a, 50, 1.5, 0)).toBe(true);
		expect(hits(a, 50, 3.5, 2)).toBe(true);
		expect(hits(a, 50, 5, 0)).toBe(false);
		// A dot.
		const dot = annotation({ subtype: 'Ink', kind: 'ink', ink: [[10, 10]], width: 2 });
		expect(hits(dot, 10.5, 10.5, 0)).toBe(true);
	});

	it('picks the topmost and skips hidden annotations', () => {
		const below = annotation({ id: 1, subtype: 'Square', kind: null });
		const above = annotation({ id: 2, subtype: 'Text', kind: 'note', bounds: [40, 40, 60, 60] });
		expect(annotationAt([below, above], 50, 50, 0)?.id).toBe(2);
		expect(annotationAt([below, above], 10, 10, 0)?.id).toBe(1);
		expect(annotationAt([below, { ...above, hidden: true }], 50, 50, 0)?.id).toBe(1);
		expect(annotationAt([], 50, 50, 0)).toBeNull();
	});
});

describe('moving and resizing', () => {
	it('finds handles and resizes from them, never inside out', () => {
		const b: [number, number, number, number] = [10, 10, 110, 60];
		expect(handleAt(b, 110, 60, 3)).toBe('se');
		expect(handleAt(b, 60, 10, 3)).toBe('n');
		expect(handleAt(b, 60, 35, 3)).toBeNull();
		expect(resizeBox(b, 'se', 20, 10)).toEqual([10, 10, 130, 70]);
		expect(resizeBox(b, 'w', -5, 99)).toEqual([5, 10, 110, 60]);
		expect(resizeBox(b, 'n', 0, 100, 4)).toEqual([10, 56, 110, 60]);
	});

	it('moves and keeps boxes on the page', () => {
		expect(moveBox([0, 0, 10, 10], 5, -2)).toEqual([5, -2, 15, 8]);
		expect(clampBox([-5, 95, 5, 105], 100, 100)).toEqual([0, 90, 10, 100]);
		expect(normalizeBox([10, 20, 0, 5])).toEqual([0, 5, 10, 20]);
		// An area becomes one quad in Acrobat's corner order (view space).
		expect(boxQuad([10, 20, 0, 5])).toEqual([0, 5, 10, 5, 0, 20, 10, 20]);
	});
});

describe('selection to character ranges', () => {
	const page = (n: number) =>
		prepareText({
			page: n,
			revision: 1,
			lines: [
				{ text: 'Hello world ', boxes: Array(12 * 4).fill(0), bbox: [0, 0, 1, 1], block: 0, vertical: false },
				{ text: 'Second line', boxes: Array(11 * 4).fill(0), bbox: [0, 0, 1, 1], block: 0, vertical: false }
			]
		});

	it('covers lines from the start caret to the end caret, without edge spaces', () => {
		const texts = new Map([[0, page(0)]]);
		expect(selectionRanges(texts, { page: 0, line: 0, offset: 6 }, { page: 0, line: 1, offset: 6 })).toEqual([
			{
				page: 0,
				ranges: [
					{ line: 0, start: 6, end: 11 },
					{ line: 1, start: 0, end: 6 }
				]
			}
		]);
		// Only spaces selected: nothing to mark.
		expect(selectionRanges(texts, { page: 0, line: 0, offset: 11 }, { page: 0, line: 0, offset: 12 })).toEqual([]);
	});

	it('splits a selection across pages', () => {
		const texts = new Map([
			[0, page(0)],
			[1, page(1)]
		]);
		const out = selectionRanges(texts, { page: 0, line: 1, offset: 7 }, { page: 1, line: 0, offset: 5 });
		expect(out).toEqual([
			{ page: 0, ranges: [{ line: 1, start: 7, end: 11 }] },
			{ page: 1, ranges: [{ line: 0, start: 0, end: 5 }] }
		]);
	});
});

describe('what may change', () => {
	it('follows the type, the permission and replies', () => {
		const ink = annotation({ subtype: 'Ink', kind: 'ink' });
		expect(capabilities(ink, true)).toEqual({ restyle: true, text: true, move: true, resize: true, delete: true });
		const note = annotation({ subtype: 'Text', kind: 'note' });
		expect(capabilities(note, true).move).toBe(true);
		expect(capabilities(note, true).resize).toBe(false);
		const highlight = annotation({});
		expect(capabilities(highlight, true).move).toBe(false);
		const stamp = annotation({ subtype: 'Stamp', kind: null });
		expect(capabilities(stamp, true)).toEqual({ restyle: false, text: true, move: false, resize: false, delete: true });
		const link = annotation({ subtype: 'Link', kind: null });
		expect(capabilities(link, true).text).toBe(false);
		expect(capabilities(ink, false)).toEqual({ restyle: false, text: false, move: false, resize: false, delete: false });
		expect(capabilities({ ...ink, id: 0 }, true).delete).toBe(false);
		expect(capabilities({ ...note, replyTo: 7 }, true).text).toBe(false);
	});

	it('names tools and types', () => {
		expect(isMarkupTool('squiggly')).toBe(true);
		expect(isMarkupTool('ink')).toBe(false);
		expect(typeName('FreeText')).toBe('Text box');
		expect(typeName('Text')).toBe('Note');
		expect(typeName('Watermark')).toBe('Watermark');
	});
});
