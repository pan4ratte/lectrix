<script lang="ts">
	// The bar of a clicked annotation without a comment (section 6.5): its colour and opacity
	// (one button that opens them), its type among the text markup types, its note (or a text
	// box's text), and delete. What the annotation or the document doesn't allow is left out; a
	// read-only annotation shows its properties. An annotation that shows its comment panel has
	// these controls in the panel's header instead.
	import { Info, MessageSquareText, Trash, Type } from '@lucide/svelte';

	import type { Annotation } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { openInspector, openProperties, remove, restyle, update } from './actions.ts';
	import { barControls, type Area } from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import FloatingBar from './FloatingBar.svelte';
	import { typeIcon } from './icons.ts';
	import { MARKUP_SUBTYPES, typeName, type MarkupKind } from './tools.ts';

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

	const controls = $derived(barControls(a, tab.flags.canAnnotate));

	function setKind(kind: MarkupKind, subtype: string) {
		if (subtype !== a.subtype) void update(tab, a.page, a.id, { kind });
	}

	function del() {
		void remove(tab, a.page, a.id).then(() => tab.viewer?.focus());
	}
</script>

<FloatingBar label="{typeName(a.subtype)} actions" {anchor} {view} {contentWidth} prefer="above">
	{#if controls.restyle}
		<ColorPicker
			color={a.color}
			opacity={a.opacity}
			label={a.kind === 'freeText' ? 'Text colour' : 'Colour'}
			oncolor={(color) => void restyle(tab, a, { color })}
			onopacity={(opacity) => void restyle(tab, a, { opacity })}
		/>
	{/if}

	{#if controls.retype}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
		{#each MARKUP_SUBTYPES as m (m.subtype)}
			{@const Icon = typeIcon(m.subtype)}
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
			onclick={() => openInspector(tab, true)}
		>
			<MessageSquareText size={18} aria-hidden="true" />
		</button>
	{:else}
		<button type="button" class="icon-button" aria-label="Properties" title="Properties" onclick={openProperties}>
			<Info size={18} aria-hidden="true" />
		</button>
	{/if}
	{#if controls.delete}
		<button type="button" class="icon-button" aria-label="Delete" title="Delete (Del)" onclick={del}>
			<Trash size={18} aria-hidden="true" />
		</button>
	{/if}
</FloatingBar>
