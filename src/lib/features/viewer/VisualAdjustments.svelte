<script lang="ts">
	// Visual adjustments (section 6.1): a button among the page and zoom controls that opens a
	// panel with brightness, contrast and sepia (sliders), inverted colours and greyscale
	// (switches), and Reset. They change how this document's pages look on screen, at once,
	// and nothing in the file; the button carries a dot while anything is adjusted. A
	// document starts with the adjustments Settings give every document, has its own once
	// something is changed here, and goes back to those of Settings with Reset.
	import { Contrast } from '@lucide/svelte';
	import { Popover } from 'bits-ui';

	import Slider from '#lib/components/Slider.svelte';
	import Switch from '#lib/components/Switch.svelte';
	import { menuOffset, type MenuSide } from '#lib/features/annotations/bars.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { ADJUST_MAX, ADJUST_MIN, ADJUST_STEP, SEPIA_MAX, isAdjusted } from './adjustments.ts';

	/** `side`: where the panel opens, beside the button in a stacked toolbar. */
	let { tab, side = 'bottom' }: { tab: DocTab; side?: MenuSide } = $props();

	const LABEL = 'Visual adjustments';
	const uid = $props.id();

	let open = $state(false);
	const adjusted = $derived(isAdjusted(tab.adjustments));

	const SLIDERS = [
		{ key: 'brightness', label: 'Brightness', min: ADJUST_MIN, max: ADJUST_MAX },
		{ key: 'contrast', label: 'Contrast', min: ADJUST_MIN, max: ADJUST_MAX },
		{ key: 'sepia', label: 'Sepia', min: 0, max: SEPIA_MAX }
	] as const;
	const SWITCHES = [
		{ key: 'invert', label: 'Invert colours' },
		{ key: 'grayscale', label: 'Greyscale' }
	] as const;

	/** Back to the adjustments of Settings. The button has nothing left to do then and is
	 * disabled, which would leave the keyboard focus on a control that takes no keys (Esc
	 * among them): the focus goes to the first slider. */
	function reset(event: MouseEvent) {
		const panel = (event.currentTarget as HTMLElement).closest('.adjust-panel');
		tab.resetAdjustments();
		panel?.querySelector<HTMLElement>('input')?.focus();
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
			{side}
			align={side === 'bottom' ? 'center' : 'start'}
			sideOffset={menuOffset(side, 6)}
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
					min={s.min}
					max={s.max}
					step={ADJUST_STEP}
					valuetext="{value}%"
					oninput={(v) => tab.adjust({ [s.key]: v })}
				/>
			{/each}
			{#each SWITCHES as s (s.key)}
				<div class="adjust-row adjust-switch">
					<label for="{uid}-{s.key}">{s.label}</label>
					<Switch id="{uid}-{s.key}" checked={tab.adjustments[s.key]} onchange={(on) => tab.adjust({ [s.key]: on })} />
				</div>
			{/each}
			<p class="adjust-note">Only how the pages look here. The file is not changed.</p>
			<!-- (To the adjustments Settings give every document: the pages as they are, unless
			     changed there.) -->
			<button type="button" class="button w-full" disabled={tab.ownAdjustments === null} onclick={reset}>Reset</button>
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
