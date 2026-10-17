// Text selection on cached page geometry (AGENTS.md section 3): hit-testing, the
// highlighted rectangles, and the copied text. Coordinates are page points in view
// space (top-left origin, y down, the document's /Rotate applied).

import type { PageLink, PageText } from '#lib/ipc/index.ts';

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
	/** The page's links (links.ts), which come with its text. */
	links: PageLink[];
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
		links: text.links,
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

/** True for a character that is part of a word. */
function isWord(c: string | undefined): boolean {
	return c !== undefined && /[\p{L}\p{N}_'’-]/u.test(c);
}

/** The whole line around a caret (triple-click). */
export function lineAt(text: TextGeometry, caret: Caret): [Caret, Caret] {
	const line = text.lines[caret.line];
	return [
		{ ...caret, offset: 0 },
		{ ...caret, offset: line ? line.chars.length : 0 }
	];
}

/** What a double-click (a word) or a triple-click (a line) selected. */
export type SelectionUnit = 'word' | 'line';

/**
 * The selection a drag makes after a double- or triple-click: the word or line it selected
 * (`unit`) stays whole, and from its far side the drag selects whole words (or lines) up to
 * the one at `focus`. `text` is the text of `focus`'s page.
 */
export function extendByUnit(
	text: TextGeometry,
	unit: readonly [Caret, Caret],
	focus: Caret,
	by: SelectionUnit
): { anchor: Caret; focus: Caret } {
	const [from, to] = unit;
	const around = by === 'word' ? wordAt(text, focus) : lineAt(text, focus);
	if (compareCarets(focus, from) < 0) return { anchor: to, focus: around[0] };
	if (compareCarets(focus, to) > 0) {
		// Past the end of a line, the caret is after its last character: that word or line.
		return { anchor: from, focus: compareCarets(around[1], to) > 0 ? around[1] : focus };
	}
	return { anchor: from, focus: to };
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

/** The selected part of each line on `text`'s page, as the line and its rectangle. */
function selectedLines(text: TextGeometry, start: Caret, end: Caret): { line: Line; rect: number[] }[] {
	const out: { line: Line; rect: number[] }[] = [];
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
		out.push({ line, rect: [x0, y0, x1, y1] });
	});
	return out;
}

/** One rectangle per selected line on `text`'s page: [x0, y0, x1, y1] in points. */
export function selectionRects(text: TextGeometry, start: Caret, end: Caret): number[][] {
	return selectedLines(text, start, end).map((s) => s.rect);
}

/** A side of the selection that can be dragged: where it starts, or where it ends. */
export interface SelectionEdge {
	side: 'start' | 'end';
	/** The line there runs down or up the page rather than across it. */
	vertical: boolean;
}

/** A side of the selection as it is on its page, for its handle (section 6.1). */
export interface SelectionHandle extends SelectionEdge {
	/** The side of the line's rectangle the selection starts (or ends) at: a box of no width. */
	box: number[];
	/** Where the handle's knob goes: the top of the line for the start, its bottom for the end. */
	knob: [number, number];
}

/** The side of a selected line's rectangle its text starts (or ends) at. */
function handleOf(line: Line, r: readonly number[], side: 'start' | 'end'): SelectionHandle {
	const [dx, dy] = line.dir;
	const vertical = Math.abs(dy) > Math.abs(dx);
	const forward = (vertical ? dy : dx) > 0;
	const low = (side === 'start') === forward;
	if (vertical) {
		// The top of a line running down the page is on its right, and the other way round.
		const y = low ? r[1]! : r[3]!;
		const top = dy > 0 ? r[2]! : r[0]!;
		const bottom = dy > 0 ? r[0]! : r[2]!;
		return { side, vertical, box: [r[0]!, y, r[2]!, y], knob: [side === 'start' ? top : bottom, y] };
	}
	const x = low ? r[0]! : r[2]!;
	return { side, vertical, box: [x, r[1]!, x, r[3]!], knob: [x, side === 'start' ? r[1]! : r[3]!] };
}

