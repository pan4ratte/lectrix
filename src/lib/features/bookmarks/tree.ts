// Bookmark tree logic shared by the panel, the inspector and the shortcuts: visible rows,
// where new and moved bookmarks go, and how targets are described. Pure functions over the
// outline Rust sent; every change goes back to Rust as an operation (section 7).

import type { Bookmark, BookmarkTarget } from '#lib/ipc/index.ts';

/** One visible line of the tree. */
export interface Row {
	bookmark: Bookmark;
	/** 0 for top-level bookmarks. */
	depth: number;
	/** Parent id, or null at the top level. */
	parent: number | null;
	/** Position among its siblings, and how many siblings there are (ARIA). */
	index: number;
	siblings: number;
}

/** Rows shown in the panel: top-level bookmarks, and the children of open ones. */
export function visibleRows(items: readonly Bookmark[]): Row[] {
	const rows: Row[] = [];
	const walk = (list: readonly Bookmark[], depth: number, parent: number | null) => {
		list.forEach((bookmark, index) => {
			rows.push({ bookmark, depth, parent, index, siblings: list.length });
			if (bookmark.open && bookmark.children.length) walk(bookmark.children, depth + 1, bookmark.id);
		});
	};
	walk(items, 0, null);
	return rows;
}

export interface Location {
	bookmark: Bookmark;
	/** The parent bookmark, or null at the top level. */
	parent: Bookmark | null;
	/** The list the bookmark is in. */
	siblings: readonly Bookmark[];
	index: number;
	/** Ancestors from the top level down (not including the bookmark). */
	ancestors: Bookmark[];
}

export function locate(items: readonly Bookmark[], id: number): Location | null {
	const walk = (list: readonly Bookmark[], parent: Bookmark | null, ancestors: Bookmark[]): Location | null => {
		for (let index = 0; index < list.length; index++) {
			const bookmark = list[index]!;
			if (bookmark.id === id) return { bookmark, parent, siblings: list, index, ancestors };
			const found = walk(bookmark.children, bookmark, [...ancestors, bookmark]);
			if (found) return found;
		}
		return null;
	};
	return walk(items, null, []);
}

/** Number of bookmarks below `bookmark`, at any depth. */
export function descendantCount(bookmark: Bookmark): number {
	return bookmark.children.reduce((n, c) => n + 1 + descendantCount(c), 0);
}

/** A place in the tree: child number `index` of `parent` (null: top level). */
export interface Position {
	parent: number | null;
	index: number;
}

/** A point in the document, for ordering bookmarks by where they lead. */
export interface DocPoint {
	page: number;
	y: number;
}

function pointOf(target: BookmarkTarget): DocPoint | null {
	return target.kind === 'page' ? { page: target.page, y: target.y ?? 0 } : null;
}

function compare(a: DocPoint, b: DocPoint): number {
	return a.page - b.page || a.y - b.y;
}

/**
 * Where Ctrl+B puts a new bookmark (Phase 2 review): right after the selected bookmark,
 * as its sibling; with nothing selected, at the top level after the last bookmark that
 * leads to the new one's place or earlier, so the top level stays in page order.
 */
export function insertionPoint(items: readonly Bookmark[], selected: number | null, at: DocPoint): Position {
	if (selected !== null) {
		const loc = locate(items, selected);
		if (loc) return { parent: loc.parent?.id ?? null, index: loc.index + 1 };
	}
	let index = 0;
	items.forEach((b, i) => {
		const p = pointOf(b.target);
		if (p && compare(p, at) <= 0) index = i + 1;
	});
	return { parent: null, index };
}

/**
 * Turns "place `id` at child number `index` of `parent`, counted before it leaves its old
 * place" into the operation's form (counted after it has left), or null when the move
 * would change nothing or put the bookmark inside itself.
 */
