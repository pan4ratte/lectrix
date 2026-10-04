<script lang="ts">
	// A small toolbar floating next to something on the page (section 6.5): placed above or
	// below its anchor in the viewer's scrolled content, so it scrolls with the page. It is
	// hidden until measured, so it never shows at the wrong place.
	import type { Snippet } from 'svelte';

	import { placeBar, type Area } from './bars.ts';

	interface Props {
		label: string;
		anchor: Area;
		/** The visible part of the content. */
		view: Area;
		contentWidth: number;
		prefer: 'above' | 'below';
		children: Snippet;
	}

	let { label, anchor, view, contentWidth, prefer, children }: Props = $props();

	let w = $state(0);
	let h = $state(0);
	const at = $derived(placeBar(anchor, { w, h }, view, contentWidth, prefer));

	// When the bar goes away with the keyboard in it (Esc, or its action ended what it was
	// for), the focus returns to the page view instead of being lost.
	let bar: HTMLDivElement | undefined = $state();
	$effect(() => {
		const node = bar;
		const home = node?.closest<HTMLElement>('.viewer-scroll');
		return () => {
			const active = document.activeElement;
			if (node?.contains(active) || active === document.body || active === null) home?.focus({ preventScroll: true });
		};
	});
</script>

<div
	bind:this={bar}
	class="absolute z-10 flex items-center gap-0.5 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]"
	class:invisible={w === 0}
	style:left="{at.left}px"
	style:top="{at.top}px"
	role="toolbar"
	aria-label={label}
	data-floating-bar
	bind:offsetWidth={w}
	bind:offsetHeight={h}
>
	{@render children()}
</div>
