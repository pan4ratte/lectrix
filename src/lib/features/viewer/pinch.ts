// Touchpad pinches passed on by Rust where the webview would otherwise magnify the whole
// app with them (WebKitGTK). They go to the element under the fingers as the Ctrl+wheel
// events Chromium makes of a pinch, so the viewer zooms the document as it does on Windows,
// and anywhere else nothing happens.

import type { TouchpadPinch } from '#lib/ipc/index.ts';

import { NOTCH_PX } from './zoom.ts';

/**
 * Ctrl+wheel `deltaY` values for a pinch going from scale `from` to `to`: in all,
 * `-100 ln(to / from)`, as Chromium sends, split so that none reads as a mouse wheel notch.
 */
export function pinchDeltas(from: number, to: number): number[] {
	if (!(from > 0) || !(to > 0) || from === to) return [];
	const total = -100 * Math.log(to / from);
	const steps = Math.ceil(Math.abs(total) / (NOTCH_PX - 1));
	return Array.from({ length: steps }, () => total / steps);
}

let lastScale = 1;

export function dispatchPinch(pinch: TouchpadPinch) {
	if (pinch.begin) lastScale = 1;
	const deltas = pinchDeltas(lastScale, pinch.scale);
	lastScale = pinch.scale;
	const target = document.elementFromPoint(pinch.x, pinch.y);
	if (!target) return;
	for (const deltaY of deltas) {
		target.dispatchEvent(
			new WheelEvent('wheel', {
				deltaY,
				deltaMode: WheelEvent.DOM_DELTA_PIXEL,
				ctrlKey: true,
				clientX: pinch.x,
				clientY: pinch.y,
				bubbles: true,
				cancelable: true,
				composed: true
			})
		);
	}
}
