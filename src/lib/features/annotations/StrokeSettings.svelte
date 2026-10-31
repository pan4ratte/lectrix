<script lang="ts">
	// The stroke's sliders (section 6.5): its width and, for the pen, how much it smooths
	// its strokes, each under its name and value. They are in the Pen's and the Shapes tool's
	// panels (ToolOptions) and the panel of a drawing's stroke button (StrokePicker). Each
	// applies when its slider is let go (or with each arrow key), so changing a drawing's
	// width is one undo step.
	import Slider from '#lib/components/Slider.svelte';

	import { MAX_WIDTH, MIN_WIDTH, WIDTH_STEP } from './pen.ts';

	interface Props {
		/** Points. */
		width: number;
		onwidth: (width: number) => void;
		/** 0 to 100 (%), or null for no smoothing slider. */
		smoothing?: number | null;
		onsmoothing?: (smoothing: number) => void;
	}

	let { width, onwidth, smoothing = null, onsmoothing }: Props = $props();

	const uid = $props.id();

	/** The sliders' values while they move, before they are applied. */
	let widthValue = $state(2);
	let smoothValue = $state(0);
	$effect(() => {
		widthValue = width;
	});
	$effect(() => {
		smoothValue = smoothing ?? 0;
	});

	const smoothText = (s: number) => (s === 0 ? 'Off' : `${s}%`);

	function applyWidth() {
		if (Math.abs(widthValue - width) > 1e-3) onwidth(widthValue);
	}

	function applySmoothing() {
		if (smoothing !== null && smoothValue !== smoothing) onsmoothing?.(smoothValue);
	}
</script>

<script lang="ts" module>
	/** A width as the panels and the buttons show it: "2 pt", "1.5 pt"; a width another app
	 * wrote to more places, to two. */
	export const pt = (w: number) => `${Number(w.toFixed(2))} pt`;
</script>

<div class="flex flex-col">
	<div class="adjust-row">
		<label for="{uid}-width">Stroke width</label>
		<span class="adjust-value tabular-nums" aria-hidden="true">{pt(widthValue)}</span>
	</div>
	<Slider
		id="{uid}-width"
		bind:value={widthValue}
		min={MIN_WIDTH}
		max={MAX_WIDTH}
		step={WIDTH_STEP}
		valuetext={pt(widthValue)}
		onchange={applyWidth}
	/>
	{#if smoothing !== null}
		<div class="adjust-row mt-2">
			<label for="{uid}-smoothing">Smoothing</label>
			<span class="adjust-value tabular-nums" aria-hidden="true">{smoothText(smoothValue)}</span>
		</div>
		<Slider
			id="{uid}-smoothing"
			bind:value={smoothValue}
			min={0}
			max={100}
			step={5}
			valuetext={smoothText(smoothValue)}
			describedby="{uid}-smoothing-note"
			onchange={applySmoothing}
		/>
		<p id="{uid}-smoothing-note" class="adjust-note mb-0">Steadies strokes: the pen trails the pointer and ignores small wobbles.</p>
	{/if}
</div>
