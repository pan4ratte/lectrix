import { describe, expect, it } from 'vitest';

import type { Bookmark, BookmarkTarget } from '#lib/ipc/index.ts';

import {
	describeTarget,
	dropPosition,
	dropZone,
	insertionPoint,
	keyboardMove,
	locate,
	moveTo,
	selectionAfterDelete,
	titleFromText,
	visibleRows
} from './tree.ts';

function bm(id: number, page: number | null, children: Bookmark[] = [], open = false): Bookmark {
	const target: BookmarkTarget =
		page === null ? { kind: 'none' } : { kind: 'page', page, x: null, y: 0, named: null };
	return { id, title: `B${id}`, open, target, bold: false, italic: false, color: null, children };
}

// 1 (open): 2, 3 (closed): 4 ; 5 ; 6
function sample(): Bookmark[] {
	return [bm(1, 0, [bm(2, 1), bm(3, 2, [bm(4, 3)])], true), bm(5, 10), bm(6, 20)];
}

describe('visible rows', () => {
	it('lists open branches only, with depth and position', () => {
		const rows = visibleRows(sample());
		expect(rows.map((r) => [r.bookmark.id, r.depth, r.parent, r.index, r.siblings])).toEqual([
			[1, 0, null, 0, 3],
			[2, 1, 1, 0, 2],
			[3, 1, 1, 1, 2],
			[5, 0, null, 1, 3],
			[6, 0, null, 2, 3]
		]);
	});
});

describe('insertion point for Ctrl+B', () => {
	it('goes after the selected bookmark', () => {
		expect(insertionPoint(sample(), 2, { page: 50, y: 0 })).toEqual({ parent: 1, index: 1 });
	});
	it('keeps the top level in page order when nothing is selected', () => {
		const items = sample();
		expect(insertionPoint(items, null, { page: 0, y: -1 })).toEqual({ parent: null, index: 0 });
		expect(insertionPoint(items, null, { page: 12, y: 0 })).toEqual({ parent: null, index: 2 });
		expect(insertionPoint(items, null, { page: 99, y: 0 })).toEqual({ parent: null, index: 3 });
		// Same page: after bookmarks higher up on it.
		expect(insertionPoint(items, null, { page: 10, y: 5 })).toEqual({ parent: null, index: 2 });
	});
	it('ignores bookmarks that lead nowhere in the document', () => {
		const items = [bm(1, null), bm(2, 5)];
		expect(insertionPoint(items, null, { page: 1, y: 0 })).toEqual({ parent: null, index: 0 });
	});
});

describe('moves', () => {
	it('counts the index after the bookmark leaves its place', () => {
		const items = sample();
		// 5 to after 6 (index 3 before removal) -> index 2 after.
		expect(moveTo(items, 5, null, 3)).toEqual({ parent: null, index: 2 });
		// Into another parent: no adjustment.
		expect(moveTo(items, 5, 1, 0)).toEqual({ parent: 1, index: 0 });
	});
	it('refuses no-ops and moves into itself', () => {
		const items = sample();
		expect(moveTo(items, 5, null, 1)).toBeNull();
		expect(moveTo(items, 5, null, 2)).toBeNull();
		expect(moveTo(items, 1, 3, 0)).toBeNull();
		expect(moveTo(items, 3, 4, 0)).toBeNull();
		expect(moveTo(items, 3, 3, 0)).toBeNull();
	});
	it('maps drop zones to positions', () => {
		const items = sample();
		const rows = visibleRows(items);
		const row = (id: number) => rows.find((r) => r.bookmark.id === id)!;
		expect(dropZone(2, 28)).toBe('before');
		expect(dropZone(14, 28)).toBe('inside');
		expect(dropZone(26, 28)).toBe('after');
		expect(dropPosition(items, 6, row(2), 'before')).toEqual({ parent: 1, index: 0 });
		expect(dropPosition(items, 6, row(5), 'inside')).toEqual({ parent: 5, index: 0 });
		// After an open bookmark: becomes its first child.
		expect(dropPosition(items, 6, row(1), 'after')).toEqual({ parent: 1, index: 0 });
		expect(dropPosition(items, 2, row(5), 'after')).toEqual({ parent: null, index: 2 });
		expect(dropPosition(items, 1, row(2), 'inside')).toBeNull();
	});
	it('moves with the keyboard', () => {
		const items = sample();
		expect(keyboardMove(items, 5, 'up')).toEqual({ parent: null, index: 0 });
		expect(keyboardMove(items, 5, 'down')).toEqual({ parent: null, index: 2 });
		expect(keyboardMove(items, 6, 'down')).toBeNull();
		expect(keyboardMove(items, 5, 'in')).toEqual({ parent: 1, index: 2 });
		expect(keyboardMove(items, 1, 'in')).toBeNull();
		expect(keyboardMove(items, 4, 'out')).toEqual({ parent: 1, index: 2 });
		expect(keyboardMove(items, 2, 'out')).toEqual({ parent: null, index: 1 });
		expect(keyboardMove(items, 5, 'out')).toBeNull();
	});
});

describe('selection after delete', () => {
	it('prefers the next sibling, then the previous one, then the parent', () => {
		const items = sample();
		expect(selectionAfterDelete(items, 2)).toBe(3);
		expect(selectionAfterDelete(items, 3)).toBe(2);
		expect(selectionAfterDelete(items, 4)).toBe(3);
		expect(selectionAfterDelete(items, 6)).toBe(5);
	});
});

describe('titles and descriptions', () => {
	it('makes one short line from selected text', () => {
		expect(titleFromText('  Chapter\n 1:\tThe   Start ')).toBe('Chapter 1: The Start');
		expect(titleFromText('x'.repeat(300))).toHaveLength(200);
	});
	it('describes targets in plain language', () => {
		const labels = ['i', 'ii', '1'];
		expect(describeTarget({ kind: 'page', page: 1, x: null, y: null, named: null }, labels)).toBe('Page ii (2)');
		expect(describeTarget({ kind: 'page', page: 0, x: null, y: null, named: null }, null)).toBe('Page 1');
		expect(describeTarget({ kind: 'page', page: 2, x: 1, y: 2, named: 'ch1' }, ['a', 'b', '3'])).toBe(
			'Page 3, through the named destination “ch1”'
		);
		expect(describeTarget({ kind: 'uri', uri: 'https://x.org' }, null)).toBe('Web link: https://x.org');
	});
	it('locates nested bookmarks with their ancestors', () => {
		const loc = locate(sample(), 4)!;
		expect(loc.ancestors.map((a) => a.id)).toEqual([1, 3]);
		expect(loc.index).toBe(0);
	});
});
