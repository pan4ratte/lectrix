// Visual adjustments (section 6.1): brightness, contrast, inverted colours and greyscale for
// a document's pages, as a CSS filter on the page images. Only on screen: nothing here
// reaches Rust's renderer or the file, and each file's adjustments are remembered with its
// view.

import type { PageAdjustments } from '#lib/ipc/index.ts';

/** The least and the most brightness and contrast, percent (mirrors `PageAdjustments::MIN`
 * and `MAX` in src-tauri/src/ipc.rs), and the sliders' step. */
export const ADJUST_MIN = 50;
export const ADJUST_MAX = 150;
export const ADJUST_STEP = 5;

/** The pages as they are. */
export const NO_ADJUSTMENTS: Readonly<PageAdjustments> = { brightness: 100, contrast: 100, invert: false, grayscale: false };

function percent(value: unknown): number {
	if (typeof value !== 'number' || !Number.isFinite(value)) return 100;
	return Math.round(Math.min(ADJUST_MAX, Math.max(ADJUST_MIN, value)));
}

/** Remembered adjustments made whole: values within their range, and none where a view
 * stored before there were adjustments has none. */
export function normalizeAdjustments(stored: Partial<PageAdjustments> | null | undefined): PageAdjustments {
	return {
		brightness: percent(stored?.brightness),
		contrast: percent(stored?.contrast),
		invert: stored?.invert === true,
		grayscale: stored?.grayscale === true
	};
}

/** Whether anything is adjusted. */
export function isAdjusted(a: PageAdjustments): boolean {
	return a.brightness !== 100 || a.contrast !== 100 || a.invert || a.grayscale;
}

/**
 * The CSS filter that shows a page with `a`, or null for the page as it is. Greyscale first,
 * then the inversion, then brightness and contrast, which so act on what is seen: less
 * brightness dims an inverted page's light text. Inverted pages get their hues turned back
 * (the half turn that inverting gave them), so a yellow highlight stays yellow and a blue
 * link blue: the lightness is what is inverted.
 */
export function pageFilter(a: PageAdjustments): string | null {
	const parts: string[] = [];
	if (a.grayscale) parts.push('grayscale(1)');
	if (a.invert) parts.push('invert(1)', 'hue-rotate(180deg)');
	if (a.brightness !== 100) parts.push(`brightness(${a.brightness / 100})`);
	if (a.contrast !== 100) parts.push(`contrast(${a.contrast / 100})`);
	return parts.length > 0 ? parts.join(' ') : null;
}
