<script lang="ts">
	// A side pane (section 8): slides open and closed, and resizes from its inner edge with
	// the pointer or the keyboard (a window splitter: arrows, Home, End; double-click resets).
	// The content keeps its width while the pane slides, clipped, so it does not reflow on
	// every frame.
	import type { Snippet } from 'svelte';
	import { cubicOut } from 'svelte/easing';

	import { PANE_MOTION_MS, clampWidth, keyResize, motionMs, shownWidth, type PaneLimits, type PaneSide } from './panes.ts';

	interface Props {
		side: PaneSide;
		open: boolean;
		width: number;
		limits: PaneLimits;
		/** The pane's accessible name. */
		label: string;
		/** False until the remembered layout is in, so restoring it does not animate. */
		animate: boolean;
		children: Snippet;
	}

	let { side, open, width = $bindable(), limits, label, animate, children }: Props = $props();

	let windowWidth = $state(typeof window === 'undefined' ? 0 : window.innerWidth);
	const shown = $derived(shownWidth(width, limits, windowWidth));

	let drag: { pointer: number; x: number; width: number } | null = $state(null);

	function slide(node: HTMLElement) {
		const w = node.getBoundingClientRect().width;
		return {
			duration: animate ? motionMs(PANE_MOTION_MS) : 0,
			easing: cubicOut,
			css: (t: number) => `width: ${t * w}px; opacity: ${t};`
		};
	}

	function onPointerDown(event: PointerEvent) {
		if (event.button !== 0) return;
		event.preventDefault();
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		drag = { pointer: event.pointerId, x: event.clientX, width: shown };
	}

	function onPointerMove(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.pointer) return;
		const dx = event.clientX - drag.x;
		width = clampWidth(drag.width + (side === 'left' ? dx : -dx), limits);
	}

	function onPointerUp(event: PointerEvent) {
		if (drag?.pointer === event.pointerId) drag = null;
	}

	function onKeyDown(event: KeyboardEvent) {
		const next = keyResize(shown, event.key, side, limits);
		if (next === null) return;
		event.preventDefault();
		width = next;
	}
</script>

<svelte:window bind:innerWidth={windowWidth} />

{#if open}
	<aside
		class="relative flex shrink-0 border-line bg-chrome"
		class:border-r={side === 'left'}
		class:border-l={side === 'right'}
		style:width="{shown}px"
		aria-label={label}
		transition:slide
	>
		<div class="flex min-w-0 flex-1 flex-col overflow-hidden">
			<div class="flex min-h-0 flex-1 flex-col" style:width="{shown - 1}px">
				{@render children()}
			</div>
		</div>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			class="pane-resizer"
			class:pane-resizer-left={side === 'left'}
			class:pane-resizer-right={side === 'right'}
			class:pane-resizer-active={drag !== null}
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize {label.toLowerCase()}"
			aria-valuemin={limits.min}
			aria-valuemax={limits.max}
			aria-valuenow={shown}
			tabindex="0"
			title="Drag to resize; double-click to reset"
			onpointerdown={onPointerDown}
			onpointermove={onPointerMove}
			onpointerup={onPointerUp}
			onpointercancel={onPointerUp}
			onkeydown={onKeyDown}
			ondblclick={() => (width = limits.initial)}
		></div>
	</aside>
{/if}
