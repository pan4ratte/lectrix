<script lang="ts">
	// Visual adjustments (section 6.1): a button among the page and zoom controls that opens a
	// panel with brightness and contrast (sliders), inverted colours and greyscale (switches),
	// and Reset. They change how this document's pages look on screen, at once, and nothing in
	// the file; the button carries a dot while anything is adjusted.
	import { Contrast } from '@lucide/svelte';
	import { Popover } from 'bits-ui';

	import Slider from '#lib/components/Slider.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { ADJUST_MAX, ADJUST_MIN, ADJUST_STEP, NO_ADJUSTMENTS, isAdjusted } from './adjustments.ts';

	let { tab }: { tab: DocTab } = $props();

	const LABEL = 'Visual adjustments';
	const uid = $props.id();

	let open = $state(false);
	const adjusted = $derived(isAdjusted(tab.adjustments));

	const SLIDERS = [
		{ key: 'brightness', label: 'Brightness' },
		{ key: 'contrast', label: 'Contrast' }
	] as const;
	const SWITCHES = [
		{ key: 'invert', label: 'Invert colours' },
		{ key: 'grayscale', label: 'Greyscale' }
	] as const;

	function reset() {
		tab.adjustments = { ...NO_ADJUSTMENTS };
	}

	// Esc closes the panel and nothing else: the window's own Esc (the selection, the tool,
	// full screen) is not for a panel that is open.
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		open = false;
	}
</script>

<Popover.Root bind:open>
	<Popover.Trigger
		class="icon-button relative size-7"
		aria-label={adjusted ? `${LABEL}: on` : LABEL}
		data-tip={open ? undefined : LABEL}
		data-visual-adjustments
	>
		<Contrast size={16} aria-hidden="true" />
		{#if adjusted}
			<span class="filter-dot adjust-dot" aria-hidden="true"></span>
		{/if}
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content
			class="adjust-panel"
			side="bottom"
			align="center"
			sideOffset={6}
			escapeKeydownBehavior="ignore"
			role="dialog"
			aria-label={LABEL}
			onkeydown={onContentKey}
		>
			{#each SLIDERS as s (s.key)}
				{@const value = tab.adjustments[s.key]}
				<div class="adjust-row">
					<label for="{uid}-{s.key}">{s.label}</label>
					<span class="adjust-value tabular-nums" aria-hidden="true">{value}%</span>
				</div>
				<Slider
					id="{uid}-{s.key}"
					{value}
					min={ADJUST_MIN}
					max={ADJUST_MAX}
					step={ADJUST_STEP}
					valuetext="{value}%"
					oninput={(v) => (tab.adjustments[s.key] = v)}
				/>
			{/each}
			{#each SWITCHES as s (s.key)}
				<div class="adjust-row adjust-switch">
					<label for="{uid}-{s.key}">{s.label}</label>
					<Switch id="{uid}-{s.key}" checked={tab.adjustments[s.key]} onchange={(on) => (tab.adjustments[s.key] = on)} />
				</div>
			{/each}
			<p class="adjust-note">Only how the pages look here. The file is not changed.</p>
			<button type="button" class="button w-full" disabled={!adjusted} onclick={reset}>Reset</button>
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
