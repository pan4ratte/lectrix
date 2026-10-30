import { describe, expect, it } from 'vitest';

import { GESTURE_GAP_MS, WheelSteps, actionFor, placePage, swipeStep } from './input';

const key = (k: string, mods: Partial<Record<'ctrlKey' | 'altKey' | 'metaKey' | 'shiftKey', boolean>> = {}) =>
	actionFor({ key: k, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...mods });

describe('actionFor', () => {
	it('turns pages with the arrows, Page Up and Down, Space, N and P', () => {
		for (const k of ['ArrowRight', 'ArrowDown', 'PageDown', ' ', 'n', 'N']) expect(key(k)).toEqual({ kind: 'next' });
		for (const k of ['ArrowLeft', 'ArrowUp', 'PageUp', 'p', 'P']) expect(key(k)).toEqual({ kind: 'previous' });
		expect(key(' ', { shiftKey: true })).toEqual({ kind: 'previous' });
		expect(key('Home')).toEqual({ kind: 'first' });
		expect(key('End')).toEqual({ kind: 'last' });
	});

	it('takes digits, Enter and Backspace for a page number, B and W for blank screens, and Esc', () => {
		expect(key('7')).toEqual({ kind: 'digit', digit: '7' });
		expect(key('Enter')).toEqual({ kind: 'enter' });
		expect(key('Backspace')).toEqual({ kind: 'erase' });
		for (const k of ['b', 'B', '.']) expect(key(k)).toEqual({ kind: 'blank', blank: 'black' });
		for (const k of ['w', 'W', ',']) expect(key(k)).toEqual({ kind: 'blank', blank: 'white' });
		expect(key('Escape')).toEqual({ kind: 'escape' });
	});

	it('leaves shortcuts with Ctrl, Alt or Command, and other keys, alone', () => {
		expect(key('ArrowRight', { ctrlKey: true })).toBeNull();
		expect(key('ArrowLeft', { altKey: true })).toBeNull();
		expect(key('n', { metaKey: true })).toBeNull();
		expect(key('F3')).toBeNull();
		expect(key('x')).toBeNull();
	});
});

describe('WheelSteps', () => {
	it('turns a page for each notch of a mouse wheel', () => {
		const wheel = new WheelSteps();
		expect(wheel.push(100, 0, 0)).toBe(1);
		expect(wheel.push(100, 0, 40)).toBe(1);
		expect(wheel.push(-100, 0, 80)).toBe(-1);
		// Lines (Firefox, some mice): three lines are a notch.
		expect(wheel.push(3, 1, 1000)).toBe(1);
	});

	it('turns one page for a whole touchpad swipe, with the scrolling that carries on after it', () => {
		const wheel = new WheelSteps();
		const steps: number[] = [];
		let t = 0;
		for (let i = 0; i < 60; i++) steps.push(wheel.push(8, 0, (t += 16)));
		expect(steps.filter((s) => s !== 0)).toEqual([1]);
		// A pause, then another swipe, back.
		t += GESTURE_GAP_MS + 1;
		const back: number[] = [];
		for (let i = 0; i < 20; i++) back.push(wheel.push(-10, 0, (t += 16)));
		expect(back.filter((s) => s !== 0)).toEqual([-1]);
	});

	it('turns nothing for a nudge', () => {
		const wheel = new WheelSteps();
		expect(wheel.push(10, 0, 0)).toBe(0);
		expect(wheel.push(10, 0, 20)).toBe(0);
		expect(wheel.push(10, 0, 20 + GESTURE_GAP_MS + 1)).toBe(0);
	});
});

describe('swipeStep', () => {
	it('turns on for a swipe to the left and back for one to the right', () => {
		expect(swipeStep(-120, 10)).toBe(1);
		expect(swipeStep(120, -10)).toBe(-1);
	});

	it('turns nothing for a tap, a short move, or one mostly up or down', () => {
		expect(swipeStep(0, 0)).toBe(0);
		expect(swipeStep(-30, 0)).toBe(0);
		expect(swipeStep(-80, 200)).toBe(0);
	});
});

describe('placePage', () => {
	it('fits a page to the screen, in its middle', () => {
		// A4 portrait on a 1920 x 1080 screen: as tall as the screen.
		const a4 = placePage({ width: 595, height: 842 }, 0, 1920, 1080);
		expect(a4.height).toBeCloseTo(1080);
		expect(a4.width).toBeCloseTo((1080 * 595) / 842);
		expect(a4.left).toBeCloseTo((1920 - a4.width) / 2);
		expect(a4.top).toBeCloseTo(0);
		// A 16:9 slide fills it.
		const slide = placePage({ width: 960, height: 540 }, 0, 1920, 1080);
		expect([slide.left, slide.top, slide.width, slide.height].map((v) => Math.round(v))).toEqual([0, 0, 1920, 1080]);
	});

	it('fits a page turned on screen as it is turned', () => {
		// On its side, A4 is landscape, and still narrower than the screen.
		const turned = placePage({ width: 595, height: 842 }, 90, 1920, 1080);
		expect(turned.height).toBeCloseTo(1080);
		expect(turned.width).toBeCloseTo((1080 * 842) / 595);
		// On a portrait screen, as wide as it.
		const portrait = placePage({ width: 595, height: 842 }, 90, 1080, 1920);
		expect(portrait.width).toBeCloseTo(1080);
		expect(portrait.top).toBeCloseTo((1920 - portrait.height) / 2);
	});
});
