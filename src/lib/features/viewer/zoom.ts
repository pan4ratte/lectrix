// Zoom levels, fitting, and the scale pages are rendered at.

import { CSS_PX_PER_PT, PAGE_GAP, PAGE_MARGIN, type Size } from './layout.ts';

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

/** Width left for pages in a viewport `viewportWidth` wide, with `columns` side by side. */
function availableWidth(viewportWidth: number, columns: 1 | 2): number {
	// One pixel of slack, so rounding never adds a horizontal scrollbar.
	return viewportWidth - 2 * PAGE_MARGIN - (columns - 1) * PAGE_GAP - 1;
}

/** Zoom at which `size` (a page as it is turned on screen) fills the viewport's width (minus
 * margins and a scrollbar); with two columns, two pages of that size side by side. */
export function fitWidthZoom(size: Size, viewportWidth: number, columns: 1 | 2 = 1): number {
	return clampZoom(availableWidth(viewportWidth, columns) / (columns * size.width * CSS_PX_PER_PT));
}

/** Zoom at which the whole page (or two side by side) fits in the viewport. */
export function fitPageZoom(size: Size, viewportWidth: number, viewportHeight: number, columns: 1 | 2 = 1): number {
	const { width, height } = size;
	const zw = availableWidth(viewportWidth, columns) / (columns * width * CSS_PX_PER_PT);
	const zh = (viewportHeight - 2 * PAGE_MARGIN - 1) / (height * CSS_PX_PER_PT);
	return clampZoom(Math.min(zw, zh));
}

/**
 * Device-pixel scale (pixels per point) to render at for `zoom` on a display with
 * `devicePixelRatio`: the page's own on-screen scale, to the thousandth the page URL
 * carries, so its pixels land one to one on the screen's. An image rendered a few percent
 * larger and shrunk by the webview had every glyph edge smeared over its neighbours.
 */
export function renderScale(zoom: number, devicePixelRatio: number): number {
	const raw = zoom * CSS_PX_PER_PT * (devicePixelRatio || 1);
	return Math.max(0.001, Math.round(raw * 1000) / 1000);
}

/**
 * CSS pixels per image pixel for an image rendered at `scale` of a page shown at `zoom`:
 * one device pixel when the scale is the page's own (`renderScale`), so the image is shown
 * as it was rendered, not resampled; the page's size otherwise (an image of another zoom,
 * stretched while its successor renders, or a low-resolution underlay).
 */
export function cssPerImagePixel(scale: number, zoom: number, devicePixelRatio: number): number {
	const dpr = devicePixelRatio || 1;
	return scale === renderScale(zoom, dpr) ? 1 / dpr : (zoom * CSS_PX_PER_PT) / scale;
}

/** `css` rounded to the nearest whole device pixel. */
export function snapToDevice(css: number, devicePixelRatio: number): number {
	const dpr = devicePixelRatio || 1;
	return Math.round(css * dpr) / dpr;
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

/** Whether a page rendered at `scale` is drawn from tiles over a low-resolution underlay. */
export function isTiled(size: Size, scale: number): boolean {
	const { width, height } = pixelSize(size, scale);
	return width * height > TILE_THRESHOLD_PIXELS;
}

/**
 * The scale of the one image of the whole page, for a page rendered at `scale`: that scale,
 * or the underlay's when the page is drawn from tiles.
 */
export function wholePageScale(size: Size, scale: number): number {
	if (!isTiled(size, scale)) return scale;
	return Math.min(scale, Math.round(scaleForPixels(size, UNDERLAY_PIXELS) * 1000) / 1000);
}
