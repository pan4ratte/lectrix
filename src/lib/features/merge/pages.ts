// The Combine view's page list (section 6.4): which page of which file goes where, turned
// how far. Pure functions, so the store and its tests share them.

import type { MergeReport } from '#lib/ipc/index.ts';
import { normalizeRotation, type Rotation } from '#lib/features/viewer/layout.ts';

export interface CombinePage {
	/** Stable identity in the list (pages of the same file can repeat after re-adding it). */
	key: number;
	/** The source document's id. */
	source: number;
	/** Page index in the source (0-based). */
	page: number;
	/** Turn added on top of the page's own rotation. */
	rotation: Rotation;
}

/**
 * Moves the pages in `keys` (keeping their order) into the gap `gap` of the current list:
 * 0 is before the first page, `pages.length` after the last.
 */
export function movePages(pages: CombinePage[], keys: ReadonlySet<number>, gap: number): CombinePage[] {
	const moved = pages.filter((p) => keys.has(p.key));
	if (moved.length === 0) return pages;
	const before = pages.slice(0, Math.max(0, Math.min(gap, pages.length))).filter((p) => !keys.has(p.key)).length;
	const rest = pages.filter((p) => !keys.has(p.key));
	return [...rest.slice(0, before), ...moved, ...rest.slice(before)];
}

/** Moves the selected pages one place earlier (`-1`) or later (`1`), as a block. */
export function nudgePages(pages: CombinePage[], keys: ReadonlySet<number>, direction: 1 | -1): CombinePage[] {
	const indexes = pages.flatMap((p, i) => (keys.has(p.key) ? [i] : []));
	if (indexes.length === 0) return pages;
	const first = indexes[0]!;
	const last = indexes[indexes.length - 1]!;
	if (direction < 0 && first === 0) return pages;
	if (direction > 0 && last === pages.length - 1) return pages;
	// The gap just past the neighbor on that side.
	return movePages(pages, keys, direction < 0 ? first - 1 : last + 2);
}

export function rotatePages(pages: CombinePage[], keys: ReadonlySet<number>, degrees: number): CombinePage[] {
	return pages.map((p) => (keys.has(p.key) ? { ...p, rotation: normalizeRotation(p.rotation + degrees) } : p));
}

export function removePages(pages: CombinePage[], keys: ReadonlySet<number>): CombinePage[] {
	return pages.filter((p) => !keys.has(p.key));
}

/** The keys from `anchor` to `to` inclusive, in list order (shift-click). */
export function rangeKeys(pages: CombinePage[], anchor: number, to: number): number[] {
	const a = pages.findIndex((p) => p.key === anchor);
	const b = pages.findIndex((p) => p.key === to);
	if (a < 0 || b < 0) return b >= 0 ? [to] : [];
	const [from, end] = a <= b ? [a, b] : [b, a];
	return pages.slice(from, end + 1).map((p) => p.key);
}

/** The grid's cell geometry, in CSS pixels. */
export interface Grid {
	columns: number;
	/** Distance between cell origins, horizontally and vertically. */
	pitchX: number;
	pitchY: number;
	/** Padding around the cells. */
	pad: number;
}

export function gridFor(width: number, cellWidth: number, cellHeight: number, gap: number, pad: number): Grid {
	const columns = Math.max(1, Math.floor((width - 2 * pad + gap) / (cellWidth + gap)));
	return { columns, pitchX: cellWidth + gap, pitchY: cellHeight + gap, pad };
}

/** A place between pages to drop dragged pages: its index (0 is before the first page)
 * and where to draw it in the grid's content. */
export interface Gap {
	gap: number;
	x: number;
	y: number;
}

