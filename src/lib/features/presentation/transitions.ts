// A page's transition in a presentation (section 6.1, ADR 0033): how the page coming on
// screen and the page going look at a moment of it. The effects of the standard (ISO
// 32000-2 Table 164) are drawn with what the compositor does cheaply: opacity, transforms,
// clip paths and gradient masks. Dissolve fades, and Glitter is a wipe with a soft edge,
// rather than scattering pixels.

import type { Transition } from '#lib/ipc/index.ts';

/** How a page looks at a moment: CSS values, or undefined for none. */
export interface PageLook {
	opacity?: number;
	transform?: string;
	clipPath?: string;
	mask?: string;
}

export interface Frame {
	/** The page coming. */
	incoming: PageLook;
	/** The page going. */
	outgoing: PageLook;
	/** The page going lies over the page coming (it is what moves or shrinks). */
	outgoingOnTop: boolean;
}

/** How many blinds there are. */
export const BLINDS = 8;

/** The way `direction` moves on screen, y down: 0 is to the right, 90 up, 270 down. */
export function directionVector(direction: number): [number, number] {
	const radians = (direction * Math.PI) / 180;
	const round = (v: number) => Math.round(v * 1e6) / 1e6 + 0;
	return [round(Math.cos(radians)), round(-Math.sin(radians))];
}

/** A CSS gradient angle along `direction` (CSS's 0deg points up, 90deg right). */
function gradientAngle(direction: number): number {
	return (((90 - direction) % 360) + 360) % 360;
}

const pct = (v: number) => `${Math.round(v * 10000) / 100}%`;

/**
 * How the two pages look a fraction `t` (0 to 1) of the way through `transition`, on a
 * screen `width` by `height` (CSS pixels), which what slides in or out crosses.
 */
export function transitionFrame(transition: Transition, t: number, width: number, height: number): Frame {
	const p = Math.min(1, Math.max(0, t));
	const frame = (incoming: PageLook, outgoing: PageLook = {}, outgoingOnTop = false): Frame => ({
		incoming,
		outgoing,
		outgoingOnTop
	});
	const [dx, dy] = directionVector(transition.direction ?? 0);
	const slide = (amount: number) => `translate(${Math.round(dx * width * amount)}px, ${Math.round(dy * height * amount)}px)`;
	switch (transition.style) {
		case 'fade':
		case 'dissolve':
			return frame({ opacity: p });
		case 'wipe': {
			const edge = pct(p);
			return frame({ mask: `linear-gradient(${gradientAngle(transition.direction ?? 0)}deg, #000 ${edge}, transparent ${edge})` });
		}
		case 'glitter': {
			// A soft edge 15% wide, from before the page to past it.
			const at = p * 1.15;
			return frame({
				mask: `linear-gradient(${gradientAngle(transition.direction ?? 0)}deg, #000 ${pct(at - 0.15)}, transparent ${pct(at)})`
			});
		}
		case 'split': {
			// Two lines, horizontal or vertical, from the middle out or from the edges in.
			const inset = (share: number) =>
				transition.horizontal ? `inset(${pct(share)} 0 ${pct(share)} 0)` : `inset(0 ${pct(share)} 0 ${pct(share)})`;
			return transition.inward ? frame({}, { clipPath: inset(p / 2) }, true) : frame({ clipPath: inset((1 - p) / 2) });
		}
		case 'box':
			return transition.inward
				? frame({}, { clipPath: `inset(${pct(p / 2)})` }, true)
				: frame({ clipPath: `inset(${pct((1 - p) / 2)})` });
		case 'blinds': {
			const band = 1 / BLINDS;
			const open = pct(band * p);
			const to = transition.horizontal ? 'to bottom' : 'to right';
			return frame({ mask: `repeating-linear-gradient(${to}, #000 0, #000 ${open}, transparent ${open}, transparent ${pct(band)})` });
		}
		case 'push':
			return frame({ transform: slide(p - 1) }, { transform: slide(p) });
		case 'cover':
			return frame({ transform: slide(p - 1) });
		case 'uncover':
			return frame({}, { transform: slide(p) }, true);
		case 'fly': {
			const scale = transition.scale > 0 ? transition.scale : 1;
			// Not opaque: only what is on the page moves, which a fade comes closest to.
			const fade = (o: number) => (transition.opaque ? undefined : o);
			if (transition.inward) {
				const s = scale + (1 - scale) * p;
				const move = transition.direction === null ? '' : `${slide(p - 1)} `;
				return frame({ transform: `${move}scale(${round3(s)})`, opacity: fade(p) });
			}
			const s = 1 + (scale - 1) * p;
			const move = transition.direction === null ? '' : `${slide(p)} `;
			return frame({}, { transform: `${move}scale(${round3(s)})`, opacity: fade(1 - p) }, true);
		}
	}
}

function round3(v: number): number {
	return Math.round(v * 1000) / 1000;
}
