// How a document tab comes and goes (section 8): it grows from nothing to its width as it
// fades in, and shrinks away as it fades out, so the tabs beside it slide over instead of
// jumping, as Chrome's and Edge's tabs do.

import { cubicOut } from 'svelte/easing';

import { motionMs } from './panes.ts';

/** Coming and going (section 8: animations under 150 ms). */
export const TAB_MOTION_MS = 140;

/** A tab's measures at full size, CSS pixels. */
export interface TabBox {
	width: number;
	paddingLeft: number;
	paddingRight: number;
	/** The row's gap after the tab, which closes with it. */
	gap: number;
}

/** The style of a tab `t` of the way in (0 gone, 1 in place). */
export function tabStyle(t: number, box: TabBox): string {
	const room = cubicOut(t);
	const px = (v: number) => `${Math.round(v * 100) / 100}px`;
	const width = px(room * box.width);
	return [
		`width: ${width}`,
		`min-width: ${width}`,
		`max-width: ${width}`,
		`padding-left: ${px(room * box.paddingLeft)}`,
		`padding-right: ${px(room * box.paddingRight)}`,
		`margin-right: ${px(-(1 - room) * box.gap)}`,
		`opacity: ${t}`,
		'overflow: hidden'
	].join('; ');
}

function measure(node: HTMLElement): TabBox {
	const style = getComputedStyle(node);
	const row = node.parentElement ? getComputedStyle(node.parentElement) : null;
	return {
		width: node.getBoundingClientRect().width,
		paddingLeft: parseFloat(style.paddingLeft) || 0,
		paddingRight: parseFloat(style.paddingRight) || 0,
		gap: parseFloat(row?.columnGap ?? '') || 0
	};
}

/** A tab growing into its place (`in:tabIn`). At once when the system asks for reduced
 * motion. */
export function tabIn(node: HTMLElement) {
	const box = measure(node);
	return { duration: motionMs(TAB_MOTION_MS), css: (t: number) => tabStyle(t, box) };
}

/** A closed tab shrinking away (`out:tabOut`). It is no tab any more while it goes: not in
 * the tab list for screen readers, the keyboard or the code that finds tabs by `data-tab`. */
export function tabOut(node: HTMLElement) {
	const box = measure(node);
	node.removeAttribute('role');
	node.removeAttribute('data-tab');
	node.removeAttribute('data-combine-tab');
	node.setAttribute('aria-hidden', 'true');
	node.inert = true;
	return { duration: motionMs(TAB_MOTION_MS), css: (t: number) => tabStyle(t, box) };
}
