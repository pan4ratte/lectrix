<script lang="ts">
	// The bar above the page grid while a document's pages are organized (section 6.7): in
	// the view bar's place and of its look. The buttons act on the selected pages; then pages
	// from a file and into a new document, and the page labels, each with its name beside
	// its icon; the size of the pictures; how many pages are selected; and at the end Done,
	// which goes back to reading.
	import { FileOutput, FilePlus, RotateCcw, RotateCw, Tag, Trash, ZoomIn, ZoomOut } from '@lucide/svelte';

	import { showLabels } from '#lib/features/labels/actions.ts';

	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { insertFromFile, leaveOrganize } from './actions.ts';
	import type { OrganizeState } from './organize.svelte.ts';

	let { tab, organize }: { tab: DocTab; organize: OrganizeState } = $props();

	const selected = $derived(organize.selected.length);
	const usable = $derived(organize.editable && selected > 0);
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
		class="bar-button"
		aria-label="Insert pages from file"
		data-tip={app.hint('Insert pages from a file, after the selected pages', 'insertPages')}
		disabled={!organize.editable}
		onclick={() => void insertFromFile(tab)}
	>
		<FilePlus size={16} aria-hidden="true" />Insert
	</button>
	<button
		type="button"
		class="bar-button"
		aria-label="Extract selected pages"
		data-tip="Extract the selected pages into a new document"
		disabled={selected === 0}
		onclick={() => organize.extract(organize.selectedSet)}
	>
		<FileOutput size={16} aria-hidden="true" />Extract
	</button>
	<button
		type="button"
		class="bar-button"
		aria-label="Page labels"
		data-tip={app.hint('Number the selected pages, or a range of pages', 'showLabels')}
		disabled={!tab.canEditLabels}
		onclick={() => void showLabels(tab)}
	>
		<Tag size={16} aria-hidden="true" />Labels
	</button>
	{@render rule()}
	<button
		type="button"
		class="icon-button"
		aria-label="Smaller pages"
		data-tip={app.hint('Smaller pages', 'zoomOut')}
		disabled={organize.zoom.smaller === null}
		onclick={() => organize.zoom.zoomStep(-1)}
	>
		<ZoomOut size={16} aria-hidden="true" />
	</button>
	<button
		type="button"
		class="icon-button"
		aria-label="Larger pages"
		data-tip={app.hint('Larger pages', 'zoomIn')}
		disabled={organize.zoom.larger === null}
		onclick={() => organize.zoom.zoomStep(1)}
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
	{@render rule()}
	<button
		type="button"
		class="button button-primary h-[28px]"
		data-tip="Back to reading (Esc)"
		onclick={() => leaveOrganize(tab)}
	>
		Done
	</button>
</div>