/**
 * The sides of the selection that are on `text`'s page: the start of its first selected
 * line and the end of its last one. A selection that starts on an earlier page has no
 * start here, and one that ends on a later page no end.
 */
export function selectionHandles(text: TextGeometry, start: Caret, end: Caret): SelectionHandle[] {
	const lines = selectedLines(text, start, end);
	const first = lines[0];
	const last = lines.at(-1);
	const out: SelectionHandle[] = [];
	if (first && text.page === start.page) out.push(handleOf(first.line, first.rect, 'start'));
	if (last && text.page === end.page) out.push(handleOf(last.line, last.rect, 'end'));
	return out;
}

/** The side of the selection within `reach` points of a point on `text`'s page; of two, the
 * closer one. */
export function edgeAt(
	text: TextGeometry,
	start: Caret,
	end: Caret,
	x: number,
	y: number,
	reach: number
): SelectionEdge | null {
	let best: SelectionEdge | null = null;
	let bestDistance = Infinity;
	for (const h of selectionHandles(text, start, end)) {
		const d = distanceToBox(x, y, h.box);
		if (d <= reach && d < bestDistance) {
			best = { side: h.side, vertical: h.vertical };
			bestDistance = d;
		}
	}
	return best;
}

/**
 * True if nothing is selected between two carets: the same place, or the end of one line
 * and the start of the next. A page between them whose text is not in `texts` counts as
 * having some.
 */
export function selectionIsEmpty(texts: ReadonlyMap<number, TextGeometry>, start: Caret, end: Caret): boolean {
	for (let page = start.page; page <= end.page; page++) {
		const text = texts.get(page);
		if (!text || selectedLines(text, start, end).length > 0) return false;
	}
	return true;
}

// ----- moving a caret from the keyboard (section 6.1) -----

/** How far a key moves the end of the selection. */
export type CaretStep = 'character' | 'word' | 'line' | 'paragraph';

/** A page's text, for `moveCaret`: null if it has none or can't be read. */
export type TextSource = (page: number) => Promise<TextGeometry | null> | TextGeometry | null;

/** The box of the character a caret is at (the last one, at the end of its line). */
export function caretBox(text: TextGeometry, caret: Caret): number[] | null {
	const line = text.lines[caret.line];
	if (!line) return null;
	if (line.chars.length === 0) return [...line.bbox];
	const i = Math.min(caret.offset, line.chars.length - 1) * 4;
	return [line.boxes[i]!, line.boxes[i + 1]!, line.boxes[i + 2]!, line.boxes[i + 3]!];
}

/** Where a caret is on its line: on the line's middle, between two characters. */
export function caretPoint(line: Line, offset: number): [number, number] {
	const n = line.chars.length;
	if (n === 0) return [(line.bbox[0] + line.bbox[2]) / 2, (line.bbox[1] + line.bbox[3]) / 2];
	const i = Math.min(offset, n - 1);
	const [cx, cy] = center(line.boxes, i);
	const [dx, dy] = line.dir;
	const along =
		Math.abs(dx) * (line.boxes[i * 4 + 2]! - line.boxes[i * 4]!) +
		Math.abs(dy) * (line.boxes[i * 4 + 3]! - line.boxes[i * 4 + 1]!);
	// Before character `i`, or after the last one.
	const half = offset < n ? -along / 2 : along / 2;
	return [cx + dx * half, cy + dy * half];
}

/** The text cursor at a caret: a line across its line of text, between two characters, as
 * high as the character there is ([x0, y0, x1, y1] in page points). */
export function caretBar(text: TextGeometry, caret: Caret): [number, number, number, number] | null {
	const line = text.lines[caret.line];
	const box = caretBox(text, caret);
	if (!line || !box) return null;
	const [x, y] = caretPoint(line, caret.offset);
	const [dx, dy] = line.dir;
	return Math.abs(dx) >= Math.abs(dy) ? [x, box[1]!, x, box[3]!] : [box[0]!, y, box[2]!, y];
}

