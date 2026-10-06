<script lang="ts">
	// The colour control of the annotation toolbar, an annotation's bar and its comment panel
	// (section 6.5), as Acrobat has it: one button showing the colour in use, which opens a
	// panel with Acrobat's 18 presets (a grid the arrow keys move in, one Tab stop), a custom
	// colour and the opacity slider. A colour applies
	// when picked and the panel stays open for the opacity, which applies when the slider is
	// let go. The custom colour's button opens Lectrix's own picker in the panel (HexPicker),
	// open from the start when the colour in use is not a preset. Esc or a click elsewhere
	// closes the panel.
	import { ChevronDown } from '@lucide/svelte';
	import { Popover } from 'bits-ui';
	import { untrack } from 'svelte';

	import HexPicker from './HexPicker.svelte';
	import { PRESET_COLORS, PRESET_COLUMNS } from './tools.ts';

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
	/** The custom colour's picker is shown. */
	let expanded = $state(false);
	const uid = $props.id();
	const pickerId = `${uid}-custom`;
	let trigger = $state<HTMLElement | null>(null);
	let content = $state<HTMLElement | null>(null);
	/** The slider's value while it moves, before it is applied. */
	let value = $state(1);
	const percent = $derived(`${Math.round(value * 100)}%`);

	$effect(() => {
		if (open) value = opacity;
	});

	$effect(() => {
		if (open) expanded = untrack(() => custom);
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

	/** The swatch Tab reaches: the colour in use, else the first. */
	const tabStop = $derived(Math.max(0, PRESET_COLORS.findIndex((c) => c.value === color)));

	/** Arrows move among the swatches, a row at a time up and down; Enter or Space picks. */
	function onSwatchKey(event: KeyboardEvent, index: number) {
		const last = PRESET_COLORS.length - 1;
		const steps: Record<string, number> = {
			ArrowRight: index + 1,
			ArrowLeft: index - 1,
			ArrowDown: index + PRESET_COLUMNS,
			ArrowUp: index - PRESET_COLUMNS,
			Home: 0,
			End: last
		};
		const next = steps[event.key];
		if (next === undefined) return;
		event.preventDefault();
		if (next < 0 || next > last) return;
		content?.querySelectorAll<HTMLElement>('[role=radiogroup] .swatch')[next]?.focus();
	}

	/** Opens on the colour in use, so arrows move from it. */
	function focusChosen(event: Event) {
		event.preventDefault();
		const chosen =
			content?.querySelector<HTMLElement>('.swatch[aria-checked="true"], .swatch-custom-on') ?? content?.querySelector<HTMLElement>('.swatch');
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
			<div class="flex items-start justify-between">
				<div class="swatch-grid" role="radiogroup" aria-label={label}>
					{#each PRESET_COLORS as c, i (c.value)}
						<button
							type="button"
							class="swatch"
							role="radio"
							aria-checked={color === c.value}
							aria-label={c.name}
							title={c.name}
							tabindex={i === tabStop ? 0 : -1}
							style:--swatch={c.value}
							onclick={() => oncolor(c.value)}
							onkeydown={(e) => onSwatchKey(e, i)}
						></button>
					{/each}
				</div>
				<button
					type="button"
					class="swatch swatch-custom"
					class:swatch-custom-on={custom}
					aria-label={custom ? `Custom colour, ${color}` : 'Custom colour'}
					aria-expanded={expanded}
					aria-controls={pickerId}
					title="Custom colour"
					onclick={() => (expanded = !expanded)}
				></button>
			</div>
			{#if expanded}
				<div id={pickerId}>
					<HexPicker {color} onpick={oncolor} />
				</div>
			{/if}
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
