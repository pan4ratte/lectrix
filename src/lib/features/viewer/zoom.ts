// Zoom levels, fitting, and the render-scale ladder.

import type { PageSize } from '#lib/ipc/index.ts';

import { CSS_PX_PER_PT, PAGE_MARGIN, type Rotation } from './layout.ts';

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

/** How long a smooth zoom step takes (section 8: animations under 150 ms). */
export const ZOOM_STEP_MS = 140;

/**
 * The zoom a fraction `t` (0 to 1) of the way through a smooth step from `from` to `to`:
 * even in ratio, so 100% to 200% passes 141% halfway, and easing out.
 */
export function zoomBetween(from: number, to: number, t: number): number {
	const clamped = Math.min(1, Math.max(0, t));
	if (clamped === 1) return to;
	const eased = 1 - (1 - clamped) ** 3;
	return from * (to / from) ** eased;
}

/**
 * Zoom factor for one Ctrl+wheel event. A touchpad pinch arrives as Ctrl+wheel with
 * `deltaY = -100 ln(scale)` (Chromium), so small deltas follow the fingers exactly; a mouse
 * wheel notch (100-150 px) zooms about 1.25x.
 */
export function wheelZoomFactor(deltaY: number, deltaMode: number): number {
	const delta = deltaMode === 1 ? deltaY * 33 : deltaY;
	const perPixel = Math.abs(delta) < 50 ? 0.01 : 0.0018;
	return Math.exp(-delta * perPixel);
}

/** Width and height of a page in points after view rotation. */
function rotatedSize(size: PageSize, rotation: Rotation) {
	return rotation === 90 || rotation === 270
		? { width: size.height, height: size.width }
		: { width: size.width, height: size.height };
}

/** Zoom at which `size` fills the viewport's width (minus margins and a scrollbar). */
export function fitWidthZoom(size: PageSize, viewportWidth: number, rotation: Rotation): number {
	const { width } = rotatedSize(size, rotation);
	// One pixel of slack, so rounding never adds a horizontal scrollbar.
	const available = viewportWidth - 2 * PAGE_MARGIN - 1;
	return clampZoom(available / (width * CSS_PX_PER_PT));
}

/** Zoom at which the whole page fits in the viewport. */
export function fitPageZoom(
	size: PageSize,
	viewportWidth: number,
	viewportHeight: number,
	rotation: Rotation
): number {
	const { width, height } = rotatedSize(size, rotation);
	const zw = (viewportWidth - 2 * PAGE_MARGIN - 1) / (width * CSS_PX_PER_PT);
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
export function pixelSize(size: PageSize, scale: number) {
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
export function scaleForPixels(size: PageSize, pixels: number): number {
	return Math.sqrt(pixels / (size.width * size.height));
}
