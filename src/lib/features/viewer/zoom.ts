// Zoom levels, fitting, and the render-scale ladder.

import { CSS_PX_PER_PT, PAGE_GAP, PAGE_MARGIN, type Rotation, type Size } from './layout.ts';

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 16;
export const ZOOM_PRESETS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 16] as const;

export type ZoomMode = 'fitWidth' | 'fitPage' | 'custom';

export function clampZoom(zoom: number): number {
	if (!Number.isFinite(zoom)) return 1;
	return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/** The next preset above (`direction` 1) or below (-1) `zoom`. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
	const eps = 1e-3;
	if (direction > 0) {
		return ZOOM_PRESETS.find((p) => p > zoom + eps) ?? MAX_ZOOM;
	}
	for (let i = ZOOM_PRESETS.length - 1; i >= 0; i--) {
		const p = ZOOM_PRESETS[i]!;
		if (p < zoom - eps) return p;
	}
	return MIN_ZOOM;
}

/** How long a zoom glide takes (section 8: animations under 150 ms). */
export const ZOOM_GLIDE_MS = 140;

/** Ease-out over `t` from 0 to 1 (clamped): fast at first, settling at the end. */
export function easeOut(t: number): number {
	const clamped = Math.min(1, Math.max(0, t));
	return 1 - (1 - clamped) ** 3;
}

/**
 * The zoom a fraction `t` (0 to 1) of the way through a glide from `from` to `to`: even in
 * ratio, so 100% to 200% passes 141% halfway, and easing out.
 */
export function zoomBetween(from: number, to: number, t: number): number {
	if (t >= 1) return to;
	return from * (to / from) ** easeOut(t);
}

/** Wheel deltas in pixels; below this, a Ctrl+wheel event is a touchpad pinch. */
export const NOTCH_PX = 50;

function wheelPixels(deltaY: number, deltaMode: number): number {
	return deltaMode === 1 ? deltaY * 33 : deltaY;
}

/**
 * Whether a Ctrl+wheel event is a mouse wheel notch (which glides) rather than part of a
 * touchpad pinch (which follows the fingers).
 */
export function isWheelNotch(deltaY: number, deltaMode: number): boolean {
	return Math.abs(wheelPixels(deltaY, deltaMode)) >= NOTCH_PX;
}

/**
 * Zoom factor for one Ctrl+wheel event. A touchpad pinch arrives as Ctrl+wheel with
 * `deltaY = -100 ln(scale)` (Chromium), so small deltas follow the fingers exactly; a mouse
 * wheel notch (100-150 px) zooms about 1.25x.
 */
export function wheelZoomFactor(deltaY: number, deltaMode: number): number {
	const delta = wheelPixels(deltaY, deltaMode);
	const perPixel = Math.abs(delta) < NOTCH_PX ? 0.01 : 0.0018;
	return Math.exp(-delta * perPixel);
}

/** Width and height of a page in points after view rotation. */
function rotatedSize(size: Size, rotation: Rotation) {
	return rotation === 90 || rotation === 270
		? { width: size.height, height: size.width }
		: { width: size.width, height: size.height };
}

/** Width left for pages in a viewport `viewportWidth` wide, with `columns` side by side. */
function availableWidth(viewportWidth: number, columns: 1 | 2): number {
	// One pixel of slack, so rounding never adds a horizontal scrollbar.
	return viewportWidth - 2 * PAGE_MARGIN - (columns - 1) * PAGE_GAP - 1;
}

/** Zoom at which `size` fills the viewport's width (minus margins and a scrollbar); with two
 * columns, two pages of that size side by side. */
export function fitWidthZoom(size: Size, viewportWidth: number, rotation: Rotation, columns: 1 | 2 = 1): number {
	const { width } = rotatedSize(size, rotation);
	return clampZoom(availableWidth(viewportWidth, columns) / (columns * width * CSS_PX_PER_PT));
}

/** Zoom at which the whole page (or two side by side) fits in the viewport. */
export function fitPageZoom(
	size: Size,
	viewportWidth: number,
	viewportHeight: number,
	rotation: Rotation,
	columns: 1 | 2 = 1
): number {
	const { width, height } = rotatedSize(size, rotation);
	const zw = availableWidth(viewportWidth, columns) / (columns * width * CSS_PX_PER_PT);
	const zh = (viewportHeight - 2 * PAGE_MARGIN - 1) / (height * CSS_PX_PER_PT);
	return clampZoom(Math.min(zw, zh));
}

/** Steps per doubling on the render-scale ladder (about 4.4% apart). */
const LADDER_STEPS = 16;

/**
 * Device-pixel scale (pixels per point) to render at for `zoom` on a display with
 * `devicePixelRatio`. Rounded *up* to a fixed ladder, so nearby zoom levels share
 * cached images and a page is never drawn blurrier than its on-screen size.
 */
export function renderScale(zoom: number, devicePixelRatio: number): number {
	const raw = zoom * CSS_PX_PER_PT * (devicePixelRatio || 1);
	const step = Math.ceil(Math.log2(raw) * LADDER_STEPS - 1e-9) / LADDER_STEPS;
	return Math.round(2 ** step * 1000) / 1000;
}

/** Pixel size of a page rendered at `scale`, rounded like MuPDF (`render::pixel_size`). */
export function pixelSize(size: Size, scale: number) {
	const round = (v: number) => Math.max(1, Math.ceil(v - 0.001));
	return { width: round(size.width * scale), height: round(size.height * scale) };
}

/** Edge of a render tile in device pixels (`render::TILE_SIZE`). */
export const TILE_SIZE = 512;

/** Above this many pixels, a page is drawn from tiles instead of one image. */
export const TILE_THRESHOLD_PIXELS = 6_000_000;

/** Pixel budget for the low-resolution whole-page image under the tiles. */
export const UNDERLAY_PIXELS = 1_500_000;

/** The scale at which a page fits in `pixels` (for thumbnails and tile underlays). */
export function scaleForPixels(size: Size, pixels: number): number {
	return Math.sqrt(pixels / (size.width * size.height));
}
