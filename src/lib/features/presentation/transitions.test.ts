import { describe, expect, it } from 'vitest';

import type { Transition, TransitionStyle } from '#lib/ipc/index.ts';

import { directionVector, transitionFrame } from './transitions';

const make = (style: TransitionStyle, more: Partial<Transition> = {}): Transition => ({
	style,
	seconds: 1,
	horizontal: true,
	inward: true,
	direction: 0,
	scale: 1,
	opaque: false,
	...more
});

const at = (t: Transition, p: number) => transitionFrame(t, p, 1000, 500);

describe('directionVector', () => {
	it('follows the standard: 0 left to right, 90 bottom to top, 270 top to bottom', () => {
		expect(directionVector(0)).toEqual([1, 0]);
		expect(directionVector(90)).toEqual([0, -1]);
		expect(directionVector(180)).toEqual([-1, 0]);
		expect(directionVector(270)).toEqual([0, 1]);
		const [x, y] = directionVector(315);
		expect(x).toBeCloseTo(Math.SQRT1_2);
		expect(y).toBeCloseTo(Math.SQRT1_2);
	});
});

describe('transitionFrame', () => {
	it('fades and dissolves the page coming in', () => {
		for (const style of ['fade', 'dissolve'] as const) {
			expect(at(make(style), 0).incoming.opacity).toBe(0);
			expect(at(make(style), 0.5).incoming.opacity).toBe(0.5);
			expect(at(make(style), 1).incoming.opacity).toBe(1);
		}
	});

	it('wipes in the direction given', () => {
		expect(at(make('wipe'), 0.25).incoming.mask).toBe('linear-gradient(90deg, #000 25%, transparent 25%)');
		expect(at(make('wipe', { direction: 270 }), 0.5).incoming.mask).toBe('linear-gradient(180deg, #000 50%, transparent 50%)');
		expect(at(make('wipe', { direction: 90 }), 1).incoming.mask).toBe('linear-gradient(0deg, #000 100%, transparent 100%)');
	});

	it('splits outward from the middle, or shrinks the page going inward', () => {
		expect(at(make('split', { inward: false }), 0).incoming.clipPath).toBe('inset(50% 0 50% 0)');
		expect(at(make('split', { inward: false }), 1).incoming.clipPath).toBe('inset(0% 0 0% 0)');
		expect(at(make('split', { inward: false, horizontal: false }), 0.5).incoming.clipPath).toBe('inset(0 25% 0 25%)');
		const inward = at(make('split'), 0.5);
		expect(inward.outgoingOnTop).toBe(true);
		expect(inward.outgoing.clipPath).toBe('inset(25% 0 25% 0)');
	});

	it('opens a box from the middle, or closes the old page into it', () => {
		expect(at(make('box', { inward: false }), 0.5).incoming.clipPath).toBe('inset(25%)');
		const inward = at(make('box'), 1);
		expect(inward.outgoingOnTop).toBe(true);
		expect(inward.outgoing.clipPath).toBe('inset(50%)');
	});

	it('opens blinds across the page', () => {
		expect(at(make('blinds'), 0.5).incoming.mask).toBe(
			'repeating-linear-gradient(to bottom, #000 0, #000 6.25%, transparent 6.25%, transparent 12.5%)'
		);
		expect(at(make('blinds', { horizontal: false }), 1).incoming.mask).toContain('to right');
	});

	it('pushes, covers and uncovers across the screen', () => {
		const push = at(make('push'), 0.5);
		expect(push.incoming.transform).toBe('translate(-500px, 0px)');
		expect(push.outgoing.transform).toBe('translate(500px, 0px)');
		const down = at(make('cover', { direction: 270 }), 0);
		expect(down.incoming.transform).toBe('translate(0px, -500px)');
		expect(down.outgoing).toEqual({});
		const uncover = at(make('uncover', { direction: 180 }), 1);
		expect(uncover.outgoingOnTop).toBe(true);
		expect(uncover.outgoing.transform).toBe('translate(-1000px, 0px)');
	});

	it('flies in from its scale, or out to it, and only scales without a direction', () => {
		const flyIn = at(make('fly', { direction: null, scale: 0.5, opaque: true }), 0.5);
		expect(flyIn.incoming).toEqual({ transform: 'scale(0.75)', opacity: undefined });
		const flyOut = at(make('fly', { inward: false, scale: 0 }), 1);
		expect(flyOut.outgoingOnTop).toBe(true);
		// A scale of 0 is no scale given: 1.
		expect(flyOut.outgoing.transform).toBe('translate(1000px, 0px) scale(1)');
		expect(flyOut.outgoing.opacity).toBe(0);
	});

	it('keeps to the transition before its start and past its end', () => {
		expect(at(make('fade'), -1).incoming.opacity).toBe(0);
		expect(at(make('fade'), 2).incoming.opacity).toBe(1);
	});
});