/** The gap nearest a point in the grid's content, in the row under the pointer. */
export function gapAt(grid: Grid, count: number, x: number, y: number): Gap {
	if (count === 0) return { gap: 0, x: grid.pad, y: grid.pad };
	const rows = Math.ceil(count / grid.columns);
	const row = Math.max(0, Math.min(rows - 1, Math.floor((y - grid.pad) / grid.pitchY)));
	const cellsInRow = Math.min(grid.columns, count - row * grid.columns);
	const col = Math.max(0, Math.min(cellsInRow, Math.round((x - grid.pad) / grid.pitchX)));
	return { gap: row * grid.columns + col, x: grid.pad + col * grid.pitchX, y: grid.pad + row * grid.pitchY };
}

/** Undo history of snapshots (the view's own; files are not changed until combining). */
export class History<T> {
	private past: T[] = [];
	private future: T[] = [];

	constructor(private readonly limit = 100) {}

	/** Records the state before a change. */
	push(before: T) {
		this.past.push(before);
		if (this.past.length > this.limit) this.past.shift();
		this.future = [];
	}

	undo(current: T): T | null {
		const previous = this.past.pop();
		if (previous === undefined) return null;
		this.future.push(current);
		return previous;
	}

	redo(current: T): T | null {
		const next = this.future.pop();
		if (next === undefined) return null;
		this.past.push(current);
		return next;
	}

	get canUndo() {
		return this.past.length > 0;
	}

	get canRedo() {
		return this.future.length > 0;
	}
}

/** "a", "a and b", "a, b and c". */
function list(items: string[]): string {
	return items.length <= 2 ? items.join(' and ') : `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

function count(n: number, one: string, many: string) {
	return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

/** "12 pages from 3 files". */
export function summary(pageCount: number, fileCount: number): string {
	return `${count(pageCount, 'page', 'pages')} from ${count(fileCount, 'file', 'files')}`;
}

/**
 * The question asked before pages of signed files are copied: a
 * signature is valid only in the file it signed, so it shows as invalid where its pages
 * go. The fields are copied as they are; nothing is removed.
 */
export function signedWarning(names: string[], inserting: boolean) {
	const one = names.length === 1;
	return {
		title: one ? 'This file is signed' : 'These files are signed',
		message: `The ${one ? 'signature' : 'signatures'} in ${list(names)} will not be valid in ${inserting ? 'this document' : 'the combined file'}.`,
		detail: `A signature is valid only in the file that was signed. The signature ${one ? 'field is' : 'fields are'} copied as ${one ? 'it is' : 'they are'}, and readers will show ${one ? 'it' : 'them'} as invalid. The original ${one ? 'file is' : 'files are'} not changed.`,
		buttons: [
			{ id: 'go', label: inserting ? 'Insert anyway' : 'Combine anyway', primary: true },
			{ id: 'cancel', label: 'Cancel' }
		],
		cancel: 'cancel'
	};
}

/** What the user should know about a combine or insert beyond "it worked", or null. */
export function reportDetail(report: MergeReport): string | null {
	const parts: string[] = [];
	const renamed: string[] = [];
	if (report.renamedDestinations) renamed.push(count(report.renamedDestinations, 'link target', 'link targets'));
	if (report.renamedFields) renamed.push(count(report.renamedFields, 'form field', 'form fields'));
	if (report.renamedAttachments) {
		renamed.push(count(report.renamedAttachments, 'attached file', 'attached files'));
	}
	if (renamed.length) {
		const total = report.renamedDestinations + report.renamedFields + report.renamedAttachments;
		parts.push(
			`${list(renamed)} ${total === 1 ? 'was' : 'were'} renamed because another file used the same name.`
		);
	}
	const dropped = report.droppedLinks + report.droppedBookmarks;
	if (dropped) {
		const what: string[] = [];
		if (report.droppedLinks) what.push(count(report.droppedLinks, 'link', 'links'));
		if (report.droppedBookmarks) what.push(count(report.droppedBookmarks, 'bookmark', 'bookmarks'));
		parts.push(`${list(what)} to pages you left out ${dropped === 1 ? 'was' : 'were'} removed.`);
	}
	if (report.bookmarksSkipped) {
		parts.push('The document’s bookmarks are damaged, so the inserted file’s bookmarks were not added.');
	}
	return parts.length ? parts.join(' ') : null;
}
