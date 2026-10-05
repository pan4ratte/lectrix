<script lang="ts">
	// The comment of the annotation under the pointer (section 6.5): its text only, no
	// author or date, in a tooltip below the annotation (above it when there is no room;
	// a selected annotation's bar is above it). It never takes the pointer.
	import { placeBar, type Area } from './bars.ts';

	interface Props {
		text: string;
		/** The annotation, in the viewer's scrolled content. */
		anchor: Area;
		view: Area;
		contentWidth: number;
	}

	let { text, anchor, view, contentWidth }: Props = $props();

	let w = $state(0);
	let h = $state(0);
	const at = $derived(placeBar(anchor, { w, h }, view, contentWidth, 'below'));
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
