// How what floats over the page comes and goes (section 8.3): the comment panel and the
// bars beside annotations and selected text. It fades in while growing slightly, from the
// point it belongs to (the element's `transform-origin`), and goes back the same way,
// faster. While going it takes no clicks and no focus.

import { cubicOut } from 'svelte/easing';

import { motionMs } from './panes.ts';

/** Coming in and going (section 8: animations under 150 ms, as dialogs). */
export const POP_IN_MS = 140;
export const POP_OUT_MS = 100;
/** How large it is as it starts to show, and as it is last seen. */
export const POP_SCALE = 0.96;

/** The style `t` of the way in (0 gone, 1 in place). */
export function popStyle(t: number): string {
	const scale = Math.round((POP_SCALE + (1 - POP_SCALE) * t) * 1000) / 1000;
	return `opacity: ${t}; scale: ${scale}`;
}

/** A Svelte transition for coming in (`in:popIn`). At once with reduced motion. */
export function popIn(_node: Element) {
	return { duration: motionMs(POP_IN_MS), easing: cubicOut, css: popStyle };
}

/** A Svelte transition for going (`out:popOut`): inert meanwhile. At once with reduced
 * motion. */
export function popOut(node: HTMLElement) {
	node.inert = true;
	return { duration: motionMs(POP_OUT_MS), easing: cubicOut, css: popStyle };
}
