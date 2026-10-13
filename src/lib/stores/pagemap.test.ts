import { describe, expect, it } from 'vitest';

import { PageMove } from './pagemap.ts';

describe('following pages through a change', () => {
	it('finds a moved page in its new place', () => {
		// Six pages; the first two went between the fifth and the sixth.
		const move = new PageMove([2, 3, 4, 0, 1, 5]);
		expect(move.to(0)).toBe(3);
		expect(move.to(2)).toBe(0);
		expect(move.to(5)).toBe(5);
		expect(move.position({ page: 1, offset: 0.4 })).toEqual({ page: 4, offset: 0.4 });
	});

	it('gives the page in its place for a deleted one', () => {
		// Pages 2 and 3 of six are gone.
		const move = new PageMove([0, 3, 4, 5]);
		expect(move.to(1)).toBeNull();
		expect(move.follow(1)).toBe(1);
		expect(move.follow(4)).toBe(2);
		expect(move.position({ page: 2, offset: 0.7 })).toEqual({ page: 2, offset: 0 });
		// The last page deleted: the last one left.
		expect(new PageMove([0, 1]).follow(2)).toBe(1);
	});

	it('knows nothing of a page that was not there', () => {
		// Two pages inserted before the second.
		const move = new PageMove([0, null, null, 1, 2]);
		expect(move.to(1)).toBe(3);
		expect(move.follow(2)).toBe(4);
	});

	it('keeps each page its key and gives new pages fresh ones', () => {
		let next = 100;
		const fresh = () => next++;
		expect(new PageMove([2, 0, 1]).keys([7, 8, 9], fresh)).toEqual([9, 7, 8]);
		expect(new PageMove([0, null, 1]).keys([7, 8], fresh)).toEqual([7, 100, 8]);
		expect(new PageMove([1]).keys([7, 8], fresh)).toEqual([8]);
	});
});
