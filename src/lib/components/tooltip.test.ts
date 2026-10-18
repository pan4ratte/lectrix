import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { TIP_OFFSET, TIP_RADIUS, TIP_TAIL, tailShape } from '#lib/features/annotations/bars.ts';

import { TOOLTIP_LARGE, TOOLTIP_MARGIN, placeTooltip, tooltipAnchor, tooltipDescription } from './tooltip.ts';

describe('tooltips', () => {
	const viewport = { w: 1000, h: 700 };
	const size = { w: 120, h: 26 };
	const reach = TIP_TAIL.length + 2;

	it('go below a control, centred on it, and above it when there is no room below', () => {
		const button = { x0: 400, y0: 100, x1: 428, y1: 128 };
		expect(placeTooltip(button, size, viewport)).toEqual({ left: 414 - 60, top: 128 + reach });
		// At the bottom of the window, as the floating toolbar is.
		const low = { x0: 400, y0: 660, x1: 428, y1: 688 };
		expect(placeTooltip(low, size, viewport)).toEqual({ left: 414 - 60, top: 660 - reach - 26 });
	});

	it('go beside a control that leaves no room below or above it, as a pane’s edge', () => {
		// The left pane's edge, from the top of the window to its bottom: to its right.
		const edge = { x0: 258, y0: 36, x1: 262, y1: 700 };
		expect(placeTooltip(edge, size, viewport)).toEqual({ left: 262 + reach, top: 368 - 13 });
		const tail = tailShape(size.w, size.h, { x0: 258 - 270, y0: 36 - 355, x1: 262 - 270, y1: 700 - 355 }, TIP_RADIUS, TIP_TAIL)!;
		expect(tail.e1.x).toBe(0.5);
		expect(tail.tip.x).toBeCloseTo(-TIP_TAIL.length);
		// The right pane's edge, with no room to its right: to its left.
		expect(placeTooltip({ x0: 938, y0: 36, x1: 942, y1: 700 }, size, viewport).left).toBe(938 - reach - 120);
		// No room anywhere: over it, inside the window.
		expect(placeTooltip({ x0: 0, y0: 0, x1: 1000, y1: 700 }, size, viewport)).toEqual({ left: 440, top: 700 - TOOLTIP_MARGIN - 26 });
	});

	it('stay inside the window, their tail still on the control', () => {
		// A narrow button at the right edge.
		const close = { x0: 980, y0: 0, x1: 1000, y1: 35 };
		const at = placeTooltip(close, size, viewport);
		expect(at.left).toBe(1000 - TOOLTIP_MARGIN - 120);
		const tail = tailShape(size.w, size.h, { x0: close.x0 - at.left, y0: close.y0 - at.top, x1: close.x1 - at.left, y1: close.y1 - at.top }, TIP_RADIUS, TIP_TAIL)!;
		// The tail's base keeps clear of the corner, so it leans towards the button's middle.
		expect(tail.e2.x).toBe(size.w - TIP_RADIUS);
		expect(tail.tip.x).toBeGreaterThan((tail.e1.x + tail.e2.x) / 2);
		expect(tail.tip.y).toBeLessThan(0);
		expect(placeTooltip({ x0: 0, y0: 100, x1: 28, y1: 128 }, size, viewport).left).toBe(TOOLTIP_MARGIN);
	});

	it('have a small tail that stops short of the control', () => {
		const tail = tailShape(size.w, size.h, { x0: 46, y0: -36, x1: 74, y1: -reach }, TIP_RADIUS, TIP_TAIL)!;
		expect(tail.e1).toEqual({ x: 60 - TIP_TAIL.width / 2, y: 0.5 });
		expect(tail.e2).toEqual({ x: 60 + TIP_TAIL.width / 2, y: 0.5 });
		expect(tail.tip.x).toBeCloseTo(60);
		expect(tail.tip.y).toBeCloseTo(-TIP_TAIL.length);
	});

	it('point at the pointer along a large control, and at the middle of any other', () => {
		// A wide row: where the pointer is along it, below the row and the cursor's arrow.
		const row = { x0: 100, y0: 300, x1: 100 + TOOLTIP_LARGE + 200, y1: 332 };
		expect(tooltipAnchor(row, { x: 180, y: 304 })).toEqual({ x0: 180, y0: 300, x1: 180, y1: 332 });
		expect(tooltipAnchor(row, { x: 180, y: 330 })).toEqual({ x0: 180, y0: 300, x1: 180, y1: 330 + TIP_OFFSET.y });
		expect(tooltipAnchor(row, { x: 5000, y: 304 })).toEqual({ x0: row.x1, y0: 300, x1: row.x1, y1: 332 });
		// A tall edge: level with the pointer.
		const edge = { x0: 258, y0: 36, x1: 262, y1: 700 };
		expect(tooltipAnchor(edge, { x: 260, y: 200 })).toEqual({ x0: 258, y0: 200, x1: 262, y1: 200 + TIP_OFFSET.y });
		// Brought by the keyboard focus: the control itself.
		expect(tooltipAnchor(row, null)).toEqual(row);
		expect(tooltipAnchor(edge, null)).toEqual(edge);
		const button = { x0: 100, y0: 300, x1: 128, y1: 328 };
		expect(tooltipAnchor(button, { x: 101, y: 301 })).toEqual(button);
	});

	it('describe a control to screen readers unless they only repeat its name', () => {
		expect(tooltipDescription('Delete (Del)', 'Delete')).toBe('Delete (Del)');
		expect(tooltipDescription('C:\\Books\\a.pdf', 'a.pdf')).toBe('C:\\Books\\a.pdf');
		expect(tooltipDescription('Properties', ' Properties ')).toBeNull();
		expect(tooltipDescription('', 'Properties')).toBeNull();
	});

	it('are never the system’s: no component has a title attribute', () => {
		const src = fileURLToPath(new URL('../..', import.meta.url));
		const files = readdirSync(src, { recursive: true, encoding: 'utf8' }).filter((name) => name.endsWith('.svelte'));
		expect(files.length).toBeGreaterThan(20);
		const withTitle = files.filter((name) => /\stitle=|[{\s]title\}/.test(readFileSync(join(src, name), 'utf8')));
		expect(withTitle, 'use data-tip, which Tooltips.svelte shows').toEqual([]);
	});
});
