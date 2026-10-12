// The floating bars (section 6.5): where the bar over selected text and the bar of a
// clicked annotation go, which of their controls apply, and when the annotation toolbar
// shows in its "when the pointer is near" mode.

import type { Annotation, DocumentFlags, QuickTool, ToolbarPosition } from '#lib/ipc/index.ts';

import { MARKUP_SUBTYPES, capabilities, type Capabilities } from './tools.ts';

/** A rectangle in the viewer's scrolled content, CSS pixels. */
export interface Area {
	x0: number;
	y0: number;
	x1: number;
	y1: number;
}

/** Space between a bar and what it belongs to, and between a bar and the view's edge. */
export const BAR_GAP = 8;

/**
 * Where a bar of size `w` × `h` goes for `anchor`: on the `prefer` side, or the other one
 * if only that fits in the visible part of the content (`view`); centred on the anchor,
 * kept inside the view and the content horizontally. An anchor taller than the view keeps
 * the bar on screen while any of it is visible.
 */
export function placeBar(
	anchor: Area,
	size: { w: number; h: number },
	view: Area,
	contentWidth: number,
	prefer: 'above' | 'below'
): { left: number; top: number } {
	const above = anchor.y0 - BAR_GAP - size.h;
	const below = anchor.y1 + BAR_GAP;
	const fits = (top: number) => top >= view.y0 && top + size.h <= view.y1;
	const fitsAbove = fits(above);
	const fitsBelow = fits(below);
	let side = prefer;
	if (prefer === 'above' && !fitsAbove && fitsBelow) side = 'below';
	else if (prefer === 'below' && !fitsBelow && fitsAbove) side = 'above';
	let top = side === 'above' ? above : below;
	const visible = anchor.y1 > view.y0 && anchor.y0 < view.y1;
	if (!fitsAbove && !fitsBelow && visible) {
		top = Math.min(Math.max(top, view.y0 + BAR_GAP), view.y1 - BAR_GAP - size.h);
	}
	const minLeft = Math.max(view.x0, 0) + BAR_GAP;
	const maxLeft = Math.min(view.x1, contentWidth) - BAR_GAP - size.w;
	const centred = (anchor.x0 + anchor.x1) / 2 - size.w / 2;
	const left = Math.max(minLeft, Math.min(centred, maxLeft));
	return { left, top };
}

/** `at` moved so a panel of `size` lies wholly inside the visible content (`view`, cut at
 * `contentWidth`), `BAR_GAP` from its edges where it fits. */
export function clampPanel(
	at: { left: number; top: number },
	size: { w: number; h: number },
	view: Area,
	contentWidth: number
): { left: number; top: number } {
	const x0 = Math.max(view.x0, 0) + BAR_GAP;
	const x1 = Math.min(view.x1, contentWidth) - BAR_GAP;
	const y0 = view.y0 + BAR_GAP;
	const y1 = view.y1 - BAR_GAP;
	return {
		left: Math.max(x0, Math.min(at.left, x1 - size.w)),
		top: Math.max(y0, Math.min(at.top, y1 - size.h))
	};
}

/**
 * Where the comment panel of size `w` × `h` goes for an annotation at `anchor`: beside it,
 * right or else left, level with its top; below it (or above) where neither side has room.
 * While the annotation is visible the panel is kept inside the view; scrolled away, it goes
 * with the annotation.
 */
export function placePanel(
	anchor: Area,
	size: { w: number; h: number },
	view: Area,
	contentWidth: number
): { left: number; top: number } {
	const x0 = Math.max(view.x0, 0) + BAR_GAP;
	const x1 = Math.min(view.x1, contentWidth) - BAR_GAP;
	const visible = anchor.y1 > view.y0 && anchor.y0 < view.y1;
	const level = visible ? Math.max(view.y0 + BAR_GAP, Math.min(anchor.y0, view.y1 - BAR_GAP - size.h)) : anchor.y0;
	const right = anchor.x1 + BAR_GAP;
	const left = anchor.x0 - BAR_GAP - size.w;
	if (right + size.w <= x1) return { left: right, top: level };
	if (left >= x0) return { left, top: level };
	return placeBar(anchor, size, view, contentWidth, 'below');
}

/** A point, CSS pixels. */
export interface Point {
	x: number;
	y: number;
}

/**
 * The comment panel's tail, in the panel's own coordinates: its base runs from `e1` to `e2`
 * just inside the panel's border, its sides curve in (`c1`, `c2` are their control points)
 * to the point `tip`.
 */
export interface TailShape {
	e1: Point;
	e2: Point;
	tip: Point;
	c1: Point;
	c2: Point;
}

/** How far the tail reaches out of the panel, and how wide its base is. */
export const TAIL = { length: 12, width: 18 };
/** The same for a tooltip's tail, and the tooltip's corner radius (--radius-control). */
export const TIP_TAIL = { length: 6, width: 12 };
export const TIP_RADIUS = 6;

