// Where the app's tooltips go (section 8). A control names its tooltip in `data-tip`, never
// in `title`, which the system would draw in its own style; `Tooltips.svelte` shows it.
import { TIP_OFFSET, TIP_TAIL, type Area, type Point } from '#lib/features/annotations/bars.ts';

/** How long the pointer rests on a control, or the keyboard focus, before its tooltip shows. */
export const TOOLTIP_DELAY_MS = 500;
/** For this long after a tooltip went, the next one shows at once. */
export const TOOLTIP_WARM_MS = 400;
/** Kept between a tooltip and the window's edges. */
export const TOOLTIP_MARGIN = 4;
/** A control wider or taller than this gets its tooltip where the pointer is along it, not
 * at its middle. */
export const TOOLTIP_LARGE = 320;
/** Between the point of the tail and the control. */
const GAP = 2;

/**
 * What a tooltip points at, for a control at `rect`: the control, or for a large one (a
 * row, a long name, a pane's edge) the place along it where the pointer is, clear of the
 * cursor's arrow. `pointer` is null when the keyboard focus brought the tooltip.
 */
export function tooltipAnchor(rect: Area, pointer: Point | null): Area {
	const wide = rect.x1 - rect.x0 > TOOLTIP_LARGE;
	const tall = rect.y1 - rect.y0 > TOOLTIP_LARGE;
	if (!pointer || (!wide && !tall)) return rect;
	const x = Math.max(rect.x0, Math.min(pointer.x, rect.x1));
	const y = Math.max(rect.y0, Math.min(pointer.y, rect.y1));
	return {
		x0: wide ? x : rect.x0,
		x1: wide ? x : rect.x1,
		y0: tall ? y : rect.y0,
		y1: tall ? y + TIP_OFFSET.y : Math.max(rect.y1, y + TIP_OFFSET.y)
	};
}

/**
 * Where a tooltip of size `w` × `h` goes for `anchor` in a window of `viewport`, always
 * with room for the tail: below it and centred on it; above it when there is no room
 * below; beside it (right, else left) at its middle when there is none above either, as
 * for a pane's edge; and shifted along it to stay inside the window.
 */
export function placeTooltip(
	anchor: Area,
	size: { w: number; h: number },
	viewport: { w: number; h: number }
): { left: number; top: number } {
	const reach = TIP_TAIL.length + GAP;
	const within = (v: number, length: number, room: number) =>
		Math.round(Math.max(TOOLTIP_MARGIN, Math.min(v, room - TOOLTIP_MARGIN - length)));
	const centred = within((anchor.x0 + anchor.x1) / 2 - size.w / 2, size.w, viewport.w);
	const below = anchor.y1 + reach;
	if (below + size.h <= viewport.h - TOOLTIP_MARGIN) return { left: centred, top: Math.round(below) };
	const above = anchor.y0 - reach - size.h;
	if (above >= TOOLTIP_MARGIN) return { left: centred, top: Math.round(above) };
	const level = within((anchor.y0 + anchor.y1) / 2 - size.h / 2, size.h, viewport.h);
	const right = anchor.x1 + reach;
	if (right + size.w <= viewport.w - TOOLTIP_MARGIN) return { left: Math.round(right), top: level };
	const left = anchor.x0 - reach - size.w;
	if (left >= TOOLTIP_MARGIN) return { left: Math.round(left), top: level };
	return { left: centred, top: within(below, size.h, viewport.h) };
}

/**
 * The tooltip as a control's description for screen readers, as the system's `title` was:
 * none when it only repeats the control's name.
 */
export function tooltipDescription(tip: string, name: string): string | null {
	const text = tip.trim();
	return text && text !== name.trim() ? text : null;
}
