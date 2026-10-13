<script lang="ts">
	// A document's pages as a grid to organize (section 6.7), in the viewer's place: the
	// shared page grid, with each page's label under it. Under the pointer, the label has
	// the buttons that turn or delete its page after it, in a bar in the label's place.
	// A right click (or Shift+F10) offers what can be done with the selected pages. A file
	// dragged in from another program shows the gap its pages would go into.
	// Selecting, dragging and the keys are the grid's (`PageGrid.svelte`); what they do to
	// the document is the state's (`organize.svelte.ts`). The pictures show the pages as the
	// file has them: without the document's visual adjustments, as the Combine view does.
	import { RotateCcw, RotateCw, Trash } from '@lucide/svelte';
	import { ContextMenu } from 'bits-ui';
	import { onMount } from 'svelte';

	import PageGrid from '#lib/components/PageGrid.svelte';
	import type { GridPicture } from '#lib/components/pagegrid.ts';
	import CopyProgress from '#lib/features/merge/CopyProgress.svelte';
	import { pageUrl } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';
	import type { DocTab } from '#lib/stores/doc.svelte.ts';

	import { insertFromFile, leaveOrganize } from './actions.ts';
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

	// The menu is for the selected pages (a right click on another page selects it first).
	const picked = $derived(organize.indexes(organize.selected));
	const focused = $derived(organize.focusKey === null ? [] : organize.indexes([organize.focusKey]));
	const many = $derived(picked.length > 1);

	// The keys are the grid's from the start.
	onMount(() => host?.querySelector<HTMLElement>('[role=listbox]')?.focus({ preventScroll: true }));
</script>

<ContextMenu.Root>
	<ContextMenu.Trigger>
		{#snippet child({ props })}
			<div {...props} bind:this={host} class="relative flex min-h-0 min-w-0 flex-1 flex-col" data-organize-view>
				{#if organize.filePoint}
					<div
						class="pointer-events-none absolute top-3 left-1/2 z-20 -translate-x-1/2 rounded-control border border-line bg-surface-raised px-3 py-1.5 text-sm"
						role="status"
					>
						{organize.editable ? 'Drop a PDF between two pages to insert its pages there' : 'This document’s pages can’t be changed'}
					</div>
				{/if}
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
					captionHeight={32}
					editable={organize.editable}
					reveal={organize.reveal}
					onopen={(page) => leaveOrganize(tab, page.index)}
					dropPoint={organize.filePoint}
					ondropgap={(gap) => (organize.fileGap = gap)}
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
						<span
							class={[
								'h-[24px] max-w-full shrink-0 truncate px-1 text-xs leading-[24px] tabular-nums',
								organize.editable && 'group-hover:invisible'
							]}
						>
							{captionOf(page.index, tab.displayLabels)}
						</span>
					{/snippet}
				</PageGrid>
			</div>
		{/snippet}
	</ContextMenu.Trigger>
	<ContextMenu.Portal>
		<ContextMenu.Content class="menu-content">
			<ContextMenu.Item
				class="menu-item"
				disabled={!organize.editable || !picked.length}
				onSelect={() => organize.rotate(organize.selectedSet, 90)}
			>
				Rotate clockwise<span class="menu-shortcut">R</span>
			</ContextMenu.Item>
			<ContextMenu.Item
				class="menu-item"
				disabled={!organize.editable || !picked.length}
				onSelect={() => organize.rotate(organize.selectedSet, -90)}
			>
				Rotate counter-clockwise<span class="menu-shortcut">Shift+R</span>
			</ContextMenu.Item>
			<ContextMenu.Item
				class="menu-item"
				disabled={!organize.editable || !picked.length}
				onSelect={() => organize.remove(organize.selectedSet)}
			>
				{many ? `Delete ${picked.length.toLocaleString()} pages…` : 'Delete page…'}<span class="menu-shortcut">Del</span>
			</ContextMenu.Item>
			<ContextMenu.Separator class="menu-separator" />
			<ContextMenu.Item class="menu-item" disabled={!picked.length} onSelect={() => organize.extract(organize.selectedSet)}>
				{many ? `Extract ${picked.length.toLocaleString()} pages into a new file…` : 'Extract page into a new file…'}
			</ContextMenu.Item>
			<ContextMenu.Item
				class="menu-item"
				disabled={!organize.editable || !picked.length}
				onSelect={() => void insertFromFile(tab, picked[0])}
			>
				Insert pages from file before…
			</ContextMenu.Item>
			<ContextMenu.Item
				class="menu-item"
				disabled={!organize.editable}
				onSelect={() => void insertFromFile(tab, picked.length ? picked[picked.length - 1]! + 1 : undefined)}
			>
				Insert pages from file after…{#if app.shortcut('insertPages')}<span class="menu-shortcut">{app.shortcut('insertPages')}</span>{/if}
			</ContextMenu.Item>
			<ContextMenu.Separator class="menu-separator" />
			<ContextMenu.Item
				class="menu-item"
				disabled={focused.length === 0}
				onSelect={() => focused[0] !== undefined && leaveOrganize(tab, focused[0])}
			>
				Read from this page<span class="menu-shortcut">Enter</span>
			</ContextMenu.Item>
		</ContextMenu.Content>
	</ContextMenu.Portal>
</ContextMenu.Root>

<CopyProgress
	heading="Extracting pages"
	progress={organize.extracting?.progress ?? null}
	stopping={organize.extracting?.stopping ?? false}
	onstop={() => organize.stopExtract()}
/>