/**
 * The tail of a `w` × `h` panel with rounded corners (`radius`) pointing at the centre of
 * `anchor` (in the panel's coordinates). With the annotation's centre beyond one side of the
 * panel, the base is on that side, opposite the centre (clear of the corners), so the tail
 * points straight at it; beyond two sides at once, the base spans that corner and the tail
 * leans towards the centre. The point stops short of the annotation. None while the
 * annotation's centre is inside the panel. A tooltip's tail is the same shape at its own
 * `size`.
 */
export function tailShape(w: number, h: number, anchor: Area, radius: number, size = TAIL): TailShape | null {
	const target = { x: (anchor.x0 + anchor.x1) / 2, y: (anchor.y0 + anchor.y1) / 2 };
	const across = target.x < 0 ? -1 : target.x > w ? 1 : 0;
	const down = target.y < 0 ? -1 : target.y > h ? 1 : 0;
	if (across === 0 && down === 0) return null;
	const inside = 2;
	const half = size.width / 2;
	const reach = radius + half;
	const around = radius + 4;
	const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(v, hi));
	let e1: Point;
	let e2: Point;
	if (across !== 0 && down !== 0) {
		// Across the corner: one end on each of its two sides.
		const cx = across < 0 ? 0 : w;
		const cy = down < 0 ? 0 : h;
		e1 = { x: cx - across * around, y: cy - down * inside };
		e2 = { x: cx - across * inside, y: cy - down * around };
	} else if (down !== 0) {
		const y = down < 0 ? inside : h - inside;
		const x = clamp(target.x, reach, w - reach);
		e1 = { x: x - half, y };
		e2 = { x: x + half, y };
	} else {
		const x = across < 0 ? inside : w - inside;
		const y = clamp(target.y, reach, h - reach);
		e1 = { x, y: y - half };
		e2 = { x, y: y + half };
	}
	const mid = { x: (e1.x + e2.x) / 2, y: (e1.y + e2.y) / 2 };
	const toTarget = Math.hypot(target.x - mid.x, target.y - mid.y);
	const u = { x: (target.x - mid.x) / toTarget, y: (target.y - mid.y) / toTarget };
	// Short of the annotation itself, so it never covers what it marks.
	const near = { x: clamp(mid.x, anchor.x0, anchor.x1), y: clamp(mid.y, anchor.y0, anchor.y1) };
	const gap = Math.hypot(near.x - mid.x, near.y - mid.y);
	const length = clamp(gap - 2, Math.min(6, size.length), size.length + inside);
	const tip = { x: mid.x + u.x * length, y: mid.y + u.y * length };
	// Each side bends in: its control point is pulled from the side's middle towards the
	// tail's axis.
	const bend = (e: Point) => {
		const side = { x: (e.x + tip.x) / 2, y: (e.y + tip.y) / 2 };
		const axis = { x: (mid.x + tip.x) / 2, y: (mid.y + tip.y) / 2 };
		return { x: side.x + (axis.x - side.x) * 0.6, y: side.y + (axis.y - side.y) * 0.6 };
	};
	return { e1, e2, tip, c1: bend(e1), c2: bend(e2) };
}

/** A tail's two sides as an SVG path, from one end of its base to its point to the other. */
export function tailPath(t: TailShape): string {
	return `M${t.e1.x} ${t.e1.y} Q${t.c1.x} ${t.c1.y} ${t.tip.x} ${t.tip.y} Q${t.c2.x} ${t.c2.y} ${t.e2.x} ${t.e2.y}`;
}

/** An edge or corner the panel is resized from: n, s, e, w, ne, nw, se, sw. */
export type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';

/** The smallest the comment panel gets, CSS pixels. */
export const PANEL_MIN = { w: 240, h: 120 };

/**
 * The panel `start` resized from `edge` by the pointer moving (dx, dy): the opposite edges
 * stay put, it keeps at least `PANEL_MIN`, and it never grows past the visible content.
 */
export function resizePanel(start: Area, edge: ResizeEdge, dx: number, dy: number, view: Area, contentWidth: number): Area {
	const bx0 = Math.max(view.x0, 0) + BAR_GAP;
	const bx1 = Math.min(view.x1, contentWidth) - BAR_GAP;
	const by0 = view.y0 + BAR_GAP;
	const by1 = view.y1 - BAR_GAP;
	let { x0, y0, x1, y1 } = start;
	if (edge.includes('w')) x0 = Math.max(bx0, Math.min(start.x0 + dx, start.x1 - PANEL_MIN.w));
	if (edge.includes('e')) x1 = Math.min(bx1, Math.max(start.x1 + dx, start.x0 + PANEL_MIN.w));
	if (edge.includes('n')) y0 = Math.max(by0, Math.min(start.y0 + dy, start.y1 - PANEL_MIN.h));
	if (edge.includes('s')) y1 = Math.min(by1, Math.max(start.y1 + dy, start.y0 + PANEL_MIN.h));
	return { x0, y0, x1, y1 };
}

