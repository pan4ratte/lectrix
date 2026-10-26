import { describe, expect, it } from 'vitest';

import { toolSpans } from './starttools';

/** The rows the cards make, each as the spans of its cards. */
function rows(count: number, columns: 2 | 3 = 3): number[][] {
	const out: number[][] = [];
	let row: number[] = [];
	let used = 0;
	for (const span of toolSpans(count, columns)) {
		row.push(span);
		used += span;
		if (used === 6) {
			out.push(row);
			row = [];
			used = 0;
		}
	}
	expect(row, `${count} cards leave a row unfilled`).toEqual([]);
	return out;
}

describe('toolSpans', () => {
	it('puts three cards to a row', () => {
		expect(rows(6)).toEqual([
			[2, 2, 2],
			[2, 2, 2]
		]);
		expect(rows(9)).toHaveLength(3);
	});

	it('stretches a last row that is not full across the grid', () => {
		expect(rows(7)).toEqual([[2, 2, 2], [2, 2, 2], [6]]);
		expect(rows(8)).toEqual([
			[2, 2, 2],
			[2, 2, 2],
			[3, 3]
		]);
		expect(rows(1)).toEqual([[6]]);
		expect(rows(2)).toEqual([[3, 3]]);
		expect(toolSpans(0)).toEqual([]);
	});

	it('puts two cards to a row in a narrower grid', () => {
		expect(rows(6, 2)).toEqual([
			[3, 3],
			[3, 3],
			[3, 3]
		]);
		expect(rows(7, 2)).toEqual([[3, 3], [3, 3], [3, 3], [6]]);
		expect(rows(1, 2)).toEqual([[6]]);
		expect(toolSpans(0, 2)).toEqual([]);
	});
});
