<script lang="ts">
	// Downsampling in the Compress dialog (section 6.11): "Downsample images above [225] dpi
	// to [150] dpi", and the method. The fields keep what is typed; numbers out of range say
	// so under them, and keep Compress disabled until they are right.
	import { untrack } from 'svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import type { DownsampleSettings } from '#lib/ipc/index.ts';

	import { METHODS, downsampleProblem } from './compress.ts';

	interface Props {
		value: DownsampleSettings;
		onchange: (value: DownsampleSettings) => void;
		/** What is downsampled: "Downsample images", "Downsample black-and-white images". */
		label?: string;
	}

	let { value, onchange, label = 'Downsample images' }: Props = $props();

	const parse = (text: string) => (text.trim() === '' ? Number.NaN : Number(text.trim()));
	const shown = (n: number) => (Number.isNaN(n) ? '' : String(n));

	let above = $state('');
	let to = $state('');
	// Shown anew when the settings change from outside (a preset); what is typed stays.
	$effect(() => {
		const v = value;
		untrack(() => {
			if (!Object.is(parse(above), v.aboveDpi)) above = shown(v.aboveDpi);
			if (!Object.is(parse(to), v.toDpi)) to = shown(v.toDpi);
		});
	});

	const problem = $derived(downsampleProblem(value));
</script>

<div class="flex flex-col gap-2">
	<div class="flex flex-wrap items-center gap-x-2 gap-y-1">
		<label class="flex items-center gap-2">
			<input
				type="checkbox"
				checked={value.enabled}
				onchange={(e) => onchange({ ...value, enabled: e.currentTarget.checked })}
			/>
			{label} above
		</label>
		<input
			class="field h-7 w-16 tabular-nums"
			inputmode="numeric"
			value={above}
			disabled={!value.enabled}
			aria-label="Downsample images finer than this resolution, in dpi"
			aria-invalid={problem !== null}
			oninput={(e) => {
				above = e.currentTarget.value;
				onchange({ ...value, aboveDpi: parse(above) });
			}}
		/>
		<span class:opacity-50={!value.enabled}>dpi to</span>
		<input
			class="field h-7 w-16 tabular-nums"
			inputmode="numeric"
			value={to}
			disabled={!value.enabled}
			aria-label="Downsample them to this resolution, in dpi"
			aria-invalid={problem !== null}
			oninput={(e) => {
				to = e.currentTarget.value;
				onchange({ ...value, toDpi: parse(to) });
			}}
		/>
		<span class:opacity-50={!value.enabled}>dpi</span>
	</div>
	{#if problem}
		<p class="ml-6 text-sm text-danger" role="alert">{problem}</p>
	{/if}
	<div class="ml-6 flex items-center gap-2">
		<span class="w-[96px] text-sm" class:opacity-50={!value.enabled} aria-hidden="true">Method</span>
		<Dropdown
			class="h-7 w-[180px]"
			value={value.method}
			label="Downsampling method"
			options={METHODS}
			disabled={!value.enabled}
			onchange={(method) => onchange({ ...value, method })}
		/>
	</div>
</div>
