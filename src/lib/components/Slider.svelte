<script lang="ts">
	// A slider: a thin track with no outline, filled up to the handle in the accent, and a
	// round handle like a switch's thumb with a thin edge and a slight shadow (the
	// `--lectrix-slider-*` tokens). The track, the fill and the handle are drawn here, so
	// they can glide to each new step; a native range input lies over them, unseen, and takes
	// the pointer, the keyboard and screen readers as usual. `oninput` follows the handle;
	// `onchange` comes when it is let go.

	interface Props {
		value: number;
		min: number;
		max: number;
		step: number;
		oninput?: (value: number) => void;
		onchange?: (value: number) => void;
		id?: string;
		/** The accessible name, when no <label> gives one. */
		label?: string;
		/** The value as screen readers say it ("40%"). */
		valuetext?: string;
		describedby?: string;
		disabled?: boolean;
	}

	let { value = $bindable(), min, max, step, oninput, onchange, id, label, valuetext, describedby, disabled = false }: Props = $props();

	/** How far along the track the handle is, 0 to 1: the track is filled up to there. */
	const ratio = $derived(max > min ? Math.min(1, Math.max(0, (value - min) / (max - min))) : 0);
</script>

<span class="slider" style:--ratio={ratio}>
	<span class="slider-track" aria-hidden="true"><span class="slider-fill"></span></span>
	<span class="slider-handle" aria-hidden="true"></span>
	<input
		{id}
		{disabled}
		type="range"
		{min}
		{max}
		{step}
		{value}
		oninput={(e) => {
			value = Number(e.currentTarget.value);
			oninput?.(value);
		}}
		onchange={() => onchange?.(value)}
		aria-label={label}
		aria-valuetext={valuetext}
		aria-describedby={describedby}
	/>
</span>
