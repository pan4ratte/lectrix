<script lang="ts">
	// A side pane (section 8): slides open and closed, and resizes from its inner edge with
	// the pointer or the keyboard (`PaneResizer.svelte`).
	// The content keeps its width while the pane slides, clipped, so it does not reflow on
	// every frame. A line on its inner edge parts it from the page, below the top row: that
	// row and the view bar beside it read as one bar. Full screen (section 6.1) it lies over
	// the pages instead of beside them (`overlay`), so they keep their size as it comes and
	// goes, and it slides by a transform. As full screen starts or ends, a pane in its place
	// beside the pages slides out of sight or back by a transform too (`sliding`, `away`),
	// which lays nothing out.
	import type { Snippet } from 'svelte';
	import { cubicOut } from 'svelte/easing';

	import PaneResizer from './PaneResizer.svelte';
	import { PANE_MOTION_MS, motionMs, shownWidth, type PaneLimits, type PaneSide } from './panes.ts';

	interface Props {
		side: PaneSide;
		open: boolean;
		/** Over the pages, at its side of them, rather than beside them. */
		overlay?: boolean;
		/** Full screen is starting or ending: the pane moves by `away`, not by opening and closing. */
		sliding?: boolean;
		/** Slid out of sight, to its side. */
		away?: boolean;
		width: number;
		limits: PaneLimits;
		/** The pane's accessible name. */
		label: string;
		/** False until the remembered layout is in, so restoring it does not animate. */
		animate: boolean;
		children: Snippet;
	}

	let { side, open, overlay = false, sliding = false, away = false, width = $bindable(), limits, label, animate, children }: Props = $props();

	let windowWidth = $state(typeof window === 'undefined' ? 0 : window.innerWidth);
	const shown = $derived(shownWidth(width, limits, windowWidth));

	function slide(node: HTMLElement) {
		// As full screen starts or ends the pane is, or has just been, slid away already.
		const still = !animate || sliding || node.classList.contains('chrome-away');
		const duration = still ? 0 : motionMs(PANE_MOTION_MS);
		if (overlay) {
			const to = side === 'left' ? -100 : 100;
			return { duration, easing: cubicOut, css: (t: number) => `transform: translateX(${(1 - t) * to}%); opacity: ${t};` };
		}
		const w = node.getBoundingClientRect().width;
		return { duration, easing: cubicOut, css: (t: number) => `width: ${t * w}px; opacity: ${t};` };
	}
</script>

<svelte:window bind:innerWidth={windowWidth} />

{#if open}
	<aside
		class={[
			'flex shrink-0 bg-chrome',
			overlay ? 'pane-overlay' : 'relative',
			overlay && (side === 'left' ? 'left-0' : 'right-0'),
			sliding && ['chrome-slide z-10', side === 'left' ? 'chrome-left' : 'chrome-right']
		]}
		class:chrome-away={away}
		class:pr-px={side === 'left'}
		class:pl-px={side === 'right'}
		style:width="{shown}px"
		aria-label={label}
		data-pane={side}
		transition:slide
	>
		<div class="flex min-w-0 flex-1 flex-col overflow-hidden">
			<div class="flex min-h-0 flex-1 flex-col" style:width="{shown - 1}px">
				{@render children()}
			</div>
		</div>
		<span class="pane-edge" class:right-0={side === 'left'} class:left-0={side === 'right'} aria-hidden="true"></span>
		<PaneResizer {side} bind:width {shown} {limits} {label} />
	</aside>
{/if}
