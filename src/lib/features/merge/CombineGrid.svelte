<script lang="ts">
	// Every page of the result in one grid (section 6.4): the shared page grid, with each
	// page's file under it and buttons to turn or remove it. Selecting, dragging and the
	// keys are the grid's (`PageGrid.svelte`).
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
>
	{#snippet tools(page: CombinePage)}
		<button
			type="button"
			class="icon-button size-6"
			tabindex="-1"
			aria-label="Turn counter-clockwise"
			data-tip="Turn counter-clockwise (Shift+R)"
			onclick={() => combine.rotate(new Set([page.key]), -90)}
		>
			<RotateCcw size={14} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button size-6"
			tabindex="-1"
			aria-label="Turn clockwise"
			data-tip="Turn clockwise (R)"
			onclick={() => combine.rotate(new Set([page.key]), 90)}
		>
			<RotateCw size={14} aria-hidden="true" />
		</button>
		<button
			type="button"
			class="icon-button size-6"
			tabindex="-1"
			aria-label="Remove"
			data-tip="Remove (Del)"
			onclick={() => combine.remove(new Set([page.key]))}
		>
			<X size={14} aria-hidden="true" />
		</button>
	{/snippet}
	{#snippet caption(page: CombinePage)}
		{@const source = combine.source(page.source)}
		<span class="mt-1.5 max-w-full truncate px-1 text-xs tabular-nums">{labelOf(page)}</span>
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
