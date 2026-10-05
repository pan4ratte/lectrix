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

/** How close to its edge the pointer reveals a toolbar that shows only then, CSS pixels. */
export const REVEAL_ZONE = 72;

/** Whether (x, y) is inside `area` and within `zone` of its top or bottom edge. */
export function nearEdge(
	area: { left: number; top: number; right: number; bottom: number },
	x: number,
	y: number,
	edge: ToolbarPosition,
	zone = REVEAL_ZONE
): boolean {
	if (x < area.left || x > area.right || y < area.top || y > area.bottom) return false;
	return edge === 'top' ? y - area.top <= zone : area.bottom - y <= zone;
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
