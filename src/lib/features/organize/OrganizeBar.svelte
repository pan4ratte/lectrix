<script lang="ts">
	// The bar above the page grid while a document's pages are organized (section 6.7): in
	// the view bar's place and of its look. Done goes back to reading; the buttons act on the
	// selected pages; then the size of the pictures, and how many pages are selected.
	import { RotateCcw, RotateCw, Trash, ZoomIn, ZoomOut } from '@lucide/svelte';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { leaveOrganize } from './actions.ts';
	import type { OrganizeState } from './organize.svelte.ts';
	import { stepThumb } from './pages.ts';

	let { tab, organize }: { tab: DocTab; organize: OrganizeState } = $props();

	const selected = $derived(organize.selected.length);
	const usable = $derived(organize.editable && selected > 0);
	const smaller = $derived(stepThumb(organize.thumbTarget, -1));
	const larger = $derived(stepThumb(organize.thumbTarget, 1));
	const pages = $derived(tab.pageCount === 1 ? '1 page' : `${tab.pageCount.toLocaleString()} pages`);
</script>

{#snippet rule()}
	<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
{/snippet}

<div
	class="flex min-h-[40px] shrink-0 flex-wrap items-center justify-center gap-x-1 gap-y-1 border-b border-line bg-chrome px-2 py-1 text-sm text-fg-muted"
	role="toolbar"
	aria-label="Organize pages"
	data-organize-bar
>
	<button
		type="button"
		class="button button-primary h-[28px]"
		data-tip="Back to reading (Esc)"
		onclick={() => leaveOrganize(tab)}
	>
		Done
	</button>
	{@render rule()}
	<button
		type="button"
		class="icon-button"
		aria-label="Rotate selected pages counter-clockwise"
		data-tip="Rotate counter-clockwise (Shift+R)"
		disabled={!usable}
		onclick={() => organize.rotate(organize.selectedSet, -90)}
	>
		<RotateCcw size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Rotate selected pages clockwise"
		data-tip="Rotate clockwise (R)"
		disabled={!usable}
		onclick={() => organize.rotate(organize.selectedSet, 90)}
	>
		<RotateCw size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Delete selected pages"
		data-tip="Delete (Del)"
		disabled={!usable}
		onclick={() => organize.remove(organize.selectedSet)}
	>
		<Trash size={16} aria-hidden="true" />
	</button>
	{@render rule()}
	<button
		type="button"
		class="icon-button"
		aria-label="Smaller pages"
		data-tip={app.hint('Smaller pages', 'zoomOut')}
		disabled={smaller === null}
		onclick={() => organize.zoomStep(-1)}
	>
		<ZoomOut size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Larger pages"
		data-tip={app.hint('Larger pages', 'zoomIn')}
		disabled={larger === null}
		onclick={() => organize.zoomStep(1)}
	>
		<ZoomIn size={16} aria-hidden="true" />
	</button>
	{@render rule()}
	<span class="px-1 tabular-nums" aria-live="polite" data-organize-count>
		{#if selected > 0}{selected.toLocaleString()} of {pages} selected{:else}{pages}{/if}
	</span>
	<!-- Open for reading only (PDF/A), the banner under this bar says why and how to change it. -->
	{#if !organize.editable && !tab.readOnly}
		{@render rule()}
		<span role="note">This document’s security settings don’t allow changing its pages.</span>
	{/if}
</div>
