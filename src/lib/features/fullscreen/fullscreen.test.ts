import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { EDGE_DWELL_MS, EDGE_ZONE, ENTER_FLASH_MS, PANE_LINGER_MS, PEEK_AFTER_MS, atPane } from './reveal.ts';

const setFullscreen = vi.hoisted(() => vi.fn<(on: boolean) => Promise<void>>(async () => {}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ setFullscreen }) }));

const { fullscreen } = await import('./fullscreen.svelte.ts');

/** A 1920 px wide screen; the left pane shows 240 px wide, the right one 300. */
const WIDTH = 1920;
const key = (k: string, mods: { ctrl?: boolean; shift?: boolean; repeat?: boolean } = {}) => ({
	key: k,
	ctrlKey: !!mods.ctrl,
	shiftKey: !!mods.shift,
	metaKey: false,
	repeat: !!mods.repeat
});
const move = (x: number, buttons = 0) => fullscreen.pointerMoved(x, buttons, WIDTH);
const shown = () => ({ left: fullscreen.shows('left'), right: fullscreen.shows('right') });

beforeEach(async () => {
	vi.useFakeTimers();
	setFullscreen.mockClear();
	fullscreen.widthOf = (side) => (side === 'left' ? 240 : 300);
	await fullscreen.enter();
});

afterEach(async () => {
	await fullscreen.leave();
	vi.useRealTimers();
});

describe('the pointer at a pane', () => {
	it('is at a hidden pane only at its side of the screen', () => {
		expect(atPane('left', 0, WIDTH, null)).toBe(true);
		expect(atPane('left', EDGE_ZONE - 1, WIDTH, null)).toBe(true);
		expect(atPane('left', EDGE_ZONE, WIDTH, null)).toBe(false);
		expect(atPane('right', WIDTH - 1, WIDTH, null)).toBe(true);
		expect(atPane('right', WIDTH - EDGE_ZONE, WIDTH, null)).toBe(true);
		expect(atPane('right', WIDTH - EDGE_ZONE - 1, WIDTH, null)).toBe(false);
	});

	it('is at a pane that shows anywhere over it and its resize edge', () => {
		expect(atPane('left', 200, WIDTH, 240)).toBe(true);
		expect(atPane('left', 243, WIDTH, 240)).toBe(true);
		expect(atPane('left', 244, WIDTH, 240)).toBe(false);
		expect(atPane('right', WIDTH - 304, WIDTH, 300)).toBe(true);
		expect(atPane('right', WIDTH - 305, WIDTH, 300)).toBe(false);
	});
});

