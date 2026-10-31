import { describe, expect, it } from 'vitest';

import { Stabilizer, arrowHeadLength, constrainEnd, shapeStrokes } from './pen.ts';

const PAGE = { width: 600, height: 800 };

/** The points of a flat stroke as pairs. */
const pairs = (stroke: number[]) => stroke.flatMap((v, i) => (i % 2 ? [] : [[v, stroke[i + 1]!] as const]));

describe('constrainEnd', () => {
	it('leaves the end alone without Shift', () => {
		expect(constrainEnd('line', [10, 10], [53, 21], false, PAGE)).toEqual([53, 21]);
		expect(constrainEnd('ellipse', [10, 10], [53, 21], false, PAGE)).toEqual([53, 21]);
	});

	it('snaps lines and arrows to 45° steps', () => {
		const [x, y] = constrainEnd('line', [100, 100], [200, 110], true, PAGE);
		expect(y).toBeCloseTo(100);
		expect(x).toBeCloseTo(200);
		const [dx, dy] = constrainEnd('arrow', [100, 100], [190, 210], true, PAGE).map((v, i) => v - 100);
		expect(dx).toBeCloseTo(dy!);
		const [ux, uy] = constrainEnd('line', [100, 100], [95, 30], true, PAGE);
		expect(ux).toBeCloseTo(100);
		expect(uy).toBeLessThan(100);
	});

	it('makes rectangles squares and ellipses circles, in the direction dragged', () => {
		expect(constrainEnd('rectangle', [100, 100], [160, 130], true, PAGE)).toEqual([160, 160]);
		expect(constrainEnd('ellipse', [100, 100], [70, 180], true, PAGE)).toEqual([20, 180]);
	});

	it('keeps a constrained shape on the page', () => {
		// 80 pt of room to the right edge: the square is no wider.
		expect(constrainEnd('rectangle', [520, 100], [560, 300], true, PAGE)).toEqual([600, 180]);
		const [x, y] = constrainEnd('line', [580, 100], [700, 220], true, PAGE);
		expect(x).toBeCloseTo(600);
		expect(y).toBeCloseTo(120);
	});
});

describe('shapeStrokes', () => {
	it('draws a line as one stroke and a rectangle as one closed stroke', () => {
		expect(shapeStrokes('line', [1, 2], [3, 4], 2)).toEqual([[1, 2, 3, 4]]);
		expect(shapeStrokes('rectangle', [10, 20], [30, 40], 2)).toEqual([[10, 20, 30, 20, 30, 40, 10, 40, 10, 20]]);
	});

	it('draws an arrow as its shaft and an open head at its end', () => {
		const [shaft, head] = shapeStrokes('arrow', [0, 0], [100, 0], 2);
		expect(shaft).toEqual([0, 0, 100, 0]);
		const [left, tip, right] = pairs(head!);
		expect(tip).toEqual([100, 0]);
		const length = arrowHeadLength(2, 100);
		for (const side of [left!, right!]) {
			expect(Math.hypot(side[0] - 100, side[1])).toBeCloseTo(length);
			// 30° off the shaft, behind the tip.
			expect(side[0]).toBeCloseTo(100 - length * Math.cos(Math.PI / 6));
			expect(Math.abs(side[1])).toBeCloseTo(length / 2);
		}
		expect(left![1]).toBeCloseTo(-right![1]);
	});

	it('gives thicker arrows bigger heads, never more than half the shaft', () => {
		expect(arrowHeadLength(4, 200)).toBeGreaterThan(arrowHeadLength(1, 200));
		expect(arrowHeadLength(12, 20)).toBe(10);
	});

	it('draws an ellipse as a closed stroke within a tenth of a point of the curve', () => {
		const [stroke] = shapeStrokes('ellipse', [100, 200], [300, 300], 2);
		const points = pairs(stroke!);
		expect(points[0]).toEqual(points[points.length - 1]);
		for (const [x, y] of points) {
			expect(((x - 200) / 100) ** 2 + ((y - 250) / 50) ** 2).toBeCloseTo(1);
		}
		// The middle of each chord is no further than 0.1 pt inside the circle of radius 100.
		const [a, b] = [points[0]!, points[1]!];
		const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
		const sagitta = 100 - Math.hypot((mid[0]! - 200) / 1, ((mid[1]! - 250) * 100) / 50);
		expect(sagitta).toBeLessThanOrEqual(0.1);
	});
});

describe('Stabilizer', () => {
	it('draws where the pointer is without a string', () => {
		const s = new Stabilizer([0, 0], 0);
		expect(s.follow([5, 7])).toEqual([5, 7]);
	});

	it('ignores movement within the string, and trails the pointer by it beyond', () => {
		const s = new Stabilizer([0, 0], 10);
		expect(s.follow([6, 0])).toEqual([0, 0]);
		expect(s.follow([0, -8])).toEqual([0, 0]);
		const [x, y] = s.follow([30, 0]);
		expect(x).toBeCloseTo(20);
		expect(y).toBeCloseTo(0);
	});

	it('smooths a wobbly line', () => {
		const s = new Stabilizer([0, 0], 8);
		let worst = 0;
		for (let i = 1; i <= 200; i++) {
			const [, y] = s.follow([i, i % 2 ? 3 : -3]);
			worst = Math.max(worst, Math.abs(y));
		}
		expect(worst).toBeLessThan(1);
	});
});
