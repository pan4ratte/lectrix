// Text selection on cached page geometry (AGENTS.md section 3): hit-testing, the
// highlighted rectangles, and the copied text. Coordinates are page points in view
// space (top-left origin, y down, the document's /Rotate applied).

import type { PageText } from '#lib/ipc/index.ts';

export interface Line {
	/** One string per character (Unicode code point). */
	chars: string[];
	/** Character boxes: x0, y0, x1, y1 per character. */
	boxes: Float32Array;
	bbox: [number, number, number, number];
	block: number;
	/** Unit vector along the line, from its first character to its last. */
	dir: [number, number];
}

export interface TextGeometry {
	page: number;
	revision: number;
	lines: Line[];
}

/** A caret position: before character `offset` of line `line` on page `page`. */
export interface Caret {
	page: number;
	line: number;
	offset: number;
}

export function prepareText(text: PageText): TextGeometry {
	return {
		page: text.page,
		revision: text.revision,
		lines: text.lines.map((l) => {
			const chars = Array.from(l.text);
			const boxes = Float32Array.from(l.boxes);
			return {
				chars,
				boxes,
				bbox: [l.bbox[0], l.bbox[1], l.bbox[2], l.bbox[3]],
				block: l.block,
				dir: lineDirection(boxes, chars.length, l.vertical)
			};
		})
	};
}

function center(boxes: Float32Array, i: number): [number, number] {
	return [(boxes[i * 4]! + boxes[i * 4 + 2]!) / 2, (boxes[i * 4 + 1]! + boxes[i * 4 + 3]!) / 2];
}

function lineDirection(boxes: Float32Array, count: number, vertical: boolean): [number, number] {
	if (count >= 2) {
		const [x0, y0] = center(boxes, 0);
		const [x1, y1] = center(boxes, count - 1);
		const len = Math.hypot(x1 - x0, y1 - y0);
		if (len > 0.01) return [(x1 - x0) / len, (y1 - y0) / len];
	}
	return vertical ? [0, 1] : [1, 0];
}

export function compareCarets(a: Caret, b: Caret): number {
	return a.page - b.page || a.line - b.line || a.offset - b.offset;
}

function distanceToBox(x: number, y: number, b: readonly number[]): number {
	const dx = x < b[0]! ? b[0]! - x : x > b[2]! ? x - b[2]! : 0;
	const dy = y < b[1]! ? b[1]! - y : y > b[3]! ? y - b[3]! : 0;
	return Math.hypot(dx, dy);
}

/** The caret within `line` closest to the point, measured along the line's direction. */
function offsetInLine(line: Line, x: number, y: number): number {
	const [dx, dy] = line.dir;
	const p = x * dx + y * dy;
	let offset = 0;
	for (let i = 0; i < line.chars.length; i++) {
		const [cx, cy] = center(line.boxes, i);
		if (cx * dx + cy * dy < p) offset = i + 1;
		else break;
	}
	return offset;
}

/** How close (points) a pointer must be to a line to count as being on it. */
const LINE_SLOP = 2;

/**
 * The caret at a point on a page. With `nearest`, a point away from all text snaps to the
 * closest line (used while dragging a selection); without it, only points on a line hit.
 */
export function hitTest(
	text: TextGeometry,
	x: number,
	y: number,
	nearest: boolean
): Caret | null {
	let best = -1;
	let bestDistance = Infinity;
	text.lines.forEach((line, i) => {
		const d = distanceToBox(x, y, line.bbox);
		if (d < bestDistance) {
			best = i;
			bestDistance = d;
		}
	});
	if (best < 0 || (!nearest && bestDistance > LINE_SLOP)) return null;
	const line = text.lines[best]!;
	return { page: text.page, line: best, offset: offsetInLine(line, x, y) };
}

/** True if the point is on text (for the I-beam cursor). */
export function isOverText(text: TextGeometry, x: number, y: number): boolean {
	return text.lines.some((l) => distanceToBox(x, y, l.bbox) <= LINE_SLOP);
}

