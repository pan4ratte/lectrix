<script lang="ts">
	// The comment of the annotation under the pointer (section 6.5): its text only, no
	// author or date, with its line breaks. It follows the pointer, below and right of it,
	// and stays wholly inside the visible part of the page area. It never takes the pointer.
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
</script>

<div
	class="comment-tip"
	class:invisible={w === 0}
	style:left="{at.left}px"
	style:top="{at.top}px"
	role="tooltip"
	bind:offsetWidth={w}
	bind:offsetHeight={h}
>
	{text}
</div>
