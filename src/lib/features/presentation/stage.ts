// What a presentation's screen shows (section 6.1), as plain data: built from the document
// in the window presenting (build.ts), and sent as it is to its audience screen on a second
// screen, which has no document of its own (audience.rs). Nothing here needs the app.

import { unturnPoint } from '#lib/features/annotations/geometry.ts';
import { CSS_PX_PER_PT, type Rotation } from '#lib/features/viewer/layout.ts';
import type { ImageFormat, PageLink, Transition } from '#lib/ipc/index.ts';

import { placePage, type Blank } from './input.ts';

/** One page, as it shows. */
export interface StagePage {
	index: number;
	/** Its size in points, after its `/Rotate`. */
	width: number;
	height: number;
	/** Its `/Rotate`. */
	rotation: number;
	/** How it is turned on screen (view rotation and the page's own turn). */
	turn: Rotation;
	/** The annotations the annotation list filters out, left out as in the viewer. */
	hidden: number[];
	/** The annotations' own layer while the colours are adjusted, or null. */
	layer: 'annots' | 'annots-dark' | null;
	links: PageLink[];
}

/** The visual adjustments (section 6.1), as the viewer applies them. */
export interface StageLook {
	/** Anything is adjusted. */
	adjusted: boolean;
	light: string | null;
	colour: string | null;
	tone: number | null;
}

/** A change of page, and the transition it comes with. */
export interface StageChange {
	/** Counts the changes, so the same page shown again is a change too. */
	seq: number;
	/** The page shown before, or null for the first. */
	from: number | null;
	transition: Transition | null;
}

export interface StageState {
	docId: number;
	revision: number;
	imageFormat: ImageFormat;
	/** The page shown, 0-based. */
	current: number;
	/** The pages kept: the one shown, those before and after it (rendered ahead), and the
	 * one going while a transition plays. */
	pages: StagePage[];
	blank: Blank | null;
	look: StageLook;
	change: StageChange;
	/** The pages show (false while the window switches). */
	shown: boolean;
}

/** The pages kept around `current` (in the order they render: it first, then the next,
 * the one before, and the page going). */
export function keptPages(current: number, count: number, from: number | null): number[] {
	const kept = [current, current + 1, current - 1];
	if (from !== null) kept.push(from);
	return [...new Set(kept)].filter((p) => p >= 0 && p < count);
}

/** Which of the links of `page`, shown in a box `width` by `height`, is at a point of the
 * box (CSS pixels); null for none. */
export function linkIndexAt(page: StagePage, width: number, height: number, x: number, y: number): number | null {
	if (page.links.length === 0) return null;
	const placed = placePage(page, page.turn, width, height);
	const dx = x - placed.left;
	const dy = y - placed.top;
	if (dx < 0 || dy < 0 || dx > placed.width || dy > placed.height) return null;
	const k = placed.zoom * CSS_PX_PER_PT;
	const [px, py] = unturnPoint(dx / k, dy / k, page.turn, page.width, page.height);
	// Of links that overlap, the one on top (drawn last), as in the viewer (`linkAt`).
	for (let i = page.links.length - 1; i >= 0; i--) {
		const [x0, y0, x1, y1] = page.links[i]!.rect;
		if (px >= x0 && px <= x1 && py >= y0 && py <= y1) return i;
	}
	return null;
}
