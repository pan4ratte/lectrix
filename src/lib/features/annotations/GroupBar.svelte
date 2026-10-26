<script lang="ts">
	// The bar of annotations selected together (section 6.5): how many there are, one colour
	// and opacity for all of them whose colour can change, and delete. Each change is one undo
	// step.
	import { Trash } from '@lucide/svelte';

	import type { Annotation } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { removeMany, restyleMany } from './actions.ts';
	import type { Area } from './bars.ts';
	import ColorPicker from './ColorPicker.svelte';
	import FloatingBar from './FloatingBar.svelte';
	import { capabilities } from './tools.ts';

	interface Props {
		tab: DocTab;
		group: readonly Annotation[];
		anchor: Area;
		view: Area;
		contentWidth: number;
	}

	let { tab, group, anchor, view, contentWidth }: Props = $props();

	const styled = $derived(group.filter((a) => capabilities(a, tab.flags.canAnnotate).restyle));
	const deletable = $derived(group.some((a) => capabilities(a, tab.flags.canAnnotate).delete));
	/** The colour and opacity shown: the first one's, which a change gives them all. */
	const first = $derived(styled[0] ?? null);

	function del() {
		void removeMany(tab, tab.selectedAnnotations).then(() => tab.viewer?.focus());
	}
</script>

<FloatingBar label="{group.length} annotations actions" {anchor} {view} {contentWidth} prefer="above">
	<span class="px-2 text-sm text-fg-muted" data-group-count>{group.length} selected</span>
	{#if first}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
		<ColorPicker
			color={first.color}
			opacity={first.opacity}
			label="Colour"
			oncolor={(color) => void restyleMany(tab, styled, { color })}
			onopacity={(opacity) => void restyleMany(tab, styled, { opacity })}
		/>
	{/if}
	{#if deletable}
		<span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
		<button type="button" class="icon-button" aria-label="Delete" data-tip="Delete (Del)" onclick={del}>
			<Trash size={18} aria-hidden="true" />
		</button>
	{/if}
</FloatingBar>
