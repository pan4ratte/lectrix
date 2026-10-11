<script lang="ts">
	// The controls of the selected annotation (section 6.5), in the header of its comment
	// panel and, while the annotation list shows, in its row there: its colour and opacity (one
	// button that opens them), stroke width or font size, its type among the text markup
	// types, and at the right end its properties (of an annotation whose note can't change)
	// and delete. What the annotation or the document doesn't allow is left out.
	import { Info, Trash } from '@lucide/svelte';

	import Dropdown from '#lib/components/Dropdown.svelte';
	import type { Annotation } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { openProperties, remove, restyle, update } from './actions.ts';
	import { barControls } from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import { typeIcon } from './icons.ts';
	import { FONT_SIZES, MARKUP_SUBTYPES, PEN_WIDTHS, ptOptions, typeName, type MarkupKind } from './tools.ts';
	import { toolMark } from './toolMark.ts';

	interface Props {
		tab: DocTab;
		annotation: Annotation;
		/** Where the focus goes once Delete is done with. */
		onremoved: () => void;
	}

	let { tab, annotation: a, onremoved }: Props = $props();

	const caps = $derived(barControls(a, tab.flags.canAnnotate));

	function setKind(kind: MarkupKind, subtype: string) {
		if (subtype !== a.subtype) void update(tab, a.page, a.id, { kind });
	}

	function del() {
		void remove(tab, a.page, a.id).then(onremoved);
	}
</script>

<div class="flex flex-wrap items-center gap-1">
	{#if caps.restyle}
		<ColorPicker
			color={a.color}
			opacity={a.opacity}
			label={a.kind === 'freeText' ? 'Text colour' : 'Colour'}
			oncolor={(color) => void restyle(tab, a, { color })}
			onopacity={(opacity) => void restyle(tab, a, { opacity })}
		/>
	{/if}
	{#if caps.restyle && a.kind === 'ink'}
		<Dropdown
			class="toolbar-dropdown"
			label="Stroke width"
			tip="Stroke width"
			value={a.width ?? 1}
			options={ptOptions([...PEN_WIDTHS, a.width ?? 1])}
			onchange={(width) => void update(tab, a.page, a.id, { width })}
		/>
	{/if}
	{#if caps.restyle && a.kind === 'freeText'}
		<Dropdown
			class="toolbar-dropdown"
			label="Font size"
			tip="Font size"
			value={a.fontSize ?? 12}
			options={ptOptions([...FONT_SIZES, a.fontSize ?? 12])}
			onchange={(fontSize) => void update(tab, a.page, a.id, { fontSize })}
		/>
	{/if}
	{#if caps.retype}
		{#if caps.restyle}<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>{/if}
		<span class="tool-group flex items-center gap-1" {@attach toolMark}>
			<span class="tool-mark" aria-hidden="true" hidden></span>
			{#each MARKUP_SUBTYPES as m (m.subtype)}
				{@const Icon = typeIcon(m.subtype)}
				<button
					type="button"
					class="icon-button tool-button"
					aria-pressed={a.subtype === m.subtype}
					aria-label={typeName(m.subtype)}
					data-tip={typeName(m.subtype)}
					onclick={() => setKind(m.kind, m.subtype)}
				>
					<Icon size={18} aria-hidden="true" />
				</button>
			{/each}
		</span>
	{/if}
	<span class="ml-auto flex items-center gap-1">
		{#if !caps.text}
			<button type="button" class="icon-button" aria-label="Properties" data-tip="Properties" onclick={openProperties}>
				<Info size={18} aria-hidden="true" />
			</button>
		{/if}
		{#if caps.delete}
			<button type="button" class="icon-button" aria-label="Delete" data-tip="Delete (Del)" onclick={del}>
				<Trash size={18} aria-hidden="true" />
			</button>
		{/if}
	</span>
</div>
