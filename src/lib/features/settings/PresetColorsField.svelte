<script lang="ts">
	// The colour panel's 18 presets in Settings (sections 6.5 and 6.6): the swatches as the
	// panel shows them, each a button that opens Lectrix's colour picker (HexPicker) to give
	// it another colour, with a button there to put its default back. Only the presets that
	// differ from the defaults are kept: a preset's place in the panel and its colour.
	import { Popover } from 'bits-ui';

	import HexPicker from '#lib/features/annotations/HexPicker.svelte';
	import { PRESET_COLORS, presetColors } from '#lib/features/annotations/tools.ts';

	interface Props {
		id: string;
		labelledby: string;
		describedby?: string;
		/** The presets that differ from the defaults, by their place in the panel. */
		value: Readonly<Record<number, string>>;
		onchange: (value: Record<number, string>) => void;
	}

	let { id, labelledby, describedby, value, onchange }: Props = $props();

	const presets = $derived(presetColors(value));
	/** The preset whose picker is open. */
	let editing = $state<number | null>(null);

	function set(place: number, color: string | null) {
		const next = { ...value };
		if (color === null || color.toLowerCase() === PRESET_COLORS[place]?.value) delete next[place];
		else next[place] = color.toLowerCase();
		onchange(next);
	}

	/** Esc closes the picker only, not Settings. */
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		editing = null;
	}
</script>

<div {id} class="swatch-grid" role="group" aria-labelledby={labelledby} aria-describedby={describedby}>
	{#each presets as c, i (i)}
		{@const original = PRESET_COLORS[i]}
		{@const changed = original !== undefined && c.value !== original.value}
		<Popover.Root bind:open={() => editing === i, (open) => (editing = open ? i : editing === i ? null : editing)}>
			<Popover.Trigger
				class="swatch"
				aria-label={changed ? `${c.name}, in place of ${original.name}` : c.name}
				data-tip={editing === i ? undefined : changed ? `${c.name} (was ${original.name})` : c.name}
				data-preset={i}
				style="--swatch: {c.value}"
			></Popover.Trigger>
			<Popover.Portal>
				<Popover.Content
					side="bottom"
					sideOffset={6}
					escapeKeydownBehavior="ignore"
					class="color-panel"
					aria-label="Colour of this preset"
					onkeydown={onContentKey}
				>
					<HexPicker color={c.value} onpick={(color) => set(i, color)} />
					{#if changed}
						<button type="button" class="button mt-2 h-7 w-full text-xs" onclick={() => set(i, null)}>
							Back to {original.name.toLowerCase()}
						</button>
					{/if}
				</Popover.Content>
			</Popover.Portal>
		</Popover.Root>
	{/each}
</div>
