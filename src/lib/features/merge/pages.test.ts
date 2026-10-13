import { describe, expect, it } from 'vitest';

import {
	History,
	gatherPages,
	moveItem,
	movePages,
	nudgePages,
	removePages,
	reportDetail,
	rotatePages,
	rowGapAt,
	signedWarning,
	summary,
	type CombinePage
} from './pages.ts';

function pages(n: number): CombinePage[] {
	return Array.from({ length: n }, (_, i) => ({ key: i + 1, source: i < 3 ? 10 : 20, page: i, rotation: 0 }));
}

const keysOf = (list: CombinePage[]) => list.map((p) => p.key);

describe('moving pages', () => {
	it('moves a block into a gap, keeping its order', () => {
		const list = pages(6);
		expect(keysOf(movePages(list, new Set([2, 4]), 0))).toEqual([2, 4, 1, 3, 5, 6]);
		expect(keysOf(movePages(list, new Set([2, 4]), 6))).toEqual([1, 3, 5, 6, 2, 4]);
		// Gap 5 is before page 6: counted in the list as it is now.
		expect(keysOf(movePages(list, new Set([1, 2]), 5))).toEqual([3, 4, 5, 1, 2, 6]);
		// Dropping a page next to itself changes nothing.
		expect(keysOf(movePages(list, new Set([3]), 3))).toEqual([1, 2, 3, 4, 5, 6]);
		expect(keysOf(movePages(list, new Set([3]), 2))).toEqual([1, 2, 3, 4, 5, 6]);
	});

	it('nudges the selection one place, stopping at the ends', () => {
		const list = pages(5);
		expect(keysOf(nudgePages(list, new Set([2, 3]), 1))).toEqual([1, 4, 2, 3, 5]);
		expect(keysOf(nudgePages(list, new Set([2, 3]), -1))).toEqual([2, 3, 1, 4, 5]);
		expect(nudgePages(list, new Set([1]), -1)).toBe(list);
		expect(nudgePages(list, new Set([5]), 1)).toBe(list);
	});
});

describe('moving files', () => {
	it('drops an item into a gap of the list as it is', () => {
		const list = ['a', 'b', 'c', 'd'];
		expect(moveItem(list, 0, 2)).toEqual(['b', 'a', 'c', 'd']);
		expect(moveItem(list, 0, 4)).toEqual(['b', 'c', 'd', 'a']);
		expect(moveItem(list, 3, 0)).toEqual(['d', 'a', 'b', 'c']);
		expect(moveItem(list, 2, 1)).toEqual(['a', 'c', 'b', 'd']);
		// The gaps beside it leave it where it is.
		expect(moveItem(list, 1, 1)).toEqual(list);
		expect(moveItem(list, 1, 2)).toEqual(list);
		// Past the ends is the ends; an item that is not there moves nothing.
		expect(moveItem(list, 1, 99)).toEqual(['a', 'c', 'd', 'b']);
		expect(moveItem(list, 9, 0)).toEqual(list);
	});

	const from = (sources: number[]): CombinePage[] => sources.map((source, i) => ({ key: i + 1, source, page: i, rotation: 0 }));

	it('takes a file’s pages along, as a block before the pages of the file after it', () => {
		// Files 10, 20, 30, their pages in that order.
		const list = from([10, 10, 20, 20, 30]);
		// 30 to the front.
		expect(keysOf(gatherPages(list, [30, 10, 20], 30))).toEqual([5, 1, 2, 3, 4]);
		// 10 after 20.
		expect(keysOf(gatherPages(list, [20, 10, 30], 10))).toEqual([3, 4, 1, 2, 5]);
		// 10 to the end.
		expect(keysOf(gatherPages(list, [20, 30, 10], 10))).toEqual([3, 4, 5, 1, 2]);
	});

	it('brings together pages that were apart, and leaves the others as they are', () => {
		// 10 and 20 alternate. 20 before 10: its pages come first, in the order they had.
		const mixed = from([10, 20, 10, 20, 30]);
		expect(keysOf(gatherPages(mixed, [20, 10, 30], 20))).toEqual([2, 4, 1, 3, 5]);
		// 30 before 20: before 20's first page; 10's and 20's pages keep their order.
		expect(keysOf(gatherPages(mixed, [10, 30, 20], 30))).toEqual([1, 5, 2, 3, 4]);
	});

	it('skips files without pages in the list, and moves nothing for one of them', () => {
		// 20 has no pages left: 10 dropped after it goes before 30's pages.
		const list = from([10, 10, 30]);
		expect(keysOf(gatherPages(list, [20, 10, 30], 10))).toEqual([1, 2, 3]);
		expect(keysOf(gatherPages(list, [30, 20, 10], 10))).toEqual([3, 1, 2]);
		expect(gatherPages(list, [20, 10, 30], 20)).toBe(list);
	});

	it('finds the gap of a list nearest a point', () => {
		const rows = [
			{ top: 0, bottom: 40 },
			{ top: 40, bottom: 80 },
			{ top: 80, bottom: 120 }
		];
		expect(rowGapAt(rows, -5)).toBe(0);
		expect(rowGapAt(rows, 19)).toBe(0);
		expect(rowGapAt(rows, 21)).toBe(1);
		expect(rowGapAt(rows, 70)).toBe(2);
		expect(rowGapAt(rows, 500)).toBe(3);
		expect(rowGapAt([], 10)).toBe(0);
	});
});

