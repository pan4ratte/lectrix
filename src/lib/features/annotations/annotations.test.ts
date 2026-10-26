import { describe, expect, it } from 'vitest';

import type { Annotation, DocumentFlags } from '#lib/ipc/index.ts';
import { prepareText } from '#lib/features/viewer/selection.ts';

import {
	BAR_GAP,
	TIP_OFFSET,
	TIP_TAIL,
	barControls,
	clampPanel,
	PANEL_MIN,
	isStacked,
	menuOffset,
	menuSide,
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
	annotationsIn,
	boxQuad,
	clampBox,
	handleAt,
	hits,
	moveBox,
	normalizeBox,
	outlineQuads,
	resizeBox,
	selectionRanges,
	textBoxFrame,
	turnBox,
	turnPoint,
	unturnBox,
	unturnPoint
} from './geometry.ts';
import {
	capabilities,
	changedStyles,
	DEFAULT_COLOR,
	DEFAULT_STYLES,
	isMarkupTool,
	lineBreaks,
	PRESET_COLORS,
	presetColors,
	shortDate,
	showsComment,
	typeName,
	type DrawTool,
	type OwnStyle
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
		turn: 0,
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

describe('selecting an area', () => {
	it('selects what the area touches: markup by its quads, drawings by their strokes', () => {
		// Two lines marked; the gap between them is not part of the highlight.
		const markup = annotation({ id: 1, quads: [10, 10, 90, 10, 10, 20, 90, 20, 10, 40, 50, 40, 10, 50, 50, 50] });
		// A diagonal stroke: its corner of the bounds away from the line is empty.
		const ink = annotation({ id: 2, subtype: 'Ink', kind: 'ink', ink: [[200, 200, 300, 300]], width: 2, bounds: [199, 199, 301, 301] });
		const note = annotation({ id: 3, subtype: 'Text', kind: 'note', bounds: [400, 400, 420, 420] });
		const all = [markup, ink, note];
		const ids = (b: [number, number, number, number]) => annotationsIn(all, b).map((a) => a.id);
		expect(ids([0, 0, 500, 500])).toEqual([1, 2, 3]);
		// Between the marked lines, and to the right of the second.
		expect(ids([60, 25, 80, 55])).toEqual([]);
		expect(ids([45, 45, 46, 46])).toEqual([1]);
		// Dragged from any corner.
		expect(ids([415, 430, 410, 410])).toEqual([3]);
		// Across the stroke without a point of it inside, and in the empty corner of its bounds.
		expect(ids([240, 260, 260, 240])).toEqual([2]);
		expect(ids([280, 205, 295, 220])).toEqual([]);
		// Touching the stroke's edge (half its width) counts.
		expect(ids([301, 290, 310, 310])).toEqual([2]);
		expect(annotationsIn([{ ...note, hidden: true }], [0, 0, 500, 500])).toEqual([]);
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

describe('a page turned on screen', () => {
	// A page 600 by 800, and a point near its top-left corner.
	const [w, h] = [600, 800];

	it('shows a point elsewhere, and gives it back', () => {
		// Turned a quarter clockwise, the page's top-left corner is at the top right.
		expect(turnPoint(0, 0, 90, w, h)).toEqual([800, 0]);
		expect(turnPoint(10, 20, 90, w, h)).toEqual([780, 10]);
		expect(turnPoint(10, 20, 180, w, h)).toEqual([590, 780]);
		expect(turnPoint(10, 20, 270, w, h)).toEqual([20, 590]);
		expect(turnPoint(10, 20, 0, w, h)).toEqual([10, 20]);
		for (const turn of [0, 90, 180, 270] as const) {
			expect(unturnPoint(...turnPoint(10, 20, turn, w, h), turn, w, h)).toEqual([10, 20]);
			expect(unturnBox(turnBox([10, 20, 110, 50], turn, w, h), turn, w, h)).toEqual([10, 20, 110, 50]);
		}
		// A box 100 wide and 30 high lies on its side there.
		expect(turnBox([10, 20, 110, 50], 90, w, h)).toEqual([750, 10, 780, 110]);
	});

	it('orders an area highlight as it is seen there', () => {
		const box: [number, number, number, number] = [10, 20, 110, 50];
		// Upper-left as seen on a page turned a quarter clockwise is the page's lower-left.
		expect(boxQuad(box, 90)).toEqual([10, 50, 10, 20, 110, 50, 110, 20]);
		expect(boxQuad(box, 180)).toEqual([110, 50, 10, 50, 110, 20, 10, 20]);
		expect(boxQuad(box, 270)).toEqual([110, 20, 110, 50, 10, 20, 10, 50]);
		expect(boxQuad(box, 0)).toEqual(boxQuad(box));
	});

	it('starts a text box where its first line starts as it is read', () => {
		const box: [number, number, number, number] = [10, 20, 110, 50];
		expect(textBoxFrame(box, 0)).toEqual({ x: 10, y: 20, across: 100, down: 30 });
		// Read on a page turned a quarter clockwise, its lines run up the page.
		expect(textBoxFrame(box, 90)).toEqual({ x: 10, y: 50, across: 30, down: 100 });
		expect(textBoxFrame(box, 180)).toEqual({ x: 110, y: 50, across: 100, down: 30 });
		expect(textBoxFrame(box, 270)).toEqual({ x: 110, y: 20, across: 30, down: 100 });
		// The frame's corner is where the turned page shows the box's top-left corner.
		for (const turn of [0, 90, 180, 270] as const) {
			const shown = turnBox(box, turn, w, h);
			const frame = textBoxFrame(box, turn);
			expect(turnPoint(frame.x, frame.y, turn, w, h)).toEqual([shown[0], shown[1]]);
			expect([frame.across, frame.down]).toEqual([shown[2] - shown[0], shown[3] - shown[1]]);
		}
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
			],
			links: []
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
		// Room everywhere: below the pointer, with room for the tail under the cursor.
		const below = 1100 + TIP_OFFSET.y + TIP_TAIL.length;
		expect(placeTip({ x: 100, y: 1100 }, tipSize, view, 800)).toEqual({ left: 100 - TIP_OFFSET.x, top: below });
		// Near the right edge: shifted left, still below.
		expect(placeTip({ x: 700, y: 1100 }, tipSize, view, 800)).toEqual({ left: 800 - BAR_GAP - 300, top: below });
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
		// At a side: near that side at any height, and not near the other one.
		expect(nearEdge(area, 150, 350, 'left')).toBe(true);
		expect(nearEdge(area, 180, 350, 'left')).toBe(false);
		expect(nearEdge(area, 150, 350, 'right')).toBe(false);
		expect(nearEdge(area, 850, 60, 'right')).toBe(true);
		expect(nearEdge(area, 950, 350, 'right')).toBe(false);
	});

	it('opens a stacked toolbar’s menus beside it, over the pages', () => {
		expect(isStacked('top')).toBe(false);
		expect(isStacked('bottom')).toBe(false);
		expect(isStacked('left')).toBe(true);
		expect(isStacked('right')).toBe(true);
		expect(menuSide('top')).toBe('bottom');
		expect(menuSide('bottom')).toBe('bottom');
		expect(menuSide('left')).toBe('right');
		expect(menuSide('right')).toBe('left');
		expect(menuOffset('bottom', 4)).toBe(4);
		expect(menuOffset('left', 4)).toBe(8);
	});

	it('offers what the document and the annotation allow', () => {
		const flags: DocumentFlags = {
			encrypted: false,
			signed: false,
			repaired: false,
			canAssemble: true,
			canAnnotate: true,
			canCopy: false,
			printing: 'highResolution',
			pdfa: null
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
	it('start with Acrobat’s highlight yellow for every tool, and its highlight opacity', () => {
		expect(DEFAULT_COLOR).toBe('#ffd100');
		expect(DEFAULT_STYLES.highlight.opacity).toBe(0.4);
		for (const tool of ['underline', 'strikeOut', 'squiggly', 'note', 'ink', 'freeText'] as const) {
			expect(DEFAULT_STYLES[tool].opacity).toBe(1);
		}
		// The colour is the first preset, the one the 1 key picks.
		expect(PRESET_COLORS[0]?.value).toBe(DEFAULT_COLOR);
	});

	it('put yellow, green, red and blue first, with each tint under its colour', () => {
		const names = PRESET_COLORS.map((c) => c.name);
		expect(names.slice(0, 4)).toEqual(['Yellow', 'Green', 'Red', 'Blue']);
		expect(names.slice(6, 12)).toEqual(['Light yellow', 'Light green', 'Pink', 'Light blue', 'Light orange', 'Light purple']);
		expect(new Set(PRESET_COLORS.map((c) => c.value)).size).toBe(18);
	});

	it('offer the presets with the user’s own colours in the places they changed', () => {
		expect(presetColors(undefined)).toBe(PRESET_COLORS);
		expect(presetColors({})).toEqual(PRESET_COLORS);
		const own = presetColors({ 0: '#12AB34', 17: '#101010' });
		expect(own).toHaveLength(PRESET_COLORS.length);
		// A changed preset is in lower case, as annotations' colours are, and named by its value.
		expect(own[0]).toEqual({ value: '#12ab34', name: '#12AB34' });
		expect(own[17]).toEqual({ value: '#101010', name: '#101010' });
		expect(own.slice(1, 17)).toEqual(PRESET_COLORS.slice(1, 17));
		// The default's own colour, a place the panel doesn't have and a value that is no
		// colour change nothing.
		expect(presetColors({ 0: PRESET_COLORS[0]!.value.toUpperCase(), 40: '#123456', 3: 'red' })).toEqual(PRESET_COLORS);
	});

	it('remember only what differs from the defaults', () => {
		const styles = structuredClone(DEFAULT_STYLES) as Record<DrawTool, OwnStyle>;
		expect(changedStyles(styles)).toEqual({});
		styles.underline = { ...styles.underline, opacity: 0.5 };
		styles.ink = { ...styles.ink, width: 5 };
		expect(changedStyles(styles)).toEqual({ underline: { opacity: 0.5 }, ink: { width: 5 } });
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
		// Base on the left side's border, centred level with the annotation's centre (-100, 60).
		expect(t.e1).toEqual({ x: 0.5, y: 60 - TAIL.width / 2 });
		expect(t.e2).toEqual({ x: 0.5, y: 60 + TAIL.width / 2 });
		expect(t.tip.y).toBeCloseTo(60);
		expect(t.tip.x).toBeCloseTo(-TAIL.length);
		// Near a corner the base stays clear of it and the point leans to the centre.
		const low = tailShape(288, 200, { x0: -80, y0: 195, x1: -40, y1: 199 }, 8)!;
		expect(low.e2.y).toBe(200 - 8);
		const mid = { x: 0.5, y: (low.e1.y + low.e2.y) / 2 };
		const cross = (low.tip.x - mid.x) * (197 - mid.y) - (low.tip.y - mid.y) * (-60 - mid.x);
		expect(Math.abs(cross)).toBeLessThan(1e-6);
	});

	it('comes out of a corner for an annotation off it, and stops short of a close annotation', () => {
		const t = tailShape(288, 200, { x0: -80, y0: -70, x1: -40, y1: -50 }, 8)!;
		expect(t.e1).toEqual({ x: 12, y: 0.5 });
		expect(t.e2).toEqual({ x: 0.5, y: 12 });
		// Each side leaves its own side of the border along it, and the fill covers the
		// rounded corner between them.
		expect(t.a1.y).toBe(0.5);
		expect(t.a1.x).toBeLessThan(t.e1.x);
		expect(t.a2.x).toBe(0.5);
		expect(t.a2.y).toBeLessThan(t.e2.y);
		expect(t.i1).toEqual({ x: 12, y: 2 });
		expect(t.i2).toEqual({ x: 2, y: 12 });
		expect(t.tip.x).toBeLessThan(0);
		expect(t.tip.y).toBeLessThan(0);
		// 8 px from the panel: the point stops 2 px short of it, not on it.
		const near = tailShape(288, 200, { x0: 296, y0: 90, x1: 400, y1: 110 }, 8)!;
		expect(near.tip.x).toBeCloseTo(296 - 2);
		// Over the annotation's centre: no tail.
		expect(tailShape(288, 200, { x0: 100, y0: 50, x1: 200, y1: 60 }, 8)).toBeNull();
	});

	it('leaves the border along it, as its continuation, and curves its sides in towards its axis', () => {
		const t = tailShape(288, 200, { x0: 50, y0: -60, x1: 150, y1: -40 }, 8)!;
		// Up from the top side: the base on the border's middle, the fill a little inside it.
		expect(t.e1).toEqual({ x: 100 - TAIL.width / 2, y: 0.5 });
		expect(t.e2).toEqual({ x: 100 + TAIL.width / 2, y: 0.5 });
		expect(t.i1).toEqual({ x: t.e1.x, y: 2 });
		expect(t.i2).toEqual({ x: t.e2.x, y: 2 });
		// Each side starts along the border, towards the tail's middle.
		expect(t.a1.y).toBe(0.5);
		expect(t.a2.y).toBe(0.5);
		expect(t.a1.x).toBeGreaterThan(t.e1.x);
		expect(t.a2.x).toBeLessThan(t.e2.x);
		// Halfway along, each side is nearer the axis (x = 100) than the straight side is.
		const at = (p0: number, p1: number, p2: number, p3: number) => (p0 + 3 * p1 + 3 * p2 + p3) / 8;
		for (const [e, a, b] of [
			[t.e1, t.a1, t.b1],
			[t.e2, t.a2, t.b2]
		] as const) {
			const curve = { x: at(e.x, a.x, b.x, t.tip.x), y: at(e.y, a.y, b.y, t.tip.y) };
			expect(curve.y).toBeLessThan(0);
			expect(Math.abs(curve.x - 100)).toBeLessThan(Math.abs((e.x + t.tip.x) / 2 - 100));
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

describe('outlineQuads', () => {
	/** An upright quad from (x0, y0) to (x1, y1), corners as Annotation.quads has them. */
	const quad = (x0: number, y0: number, x1: number, y1: number) => [x0, y0, x1, y0, x0, y1, x1, y1];
	/** An outline shape back as a box (it is upright here). */
	const box = (q: number[]) => [Math.min(q[0]!, q[6]!), Math.min(q[1]!, q[3]!), Math.max(q[2]!, q[4]!), Math.max(q[5]!, q[7]!)];

	it('keeps each line as wide as its text', () => {
		const lines = [...quad(300, 100, 500, 112), ...quad(72, 114, 500, 126), ...quad(72, 128, 180, 140)];
		const shapes = outlineQuads(lines).map(box);
		expect(shapes.map((b) => [b[0], b[2]])).toEqual([
			[300, 500],
			[72, 500],
			[72, 180]
		]);
	});

	it('closes the gaps between lines, so they make one shape', () => {
		const [a, b, c] = outlineQuads([...quad(300, 100, 500, 112), ...quad(72, 114, 500, 126), ...quad(72, 128, 180, 140)]).map(box);
		// Each goes half way to the next, and a little further.
		expect(a![3]).toBeCloseTo(113.25);
		expect(b![1]).toBeCloseTo(112.75);
		expect(b![3]).toBeCloseTo(127.25);
		expect(c![1]).toBeCloseTo(126.75);
		// The outer edges stay where the text is.
		expect(a![1]).toBe(100);
		expect(c![3]).toBe(140);
	});

	it('leaves apart lines far from each other or side by side with no width in common', () => {
		// A paragraph further down, and a line whose text ends before the next line's begins.
		const far = outlineQuads([...quad(72, 100, 500, 112), ...quad(72, 140, 500, 152)]).map(box);
		expect(far).toEqual([
			[72, 100, 500, 112],
			[72, 140, 500, 152]
		]);
		const apart = outlineQuads([...quad(300, 100, 500, 112), ...quad(72, 114, 200, 126)]).map(box);
		expect(apart).toEqual([
			[300, 100, 500, 112],
			[72, 114, 200, 126]
		]);
	});

	it('works in the text’s own directions on a turned page', () => {
		// Text running down the page (turned 90° clockwise): "down" the text is to the left.
		const turned = (x0: number, y0: number, x1: number, y1: number) => [x1, y0, x1, y1, x0, y0, x0, y1];
		const [a, b] = outlineQuads([...turned(488, 72, 500, 500), ...turned(474, 72, 486, 300)]);
		// The first line's lower edge (at its left, x 488) goes to 486.75; the second's upper
		// edge (x 486) to 487.25; along the text nothing changes.
		expect(a![4]).toBeCloseTo(486.75);
		expect(b![0]).toBeCloseTo(487.25);
		expect([a![1], a![3]]).toEqual([72, 500]);
		expect([b![1], b![3]]).toEqual([72, 300]);
	});
});
