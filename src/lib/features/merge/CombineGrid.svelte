<script lang="ts">
	// Every page of the result in one grid (section 6.4): the shared page grid, with each
	// page's label and file under it. Under the pointer, the label has the buttons that
	// turn or remove its page after it, in a bar in the label's place. Selecting, dragging
	// and the keys are the grid's (`PageGrid.svelte`).
	import { RotateCcw, RotateCw, X } from '@lucide/svelte';

	import PageGrid from '#lib/components/PageGrid.svelte';
	import type { GridPicture } from '#lib/components/pagegrid.ts';
	import { pageUrl } from '#lib/ipc/index.ts';

	import type { CombineState } from './combine.svelte.ts';
	import type { CombinePage } from './pages.ts';

	let { combine }: { combine: CombineState } = $props();

	function picture(page: CombinePage): GridPicture | null {
		const source = combine.source(page.source);
		if (!source) return null;
		const size = source.pages[page.page] ?? { width: 612, height: 792 };
		return {
			width: size.width,
			height: size.height,
			turn: page.rotation,
			url: (scale) => pageUrl(source.id, page.page, scale, source.revision),
			imageKey: (scale) => `${source.id}:${page.page}:${source.revision}:${scale}`
		};
	}

	function labelOf(page: CombinePage): string {
		const label = combine.source(page.source)?.labels?.[page.page];
		const number = String(page.page + 1);
		return label && label !== number ? `${label} (${number})` : number;
	}

	/** The side of the square each picture is fitted into (the grid's own). */
	const THUMB = 120;

	function describe(page: CombinePage, index: number): string {
		const source = combine.source(page.source);
		const turned = page.rotation ? `, turned ${page.rotation}°` : '';
		return `${index + 1} of ${combine.pages.length}: ${source?.name ?? ''}, page ${labelOf(page)}${turned}`;
	}
</script>

<PageGrid
	items={combine.pages}
	model={combine}
	{picture}
	{describe}
	label="Pages of the combined file"
	idPrefix="combine-page"
	thumb={THUMB}
>
	{#snippet tools(page: CombinePage)}
		<!-- (At most a little wider than the cell, so the bar never reaches out of the grid.) -->
		<span class="page-cell-label px-1.5" style:max-width="{THUMB - 56}px" aria-hidden="true">{labelOf(page)}</span>
		<button
			type="button"
			class="icon-button"
			tabindex="-1"
			aria-label="Turn counter-clockwise"
			data-tip="Turn counter-clockwise (Shift+R)"
			onclick={() => combine.rotate(new Set([page.key]), -90)}
		>
			<RotateCcw size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			tabindex="-1"
			aria-label="Turn clockwise"
			data-tip="Turn clockwise (R)"
			onclick={() => combine.rotate(new Set([page.key]), 90)}
		>
			<RotateCw size={16} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button"
			tabindex="-1"
			aria-label="Remove"
			data-tip="Remove (Del)"
			onclick={() => combine.remove(new Set([page.key]))}
		>
			<X size={16} aria-hidden="true" />
		</button>
	{/snippet}
	{#snippet caption(page: CombinePage)}
		{@const source = combine.source(page.source)}
		<!-- A line as tall as the bar that shows in its place under the pointer, with the same words. -->
		<span class="page-cell-label h-[36px] max-w-full shrink-0 px-1 leading-[36px] group-hover:invisible">
			{labelOf(page)}
		</span>
		<span class="flex max-w-full items-center gap-1 px-1 text-xs text-fg-muted">
			<span
				class="size-2 shrink-0 rounded-full"
				style:background="var(--lectrix-source-{source?.color ?? 1})"
				aria-hidden="true"
			></span>
			<span class="truncate">{source?.name ?? ''}</span>
		</span>
	{/snippet}
</PageGrid>
