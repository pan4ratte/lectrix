<script lang="ts">
	// The colour control of the annotation toolbar, an annotation's bar and its comment panel
	// (section 6.5), as Acrobat has it: one button showing the colour in use, which opens a
	// panel with the six presets, a custom colour and the opacity slider. A colour applies
	// when picked and the panel stays open for the opacity, which applies when the slider is
	// let go. Esc or a click elsewhere closes it.
	import { ChevronDown } from '@lucide/svelte';
	import { Popover } from 'bits-ui';

	import { PRESET_COLORS } from './tools.ts';

	interface Props {
		/** #rrggbb, or null for an annotation without a colour. */
		color: string | null;
		/** 0.1 to 1. */
		opacity: number;
		label: string;
		oncolor: (color: string) => void;
		onopacity: (opacity: number) => void;
	}

	let { color, opacity, label, oncolor, onopacity }: Props = $props();

	/** The slider's low end: below it, a mark is hard to find again. */
	const MIN_OPACITY = 0.1;

	const preset = $derived(PRESET_COLORS.find((c) => c.value === color));
	const custom = $derived(color !== null && !preset);
	const name = $derived(preset?.name ?? (color === null ? 'none' : 'custom'));

	let open = $state(false);
	let trigger = $state<HTMLElement | null>(null);
	let content = $state<HTMLElement | null>(null);
	/** The slider's value while it moves, before it is applied. */
	let value = $state(1);
	const percent = $derived(`${Math.round(value * 100)}%`);

	$effect(() => {
		if (open) value = opacity;
	});

	function apply() {
		if (Math.abs(value - opacity) > 1e-3) onopacity(value);
	}

	/** Esc closes the panel only: it must not also pick the Select tool or deselect. */
	function onContentKey(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		open = false;
	}

	/** Opens on the colour in use, so arrows move from it. */
	function focusChosen(event: Event) {
		event.preventDefault();
		const chosen = content?.querySelector<HTMLElement>('.swatch[aria-checked="true"]') ?? content?.querySelector<HTMLElement>('.swatch');
		chosen?.focus();
	}

	/** Back to the button, unless the focus went somewhere else on purpose. */
	function restoreFocus(event: Event) {
		event.preventDefault();
		const active = document.activeElement;
		if (active === null || active === document.body || content?.contains(active)) trigger?.focus();
	}
</script>

<Popover.Root bind:open>
	<Popover.Trigger
		bind:ref={trigger}
		class="color-button"
		aria-label="{label}: {name}, opacity {Math.round(opacity * 100)}%"
		title={label}
		data-color-button
	>
		<span class="swatch-dot" style:--swatch={color ?? 'transparent'} aria-hidden="true"></span>
		<ChevronDown size={12} aria-hidden="true" />
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content
			bind:ref={content}
			side="bottom"
			sideOffset={6}
			escapeKeydownBehavior="ignore"
			class="color-panel"
			aria-label={label}
			onkeydown={onContentKey}
			onOpenAutoFocus={focusChosen}
			onCloseAutoFocus={restoreFocus}
		>
			<div class="flex items-center justify-between" role="radiogroup" aria-label={label}>
				{#each PRESET_COLORS as c (c.value)}
					<button
						type="button"
						class="swatch"
						role="radio"
						aria-checked={color === c.value}
						aria-label={c.name}
						title={c.name}
						style:--swatch={c.value}
						onclick={() => oncolor(c.value)}
					></button>
				{/each}
				<label class="swatch swatch-custom" class:swatch-custom-on={custom} title="Custom colour">
					<span class="sr-only">Custom colour</span>
					<input type="color" class="sr-only" value={color ?? '#000000'} onchange={(e) => oncolor(e.currentTarget.value)} />
				</label>
			</div>
			<label class="mt-2 flex items-center gap-2">
				<span class="text-xs text-fg-muted">Opacity</span>
				<input
					type="range"
					class="settings-slider"
					min={MIN_OPACITY}
					max="1"
					step="0.05"
					aria-label="Opacity"
					aria-valuetext={percent}
					bind:value
					onchange={apply}
				/>
				<span class="w-9 shrink-0 text-right text-xs tabular-nums" aria-hidden="true">{percent}</span>
			</label>
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
