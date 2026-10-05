// Side panes (section 8): width limits, resizing from the keyboard, the open/close
// animation's length, and which panels each pane holds.

import type { PanelId } from '#lib/ipc/index.ts';

export interface PaneLimits {
	min: number;
	max: number;
	/** The width a double-click on the edge goes back to. */
	initial: number;
}

/** The left pane, which holds pages, bookmarks and page labels at first. */
export const LEFT_LIMITS: PaneLimits = { min: 180, max: 480, initial: 240 };
/** The right pane, which holds the annotation list at first. */
export const RIGHT_LIMITS: PaneLimits = { min: 240, max: 560, initial: 300 };

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

/** The panels in each pane, in order. Every panel is in exactly one of them. */
export interface PanelArrangement {
	left: PanelId[];
	right: PanelId[];
}

export const DEFAULT_ARRANGEMENT: Readonly<PanelArrangement> = {
	left: ['pages', 'bookmarks', 'labels'],
	right: ['annotations']
};

const ALL_PANELS: readonly PanelId[] = [...DEFAULT_ARRANGEMENT.left, ...DEFAULT_ARRANGEMENT.right];

/**
 * A remembered arrangement made whole: unknown and repeated panels are dropped, and a
 * panel missing from both panes goes back to the end of its first pane.
 */
export function normalizeArrangement(left: readonly unknown[], right: readonly unknown[]): PanelArrangement {
	const seen = new Set<PanelId>();
	const keep = (list: readonly unknown[]) =>
		list.filter((p): p is PanelId => {
			if (!ALL_PANELS.includes(p as PanelId) || seen.has(p as PanelId)) return false;
			seen.add(p as PanelId);
			return true;
		});
	const result = { left: keep(left), right: keep(right) };
	for (const side of ['left', 'right'] as const) {
		for (const p of DEFAULT_ARRANGEMENT[side]) if (!seen.has(p)) result[side].push(p);
	}
	return result;
}

/** The pane that holds a panel. */
export function paneOf(arrangement: PanelArrangement, panel: PanelId): PaneSide {
	return arrangement.left.includes(panel) ? 'left' : 'right';
}

/**
 * The arrangement after moving a panel to `index` among the panels of `side` (counted
 * without the panel itself; clamped to the ends).
 */
export function movePanel(arrangement: PanelArrangement, panel: PanelId, side: PaneSide, index: number): PanelArrangement {
	const result = {
		left: arrangement.left.filter((p) => p !== panel),
		right: arrangement.right.filter((p) => p !== panel)
	};
	const target = result[side];
	target.splice(Math.max(0, Math.min(target.length, Math.round(index))), 0, panel);
	return result;
}

/**
 * Where Ctrl+Shift+Left or Right moves a panel: one place along its pane's row, and past
 * the inner end of a pane into the other one (the left pane's last place leads to the right
 * pane's first). Null at the window's edges.
 */
export function stepPanel(
	arrangement: PanelArrangement,
	panel: PanelId,
	direction: -1 | 1
): { side: PaneSide; index: number } | null {
	const side = paneOf(arrangement, panel);
	const list = arrangement[side];
	const i = list.indexOf(panel);
	const next = i + direction;
	if (next >= 0 && next < list.length) return { side, index: next };
	if (side === 'left' && direction === 1) return { side: 'right', index: 0 };
	if (side === 'right' && direction === -1) return { side: 'left', index: arrangement.left.length };
	return null;
}
