import { describe, expect, it } from 'vitest';

import type { Difference, DifferenceKind, DifferenceSide } from '#lib/ipc/index.ts';

import { copyText, countText, identical, kindShown, mapPlace, markAt, marksByPage, pageName, placeIn, progressText, sections } from './compare.ts';

function side(text: string, page: number, rect: [number, number, number, number] | null): DifferenceSide {
	return {
		text,
		cut: false,
		page,
		rects: rect ? [{ page, rect }] : [],
		caret: rect ? null : { page, x: 100, y0: 50, y1: 60 }
	};
}

function diff(kind: Difference['kind'], group: Difference['group'], old: DifferenceSide, nw: DifferenceSide): Difference {
	return { kind, group, old, new: nw };
}

const changed = diff('changed', 'body', side('brown', 0, [10, 10, 40, 20]), side('red', 0, [10, 10, 30, 20]));
const inserted = diff('inserted', 'margins', side('', 1, null), side('v2', 2, [50, 5, 60, 12]));
const deleted = diff('deleted', 'body', side('lazy', 3, [5, 5, 25, 15]), side('', 4, null));

describe('mapPlace', () => {
	const anchors = [
		{ old: 0.5, new: 0.5 },
		// The new document has a page inserted after the first.
		{ old: 1.2, new: 2.2 },
		{ old: 2.5, new: 3.5 }
	];

	it('follows the anchors both ways', () => {
		expect(mapPlace(anchors, 1.2, 'old', 3, 4)).toBeCloseTo(2.2);
		expect(mapPlace(anchors, 0.85, 'old', 3, 4)).toBeCloseTo(1.35);
		expect(mapPlace(anchors, 3.5, 'new', 4, 3)).toBeCloseTo(2.5);
		// Before the first anchor, from the start; after the last, to the end.
		expect(mapPlace(anchors, 0.25, 'old', 3, 4)).toBeCloseTo(0.25);
		expect(mapPlace(anchors, 2.75, 'old', 3, 4)).toBeCloseTo(3.75);
	});

	it('keeps the same place without anchors, within the other document', () => {
		expect(mapPlace([], 2.5, 'old', 5, 3)).toBe(2.5);
		expect(mapPlace([], 4.5, 'old', 5, 3)).toBe(3);
	});

	it('skips anchors that go back', () => {
		const back = [
			{ old: 1, new: 2 },
			{ old: 1.5, new: 1.5 },
			{ old: 2, new: 3 }
		];
		const at = mapPlace(back, 1.5, 'old', 3, 4);
		expect(at).toBeGreaterThan(2);
		expect(at).toBeLessThan(3);
	});
});

describe('marks', () => {
	it('puts text and carets on their pages, without the hidden ones', () => {
		const list = [changed, inserted, deleted];
		const old = marksByPage(list, 'old', new Set([2]));
		expect([...old.keys()]).toEqual([0, 1]);
		expect(old.get(1)![0]!.caret).toBe(true);
		const nw = marksByPage(list, 'new', new Set());
		expect(nw.get(4)![0]).toMatchObject({ id: 2, kind: 'deleted', caret: true });
	});

	it('finds the mark under a point, a caret with some slop', () => {
		const marks = marksByPage([changed, inserted], 'old', new Set()).get(1)!;
		expect(markAt(marks, 101.5, 55)).toBe(1);
		expect(markAt(marks, 110, 55)).toBeNull();
		expect(markAt(marksByPage([changed], 'old', new Set()).get(0)!, 20, 15)).toBe(0);
	});

	it('brings the text or the caret into view', () => {
		expect(placeIn(changed, 'new')).toEqual({ page: 0, rect: [10, 10, 30, 20] });
		expect(placeIn(inserted, 'old')).toEqual({ page: 1, rect: [100, 50, 101, 60] });
	});
});

describe('the list', () => {
	const list = [changed, inserted, deleted, { ...changed }];

	it('groups the differences and leaves out the ignored ones', () => {
		const grouped = sections(list, true, new Set([2]), false);
		expect(grouped.map((s) => [s.title, s.ids])).toEqual([
			['Body text', [0, 3]],
			['Headers and footers', [1]]
		]);
		expect(sections(list, false, new Set([2]), true)).toEqual([{ group: null, title: 'Differences', ids: [0, 1, 2, 3] }]);
		expect(sections(list, true, new Set([0, 1, 2, 3]), false)).toEqual([]);
	});

	it('lists the kinds picked in the filter, or all while none is', () => {
		const ids = (kinds: DifferenceKind[]) => sections(list, false, new Set(), false, new Set(kinds)).flatMap((s) => s.ids);
		expect(ids([])).toEqual([0, 1, 2, 3]);
		expect(ids(['changed'])).toEqual([0, 3]);
		expect(ids(['inserted', 'deleted'])).toEqual([1, 2]);
		expect(kindShown('deleted', new Set(['changed']))).toBe(false);
	});

	it('finds identical differences', () => {
		expect(identical(list, 0)).toEqual([0, 3]);
		expect(identical(list, 1)).toEqual([1]);
	});

	it('names pages and counts', () => {
		expect(pageName(2, ['i', 'ii', 'iii'])).toBe('iii');
		expect(pageName(2, null)).toBe('3');
		expect(pageName(2, ['i', 'ii', ''])).toBe('3');
		expect(countText(0)).toBe('No differences');
		expect(countText(1)).toBe('1 difference');
		expect(countText(1200)).toBe(`${(1200).toLocaleString()} differences`);
	});

	it('copies a difference as text', () => {
		const text = copyText(deleted, { old: 'a.pdf', new: 'b.pdf' }, { old: null, new: null });
		expect(text).toBe('Deleted\na.pdf, page 4: lazy\nb.pdf, page 5: —');
	});

	it('says how far comparing is', () => {
		expect(progressText({ stage: 'reading', done: 5, total: 20 })).toEqual({ text: 'Reading pages… 5 of 20', share: 0.25 });
		expect(progressText({ stage: 'comparing' }).share).toBeNull();
	});
});
