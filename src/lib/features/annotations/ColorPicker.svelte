<script lang="ts">
	// The colour control of the annotation toolbar, an annotation's bar, its comment panel and
	// its row in the annotation list (section 6.5): one button showing the colour in use, which
	// opens a panel with the 18 presets (Acrobat's, or the user's own from Settings; a grid the
	// arrow keys move in, one Tab stop), then the opacity slider with the custom colour's
	// button at its end. A colour applies when picked and the panel stays open for the
	// opacity, which applies when the slider is let go. The custom colour's button slides
	// Lectrix's own picker (HexPicker) open below, open from the start when the colour in use
	// is not a preset.
	//
	// The panel opens when the pointer rests on the button (Settings can turn that off), and
	// then goes when the pointer leaves the button and the panel. Once something in it has
	// been pressed or focused, or the button clicked, it stays as a panel opened by a click or
	// a key does: until Esc or a click elsewhere (Bits UI's popover keeps track of this).
	import { ChevronDown } from '@lucide/svelte';
	import { Popover } from 'bits-ui';
	import { slide } from 'svelte/transition';

	import { motionMs } from '#lib/components/panes.ts';
	import Slider from '#lib/components/Slider.svelte';
	import { app } from '#lib/stores/app.svelte.ts';

	import { menuOffset, type MenuSide } from './bars.ts';
	import HexPicker from './HexPicker.svelte';
	import { tools } from './state.svelte.ts';
	import { PRESET_COLUMNS } from './tools.ts';

	interface Props {
		/** #rrggbb, or null for an annotation without a colour. */
		color: string | null;
		/** 0.1 to 1. */
		opacity: number;
		label: string;
		oncolor: (color: string) => void;
		onopacity: (opacity: number) => void;
		/** Where the panel opens: below the button, or beside it (a stacked toolbar, where
		 * the button is the colour alone, as wide as a tool). */
		side?: MenuSide;
	}

	let { color, opacity, label, oncolor, onopacity, side = 'bottom' }: Props = $props();

	/** The slider's low end: below it, a mark is hard to find again. */
	const MIN_OPACITY = 0.1;
	/** How long the pointer rests on the button before the panel opens, and how long after
	 * it has left the button and the panel they close. */
	const HOVER_OPEN_MS = 150;
	const HOVER_CLOSE_MS = 300;

	const presets = $derived(tools.presets);
	const preset = $derived(presets.find((c) => c.value === color));
	const custom = $derived(color !== null && !preset);
	const name = $derived(preset?.name ?? (color === null ? 'none' : 'custom'));
	const onHover = $derived(app.settings?.colorPanelOnHover !== false);

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
	/** The panel took the focus as it opened (a click or a key opened it, not the pointer). */
	let tookFocus = false;

	$effect(() => {
		if (open) value = opacity;
	});

	function setOpen(next: boolean) {
		// Before the panel is there, so the picker of a custom colour doesn't slide open.
		if (next) expanded = custom;
		open = next;
	}

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
	const tabStop = $derived(Math.max(0, presets.findIndex((c) => c.value === color)));

	/** Arrows move among the swatches, a row at a time up and down; Enter or Space picks. */
	function onSwatchKey(event: KeyboardEvent, index: number) {
		const last = presets.length - 1;
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

	/** Opened by a click or a key, the panel takes the focus, on the colour in use, so arrows
	 * move from it. Opened by the pointer, it leaves the focus where it is (the popover has
	 * said so already). */
	function focusChosen(event: Event) {
		tookFocus = !event.defaultPrevented;
		event.preventDefault();
		if (!tookFocus) return;
		const chosen =
			content?.querySelector<HTMLElement>('.swatch[aria-checked="true"], .swatch-custom-on') ?? content?.querySelector<HTMLElement>('.swatch');
		chosen?.focus();
	}

	/** Back to the button, unless the focus went somewhere else on purpose, or never came
	 * to a panel the pointer opened. */
	function restoreFocus(event: Event) {
		event.preventDefault();
		const active = document.activeElement;
		const stranded = active === null || active === document.body;
		if (content?.contains(active) || (stranded && tookFocus)) trigger?.focus();
	}
</script>

<Popover.Root bind:open={() => open, setOpen}>
	<Popover.Trigger
		bind:ref={trigger}
		class={['color-button', side !== 'bottom' && 'color-button-stacked']}
		aria-label="{label}: {name}, opacity {Math.round(opacity * 100)}%"
		data-tip={open ? undefined : label}
		data-color-button
		openOnHover={onHover}
		openDelay={HOVER_OPEN_MS}
		closeDelay={HOVER_CLOSE_MS}
	>
		<span class="swatch-dot" style:--swatch={color ?? 'transparent'} aria-hidden="true"></span>
		{#if side === 'bottom'}
			<ChevronDown size={12} aria-hidden="true" />
		{/if}
	</Popover.Trigger>
	<Popover.Portal>
		<Popover.Content
			bind:ref={content}
			{side}
			align={side === 'bottom' ? 'center' : 'start'}
			sideOffset={menuOffset(side, 6)}
			escapeKeydownBehavior="ignore"
			class="color-panel"
			aria-label={label}
			onkeydown={onContentKey}
			onOpenAutoFocus={focusChosen}
			onCloseAutoFocus={restoreFocus}
		>
			<div class="swatch-grid" role="radiogroup" aria-label={label}>
				{#each presets as c, i (i)}
					<button
						type="button"
						class="swatch"
						role="radio"
						aria-checked={color === c.value}
						aria-label={c.name}
						data-tip={c.name}
						tabindex={i === tabStop ? 0 : -1}
						style:--swatch={c.value}
						onclick={() => oncolor(c.value)}
						onkeydown={(e) => onSwatchKey(e, i)}
					></button>
				{/each}
			</div>
			<div class="mt-2 flex items-center gap-2">
				<span class="flex min-w-0 flex-1" data-tip="Opacity">
					<Slider min={MIN_OPACITY} max={1} step={0.05} label="Opacity" valuetext={percent} bind:value onchange={apply} />
				</span>
				<span class="w-8 shrink-0 text-right text-xs tabular-nums" aria-hidden="true">{percent}</span>
				<button
					type="button"
					class="swatch swatch-custom"
					class:swatch-custom-on={custom}
					aria-label={custom ? `Custom colour, ${color}` : 'Custom colour'}
					aria-expanded={expanded}
					aria-controls={pickerId}
					data-tip="Custom colour"
					onclick={() => (expanded = !expanded)}
				></button>
			</div>
			{#if expanded}
				<div id={pickerId} transition:slide={{ duration: motionMs(140) }}>
					<div class="mt-2 border-t border-menu-line pt-2">
						<HexPicker {color} onpick={oncolor} />
					</div>
				</div>
			{/if}
		</Popover.Content>
	</Popover.Portal>
</Popover.Root>
