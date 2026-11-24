<script lang="ts">
	// A small toolbar floating next to something on the page (section 6.5): placed above or
	// below its anchor in the viewer's scrolled content, so it scrolls with the page. It is
	// hidden until measured, so it never shows at the wrong place. It comes and goes with
	// a slight zoom from its anchor (section 8.3).
	import type { Snippet } from 'svelte';

	import { popIn, popOut } from '#lib/components/pop.ts';

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
	/** It grows from the middle of its anchor, on the side towards it. */
	const origin = $derived(`${(anchor.x0 + anchor.x1) / 2 - at.left}px ${at.top < anchor.y0 ? `${h}px` : '0px'}`);

	// When the bar goes away with the keyboard in it (Esc, or its action ended what it was
	// for), the focus returns to the page view instead of being lost: as it starts to go,
	// and again once gone.
	let bar: HTMLDivElement | undefined = $state();
	function giveBackFocus(node: HTMLElement | undefined, home: HTMLElement | null | undefined) {
		const active = document.activeElement;
		if (node?.contains(active) || active === document.body || active === null) home?.focus({ preventScroll: true });
	}
	$effect(() => {
		const node = bar;
		const home = node?.closest<HTMLElement>('.viewer-scroll');
		return () => giveBackFocus(node, home);
	});
</script>

<div
	bind:this={bar}
	class="absolute z-10 flex items-center gap-0.5 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]"
	class:invisible={w === 0}
	style:left="{at.left}px"
	style:top="{at.top}px"
	style:transform-origin={origin}
	in:popIn
	out:popOut
	onoutrostart={() => giveBackFocus(bar, bar?.closest<HTMLElement>('.viewer-scroll'))}
	role="toolbar"
	aria-label={label}
	data-floating-bar
	bind:offsetWidth={w}
	bind:offsetHeight={h}
>
	{@render children()}
</div>
