import { describe, expect, it } from 'vitest';

import {
	History,
	movePages,
	nudgePages,
	removePages,
	reportDetail,
	rotatePages,
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
