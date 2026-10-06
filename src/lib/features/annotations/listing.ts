// What the annotation list shows (section 6.5): filters by type, colour and author, search
// in comments or in the marked text, and the order and groups of its rows.

import type { Annotation } from '#lib/ipc/index.ts';
import type { Line } from '#lib/features/viewer/selection.ts';

import { MARKUP_SUBTYPES } from './tools.ts';

export type ListSort = 'page' | 'author' | 'created' | 'modified';
/** A to Z (first page, oldest date first) or Z to A. */
export type SortOrder = 'asc' | 'desc';

export const SORTS: readonly { id: ListSort; label: string }[] = [
	{ id: 'page', label: 'Page' },
	{ id: 'author', label: 'Author' },
	{ id: 'created', label: 'Date created' },
	{ id: 'modified', label: 'Date modified' }
];

/** The two orders, in the words that suit `sort`. */
export function orderLabels(sort: ListSort): Record<SortOrder, string> {
	if (sort === 'page') return { asc: 'A–Z (first page first)', desc: 'Z–A (last page first)' };
	if (sort === 'author') return { asc: 'A–Z', desc: 'Z–A' };
	return { asc: 'A–Z (oldest first)', desc: 'Z–A (newest first)' };
}

/** The values picked in each filter; an empty set lets everything through. */
export interface AnnotationFilter {
	types: ReadonlySet<string>;
	colors: ReadonlySet<string>;
	authors: ReadonlySet<string>;
}

export function emptyFilter(): AnnotationFilter {
	return { types: new Set(), colors: new Set(), authors: new Set() };
}

export function filterCount(f: AnnotationFilter): number {
	return f.types.size + f.colors.size + f.authors.size;
}

/** An annotation's colour as the colour filter knows it. */
export function colorKey(a: Annotation): string | null {
	return a.color && /^#[0-9a-f]{6}$/i.test(a.color) ? a.color.toLowerCase() : null;
}

/** Any of the picked values in each filter that has some, all filters together. */
export function matchesFilter(a: Annotation, f: AnnotationFilter): boolean {
	if (f.types.size && !f.types.has(a.subtype)) return false;
	if (f.authors.size && !f.authors.has(a.author)) return false;
	if (f.colors.size) {
		const c = colorKey(a);
		if (c === null || !f.colors.has(c)) return false;
	}
	return true;
}

/** Text and a query compared ignoring case and how much white space separates words. */
function fold(text: string): string {
	return text.toLocaleLowerCase().replace(/\s+/g, ' ').trim();
}

/** Whether `text` contains `query` (an empty query matches everything). */
export function matchesSearch(text: string, query: string): boolean {
	const q = fold(query);
	return q === '' || fold(text).includes(q);
}

/** Text markup: the annotations that mark text, which "Marked text" searches. */
export function marksText(a: Annotation): boolean {
	return MARKUP_SUBTYPES.some((m) => m.subtype === a.subtype);
}

/** Whether point (x, y) is inside the convex quad ul, ur, ll, lr (8 numbers at `q[i]`). */
function inQuad(q: readonly number[], i: number, x: number, y: number): boolean {
	// Around the edge in order: ul, ur, lr, ll.
	const xs = [q[i]!, q[i + 2]!, q[i + 6]!, q[i + 4]!];
	const ys = [q[i + 1]!, q[i + 3]!, q[i + 7]!, q[i + 5]!];
	let sign = 0;
	for (let k = 0; k < 4; k++) {
		const j = (k + 1) % 4;
		const cross = (xs[j]! - xs[k]!) * (y - ys[k]!) - (ys[j]! - ys[k]!) * (x - xs[k]!);
		if (cross === 0) continue;
		const s = Math.sign(cross);
		if (sign === 0) sign = s;
		else if (s !== sign) return false;
	}
	return true;
}

/**
 * The text a markup annotation marks: the characters whose centres fall inside its quads,
 * line by line, lines joined with a space.
 */
export function markedText(lines: readonly Line[], quads: readonly number[]): string {
	const out: string[] = [];
	for (const line of lines) {
		let text = '';
		for (let c = 0; c < line.chars.length; c++) {
			const x = (line.boxes[c * 4]! + line.boxes[c * 4 + 2]!) / 2;
			const y = (line.boxes[c * 4 + 1]! + line.boxes[c * 4 + 3]!) / 2;
			for (let i = 0; i + 8 <= quads.length; i += 8) {
				if (inQuad(quads, i, x, y)) {
					text += line.chars[c];
					break;
				}
			}
		}
		if (text.trim()) out.push(text.trim());
	}
	return out.join(' ');
}

/** The value rows sort on; null (no author, no date) sorts last either way. */
function sortValue(a: Annotation, sort: ListSort): string | number | null {
	if (sort === 'page') return a.page;
	if (sort === 'author') return a.author || null;
	return sort === 'created' ? a.created : a.modified;
}

/**
 * The rows in `sort` order, A to Z (first page, A, oldest date first) or Z to A. Rows
 * without the value go last either way; ties keep their order on the page.
 */
export function sortAnnotations(list: readonly Annotation[], sort: ListSort, order: SortOrder = 'asc'): Annotation[] {
	const sign = order === 'asc' ? 1 : -1;
	const indexed = list.map((a, i) => ({ a, i, v: sortValue(a, sort) }));
	indexed.sort((x, y) => {
		if (x.v === y.v) return x.i - y.i;
		if (x.v === null) return 1;
		if (y.v === null) return -1;
		const c = typeof x.v === 'string' ? x.v.localeCompare(String(y.v)) : x.v - (y.v as number);
		return sign * c || x.i - y.i;
	});
	return indexed.map((x) => x.a);
}

export interface ListGroup {
	key: string;
	title: string;
	items: Annotation[];
}

/**
 * Sorted rows under headings: the page (by page), the author (by author) or the day (by a
 * date). Consecutive rows with the same heading share it.
 */
export function groupAnnotations(
	sorted: readonly Annotation[],
	sort: ListSort,
	pageTitle: (page: number) => string,
	dayTitle: (ms: number) => string
): ListGroup[] {
	const heading = (a: Annotation): string => {
		if (sort === 'page') return pageTitle(a.page);
		if (sort === 'author') return a.author || 'Unknown author';
		const ms = sort === 'created' ? a.created : a.modified;
		return ms === null ? 'No date' : dayTitle(ms);
	};
	const out: ListGroup[] = [];
	for (const a of sorted) {
		const title = heading(a);
		const last = out.at(-1);
		if (last && last.title === title) last.items.push(a);
		else out.push({ key: `${out.length}:${title}`, title, items: [a] });
	}
	return out;
}