export function moveTo(items: readonly Bookmark[], id: number, parent: number | null, index: number): Position | null {
	const loc = locate(items, id);
	if (!loc) return null;
	if (parent !== null) {
		if (parent === id) return null;
		const target = locate(items, parent);
		if (!target || target.ancestors.some((a) => a.id === id)) return null;
	}
	const currentParent = loc.parent?.id ?? null;
	let adjusted = index;
	if (parent === currentParent && loc.index < index) adjusted -= 1;
	if (parent === currentParent && adjusted === loc.index) return null;
	return { parent, index: adjusted };
}

export type DropZone = 'before' | 'after' | 'inside';

/** Which part of a row the pointer is over: the top and bottom quarters insert next to it. */
export function dropZone(offsetY: number, rowHeight: number): DropZone {
	if (offsetY < rowHeight / 4) return 'before';
	if (offsetY > (rowHeight * 3) / 4) return 'after';
	return 'inside';
}

/** Where dropping `dragged` on `row` puts it, or null if nowhere (see `moveTo`). */
export function dropPosition(items: readonly Bookmark[], dragged: number, row: Row, zone: DropZone): Position | null {
	const b = row.bookmark;
	switch (zone) {
		case 'before':
			return moveTo(items, dragged, row.parent, row.index);
		case 'inside':
			return moveTo(items, dragged, b.id, b.children.length);
		case 'after':
			// Below an open bookmark, the next line is its first child.
			if (b.open && b.children.length) return moveTo(items, dragged, b.id, 0);
			return moveTo(items, dragged, row.parent, row.index + 1);
	}
}

export type KeyboardMove = 'up' | 'down' | 'in' | 'out';

/** Moves for Alt+Shift+arrows and the context menu: among siblings, into the previous
 * sibling (as its last child), or out to follow the parent. */
export function keyboardMove(items: readonly Bookmark[], id: number, move: KeyboardMove): Position | null {
	const loc = locate(items, id);
	if (!loc) return null;
	const parent = loc.parent?.id ?? null;
	switch (move) {
		case 'up':
			return loc.index > 0 ? { parent, index: loc.index - 1 } : null;
		case 'down':
			return loc.index < loc.siblings.length - 1 ? { parent, index: loc.index + 1 } : null;
		case 'in': {
			const previous = loc.siblings[loc.index - 1];
			return previous ? { parent: previous.id, index: previous.children.length } : null;
		}
		case 'out': {
			if (!loc.parent) return null;
			const outer = locate(items, loc.parent.id);
			return outer ? { parent: outer.parent?.id ?? null, index: outer.index + 1 } : null;
		}
	}
}

/** The bookmark to select after `id` is deleted: the next sibling, else the previous one,
 * else the parent. */
export function selectionAfterDelete(items: readonly Bookmark[], id: number): number | null {
	const loc = locate(items, id);
	if (!loc) return null;
	return (loc.siblings[loc.index + 1] ?? loc.siblings[loc.index - 1] ?? loc.parent)?.id ?? null;
}

/** A bookmark title from selected text: one line, at most 200 characters. */
export function titleFromText(text: string): string {
	const line = text.replace(/\s+/g, ' ').trim();
	return line.length > 200 ? `${line.slice(0, 199).trimEnd()}…` : line;
}

/** Plain-language description of where a bookmark leads. */
export function describeTarget(target: BookmarkTarget, labels: readonly string[] | null): string {
	switch (target.kind) {
		case 'page': {
			const label = labels?.[target.page];
			const number = target.page + 1;
			const page = label && label !== String(number) ? `Page ${label} (${number})` : `Page ${number}`;
			return target.named ? `${page}, through the named destination “${target.named}”` : page;
		}
		case 'broken':
			return target.named
				? `The named destination “${target.named}” doesn’t exist in this document`
				: 'Its destination doesn’t exist in this document';
		case 'uri':
			return `Web link: ${target.uri}`;
		case 'file':
			return target.file ? `Opens another file: ${target.file}` : 'Opens another file';
		case 'action':
			return `Runs a ${target.action || 'custom'} action, which Folio doesn’t do`;
		case 'none':
			return 'No destination (a heading)';
	}
}