/** The end of the word at or after `offset` (forward), or the start of the one at or
 * before it. */
function wordBoundary(line: Line, offset: number, direction: 1 | -1): number {
	const n = line.chars.length;
	let i = offset;
	if (direction > 0) {
		while (i < n && !isWord(line.chars[i])) i++;
		while (i < n && isWord(line.chars[i])) i++;
	} else {
		while (i > 0 && !isWord(line.chars[i - 1])) i--;
		while (i > 0 && isWord(line.chars[i - 1])) i--;
	}
	return i;
}

/** The last line (forward) or the first line of the paragraph (block) that `line` is in. */
function paragraphEnd(text: TextGeometry, line: number, direction: 1 | -1): number {
	const block = text.lines[line]!.block;
	let i = line;
	while (text.lines[i + direction]?.block === block) i += direction;
	return i;
}

/** The next line with text after (or before) a line, on its page or on the nearest page
 * that has any; null at the end (or the start) of the document. */
async function adjacentLine(
	source: TextSource,
	pageCount: number,
	page: number,
	line: number,
	direction: 1 | -1
): Promise<{ text: TextGeometry; line: number } | null> {
	let text = await source(page);
	let i = line + direction;
	for (;;) {
		if (text) {
			for (; i >= 0 && i < text.lines.length; i += direction) {
				if (text.lines[i]!.chars.length > 0) return { text, line: i };
			}
		}
		page += direction;
		if (page < 0 || page >= pageCount) return null;
		text = await source(page);
		i = direction > 0 ? 0 : (text?.lines.length ?? 0) - 1;
	}
}

/**
 * Where a caret goes by one step forward (1) or back (-1), across lines and pages; the
 * caret itself where there is nowhere to go.
 *
 * - `character`: the end of a line and the start of the next are one place, so each step
 *   passes a character.
 * - `word`: forward to the end of the word, back to the start of it.
 * - `line`: the place on the next line closest to `goal` (a point on the page; the
 *   caret's own, without one). On the last line, its end; on the first, its start.
 * - `paragraph`: forward to the end of the paragraph, back to its start; from there, the
 *   next paragraph's.
 */
export async function moveCaret(
	source: TextSource,
	pageCount: number,
	caret: Caret,
	direction: 1 | -1,
	step: CaretStep,
	goal?: readonly [number, number]
): Promise<Caret> {
	const text = await source(caret.page);
	const line = text?.lines[caret.line];
	if (!text || !line) return caret;
	const forward = direction > 0;
	const length = line.chars.length;
	const atEnd = forward ? caret.offset >= length : caret.offset <= 0;
	const next = () => adjacentLine(source, pageCount, caret.page, caret.line, direction);
	switch (step) {
		case 'character': {
			if (!atEnd) return { ...caret, offset: caret.offset + direction };
			const to = await next();
			if (!to) return caret;
			return { page: to.text.page, line: to.line, offset: forward ? 1 : to.text.lines[to.line]!.chars.length - 1 };
		}
		case 'word': {
			if (!atEnd) return { ...caret, offset: wordBoundary(line, caret.offset, direction) };
			const to = await next();
			if (!to) return caret;
			const l = to.text.lines[to.line]!;
			return { page: to.text.page, line: to.line, offset: wordBoundary(l, forward ? 0 : l.chars.length, direction) };
		}
		case 'line': {
			const to = await next();
			if (!to) return { ...caret, offset: forward ? length : 0 };
			const [x, y] = goal ?? caretPoint(line, caret.offset);
			return { page: to.text.page, line: to.line, offset: offsetInLine(to.text.lines[to.line]!, x, y) };
		}
		case 'paragraph': {
			let here = { text, line: paragraphEnd(text, caret.line, direction) };
			if (atEnd && here.line === caret.line) {
				const to = await next();
				if (!to) return caret;
				here = { text: to.text, line: paragraphEnd(to.text, to.line, direction) };
			}
			return { page: here.text.page, line: here.line, offset: forward ? here.text.lines[here.line]!.chars.length : 0 };
		}
	}
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