/** The word around a caret, as a [start, end) range of carets (double-click). */
export function wordAt(text: TextGeometry, caret: Caret): [Caret, Caret] {
	const line = text.lines[caret.line];
	if (!line) return [caret, caret];
	const isWord = (c: string | undefined) => c !== undefined && /[\p{L}\p{N}_'’-]/u.test(c);
	let start = Math.min(caret.offset, line.chars.length - 1);
	if (!isWord(line.chars[start]) && isWord(line.chars[start - 1])) start--;
	if (!isWord(line.chars[start])) {
		return [
			{ ...caret, offset: start },
			{ ...caret, offset: start + 1 }
		];
	}
	let end = start;
	while (start > 0 && isWord(line.chars[start - 1])) start--;
	while (end < line.chars.length && isWord(line.chars[end])) end++;
	return [
		{ ...caret, offset: start },
		{ ...caret, offset: end }
	];
}

/** The whole line around a caret (triple-click). */
export function lineAt(text: TextGeometry, caret: Caret): [Caret, Caret] {
	const line = text.lines[caret.line];
	return [
		{ ...caret, offset: 0 },
		{ ...caret, offset: line ? line.chars.length : 0 }
	];
}

/** Orders two carets into [start, end]. */
export function ordered(a: Caret, b: Caret): [Caret, Caret] {
	return compareCarets(a, b) <= 0 ? [a, b] : [b, a];
}

/** The character range [from, to) of `line` on `page` covered by the selection. */
function lineRange(
	page: number,
	lineIndex: number,
	lineLength: number,
	start: Caret,
	end: Caret
): [number, number] | null {
	const here = { page, line: lineIndex };
	const before = (c: Caret) => c.page < here.page || (c.page === here.page && c.line < here.line);
	const after = (c: Caret) => c.page > here.page || (c.page === here.page && c.line > here.line);
	if (after(start) || before(end)) return null;
	const from = start.page === page && start.line === lineIndex ? start.offset : 0;
	const to = end.page === page && end.line === lineIndex ? end.offset : lineLength;
	return to > from ? [from, to] : null;
}

/** One rectangle per selected line on `text`'s page: [x0, y0, x1, y1] in points. */
export function selectionRects(text: TextGeometry, start: Caret, end: Caret): number[][] {
	const rects: number[][] = [];
	text.lines.forEach((line, i) => {
		const range = lineRange(text.page, i, line.chars.length, start, end);
		if (!range) return;
		let x0 = Infinity;
		let y0 = Infinity;
		let x1 = -Infinity;
		let y1 = -Infinity;
		for (let c = range[0]; c < range[1]; c++) {
			x0 = Math.min(x0, line.boxes[c * 4]!);
			y0 = Math.min(y0, line.boxes[c * 4 + 1]!);
			x1 = Math.max(x1, line.boxes[c * 4 + 2]!);
			y1 = Math.max(y1, line.boxes[c * 4 + 3]!);
		}
		rects.push([x0, y0, x1, y1]);
	});
	return rects;
}

/**
 * The selected text: lines joined with line breaks, a blank line between paragraphs
 * (blocks) and between pages. `texts` must hold every page from start to end.
 */
export function selectionText(
	texts: ReadonlyMap<number, TextGeometry>,
	start: Caret,
	end: Caret
): string {
	const parts: string[] = [];
	let lastBlock: number | null = null;
	for (let page = start.page; page <= end.page; page++) {
		const text = texts.get(page);
		if (!text) continue;
		if (parts.length > 0) parts.push('\n');
		lastBlock = null;
		text.lines.forEach((line, i) => {
			const range = lineRange(page, i, line.chars.length, start, end);
			if (!range) return;
			if (lastBlock !== null) parts.push(line.block === lastBlock ? '\n' : '\n\n');
			parts.push(line.chars.slice(range[0], range[1]).join(''));
			lastBlock = line.block;
		});
	}
	return parts.join('');
}
