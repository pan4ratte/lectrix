<script lang="ts">
	// A tooltip's bubble (section 8): flat, in the menus' colour with their thin edge, and a
	// tail with curved sides pointing at what the tooltip is about. Whoever shows it places
	// it, from the size it reports. It never takes the pointer.
	import { TIP_RADIUS, TIP_TAIL, tailPath, tailShape, type Area } from '#lib/features/annotations/bars.ts';

	interface Props {
		text: string;
		left: number;
		top: number;
		/** What the tail points at, in the coordinates `left` and `top` are in. */
		anchor: Area;
		/** In the window, over everything, instead of in a scrolled parent. */
		fixed?: boolean;
		/** The most lines shown; all of them without it. */
		lines?: number;
		w?: number;
		h?: number;
	}

	let { text, left, top, anchor, fixed = false, lines, w = $bindable(0), h = $bindable(0) }: Props = $props();

	const tail = $derived(
		w === 0
			? null
			: tailShape(
					w,
					h,
					{ x0: anchor.x0 - left, y0: anchor.y0 - top, x1: anchor.x1 - left, y1: anchor.y1 - top },
					TIP_RADIUS,
					TIP_TAIL
				)
	);
	const tailSides = $derived(tail ? tailPath(tail) : '');
</script>

<div
	class="tip"
	class:tip-fixed={fixed}
	class:invisible={w === 0}
	style:left="{left}px"
	style:top="{top}px"
	role="tooltip"
	bind:offsetWidth={w}
	bind:offsetHeight={h}
>
	<div class="tip-text" style:-webkit-line-clamp={lines}>{text}</div>
	{#if tail}
		<svg class="tip-tail" width={w} height={h} aria-hidden="true">
			<path class="tip-tail-fill" d="{tailSides} Z" />
			<path class="tip-tail-edge" d={tailSides} />
		</svg>
	{/if}
</div>
