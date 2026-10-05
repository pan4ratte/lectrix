<script lang="ts">
	// The annotation toolbar (sections 6.5 and 8): the tools, and the style the active tool
	// draws with (remembered per tool). Settings make it float over the page canvas, at the
	// bottom or the top, shown always or only while the pointer is near that edge (or the
	// toolbar has focus, or a tool was just picked); or make it a panel docked above the
	// pages, always shown (App.svelte places it).
	import {
		Highlighter,
		MousePointer2,
		PenLine,
		Spline,
		StickyNote,
		Strikethrough,
		Type,
		Underline
	} from '@lucide/svelte';
	import { onMount, type Component } from 'svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { refuseIfLocked } from './actions.ts';
	import { nearEdge } from './bars.ts';
	import { tools } from './state.svelte.ts';
	import { FONT_SIZES, PEN_WIDTHS, PRESET_COLORS, TOOLS, type Tool } from './tools.ts';

	let { tab }: { tab: DocTab } = $props();

	const ICONS: Record<Tool, Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>> = {
		select: MousePointer2,
		highlight: Highlighter,
		underline: Underline,
		strikeOut: Strikethrough,
		squiggly: Spline,
		note: StickyNote,
		ink: PenLine,
		freeText: Type
	};

	const OPACITIES = [1, 0.8, 0.6, 0.4, 0.2];

	const active = $derived(tools.tool);
	const drawTool = $derived(active === 'select' ? null : active);
	const style = $derived(drawTool ? tools.style(drawTool) : null);
	const locked = $derived(!tab.flags.canAnnotate);
	const custom = $derived(style !== null && !PRESET_COLORS.some((c) => c.value === style.color));

	function pick(tool: Tool) {
		if (tool !== 'select' && refuseIfLocked(tab)) return;
		tools.tool = tool;
		tab.viewer?.focus();
	}

	// ----- showing on demand -----

	/** How long a tool picked from the keyboard keeps the toolbar shown. */
	const FLASH_MS = 1500;
	/** How long the toolbar stays after the pointer leaves, so it doesn't flicker. */
	const LINGER_MS = 300;

	const panel = $derived(app.settings?.toolbarStyle === 'panel');
	const position = $derived(app.settings?.toolbarPosition ?? 'bottom');
	const onHover = $derived(!panel && app.settings?.toolbarVisibility === 'onHover');

	let bar: HTMLDivElement | undefined = $state();
	let near = $state(false);
	let focused = $state(false);
	let flash = $state(false);
	let shown = $state(true);
	const wanted = $derived(!onHover || near || focused || flash);

	let lingerTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		if (wanted) {
			clearTimeout(lingerTimer);
			lingerTimer = undefined;
			shown = true;
		} else if (shown && lingerTimer === undefined) {
			lingerTimer = setTimeout(() => {
				lingerTimer = undefined;
				shown = false;
			}, LINGER_MS);
		}
	});

	function onWindowPointerMove(event: PointerEvent) {
		const area = bar?.parentElement?.getBoundingClientRect();
		if (!onHover || !area) return;
		const inside = nearEdge(area, event.clientX, event.clientY, position);
		// A drag (selecting, drawing) reaching the edge doesn't bring the toolbar up.
		if (inside && !near && event.buttons !== 0) return;
		near = inside;
	}

	let flashTimer: ReturnType<typeof setTimeout> | undefined;
	let lastTool = tools.tool;
	$effect(() => {
		const tool = tools.tool;
		if (tool === lastTool) return;
		lastTool = tool;
		flash = true;
		clearTimeout(flashTimer);
		flashTimer = setTimeout(() => (flash = false), FLASH_MS);
	});

	onMount(() => {
		// The pointer leaving the window over the toolbar's edge.
		const leave = () => (near = false);
		document.documentElement.addEventListener('mouseleave', leave);
		return () => {
			document.documentElement.removeEventListener('mouseleave', leave);
			clearTimeout(lingerTimer);
			clearTimeout(flashTimer);
		};
	});
</script>

<svelte:window onpointermove={onWindowPointerMove} />

<div
	bind:this={bar}
	class={panel
		? 'flex h-10 shrink-0 items-center justify-center gap-0.5 border-b border-line bg-surface px-2'
		: 'annotation-toolbar absolute left-1/2 z-20 flex items-center gap-0.5 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]'}
	class:top-3={!panel && position === 'top'}
	class:bottom-4={!panel && position === 'bottom'}
	class:annotation-toolbar-top={!panel && position === 'top'}
	class:annotation-toolbar-hidden={!shown}
	role="toolbar"
	aria-label="Annotation tools"
	onfocusin={() => (focused = true)}
	onfocusout={(e) => (focused = bar?.contains(e.relatedTarget as Node | null) ?? false)}
>
	{#each TOOLS as t (t.id)}
		{@const Icon = ICONS[t.id]}
		<button
			type="button"
			class="icon-button tool-button"
			aria-pressed={active === t.id}
			aria-label={t.label}
			title={t.key ? `${t.label} (${t.key})` : t.label}
			disabled={t.id !== 'select' && locked}
			onclick={() => pick(t.id)}
		>
			<Icon size={18} aria-hidden="true" />
		</button>
	{/each}

	{#if style && drawTool}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
		<div class="flex items-center gap-0.5" role="radiogroup" aria-label="Colour">
			{#each PRESET_COLORS as c (c.value)}
				<button
					type="button"
					class="swatch"
					role="radio"
					aria-checked={style.color === c.value}
					aria-label={c.name}
					title={c.name}
					style:--swatch={c.value}
					onclick={() => tools.setStyle(drawTool, { color: c.value })}
				></button>
			{/each}
			<label class="swatch swatch-custom" class:swatch-custom-on={custom} title="Custom colour">
				<span class="sr-only">Custom colour</span>
				<input
					type="color"
					class="sr-only"
					value={style.color}
					onchange={(e) => tools.setStyle(drawTool, { color: e.currentTarget.value })}
				/>
			</label>
		</div>
		{#if drawTool === 'ink'}
			<select
				class="toolbar-select"
				aria-label="Stroke width"
				title="Stroke width"
				value={style.width}
				onchange={(e) => tools.setStyle('ink', { width: Number(e.currentTarget.value) })}
			>
				{#each PEN_WIDTHS as w (w)}
					<option value={w}>{w} pt</option>
				{/each}
			</select>
		{/if}
		{#if drawTool === 'freeText'}
			<select
				class="toolbar-select"
				aria-label="Font size"
				title="Font size"
				value={style.fontSize}
				onchange={(e) => tools.setStyle('freeText', { fontSize: Number(e.currentTarget.value) })}
			>
				{#each FONT_SIZES as s (s)}
					<option value={s}>{s} pt</option>
				{/each}
			</select>
		{/if}
		<select
			class="toolbar-select"
			aria-label="Opacity"
			title="Opacity"
			value={style.opacity}
			onchange={(e) => tools.setStyle(drawTool, { opacity: Number(e.currentTarget.value) })}
		>
			{#each OPACITIES as o (o)}
				<option value={o}>{Math.round(o * 100)}%</option>
			{/each}
		</select>
	{/if}
</div>
