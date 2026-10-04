// Side panes (section 8): width limits, resizing from the keyboard, and the open/close
// animation's length.

export interface PaneLimits {
	min: number;
	max: number;
	/** The width a double-click on the edge goes back to. */
	initial: number;
}

/** The left sidebar: pages, bookmarks, page labels. */
export const SIDEBAR_LIMITS: PaneLimits = { min: 180, max: 480, initial: 240 };
/** The right pane: the annotation list. */
export const ANNOTATIONS_LIMITS: PaneLimits = { min: 240, max: 560, initial: 300 };

/** Each pane takes at most this share of the window, so the page always keeps room. */
export const MAX_WINDOW_SHARE = 0.4;

/** Arrow keys on a pane's edge resize by this much. */
export const RESIZE_STEP = 16;

/** Opening and closing a pane (section 8: animations under 150 ms). */
export const PANE_MOTION_MS = 140;

export type PaneSide = 'left' | 'right';

export function clampWidth(width: number, limits: PaneLimits): number {
	if (!Number.isFinite(width)) return limits.initial;
	return Math.round(Math.min(limits.max, Math.max(limits.min, width)));
}

/** The width a pane is shown at: its own, within its share of the window. */
export function shownWidth(width: number, limits: PaneLimits, windowWidth: number): number {
	const room = windowWidth > 0 ? windowWidth * MAX_WINDOW_SHARE : limits.max;
	return Math.round(Math.min(clampWidth(width, limits), Math.max(limits.min, room)));
}

/**
 * The width after a key on a pane's edge, or null if the key does not resize. The arrow
 * pointing away from the page widens the pane, as the edge moves that way; Home and End go
 * to the limits.
 */
export function keyResize(width: number, key: string, side: PaneSide, limits: PaneLimits): number | null {
	const outward = side === 'left' ? 'ArrowRight' : 'ArrowLeft';
	const inward = side === 'left' ? 'ArrowLeft' : 'ArrowRight';
	switch (key) {
		case outward:
			return clampWidth(width + RESIZE_STEP, limits);
		case inward:
			return clampWidth(width - RESIZE_STEP, limits);
		case 'Home':
			return limits.min;
		case 'End':
			return limits.max;
		default:
			return null;
	}
}

/** `ms`, or 0 when the system asks for reduced motion. */
export function motionMs(ms: number): number {
	if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return ms;
	return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : ms;
}
