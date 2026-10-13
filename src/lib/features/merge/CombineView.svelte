<script lang="ts">
	// The Combine view (section 6.4): the files on the left with the options, in a pane
	// that resizes from its edge as the side panes do, every page of every file in one grid,
	// and the Combine button. Pages can be reordered, turned and removed; the files are only
	// read, and the result is written to a new file.
	import { FilePlus, Layers, Redo2, RotateCcw, RotateCw, Trash, Undo2, ZoomIn, ZoomOut } from '@lucide/svelte';

	import PaneResizer from '#lib/components/PaneResizer.svelte';
	import { COMBINE_LIMITS, shownWidth } from '#lib/components/panes.ts';
	import type { BookmarkMode, LabelMode } from '#lib/ipc/index.ts';
	import { app } from '#lib/stores/app.svelte.ts';

	import type { CombineState } from './combine.svelte.ts';
	import CombineFiles from './CombineFiles.svelte';
	import CombineGrid from './CombineGrid.svelte';
	import CopyProgress from './CopyProgress.svelte';

	let { combine }: { combine: CombineState } = $props();

	const BOOKMARK_OPTIONS: { value: BookmarkMode; label: string }[] = [
		{ value: 'nest', label: 'One bookmark per file, with its bookmarks inside' },
		{ value: 'flat', label: 'Each file’s bookmarks as they are' },
		{ value: 'drop', label: 'No bookmarks' }
	];
	const LABEL_OPTIONS: { value: LabelMode; label: string }[] = [
		{ value: 'keep', label: 'Each page keeps its label' },
		{ value: 'continuous', label: 'Number all pages 1, 2, 3…' },
		{ value: 'none', label: 'No page labels' }
	];

	const hasSelection = $derived(combine.selected.length > 0);

	let windowWidth = $state(typeof window === 'undefined' ? 0 : window.innerWidth);
	const paneWidth = $derived(shownWidth(app.combineWidth, COMBINE_LIMITS, windowWidth));
</script>

<svelte:window bind:innerWidth={windowWidth} />

