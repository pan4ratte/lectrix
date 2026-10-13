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

/**
 * `items` with the one at `from` dropped into gap `gap` of the list as it is: 0 is before
 * the first, `items.length` after the last. The gaps on either side of it leave it where it
 * is.
 */
export function moveItem<T>(items: readonly T[], from: number, gap: number): T[] {
	const item = items[from];
	if (item === undefined) return [...items];
	const rest = items.filter((_, i) => i !== from);
	const at = Math.max(0, Math.min(rest.length, gap > from ? gap - 1 : gap));
	return [...rest.slice(0, at), item, ...rest.slice(at)];
}

/**
 * The pages after file `source` took a new place in the list of files, `order` (the
 * files' ids as they are listed now). Its pages come together, in the order they have,
 * before the first page of the file listed after it (the next one that has pages), or at
 * the end without such a file. The other pages stay as they are.
 */
export function gatherPages(pages: CombinePage[], order: readonly number[], source: number): CombinePage[] {
	const own = pages.filter((p) => p.source === source);
	if (own.length === 0) return pages;
	const rest = pages.filter((p) => p.source !== source);
	let at = rest.length;
	for (const next of order.slice(order.indexOf(source) + 1)) {
		const first = rest.findIndex((p) => p.source === next);
		if (first >= 0) {
			at = first;
			break;
		}
	}
	return [...rest.slice(0, at), ...own, ...rest.slice(at)];
}

/**
 * The gap of a list a point is nearest to: `rows` are its rows' tops and bottoms, in
 * order, and `y` is the point, measured as they are. 0 is before the first row.
 */
export function rowGapAt(rows: readonly { top: number; bottom: number }[], y: number): number {
	const after = rows.findIndex((row) => y < (row.top + row.bottom) / 2);
	return after < 0 ? rows.length : after;
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
