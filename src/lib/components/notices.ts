// How a notification comes and goes (section 8), for the notifications and the update
// notice above them: it grows into view where it belongs as it fades in, and shrinks away
// there as it fades out. Its place in their column opens ahead of it and closes after it,
// so the others make way for it and close up after it without a jump.

import { cubicOut } from 'svelte/easing';

import { motionMs } from './panes.ts';

/** Coming and going (section 8: animations under 150 ms). */
export const NOTICE_MOTION_MS = 140;

/** How large a notification is as it starts to show, and as it is last seen. */
export const NOTICE_SCALE = 0.9;

/**
 * The style of a notification's place `t` of the way in (0 gone, 1 in place), for a place
 * `height` tall with `gap` above it. The place opens from nothing, quickly at first, so
 * that there is room before the notification shows much; the notification stays at the
 * place's lower edge (`.notice-slot`), which does not move, and grows from `NOTICE_SCALE`
 * around its own middle.
 */
export function noticeStyle(t: number, height: number, gap: number): string {
	const room = cubicOut(t);
	const scale = Math.round((NOTICE_SCALE + (1 - NOTICE_SCALE) * room) * 1000) / 1000;
	return [
		`height: ${room * height}px`,
		`margin-top: ${room * gap}px`,
		`opacity: ${t}`,
		`scale: ${scale}`,
		// (The place is shorter than the notification on the way: its middle is not the
		// notification's.)
		`transform-origin: 50% calc(100% - ${height / 2}px)`
	].join('; ');
}

/** A Svelte transition for a notification's place (`transition:noticeMotion`). At once when
 * the system asks for reduced motion. */
export function noticeMotion(node: HTMLElement) {
	const height = node.offsetHeight;
	const gap = parseFloat(getComputedStyle(node).marginTop) || 0;
	return {
		duration: motionMs(NOTICE_MOTION_MS),
		css: (t: number) => noticeStyle(t, height, gap)
	};
}