describe('page edits', () => {
	it('rotates and removes only the chosen pages', () => {
		const list = pages(3);
		const turned = rotatePages(rotatePages(list, new Set([2]), -90), new Set([2, 3]), 180);
		expect(turned.map((p) => p.rotation)).toEqual([0, 90, 180]);
		expect(keysOf(removePages(list, new Set([1, 3])))).toEqual([2]);
	});
});

describe('history', () => {
	it('undoes and redoes snapshots', () => {
		const h = new History<number>();
		h.push(1);
		h.push(2);
		expect(h.undo(3)).toBe(2);
		expect(h.undo(2)).toBe(1);
		expect(h.undo(1)).toBeNull();
		expect(h.redo(1)).toBe(2);
		h.push(2);
		expect(h.canRedo).toBe(false);
	});
});

describe('messages', () => {
	it('warns that signatures will not be valid where the pages go', () => {
		const one = signedWarning(['contract.pdf'], false);
		expect(one.message).toBe('The signature in contract.pdf will not be valid in the combined file.');
		expect(one.buttons[0]!.label).toBe('Combine anyway');
		const two = signedWarning(['a.pdf', 'b.pdf'], true);
		expect(two.title).toBe('These files are signed');
		expect(two.message).toBe('The signatures in a.pdf and b.pdf will not be valid in this document.');
		expect(two.buttons[0]!.label).toBe('Insert anyway');
	});

	it('summarizes pages and files', () => {
		expect(summary(1, 1)).toBe('1 page from 1 file');
		expect(summary(1200, 3)).toBe(`${(1200).toLocaleString()} pages from 3 files`);
	});

	it('explains renamed names and removed links, or says nothing', () => {
		const none = {
			pages: 4,
			renamedDestinations: 0,
			renamedFields: 0,
			renamedAttachments: 0,
			droppedLinks: 0,
			droppedBookmarks: 0,
			bookmarksSkipped: false
		};
		expect(reportDetail(none)).toBeNull();
		expect(reportDetail({ ...none, renamedDestinations: 2, renamedFields: 1, droppedLinks: 1 })).toBe(
			'2 link targets and 1 form field were renamed because another file used the same name. 1 link to pages you left out was removed.'
		);
		expect(reportDetail({ ...none, renamedFields: 1 })).toBe(
			'1 form field was renamed because another file used the same name.'
		);
		expect(reportDetail({ ...none, renamedFields: 1, renamedAttachments: 2 })).toBe(
			'1 form field and 2 attached files were renamed because another file used the same name.'
		);
	});
});
