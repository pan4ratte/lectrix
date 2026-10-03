// Annotation geometry on the page canvas: hit-testing, moving and resizing, and turning a
// text selection into character ranges. Coordinates are page points in view space (top-left
// origin, y down), like text geometry.

import type { Annotation } from '#lib/ipc/index.ts';
import type { Caret, TextGeometry } from '#lib/features/viewer/selection.ts';

export type Box = [number, number, number, number];

/** Resize handles: corners and edge midpoints. */
export type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';
export const HANDLES: readonly Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

export function handlePoint(b: Box, h: Handle): [number, number] {
	const [x0, y0, x1, y1] = b;
	const xm = (x0 + x1) / 2;
	const ym = (y0 + y1) / 2;
	switch (h) {
		case 'nw':
			return [x0, y0];
		case 'n':
			return [xm, y0];
		case 'ne':
			return [x1, y0];
		case 'e':
			return [x1, ym];
		case 'se':
			return [x1, y1];
		case 's':
			return [xm, y1];
		case 'sw':
			return [x0, y1];
		case 'w':
			return [x0, ym];
	}
}

/** The handle within `tolerance` of (x, y), if any. */
export function handleAt(b: Box, x: number, y: number, tolerance: number): Handle | null {
	for (const h of HANDLES) {
		const [hx, hy] = handlePoint(b, h);
		if (Math.abs(x - hx) <= tolerance && Math.abs(y - hy) <= tolerance) return h;
	}
	return null;
}

/** `b` resized by dragging handle `h` by (dx, dy); never smaller than `min` on a side. */
export function resizeBox(b: Box, h: Handle, dx: number, dy: number, min = 4): Box {
	let [x0, y0, x1, y1] = b;
	if (h.includes('w')) x0 = Math.min(x0 + dx, x1 - min);
	if (h.includes('e')) x1 = Math.max(x1 + dx, x0 + min);
	if (h.includes('n')) y0 = Math.min(y0 + dy, y1 - min);
	if (h.includes('s')) y1 = Math.max(y1 + dy, y0 + min);
	return [x0, y0, x1, y1];
}

export function moveBox(b: Box, dx: number, dy: number): Box {
	return [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy];
}

/** `b` kept inside a page of size w × h (as far as it fits). */
export function clampBox(b: Box, w: number, h: number): Box {
	const dx = b[0] < 0 ? -b[0] : b[2] > w ? w - b[2] : 0;
	const dy = b[1] < 0 ? -b[1] : b[3] > h ? h - b[3] : 0;
	return moveBox(b, dx, dy);
}

export function inBox(b: Box | readonly number[], x: number, y: number, tolerance = 0): boolean {
	return x >= b[0]! - tolerance && x <= b[2]! + tolerance && y >= b[1]! - tolerance && y <= b[3]! + tolerance;
}

function inQuad(q: readonly number[], i: number, x: number, y: number): boolean {
	// Corners in order ul, ur, lr, ll make a convex polygon.
	const pts = [0, 1, 3, 2].map((k) => [q[i + k * 2]!, q[i + k * 2 + 1]!] as const);
	let sign = 0;
	for (let k = 0; k < 4; k++) {
		const [ax, ay] = pts[k]!;
		const [bx, by] = pts[(k + 1) % 4]!;
		const cross = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
		if (Math.abs(cross) < 1e-9) continue;
		const s = Math.sign(cross);
		if (sign === 0) sign = s;
		else if (s !== sign) return false;
	}
	return true;
}

function segmentDistance(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
	const dx = bx - ax;
	const dy = by - ay;
	const len2 = dx * dx + dy * dy;
	const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2));
	return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

/** True if (x, y) is on annotation `a`, within `tolerance` points of thin content. */
export function hits(a: Annotation, x: number, y: number, tolerance: number): boolean {
	if (a.hidden) return false;
	if (a.quads.length >= 8) {
		for (let i = 0; i + 8 <= a.quads.length; i += 8) if (inQuad(a.quads, i, x, y)) return true;
		return false;
	}
	if (a.ink.length > 0) {
		const reach = tolerance + (a.width ?? 1) / 2;
		for (const s of a.ink) {
			if (s.length === 2 && Math.hypot(x - s[0]!, y - s[1]!) <= reach) return true;
			for (let i = 0; i + 4 <= s.length; i += 2) {
				if (segmentDistance(x, y, s[i]!, s[i + 1]!, s[i + 2]!, s[i + 3]!) <= reach) return true;
			}
		}
		return false;
	}
	return inBox(a.bounds, x, y);
}

/** The topmost annotation at (x, y): the last one drawn. */
export function annotationAt(list: readonly Annotation[], x: number, y: number, tolerance: number): Annotation | null {
	for (let i = list.length - 1; i >= 0; i--) {
		const a = list[i]!;
		if (hits(a, x, y, tolerance)) return a;
	}
	return null;
}

/** A rectangle as one quad (ul, ur, ll, lr): an area highlight. */
export function boxQuad(b: Box): number[] {
	const [x0, y0, x1, y1] = [Math.min(b[0], b[2]), Math.min(b[1], b[3]), Math.max(b[0], b[2]), Math.max(b[1], b[3])];
	return [x0, y0, x1, y0, x0, y1, x1, y1];
}

export function normalizeBox(b: Box): Box {
	return [Math.min(b[0], b[2]), Math.min(b[1], b[3]), Math.max(b[0], b[2]), Math.max(b[1], b[3])];
}

export interface PageRanges {
	page: number;
	ranges: { line: number; start: number; end: number }[];
}

/**
 * The characters of a selection, page by page (section 3: Rust makes the quads from them).
 * `texts` must hold every page from start to end.
 */
export function selectionRanges(texts: ReadonlyMap<number, TextGeometry>, start: Caret, end: Caret): PageRanges[] {
	const out: PageRanges[] = [];
	for (let page = start.page; page <= end.page; page++) {
		const text = texts.get(page);
		if (!text) continue;
		const ranges: PageRanges['ranges'] = [];
		text.lines.forEach((line, i) => {
			const from = page === start.page && i === start.line ? start.offset : page === start.page && i < start.line ? null : 0;
			const to = page === end.page && i === end.line ? end.offset : page === end.page && i > end.line ? null : line.chars.length;
			if (from === null || to === null || to <= from) return;
			// Leading and trailing spaces would make a highlight start or end in empty space.
			let a = from;
			let b = to;
			while (a < b && /\s/.test(line.chars[a]!)) a++;
			while (b > a && /\s/.test(line.chars[b - 1]!)) b--;
			if (b > a) ranges.push({ line: i, start: a, end: b });
		});
		if (ranges.length) out.push({ page, ranges });
	}
	return out;
}
