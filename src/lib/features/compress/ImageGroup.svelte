<script lang="ts">
	// How colour or greyscale images are written, in the Compress dialog (section 6.11): their
	// downsampling, compression and quality.
	import Dropdown from '#lib/components/Dropdown.svelte';
	import Slider from '#lib/components/Slider.svelte';
	import type { ImageSettings } from '#lib/ipc/index.ts';

	import { COMPRESSIONS } from './compress.ts';
	import DownsampleFields from './DownsampleFields.svelte';

	interface Props {
		value: ImageSettings;
		onchange: (value: ImageSettings) => void;
		/** Names the controls apart when there are several groups: "colour", "greyscale". */
		kind?: string;
	}

	let { value, onchange, kind }: Props = $props();

	const named = (what: string) => (kind ? `${what} of ${kind} images` : what);
</script>

<div class="flex flex-col gap-2">
	<DownsampleFields
		value={value.downsample}
		label={kind ? `Downsample ${kind} images` : 'Downsample images'}
		onchange={(downsample) => onchange({ ...value, downsample })}
	/>
	<div class="ml-6 flex items-center gap-2">
		<span class="w-[96px] text-sm" aria-hidden="true">Compression</span>
		<Dropdown
			class="h-7 w-[180px]"
			value={value.compression}
			label={named('Compression')}
			options={COMPRESSIONS}
			onchange={(compression) => onchange({ ...value, compression })}
		/>
	</div>
	<div class="ml-6 flex items-center gap-2">
		<span class="w-[96px] text-sm" aria-hidden="true">Quality</span>
		<span class="text-xs text-fg-muted" aria-hidden="true">Low</span>
		<div class="w-[160px]">
			<Slider
				value={value.quality}
				min={1}
				max={100}
				step={1}
				label={named('Quality')}
				valuetext={String(value.quality)}
				oninput={(quality) => onchange({ ...value, quality })}
			/>
		</div>
		<span class="text-xs text-fg-muted" aria-hidden="true">High</span>
		<span class="w-[28px] text-right text-sm tabular-nums" aria-hidden="true">{value.quality}</span>
	</div>
</div>
