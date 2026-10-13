import { describe, expect, it } from 'vitest';

import { caption, clampThumb, deleteQuestion, deletedMessage, describePage, nudgeGap, stepThumb, THUMB_SIZES } from './pages.ts';

describe('what the grid says about a page', () => {
	const labels = ['i', 'ii', '', '4', 'A-1'];

	it('shows the label, and the place where that says something else', () => {
		expect(caption(0, labels)).toBe('i (1)');
		expect(caption(3, labels)).toBe('4');
		expect(caption(4, labels)).toBe('A-1 (5)');
		// A page whose label is empty has its place alone.
		expect(caption(2, labels)).toBe('(3)');
		// Without labels, the page number.
		expect(caption(6, null)).toBe('7');
	});

	it('names a page by its place and its label', () => {
		expect(describePage(1, 5, labels)).toBe('2 of 5: page ii');
		expect(describePage(2, 5, labels)).toBe('3 of 5: page 3');
		expect(describePage(2, 5, null)).toBe('3 of 5: page 3');
	});
});

describe('picture sizes', () => {
	it('step through the sizes and stop at the ends', () => {
		expect(stepThumb(160, 1)).toBe(208);
		expect(stepThumb(160, -1)).toBe(128);
		expect(stepThumb(THUMB_SIZES[0], -1)).toBeNull();
		expect(stepThumb(THUMB_SIZES[THUMB_SIZES.length - 1]!, 1)).toBeNull();
	});

	it('step from a size between two of them, as the wheel and a pinch leave', () => {
		expect(stepThumb(140, 1)).toBe(160);
		expect(stepThumb(140, -1)).toBe(128);
		// A hair off a size is that size.
		expect(stepThumb(159.8, 1)).toBe(208);
		expect(stepThumb(160.3, -1)).toBe(128);
	});

	it('stay between the smallest and the largest', () => {
		expect(clampThumb(20)).toBe(THUMB_SIZES[0]);
		expect(clampThumb(1000)).toBe(THUMB_SIZES[THUMB_SIZES.length - 1]);
		expect(clampThumb(141.5)).toBe(141.5);
		expect(clampThumb(Number.NaN)).toBe(160);
	});
});

describe('nudging', () => {
	it('moves the pages one place as a block', () => {
		// Pages 3 and 4 of six, earlier: before page 2. Later: after page 5.
		expect(nudgeGap([2, 3], 6, -1)).toBe(1);
		expect(nudgeGap([2, 3], 6, 1)).toBe(5);
		// Pages apart come together around the same end.
		expect(nudgeGap([1, 4], 6, -1)).toBe(0);
		expect(nudgeGap([1, 4], 6, 1)).toBe(6);
	});

	it('stops at the ends', () => {
		expect(nudgeGap([0, 1], 6, -1)).toBeNull();
		expect(nudgeGap([4, 5], 6, 1)).toBeNull();
		expect(nudgeGap([], 6, 1)).toBeNull();
	});
});

describe('deleting pages', () => {
	it('asks first, naming one page by its label', () => {
		const one = deleteQuestion([1], ['i', 'ii', '1']);
		expect(one.title).toBe('Delete page ii?');
		expect(one.message).toBe('Links and bookmarks that lead to it are deleted with it.');
		expect(one.buttons[0]).toEqual({ id: 'delete', label: 'Delete', danger: true });
		expect(one.cancel).toBe('cancel');
		const many = deleteQuestion([0, 1, 2], null);
		expect(many.title).toBe('Delete 3 pages?');
		expect(many.message).toBe('Links and bookmarks that lead to them are deleted with them.');
	});

	it('says what went with the pages, and that they stay in the file', () => {
		const stays = 'The pages stay inside the file, where they can be recovered, until you save with Save As (optimized).';
		expect(deletedMessage({ pages: 2, links: 0, bookmarks: 0, fields: 0 })).toEqual({
			message: 'Deleted 2 pages.',
			suggestion: stays
		});
		expect(deletedMessage({ pages: 2, links: 3, bookmarks: 1, fields: 2 }).suggestion).toBe(
			`3 links and 1 bookmark that led to them were deleted too. 2 form fields on them were deleted too. ${stays}`
		);
		expect(deletedMessage({ pages: 1, links: 0, bookmarks: 1, fields: 0 })).toEqual({
			message: 'Deleted 1 page.',
			suggestion:
				'1 bookmark that led to it was deleted too. The page stays inside the file, where it can be recovered, until you save with Save As (optimized).'
		});
	});
});
