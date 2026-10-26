// Annotation geometry on the page canvas: hit-testing, moving and resizing, and turning a
// text selection into character ranges. Coordinates are page points in view space (top-left
// origin, y down), like text geometry.

import type { Annotation } from '#lib/ipc/index.ts';
import type { Rotation } from '#lib/features/viewer/layout.ts';
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

/** Whether boxes `a` and `b` (normalized) overlap or touch. */
function boxesMeet(a: readonly number[], b: readonly number[]): boolean {
	return a[0]! <= b[2]! && b[0]! <= a[2]! && a[1]! <= b[3]! && b[1]! <= a[3]!;
}

/** Whether the segment from (ax, ay) to (bx, by) crosses box `b` (normalized). */
function segmentMeets(b: Box, ax: number, ay: number, bx: number, by: number): boolean {
	// Clips the segment to the box one side at a time (Liang–Barsky).
	let t0 = 0;
	let t1 = 1;
	const dx = bx - ax;
	const dy = by - ay;
	const sides: [number, number][] = [
		[-dx, ax - b[0]],
		[dx, b[2] - ax],
		[-dy, ay - b[1]],
		[dy, b[3] - ay]
	];
	for (const [p, q] of sides) {
		if (p === 0) {
			if (q < 0) return false;
			continue;
		}
		const t = q / p;
		if (p < 0) t0 = Math.max(t0, t);
		else t1 = Math.min(t1, t);
		if (t0 > t1) return false;
	}
	return true;
}

/** True if annotation `a` touches box `b` (page points, either corner first): text markup
 * by its quads, a drawing by its strokes, anything else by its bounds. */
export function touches(a: Annotation, b: Box): boolean {
	if (a.hidden) return false;
	const box = normalizeBox(b);
	if (a.quads.length >= 8) {
		for (let i = 0; i + 8 <= a.quads.length; i += 8) {
			const xs = [a.quads[i]!, a.quads[i + 2]!, a.quads[i + 4]!, a.quads[i + 6]!];
			const ys = [a.quads[i + 1]!, a.quads[i + 3]!, a.quads[i + 5]!, a.quads[i + 7]!];
			if (boxesMeet([Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)], box)) return true;
		}
		return false;
	}
	if (a.ink.length > 0) {
		const half = (a.width ?? 1) / 2;
		const wide: Box = [box[0] - half, box[1] - half, box[2] + half, box[3] + half];
		for (const s of a.ink) {
			if (s.length >= 2 && inBox(wide, s[0]!, s[1]!)) return true;
			for (let i = 0; i + 4 <= s.length; i += 2) {
				if (segmentMeets(wide, s[i]!, s[i + 1]!, s[i + 2]!, s[i + 3]!)) return true;
			}
		}
		return false;
	}
	return boxesMeet(normalizeBox([...a.bounds] as Box), box);
}

/** The annotations of `list` that box `b` touches, as an area dragged over them selects
 * them (section 6.5). */
export function annotationsIn(list: readonly Annotation[], b: Box): Annotation[] {
	return list.filter((a) => touches(a, b));
}

/** The topmost annotation at (x, y): the last one drawn. */
export function annotationAt(list: readonly Annotation[], x: number, y: number, tolerance: number): Annotation | null {
	for (let i = list.length - 1; i >= 0; i--) {
		const a = list[i]!;
		if (hits(a, x, y, tolerance)) return a;
	}
	return null;
}

export function normalizeBox(b: Box): Box {
	return [Math.min(b[0], b[2]), Math.min(b[1], b[3]), Math.max(b[0], b[2]), Math.max(b[1], b[3])];
}

// A page turned on screen (the view rotation, a page's own turn: section 6.1) shows its
// points elsewhere. What is placed upright as seen there (a text box, a note's icon, an
// area highlight: section 6.5) is worked out where the page is shown so, and brought back.

/** A point of a page `w` by `h`, where the page turned by `turn` (clockwise) shows it. */
export function turnPoint(x: number, y: number, turn: Rotation, w: number, h: number): [number, number] {
	switch (turn) {
		case 90:
			return [h - y, x];
		case 180:
			return [w - x, h - y];
		case 270:
			return [y, w - x];
		default:
			return [x, y];
	}
}

/** The point of the page that the page turned by `turn` shows at (x, y): the inverse of
 * `turnPoint`. */
export function unturnPoint(x: number, y: number, turn: Rotation, w: number, h: number): [number, number] {
	switch (turn) {
		case 90:
			return [y, h - x];
		case 180:
			return [w - x, h - y];
		case 270:
			return [w - y, x];
		default:
			return [x, y];
	}
}

/** A box of the page, where the page turned by `turn` shows it. */
export function turnBox(b: Box, turn: Rotation, w: number, h: number): Box {
	const [ax, ay] = turnPoint(b[0], b[1], turn, w, h);
	const [bx, by] = turnPoint(b[2], b[3], turn, w, h);
	return normalizeBox([ax, ay, bx, by]);
}

/** The box of the page that the page turned by `turn` shows at `b`. */
export function unturnBox(b: Box, turn: Rotation, w: number, h: number): Box {
	const [ax, ay] = unturnPoint(b[0], b[1], turn, w, h);
	const [bx, by] = unturnPoint(b[2], b[3], turn, w, h);
	return normalizeBox([ax, ay, bx, by]);
}

/**
 * A rectangle as one quad (ul, ur, ll, lr): an area highlight. Upper and lower, left and
 * right are as the page turned by `turn` shows it, where the area was drawn.
 */
export function boxQuad(b: Box, turn: Rotation = 0): number[] {
	// The page's size cancels out: the quad's corners are the box's own.
	const [x0, y0, x1, y1] = turnBox(b, turn, 0, 0);
	return [
		[x0, y0],
		[x1, y0],
		[x0, y1],
		[x1, y1]
	].flatMap(([x, y]) => unturnPoint(x!, y!, turn, 0, 0));
}

/**
 * Where a text box whose text is turned by `turn` from the page (its lines run across
 * the page turned so) starts, and how far its lines run and its box reaches down: the
 * corner of `b` at the start of its first line, the length across and the length down.
 */
export function textBoxFrame(b: Box, turn: Rotation): { x: number; y: number; across: number; down: number } {
	const [w, h] = [b[2] - b[0], b[3] - b[1]];
	switch (turn) {
		case 90:
			return { x: b[0], y: b[3], across: h, down: w };
		case 180:
			return { x: b[2], y: b[3], across: w, down: h };
		case 270:
			return { x: b[2], y: b[1], across: h, down: w };
		default:
			return { x: b[0], y: b[1], across: w, down: h };
	}
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
