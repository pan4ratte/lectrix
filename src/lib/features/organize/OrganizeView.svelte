<script lang="ts">
	// A document's pages as a grid to organize (section 6.7), in the viewer's place: the
	// shared page grid, with each page's label under it. Under the pointer, the label has
	// the buttons that turn or delete its page after it, in a bar in the label's place.
	// Selecting, dragging and the keys are the grid's (`PageGrid.svelte`); what they do to
	// the document is the state's (`organize.svelte.ts`). The pictures show the pages as the
	// file has them: without the document's visual adjustments, as the Combine view does.
	import { RotateCcw, RotateCw, Trash } from '@lucide/svelte';
	import { onMount } from 'svelte';

	import PageGrid from '#lib/components/PageGrid.svelte';
	import type { GridPicture } from '#lib/components/pagegrid.ts';
	import { pageUrl } from '#lib/ipc/index.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { leaveOrganize } from './actions.ts';
	import type { GridPage, OrganizeState } from './organize.svelte.ts';
	import { caption as captionOf, describePage } from './pages.ts';

	let { tab, organize }: { tab: DocTab; organize: OrganizeState } = $props();

	let host: HTMLDivElement | undefined = $state();

	function picture(page: GridPage): GridPicture | null {
		const size = tab.pages[page.index];
		if (!size) return null;
		const revision = tab.state.revision;
		return {
			width: size.width,
			height: size.height,
			turn: 0,
			rotation: size.rotation,
			url: (scale) => pageUrl(tab.id, page.index, scale, revision),
			imageKey: (scale) => `${tab.id}:${page.index}:${revision}:${scale}`
		};
	}

	// The keys are the grid's from the start.
	onMount(() => host?.querySelector<HTMLElement>('[role=listbox]')?.focus({ preventScroll: true }));
</script>

<div bind:this={host} class="flex min-h-0 min-w-0 flex-1 flex-col" data-organize-view>
	<PageGrid
		items={organize.items}
		model={organize}
		{picture}
		describe={(page) => describePage(page.index, tab.pageCount, tab.displayLabels)}
		label="Pages of {tab.name}"
		idPrefix="organize-page"
		thumb={organize.thumb}
		renderThumb={organize.renderThumb}
		onzoom={(factor, notch) => (notch ? organize.zoomNotch(factor) : organize.zoomPinch(factor))}
		toolsAt="caption"
		captionHeight={32}
		editable={organize.editable}
		reveal={organize.reveal}
		onopen={(page) => leaveOrganize(tab, page.index)}
	>
		{#snippet tools(page: GridPage)}
			<!-- (At most a little wider than the cell, so the bar never reaches out of the grid.) -->
			<span class="truncate px-1.5 text-xs tabular-nums" style:max-width="{organize.thumb - 40}px" aria-hidden="true">
				{captionOf(page.index, tab.displayLabels)}
			</span>
			<button
				type="button"
				class="icon-button icon-button-small"
				tabindex="-1"
				aria-label="Rotate counter-clockwise"
				data-tip="Rotate counter-clockwise (Shift+R)"
				onclick={() => organize.rotate(new Set([page.key]), -90)}
			>
				<RotateCcw size={14} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button icon-button-small"
				tabindex="-1"
				aria-label="Rotate clockwise"
				data-tip="Rotate clockwise (R)"
				onclick={() => organize.rotate(new Set([page.key]), 90)}
			>
				<RotateCw size={14} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button icon-button-small"
				tabindex="-1"
				aria-label="Delete"
				data-tip="Delete (Del)"
				onclick={() => organize.remove(new Set([page.key]))}
			>
				<Trash size={14} aria-hidden="true" />
			</button>
		{/snippet}
		{#snippet caption(page: GridPage)}
			<!-- Under the pointer its bar shows in its place, with the same words. -->
			<span class={['mt-1.5 max-w-full truncate px-1 text-xs tabular-nums', organize.editable && 'group-hover:invisible']}>
				{captionOf(page.index, tab.displayLabels)}
			</span>
		{/snippet}
	</PageGrid>
</div>
