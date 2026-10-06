import { describe, expect, it } from 'vitest';

import type { Annotation, DocumentFlags } from '#lib/ipc/index.ts';
import { prepareText } from '#lib/features/viewer/selection.ts';

import {
	BAR_GAP,
	TIP_OFFSET,
	barControls,
	clampPanel,
	PANEL_MIN,
	nearEdge,
	placeBar,
	placePanel,
	placeTip,
	quickToolAllowed,
	releaseAnchor,
	resizePanel,
	TAIL,
	tailShape
} from './bars.ts';
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
import {
	capabilities,
	changedStyles,
	DEFAULT_STYLES,
	isMarkupTool,
	lineBreaks,
	PRESET_COLORS,
	shortDate,
	showsComment,
	typeName,
	type DrawTool,
	type ToolStyle
} from './tools.ts';

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
		// A reply's text can change, nothing else (ADR 0012).
		expect(capabilities({ ...note, replyTo: 7 }, true)).toEqual({ restyle: false, text: true, move: false, resize: false, delete: true });
	});

	it('shows the comment panel when selected only with a comment', () => {
		expect(showsComment(annotation({ contents: 'Check this' }))).toBe(true);
		expect(showsComment(annotation({ contents: '' }))).toBe(false);
		expect(showsComment(annotation({ contents: ' \n ' }))).toBe(false);
		// A text box's text is on the page: it gets its bar.
		expect(showsComment(annotation({ subtype: 'FreeText', kind: 'freeText', contents: 'Hi' }))).toBe(false);
		expect(showsComment(annotation({ subtype: 'Stamp', kind: null, contents: 'Approved' }))).toBe(true);
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

	it('puts a tooltip by the pointer, always wholly in view', () => {
		const tipSize = { w: 300, h: 60 };
		// Room everywhere: below and right of the pointer.
		expect(placeTip({ x: 100, y: 1100 }, tipSize, view, 800)).toEqual({ left: 100 + TIP_OFFSET.x, top: 1100 + TIP_OFFSET.y });
		// Near the right edge: shifted left, still below.
		expect(placeTip({ x: 700, y: 1100 }, tipSize, view, 800)).toEqual({ left: 800 - BAR_GAP - 300, top: 1100 + TIP_OFFSET.y });
		// Near the bottom: above the pointer.
		expect(placeTip({ x: 100, y: 1580 }, tipSize, view, 800).top).toBe(1580 - BAR_GAP - 60);
		// Content narrower than the view: its edge counts.
		expect(placeTip({ x: 550, y: 1100 }, tipSize, view, 600).left).toBe(600 - BAR_GAP - 300);
		// Taller than the view: pinned to the top edge rather than cut off at the top.
		expect(placeTip({ x: 100, y: 1300 }, { w: 300, h: 700 }, view, 800).top).toBe(1000 + BAR_GAP);
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

describe('comment line breaks', () => {
	const [CR, LF, LS, PS] = [13, 10, 0x2028, 0x2029].map((c) => String.fromCharCode(c)) as [string, string, string, string];

	it('turns every kind of line break into LF', () => {
		// Acrobat writes a lone CR; Windows text CR LF; some apps the Unicode separators.
		expect(lineBreaks(`one${CR}two${CR}${LF}three${LF}four${LS}five${PS}six`)).toBe(
			['one', 'two', 'three', 'four', 'five', 'six'].join(LF)
		);
		expect(lineBreaks(`blank${CR}${CR}line`)).toBe(`blank${LF}${LF}line`);
		expect(lineBreaks('plain')).toBe('plain');
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

describe('tool styles', () => {
	it('start with Acrobat’s colours and highlight opacity, which are preset swatches', () => {
		expect(DEFAULT_STYLES.highlight).toMatchObject({ color: '#ffd100', opacity: 0.4 });
		expect(DEFAULT_STYLES.note.color).toBe('#ffd100');
		for (const tool of ['underline', 'strikeOut', 'squiggly', 'ink', 'freeText'] as const) {
			expect(DEFAULT_STYLES[tool]).toMatchObject({ color: '#e52237', opacity: 1 });
		}
		const presets = PRESET_COLORS.map((c) => c.value);
		for (const style of Object.values(DEFAULT_STYLES)) expect(presets).toContain(style.color);
	});

	it('remember only what differs from the defaults', () => {
		const styles = structuredClone(DEFAULT_STYLES) as Record<DrawTool, ToolStyle>;
		expect(changedStyles(styles)).toEqual({});
		styles.underline = { ...styles.underline, color: '#5fd35f' };
		styles.ink = { ...styles.ink, width: 5 };
		expect(changedStyles(styles)).toEqual({ underline: { color: '#5fd35f' }, ink: { width: 5 } });
	});
});

describe('comment panel placement', () => {
	const view = { x0: 0, y0: 1000, x1: 800, y1: 1600 };
	const size = { w: 288, h: 200 };

	it('goes beside the annotation: right, else left, else below', () => {
		expect(placePanel({ x0: 100, y0: 1100, x1: 300, y1: 1120 }, size, view, 800)).toEqual({ left: 300 + BAR_GAP, top: 1100 });
		expect(placePanel({ x0: 400, y0: 1100, x1: 700, y1: 1120 }, size, view, 800)).toEqual({
			left: 400 - BAR_GAP - 288,
			top: 1100
		});
		const below = placePanel({ x0: 100, y0: 1100, x1: 700, y1: 1120 }, size, view, 800);
		expect(below.top).toBe(1120 + BAR_GAP);
	});

	it('stays inside the view while the annotation shows, and goes with it once scrolled away', () => {
		const low = { x0: 100, y0: 1550, x1: 300, y1: 1570 };
		expect(placePanel(low, size, view, 800).top).toBe(1600 - BAR_GAP - 200);
		const away = { x0: 100, y0: 2000, x1: 300, y1: 2020 };
		expect(placePanel(away, size, view, 800).top).toBe(2000);
	});

	it('is dragged only within the visible content', () => {
		expect(clampPanel({ left: -50, top: 900 }, size, view, 800)).toEqual({ left: BAR_GAP, top: 1000 + BAR_GAP });
		expect(clampPanel({ left: 700, top: 1500 }, size, view, 600)).toEqual({ left: 600 - BAR_GAP - 288, top: 1600 - BAR_GAP - 200 });
	});
});

describe('comment panel tail and resizing', () => {
	const panel = { x0: 400, y0: 100, x1: 688, y1: 300 };

	it('points straight at the centre of an annotation beside it, from opposite that centre', () => {
		const t = tailShape(288, 200, { x0: -180, y0: 50, x1: -20, y1: 70 }, 8)!;
		// Base on the left side, 2 px in, centred level with the annotation's centre (-100, 60).
		expect(t.e1).toEqual({ x: 2, y: 60 - TAIL.width / 2 });
		expect(t.e2).toEqual({ x: 2, y: 60 + TAIL.width / 2 });
		expect(t.tip.y).toBeCloseTo(60);
		expect(t.tip.x).toBeCloseTo(2 - TAIL.length - 2);
		// Near a corner the base stays clear of it and the point leans to the centre.
		const low = tailShape(288, 200, { x0: -80, y0: 195, x1: -40, y1: 199 }, 8)!;
		expect(low.e2.y).toBe(200 - 8);
		const mid = { x: 2, y: (low.e1.y + low.e2.y) / 2 };
		const cross = (low.tip.x - mid.x) * (197 - mid.y) - (low.tip.y - mid.y) * (-60 - mid.x);
		expect(Math.abs(cross)).toBeLessThan(1e-6);
	});

	it('comes out of a corner for an annotation off it, and stops short of a close annotation', () => {
		const t = tailShape(288, 200, { x0: -80, y0: -70, x1: -40, y1: -50 }, 8)!;
		expect(t.e1).toEqual({ x: 12, y: 2 });
		expect(t.e2).toEqual({ x: 2, y: 12 });
		expect(t.tip.x).toBeLessThan(0);
		expect(t.tip.y).toBeLessThan(0);
		// 8 px from the panel: the point stops 2 px short of it, not on it.
		const near = tailShape(288, 200, { x0: 296, y0: 90, x1: 400, y1: 110 }, 8)!;
		expect(near.tip.x).toBeCloseTo(296 - 2);
		// Over the annotation's centre: no tail.
		expect(tailShape(288, 200, { x0: 100, y0: 50, x1: 200, y1: 60 }, 8)).toBeNull();
	});

	it('curves its sides in towards its axis', () => {
		const t = tailShape(288, 200, { x0: 50, y0: -60, x1: 150, y1: -40 }, 8)!;
		const mid = { x: (t.e1.x + t.e2.x) / 2, y: (t.e1.y + t.e2.y) / 2 };
		const axisX = (x: number, y: number) => mid.x + ((t.tip.x - mid.x) * (y - mid.y)) / (t.tip.y - mid.y);
		// Each control point is nearer the axis than the straight side's middle.
		for (const [e, c] of [
			[t.e1, t.c1],
			[t.e2, t.c2]
		] as const) {
			const side = { x: (e.x + t.tip.x) / 2, y: (e.y + t.tip.y) / 2 };
			expect(Math.abs(c.x - axisX(c.x, c.y))).toBeLessThan(Math.abs(side.x - axisX(side.x, side.y)));
		}
	});

	it('resizes from any edge or corner, keeping the opposite ones and the minimum size, inside the view', () => {
		const view = { x0: 0, y0: 0, x1: 1000, y1: 800 };
		expect(resizePanel(panel, 'e', 50, 0, view, 1000)).toEqual({ ...panel, x1: 738 });
		expect(resizePanel(panel, 'nw', -20, -30, view, 1000)).toEqual({ x0: 380, y0: 70, x1: 688, y1: 300 });
		expect(resizePanel(panel, 'w', 200, 0, view, 1000).x0).toBe(688 - PANEL_MIN.w);
		expect(resizePanel(panel, 's', 0, 2000, view, 1000).y1).toBe(800 - BAR_GAP);
		expect(resizePanel(panel, 'n', 0, 500, view, 1000).y0).toBe(300 - PANEL_MIN.h);
	});
});