/**
 * What the bar over selected text belongs to when a pointer made the selection: the point
 * where it was released. Released on the selection's last line, the bar keeps clear of
 * that line, so it never covers the text the pointer just ended on.
 */
export function releaseAnchor(line: Area, pointer: { x: number; y: number }): Area {
	const onLine = pointer.y >= line.y0 && pointer.y <= line.y1;
	return {
		x0: pointer.x,
		x1: pointer.x,
		y0: onLine ? line.y0 : pointer.y,
		y1: onLine ? line.y1 : pointer.y
	};
}

/** How long the pointer rests on an annotation before its comment shows, until Settings
 * say otherwise (mirrors `store::DEFAULT_TOOLTIP_DELAY_MS`). */
export const DEFAULT_TIP_DELAY_MS = 300;
/** The longest delay Settings offer (mirrors `store::MAX_TOOLTIP_DELAY_MS`). */
export const MAX_TIP_DELAY_MS = 2000;

/** Where a comment's tooltip sits from the pointer: the point of its tail below the
 * cursor's arrow (`y`), and its left edge `x` before the pointer, so the tail is clear of
 * the corner. */
export const TIP_OFFSET = { x: 16, y: 20 };

/**
 * Where a comment's tooltip of size `w` × `h` goes for the pointer at `pointer` (content
 * pixels): below it, its tail pointing up at the pointer; shifted left near the right
 * edge, and above the pointer when there is no room below. It always stays wholly inside
 * the visible part of the content (`view`, cut at `contentWidth`).
 */
export function placeTip(
	pointer: { x: number; y: number },
	size: { w: number; h: number },
	view: Area,
	contentWidth: number
): { left: number; top: number } {
	const x0 = Math.max(view.x0, 0) + BAR_GAP;
	const x1 = Math.min(view.x1, contentWidth) - BAR_GAP;
	const y0 = view.y0 + BAR_GAP;
	const y1 = view.y1 - BAR_GAP;
	const left = pointer.x - TIP_OFFSET.x;
	let top = pointer.y + TIP_OFFSET.y + TIP_TAIL.length;
	if (top + size.h > y1) top = pointer.y - BAR_GAP - size.h;
	return {
		left: Math.max(x0, Math.min(left, x1 - size.w)),
		top: Math.max(y0, Math.min(top, y1 - size.h))
	};
}

/** How close to its edge the pointer reveals a toolbar that shows only then, CSS pixels. */
export const REVEAL_ZONE = 72;

/** Whether (x, y) is inside `area` and within `zone` of that edge of it. */
export function nearEdge(
	area: { left: number; top: number; right: number; bottom: number },
	x: number,
	y: number,
	edge: ToolbarPosition,
	zone = REVEAL_ZONE
): boolean {
	if (x < area.left || x > area.right || y < area.top || y > area.bottom) return false;
	switch (edge) {
		case 'top':
			return y - area.top <= zone;
		case 'bottom':
			return area.bottom - y <= zone;
		case 'left':
			return x - area.left <= zone;
		case 'right':
			return area.right - x <= zone;
	}
}

/** Where a toolbar's control opens its menu or panel: below it, or beside it. */
export type MenuSide = 'bottom' | 'left' | 'right';

/** A toolbar at a side of the pages stacks its controls (sections 6.6 and 8). */
export function isStacked(edge: ToolbarPosition): boolean {
	return edge === 'left' || edge === 'right';
}

/** What a control of a toolbar at that edge opens goes below it, or, from a stacked
 * toolbar, beside it and over the pages: a list below would lie over the tools under it. */
export function menuSide(edge: ToolbarPosition): MenuSide {
	if (edge === 'left') return 'right';
	if (edge === 'right') return 'left';
	return 'bottom';
}

/** How far from its control such a menu or panel opens, CSS pixels: beside a stacked
 * toolbar, clear of the toolbar's own edge. */
export function menuOffset(side: MenuSide, below: number): number {
	return side === 'bottom' ? below : 8;
}

/** Whether a quick tool can act in this document. */
export function quickToolAllowed(tool: QuickTool, flags: DocumentFlags, canEditBookmarks: boolean): boolean {
	switch (tool) {
		case 'copy':
			return flags.canCopy;
		case 'bookmark':
			return canEditBookmarks;
		default:
			return flags.canAnnotate;
	}
}

export interface BarControls extends Capabilities {
	/** It can become another text-markup type. */
	retype: boolean;
}

/** What the bar of annotation `a` offers. */
export function barControls(a: Annotation, canAnnotate: boolean): BarControls {
	const caps = capabilities(a, canAnnotate);
	return {
		...caps,
		retype: caps.restyle && a.replyTo === null && MARKUP_SUBTYPES.some((m) => m.subtype === a.subtype)
	};
}
