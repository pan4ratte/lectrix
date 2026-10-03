<script lang="ts">
	// The floating annotation toolbar (sections 6.5 and 8): the tools, and the style the
	// active tool draws with (remembered per tool).
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
	import type { Component } from 'svelte';

	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { refuseIfLocked } from './actions.ts';
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
</script>

<div
	class="absolute bottom-4 left-1/2 z-10 flex -translate-x-1/2 items-center gap-0.5 rounded-panel border border-line bg-surface-raised p-1 shadow-[0_4px_12px_var(--color-page-shadow)]"
	role="toolbar"
	aria-label="Annotation tools"
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
