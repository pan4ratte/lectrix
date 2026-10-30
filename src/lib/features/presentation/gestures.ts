// What the pointer does on a presentation's page (section 6.1), on the window presenting
// and on an audience screen alike: a click turns on or follows a link, a right click turns
// back, the wheel and a swipe turn either way.

import { WheelSteps, swipeStep } from './input.ts';

export interface GestureTarget {
	/** Whether the pages show and take the pointer. */
	active(): boolean;
	/** The link at a point (client pixels) of the page shown, by its index; null for none. */
	linkAt(x: number, y: number): number | null;
	step(direction: -1 | 1): void;
	follow(link: number): void;
	/** Whether an event's target is a control, which deals with it itself. */
	onControl(target: EventTarget | null): boolean;
}

/** How long after a swipe the click a touch ends with is left alone. */
const AFTER_SWIPE_MS = 500;

export class StageGestures {
	#wheel = new WheelSteps();
	#touch: { id: number; x: number; y: number } | null = null;
	#swipedAt = -Infinity;

	constructor(private readonly target: GestureTarget) {}

	click = (event: MouseEvent) => {
		const t = this.target;
		if (!t.active() || event.button !== 0 || t.onControl(event.target)) return;
		if (event.timeStamp - this.#swipedAt < AFTER_SWIPE_MS) return;
		const link = t.linkAt(event.clientX, event.clientY);
		if (link === null) t.step(1);
		else t.follow(link);
	};

	contextMenu = (event: MouseEvent) => {
		event.preventDefault();
		if (this.target.active() && !this.target.onControl(event.target)) this.target.step(-1);
	};

	wheel = (event: WheelEvent) => {
		if (!this.target.active() || event.ctrlKey) return;
		const step = this.#wheel.push(event.deltaY, event.deltaMode, event.timeStamp);
		if (step) this.target.step(step);
	};

	pointerDown = (event: PointerEvent) => {
		if (event.pointerType === 'touch' && event.isPrimary) this.#touch = { id: event.pointerId, x: event.clientX, y: event.clientY };
	};

	pointerUp = (event: PointerEvent) => {
		const touch = this.#touch;
		if (!touch || event.pointerId !== touch.id) return;
		this.#touch = null;
		const step = swipeStep(event.clientX - touch.x, event.clientY - touch.y);
		if (!step || !this.target.active()) return;
		this.#swipedAt = event.timeStamp;
		this.target.step(step);
	};

	pointerCancel = () => {
		this.#touch = null;
	};
}
