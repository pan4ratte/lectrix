<script lang="ts">
	// A choice between a few peers shown side by side (the theme: system, light, dark): a
	// radio group in one pill, the chosen one raised by a mark that glides to it. One Tab
	// stop; arrow keys move to the next one and choose it, as radio buttons do.
	import type { Component } from 'svelte';

	import { toolMark } from '#lib/features/annotations/toolMark.ts';

	interface Option {
		value: string;
		label: string;
		icon?: Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>;
	}

	interface Props {
		value: string;
		options: readonly Option[];
		onchange?: (value: string) => void;
		id?: string;
		/** The id of the element that names the group. */
		labelledby?: string;
		describedby?: string;
		disabled?: boolean;
	}

	let { value, options, onchange, id, labelledby, describedby, disabled = false }: Props = $props();

	let group = $state<HTMLElement | null>(null);

	function onKey(event: KeyboardEvent, index: number) {
		const last = options.length - 1;
		const steps: Record<string, number> = {
			ArrowRight: index === last ? 0 : index + 1,
			ArrowDown: index === last ? 0 : index + 1,
			ArrowLeft: index === 0 ? last : index - 1,
			ArrowUp: index === 0 ? last : index - 1,
			Home: 0,
			End: last
		};
		const next = steps[event.key];
		if (next === undefined) return;
		event.preventDefault();
		event.stopPropagation();
		const option = options[next];
		if (!option) return;
		group?.querySelectorAll<HTMLElement>('[role=radio]')[next]?.focus();
		if (option.value !== value) onchange?.(option.value);
	}
</script>

<div
	bind:this={group}
	{id}
	class="segmented tool-group"
	role="radiogroup"
	aria-labelledby={labelledby}
	aria-describedby={describedby}
	{@attach toolMark}
>
	<span class="tool-mark segment-mark" aria-hidden="true"></span>
	{#each options as o, i (o.value)}
		<button
			type="button"
			class="segment glide-item"
			role="radio"
			aria-checked={o.value === value}
			tabindex={o.value === value ? 0 : -1}
			{disabled}
			onclick={() => {
				if (o.value !== value) onchange?.(o.value);
			}}
			onkeydown={(e) => onKey(e, i)}
		>
			{#if o.icon}
				<o.icon size={14} aria-hidden="true" />
			{/if}
			{o.label}
		</button>
	{/each}
</div>
