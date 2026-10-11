<script lang="ts">
	// The app's tooltips (section 8). A control names its tooltip in `data-tip`, and this one
	// layer shows it in the app's own style, where the system would draw a `title` in its
	// own. It shows once the pointer has rested on the control, or when Tab or an arrow key
	// brings the keyboard focus there, and at once when another tooltip has just gone. A
	// press, a key, scrolling or leaving the control takes it away.
	import type { Area, Point } from '#lib/features/annotations/bars.ts';

	import Tip from './Tip.svelte';
	import { TOOLTIP_DELAY_MS, TOOLTIP_WARM_MS, placeTooltip, tooltipAnchor, tooltipDescription } from './tooltip.ts';

	let shown = $state<{ text: string; anchor: Area } | null>(null);
	let w = $state(0);
	let h = $state(0);
	let innerWidth = $state(0);
	let innerHeight = $state(0);
	const at = $derived(shown ? placeTooltip(shown.anchor, { w, h }, { w: innerWidth, h: innerHeight }) : { left: 0, top: 0 });

	/** The control whose tooltip shows or is waiting to, and what brought it. */
	let target: HTMLElement | null = null;
	let by: 'pointer' | 'focus' = 'pointer';
	/** The control under the pointer, and the one whose tooltip a press or a key took away:
	 * it stays away until the pointer leaves that control. */
	let hovered: HTMLElement | null = null;
	let ignored: HTMLElement | null = null;
	let pointer: Point = { x: 0, y: 0 };
	/** Where the pointer was when the tooltip showed: a large control's tooltip stays there. */
	let shownAt: Point | null = null;
	/** Whether the last key pressed moves the focus (Tab, an arrow): only then does a focused
	 * control show its tooltip, not when a click or a command puts the focus there. */
	let movedByKey = false;
	let wentAt = -Infinity;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let frame = 0;

	function tipped(node: EventTarget | null): HTMLElement | null {
		return node instanceof Element ? node.closest<HTMLElement>('[data-tip]') : null;
	}

	/** Keeps the tooltip on its control, whose text and place may change while it shows;
	 * false once the control has no tooltip or is gone. */
	function sync(): boolean {
		const text = target?.isConnected ? (target.dataset.tip ?? '') : '';
		if (!target || !text) return false;
		const r = target.getBoundingClientRect();
		if (r.width === 0 && r.height === 0) return false;
		const anchor = tooltipAnchor({ x0: r.left, y0: r.top, x1: r.right, y1: r.bottom }, shownAt);
		const a = shown?.anchor;
		if (shown?.text !== text || !a || a.x0 !== anchor.x0 || a.y0 !== anchor.y0 || a.x1 !== anchor.x1 || a.y1 !== anchor.y1) {
			shown = { text, anchor };
		}
		return true;
	}

	function follow() {
		if (sync()) frame = requestAnimationFrame(follow);
		else drop();
	}

	function drop() {
		clearTimeout(timer);
		cancelAnimationFrame(frame);
		if (shown) wentAt = performance.now();
		shown = null;
		w = 0;
		h = 0;
		target = null;
	}

	function aim(el: HTMLElement | null, how: 'pointer' | 'focus') {
		if (el === target) return;
		drop();
		if (!el) return;
		target = el;
		by = how;
		timer = setTimeout(
			() => {
				shownAt = how === 'pointer' ? pointer : null;
				follow();
			},
			performance.now() - wentAt < TOOLTIP_WARM_MS ? 0 : TOOLTIP_DELAY_MS
		);
	}

	/** The pointer is on `el` (or on no control with a tooltip). */
	function over(el: HTMLElement | null) {
		hovered = el;
		if (el && el === ignored) return;
		ignored = null;
		// The focused control's tooltip stays while the pointer is on nothing that has one.
		if (!el && target && by === 'focus') return;
		aim(el, 'pointer');
	}

	/**
	 * For screen readers, what the system's `title` gave: the tooltip as the control's
	 * description, unless it only repeats the control's name.
	 */
	function describe(el: HTMLElement) {
		const text = tooltipDescription(el.dataset.tip ?? '', el.getAttribute('aria-label') ?? el.textContent ?? '');
		if (text) el.setAttribute('aria-description', text);
		else el.removeAttribute('aria-description');
	}

	$effect(() => {
		const stop = new AbortController();
		const on = <K extends keyof WindowEventMap>(type: K, handler: (event: WindowEventMap[K]) => void) =>
			window.addEventListener(type, handler, { capture: true, passive: true, signal: stop.signal });

		on('pointerover', (e) => {
			if (e.pointerType === 'touch') return;
			pointer = { x: e.clientX, y: e.clientY };
			// Nothing shows while something is dragged over the controls.
			over(e.buttons === 0 ? tipped(e.target) : null);
		});
		on('pointermove', (e) => (pointer = { x: e.clientX, y: e.clientY }));
		// The pointer left the window.
		on('pointerout', (e) => {
			if (!e.relatedTarget) over(null);
		});
		on('pointerdown', (e) => {
			movedByKey = false;
			ignored = tipped(e.target);
			drop();
			wentAt = -Infinity;
		});
		on('keydown', (e) => {
			movedByKey = e.key === 'Tab' || e.key.startsWith('Arrow');
			ignored = hovered;
			drop();
		});
		on('focusin', (e) => {
			const el = e.target instanceof HTMLElement && e.target.dataset.tip ? e.target : null;
			if (el) describe(el);
			if (movedByKey) aim(el?.matches(':focus-visible') ? el : null, 'focus');
		});
		on('focusout', () => {
			if (target && by === 'focus') drop();
		});
		on('scroll', drop);
		on('wheel', drop);
		on('blur', (e) => {
			if (e.target === window) drop();
		});

		return () => {
			stop.abort();
			drop();
		};
	});
</script>

<svelte:window bind:innerWidth bind:innerHeight />

{#if shown}
	<Tip text={shown.text} left={at.left} top={at.top} anchor={shown.anchor} fixed bind:w bind:h />
{/if}
