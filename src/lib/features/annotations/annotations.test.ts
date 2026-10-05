import { describe, expect, it } from 'vitest';

import type { Annotation, DocumentFlags } from '#lib/ipc/index.ts';
import { prepareText } from '#lib/features/viewer/selection.ts';

import { BAR_GAP, barControls, nearEdge, placeBar, quickToolAllowed, releaseAnchor } from './bars.ts';
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
import { capabilities, isMarkupTool, shortDate, typeName } from './tools.ts';

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

describe('floating bars', () => {
	const view = { x0: 0, y0: 1000, x1: 800, y1: 1600 };
	const size = { w: 200, h: 40 };

	it('goes on the preferred side, or the other one if only that fits', () => {
		const line = { x0: 300, y0: 1300, x1: 500, y1: 1320 };
		expect(placeBar(line, size, view, 800, 'below')).toEqual({ left: 300, top: 1320 + BAR_GAP });
		expect(placeBar(line, size, view, 800, 'above')).toEqual({ left: 300, top: 1300 - BAR_GAP - 40 });
		// Too close to the bottom of the view for below; too close to the top for above.
		const low = { x0: 300, y0: 1570, x1: 500, y1: 1590 };
		expect(placeBar(low, size, view, 800, 'below').top).toBe(1570 - BAR_GAP - 40);
		const high = { x0: 300, y0: 1010, x1: 500, y1: 1030 };
		expect(placeBar(high, size, view, 800, 'above').top).toBe(1030 + BAR_GAP);
	});

	it('stays inside the view and the content horizontally', () => {
		const left = { x0: 0, y0: 1300, x1: 20, y1: 1320 };
		expect(placeBar(left, size, view, 800, 'below').left).toBe(BAR_GAP);
		const right = { x0: 780, y0: 1300, x1: 800, y1: 1320 };
		expect(placeBar(right, size, view, 800, 'below').left).toBe(800 - BAR_GAP - 200);
		// Content narrower than the view: the content's edge counts.
		expect(placeBar(right, size, view, 600, 'below').left).toBe(600 - BAR_GAP - 200);
		// Scrolled sideways: the view's edge counts.
		expect(placeBar(left, size, { ...view, x0: 100, x1: 900 }, 1200, 'below').left).toBe(100 + BAR_GAP);
	});

	it('goes above where the pointer was released, clear of the line it ended on', () => {
		const line = { x0: 100, y0: 1300, x1: 500, y1: 1320 };
		// Released on the line: centred on the pointer, above the line.
		const onLine = releaseAnchor(line, { x: 420, y: 1312 });
		expect(onLine).toEqual({ x0: 420, x1: 420, y0: 1300, y1: 1320 });
		expect(placeBar(onLine, size, view, 800, 'above')).toEqual({ left: 320, top: 1300 - BAR_GAP - 40 });
		// Released below the text: right above the pointer.
		const below = releaseAnchor(line, { x: 250, y: 1400 });
		expect(placeBar(below, size, view, 800, 'above')).toEqual({ left: 150, top: 1400 - BAR_GAP - 40 });
		// No room above, at the top of the view: below the line instead.
		const high = releaseAnchor({ x0: 100, y0: 1010, x1: 500, y1: 1030 }, { x: 300, y: 1020 });
		expect(placeBar(high, size, view, 800, 'above').top).toBe(1030 + BAR_GAP);
	});

	it('keeps the bar of a tall anchor on screen, and lets an off-screen one go', () => {
		const tall = { x0: 300, y0: 900, x1: 500, y1: 1700 };
		expect(placeBar(tall, size, view, 800, 'above').top).toBe(1000 + BAR_GAP);
		const gone = { x0: 300, y0: 200, x1: 500, y1: 220 };
		expect(placeBar(gone, size, view, 800, 'above').top).toBe(200 - BAR_GAP - 40);
	});

	it('reveals the toolbar near its own edge only', () => {
		const area = { left: 100, top: 50, right: 900, bottom: 650 };
		expect(nearEdge(area, 500, 600, 'bottom')).toBe(true);
		expect(nearEdge(area, 500, 560, 'bottom')).toBe(false);
		expect(nearEdge(area, 500, 600, 'top')).toBe(false);
		expect(nearEdge(area, 500, 100, 'top')).toBe(true);
		expect(nearEdge(area, 50, 600, 'bottom')).toBe(false);
		expect(nearEdge(area, 500, 700, 'bottom')).toBe(false);
	});

	it('offers what the document and the annotation allow', () => {
		const flags: DocumentFlags = {
			encrypted: false,
			signed: false,
			repaired: false,
			canAssemble: true,
			canAnnotate: true,
			canCopy: false
		};
		expect(quickToolAllowed('highlight', flags, true)).toBe(true);
		expect(quickToolAllowed('highlightNote', { ...flags, canAnnotate: false }, true)).toBe(false);
		expect(quickToolAllowed('copy', flags, true)).toBe(false);
		expect(quickToolAllowed('bookmark', flags, false)).toBe(false);

		expect(barControls(annotation({}), true).retype).toBe(true);
		expect(barControls(annotation({ subtype: 'Squiggly', kind: 'squiggly' }), true).retype).toBe(true);
		expect(barControls(annotation({ subtype: 'Ink', kind: 'ink' }), true).retype).toBe(false);
		expect(barControls(annotation({ subtype: 'Square', kind: null }), true)).toMatchObject({ restyle: true, retype: false });
		expect(barControls(annotation({}), false)).toMatchObject({ restyle: false, retype: false, delete: false });
		expect(barControls(annotation({ id: 0 }), true).retype).toBe(false);
	});
});

describe('dates in the list', () => {
	// Local times, so the test doesn't depend on the machine's time zone.
	const now = new Date(2026, 9, 5, 21, 42).getTime();

	it('shows the time today, the day this year, and the year before that', () => {
		expect(shortDate(new Date(2026, 9, 5, 9, 7).getTime(), now, 'en-GB')).toBe('09:07');
		expect(shortDate(new Date(2026, 9, 4, 23, 59).getTime(), now, 'en-GB')).toBe('4 Oct');
		expect(shortDate(new Date(2026, 0, 1).getTime(), now, 'en-GB')).toBe('1 Jan');
		expect(shortDate(new Date(2025, 11, 31).getTime(), now, 'en-GB')).toBe('31 Dec 2025');
		expect(shortDate(new Date(2026, 9, 5, 9, 7).getTime(), now, 'en-US')).toBe('9:07 AM');
	});
});
