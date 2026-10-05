import { describe, expect, it } from 'vitest';

import {
	DEFAULT_ARRANGEMENT,
	LEFT_LIMITS,
	RESIZE_STEP,
	RIGHT_LIMITS,
	clampWidth,
	keyResize,
	movePanel,
	normalizeArrangement,
	paneOf,
	shownWidth,
	stepPanel
} from './panes.ts';

type PanelArrangementInput = Parameters<typeof stepPanel>[0];

describe('side panes', () => {
	it('keeps widths within the limits', () => {
		expect(clampWidth(100, LEFT_LIMITS)).toBe(LEFT_LIMITS.min);
		expect(clampWidth(9999, LEFT_LIMITS)).toBe(LEFT_LIMITS.max);
		expect(clampWidth(300.4, LEFT_LIMITS)).toBe(300);
		expect(clampWidth(Number.NaN, RIGHT_LIMITS)).toBe(RIGHT_LIMITS.initial);
	});

	it('leaves the page room in a narrow window, but never goes below the minimum', () => {
		expect(shownWidth(480, LEFT_LIMITS, 2000)).toBe(480);
		expect(shownWidth(480, LEFT_LIMITS, 1000)).toBe(400);
		expect(shownWidth(480, LEFT_LIMITS, 300)).toBe(LEFT_LIMITS.min);
		expect(shownWidth(300, RIGHT_LIMITS, 0)).toBe(300);
	});

	it('resizes from the keyboard, the outward arrow widening', () => {
		expect(keyResize(240, 'ArrowRight', 'left', LEFT_LIMITS)).toBe(240 + RESIZE_STEP);
		expect(keyResize(240, 'ArrowLeft', 'left', LEFT_LIMITS)).toBe(240 - RESIZE_STEP);
		expect(keyResize(300, 'ArrowLeft', 'right', RIGHT_LIMITS)).toBe(300 + RESIZE_STEP);
		expect(keyResize(300, 'ArrowRight', 'right', RIGHT_LIMITS)).toBe(300 - RESIZE_STEP);
		expect(keyResize(LEFT_LIMITS.max, 'ArrowRight', 'left', LEFT_LIMITS)).toBe(LEFT_LIMITS.max);
		expect(keyResize(300, 'Home', 'right', RIGHT_LIMITS)).toBe(RIGHT_LIMITS.min);
		expect(keyResize(300, 'End', 'right', RIGHT_LIMITS)).toBe(RIGHT_LIMITS.max);
		expect(keyResize(300, 'Enter', 'right', RIGHT_LIMITS)).toBeNull();
	});

	it('makes a remembered arrangement whole', () => {
		expect(normalizeArrangement([], [])).toEqual(DEFAULT_ARRANGEMENT);
		expect(normalizeArrangement(['annotations', 'pages', 'pages', 'outline'], ['labels'])).toEqual({
			left: ['annotations', 'pages', 'bookmarks'],
			right: ['labels']
		});
		// A panel in both panes stays in the first one met.
		expect(normalizeArrangement(['bookmarks'], ['bookmarks', 'pages', 'labels', 'annotations'])).toEqual({
			left: ['bookmarks'],
			right: ['pages', 'labels', 'annotations']
		});
	});

	it('moves panels along a row and between the panes', () => {
		const start = { left: ['pages', 'bookmarks', 'labels'], right: ['annotations'] } as const;
		const a = { left: [...start.left], right: [...start.right] };
		expect(movePanel(a, 'pages', 'left', 2)).toEqual({ left: ['bookmarks', 'labels', 'pages'], right: ['annotations'] });
		expect(movePanel(a, 'labels', 'right', 0)).toEqual({ left: ['pages', 'bookmarks'], right: ['labels', 'annotations'] });
		expect(movePanel(a, 'annotations', 'left', 99)).toEqual({ left: ['pages', 'bookmarks', 'labels', 'annotations'], right: [] });
		expect(a).toEqual(start);
		expect(paneOf(a, 'annotations')).toBe('right');
		expect(paneOf(a, 'labels')).toBe('left');
	});

	it('steps panels with Ctrl+Shift+arrows, across the panes at their inner ends', () => {
		const a = { left: ['pages', 'bookmarks'], right: ['labels', 'annotations'] } as PanelArrangementInput;
		expect(stepPanel(a, 'pages', 1)).toEqual({ side: 'left', index: 1 });
		expect(stepPanel(a, 'pages', -1)).toBeNull();
		expect(stepPanel(a, 'bookmarks', 1)).toEqual({ side: 'right', index: 0 });
		expect(stepPanel(a, 'labels', -1)).toEqual({ side: 'left', index: 2 });
		expect(stepPanel(a, 'annotations', 1)).toBeNull();
	});
});