describe('full screen', () => {
	it('switches the window, and back', async () => {
		expect(fullscreen.on).toBe(true);
		expect(setFullscreen).toHaveBeenLastCalledWith(true);
		await fullscreen.toggle();
		expect(fullscreen.on).toBe(false);
		expect(setFullscreen).toHaveBeenLastCalledWith(false);
		await fullscreen.toggle();
		expect(fullscreen.on).toBe(true);
	});

	it('stays off when the window refuses', async () => {
		await fullscreen.leave();
		setFullscreen.mockRejectedValueOnce(new Error('not allowed'));
		await expect(fullscreen.enter()).rejects.toThrow('not allowed');
		expect(fullscreen.on).toBe(false);
		expect(fullscreen.justEntered).toBe(false);
	});

	it('shows a toolbar that hides for a moment as it starts', () => {
		expect(fullscreen.justEntered).toBe(true);
		vi.advanceTimersByTime(ENTER_FLASH_MS);
		expect(fullscreen.justEntered).toBe(false);
	});

	it('shows no pane until something brings one', () => {
		expect(shown()).toEqual({ left: false, right: false });
		move(900);
		vi.advanceTimersByTime(1000);
		expect(shown()).toEqual({ left: false, right: false });
	});

	it('brings a pane once the pointer has rested at its side of the screen', () => {
		move(0);
		expect(fullscreen.shows('left')).toBe(false);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		expect(shown()).toEqual({ left: true, right: false });
		move(WIDTH - 1);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		expect(fullscreen.shows('right')).toBe(true);
	});

	it('brings nothing for a pointer on its way past the side', () => {
		move(0);
		vi.advanceTimersByTime(EDGE_DWELL_MS - 50);
		move(40);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.shows('left')).toBe(false);
	});

	it('keeps a pane while the pointer is over it, then lets it go after a moment', () => {
		move(0);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		move(200);
		vi.advanceTimersByTime(5000);
		expect(fullscreen.shows('left')).toBe(true);
		move(400);
		vi.advanceTimersByTime(PANE_LINGER_MS - 1);
		expect(fullscreen.shows('left')).toBe(true);
		// Back over it in time: it stays.
		move(100);
		vi.advanceTimersByTime(5000);
		expect(fullscreen.shows('left')).toBe(true);
		move(400);
		vi.advanceTimersByTime(PANE_LINGER_MS);
		expect(fullscreen.shows('left')).toBe(false);
	});

	it('lets a pane go when the pointer leaves the window', () => {
		move(WIDTH - 1);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		expect(fullscreen.shows('right')).toBe(true);
		fullscreen.pointerLeft();
		vi.advanceTimersByTime(PANE_LINGER_MS);
		expect(fullscreen.shows('right')).toBe(false);
	});

	it('neither brings a pane nor lets one go during a drag', () => {
		// The scrollbar's thumb dragged at the right side of the screen.
		move(WIDTH - 1, 1);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.shows('right')).toBe(false);
		// A pressed button ends a rest that had started.
		move(0);
		move(0, 1);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.shows('left')).toBe(false);
		// The pane's edge dragged wider than the pane was.
		move(0);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		move(500, 1);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.shows('left')).toBe(true);
	});

	it('shows a pane a command asked for until it is hidden or the page is pressed', () => {
		fullscreen.pin('left', true);
		move(900);
		vi.advanceTimersByTime(5000);
		expect(shown()).toEqual({ left: true, right: false });
		fullscreen.pagePressed();
		expect(fullscreen.shows('left')).toBe(false);

		fullscreen.pin('right', true);
		fullscreen.pin('right', false);
		expect(fullscreen.shows('right')).toBe(false);
	});

	it('keeps a pane hidden under the pointer away until the pointer has left its side', () => {
		move(0);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		// Its own button, at the screen's edge, hides it.
		fullscreen.pin('left', false);
		expect(fullscreen.shows('left')).toBe(false);
		move(2);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.shows('left')).toBe(false);
		move(300);
		move(0);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		expect(fullscreen.shows('left')).toBe(true);
	});

	it('shows both panes and the toolbar while Alt is held', () => {
		expect(fullscreen.keyDown(key('Alt'))).toBe(true);
		expect(fullscreen.peek).toBe(false);
		vi.advanceTimersByTime(PEEK_AFTER_MS);
		expect(fullscreen.peek).toBe(true);
		expect(shown()).toEqual({ left: true, right: true });
		// The key repeats while it is held.
		expect(fullscreen.keyDown(key('Alt', { repeat: true }))).toBe(true);
		expect(fullscreen.peek).toBe(true);
		expect(fullscreen.keyUp(key('Alt'))).toBe(true);
		expect(fullscreen.peek).toBe(false);
		expect(shown()).toEqual({ left: false, right: false });
	});

	it('takes Alt with another key for a shortcut, not a hold', () => {
		fullscreen.keyDown(key('Alt'));
		expect(fullscreen.keyDown(key('ArrowLeft'))).toBe(false);
		vi.advanceTimersByTime(1000);
		expect(fullscreen.peek).toBe(false);
		fullscreen.keyUp(key('Alt'));
		// Shift first, then Alt (the keyboard layout's switch), and AltGr (Ctrl with it).
		fullscreen.keyDown(key('Alt', { shift: true }));
		fullscreen.keyDown(key('Alt', { ctrl: true }));
		vi.advanceTimersByTime(1000);
		expect(fullscreen.peek).toBe(false);
		// Another key while both panes show ends that; holding on brings nothing back.
		fullscreen.keyUp(key('Alt'));
		fullscreen.keyDown(key('Alt'));
		vi.advanceTimersByTime(PEEK_AFTER_MS);
		expect(fullscreen.peek).toBe(true);
		fullscreen.keyDown(key('ArrowDown'));
		expect(fullscreen.peek).toBe(false);
		fullscreen.keyDown(key('Alt', { repeat: true }));
		vi.advanceTimersByTime(1000);
		expect(fullscreen.peek).toBe(false);
	});

	it('ends what Alt shows when the page is pressed or the window loses the focus', () => {
		for (const end of [() => fullscreen.pagePressed(), () => fullscreen.blurred()]) {
			fullscreen.keyDown(key('Alt'));
			vi.advanceTimersByTime(PEEK_AFTER_MS);
			expect(fullscreen.peek).toBe(true);
			end();
			expect(fullscreen.peek).toBe(false);
			fullscreen.keyUp(key('Alt'));
		}
	});

	it('keeps the pane under the pointer when Alt is let go', () => {
		move(100);
		fullscreen.keyDown(key('Alt'));
		vi.advanceTimersByTime(PEEK_AFTER_MS);
		// The pointer has not moved since: it is over the left pane, which came under it.
		fullscreen.keyUp(key('Alt'));
		expect(shown()).toEqual({ left: true, right: false });
		move(600);
		vi.advanceTimersByTime(PANE_LINGER_MS);
		expect(fullscreen.shows('left')).toBe(false);
	});

	it('leaves Alt alone, and shows nothing, when it is off', async () => {
		await fullscreen.leave();
		expect(fullscreen.keyDown(key('Alt'))).toBe(false);
		expect(fullscreen.keyUp(key('Alt'))).toBe(false);
		move(0);
		fullscreen.pin('left', true);
		vi.advanceTimersByTime(1000);
		expect(shown()).toEqual({ left: false, right: false });
		expect(fullscreen.peek).toBe(false);
	});

	it('starts again with nothing shown', async () => {
		fullscreen.pin('left', true);
		move(WIDTH - 1);
		vi.advanceTimersByTime(EDGE_DWELL_MS);
		expect(shown()).toEqual({ left: true, right: true });
		await fullscreen.leave();
		await fullscreen.enter();
		expect(shown()).toEqual({ left: false, right: false });
	});
});