<div class="flex min-h-0 flex-1">
	<aside
		class="relative flex shrink-0 flex-col border-r border-line bg-chrome"
		style:width="{paneWidth}px"
		aria-label="Files and options"
		data-combine-pane
	>
		<div class="flex items-center justify-between px-3 pt-3 pb-1">
			<h2 class="text-sm font-semibold">Files</h2>
			<button
				type="button"
				class="icon-button size-7"
				aria-label="Add files"
				data-tip="Add files"
				disabled={combine.running}
				onclick={() => void combine.addFiles()}
			>
				<FilePlus size={16} aria-hidden="true" />
			</button>
		</div>
		<CombineFiles {combine} />
		<div class="flex flex-col gap-4 border-t border-line p-3">
			<fieldset class="flex flex-col gap-1.5">
				<legend class="mb-1 text-sm font-semibold">Bookmarks</legend>
				{#each BOOKMARK_OPTIONS as option (option.value)}
					<label class="flex items-start gap-2 text-sm">
						<input class="mt-0.5" type="radio" bind:group={combine.bookmarks} value={option.value} />
						{option.label}
					</label>
				{/each}
			</fieldset>
			<fieldset class="flex flex-col gap-1.5">
				<legend class="mb-1 text-sm font-semibold">Page labels</legend>
				{#each LABEL_OPTIONS as option (option.value)}
					<label class="flex items-start gap-2 text-sm">
						<input class="mt-0.5" type="radio" bind:group={combine.labels} value={option.value} />
						{option.label}
					</label>
				{/each}
			</fieldset>
		</div>
		<PaneResizer side="left" bind:width={app.combineWidth} shown={paneWidth} limits={COMBINE_LIMITS} label="Files and options" />
	</aside>

	<section class="flex min-w-0 flex-1 flex-col" aria-label="Combine files">
		<div class="flex h-11 shrink-0 items-center gap-1 border-b border-line bg-chrome px-2" role="toolbar" aria-label="Page tools">
			<button type="button" class="button gap-2" disabled={combine.running} onclick={() => void combine.addFiles()}>
				<FilePlus size={16} aria-hidden="true" />
				Add files…
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Turn selected pages counter-clockwise"
				data-tip="Turn counter-clockwise (Shift+R)"
				disabled={!hasSelection}
				onclick={() => combine.rotate(combine.selectedSet, -90)}
			>
				<RotateCcw size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Turn selected pages clockwise"
				data-tip="Turn clockwise (R)"
				disabled={!hasSelection}
				onclick={() => combine.rotate(combine.selectedSet, 90)}
			>
				<RotateCw size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Remove selected pages"
				data-tip="Remove (Del)"
				disabled={!hasSelection}
				onclick={() => combine.remove(combine.selectedSet)}
			>
				<Trash size={16} aria-hidden="true" />
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Undo"
				data-tip={app.hint('Undo', 'undo')}
				disabled={!combine.canUndo}
				onclick={() => combine.undo()}
			>
				<Undo2 size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Redo"
				data-tip={app.hint('Redo', 'redo')}
				disabled={!combine.canRedo}
				onclick={() => combine.redo()}
			>
				<Redo2 size={16} aria-hidden="true" />
			</button>
			<span class="mx-1 h-5 w-px bg-line" aria-hidden="true"></span>
			<button
				type="button"
				class="icon-button"
				aria-label="Smaller pages"
				data-tip={app.hint('Smaller pages', 'zoomOut')}
				disabled={combine.zoom.smaller === null}
				onclick={() => combine.zoom.zoomStep(-1)}
			>
				<ZoomOut size={16} aria-hidden="true" />
			</button>
			<button
				type="button"
				class="icon-button"
				aria-label="Larger pages"
				data-tip={app.hint('Larger pages', 'zoomIn')}
				disabled={combine.zoom.larger === null}
				onclick={() => combine.zoom.zoomStep(1)}
			>
				<ZoomIn size={16} aria-hidden="true" />
			</button>
			<span class="ml-auto truncate pr-2 text-sm text-fg-muted" aria-live="polite">
				{#if hasSelection}{combine.selected.length.toLocaleString()} selected ·
				{/if}{combine.summary}
			</span>
		</div>

		{#if combine.pages.length === 0}
			<div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 bg-canvas p-8 text-center">
				<Layers size={40} aria-hidden="true" class="text-fg-muted" />
				<h2 class="text-lg font-semibold">
					{combine.sources.length ? 'Every page was removed' : 'Combine PDFs into one'}
				</h2>
				<p class="max-w-md text-fg-muted">
					{combine.sources.length
						? `Put pages back from the file list, ${app.hint('undo', 'undo')}, or add more files.`
						: 'Add files, or drop them here. You can reorder, turn and remove pages before combining; the files themselves are not changed.'}
				</p>
				<button type="button" class="button button-primary gap-2" onclick={() => void combine.addFiles()}>
					<FilePlus size={16} aria-hidden="true" />
					Add files…
				</button>
			</div>
		{:else}
			<CombineGrid {combine} />
		{/if}

		<div class="flex h-14 shrink-0 items-center justify-end gap-2 border-t border-line bg-chrome px-4">
			{#if combine.combinedAs}
				<span class="mr-auto truncate text-sm text-fg-muted">Combined into {combine.combinedAs}</span>
			{/if}
			<button type="button" class="button" disabled={combine.running} onclick={() => void app.closeCombine()}>
				Close
			</button>
			<button
				type="button"
				class="button button-primary"
				disabled={combine.pages.length === 0 || combine.running}
				onclick={() => void combine.combine()}
			>
				Combine…
			</button>
		</div>
	</section>
</div>

<CopyProgress
	heading="Combining files"
	progress={combine.running ? combine.progress : null}
	stopping={combine.stopping}
	onstop={() => combine.stop()}
/>
