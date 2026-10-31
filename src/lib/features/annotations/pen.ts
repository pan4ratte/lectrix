// What the Shapes tool draws (section 6.5): lines, arrows, rectangles and ellipses, each
// written as an ordinary ink annotation (one or two strokes), so every reader shows and
// edits it as a drawing; and the stabilizer that smooths the pen's freehand strokes.
// Page points throughout; a stroke is a flat list of x, y pairs.

import { ArrowUpRight, Circle, Minus, Square } from '@lucide/svelte';
import type { Component } from 'svelte';

export type ShapeKind = 'line' | 'arrow' | 'rectangle' | 'ellipse';

export const SHAPES: readonly {
	id: ShapeKind;
	label: string;
	icon: Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;
	/** What Shift does while it is drawn. */
	shift: string;
}[] = [
	{ id: 'line', label: 'Line', icon: Minus, shift: '45° steps' },
	{ id: 'arrow', label: 'Arrow', icon: ArrowUpRight, shift: '45° steps' },
	{ id: 'rectangle', label: 'Rectangle', icon: Square, shift: 'square' },
	{ id: 'ellipse', label: 'Ellipse', icon: Circle, shift: 'circle' }
];

export function isShapeKind(value: unknown): value is ShapeKind {
	return SHAPES.some((s) => s.id === value);
}

/** The stroke width slider's range and step, points (Acrobat's pencil goes to 12 pt). */
export const MIN_WIDTH = 0.5;
export const MAX_WIDTH = 12;
export const WIDTH_STEP = 0.5;

/** How far the stabilizer's string reaches at 100% smoothing, screen pixels. */
export const MAX_STRING_PX = 32;

type Pt = [number, number];

/** How far `end` may go from `start` and stay on a page of `size`, along each axis in the
 * direction it goes. */
function room(start: Pt, dx: number, dy: number, size: { width: number; height: number }): [number, number] {
	return [dx >= 0 ? size.width - start[0] : start[0], dy >= 0 ? size.height - start[1] : start[1]];
}

/**
 * Where a shape dragged from `start` to `end` ends: with Shift (`constrain`), a line or an
 * arrow at the nearest multiple of 45°, a rectangle a square and an ellipse a circle, kept
 * on the page; otherwise `end` itself.
 */
export function constrainEnd(shape: ShapeKind, start: Pt, end: Pt, constrain: boolean, size: { width: number; height: number }): Pt {
	const dx = end[0] - start[0];
	const dy = end[1] - start[1];
	if (!constrain) return end;
	const [roomX, roomY] = room(start, dx, dy, size);
	if (shape === 'rectangle' || shape === 'ellipse') {
		const side = Math.min(Math.max(Math.abs(dx), Math.abs(dy)), roomX, roomY);
		return [start[0] + Math.sign(dx || 1) * side, start[1] + Math.sign(dy || 1) * side];
	}
	const step = Math.PI / 4;
	const angle = Math.round(Math.atan2(dy, dx) / step) * step;
	const ux = Math.abs(Math.cos(angle)) < 1e-9 ? 0 : Math.cos(angle);
	const uy = Math.abs(Math.sin(angle)) < 1e-9 ? 0 : Math.sin(angle);
	// The drag's length along that direction, no further than the page lets it go.
	let length = Math.max(0, dx * ux + dy * uy);
	const [rx, ry] = room(start, ux, uy, size);
	if (ux !== 0) length = Math.min(length, rx / Math.abs(ux));
	if (uy !== 0) length = Math.min(length, ry / Math.abs(uy));
	return [start[0] + ux * length, start[1] + uy * length];
}

/** How long an arrow's head is for a stroke of `width`, on a shaft of `length`: larger for
 * thicker strokes, and never more than half the shaft. */
export function arrowHeadLength(width: number, length: number): number {
	return Math.min(6 + 3 * width, length / 2);
}

/** Points on an ellipse in the box from `a` to `b`: enough that the lines between them stay
 * within a tenth of a point of the curve (Rust simplifies them to 0.5 pt before writing). */
function ellipse(a: Pt, b: Pt): number[] {
	const cx = (a[0] + b[0]) / 2;
	const cy = (a[1] + b[1]) / 2;
	const rx = Math.abs(b[0] - a[0]) / 2;
	const ry = Math.abs(b[1] - a[1]) / 2;
	const r = Math.max(rx, ry);
	// The sagitta of a chord of angle θ is r(1 − cos(θ/2)) ≈ rθ²/8.
	const n = Math.min(360, Math.max(16, Math.ceil((2 * Math.PI) / Math.sqrt((8 * 0.1) / Math.max(r, 1)))));
	const points: number[] = [];
	for (let i = 0; i <= n; i++) {
		const t = (i / n) * 2 * Math.PI;
		points.push(cx + rx * Math.cos(t), cy + ry * Math.sin(t));
	}
	return points;
}

/** The strokes of `shape` from `start` to `end` (already constrained), for a pen of `width`. */
export function shapeStrokes(shape: ShapeKind, start: Pt, end: Pt, width: number): number[][] {
	const [x0, y0] = start;
	const [x1, y1] = end;
	switch (shape) {
		case 'line':
			return [[x0, y0, x1, y1]];
		case 'arrow': {
			const length = Math.hypot(x1 - x0, y1 - y0);
			if (length === 0) return [[x0, y0, x1, y1]];
			const head = arrowHeadLength(width, length);
			const ux = (x1 - x0) / length;
			const uy = (y1 - y0) / length;
			// The head's sides at 30° either side of the shaft, as Acrobat's open arrow.
			const cos = Math.cos(Math.PI / 6);
			const sin = Math.sin(Math.PI / 6);
			const left: Pt = [x1 - head * (ux * cos - uy * sin), y1 - head * (uy * cos + ux * sin)];
			const right: Pt = [x1 - head * (ux * cos + uy * sin), y1 - head * (uy * cos - ux * sin)];
			return [
				[x0, y0, x1, y1],
				[left[0], left[1], x1, y1, right[0], right[1]]
			];
		}
		case 'rectangle':
			return [[x0, y0, x1, y0, x1, y1, x0, y1, x0, y0]];
		case 'ellipse':
			return [ellipse(start, end)];
	}
}

/**
 * The stabilizer of freehand strokes ("lazy brush"): the pen's tip trails the pointer on a
 * string of `radius` points, and moves only when the pointer pulls the string taut, so the
 * tremor of a hand or a mouse within the string's reach never reaches the stroke. A radius
 * of 0 draws where the pointer is.
 */
export class Stabilizer {
	tip: Pt;

	constructor(
		start: Pt,
		private radius: number
	) {
		this.tip = [start[0], start[1]];
	}

	/** The tip after the pointer has moved to `to`. */
	follow(to: Pt): Pt {
		const dx = to[0] - this.tip[0];
		const dy = to[1] - this.tip[1];
		const d = Math.hypot(dx, dy);
		if (d > this.radius) {
			const k = (d - this.radius) / d;
			this.tip = [this.tip[0] + dx * k, this.tip[1] + dy * k];
		}
		return this.tip;
	}
}
