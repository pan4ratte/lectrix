// Visual adjustments (section 6.1): brightness, contrast, sepia, inverted colours and
// greyscale for a document's pages, as a CSS filter on the page images and, for sepia, a
// paper tone laid over them. Greyscale and inverted colours leave the annotations out: a
// page with annotations is then its content, filtered, with its annotations on a layer of
// their own over it, both rendered by Rust. Only on screen: nothing here reaches the file.
// Settings give every document its adjustments; one adjusted by itself has its own,
// remembered with its view.

import type { PageAdjustments } from '#lib/ipc/index.ts';

/** The least and the most brightness and contrast, percent (mirrors `PageAdjustments::MIN`
 * and `MAX` in src-tauri/src/ipc.rs), and the sliders' step. */
export const ADJUST_MIN = 50;
export const ADJUST_MAX = 150;
export const ADJUST_STEP = 5;
/** The most sepia, percent (`PageAdjustments::SEPIA_MAX`): from 0, the pages as they are. */
export const SEPIA_MAX = 100;

/** The pages as they are. */
export const NO_ADJUSTMENTS: Readonly<PageAdjustments> = { brightness: 100, contrast: 100, invert: false, grayscale: false, sepia: 0 };

function percent(value: unknown): number {
	if (typeof value !== 'number' || !Number.isFinite(value)) return 100;
	return Math.round(Math.min(ADJUST_MAX, Math.max(ADJUST_MIN, value)));
}

function sepia(value: unknown): number {
	if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
	return Math.round(Math.min(SEPIA_MAX, Math.max(0, value)));
}

/** Remembered adjustments made whole: values within their range, and none where a view
 * stored before there were adjustments has none. */
export function normalizeAdjustments(stored: Partial<PageAdjustments> | null | undefined): PageAdjustments {
	return {
		brightness: percent(stored?.brightness),
		contrast: percent(stored?.contrast),
		invert: stored?.invert === true,
		grayscale: stored?.grayscale === true,
		sepia: sepia(stored?.sepia)
	};
}

/** Whether two sets of adjustments show the pages the same. */
export function sameAdjustments(a: PageAdjustments, b: PageAdjustments): boolean {
	return (
		a.brightness === b.brightness &&
		a.contrast === b.contrast &&
		a.invert === b.invert &&
		a.grayscale === b.grayscale &&
		a.sepia === b.sepia
	);
}

/** Whether anything is adjusted. */
export function isAdjusted(a: PageAdjustments): boolean {
	return a.brightness !== 100 || a.contrast !== 100 || a.invert || a.grayscale || a.sepia !== 0;
}

/**
 * The CSS filter that shows a page with `a`, or null for the page as it is. Greyscale first,
 * then the inversion, then brightness and contrast, which so act on what is seen: less
 * brightness dims an inverted page's light text. Inverted pages get their hues turned back
 * (the half turn that inverting gave them), so a yellow highlight stays yellow and a blue
 * link blue: the lightness is what is inverted. Sepia is not a filter (`pageTone`).
 */
export function pageFilter(a: PageAdjustments): string | null {
	const parts = [colourFilter(a), lightFilter(a)].filter((f) => f !== null);
	return parts.length > 0 ? parts.join(' ') : null;
}

/**
 * The part of `pageFilter` that changes the colours, greyscale and the inversion, or null
 * for none. It is applied to the page without its annotations, which are laid over it as
 * they are (`annotationLayer`).
 */
export function colourFilter(a: PageAdjustments): string | null {
	const parts: string[] = [];
	if (a.grayscale) parts.push('grayscale(1)');
	if (a.invert) parts.push('invert(1)', 'hue-rotate(180deg)');
	return parts.length > 0 ? parts.join(' ') : null;
}

/**
 * The part of `colourFilter` for what is drawn over the page (the selection, search hits,
 * what is being drawn, a text box being typed), or null for none: only the inversion, so
 * they stay as visible on a dark page as on a light one. Greyscale leaves them their
 * colours, as it leaves the annotations theirs.
 */
export function overlayFilter(a: PageAdjustments): string | null {
	return a.invert ? 'invert(1) hue-rotate(180deg)' : null;
}

/** The rest of `pageFilter`, brightness and contrast, or null for none: applied to the
 * page as it is seen, annotations included. */
export function lightFilter(a: PageAdjustments): string | null {
	const parts: string[] = [];
	if (a.brightness !== 100) parts.push(`brightness(${a.brightness / 100})`);
	if (a.contrast !== 100) parts.push(`contrast(${a.contrast / 100})`);
	return parts.length > 0 ? parts.join(' ') : null;
}

/**
 * The layer a page's annotations are drawn on, apart from the page, while its colours are
 * adjusted, or null while they are not. Greyscale leaves the annotations their colours
 * (`annots`: drawn as on the page as it is). An inverted page has them inverted in Rust
 * (`annots-dark`: lightness inverted with hue and saturation kept, a highlight dark enough
 * for the light text over it) and laid over it as light (`screen`), so that a yellow
 * highlight is yellow, not the muddy brown that inverting the page makes of it.
 */
export function annotationLayer(a: PageAdjustments): 'annots' | 'annots-dark' | null {
	if (a.invert) return 'annots-dark';
	return a.grayscale ? 'annots' : null;
}

/**
 * How much of the sepia paper tone lies over a page with `a`, from 0 to 1, or null for
 * none. An e-book's sepia: the tone of old paper (`--lectrix-sepia-paper`) multiplied
 * into the page (`.page-tone`), so white paper takes it, black text stays black and
 * pictures keep their colours, warmed. CSS's own `sepia()` cannot do this: it leaves white
 * nearly white and takes the colours away. It lies over the filtered page, last of all: it
 * tones what is seen, so an inverted page keeps its dark paper and has its light text
 * warmed, and more brightness does not wash the tone out.
 */
export function pageTone(a: PageAdjustments): number | null {
	return a.sepia > 0 ? Math.min(SEPIA_MAX, a.sepia) / SEPIA_MAX : null;
}
