<script lang="ts">
	// The bar of a clicked annotation (section 6.5): its colour, its type among the text
	// markup types, its note (or a text box's text), and delete. What the annotation or the
	// document doesn't allow is left out; a read-only annotation shows its properties.
	import {
		Highlighter,
		Info,
		MessageSquareText,
		Spline,
		Strikethrough,
		Trash,
		Type,
		Underline
	} from '@lucide/svelte';
	import type { Component } from 'svelte';

	import type { Annotation } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { openInspector, remove, update } from './actions.ts';
	import { barControls, type Area } from './bars.ts';
	import FloatingBar from './FloatingBar.svelte';
	import { MARKUP_SUBTYPES, PRESET_COLORS, typeName, type MarkupKind } from './tools.ts';

	interface Props {
		tab: DocTab;
		annotation: Annotation;
		anchor: Area;
		view: Area;
		contentWidth: number;
		/** Opens a text box's text for typing in place. */
		onedittext: () => void;
	}

	let { tab, annotation: a, anchor, view, contentWidth, onedittext }: Props = $props();

	const ICONS: Record<MarkupKind, Component<{ size?: number; 'aria-hidden'?: boolean | 'true' }>> = {
		highlight: Highlighter,
		underline: Underline,
		strikeOut: Strikethrough,
		squiggly: Spline
	};

	const controls = $derived(barControls(a, tab.flags.canAnnotate));
	const custom = $derived(a.color !== null && !PRESET_COLORS.some((c) => c.value === a.color));

	function setColor(color: string) {
		void update(tab, a.page, a.id, { color });
	}

	function setKind(kind: MarkupKind, subtype: string) {
		if (subtype !== a.subtype) void update(tab, a.page, a.id, { kind });
	}

	function del() {
		void remove(tab, a.page, a.id).then(() => tab.viewer?.focus());
	}
</script>

<FloatingBar label="{typeName(a.subtype)} actions" {anchor} {view} {contentWidth} prefer="above">
	{#if controls.restyle}
		<div
			class="flex items-center gap-0.5 px-0.5"
			role="radiogroup"
			aria-label={a.kind === 'freeText' ? 'Text colour' : 'Colour'}
		>
			{#each PRESET_COLORS as c (c.value)}
				<button
					type="button"
					class="swatch"
					role="radio"
					aria-checked={a.color === c.value}
					aria-label={c.name}
					title={c.name}
					style:--swatch={c.value}
					onclick={() => setColor(c.value)}
				></button>
			{/each}
			<label class="swatch swatch-custom" class:swatch-custom-on={custom} title="Custom colour">
				<span class="sr-only">Custom colour</span>
				<input
					type="color"
					class="sr-only"
					value={a.color ?? '#000000'}
					onchange={(e) => setColor(e.currentTarget.value)}
				/>
			</label>
		</div>
	{/if}

	{#if controls.retype}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
		{#each MARKUP_SUBTYPES as m (m.subtype)}
			{@const Icon = ICONS[m.kind]}
			<button
				type="button"
				class="icon-button tool-button"
				aria-pressed={a.subtype === m.subtype}
				aria-label={typeName(m.subtype)}
				title={typeName(m.subtype)}
				onclick={() => setKind(m.kind, m.subtype)}
			>
				<Icon size={18} aria-hidden="true" />
			</button>
		{/each}
	{/if}

	{#if controls.restyle || controls.retype}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
	{/if}
	{#if a.kind === 'freeText' && controls.text}
		<button type="button" class="icon-button" aria-label="Edit text" title="Edit text (double-click)" onclick={onedittext}>
			<Type size={18} aria-hidden="true" />
		</button>
	{:else if controls.text}
		<button
			type="button"
			class="icon-button"
			aria-label="Note"
			title="Note (double-click)"
			onclick={() => openInspector(true)}
		>
			<MessageSquareText size={18} aria-hidden="true" />
		</button>
	{:else}
		<button type="button" class="icon-button" aria-label="Properties" title="Properties" onclick={() => openInspector(false)}>
			<Info size={18} aria-hidden="true" />
		</button>
	{/if}
	{#if controls.delete}
		<button type="button" class="icon-button" aria-label="Delete" title="Delete (Del)" onclick={del}>
			<Trash size={18} aria-hidden="true" />
		</button>
	{/if}
</FloatingBar>
