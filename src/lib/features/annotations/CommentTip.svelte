<script lang="ts">
	// The comment of the annotation under the pointer (section 6.5): its text only, no
	// author or date, with its line breaks. It follows the pointer, below it with its tail
	// pointing at it, and stays wholly inside the visible part of the page area. It never
	// takes the pointer.
	import Tip from '#lib/components/Tip.svelte';

	import { placeTip, type Area } from './bars.ts';

	interface Props {
		text: string;
		/** The pointer, in the viewer's scrolled content. */
		pointer: { x: number; y: number };
		view: Area;
		contentWidth: number;
	}

	let { text, pointer, view, contentWidth }: Props = $props();

	let w = $state(0);
	let h = $state(0);
	const at = $derived(placeTip(pointer, { w, h }, view, contentWidth));
	const anchor = $derived({ x0: pointer.x, y0: pointer.y, x1: pointer.x, y1: pointer.y });
</script>

<Tip {text} left={at.left} top={at.top} {anchor} lines={12} bind:w bind:h />
