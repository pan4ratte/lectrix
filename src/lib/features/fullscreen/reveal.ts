// Full screen (section 6.1): when the pointer at a side of the screen brings that side's pane
// over the pages, and how long things wait.

import type { PaneSide } from '#lib/components/panes.ts';

/** How close to a side of the screen the pointer brings that side's pane, CSS pixels. As
 * narrow as the strip that shows an auto-hidden taskbar: full screen, the pointer stops at
 * the screen's edge, and the pages' scrollbar beside it stays usable. */
export const EDGE_ZONE = 4;
/** How long the pointer rests there first, so one on its way past (to the scrollbar, to
 * another monitor) brings nothing. */
export const EDGE_DWELL_MS = 150;
/** How long a pane stays after the pointer has left it, so it doesn't flicker. */
export const PANE_LINGER_MS = 300;
/** How long Alt is held before both panes and the toolbar show: a shorter press is the Alt
 * of a shortcut (Alt+Left). */
export const PEEK_AFTER_MS = 150;
/** How long the toolbar shows after full screen starts, when it shows only on demand. */
export const ENTER_FLASH_MS = 1500;
/** A pane's resize edge reaches this far out of it. */
const RESIZE_GRIP = 4;

/**
 * Whether the pointer at `x`, in a window `width` wide, is at the pane of `side`: over it
 * (`reach` wide, with its resize edge) while it shows, and within `EDGE_ZONE` of that side
 * of the screen while it doesn't (`reach` null).
 */
export function atPane(side: PaneSide, x: number, width: number, reach: number | null): boolean {
	const depth = reach === null ? EDGE_ZONE : reach + RESIZE_GRIP;
	return side === 'left' ? x < depth : x >= width - depth;
}
