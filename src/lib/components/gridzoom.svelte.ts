// The size of the pictures in a page grid (`PageGrid.svelte`): the square each page is
// fitted into, which the buttons and the keys step through at once, a mouse wheel notch
// glides to and a touchpad pinch follows, up to the size at which one page fills the grid's view. The
// Combine view and the page organizing tool each have one.

import { ZOOM_GLIDE_MS, zoomBetween } from '#lib/features/viewer/zoom.ts';

import { DEFAULT_THUMB, MIN_THUMB, clampThumb, stepThumb } from './pagegrid.ts';
import { motionMs } from './panes.ts';

/** How long a pinch must rest before the pictures are rendered for their new size. */
const PINCH_REST_MS = 150;

export class GridZoom {
	/** The square each page's picture is fitted into, in CSS pixels: on its way to
	 * `thumbTarget` while the size glides there. */
	thumb = $state(DEFAULT_THUMB);
	/** The size asked for. */
	thumbTarget = $state(DEFAULT_THUMB);
	/** The size the pictures are rendered for: where a glide ends or a pinch rests, so that
	 * they are rendered once and not for every frame on the way. */
	renderThumb = $state(DEFAULT_THUMB);
	/** The largest size: the one at which a single page fills the grid's view, which the
	 * grid measures (no limit until it has). */
	largest = $state(Infinity);
	/** The next smaller and larger of the buttons' sizes, or null at that end. */
	smaller = $derived(stepThumb(this.thumbTarget, -1, this.largest));
	larger = $derived(stepThumb(this.thumbTarget, 1, this.largest));
	private glide = 0;
	private pinch = 1;
	private pinchFrame = 0;
	private rest: ReturnType<typeof setTimeout> | undefined;

	/** `smooth` says whether a new size is glided to (Settings' smooth zooming). */
	constructor(private readonly smooth: () => boolean) {}

	/** Glides the pictures to `size` (140 ms, as zooming the pages does; at once when
	 * Settings' smooth zooming is off or the system asks for reduced motion), or takes it at
	 * once without `glide`. */
	zoomTo(size: number, glide = true) {
		const to = Math.round(clampThumb(size, this.largest));
		this.stopZoom();
		this.thumbTarget = to;
		this.renderThumb = to;
		const from = this.thumb;
		const duration = glide && this.smooth() ? motionMs(ZOOM_GLIDE_MS) : 0;
		if (duration === 0 || from === to) {
			this.thumb = to;
			return;
		}
		const start = performance.now();
		const frame = (now: number) => {
			const t = Math.min(1, (now - start) / duration);
			this.thumb = zoomBetween(from, to, t);
			this.glide = t < 1 ? requestAnimationFrame(frame) : 0;
		};
		this.glide = requestAnimationFrame(frame);
	}

	/** The next smaller or larger of the buttons' sizes, at once, going on from where a
	 * glide is heading. */
	zoomStep(direction: 1 | -1) {
		const next = stepThumb(this.thumbTarget, direction, this.largest);
		if (next !== null) this.zoomTo(next, false);
	}

	/** A mouse wheel notch: glides on by `factor` from where a glide is heading. */
	zoomNotch(factor: number) {
		this.zoomTo(this.thumbTarget * factor);
	}

	/** A touchpad pinch: the pictures follow the fingers, once a frame, and are rendered
	 * for their new size when the pinch rests. */
	zoomPinch(factor: number) {
		this.pinch *= factor;
		if (this.pinchFrame) return;
		this.pinchFrame = requestAnimationFrame(() => {
			const to = clampThumb(this.thumb * this.pinch, this.largest);
			this.stopZoom();
			this.thumb = to;
			this.thumbTarget = to;
			this.rest = setTimeout(() => {
				this.thumb = this.thumbTarget = this.renderThumb = Math.round(this.thumb);
			}, PINCH_REST_MS);
		});
	}

	/** The grid's view changed size: the largest size is the one at which one page fills it.
	 * Pictures larger than that now are that size at once. */
	setLargest(size: number) {
		this.largest = Math.max(MIN_THUMB, size);
		if (this.thumbTarget <= this.largest) return;
		this.stopZoom();
		this.thumb = this.thumbTarget = this.renderThumb = this.largest;
	}

	/** Ends a glide or a pinch where it is. */
	stopZoom() {
		cancelAnimationFrame(this.glide);
		cancelAnimationFrame(this.pinchFrame);
		clearTimeout(this.rest);
		this.glide = 0;
		this.pinchFrame = 0;
		this.pinch = 1;
	}
}
